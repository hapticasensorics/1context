from __future__ import annotations

from pathlib import Path

import pytest

from onectx.wiki.families import family_by_id
from onectx.wiki.render import render_family, source_inputs
from onectx.wiki.families import WikiError
from onectx.wiki.site import build_content_index


def make_tier_family(root: Path, *, include_public: bool = True) -> Path:
    (root / "wiki-engine").symlink_to(Path.cwd() / "wiki-engine", target_is_directory=True)
    group = root / "wiki" / "menu" / "10-test"
    family = group / "10-tiers"
    source = family / "source"
    generated = family / "generated"
    source.mkdir(parents=True)
    generated.mkdir()
    (root / "wiki").mkdir(exist_ok=True)
    (root / "wiki" / "wiki.toml").write_text('title = "Test Wiki"\n', encoding="utf-8")
    (group / "group.toml").write_text('id = "test"\nlabel = "Test"\nmenu_order = 10\n', encoding="utf-8")
    (family / "family.toml").write_text(
        "\n".join(
            [
                'id = "tiers"',
                'label = "Tiers"',
                'route = "/tiers"',
                'menu_group = "test"',
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
    write_page(source / "example.md", access="private", body="Private-only memory detail.")
    write_page(source / "example.internal.md", access="shared", body="Internal collaboration detail.")
    if include_public:
        write_page(source / "example.public.md", access="public", body="Public release detail.")
    return source


def write_page(path: Path, *, access: str, body: str) -> None:
    shared_with = 'shared_with: ["team"]\n' if access == "shared" else ""
    path.write_text(
        f"""---
title: Example
slug: example
section: product
access: {access}
{shared_with}---
# Example

{body}
""",
        encoding="utf-8",
    )


def test_tier_rendering_uses_explicit_public_internal_and_private_sources(tmp_path: Path) -> None:
    make_tier_family(tmp_path)

    result = render_family(tmp_path, "tiers", include_talk=False)

    output_dir = tmp_path / "wiki" / "menu" / "10-test" / "10-tiers" / "generated"
    public_html = (output_dir / "example.html").read_text(encoding="utf-8")
    internal_html = (output_dir / "example.internal.html").read_text(encoding="utf-8")
    private_html = (output_dir / "example.private.html").read_text(encoding="utf-8")
    assert "Public release detail." in public_html
    assert "Internal collaboration detail." in internal_html
    assert "Private-only memory detail." in private_html
    assert "Private-only memory detail." not in public_html
    assert "Internal collaboration detail." not in public_html

    manifest = result.manifest or {}
    tier_sources = manifest["tier_sources"]
    assert tier_sources[0]["canonical_private_source"].endswith("example.md")
    assert tier_sources[0]["public_reads_private"] is False
    assert tier_sources[0]["tiers"]["private"]["exists"] is True
    assert tier_sources[0]["tiers"]["internal"]["exists"] is True
    assert tier_sources[0]["tiers"]["public"]["exists"] is True
    output_tiers = {item.get("audience_tier") for item in manifest["outputs"] if item["path"].endswith(".html")}
    assert {"public", "internal", "private"} <= output_tiers


def test_source_inputs_keep_canonical_private_source_as_single_input(tmp_path: Path) -> None:
    make_tier_family(tmp_path)
    family = family_by_id(tmp_path, "tiers")

    inputs = source_inputs(family)

    assert [path.name for path in inputs] == ["example.md"]


def test_internal_tier_without_public_refuses_private_public_fallback(tmp_path: Path) -> None:
    make_tier_family(tmp_path, include_public=False)

    with pytest.raises(WikiError, match="Refusing to render public output from the private canonical source"):
        render_family(tmp_path, "tiers", include_talk=False)


def test_content_index_excludes_private_source_and_private_outputs(tmp_path: Path) -> None:
    make_tier_family(tmp_path)
    render_family(tmp_path, "tiers", include_talk=False)

    index = build_content_index(tmp_path)
    payload = str(index)

    assert "Public release detail." in payload
    assert "Private-only memory detail." not in payload
    assert "Internal collaboration detail." not in payload
    assert "markdown" not in index["pages"][0]


def test_family_paths_cannot_escape_family_directory(tmp_path: Path) -> None:
    make_tier_family(tmp_path)
    manifest = tmp_path / "wiki" / "menu" / "10-test" / "10-tiers" / "family.toml"
    manifest.write_text(
        "\n".join(
            [
                'id = "tiers"',
                'route = "/tiers"',
                "[source]",
                'dir = "../outside"',
            ]
        ),
        encoding="utf-8",
    )

    with pytest.raises(WikiError, match="escapes"):
        family_by_id(tmp_path, "tiers")
