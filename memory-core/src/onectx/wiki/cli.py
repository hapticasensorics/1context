from __future__ import annotations

import argparse
import json
from pathlib import Path

from ..config import load_system
from .evidence import record_render_evidence
from .ensure import ensure_wiki
from .families import discover_families
from .render import render_family
from .routes import load_route_table
from .server import DEFAULT_WIKI_HOST, DEFAULT_WIKI_PORT, open_wiki_url, serve_wiki, wiki_url
from .site import build_wiki_stats, load_wiki_stats, write_site_files


def cmd_wiki_list(args: argparse.Namespace) -> int:
    system = load_system(args.root, args.plugin)
    families = discover_families(system.root)
    payload = {
        "wiki": str(system.root / "wiki"),
        "families": [family.to_payload(system.root) for family in families],
    }
    if args.json:
        print(json.dumps(payload, indent=2))
        return 0
    if not families:
        print("(no wiki families)")
        return 0
    for family in families:
        print(f"{family.id:<18} {family.label:<22} group={family.menu_group:<12} route={family.route}")
        print(f"  source: {family.to_payload(system.root)['source_primary'] or family.to_payload(system.root)['source_dir']}")
        print(f"  talk:   {family.to_payload(system.root)['talk_primary'] or family.to_payload(system.root)['talk_dir']}")
        print(f"  output: {family.to_payload(system.root)['generated_dir']}")
    return 0


def cmd_wiki_ensure(args: argparse.Namespace) -> int:
    system = load_system(args.root, args.plugin)
    results = ensure_wiki(system.root, args.family_id)
    payload = {"families": [result.to_payload(system.root) for result in results]}
    if args.json:
        print(json.dumps(payload, indent=2))
        return 0
    if not results:
        print("(no wiki families to ensure)")
        return 0
    for result in results:
        print(f"ensured {result.family.id}")
        if result.created:
            print("  created:")
            for path in result.created:
                print(f"    {format_output_path(path, system.root)}")
        if result.updated:
            print("  updated:")
            for path in result.updated:
                print(f"    {format_output_path(path, system.root)}")
        if result.archived:
            print("  archived:")
            for path in result.archived:
                print(f"    {format_output_path(path, system.root)}")
        if not result.created and not result.updated and not result.archived:
            print("  no changes")
    return 0


def cmd_wiki_render(args: argparse.Namespace) -> int:
    system = load_system(args.root, args.plugin)
    families = discover_families(system.root)
    family_ids = [args.family_id] if args.family_id else [family.id for family in families]
    if not family_ids:
        print("(no wiki families to render)")
        return 0

    results = [
        render_family(
            system.root,
            family_id,
            output_dir=args.output_dir,
            include_talk=not args.skip_talk,
        )
        for family_id in family_ids
    ]
    evidence_results = [] if args.no_evidence else [record_render_evidence(system, result) for result in results]
    site_paths = write_site_files(system.root)
    if args.json:
        print(
            json.dumps(
                {
                    "renders": [result.to_payload(system.root) for result in results],
                    "evidence": [result.to_payload() for result in evidence_results],
                    "site": [format_output_path(path, system.root) for path in site_paths],
                },
                indent=2,
            )
        )
        return 0

    for index, result in enumerate(results):
        print(f"rendered {result.family.id} -> {format_output_path(result.output_dir, system.root)}")
        for invocation in result.invocations:
            print(f"  input: {format_output_path(invocation.input_path, system.root)}")
            if invocation.stdout:
                for line in invocation.stdout.splitlines():
                    print(f"    {line}")
            if invocation.stderr:
                for line in invocation.stderr.splitlines():
                    print(f"    stderr: {line}")
        print(f"  outputs: {len(result.outputs)} file(s)")
        if result.manifest_path:
            print(f"  manifest: {format_output_path(result.manifest_path, system.root)}")
        if evidence_results:
            evidence = evidence_results[index]
            print(f"  artifact: {evidence.artifact['artifact_id']}")
            print("  evidence: " + ", ".join(row["evidence_id"] for row in evidence.evidence))
            print(f"  event: {evidence.event['event_id']}")
    print("site: " + ", ".join(format_output_path(path, system.root) for path in site_paths))
    return 0


def cmd_wiki_routes(args: argparse.Namespace) -> int:
    system = load_system(args.root, args.plugin)
    table = load_route_table(system.root)
    payload = table.to_payload()
    if args.json:
        print(json.dumps(payload, indent=2))
        return 0
    if not table.routes:
        print("(no rendered wiki routes)")
        return 0
    for route, target in sorted(table.routes.items()):
        print(f"{route:<38} {format_output_path(target.path, system.root)}")
    return 0


def cmd_wiki_stats(args: argparse.Namespace) -> int:
    system = load_system(args.root, args.plugin)
    stats = load_wiki_stats(system.root)
    if "story" not in stats:
        stats = build_wiki_stats(system.root)
    if args.json:
        print(json.dumps(stats, indent=2))
        return 0
    print(render_stats_dashboard(stats))
    return 0


def cmd_wiki_serve(args: argparse.Namespace) -> int:
    system = load_system(args.root, args.plugin)
    if args.render:
        family_ids = [args.family_id] if args.family_id else [family.id for family in discover_families(system.root)]
        for family_id in family_ids:
            result = render_family(system.root, family_id, include_talk=not args.skip_talk)
            if not args.no_evidence:
                record_render_evidence(system, result)
        write_site_files(system.root)
    serve_wiki(
        system,
        host=args.host,
        port=args.port,
        allow_port_fallback=not getattr(args, "no_port_fallback", False),
    )
    return 0


def cmd_wiki_open(args: argparse.Namespace) -> int:
    system = load_system(args.root, args.plugin)
    if args.render:
        family_id = args.family_id or "for-you"
        result = render_family(system.root, family_id, include_talk=not args.skip_talk)
        if not args.no_evidence:
            record_render_evidence(system, result)
    url = wiki_url(args.path, host=args.host, port=args.port)
    if args.print:
        print(url)
        return 0
    opened = open_wiki_url(args.path, host=args.host, port=args.port)
    print(f"opened: {opened}")
    return 0


def format_output_path(path: Path, root: Path) -> str:
    try:
        return str(path.relative_to(root))
    except ValueError:
        return str(path)


def render_stats_dashboard(stats: dict[str, object]) -> str:
    width = 65
    story = as_dict(stats.get("story"))
    totals = as_dict(stats.get("totals"))
    source = as_dict(stats.get("source_corpus"))
    reading = as_dict(story.get("reading"))
    coverage = as_dict(story.get("coverage"))
    connections = as_dict(story.get("connections"))
    compression = as_dict(story.get("compression"))
    behind = as_dict(story.get("behind_the_scenes"))

    reader_pages = int_value(reading.get("reader_pages")) or int_value(totals.get("source_pages"))
    reader_words = int_value(reading.get("reader_words"))
    if not reader_words:
        reader_words = int_value(totals.get("words"))
    read_minutes = int_value(reading.get("estimated_reading_minutes")) or round(reader_words / 225) if reader_words else 0
    links = int_value(connections.get("links")) or int_value(totals.get("links"))
    concepts = concept_count(stats)
    days = float_value(coverage.get("days")) or float_value(source.get("days"))
    events = int_value(coverage.get("events")) or int_value(source.get("events"))
    sessions = int_value(coverage.get("sessions")) or int_value(source.get("sessions"))
    raw_ratio = float_value(compression.get("raw_chars_to_reader_text_ratio"))
    agent_debate_words = int_value(behind.get("agent_debate_words")) or int_value(totals.get("talk_words"))
    top = top_destinations(stats)

    lines: list[str] = []
    lines.append(top_border(width))
    lines.append(row("What your work is orbiting", width))
    lines.append(mid_border(width))
    lines.extend(center_orbit_lines(width, top))
    lines.append(mid_border(width))
    lines.extend(card_rows(width, [
        (format_number(reader_pages), "readable pages"),
        (format_number(links), "links between pages"),
        (format_number(concepts), "named subjects"),
    ]))
    lines.append(mid_border(width))
    comparison = known_work_comparison(reader_words)
    lines.extend(card_rows(width, [
        (f"{compact_number(reader_words)} words", comparison),
        (reading_time(read_minutes), "focused read"),
        (compression_label(raw_ratio, events, reader_words), "raw work -> wiki"),
    ]))
    lines.append(mid_border(width))
    lines.append(row("Behind the scenes", width))
    if days:
        lines.append(row(f"{format_decimal(days)} days captured", width))
    elif sessions:
        lines.append(row(f"{format_number(sessions)} sessions captured", width))
    else:
        lines.append(row("Source activity captured as the wiki grows", width))
    if agent_debate_words:
        lines.append(row(f"{compact_number(agent_debate_words)} words of agent debate beneath the readable wiki", width))
    else:
        lines.append(row("Agent debate and talk-page reasoning preserved beneath the readable wiki", width))
    broken = int_value(totals.get("broken_internal_links"))
    if broken:
        lines.append(row(f"{format_number(broken)} links need repair", width))
    else:
        lines.append(row("All internal links working", width))
    lines.append(bottom_border(width))
    return "\n".join(lines)


def card_rows(width: int, cards: list[tuple[str, str]]) -> list[str]:
    inner = width - 2
    gap = 1
    cell = (inner - (gap * 2)) // 3
    rows = []
    for values in ((card[0] for card in cards), (card[1] for card in cards)):
        parts = [fit_text(value, cell).center(cell) for value in values]
        rows.append("|" + (" " * gap).join(parts).ljust(inner) + "|")
    return rows


def center_orbit_lines(width: int, top: list[dict[str, object]]) -> list[str]:
    labels = [str(item.get("title") or item.get("route") or "") for item in top[:6]]
    while len(labels) < 6:
        labels.append("")
    root = labels[0] or "1Context"
    left = labels[1] or "Guardian"
    middle = labels[2] or "wiki-engine"
    right = labels[3] or "Agent UX"
    low_left = labels[4] or "BookStack"
    low_right = labels[5] or "Haptica"
    return [
        centered(root, width),
        centered("/       |       \\", width),
        centered(f"{left}  {middle}  {right}", width),
        centered("|          |          |", width),
        centered(f"{low_left}     {low_right}", width),
    ]


def top_destinations(stats: dict[str, object]) -> list[dict[str, object]]:
    links = as_dict(stats.get("links"))
    raw = links.get("top_destinations")
    return raw if isinstance(raw, list) else []


def concept_count(stats: dict[str, object]) -> int:
    families = stats.get("families")
    if not isinstance(families, list):
        return 0
    for family in families:
        if not isinstance(family, dict):
            continue
        if family.get("id") == "topics":
            pages = int_value(family.get("source_pages")) + int_value(family.get("generated_markdown_pages"))
            return max(0, pages - 1)
    return 0


def known_work_comparison(words: int) -> str:
    if words <= 0:
        return "readable wiki"
    comparisons = [
        (1000, "short note"),
        (3000, "feature article"),
        (7500, "long essay"),
        (30000, "Of Mice and Men-sized"),
        (47000, "Gatsby-sized"),
        (59000, "Fahrenheit 451-sized"),
        (89000, "typical novel-sized"),
        (181000, "Moby-Dick-sized"),
        (587000, "War and Peace-sized"),
    ]
    for limit, label in comparisons:
        if words <= limit:
            return label
    return "library-sized"


def reading_time(minutes: int) -> str:
    if minutes <= 0:
        return "-"
    if minutes < 60:
        return f"~{minutes} min"
    hours = minutes / 60
    return f"~{hours:.1f} hr" if hours < 10 else f"~{round(hours)} hr"


def compression_label(ratio: float, events: int, words: int) -> str:
    if ratio and words >= 5000:
        return f"~{ratio:g}x compression"
    if events and words >= 5000:
        return f"~{round(events / max(words, 1), 1):g} events/word"
    return "compression growing"


def top_border(width: int) -> str:
    return "+" + "-" * (width - 2) + "+"


def mid_border(width: int) -> str:
    return "+" + "-" * (width - 2) + "+"


def bottom_border(width: int) -> str:
    return "+" + "-" * (width - 2) + "+"


def row(text: str, width: int) -> str:
    return "|" + fit_text(text, width - 4).ljust(width - 4).center(width - 2) + "|"


def centered(text: str, width: int) -> str:
    return "|" + fit_text(text, width - 4).center(width - 2) + "|"


def fit_text(text: object, width: int) -> str:
    value = str(text)
    if len(value) <= width:
        return value
    return value[: max(0, width - 3)] + "..."


def as_dict(value: object) -> dict[str, object]:
    return value if isinstance(value, dict) else {}


def int_value(value: object) -> int:
    try:
        return int(value or 0)
    except (TypeError, ValueError):
        return 0


def float_value(value: object) -> float:
    try:
        return float(value or 0)
    except (TypeError, ValueError):
        return 0.0


def format_number(value: int) -> str:
    return f"{value:,}"


def compact_number(value: int) -> str:
    if value >= 1_000_000:
        return f"{value / 1_000_000:.1f}M"
    if value >= 10_000:
        return f"{round(value / 1000)}K"
    if value >= 1_000:
        return f"{value / 1000:.1f}K"
    return format_number(value)


def format_decimal(value: float) -> str:
    if value == int(value):
        return str(int(value))
    return f"{value:.1f}"
