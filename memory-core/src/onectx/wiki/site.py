from __future__ import annotations

import hashlib
import json
import re
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

from ..storage import LakeStore
from ..storage import list_tables as storage_list_tables
from .families import WikiFamily, discover_families, format_path
from .render import source_inputs
from .routes import RouteTable, load_route_table


SITE_MANIFEST_SCHEMA_VERSION = "wiki.site-manifest.v1"
SITE_GENERATED_DIR = "generated"
SITE_MANIFEST_FILENAME = "site-manifest.json"
CONTENT_INDEX_FILENAME = "content-index.json"
WIKI_STATS_FILENAME = "wiki-stats.json"
WIKI_STATS_SCHEMA_VERSION = "wiki.stats.v1"

FRONTMATTER_RE = re.compile(r"\A---\s*\n(.*?)\n---\s*(?:\n|\Z)", re.DOTALL)
LINK_RE = re.compile(r"\[([^\]]+)\]\(([^)]+)\)")


def build_site_manifest(root: Path | str) -> dict[str, Any]:
    root = Path(root).resolve()
    families = discover_families(root)
    table = load_route_table(root)
    pages = content_pages(root, families, table)
    stats = build_wiki_stats(root, families=families, table=table, pages=pages)
    return {
        "schema_version": SITE_MANIFEST_SCHEMA_VERSION,
        "generated_at": utc_now(),
        "wiki": {
            "path": "wiki",
            "generated_dir": f"wiki/{SITE_GENERATED_DIR}",
        },
        "families": [family.to_payload(root) for family in families],
        "routes": [target.to_payload(root) for _, target in sorted(table.routes.items())],
        "stats": stats,
        "content_policy": {
            "included": ["public generated page metadata and excerpts"],
            "excluded": ["canonical source markdown", "private/internal outputs", "talk folders", "raw markdown payloads", "render manifests"],
        },
        "pages": pages,
        "chat": {
            "default_role": "1Context librarian",
            "content_index": f"/{CONTENT_INDEX_FILENAME}",
            "api": {
                "pages": "/api/wiki/pages",
                "search": "/api/wiki/search",
                "chat": "/api/wiki/chat",
            },
        },
    }


def build_content_index(root: Path | str) -> dict[str, Any]:
    manifest = build_site_manifest(root)
    return {
        "schema_version": "wiki.content-index.v1",
        "generated_at": manifest["generated_at"],
        "content_policy": manifest["content_policy"],
        "pages": manifest["pages"],
        "stats": manifest["stats"],
    }


def load_content_index(root: Path | str) -> dict[str, Any]:
    root = Path(root).resolve()
    index_path = root / "wiki" / SITE_GENERATED_DIR / CONTENT_INDEX_FILENAME
    if index_path.exists():
        try:
            payload = json.loads(index_path.read_text(encoding="utf-8"))
            if isinstance(payload, dict):
                return payload
        except (OSError, json.JSONDecodeError):
            pass
    return build_content_index(root)


def load_wiki_stats(root: Path | str) -> dict[str, Any]:
    root = Path(root).resolve()
    stats_path = root / "wiki" / SITE_GENERATED_DIR / WIKI_STATS_FILENAME
    if stats_path.exists():
        try:
            payload = json.loads(stats_path.read_text(encoding="utf-8"))
            if isinstance(payload, dict):
                return payload
        except (OSError, json.JSONDecodeError):
            pass
    return build_wiki_stats(root)


def write_site_files(root: Path | str) -> tuple[Path, ...]:
    root = Path(root).resolve()
    generated_dir = root / "wiki" / SITE_GENERATED_DIR
    generated_dir.mkdir(parents=True, exist_ok=True)
    gitignore = generated_dir / ".gitignore"
    if not gitignore.exists():
        gitignore.write_text("*\n!.gitignore\n", encoding="utf-8")

    manifest = build_site_manifest(root)
    content_index = {
        "schema_version": "wiki.content-index.v1",
        "generated_at": manifest["generated_at"],
        "content_policy": manifest["content_policy"],
        "pages": manifest["pages"],
        "stats": manifest["stats"],
    }
    manifest_path = generated_dir / SITE_MANIFEST_FILENAME
    index_path = generated_dir / CONTENT_INDEX_FILENAME
    stats_path = generated_dir / WIKI_STATS_FILENAME
    manifest_path.write_text(stable_json(manifest) + "\n", encoding="utf-8")
    index_path.write_text(stable_json(content_index) + "\n", encoding="utf-8")
    stats_path.write_text(stable_json(manifest["stats"]) + "\n", encoding="utf-8")
    return manifest_path, index_path, stats_path


def build_wiki_stats(
    root: Path | str,
    *,
    families: tuple[WikiFamily, ...] | None = None,
    table: RouteTable | None = None,
    pages: list[dict[str, Any]] | None = None,
) -> dict[str, Any]:
    root = Path(root).resolve()
    families = families if families is not None else discover_families(root)
    table = table if table is not None else load_route_table(root)
    pages = pages if pages is not None else content_pages(root, families, table)
    generated_files = tuple(sorted(path for path in (root / "wiki" / "menu").glob("**/generated/**/*") if path.is_file()))
    talk_folders = tuple(sorted(path for family in families for path in talk_folder_paths(family)))
    talk_entries = tuple(sorted(path for folder in talk_folders for path in talk_entry_paths(folder, archived=False)))
    archived_talk_entries = tuple(sorted(path for folder in talk_folders for path in talk_entry_paths(folder, archived=True)))
    talk_words = sum(markdown_file_word_count(path) for path in talk_entries + archived_talk_entries)
    source_markdown = tuple(sorted(path for family in families for path in source_inputs(family)))
    links = [link for page in pages for link in page.get("links", []) if isinstance(link, dict)]
    link_stats = classify_links(table, links, pages=pages)
    page_sources = count_values(str(page.get("source") or "") for page in pages)
    extensions = count_values(path.suffix.lower() or "(none)" for path in generated_files)
    route_kinds = count_values(target.kind for target in table.routes.values())
    source_corpus = source_corpus_stats(root)
    family_stats = [
        family_stats_record(
            root,
            family,
            table=table,
            pages=pages,
            generated_files=generated_files,
            talk_folders=talk_folders,
            talk_entries=talk_entries,
            archived_talk_entries=archived_talk_entries,
        )
        for family in families
    ]
    return {
        "schema_version": WIKI_STATS_SCHEMA_VERSION,
        "generated_at": utc_now(),
        "totals": {
            "families": len(families),
            "routes": len(table.routes),
            "render_manifests": len(table.manifests),
            "content_pages": len(pages),
            "source_pages": len(source_markdown),
            "generated_markdown_pages": page_sources.get("generated_markdown", 0),
            "source_markdown_files": len(source_markdown),
            "generated_files": len(generated_files),
            "talk_folders": len(talk_folders),
            "talk_entries": len(talk_entries),
            "archived_talk_entries": len(archived_talk_entries),
            "talk_words": talk_words,
            "links": len(links),
            "internal_links": link_stats["internal"],
            "external_links": link_stats["external"],
            "broken_internal_links": link_stats["broken_internal"],
            "words": sum(markdown_file_word_count(path) for path in source_markdown),
            "bytes": sum(file_size(path) for path in source_markdown + generated_files),
        },
        "story": story_stats(
            pages=pages,
            families=family_stats,
            links=link_stats,
            source_corpus=source_corpus,
            talk_words=talk_words,
        ),
        "source_corpus": source_corpus,
        "by_source": page_sources,
        "routes_by_kind": route_kinds,
        "generated_files_by_extension": extensions,
        "links": link_stats,
        "families": family_stats,
    }


def family_stats_record(
    root: Path,
    family: WikiFamily,
    *,
    table: RouteTable,
    pages: list[dict[str, Any]],
    generated_files: tuple[Path, ...],
    talk_folders: tuple[Path, ...],
    talk_entries: tuple[Path, ...],
    archived_talk_entries: tuple[Path, ...],
) -> dict[str, Any]:
    family_pages = [page for page in pages if page.get("family_id") == family.id]
    family_generated = [path for path in generated_files if is_relative_to(path, family.generated_dir)]
    family_talk_folders = [path for path in talk_folders if is_relative_to(path, family.talk_dir)]
    family_talk_entries = [path for path in talk_entries if any(is_relative_to(path, folder) for folder in family_talk_folders)]
    family_archived = [path for path in archived_talk_entries if any(is_relative_to(path, folder) for folder in family_talk_folders)]
    family_routes = [target for target in table.routes.values() if target.family_id == family.id]
    return {
        "id": family.id,
        "label": family.label,
        "route": family.route,
        "menu_group": family.menu_group,
        "source_pages": len(source_inputs(family)),
        "generated_markdown_pages": sum(1 for page in family_pages if page.get("source") == "generated_markdown"),
        "routes": len(family_routes),
        "generated_files": len(family_generated),
        "generated_files_by_extension": count_values(path.suffix.lower() or "(none)" for path in family_generated),
        "talk_folders": len(family_talk_folders),
        "talk_entries": len(family_talk_entries),
        "archived_talk_entries": len(family_archived),
        "links": sum(len(page.get("links", [])) for page in family_pages),
        "words": sum(word_count(str(page.get("markdown") or "")) for page in family_pages),
        "bytes": sum(file_size(path) for path in tuple(source_inputs(family)) + tuple(family_generated)),
        "path": format_path(family.path, root),
    }


def talk_folder_paths(family: WikiFamily) -> tuple[Path, ...]:
    if family.talk_primary and family.talk_primary.is_dir():
        return (family.talk_primary,)
    if not family.talk_dir.is_dir():
        return ()
    return tuple(sorted(path for path in family.talk_dir.rglob("*.talk") if path.is_dir()))


def talk_entry_paths(folder: Path, *, archived: bool) -> tuple[Path, ...]:
    base = folder / "archive" if archived else folder
    if not base.is_dir():
        return ()
    return tuple(
        sorted(
            path
            for path in base.glob("*.md")
            if path.is_file() and not path.name.startswith("_") and (archived or path.parent == folder)
        )
    )


def classify_links(table: RouteTable, links: list[dict[str, str]], *, pages: list[dict[str, Any]]) -> dict[str, Any]:
    internal = 0
    external = 0
    anchors = 0
    broken: dict[str, int] = {}
    destinations: dict[str, int] = {}
    route_titles = route_title_map(pages)
    for link in links:
        href = str(link.get("href") or "").strip()
        if not href:
            continue
        if href.startswith("#"):
            anchors += 1
            continue
        if href.startswith(("http://", "https://", "mailto:", "tel:")):
            external += 1
            continue
        internal += 1
        route = href.split("#", 1)[0]
        if route.endswith(".md"):
            route = route[:-3]
        if route.startswith("/") and not table.resolve(route):
            broken[route] = broken.get(route, 0) + 1
        elif route.startswith("/"):
            destinations[route] = destinations.get(route, 0) + 1
    top_destinations = [
        {"route": route, "title": route_titles.get(route, route.strip("/") or "Home"), "count": count}
        for route, count in sorted(destinations.items(), key=lambda item: (-item[1], item[0]))[:10]
    ]
    return {
        "internal": internal,
        "external": external,
        "anchors": anchors,
        "broken_internal": sum(broken.values()),
        "broken_internal_routes": [{"route": route, "count": count} for route, count in sorted(broken.items())],
        "top_destinations": top_destinations,
    }


def route_title_map(pages: list[dict[str, Any]]) -> dict[str, str]:
    titles: dict[str, str] = {}
    for page in pages:
        route = str(page.get("route") or "")
        title = str(page.get("title") or route)
        if route and title and route not in titles:
            titles[route] = title
            if route.endswith(".md"):
                titles[route[:-3]] = title
    return titles


def story_stats(
    *,
    pages: list[dict[str, Any]],
    families: list[dict[str, Any]],
    links: dict[str, Any],
    source_corpus: dict[str, Any],
    talk_words: int,
) -> dict[str, Any]:
    source_pages = [page for page in pages if page.get("source") == "source"]
    source_words = sum(word_count(str(page.get("markdown") or "")) for page in source_pages)
    all_words = sum(word_count(str(page.get("markdown") or "")) for page in pages)
    reader_pages = len(source_pages) or len(pages)
    events = int(source_corpus.get("events") or 0)
    sessions = int(source_corpus.get("sessions") or 0)
    days = float(source_corpus.get("days") or 0)
    headline = f"{reader_pages} pages with {links.get('internal', 0) + links.get('external', 0)} links."
    if days and events:
        headline = f"{format_decimal(days)} days of work condensed into {reader_pages} pages."
    elif events:
        headline = f"{format_number(events)} events condensed into {reader_pages} pages."
    compression: dict[str, Any] = {
        "events_per_reader_page": round(events / reader_pages) if events and reader_pages else 0,
        "events_per_reader_word": round(events / source_words, 1) if events and source_words else 0,
    }
    raw_chars = int(source_corpus.get("raw_chars") or 0)
    readable_chars = sum(len(str(page.get("markdown") or "")) for page in source_pages)
    if raw_chars and readable_chars:
        compression["raw_chars_to_reader_text_ratio"] = round(raw_chars / readable_chars, 1)
    return {
        "headline": headline,
        "coverage": {
            "days": round(days, 1) if days else 0,
            "events": events,
            "sessions": sessions,
        },
        "reading": {
            "reader_pages": reader_pages,
            "reading_surfaces": len(pages),
            "reader_words": source_words,
            "total_indexed_words": all_words,
            "estimated_reading_minutes": round(source_words / 225) if source_words else 0,
        },
        "connections": {
            "links": int(links.get("internal", 0)) + int(links.get("external", 0)),
            "internal_links": int(links.get("internal", 0)),
            "external_links": int(links.get("external", 0)),
            "top_destinations": links.get("top_destinations", [])[:5],
        },
        "compression": compression,
        "behind_the_scenes": {
            "captured_days": round(days, 1) if days else 0,
            "agent_debate_words": talk_words,
        },
        "families": [
            {"id": family["id"], "label": family["label"], "words": family["words"], "links": family["links"]}
            for family in sorted(families, key=lambda item: (-int(item.get("words") or 0), str(item.get("label") or "")))[:5]
        ],
    }


def source_corpus_stats(root: Path) -> dict[str, Any]:
    store = LakeStore(root / "storage" / "lakestore")
    try:
        counts = store.counts()
    except Exception:
        return {}
    sessions_count = int(counts.get("sessions") or 0)
    events_count = int(counts.get("events") or 0)
    stats: dict[str, Any] = {
        "events": events_count,
        "sessions": sessions_count,
    }
    if sessions_count:
        sessions = store.rows("sessions", limit=0)
        first_ts = min((str(row.get("first_ts") or "") for row in sessions if row.get("first_ts")), default="")
        last_ts = max((str(row.get("last_ts") or "") for row in sessions if row.get("last_ts")), default="")
        by_source: dict[str, int] = {}
        for row in sessions:
            source = str(row.get("source") or "unknown")
            by_source[source] = by_source.get(source, 0) + int(row.get("event_count") or 0)
        stats["first_ts"] = first_ts
        stats["last_ts"] = last_ts
        stats["days"] = corpus_days(first_ts, last_ts)
        stats["events_by_source"] = dict(sorted(by_source.items(), key=lambda item: (-item[1], item[0])))
    raw_chars = raw_event_chars(store, events_count)
    if raw_chars:
        stats["raw_chars"] = raw_chars
    return stats


def raw_event_chars(store: LakeStore, events_count: int) -> int:
    if not events_count or events_count > 1_000_000:
        return 0
    try:
        db = store.connect()
        if "events" not in set(storage_list_tables(db)):
            return 0
        table = db.open_table("events")
        try:
            rows = table.to_arrow(columns=["char_count"]).to_pylist()
        except TypeError:
            rows = table.to_arrow().select(["char_count"]).to_pylist()
    except Exception:
        return 0
    return sum(int(row.get("char_count") or 0) for row in rows)


def corpus_days(first_ts: str, last_ts: str) -> float:
    first = parse_iso_ts(first_ts)
    last = parse_iso_ts(last_ts)
    if not first or not last or last <= first:
        return 0
    return round((last - first).total_seconds() / 86400, 1)


def parse_iso_ts(value: str) -> datetime | None:
    text = value.strip()
    if not text:
        return None
    if text.endswith("Z"):
        text = text[:-1] + "+00:00"
    try:
        parsed = datetime.fromisoformat(text)
    except ValueError:
        return None
    if parsed.tzinfo is None:
        parsed = parsed.replace(tzinfo=timezone.utc)
    return parsed.astimezone(timezone.utc)


def format_number(value: int) -> str:
    return f"{value:,}"


def format_decimal(value: float) -> str:
    if value == int(value):
        return str(int(value))
    return f"{value:.1f}"


def count_values(values: Any) -> dict[str, int]:
    counts: dict[str, int] = {}
    for value in values:
        key = str(value or "")
        if not key:
            continue
        counts[key] = counts.get(key, 0) + 1
    return dict(sorted(counts.items()))


def word_count(text: str) -> int:
    return len(re.findall(r"\b[\w'-]+\b", split_frontmatter(text)[1]))


def markdown_file_word_count(path: Path) -> int:
    try:
        return word_count(path.read_text(encoding="utf-8"))
    except OSError:
        return 0


def file_size(path: Path) -> int:
    try:
        return path.stat().st_size
    except OSError:
        return 0


def is_relative_to(path: Path, parent: Path) -> bool:
    try:
        path.resolve().relative_to(parent.resolve())
        return True
    except ValueError:
        return False


def content_pages(root: Path, families: tuple[WikiFamily, ...], table: RouteTable) -> list[dict[str, Any]]:
    pages: list[dict[str, Any]] = []
    seen: set[str] = set()
    for family in families:
        for path in sorted(family.generated_dir.glob("**/*.md")):
            if not public_generated_markdown(path):
                continue
            page = markdown_page_record(
                root,
                family,
                path,
                source="generated_markdown",
                route=route_for_generated_markdown(family, path),
            )
            add_page(pages, seen, page)

    return sorted(pages, key=lambda item: (item.get("family_id", ""), item.get("route", ""), item.get("path", "")))


def add_page(pages: list[dict[str, Any]], seen: set[str], page: dict[str, Any]) -> None:
    key = f"{page.get('source')}:{page.get('path')}"
    if key in seen:
        return
    seen.add(key)
    pages.append(page)


def markdown_page_record(
    root: Path,
    family: WikiFamily,
    path: Path,
    *,
    source: str,
    route: str | None = None,
) -> dict[str, Any]:
    raw = path.read_text(encoding="utf-8")
    frontmatter, body = split_frontmatter(raw)
    title = str(frontmatter.get("title") or heading_title(body) or path.stem)
    slug = str(frontmatter.get("slug") or path.stem)
    resolved_route = route or route_for_source(family, slug)
    text = body.strip()
    return {
        "id": stable_page_id(f"{source}:{format_path(path.resolve(), root)}"),
        "source": source,
        "family_id": family.id,
        "family_label": family.label,
        "title": title,
        "slug": slug,
        "route": resolved_route,
        "path": format_path(path.resolve(), root),
        "sha256": hashlib.sha256(raw.encode("utf-8")).hexdigest(),
        "frontmatter": frontmatter,
        "summary": str(frontmatter.get("summary") or first_sentence(text)),
        "excerpt": excerpt(text),
        "links": markdown_links(body),
    }


def public_generated_markdown(path: Path) -> bool:
    name = path.name.lower()
    return (
        path.suffix.lower() == ".md"
        and not excluded_markdown_output(path)
        and ".private." not in name
        and ".internal." not in name
        and ".talk." not in name
    )


def route_for_generated_markdown(family: WikiFamily, path: Path) -> str:
    try:
        relative = path.relative_to(family.generated_dir)
    except ValueError:
        return route_for_source(family, path.stem)
    return "/" + relative.with_suffix("").as_posix()


def split_frontmatter(raw: str) -> tuple[dict[str, Any], str]:
    match = FRONTMATTER_RE.match(raw)
    if not match:
        return {}, raw
    return parse_frontmatter(match.group(1)), raw[match.end() :]


def parse_frontmatter(raw: str) -> dict[str, Any]:
    values: dict[str, Any] = {}
    for line in raw.splitlines():
        if not line.strip() or line.lstrip().startswith("#"):
            continue
        key, sep, value = line.partition(":")
        if not sep:
            continue
        key = key.strip()
        value = value.strip()
        if value.startswith("[") and value.endswith("]"):
            values[key] = [item.strip().strip("\"'") for item in value[1:-1].split(",") if item.strip()]
        elif value.lower() in {"true", "false"}:
            values[key] = value.lower() == "true"
        else:
            values[key] = value.strip("\"'")
    return values


def route_for_source(family: WikiFamily, slug: str) -> str:
    if family.source_primary and family.source_primary.stem == slug:
        return family.route
    return f"/{slug}"


def excluded_markdown_output(path: Path) -> bool:
    name = path.name
    return (
        name.endswith(".talk.md")
        or name in {"render-manifest.json", "latest_for_family.json"}
        or name.endswith("-index.json")
    )


def heading_title(body: str) -> str:
    for line in body.splitlines():
        stripped = line.strip()
        if stripped.startswith("# "):
            return stripped[2:].strip()
    return ""


def first_sentence(text: str) -> str:
    cleaned = whitespace(text)
    if not cleaned:
        return ""
    for marker in (". ", "! ", "? "):
        index = cleaned.find(marker)
        if 0 <= index < 220:
            return cleaned[: index + 1]
    return cleaned[:220]


def excerpt(text: str, limit: int = 480) -> str:
    cleaned = whitespace(strip_markdown_comments(text))
    return cleaned[:limit] + ("..." if len(cleaned) > limit else "")


def strip_markdown_comments(text: str) -> str:
    return re.sub(r"<!--.*?-->", " ", text, flags=re.DOTALL)


def whitespace(text: str) -> str:
    return re.sub(r"\s+", " ", text).strip()


def markdown_links(body: str) -> list[dict[str, str]]:
    links: list[dict[str, str]] = []
    for label, href in LINK_RE.findall(body):
        links.append({"label": whitespace(label), "href": href.strip()})
    return links


def stable_page_id(value: str) -> str:
    return "page_" + hashlib.sha256(value.encode("utf-8")).hexdigest()[:20]


def stable_json(value: Any) -> str:
    return json.dumps(value, indent=2, sort_keys=True)


def utc_now() -> str:
    return datetime.now(timezone.utc).isoformat(timespec="seconds").replace("+00:00", "Z")
