#!/usr/bin/env python3
"""Generate the agent-discovery files from the authored .md sources.

Reads every .md in `preview/public/` (skipping .talk.md sibling files)
and produces:

  preview/public/docs-index.json   — one entry per page, frontmatter as fields
  preview/public/llms-full.txt     — concatenated markdown bodies, AX-spec
                                      separator with per-doc metadata

The order matches the curated `llms.txt` ordering rather than alphabetical:
overview / project pages first, then format references, then sample content.
Talk pages are excluded (they're audience-internal by default per the talk-
conventions article).

Run from the repo root:

    python3 tools/build-discovery-files.py

Re-run after adding or editing a `.md` file. Eventually this folds into the
vite build pipeline.
"""
from __future__ import annotations

import json
import re
import sys
from pathlib import Path

# Script lives at wiki-engine/tools/, so up two levels gets the repo
# root where preview/ + content sit.
REPO = Path(__file__).resolve().parents[2]
PUBLIC = REPO / "preview" / "public"

# Curated order — matches the categorization in llms.txt.
ORDER = [
    "1context-project",
    "wiki-engine",
    "agent-ux",
    "talk-conventions",
    "talk-page-format",
]

# Pages that are imported reference content rather than authored docs.
# AX spec partitions these out of the top-level corpus. Currently
# empty — the imported Einstein sample was removed once the project's
# own pages (1context-project, agent-ux, talk-conventions, wiki-engine)
# were substantial enough to be the demo content.
IMPORTED_SLUGS: set[str] = set()


def parse_frontmatter(text: str) -> tuple[dict, str]:
    """Shallow YAML parser for our flat frontmatter shape."""
    m = re.match(r"^---\n(.*?)\n---\n+(.*)$", text, re.DOTALL)
    if not m:
        return {}, text
    yaml, body = m.group(1), m.group(2)
    fm: dict = {}
    last_key: str | None = None
    for line in yaml.split("\n"):
        nested = re.match(r"^\s{2,}([a-z_]+):\s*(.*)$", line, re.IGNORECASE)
        if nested and last_key and isinstance(fm.get(last_key), dict):
            fm[last_key][nested.group(1)] = nested.group(2).strip()
            continue
        top = re.match(r"^([a-z][\w_]*)\s*:\s*(.*)$", line, re.IGNORECASE)
        if not top:
            continue
        key = top.group(1)
        val = top.group(2).strip()
        last_key = key
        if val == "":
            fm[key] = {}
            continue
        list_match = re.match(r"^\[(.*)\]$", val)
        if list_match:
            fm[key] = [s.strip() for s in list_match.group(1).split(",") if s.strip()]
        else:
            fm[key] = val.strip("'\"")
    return fm, body


def slug_for(path: Path) -> str:
    return path.stem


def collect_pages() -> list[tuple[str, dict, str]]:
    found = {}
    for md in PUBLIC.glob("*.md"):
        if md.name.endswith(".talk.md"):
            continue
        slug = slug_for(md)
        text = md.read_text()
        fm, body = parse_frontmatter(text)
        found[slug] = (fm, body)
    # Order: curated list first, then any extras alphabetically.
    pages = []
    for slug in ORDER:
        if slug in found:
            fm, body = found.pop(slug)
            pages.append((slug, fm, body))
    for slug in sorted(found):
        fm, body = found[slug]
        pages.append((slug, fm, body))
    return pages


def write_docs_index(pages: list[tuple[str, dict, str]]) -> None:
    entries = []
    for slug, fm, _body in pages:
        entries.append(
            {
                "slug": slug,
                "title": fm.get("title", slug),
                "summary": fm.get("summary", ""),
                "doc_id": fm.get("doc_id", slug),
                "section": fm.get("section", "project"),
                "tags": fm.get("tags", []),
                "canonical_url": fm.get("canonical_url", f"/{slug}.html"),
                "md_url": fm.get("md_url", f"/{slug}.md"),
                "llms_section_url": fm.get("llms_section_url", "/llms-full.txt"),
                "last_updated": fm.get("last_updated", ""),
                "version": fm.get("version", "1"),
                "access": fm.get("access", "public"),
                "source_type": (
                    "imported" if slug in IMPORTED_SLUGS else fm.get("source_type", "authored")
                ),
            }
        )
    out = PUBLIC / "docs-index.json"
    # ensure_ascii=False keeps em-dashes / Unicode readable in the output
    # rather than `\u2014` escape sequences.
    out.write_text(json.dumps(entries, indent=2, ensure_ascii=False) + "\n")
    print(f"wrote {out.relative_to(REPO)} ({len(entries)} entries)")


def write_llms_full(pages: list[tuple[str, dict, str]]) -> None:
    """Top-level corpus excludes imported reference content per AX spec."""
    sep = "=" * 80
    blocks = []
    for slug, fm, body in pages:
        if slug in IMPORTED_SLUGS:
            continue
        meta = "\n".join(
            [
                sep,
                "DOC_START",
                f"title: {fm.get('title', slug)}",
                f"doc_id: {fm.get('doc_id', slug)}",
                f"canonical_url: {fm.get('canonical_url', f'/{slug}.html')}",
                f"md_url: {fm.get('md_url', f'/{slug}.md')}",
                f"last_updated: {fm.get('last_updated', '')}",
                f"section: {fm.get('section', 'project')}",
                f"source_type: {fm.get('source_type', 'authored')}",
                sep,
                "",
            ]
        )
        blocks.append(meta + body.rstrip() + "\n")
    out = PUBLIC / "llms-full.txt"
    out.write_text("\n".join(blocks))
    print(f"wrote {out.relative_to(REPO)} ({len(blocks)} authored pages)")


def main() -> int:
    pages = collect_pages()
    if not pages:
        print("no pages found in preview/public/*.md", file=sys.stderr)
        return 1
    write_docs_index(pages)
    write_llms_full(pages)
    return 0


if __name__ == "__main__":
    sys.exit(main())
