from __future__ import annotations

import hashlib
import json
import re
from collections import defaultdict
from dataclasses import dataclass
from datetime import timedelta
from pathlib import Path
from typing import Any, Iterable

from onectx.config import MemorySystem
from onectx.storage import LakeStore, stable_id, utc_now
from onectx.storage.hour_events import HourEvent, events_between, format_ts, parse_ts


RENDERER_NAME = "render_hour_experience"
RENDERER_VERSION = "0.1.0"
FULL_TRANSCRIPT_EXPERIENCE_MODE = "braided_lived_transcript"
MESSAGES_ONLY_EXPERIENCE_MODE = "braided_lived_messages"
DEFAULT_EXPERIENCE_MODE = MESSAGES_ONLY_EXPERIENCE_MODE
SUPPORTED_EXPERIENCE_MODES = {FULL_TRANSCRIPT_EXPERIENCE_MODE, MESSAGES_ONLY_EXPERIENCE_MODE}
MAX_WEAVE_EVENTS = 80
MAX_STREAM_EVENTS_PER_STREAM = 160
MAX_EVENT_TEXT_CHARS = 3000


class HourExperienceError(RuntimeError):
    """Raised when an hour experience packet cannot be rendered."""


@dataclass(frozen=True)
class RenderedHourExperience:
    experience_id: str
    path: Path
    experience_path: Path
    agent_context_path: Path
    meta_path: Path
    experience_sha256: str
    agent_context_sha256: str
    agent_context_bytes: int
    experience_mode: str
    source_window: dict[str, str]
    event_count: int
    stream_count: int
    sources: list[dict[str, Any]]
    cache_hit: bool = False

    def to_packet(self) -> dict[str, Any]:
        return {
            "kind": "experience_packet",
            "experience_id": self.experience_id,
            "experience_mode": self.experience_mode,
            "path": str(self.path),
            "experience_path": str(self.experience_path),
            "agent_context_path": str(self.agent_context_path),
            "meta_path": str(self.meta_path),
            "experience_sha256": self.experience_sha256,
            "agent_context_sha256": self.agent_context_sha256,
            "agent_context_bytes": self.agent_context_bytes,
            "source_window": self.source_window,
            "event_count": self.event_count,
            "stream_count": self.stream_count,
            "sources": concise_sources(self.sources),
            "renderer": {
                "name": RENDERER_NAME,
                "version": RENDERER_VERSION,
            },
            "cache": {"hit": self.cache_hit},
        }


def render_hour_experience(
    system: MemorySystem,
    *,
    date: str,
    hour: str,
    source_harnesses: Iterable[str] = ("codex", "claude-code"),
    experience_mode: str = DEFAULT_EXPERIENCE_MODE,
    experience_id: str | None = None,
) -> RenderedHourExperience:
    if experience_mode not in SUPPORTED_EXPERIENCE_MODES:
        raise HourExperienceError(f"unsupported experience mode {experience_mode!r}")
    start, end = hour_window(date=date, hour=hour)
    store = LakeStore(system.storage_dir)
    store.ensure()
    events = events_between(store, start=start, end=end, sources=source_harnesses)
    return render_hour_experience_from_events(
        system,
        date=date,
        hour=hour,
        events=events,
        source_harnesses=source_harnesses,
        experience_mode=experience_mode,
        experience_id=experience_id,
    )


def render_hour_experience_from_events(
    system: MemorySystem,
    *,
    date: str,
    hour: str,
    events: list[HourEvent],
    source_harnesses: Iterable[str] = ("codex", "claude-code"),
    experience_mode: str = DEFAULT_EXPERIENCE_MODE,
    experience_id: str | None = None,
) -> RenderedHourExperience:
    if experience_mode not in SUPPORTED_EXPERIENCE_MODES:
        raise HourExperienceError(f"unsupported experience mode {experience_mode!r}")
    start, end = hour_window(date=date, hour=hour)
    resolved_id = experience_id or default_experience_id(date=date, hour=hour, mode=experience_mode)
    base = system.runtime_dir / "experiences" / safe_id(resolved_id)
    streams = group_streams(events)
    sources = source_records(streams)
    source_event_hash = source_events_sha256(events)
    store = LakeStore(system.storage_dir)
    store.ensure()
    cached = cached_hour_experience(
        store,
        artifact_id=runtime_experience_artifact_id(resolved_id, experience_mode),
        experience_id=resolved_id,
        experience_mode=experience_mode,
        source_window={"start": start, "end": end},
        source_event_hash=source_event_hash,
        event_count=len(events),
        stream_count=len(streams),
        sources=sources,
    )
    if cached:
        return cached

    base.mkdir(parents=True, exist_ok=True)
    streams_dir = base / "streams"
    streams_dir.mkdir(exist_ok=True)

    stream_files = []
    for stream_key, stream_events in sorted(streams.items()):
        filename = f"{safe_id(stream_key)}.md"
        path = streams_dir / filename
        rendered_stream_events = select_agent_facing_events(
            stream_events,
            max_events=MAX_STREAM_EVENTS_PER_STREAM,
            experience_mode=experience_mode,
        )
        path.write_text(
            render_stream(
                stream_key,
                rendered_stream_events,
                raw_event_count=len(stream_events),
            ),
            encoding="utf-8",
        )
        stream_files.append(
            {
                "stream_id": stream_key,
                "path": f"streams/{filename}",
                "event_count": len(stream_events),
                "rendered_event_count": len(rendered_stream_events),
            }
        )

    experience_text = render_experience_markdown(
        date=date,
        hour=hour,
        start=start,
        end=end,
        events=events,
        streams=streams,
        stream_files=stream_files,
        experience_mode=experience_mode,
    )
    experience_path = base / "experience.md"
    experience_path.write_text(experience_text, encoding="utf-8")
    experience_sha256 = sha256_text(experience_text)
    agent_context_text = render_agent_context_markdown(
        experience_text=experience_text,
        stream_files=stream_files,
        base=base,
        start=start,
        end=end,
        experience_mode=experience_mode,
    )
    agent_context_path = base / "agent-context.md"
    agent_context_path.write_text(agent_context_text, encoding="utf-8")
    agent_context_sha256 = sha256_text(agent_context_text)
    projection = projection_policy(experience_mode)
    agent_context_bytes = len(agent_context_text.encode("utf-8"))
    meta = {
        "kind": "experience_packet",
        "experience_mode": experience_mode,
        "experience_id": resolved_id,
        "created_at": utc_now(),
        "source_window": {"start": start, "end": end},
        "source_harnesses": list(source_harnesses),
        "renderer": {"name": RENDERER_NAME, "version": RENDERER_VERSION},
        "experience_sha256": experience_sha256,
        "agent_context_sha256": agent_context_sha256,
        "agent_context_bytes": agent_context_bytes,
        "event_count": len(events),
        "stream_count": len(streams),
        "source_events_sha256": source_event_hash,
        "projection": projection,
        "sources": sources,
        "stream_files": stream_files,
        "birth_context": {
            "path": "agent-context.md",
            "loaded_at_birth": True,
            "contains": ["experience.md", "streams/*.md"],
        },
    }
    meta_path = base / "meta.yaml"
    meta_path.write_text(simple_yaml(meta), encoding="utf-8")
    artifact = store.artifact_row(
        "runtime_experience_packet",
        artifact_id=runtime_experience_artifact_id(resolved_id, experience_mode),
        uri=f"file://{base}",
        path=str(agent_context_path),
        content_type="text/markdown",
        content_hash=agent_context_sha256,
        bytes=agent_context_bytes,
        source=RENDERER_NAME,
        state="rendered",
        text=f"{experience_mode} {start}/{end}",
        metadata={
            "experience_id": resolved_id,
            "experience_mode": experience_mode,
            "source_window": {"start": start, "end": end},
            "source_harnesses": list(source_harnesses),
            "raw_event_count": len(events),
            "stream_count": len(streams),
            "source_events_sha256": source_event_hash,
            "projection": projection,
            "experience_path": str(experience_path),
            "agent_context_path": str(agent_context_path),
            "meta_path": str(meta_path),
            "experience_sha256": experience_sha256,
            "agent_context_sha256": agent_context_sha256,
            "renderer": {"name": RENDERER_NAME, "version": RENDERER_VERSION},
        },
    )
    store.replace_rows("artifacts", "artifact_id", [artifact])
    return RenderedHourExperience(
        experience_id=resolved_id,
        path=base,
        experience_path=experience_path,
        agent_context_path=agent_context_path,
        meta_path=meta_path,
        experience_sha256=experience_sha256,
        agent_context_sha256=agent_context_sha256,
        agent_context_bytes=len(agent_context_text.encode("utf-8")),
        experience_mode=experience_mode,
        source_window={"start": start, "end": end},
        event_count=len(events),
        stream_count=len(streams),
        sources=sources,
        cache_hit=False,
    )


def hour_window(*, date: str, hour: str) -> tuple[str, str]:
    hour_int = int(str(hour).strip())
    if hour_int < 0 or hour_int > 23:
        raise HourExperienceError(f"hour must be 00-23, got {hour!r}")
    start = parse_ts(f"{date.strip()}T{hour_int:02d}:00:00Z")
    end = start + timedelta(hours=1)
    return format_ts(start), format_ts(end)


def default_experience_id(*, date: str, hour: str, mode: str) -> str:
    return safe_id(f"hour-{date}T{int(hour):02d}-{mode}")


def group_streams(events: list[HourEvent]) -> dict[str, list[HourEvent]]:
    grouped: dict[str, list[HourEvent]] = defaultdict(list)
    for event in events:
        session = event.session_id or "unknown-session"
        grouped[f"{event.source}:{session}"].append(event)
    return dict(grouped)


def source_records(streams: dict[str, list[HourEvent]]) -> list[dict[str, Any]]:
    records = []
    for stream_id, stream_events in sorted(streams.items()):
        first = stream_events[0] if stream_events else None
        records.append(
            {
                "stream_id": stream_id,
                "source": first.source if first else "",
                "session_id": first.session_id if first else "",
                "cwd": first.cwd if first else "",
                "first_ts": stream_events[0].ts if stream_events else "",
                "last_ts": stream_events[-1].ts if stream_events else "",
                "event_count": len(stream_events),
                "event_hashes": [event.hash or event.event_id for event in stream_events if event.hash or event.event_id],
            }
        )
    return records


def source_events_sha256(events: list[HourEvent]) -> str:
    refs = [
        {
            "event_id": event.event_id,
            "hash": event.hash,
            "ts": event.ts,
            "source": event.source,
            "session_id": event.session_id,
            "kind": event.kind,
        }
        for event in events
    ]
    return sha256_text(json.dumps(refs, sort_keys=True, separators=(",", ":")))


def runtime_experience_artifact_id(experience_id: str, experience_mode: str) -> str:
    return stable_id("artifact", "runtime_experience_packet", experience_id, experience_mode)


def cached_hour_experience(
    store: LakeStore,
    *,
    artifact_id: str,
    experience_id: str,
    experience_mode: str,
    source_window: dict[str, str],
    source_event_hash: str,
    event_count: int,
    stream_count: int,
    sources: list[dict[str, Any]],
) -> RenderedHourExperience | None:
    row = artifact_row_by_id(store, artifact_id)
    if not row:
        return None
    metadata = parse_json_object(row.get("metadata_json"))
    if metadata.get("experience_id") != experience_id:
        return None
    if metadata.get("experience_mode") != experience_mode:
        return None
    if metadata.get("source_window") != source_window:
        return None
    if metadata.get("source_events_sha256") != source_event_hash:
        return None
    agent_context_path = Path(str(metadata.get("agent_context_path") or row.get("path") or ""))
    experience_path = Path(str(metadata.get("experience_path") or ""))
    meta_path = Path(str(metadata.get("meta_path") or ""))
    if not (agent_context_path.is_file() and experience_path.is_file() and meta_path.is_file()):
        return None
    return RenderedHourExperience(
        experience_id=experience_id,
        path=agent_context_path.parent,
        experience_path=experience_path,
        agent_context_path=agent_context_path,
        meta_path=meta_path,
        experience_sha256=str(metadata.get("experience_sha256") or sha256_text(experience_path.read_text(encoding="utf-8"))),
        agent_context_sha256=str(metadata.get("agent_context_sha256") or row.get("content_hash") or ""),
        agent_context_bytes=int(row.get("bytes") or agent_context_path.stat().st_size),
        experience_mode=experience_mode,
        source_window=source_window,
        event_count=event_count,
        stream_count=stream_count,
        sources=sources,
        cache_hit=True,
    )


def artifact_row_by_id(store: LakeStore, artifact_id: str) -> dict[str, Any] | None:
    return store.row_by_value("artifacts", "artifact_id", artifact_id)


def parse_json_object(value: Any) -> dict[str, Any]:
    if isinstance(value, dict):
        return dict(value)
    if not isinstance(value, str) or not value:
        return {}
    try:
        parsed = json.loads(value)
    except json.JSONDecodeError:
        return {}
    return parsed if isinstance(parsed, dict) else {}


def concise_sources(sources: list[dict[str, Any]]) -> list[dict[str, Any]]:
    records = []
    for source in sources:
        hashes = source.get("event_hashes", [])
        records.append(
            {
                "stream_id": source.get("stream_id", ""),
                "source": source.get("source", ""),
                "session_id": source.get("session_id", ""),
                "cwd": source.get("cwd", ""),
                "first_ts": source.get("first_ts", ""),
                "last_ts": source.get("last_ts", ""),
                "event_count": source.get("event_count", 0),
                "event_hashes_sha256": sha256_text(json.dumps(hashes, sort_keys=True)),
            }
        )
    return records


def render_experience_markdown(
    *,
    date: str,
    hour: str,
    start: str,
    end: str,
    events: list[HourEvent],
    streams: dict[str, list[HourEvent]],
    stream_files: list[dict[str, Any]],
    experience_mode: str,
) -> str:
    lines = [
        "---",
        f"kind: {experience_mode}",
        f"experience_mode: {experience_mode}",
        f"source_window: {start}/{end}",
        f"event_count: {len(events)}",
        f"stream_count: {len(streams)}",
        "---",
        "",
        "# Inherited Multi-Stream Operational Experience",
        "",
        "You are continuing from this rendered operational history. Treat it as the lived working context you inherit for this job.",
        "",
        "The packet is braided: a stream manifest, a compressed global weave, and per-stream transcript files. Preserve stream identity when making claims.",
        "",
        "## Stream Manifest",
        "",
    ]
    if not streams:
        lines.extend(["No session events were found in this hour.", ""])
    for index, (stream_id, stream_events) in enumerate(sorted(streams.items()), start=1):
        first = stream_events[0]
        lines.extend(
            [
                f"### Stream {index}: {stream_id}",
                "",
                f"- source: {first.source}",
                f"- session_id: {first.session_id or '-'}",
                f"- cwd: {first.cwd or '-'}",
                f"- window: {stream_events[0].ts} -> {stream_events[-1].ts}",
                f"- events: {len(stream_events)}",
                "",
            ]
        )
    lines.extend(["## Global Weave", ""])
    agent_events = select_agent_facing_events(events, max_events=MAX_WEAVE_EVENTS, experience_mode=experience_mode)
    for event in agent_events:
        lines.append(f"- {event.ts} {event.source}/{short_id(event.session_id)} {event.kind}: {one_line(event.text, 180)}")
    if len(events) > len(agent_events):
        lines.append(
            f"- ... {len(events) - len(agent_events)} lower-level/raw events omitted from the agent-facing weave; "
            "source hashes remain in metadata."
        )
    lines.extend(["", "## Stream Files", ""])
    for file_item in stream_files:
        lines.append(
            f"- {file_item['stream_id']}: `{file_item['path']}` "
            f"({file_item['rendered_event_count']} rendered / {file_item['event_count']} raw events)"
        )
    lines.extend(
        [
            "",
            "## Open Questions",
            "",
            "If this inherited hour is insufficient, say so explicitly with a NEEDS wider-window request rather than guessing.",
            "",
        ]
    )
    return "\n".join(lines).rstrip() + "\n"


def render_stream(stream_id: str, events: list[HourEvent], *, raw_event_count: int | None = None) -> str:
    raw_count = raw_event_count if raw_event_count is not None else len(events)
    lines = [
        f"# Stream: {stream_id}",
        "",
        f"Rendered events: {len(events)}",
        f"Raw events in source stream: {raw_count}",
        "",
    ]
    if raw_count > len(events):
        lines.extend(
            [
                "> This is a cleaned, bounded transcript for agent cognition. "
                "Events outside this projection were omitted from the prompt path; provenance remains in meta.yaml and lakestore.",
                "",
            ]
        )
    for event in events:
        lines.extend(
            [
                f"## {event.ts} [{event.kind or event.event}]",
                "",
                f"- source: {event.source}",
                f"- session_id: {event.session_id or '-'}",
                f"- cwd: {event.cwd or '-'}",
                f"- event_id: {event.event_id or '-'}",
                f"- hash: {event.hash or '-'}",
                "",
                fenced(truncate_text(event.text or "", MAX_EVENT_TEXT_CHARS)),
                "",
            ]
        )
    return "\n".join(lines).rstrip() + "\n"


def render_agent_context_markdown(
    *,
    experience_text: str,
    stream_files: list[dict[str, Any]],
    base: Path,
    start: str,
    end: str,
    experience_mode: str,
) -> str:
    lines = [
        "---",
        "kind: birth_loaded_lived_experience",
        f"experience_mode: {experience_mode}",
        f"source_window: {start}/{end}",
        "loaded_at_birth: true",
        "---",
        "",
        "# Birth-Loaded Lived Experience",
        "",
        "This entire document is loaded into the hired agent's starting context before the current task.",
        "Treat it like restored session continuity, not like a reference file to discover later.",
        "",
        "Claude Code `--resume` and Codex `resume` continue by restoring prior conversation/session context before the new user prompt. This v0 does not forge native session files; it gives the hired agent the same cognitive shape by placing the rendered operational history directly before the job prompt.",
        "",
        "The first section is the braided control surface. The following stream transcripts are also already present in context.",
        "",
        "<lived_experience_control_surface>",
        "",
        experience_text.rstrip(),
        "",
        "</lived_experience_control_surface>",
        "",
        "# Full Stream Transcripts Loaded At Birth",
        "",
    ]
    for file_item in stream_files:
        rel_path = str(file_item["path"])
        stream_path = base / rel_path
        lines.extend(
            [
                f"<stream_transcript id=\"{xml_attr(str(file_item['stream_id']))}\" path=\"{xml_attr(rel_path)}\">",
                "",
                stream_path.read_text(encoding="utf-8").rstrip(),
                "",
                "</stream_transcript>",
                "",
            ]
        )
    return "\n".join(lines).rstrip() + "\n"


def meaningful_events(events: list[HourEvent]) -> list[HourEvent]:
    selected = []
    for event in events:
        text = event.text.strip()
        if not text:
            continue
        if event.kind in {"user", "assistant", "tool_use", "tool_result"}:
            selected.append(event)
    return selected or events


def messages_only_events(events: list[HourEvent]) -> list[HourEvent]:
    selected = []
    for event in events:
        text = event.text.strip()
        if not text:
            continue
        if event.kind in {"user", "assistant"}:
            selected.append(event)
    return selected


def projection_policy(experience_mode: str) -> dict[str, Any]:
    if experience_mode == MESSAGES_ONLY_EXPERIENCE_MODE:
        return {
            "name": MESSAGES_ONLY_EXPERIENCE_MODE,
            "source_table": "events",
            "source_truth": "lakestore.events",
            "destructive": False,
            "selection": "all user and assistant messages; no sampling",
            "included_event_kinds": ["user", "assistant"],
            "omitted_event_kinds": ["tool_use", "tool_result"],
            "retry_mode_for_tool_detail": FULL_TRANSCRIPT_EXPERIENCE_MODE,
        }
    return {
        "name": FULL_TRANSCRIPT_EXPERIENCE_MODE,
        "source_table": "events",
        "source_truth": "lakestore.events",
        "destructive": False,
        "selection": f"user, assistant, tool_use, and tool_result events capped to {MAX_STREAM_EVENTS_PER_STREAM} per stream",
        "included_event_kinds": ["user", "assistant", "tool_use", "tool_result"],
        "omitted_event_kinds": [],
        "retry_mode_for_tool_detail": FULL_TRANSCRIPT_EXPERIENCE_MODE,
    }


def select_agent_facing_events(
    events: list[HourEvent],
    *,
    max_events: int,
    experience_mode: str = DEFAULT_EXPERIENCE_MODE,
) -> list[HourEvent]:
    if experience_mode == MESSAGES_ONLY_EXPERIENCE_MODE:
        meaningful = messages_only_events(events)
        return meaningful
    else:
        meaningful = meaningful_events(events)
    if len(meaningful) <= max_events:
        return meaningful
    head_count = min(40, max_events // 4)
    tail_count = min(80, max_events // 2)
    middle_count = max_events - head_count - tail_count
    head = meaningful[:head_count]
    tail = meaningful[-tail_count:] if tail_count else []
    middle_source = meaningful[head_count : len(meaningful) - tail_count]
    middle = evenly_sample(middle_source, middle_count)
    return head + middle + tail


def evenly_sample(events: list[HourEvent], count: int) -> list[HourEvent]:
    if count <= 0 or not events:
        return []
    if len(events) <= count:
        return events
    if count == 1:
        return [events[len(events) // 2]]
    step = (len(events) - 1) / (count - 1)
    return [events[round(index * step)] for index in range(count)]


def truncate_text(text: str, max_chars: int) -> str:
    if len(text) <= max_chars:
        return text
    omitted = len(text) - max_chars
    return text[:max_chars].rstrip() + f"\n\n[... {omitted} chars omitted from this event ...]"


def safe_id(value: str) -> str:
    return re.sub(r"[^A-Za-z0-9_.-]+", "-", value.strip()).strip("-") or "experience"


def short_id(value: str) -> str:
    return value[:8] if value else "-"


def one_line(value: str, limit: int) -> str:
    text = " ".join(value.split())
    return text if len(text) <= limit else text[: limit - 1] + "..."


def xml_attr(value: str) -> str:
    return value.replace("&", "&amp;").replace('"', "&quot;").replace("<", "&lt;").replace(">", "&gt;")


def fenced(value: str) -> str:
    fence = "```"
    if "```" in value:
        fence = "````"
    return f"{fence}\n{value.rstrip()}\n{fence}"


def sha256_text(value: str) -> str:
    return hashlib.sha256(value.encode("utf-8")).hexdigest()


def simple_yaml(value: Any, indent: int = 0) -> str:
    if isinstance(value, dict):
        lines: list[str] = []
        for key, item in value.items():
            if isinstance(item, (dict, list)):
                lines.append(" " * indent + f"{key}:")
                lines.append(simple_yaml(item, indent + 2).rstrip())
            else:
                lines.append(" " * indent + f"{key}: {yaml_scalar(item)}")
        return "\n".join(lines) + "\n"
    if isinstance(value, list):
        lines = []
        for item in value:
            if isinstance(item, dict):
                lines.append(" " * indent + "-")
                lines.append(simple_yaml(item, indent + 2).rstrip())
            else:
                lines.append(" " * indent + f"- {yaml_scalar(item)}")
        return "\n".join(lines) + "\n"
    return " " * indent + yaml_scalar(value) + "\n"


def yaml_scalar(value: Any) -> str:
    if value is None:
        return "null"
    if isinstance(value, bool):
        return "true" if value else "false"
    if isinstance(value, (int, float)):
        return str(value)
    text = str(value)
    if not text:
        return '""'
    if re.fullmatch(r"[A-Za-z0-9_./:@+-]+", text):
        return text
    return json.dumps(text)
