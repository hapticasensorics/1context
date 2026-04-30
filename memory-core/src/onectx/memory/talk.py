from __future__ import annotations

import hashlib
import json
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


TALK_ENTRY_KINDS = {
    "archival-proposal",
    "archived",
    "conversation",
    "contradiction",
    "concern",
    "decided",
    "deferred",
    "fading",
    "proposal",
    "question",
    "redacted",
    "rfc",
    "synthesis",
    "verify",
    "merge",
    "split",
    "move",
    "cleanup",
    "reply",
}


@dataclass(frozen=True)
class TalkEntry:
    path: Path
    frontmatter: dict[str, str]
    body: str

    @property
    def kind(self) -> str:
        return self.frontmatter.get("kind", "")

    @property
    def ts(self) -> str:
        return self.frontmatter.get("ts", "")

    @property
    def title(self) -> str:
        return heading_for_entry(self)


def ensure_for_you_talk_folder(
    talk_folder: Path,
    *,
    date: str,
    audience: str,
    page_slug: str,
) -> None:
    talk_folder.mkdir(parents=True, exist_ok=True)
    meta_path = talk_folder / "_meta.yaml"
    if meta_path.exists():
        return
    meta = {
        "title": f"Talk · {audience.title()} · {date}",
        "slug": f"{page_slug}.{audience}.talk",
        "section": "product",
        "access": "public",
        "summary": f"Talk page input folder for Paul's For You · {audience.title()} · {date}.",
        "status": "draft",
        "md_url": "./index.md",
        "toc_enabled": True,
        "talk_enabled": False,
        "agent_view_enabled": True,
        "copy_buttons_enabled": True,
        "footer_enabled": True,
        "tags": ["for-you", "talk", audience, "paul"],
        "last_updated": date,
        "era_kind": "day",
        "era_anchor": date,
        "era_anchor_label": date,
        "talk_for": f"{page_slug}.{audience}",
        "talk_audience": audience,
        "talk_conventions": "for-you",
        "see_also": [{"url": f"./{page_slug}.{audience}.md", "text": f"For You · {audience.title()} · {date}"}],
    }
    meta_path.write_text(simple_yaml(meta), encoding="utf-8")


def render_talk_folder(talk_folder: Path, *, output_path: Path | None = None) -> dict[str, Any]:
    output_path = output_path or talk_folder / "index.md"
    entries = read_talk_entries(talk_folder)
    meta = read_meta(talk_folder / "_meta.yaml")
    title = meta.get("title") or f"Talk · {talk_folder.name}"
    lines = [
        "---",
        f"title: {title}",
        f"slug: {meta.get('slug', talk_folder.name)}",
        f"talk_for: {meta.get('talk_for', '')}",
        f"talk_audience: {meta.get('talk_audience', '')}",
        f"talk_conventions: {meta.get('talk_conventions', 'for-you')}",
        "---",
        "",
        f"# {title}",
        "",
        "> Markdown-only assembled talk input view. This is not the full wiki engine renderer.",
        "",
    ]
    for entry in entries:
        rel = entry.path.name
        lines.extend(
            [
                f"## {entry.title}",
                "",
                f"`{rel}`",
                "",
                entry.body.rstrip(),
                "",
                f"_Posted by {entry.frontmatter.get('author', 'unknown')} at {entry.ts or 'unknown'}._",
                "",
            ]
        )
        parent = entry.frontmatter.get("parent")
        if parent:
            lines.extend([f"_Parent: `{parent}`._", ""])
    text = "\n".join(lines).rstrip() + "\n"
    output_path.write_text(text, encoding="utf-8")
    return {
        "ok": True,
        "talk_folder": str(talk_folder),
        "output_path": str(output_path),
        "entry_count": len(entries),
        "bytes": len(text.encode("utf-8")),
        "sha256": sha256_text(text),
    }


def read_talk_entries(talk_folder: Path) -> list[TalkEntry]:
    entries = []
    for path in sorted(talk_folder.glob("*.md")):
        if path.name == "index.md" or path.name.startswith("_"):
            continue
        entries.append(parse_talk_entry(path))
    return sorted(entries, key=lambda item: (item.ts or item.path.name, item.path.name))


def parse_talk_entry(path: Path) -> TalkEntry:
    text = path.read_text(encoding="utf-8")
    if not text.startswith("---\n"):
        return TalkEntry(path=path, frontmatter={}, body=text.strip())
    end = text.find("\n---", 4)
    if end < 0:
        return TalkEntry(path=path, frontmatter={}, body=text.strip())
    return TalkEntry(
        path=path,
        frontmatter=parse_simple_frontmatter(text[4:end]),
        body=text[end + 4 :].strip(),
    )


def validate_talk_entry(
    path: Path,
    *,
    expected_ts: str | None = None,
    expected_kind: str | tuple[str, ...] = "conversation",
) -> dict[str, Any]:
    checks: list[str] = []
    failures: list[str] = []
    if not path.is_file():
        return {"ok": False, "checks": checks, "failures": [f"missing file: {path}"]}
    text = path.read_text(encoding="utf-8")
    entry = parse_talk_entry(path)
    frontmatter = entry.frontmatter
    body = entry.body
    if frontmatter:
        checks.append("frontmatter parses")
    else:
        failures.append("frontmatter missing or invalid")
    expected_kinds = (expected_kind,) if isinstance(expected_kind, str) else expected_kind
    if frontmatter.get("kind") in expected_kinds:
        if len(expected_kinds) == 1:
            checks.append(f"frontmatter.kind == {expected_kinds[0]}")
        else:
            checks.append(f"frontmatter.kind in {', '.join(expected_kinds)}")
    else:
        failures.append(f"frontmatter.kind must be one of {', '.join(expected_kinds)}")
    if expected_ts:
        if frontmatter.get("ts") == expected_ts:
            checks.append("frontmatter.ts matches expected timestamp")
        else:
            failures.append(f"frontmatter.ts must be {expected_ts}")
    if frontmatter.get("author"):
        checks.append("frontmatter.author exists")
    else:
        failures.append("frontmatter.author is missing")
    if frontmatter.get("kind") in TALK_ENTRY_KINDS:
        checks.append("frontmatter.kind is known talk kind")
    else:
        failures.append("frontmatter.kind is not a known talk kind")
    if body:
        checks.append("body non-empty")
    else:
        failures.append("body is empty")
    if "conversation" in expected_kinds:
        if "## " in body:
            checks.append("body has section headings")
        else:
            failures.append("body should include section headings")
        if "What I'd flag" in body or "What I’d flag" in body:
            checks.append("body has What I'd flag section")
        else:
            failures.append("body should include a What I'd flag section")
        operational_markers = ("operator", "user", "asked", "handoff", "spawned", "session", "worker")
        if any(marker in body.lower() for marker in operational_markers):
            checks.append("body discusses operational context")
        else:
            failures.append("body should discuss operational context")
        uncertainty_markers = (
            "unresolved",
            "unknown",
            "uncertain",
            "no record",
            "outside the window",
            "wider window",
            "unverified",
            "incomplete",
            "not confirmed",
            "no reply",
            "mid-flight",
            "failed",
            "missing",
            "open",
            "risk",
            "flag",
        )
        if "What I'd flag" in body or "What I’d flag" in body:
            checks.append("body marks uncertainty through What I'd flag")
        elif any(marker in body.lower() for marker in uncertainty_markers):
            checks.append("body marks uncertainty or unresolved context")
        else:
            failures.append("body should mark uncertainty or unresolved context")
    return {
        "ok": not failures,
        "checks": checks,
        "failures": failures,
        "bytes": len(text.encode("utf-8")),
        "sha256": sha256_text(text),
    }


def validate_hourly_block_result(
    manifest_path: Path,
    *,
    talk_folder: Path,
    date: str,
    expected_hours: tuple[str, ...],
) -> dict[str, Any]:
    checks: list[str] = []
    failures: list[str] = []
    written: list[str] = []
    skipped: list[str] = []
    needs_retry: list[str] = []
    if not manifest_path.is_file():
        return {"ok": False, "checks": checks, "failures": [f"missing block manifest: {manifest_path}"]}
    text = manifest_path.read_text(encoding="utf-8")
    try:
        manifest = json.loads(text)
        checks.append("block manifest parses")
    except json.JSONDecodeError as exc:
        return {"ok": False, "checks": checks, "failures": [f"block manifest JSON invalid: {exc}"]}

    hours = manifest.get("hours")
    if not isinstance(hours, list):
        failures.append("block manifest hours must be a list")
        hours = []
    by_hour = {str(item.get("hour", "")).zfill(2): item for item in hours if isinstance(item, dict)}
    for hour in expected_hours:
        item = by_hour.get(hour)
        if not item:
            failures.append(f"missing hour result: {hour}")
            continue
        status = str(item.get("status", ""))
        if status == "written":
            path = Path(str(item.get("path") or talk_folder / f"{date}T{hour}-00Z.conversation.md"))
            if not path.is_absolute():
                path = talk_folder / path
            entry_validation = validate_talk_entry(path, expected_ts=f"{date}T{hour}:00:00Z")
            if entry_validation.get("ok"):
                written.append(hour)
                checks.append(f"{hour} written entry valid")
            else:
                failures.extend(f"{hour}: {failure}" for failure in entry_validation.get("failures", []))
        elif status == "no-talk":
            skipped.append(hour)
            checks.append(f"{hour} no-talk skip recorded")
        elif status == "needs-retry":
            needs_retry.append(hour)
            checks.append(f"{hour} needs-retry recorded")
        else:
            failures.append(f"{hour} has invalid status {status!r}")
    unexpected = sorted(set(by_hour) - set(expected_hours))
    if unexpected:
        failures.append(f"unexpected hour results: {', '.join(unexpected)}")
    return {
        "ok": not failures,
        "checks": checks,
        "failures": failures,
        "bytes": len(text.encode("utf-8")),
        "sha256": sha256_text(text),
        "written_hours": written,
        "no_talk_hours": skipped,
        "needs_retry_hours": needs_retry,
        "written_count": len(written),
        "no_talk_count": len(skipped),
        "needs_retry_count": len(needs_retry),
    }


def heading_for_entry(entry: TalkEntry) -> str:
    kind = entry.kind
    if kind == "conversation":
        return conversation_heading(entry.ts)
    prefix = kind.upper() if kind else "ENTRY"
    subject = entry.path.stem.split(".", 2)[-1].replace("-", " ")
    return f"[{prefix}] {subject}"


def conversation_heading(ts: str) -> str:
    try:
        parsed = datetime.fromisoformat(ts.replace("Z", "+00:00")).astimezone(timezone.utc)
    except ValueError:
        return ts or "Conversation"
    return parsed.strftime("%Y-%m-%d · %H:%M UTC")


def read_meta(path: Path) -> dict[str, str]:
    if not path.is_file():
        return {}
    return parse_simple_frontmatter(path.read_text(encoding="utf-8"))


def parse_simple_frontmatter(text: str) -> dict[str, str]:
    result: dict[str, str] = {}
    for line in text.splitlines():
        if ":" not in line:
            continue
        key, value = line.split(":", 1)
        result[key.strip()] = value.strip().strip('"')
    return result


def simple_yaml(value: Any, *, indent: int = 0) -> str:
    prefix = " " * indent
    if isinstance(value, dict):
        lines: list[str] = []
        for key, item in value.items():
            if isinstance(item, (dict, list)):
                lines.append(f"{prefix}{key}:")
                lines.append(simple_yaml(item, indent=indent + 2))
            else:
                lines.append(f"{prefix}{key}: {yaml_scalar(item)}")
        return "\n".join(lines) + "\n"
    if isinstance(value, list):
        lines = []
        for item in value:
            if isinstance(item, dict):
                lines.append(f"{prefix}- {inline_dict(item)}")
            else:
                lines.append(f"{prefix}- {yaml_scalar(item)}")
        return "\n".join(lines) + "\n"
    return f"{prefix}{yaml_scalar(value)}\n"


def inline_dict(value: dict[str, Any]) -> str:
    return "{ " + ", ".join(f"{key}: {yaml_scalar(item)}" for key, item in value.items()) + " }"


def yaml_scalar(value: Any) -> str:
    if isinstance(value, bool):
        return "true" if value else "false"
    if isinstance(value, (int, float)):
        return str(value)
    text = str(value)
    if not text or any(char in text for char in ":#[]{}\","):
        return '"' + text.replace("\\", "\\\\").replace('"', '\\"') + '"'
    return text


def sha256_text(text: str) -> str:
    return hashlib.sha256(text.encode("utf-8")).hexdigest()
