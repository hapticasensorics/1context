from __future__ import annotations

import json
from pathlib import Path

from onectx.wiki.cli import render_stats_dashboard
from onectx.wiki.routes import load_route_table
from onectx.wiki.site import WIKI_STATS_FILENAME, build_wiki_stats, load_wiki_stats, write_site_files


def test_wiki_stats_count_sources_talk_and_links(tmp_path: Path) -> None:
    make_family(tmp_path)
    source = tmp_path / "wiki" / "menu" / "10-group" / "10-example" / "source" / "example.md"
    source.write_text(
        "---\ntitle: Example\nslug: example\n---\n\n"
        "# Example\n\n"
        "Words for the example page with [self](/example), [missing](/missing), "
        "and [external](https://example.com).\n",
        encoding="utf-8",
    )
    talk = tmp_path / "wiki" / "menu" / "10-group" / "10-example" / "talk" / "example.talk"
    talk.mkdir(parents=True)
    (talk / "2026-04-29T00-00Z.conversation.md").write_text("hello\n", encoding="utf-8")
    archive = talk / "archive"
    archive.mkdir()
    (archive / "2000-01-01T00-00Z.conversation.md").write_text("old\n", encoding="utf-8")
    generated = tmp_path / "wiki" / "menu" / "10-group" / "10-example" / "generated"
    generated.mkdir(exist_ok=True)
    (generated / "example.html").write_text("<h1>Example</h1>", encoding="utf-8")
    (generated / "example.md").write_text(source.read_text(encoding="utf-8"), encoding="utf-8")
    (generated / "render-manifest.json").write_text(
        json.dumps(
            {
                "family": {"id": "example"},
                "routes": [{"route": "/example", "output_path": "example.html", "kind": "html"}],
                "outputs": [{"path": "example.html"}, {"path": "example.md"}],
            }
        ),
        encoding="utf-8",
    )

    stats = build_wiki_stats(tmp_path)

    assert stats["schema_version"] == "wiki.stats.v1"
    assert stats["totals"]["families"] == 1
    assert stats["totals"]["routes"] == 2
    assert stats["totals"]["source_pages"] == 1
    assert stats["totals"]["generated_markdown_pages"] == 1
    assert stats["totals"]["talk_entries"] == 1
    assert stats["totals"]["archived_talk_entries"] == 1
    assert stats["links"]["internal"] == 2
    assert stats["links"]["external"] == 1
    assert stats["links"]["broken_internal"] == 1
    assert stats["families"][0]["id"] == "example"


def test_route_table_includes_rendered_talk_pages(tmp_path: Path) -> None:
    make_family(tmp_path)
    generated = tmp_path / "wiki" / "menu" / "10-group" / "10-example" / "generated"
    (generated / "example.html").write_text("<h1>Example</h1>", encoding="utf-8")
    (generated / "example.talk.html").write_text("<h1>Example Talk</h1>", encoding="utf-8")
    (generated / "render-manifest.json").write_text(
        json.dumps(
            {
                "family": {"id": "example"},
                "routes": [
                    {"route": "/example", "output_path": "example.html", "kind": "html"},
                    {"route": "/example.talk", "output_path": "example.talk.html", "kind": "html"},
                ],
                "outputs": [{"path": "example.html"}, {"path": "example.talk.html"}],
            }
        ),
        encoding="utf-8",
    )

    routes = load_route_table(tmp_path).routes

    assert "/example" in routes
    assert "/example.talk" in routes
    assert "/example.talk.html" in routes


def test_write_site_files_writes_stats_file(tmp_path: Path) -> None:
    make_family(tmp_path)
    source = tmp_path / "wiki" / "menu" / "10-group" / "10-example" / "source" / "example.md"
    source.write_text("# Example\n", encoding="utf-8")

    paths = write_site_files(tmp_path)
    stats_path = tmp_path / "wiki" / "generated" / WIKI_STATS_FILENAME

    assert stats_path in paths
    assert load_wiki_stats(tmp_path)["totals"]["families"] == 1


def test_stats_dashboard_formats_like_reader_dashboard() -> None:
    stats = {
        "totals": {"source_pages": 40, "links": 555, "talk_words": 154000, "broken_internal_links": 0},
        "source_corpus": {"days": 35.3, "events": 821402, "sessions": 1060},
        "story": {
            "reading": {"reader_pages": 40, "reader_words": 40000, "estimated_reading_minutes": 178},
            "coverage": {"days": 35.3, "events": 821402, "sessions": 1060},
            "connections": {
                "links": 555,
                "top_destinations": [
                    {"title": "1Context", "count": 16},
                    {"title": "BookStack", "count": 14},
                    {"title": "Guardian", "count": 13},
                    {"title": "wiki-engine", "count": 12},
                    {"title": "Agent UX", "count": 11},
                ],
            },
            "compression": {"raw_chars_to_reader_text_ratio": 200},
            "behind_the_scenes": {"agent_debate_words": 154000},
        },
        "families": [{"id": "topics", "source_pages": 22, "generated_markdown_pages": 0}],
    }

    output = render_stats_dashboard(stats)

    assert "What your work is orbiting" in output
    assert "40K words" in output
    assert "Gatsby-sized" in output
    assert "~200x compression" in output
    assert "154K words of agent debate beneath the readable wiki" in output


def make_family(root: Path) -> None:
    group = root / "wiki" / "menu" / "10-group"
    family = group / "10-example"
    (family / "source").mkdir(parents=True)
    (family / "generated").mkdir()
    group.joinpath("group.toml").write_text('id = "group"\nlabel = "Group"\nmenu_order = 10\n', encoding="utf-8")
    family.joinpath("family.toml").write_text(
        '\n'.join(
            [
                'id = "example"',
                'label = "Example"',
                'route = "/example"',
                'menu_group = "group"',
                'menu_order = 10',
                "",
                "[source]",
                'dir = "source"',
                "",
                "[talk]",
                'dir = "talk"',
                "",
                "[generated]",
                'dir = "generated"',
                "",
            ]
        ),
        encoding="utf-8",
    )
