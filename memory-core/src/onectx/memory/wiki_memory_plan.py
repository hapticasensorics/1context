from __future__ import annotations

import json
import os
import re
from collections import defaultdict
from dataclasses import dataclass
from datetime import datetime, timedelta, timezone
from pathlib import Path
from typing import Any, Iterable

from onectx.io_utils import atomic_write_json
from onectx.storage import stable_id


DEFAULT_USABLE_CONTEXT_TOKENS = 258_400
DEFAULT_CONTEXT_FRACTION = 0.62
DEFAULT_MAX_PACKETS_PER_RUN = 20
DEFAULT_RECENT_PRIORITY_DAYS = 3


@dataclass(frozen=True)
class WikiMemoryPacket:
    packet_id: str
    packet_kind: str
    date: str
    hour: str
    shard_index: int
    shard_count: int
    session_ids: tuple[str, ...]
    event_count: int
    char_count: int
    estimated_tokens: int
    content_sha256: str
    source_packet_path: Path
    cache_path: Path
    cached: bool = False

    def to_payload(self, *, root: Path | None = None) -> dict[str, Any]:
        return {
            "packet_id": self.packet_id,
            "packet_kind": self.packet_kind,
            "date": self.date,
            "hour": self.hour,
            "shard_index": self.shard_index,
            "shard_count": self.shard_count,
            "session_ids": list(self.session_ids),
            "event_count": self.event_count,
            "char_count": self.char_count,
            "estimated_tokens": self.estimated_tokens,
            "content_sha256": self.content_sha256,
            "source_packet_path": format_path(self.source_packet_path, root),
            "cache_path": format_path(self.cache_path, root),
            "cached": self.cached,
        }


@dataclass(frozen=True)
class WikiMemoryPlan:
    run_id: str
    mode: str
    status: str
    path: Path
    summary_packet_path: Path
    raw_ingest_cursor: str
    wiki_memory_cursor: str
    usable_context_tokens: int
    context_fraction: float
    target_packet_tokens: int
    max_packets_per_run: int
    active_day_count: int
    active_hour_count: int
    total_packet_count: int
    selected_packet_count: int
    cached_packet_count: int
    selection_strategy: str
    recent_priority_day_count: int
    packets: tuple[WikiMemoryPacket, ...]
    selected_packets: tuple[WikiMemoryPacket, ...]
    signal_flags: tuple[str, ...]

    def to_payload(self, *, root: Path | None = None) -> dict[str, Any]:
        return {
            "schema_version": 1,
            "operation": "memory.wiki.plan_backfill",
            "run_id": self.run_id,
            "mode": self.mode,
            "status": self.status,
            "path": format_path(self.path, root),
            "summary_packet_path": format_path(self.summary_packet_path, root),
            "raw_ingest_cursor": self.raw_ingest_cursor,
            "wiki_memory_cursor": self.wiki_memory_cursor,
            "usable_context_tokens": self.usable_context_tokens,
            "context_fraction": self.context_fraction,
            "target_packet_tokens": self.target_packet_tokens,
            "max_packets_per_run": self.max_packets_per_run,
            "active_day_count": self.active_day_count,
            "active_hour_count": self.active_hour_count,
            "total_packet_count": self.total_packet_count,
            "selected_packet_count": self.selected_packet_count,
            "cached_packet_count": self.cached_packet_count,
            "selection_strategy": self.selection_strategy,
            "recent_priority_day_count": self.recent_priority_day_count,
            "signal_flags": list(self.signal_flags),
            "packets": [packet.to_payload(root=root) for packet in self.packets],
            "selected_packets": [packet.to_payload(root=root) for packet in self.selected_packets],
        }


def build_wiki_memory_plan(
    *,
    run_id: str,
    update_dir: Path,
    events: Iterable[dict[str, Any]],
    cursor_name: str,
    window_days: int,
    cache_root: Path,
    usable_context_tokens: int | None = None,
    context_fraction: float | None = None,
    model_policy: dict[str, Any] | None = None,
    max_packets_per_run: int | None = None,
) -> WikiMemoryPlan:
    resolved_events = sorted((event for event in events if event.get("ts")), key=lambda row: str(row.get("ts") or ""))
    policy = model_policy if isinstance(model_policy, dict) else {}
    usable_tokens = max(
        1,
        usable_context_tokens
        or env_int("ONECONTEXT_WIKI_SCRIBE_USABLE_CONTEXT_TOKENS", policy_int(policy, "usable_context_tokens", DEFAULT_USABLE_CONTEXT_TOKENS)),
    )
    fraction = context_fraction or env_float(
        "ONECONTEXT_WIKI_SCRIBE_CONTEXT_FRACTION",
        policy_float(policy, "context_fraction", DEFAULT_CONTEXT_FRACTION),
    )
    fraction = min(0.9, max(0.25, fraction))
    target_tokens = max(2_000, int(usable_tokens * fraction))
    max_packets = max(1, max_packets_per_run or env_int("ONECONTEXT_WIKI_MAX_SCRIBE_PACKETS_PER_RUN", DEFAULT_MAX_PACKETS_PER_RUN))
    packet_dir = update_dir / "source-packets"
    cache_dir = cache_root / "wiki-memory-cache" / "scribe-packets"
    packet_dir.mkdir(parents=True, exist_ok=True)
    cache_dir.mkdir(parents=True, exist_ok=True)

    by_hour: dict[tuple[str, str], list[dict[str, Any]]] = defaultdict(list)
    for event in resolved_events:
        key = event_day_hour(event)
        if key[0]:
            by_hour[key].append(event)

    packets: list[WikiMemoryPacket] = []
    for (date, hour), hour_events in sorted(by_hour.items()):
        packets.extend(
            packetize_hour(
                packet_dir=packet_dir,
                cache_dir=cache_dir,
                date=date,
                hour=hour,
                events=hour_events,
                target_tokens=target_tokens,
            )
        )

    mode = "catch_up_backfill" if window_days > 2 else "daily_maintenance"
    selection_strategy = "recent_three_days_first_then_oldest_to_newest" if mode == "catch_up_backfill" else "oldest_to_newest"
    selected = select_packets_for_run(
        packets,
        mode=mode,
        max_packets=max_packets,
        recent_priority_days=DEFAULT_RECENT_PRIORITY_DAYS,
    )
    active_days = {packet.date for packet in packets}
    active_hours = {(packet.date, packet.hour) for packet in packets}
    summary_packet_path = update_dir / "wiki-memory-plan-summary.md"
    signal_flags = tuple(signal_flags_for_events(resolved_events))
    plan = WikiMemoryPlan(
        run_id=run_id,
        mode=mode,
        status="planned",
        path=update_dir / "wiki-memory-plan.json",
        summary_packet_path=summary_packet_path,
        raw_ingest_cursor=cursor_name,
        wiki_memory_cursor=f"wiki_memory:{cursor_name or 'default'}",
        usable_context_tokens=usable_tokens,
        context_fraction=fraction,
        target_packet_tokens=target_tokens,
        max_packets_per_run=max_packets,
        active_day_count=len(active_days),
        active_hour_count=len(active_hours),
        total_packet_count=len(packets),
        selected_packet_count=len(selected),
        cached_packet_count=sum(1 for packet in packets if packet.cached),
        selection_strategy=selection_strategy,
        recent_priority_day_count=DEFAULT_RECENT_PRIORITY_DAYS,
        packets=tuple(packets),
        selected_packets=selected,
        signal_flags=signal_flags,
    )
    write_plan_summary(plan, resolved_events)
    atomic_write_json(plan.path, plan.to_payload())
    return plan


def select_packets_for_run(
    packets: list[WikiMemoryPacket],
    *,
    mode: str,
    max_packets: int,
    recent_priority_days: int,
) -> tuple[WikiMemoryPacket, ...]:
    uncached = [packet for packet in packets if not packet.cached]
    if mode != "catch_up_backfill" or recent_priority_days <= 0:
        return tuple(uncached[:max_packets])

    recent_cutoff = recent_priority_cutoff(packets, recent_priority_days)
    if recent_cutoff is None:
        return tuple(uncached[:max_packets])

    recent = [packet for packet in uncached if packet.date >= recent_cutoff]
    older = [packet for packet in uncached if packet.date < recent_cutoff]
    return tuple((recent + older)[:max_packets])


def recent_priority_cutoff(packets: list[WikiMemoryPacket], recent_priority_days: int) -> str | None:
    parsed_dates = sorted({datetime.strptime(packet.date, "%Y-%m-%d").date() for packet in packets})
    if not parsed_dates:
        return None
    recent_count = max(1, recent_priority_days)
    return (parsed_dates[-1] - timedelta(days=recent_count - 1)).isoformat()


def packetize_hour(
    *,
    packet_dir: Path,
    cache_dir: Path,
    date: str,
    hour: str,
    events: list[dict[str, Any]],
    target_tokens: int,
) -> list[WikiMemoryPacket]:
    if estimate_tokens(events) <= target_tokens:
        return [write_packet(packet_dir, cache_dir, date, hour, 1, 1, "hour", events, target_tokens)]

    by_session: dict[str, list[dict[str, Any]]] = defaultdict(list)
    for event in events:
        by_session[str(event.get("session_id") or "unknown")].append(event)

    packets: list[list[dict[str, Any]]] = []
    for _, rows in sorted(by_session.items(), key=lambda item: str(item[1][0].get("ts") or "")):
        if estimate_tokens(rows) <= target_tokens:
            packets.append(rows)
            continue
        packets.extend(split_event_rows(rows, target_tokens=target_tokens))

    packet_count = max(1, len(packets))
    return [
        write_packet(packet_dir, cache_dir, date, hour, index, packet_count, "hour_part", rows, target_tokens)
        for index, rows in enumerate(packets, start=1)
    ]


def split_event_rows(rows: list[dict[str, Any]], *, target_tokens: int) -> list[list[dict[str, Any]]]:
    chunks: list[list[dict[str, Any]]] = []
    current: list[dict[str, Any]] = []
    current_tokens = 0
    for row in rows:
        row_tokens = estimate_tokens([row])
        if current and current_tokens + row_tokens > target_tokens:
            chunks.append(current)
            current = []
            current_tokens = 0
        current.append(row)
        current_tokens += row_tokens
    if current:
        chunks.append(current)
    return chunks


def write_packet(
    packet_dir: Path,
    cache_dir: Path,
    date: str,
    hour: str,
    shard_index: int,
    shard_count: int,
    packet_kind: str,
    events: list[dict[str, Any]],
    target_tokens: int,
) -> WikiMemoryPacket:
    content = render_scribe_packet(
        date=date,
        hour=hour,
        shard_index=shard_index,
        shard_count=shard_count,
        packet_kind=packet_kind,
        events=events,
        target_tokens=target_tokens,
    )
    content_hash = stable_id("wiki_scribe_packet", content)
    packet_id = stable_id("wiki_scribe_packet_id", date, hour, shard_index, shard_count, content_hash)[:16]
    packet_path = packet_dir / f"{date}T{hour}-packet-{shard_index:02d}-of-{shard_count:02d}-{packet_id}.md"
    cache_path = cache_dir / f"{content_hash}.json"
    packet_path.write_text(content, encoding="utf-8")
    session_ids = tuple(sorted({str(event.get("session_id") or "") for event in events if str(event.get("session_id") or "")}))
    char_count = sum(int(event.get("char_count") or len(str(event.get("text") or ""))) for event in events)
    return WikiMemoryPacket(
        packet_id=packet_id,
        packet_kind=packet_kind,
        date=date,
        hour=hour,
        shard_index=shard_index,
        shard_count=shard_count,
        session_ids=session_ids,
        event_count=len(events),
        char_count=char_count,
        estimated_tokens=estimate_tokens(events),
        content_sha256=content_hash,
        source_packet_path=packet_path,
        cache_path=cache_path,
        cached=cache_path.is_file(),
    )


def render_scribe_packet(
    *,
    date: str,
    hour: str,
    shard_index: int,
    shard_count: int,
    packet_kind: str,
    events: list[dict[str, Any]],
    target_tokens: int,
) -> str:
    lines = [
        "# Bounded Scribe Source Packet",
        "",
        "This raw Perception/Timescale transcript packet is for an hourly scribe only.",
        "Downstream editors, curators, and librarians should read scribe artifacts rather than this raw packet.",
        "",
        "## Packet",
        "",
        f"- Date: `{date}`",
        f"- Hour: `{hour}`",
        f"- Kind: `{packet_kind}`",
        f"- Packet: `{shard_index}` of `{shard_count}`",
        f"- Event count: `{len(events)}`",
        f"- Estimated tokens: `{estimate_tokens(events)}`",
        f"- Target packet tokens: `{target_tokens}`",
        "",
        "## Deterministic Extraction",
        "",
        *deterministic_extraction_lines(events),
        "",
        "## Transcript Events",
        "",
    ]
    for event in events:
        lines.append(
            "- "
            f"`{event.get('ts')}` "
            f"`{event.get('source')}/{event.get('kind')}` "
            f"`{event.get('session_id')}` "
            f"`{event.get('cwd') or event.get('project_key') or ''}`: "
            f"{compact_text(event.get('text'))}"
        )
    return "\n".join(lines).rstrip() + "\n"


def deterministic_extraction_lines(events: list[dict[str, Any]]) -> list[str]:
    projects = sorted({str(event.get("project_key") or event.get("cwd") or "").strip() for event in events if str(event.get("project_key") or event.get("cwd") or "").strip()})
    user_asks = [compact_text(event.get("text"), limit=180) for event in events if str(event.get("kind") or "") == "user"][-6:]
    assistant_summaries = [compact_text(event.get("text"), limit=180) for event in events if str(event.get("kind") or "") == "assistant"][-6:]
    commands = extract_matches(events, r"`([^`]*(?:uv|cargo|swift|git|pytest|npm|pnpm|node|onecontext|codex)[^`]*)`", limit=10)
    paths = extract_matches(events, r"(/Users/[^\s)\]]+|[A-Za-z0-9_./-]+\.(?:py|swift|rs|md|toml|json|mjs|ts|tsx))", limit=12)
    lines = [
        "- Projects/CWD: " + (", ".join(f"`{item}`" for item in projects[:8]) if projects else "`unknown`"),
        "- Commands: " + (", ".join(f"`{item}`" for item in commands) if commands else "`none extracted`"),
        "- Paths: " + (", ".join(f"`{item}`" for item in paths) if paths else "`none extracted`"),
        "- Recent user asks:",
    ]
    lines.extend([f"  - {item}" for item in user_asks] or ["  - none extracted"])
    lines.append("- Recent assistant summaries:")
    lines.extend([f"  - {item}" for item in assistant_summaries] or ["  - none extracted"])
    return lines


def signal_flags_for_events(events: list[dict[str, Any]]) -> list[str]:
    text = "\n".join(str(event.get("text") or "") for event in events).casefold()
    flags: list[str] = []
    if any(token in text for token in ("preference", "working style", "i like", "i want", "remember", "your context")):
        flags.append("personal_context")
    if any(token in text for token in ("branch", "commit", "merge", "project", "build", "release", "repo")):
        flags.append("project_history")
    if any(token in text for token in ("concept", "topic", "citation", "source", "wiki", "librarian")):
        flags.append("topics_or_sources")
    if any(token in text for token in ("contradiction", "wrong", "failed", "error", "timeout", "broken")):
        flags.append("contradictions_or_failures")
    return flags or ["daily_activity"]


def write_plan_summary(plan: WikiMemoryPlan, events: list[dict[str, Any]]) -> None:
    lines = [
        "# Wiki Memory Planner Summary",
        "",
        "This deterministic planner creates bounded source packets before any agent sees raw transcript text.",
        "",
        "## Cursors",
        "",
        f"- Raw ingest cursor: `{plan.raw_ingest_cursor}`",
        f"- Wiki memory cursor: `{plan.wiki_memory_cursor}`",
        "",
        "## Packet Budget",
        "",
        f"- Usable context tokens: `{plan.usable_context_tokens}`",
        f"- Target fraction: `{plan.context_fraction}`",
        f"- Target packet tokens: `{plan.target_packet_tokens}`",
        f"- Max scribe packets this run: `{plan.max_packets_per_run}`",
        f"- Selection strategy: `{plan.selection_strategy}`",
        f"- Recent priority days: `{plan.recent_priority_day_count}`",
        "",
        "## Active Window",
        "",
        f"- Events: `{len(events)}`",
        f"- Active days: `{plan.active_day_count}`",
        f"- Active hours: `{plan.active_hour_count}`",
        f"- Total packets: `{plan.total_packet_count}`",
        f"- Selected uncached packets: `{plan.selected_packet_count}`",
        f"- Cached packets: `{plan.cached_packet_count}`",
        "",
        "## Selected Packets",
        "",
    ]
    for packet in plan.selected_packets:
        lines.append(
            "- "
            f"`{packet.date}T{packet.hour}` `{packet.packet_kind}` "
            f"packet `{packet.shard_index}/{packet.shard_count}` "
            f"events `{packet.event_count}` est_tokens `{packet.estimated_tokens}` "
            f"packet `{packet.source_packet_path}`"
        )
    if not plan.selected_packets:
        lines.append("- No uncached packets selected.")
    plan.summary_packet_path.write_text("\n".join(lines).rstrip() + "\n", encoding="utf-8")


def estimate_tokens(events: Iterable[dict[str, Any]]) -> int:
    chars = 0
    count = 0
    for event in events:
        count += 1
        chars += int(event.get("char_count") or len(str(event.get("text") or "")))
    return max(1, chars // 4 + count * 16)


def event_day_hour(event: dict[str, Any]) -> tuple[str, str]:
    parsed = parse_time(str(event.get("ts") or ""))
    if parsed is None:
        return "", ""
    return parsed.strftime("%Y-%m-%d"), parsed.strftime("%H")


def parse_time(value: str) -> datetime | None:
    try:
        return datetime.fromisoformat(value.replace("Z", "+00:00")).astimezone(timezone.utc)
    except ValueError:
        return None


def extract_matches(events: list[dict[str, Any]], pattern: str, *, limit: int) -> list[str]:
    seen: set[str] = set()
    result: list[str] = []
    for event in events:
        for match in re.findall(pattern, str(event.get("text") or "")):
            value = match if isinstance(match, str) else next((item for item in match if item), "")
            value = value.strip().rstrip(".,;:")
            if not value or value in seen:
                continue
            seen.add(value)
            result.append(value)
            if len(result) >= limit:
                return result
    return result


def compact_text(value: Any, *, limit: int = 420) -> str:
    text = " ".join(str(value or "").split())
    if len(text) <= limit:
        return text
    return text[: limit - 1].rstrip() + "..."


def env_int(name: str, default: int) -> int:
    try:
        return int(os.environ.get(name, "") or default)
    except ValueError:
        return default


def env_float(name: str, default: float) -> float:
    try:
        return float(os.environ.get(name, "") or default)
    except ValueError:
        return default


def policy_int(policy: dict[str, Any], key: str, default: int) -> int:
    try:
        return int(policy.get(key) or default)
    except (TypeError, ValueError):
        return default


def policy_float(policy: dict[str, Any], key: str, default: float) -> float:
    try:
        return float(policy.get(key) or default)
    except (TypeError, ValueError):
        return default


def format_path(path: Path | None, root: Path | None) -> str | None:
    if path is None:
        return None
    if root:
        try:
            return str(path.relative_to(root))
        except ValueError:
            pass
    return str(path)
