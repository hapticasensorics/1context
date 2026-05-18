#!/usr/bin/env python3
"""Materialize configured 1Context wiki pages from user-owned templates."""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import re
import sys
import tomllib
from pathlib import Path
from typing import Any


PLACEHOLDER_RE = re.compile(r"{{\s*([A-Za-z0-9_.-]+)\s*}}")
SAFE_SEGMENT_RE = re.compile(r"^[a-z0-9][a-z0-9-]*$")
SAFE_ROUTE_RE = re.compile(r"^/(?:[a-z0-9][a-z0-9-]*(?:/[a-z0-9][a-z0-9-]*)*)?$")
ALLOWED_SITE_PAGE_KINDS = {"source", "generated", "alias", "diagnostic"}


def toml_quote(value: str) -> str:
    return '"' + value.replace("\\", "\\\\").replace('"', '\\"') + '"'


def sha256_text(value: str) -> str:
    return hashlib.sha256(value.encode("utf-8")).hexdigest()


def sha256_file(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def read_toml(path: Path) -> dict[str, Any]:
    with path.open("rb") as handle:
        return tomllib.load(handle)


def render_template(text: str, values: dict[str, str], *, strict: bool) -> str:
    def replace(match: re.Match[str]) -> str:
        key = match.group(1)
        if key in values:
            return values[key]
        if strict:
            raise ValueError(f"missing template value: {key}")
        return match.group(0)

    return PLACEHOLDER_RE.sub(replace, text)


class Materializer:
    def __init__(self, runtime_home: Path, dry_run: bool = False) -> None:
        self.runtime_home = runtime_home
        self.user_wiki = runtime_home / "1Context" / "user-wiki"
        self.app_support = runtime_home / "Library" / "Application Support" / "1Context"
        self.config_path = self.user_wiki / "wiki.toml"
        self.templates_dir = self.user_wiki / "templates"
        self.setup_dir = self.app_support / "setup"
        self.state_path = self.setup_dir / "wiki-page-materialize.toml"
        self.dry_run = dry_run
        self.files: list[dict[str, str]] = []
        self.pages: list[dict[str, str]] = []

    def run(self) -> None:
        if not self.config_path.exists():
            raise SystemExit(f"Missing wiki config: {self.config_path}")

        config = read_toml(self.config_path)
        self.validate_config(config)
        if not config.get("materialization", {}).get("enabled", True):
            self.write_state(config_hash=sha256_file(self.config_path))
            print(f"materialized_pages=0 state={self.state_path}")
            return

        for page in config.get("pages", []):
            self.materialize_page(config, page)

        self.write_state(config_hash=sha256_file(self.config_path))
        installed = sum(1 for page in self.pages if page["status"] == "materialized")
        print(f"materialized_pages={installed} state={self.state_path}")

    def validate_config(self, config: dict[str, Any]) -> None:
        seen_ids: dict[str, str] = {}
        seen_routes: dict[str, str] = {}

        def remember(kind: str, entry: dict[str, Any]) -> None:
            entry_id = required(entry, "id")
            route = required(entry, "route")
            validate_id(entry_id, f"{kind}.id")
            validate_route(route, f"{kind} {entry_id}")
            if entry_id in seen_ids:
                raise SystemExit(f"Duplicate wiki page id: {entry_id}")
            if route in seen_routes:
                raise SystemExit(f"Duplicate wiki route {route}: {seen_routes[route]} and {entry_id}")
            seen_ids[entry_id] = kind
            seen_routes[route] = entry_id

        for site_page in config.get("site_pages", []):
            remember("site_pages", site_page)
            kind = str(site_page.get("kind", "generated"))
            if kind not in ALLOWED_SITE_PAGE_KINDS:
                allowed = ", ".join(sorted(ALLOWED_SITE_PAGE_KINDS))
                raise SystemExit(f"Invalid site page kind for {required(site_page, 'id')}: {kind}; allowed: {allowed}")
            if "template" in site_page:
                self.validate_template_rel(str(site_page["template"]), f"site_pages.{required(site_page, 'id')}.template")
            if kind == "alias" and site_page.get("enabled", True):
                has_target = any(site_page.get(key) for key in ["target_page", "target_route", "target_policy"])
                if not has_target:
                    raise SystemExit(f"Enabled alias site page {required(site_page, 'id')} must declare a target")

        for page in config.get("pages", []):
            remember("pages", page)
            validate_id(required(page, "slug"), f"pages.{required(page, 'id')}.slug")
            validate_id(required(page, "family_group"), f"pages.{required(page, 'id')}.family_group")
            validate_id(required(page, "family_id"), f"pages.{required(page, 'id')}.family_id")
            self.validate_template_rel(required(page, "template"), f"pages.{required(page, 'id')}.template")
            for key in ["talk_conventions_template", "talk_curator_template"]:
                if key in page:
                    self.validate_template_rel(str(page[key]), f"pages.{required(page, 'id')}.{key}")

    def validate_template_rel(self, template_rel: str, label: str) -> None:
        if Path(template_rel).is_absolute():
            raise SystemExit(f"{label} must be relative to templates/: {template_rel}")
        if any(part in {"", ".", ".."} for part in Path(template_rel).parts):
            raise SystemExit(f"{label} must stay inside templates/: {template_rel}")
        resolved = (self.templates_dir / template_rel).resolve(strict=False)
        try:
            resolved.relative_to(self.templates_dir.resolve(strict=False))
        except ValueError as exc:
            raise SystemExit(f"{label} escapes templates/: {template_rel}") from exc

    def materialize_page(self, config: dict[str, Any], page: dict[str, Any]) -> None:
        page_id = required(page, "id")
        if not page.get("enabled", True):
            self.pages.append({"id": page_id, "route": str(page.get("route", "")), "status": "disabled"})
            return

        defaults = config.get("defaults", {})
        family_group = required(page, "family_group")
        family_id = required(page, "family_id")
        slug = required(page, "slug")
        title = required(page, "title")
        route = required(page, "route")
        template_rel = required(page, "template")

        family_root = self.user_wiki / "source" / "families" / family_group / family_id
        source_path = family_root / "source" / f"{slug}.md"
        tombstone_path = family_root / "source" / f"{slug}.tombstone.toml"
        talk_folder = family_root / "talk" / f"{slug}.talk"
        template_path = self.templates_dir / template_rel
        if not template_path.exists():
            raise SystemExit(f"Missing page template for {page_id}: {template_path}")

        if tombstone_path.exists():
            self.pages.append({"id": page_id, "route": route, "status": "tombstoned"})
            self.files.append({"path": relpath(tombstone_path, self.runtime_home), "status": "tombstoned"})
            return

        values = self.page_values(
            page=page,
            defaults=defaults,
            family_root=family_root,
            source_path=source_path,
            talk_folder=talk_folder,
        )

        self.install_file(
            self.user_wiki / "source" / "families" / family_group / "group.toml",
            self.group_toml(page),
            source_template="generated:group.toml",
        )
        self.install_file(
            family_root / "family.toml",
            self.family_toml(page),
            source_template="generated:family.toml",
        )

        rendered_source = render_template(template_path.read_text(encoding="utf-8"), values, strict=True)
        self.install_file(source_path, rendered_source, source_template=template_rel)
        self.install_file(
            talk_folder / "_meta.yaml",
            self.talk_meta_yaml(page, source_path, values),
            source_template="generated:talk-meta.yaml",
        )

        conventions_template = str(page.get("talk_conventions_template", "talk/conventions.md"))
        self.install_from_template(
            conventions_template,
            talk_folder / "_conventions.md",
            values,
            strict=False,
        )

        curator_template = page.get("talk_curator_template")
        if curator_template:
            self.install_from_template(str(curator_template), talk_folder / "_curator.md", values, strict=False)
            self.install_from_template(
                str(curator_template),
                family_root / "templates" / "talk" / "_curator.template.md",
                values,
                strict=False,
            )

        self.install_file(
            family_root / "templates" / "page.template.md",
            rendered_source,
            source_template=template_rel,
        )
        self.install_from_template(
            conventions_template,
            family_root / "templates" / "talk" / "_conventions.template.md",
            values,
            strict=False,
        )
        self.install_from_template(
            "talk/entry.md",
            family_root / "templates" / "talk" / "entry.template.md",
            values,
            strict=False,
        )

        self.pages.append({"id": page_id, "route": route, "status": "materialized"})

    def page_values(
        self,
        *,
        page: dict[str, Any],
        defaults: dict[str, Any],
        family_root: Path,
        source_path: Path,
        talk_folder: Path,
    ) -> dict[str, str]:
        created_date = dt.datetime.now(dt.UTC).date().isoformat()
        now_utc = dt.datetime.now(dt.UTC).replace(microsecond=0).isoformat().replace("+00:00", "Z")
        slug = required(page, "slug")
        route = required(page, "route")
        values = {
            "page_id": required(page, "id"),
            "title": required(page, "title"),
            "slug": slug,
            "page_slug": slug,
            "route": route,
            "md_url": f"/{slug}.md",
            "talk_route": route.rstrip("/") + "/talk",
            "talk_md_url": f"/{slug}.talk.md",
            "page_type": str(page.get("type", "")),
            "section": str(page.get("section", page.get("family_group", "context"))),
            "operator_name": str(defaults.get("operator_name", "Operator")),
            "access_tier": str(defaults.get("access_tier", "private")),
            "asset_base": str(defaults.get("asset_base", ".")),
            "home_href": str(defaults.get("home_href", "/")),
            "created_date": str(page.get("created_date", created_date)),
            "last_updated": str(page.get("last_updated", created_date)),
            "created_at": now_utc,
            "updated_at": now_utc,
            "summary": str(page.get("summary", "")),
            "article_path": "user-wiki://" + relpath(source_path, self.user_wiki),
            "talk_folder": "user-wiki://" + relpath(talk_folder, self.user_wiki),
            "talk_for_uri": "page://" + required(page, "id"),
            "concept_page_dir": "user-wiki://" + relpath(family_root.parent, self.user_wiki),
        }
        return values

    def install_from_template(
        self,
        template_rel: str,
        dest: Path,
        values: dict[str, str],
        *,
        strict: bool,
    ) -> None:
        source = self.templates_dir / template_rel
        if not source.exists():
            raise SystemExit(f"Missing template: {source}")
        rendered = render_template(source.read_text(encoding="utf-8"), values, strict=strict)
        self.install_file(dest, rendered, source_template=template_rel)

    def install_file(self, dest: Path, content: str, *, source_template: str) -> None:
        dest_rel = relpath(dest, self.runtime_home)
        status = "installed"
        if dest.exists():
            existing = dest.read_text(encoding="utf-8")
            status = "unchanged" if existing == content else "skipped_existing"
        elif not self.dry_run:
            dest.parent.mkdir(parents=True, exist_ok=True)
            dest.write_text(content, encoding="utf-8")

        installed_hash = sha256_file(dest) if dest.exists() else ""
        self.files.append(
            {
                "path": dest_rel,
                "source_template": source_template,
                "source_hash": sha256_text(content),
                "installed_hash": installed_hash,
                "status": status,
            }
        )

    def write_state(self, *, config_hash: str) -> None:
        lines = [
            "schema_version = 1",
            f'materialized_at = "{dt.datetime.now(dt.UTC).replace(microsecond=0).isoformat().replace("+00:00", "Z")}"',
            f'wiki_config = "{relpath(self.config_path, self.runtime_home)}"',
            f'wiki_config_hash = "{config_hash}"',
            "",
        ]

        for page in self.pages:
            lines.append("[[pages]]")
            for key in ["id", "route", "status"]:
                if key in page:
                    lines.append(f"{key} = {toml_quote(page[key])}")
            lines.append("")

        for file in self.files:
            lines.append("[[files]]")
            for key in ["path", "source_template", "source_hash", "installed_hash", "status"]:
                if key in file:
                    lines.append(f"{key} = {toml_quote(file[key])}")
            lines.append("")

        if not self.dry_run:
            self.setup_dir.mkdir(parents=True, exist_ok=True)
            self.state_path.write_text("\n".join(lines), encoding="utf-8")

    def group_toml(self, page: dict[str, Any]) -> str:
        title = str(page.get("family_group_title", required(page, "family_group").replace("-", " ").title()))
        return f'title = {toml_quote(title)}\n'

    def family_toml(self, page: dict[str, Any]) -> str:
        lines = [
            "schema_version = 1",
            f'id = {toml_quote(required(page, "family_id"))}',
            f'title = {toml_quote(str(page.get("family_title", required(page, "title"))))}',
            f'page_id = {toml_quote(required(page, "id"))}',
            f'slug = {toml_quote(required(page, "slug"))}',
            f'route = {toml_quote(required(page, "route"))}',
            f'type = {toml_quote(str(page.get("type", "")))}',
            f'template = {toml_quote(required(page, "template"))}',
            "",
        ]
        return "\n".join(lines)

    def talk_meta_yaml(self, page: dict[str, Any], source_path: Path, values: dict[str, str]) -> str:
        slug = required(page, "slug")
        route = required(page, "route")
        talk_route = route.rstrip("/") + "/talk"
        return "\n".join(
            [
                f'title: "Talk - {required(page, "title")}"',
                f'page_id: "{required(page, "id")}"',
                f'page_route: "{route}"',
                f'talk_route: "{talk_route}"',
                f'slug: "{slug}.talk"',
                f'section: "{required(page, "family_group")}"',
                f'access: "{values["access_tier"]}"',
                f'talk_for: "page://{required(page, "id")}"',
                f'page: "user-wiki://{relpath(source_path, self.user_wiki)}"',
                "status: open",
                "schema_version: 1",
                f'created: "{values["created_at"]}"',
                f'updated: "{values["updated_at"]}"',
                f'md_url: "/{slug}.talk.md"',
                "",
            ]
        )


def required(mapping: dict[str, Any], key: str) -> str:
    value = mapping.get(key)
    if value is None or str(value) == "":
        raise SystemExit(f"Missing required page field: {key}")
    return str(value)


def validate_id(value: str, label: str) -> None:
    if not SAFE_SEGMENT_RE.match(value):
        raise SystemExit(f"Invalid {label}: {value}; expected lower-kebab id")


def validate_route(value: str, label: str) -> None:
    if not SAFE_ROUTE_RE.match(value):
        raise SystemExit(f"Invalid route for {label}: {value}")


def relpath(path: Path, root: Path) -> str:
    return path.resolve().relative_to(root.resolve()).as_posix()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("runtime_home", help="Runtime root shaped like runtime-test/")
    parser.add_argument("--dry-run", action="store_true")
    args = parser.parse_args()

    Materializer(Path(args.runtime_home), dry_run=args.dry_run).run()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
