from __future__ import annotations

from dataclasses import dataclass, field
from datetime import date, datetime
from pathlib import Path
from typing import Any

from .families import WikiFamily, discover_families, family_by_id, format_path
from .site import split_frontmatter


TALK_ARCHIVE_AFTER_DAYS = 90


@dataclass
class EnsureResult:
    family: WikiFamily
    created: list[Path] = field(default_factory=list)
    updated: list[Path] = field(default_factory=list)
    archived: list[Path] = field(default_factory=list)
    existing: list[Path] = field(default_factory=list)

    def to_payload(self, root: Path) -> dict[str, Any]:
        return {
            "family": self.family.to_payload(root),
            "created": [format_path(path, root) for path in self.created],
            "updated": [format_path(path, root) for path in self.updated],
            "archived": [format_path(path, root) for path in self.archived],
            "existing": [format_path(path, root) for path in self.existing],
        }


def ensure_wiki(root: Path | str, family_id: str | None = None) -> tuple[EnsureResult, ...]:
    root = Path(root).resolve()
    families = (family_by_id(root, family_id),) if family_id else discover_families(root)
    return tuple(ensure_family(root, family) for family in families)


def ensure_family(root: Path, family: WikiFamily) -> EnsureResult:
    result = EnsureResult(family=family)
    ensure_dir(family.source_dir, result)
    ensure_dir(family.talk_dir, result)
    ensure_dir(family.generated_dir, result)

    ensure_templates(family, result)
    sources = ensure_source_pages(family, result)
    for source_path in sources:
        ensure_talk_folder_for_source(family, source_path, result)

    return result


def ensure_templates(family: WikiFamily, result: EnsureResult) -> None:
    templates_dir = family.path / "templates"
    talk_templates_dir = templates_dir / "talk"
    ensure_dir(templates_dir, result)
    ensure_dir(talk_templates_dir, result)
    ensure_file(templates_dir / "page.template.md", default_page_template(family), result)
    ensure_file(talk_templates_dir / "_meta.template.yaml", default_talk_meta_template(family), result)
    ensure_file(talk_templates_dir / "_curator.template.md", default_curator_template(family), result)
    ensure_file(talk_templates_dir / "_conventions.template.md", default_conventions_template(family), result)
    ensure_file(talk_templates_dir / "entry.template.md", default_entry_template(family), result)


def ensure_source_pages(family: WikiFamily, result: EnsureResult) -> tuple[Path, ...]:
    if family.source_primary:
        if family.source_primary.exists():
            result.existing.append(family.source_primary)
        else:
            ensure_file(family.source_primary, render_page_template(family, slug=family.id, title=family.label), result)
        return (family.source_primary,)

    existing_sources = tuple(sorted(path for path in family.source_dir.glob("*.md") if path.is_file()))
    if existing_sources:
        result.existing.extend(existing_sources)
        return existing_sources

    path = family.source_dir / f"{family.id}.md"
    ensure_file(path, render_page_template(family, slug=family.id, title=family.label), result)
    return (path,)


def ensure_talk_folder_for_source(family: WikiFamily, source_path: Path, result: EnsureResult) -> None:
    page = page_info(family, source_path)
    talk_folder = talk_folder_for_source(family, source_path, page["slug"])
    ensure_dir(talk_folder, result)
    ensure_dir(talk_folder / "archive", result)
    ensure_file(talk_folder / "archive" / ".gitkeep", "", result)
    ensure_file(talk_folder / "_meta.yaml", render_talk_meta(family, page), result)
    ensure_file(talk_folder / "_curator.md", render_curator_prompt(family, page), result)
    ensure_file(talk_folder / "_conventions.md", render_conventions(family, page), result)
    ensure_archive_policy(talk_folder / "_meta.yaml", talk_folder / "_conventions.md", result)
    archive_expired_conversations(talk_folder, result)


def talk_folder_for_source(family: WikiFamily, source_path: Path, slug: str) -> Path:
    if family.talk_primary and is_same_path(source_path, family.source_primary):
        return family.talk_primary
    return family.talk_dir / f"{slug}.talk"


def page_info(family: WikiFamily, source_path: Path) -> dict[str, str]:
    frontmatter: dict[str, Any] = {}
    body = ""
    if source_path.exists():
        frontmatter, body = split_frontmatter(source_path.read_text(encoding="utf-8"))
    slug = str(frontmatter.get("slug") or source_path.stem or family.id)
    title = str(frontmatter.get("title") or heading_title(body) or family.label or slug)
    route = family.route if is_same_path(source_path, family.source_primary) else f"/{slug}"
    return {"slug": slug, "title": title, "route": route}


def heading_title(body: str) -> str:
    for line in body.splitlines():
        stripped = line.strip()
        if stripped.startswith("# "):
            return stripped[2:].strip()
    return ""


def render_page_template(family: WikiFamily, *, slug: str, title: str) -> str:
    return default_page_template(family).format(
        title=title,
        slug=slug,
        family_id=family.id,
        family_label=family.label,
        today=date.today().isoformat(),
    )


def render_talk_meta(family: WikiFamily, page: dict[str, str]) -> str:
    return default_talk_meta_template(family).format(
        title=page["title"],
        slug=page["slug"],
        route=page["route"],
        family_id=family.id,
        family_label=family.label,
        archive_after_days=TALK_ARCHIVE_AFTER_DAYS,
    )


def render_curator_prompt(family: WikiFamily, page: dict[str, str]) -> str:
    return default_curator_template(family).format(
        title=page["title"],
        slug=page["slug"],
        route=page["route"],
        family_id=family.id,
        family_label=family.label,
        archive_after_days=TALK_ARCHIVE_AFTER_DAYS,
    )


def render_conventions(family: WikiFamily, page: dict[str, str]) -> str:
    return default_conventions_template(family).format(
        title=page["title"],
        slug=page["slug"],
        route=page["route"],
        family_id=family.id,
        family_label=family.label,
        archive_after_days=TALK_ARCHIVE_AFTER_DAYS,
    )


def ensure_archive_policy(meta_path: Path, conventions_path: Path, result: EnsureResult) -> None:
    if meta_path.exists():
        raw = meta_path.read_text(encoding="utf-8")
        if "archive_after_days:" not in raw:
            append_text(meta_path, f"\narchive_after_days: {TALK_ARCHIVE_AFTER_DAYS}\n", result)
    if conventions_path.exists():
        raw = conventions_path.read_text(encoding="utf-8")
        if "Archive Policy" not in raw:
            append_text(
                conventions_path,
                (
                    "\n## Archive Policy\n\n"
                    f"Keep active talk entries in this folder for {TALK_ARCHIVE_AFTER_DAYS} days. "
                    "After that, move settled conversation files into `archive/` while preserving "
                    "the filename, frontmatter, and parent/thread references.\n"
                ),
                result,
            )


def archive_expired_conversations(talk_folder: Path, result: EnsureResult) -> None:
    archive_after_days = read_archive_after_days(talk_folder / "_meta.yaml")
    today = date.today()
    archive_dir = talk_folder / "archive"
    if not archive_dir.exists():
        ensure_dir(archive_dir, result)
    for path in sorted(talk_folder.glob("*.md")):
        if path.name.startswith("_") or not is_conversation_entry(path):
            continue
        entry_date = conversation_entry_date(path)
        if not entry_date or (today - entry_date).days < archive_after_days:
            continue
        destination = archive_dir / path.name
        if destination.exists():
            result.existing.append(destination)
            continue
        path.replace(destination)
        result.archived.append(destination)


def read_archive_after_days(meta_path: Path) -> int:
    if not meta_path.exists():
        return TALK_ARCHIVE_AFTER_DAYS
    for line in meta_path.read_text(encoding="utf-8").splitlines():
        key, sep, value = line.partition(":")
        if sep and key.strip() == "archive_after_days":
            try:
                return max(0, int(value.strip()))
            except ValueError:
                return TALK_ARCHIVE_AFTER_DAYS
    return TALK_ARCHIVE_AFTER_DAYS


def is_conversation_entry(path: Path) -> bool:
    if path.name.endswith(".conversation.md"):
        return True
    frontmatter, _body = split_frontmatter(path.read_text(encoding="utf-8"))
    return frontmatter.get("kind") == "conversation"


def conversation_entry_date(path: Path) -> date | None:
    frontmatter, _body = split_frontmatter(path.read_text(encoding="utf-8"))
    raw_ts = str(frontmatter.get("ts") or "")
    parsed = parse_entry_date(raw_ts)
    if parsed:
        return parsed
    return parse_entry_date(path.name[:10])


def parse_entry_date(raw: str) -> date | None:
    if not raw:
        return None
    try:
        if "T" in raw:
            return datetime.fromisoformat(raw.replace("Z", "+00:00")).date()
        return date.fromisoformat(raw[:10])
    except ValueError:
        return None


def ensure_dir(path: Path, result: EnsureResult) -> None:
    if path.exists():
        result.existing.append(path)
        return
    path.mkdir(parents=True, exist_ok=True)
    result.created.append(path)


def ensure_file(path: Path, content: str, result: EnsureResult) -> None:
    if path.exists():
        result.existing.append(path)
        return
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content, encoding="utf-8")
    result.created.append(path)


def append_text(path: Path, content: str, result: EnsureResult) -> None:
    path.write_text(path.read_text(encoding="utf-8").rstrip() + content, encoding="utf-8")
    result.updated.append(path)


def is_same_path(left: Path | None, right: Path | None) -> bool:
    return bool(left and right and left.resolve() == right.resolve())


def default_page_template(family: WikiFamily) -> str:
    if family.kind == "rolling_memory_surface":
        return """---
title: {title}
slug: {slug}
section: product
access: private
summary: Private rolling memory surface for {family_label}.
status: draft
family: {family_id}
last_updated: {today}
toc_enabled: true
talk_enabled: true
agent_view_enabled: true
copy_buttons_enabled: true
footer_enabled: true
---

# {title}

<!-- section: {{ slug: "biography", talk: false }} -->
## Biography

<!-- empty: weekly-rewrite slot -->

<!-- section: {{ slug: "{today}", talk: true, date: "{today}" }} -->
## Today - {today}

<!-- empty: current-day slot -->

## Open Questions

<!-- empty: agent-populated -->

## See Also

<!-- empty: agent-populated -->
"""
    return """---
title: {title}
slug: {slug}
section: product
access: private
summary: Private seed page for {family_label}.
status: draft
family: {family_id}
last_updated: {today}
toc_enabled: true
talk_enabled: true
agent_view_enabled: true
copy_buttons_enabled: true
footer_enabled: true
---

# {title}

<!-- empty: template-created page; fill with durable context before treating as authoritative. -->

## Overview

<!-- empty: agent-populated -->

## Links

<!-- empty: agent-populated -->
"""


def default_talk_meta_template(_family: WikiFamily) -> str:
    return """title: {title} - Talk
slug: {slug}.talk
talk_for: {slug}
talk_audience: private
talk_conventions: {family_id}
talk_conventions_path: ./_conventions.md
archive_after_days: {archive_after_days}
lede: Talk folder for {title}.
see_also:
  - text: Source page
    url: {route}
"""


def default_curator_template(_family: WikiFamily) -> str:
    return """# Curator Prompt - {title}

Maintain `{route}` as a private, operator-readable wiki page. Prefer small,
evidence-backed edits. Use this talk folder for proposals, decisions,
contradictions, redactions, and questions before changing durable article text.

When applying a decision, leave an explicit `decided` entry in talk. Do not
silently erase uncertainty from the discussion history.
"""


def default_conventions_template(_family: WikiFamily) -> str:
    return """# Talk Conventions - {title}

Use one timestamped markdown file per contribution.

Each top-level thread is one topic. Replies use `parent:` frontmatter pointing
to the parent filename or stem. Treat this folder as append-only active working
memory for `{route}`; the curated article is the readable state.

Agents should usually propose in talk first, then a curator/editor applies
accepted changes to the source page.

## Archive Policy

Keep active talk entries in this folder for {archive_after_days} days. After
that, move settled conversation files into `archive/` while preserving the
filename, frontmatter, and parent/thread references.
"""


def default_entry_template(_family: WikiFamily) -> str:
    return """---
kind: proposal
author: agent-id
ts: 2026-04-29T00:00:00Z
---

State the proposed change, cite the evidence, and name what page/context should
change if accepted.
"""
