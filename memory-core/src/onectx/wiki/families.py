from __future__ import annotations

import tomllib
from dataclasses import dataclass
from pathlib import Path
from typing import Any


class WikiError(RuntimeError):
    """Raised when the wiki workspace or engine cannot be used."""


@dataclass(frozen=True)
class WikiFamily:
    id: str
    label: str
    kind: str
    route: str
    menu_group: str
    menu_group_label: str
    menu_group_order: int
    menu_order: int
    template_version: str
    path: Path
    manifest_path: Path
    source_dir: Path
    source_primary: Path | None
    talk_dir: Path
    talk_primary: Path | None
    talk_conventions: Path | None
    curator_prompt: Path | None
    generated_dir: Path
    jobs: dict[str, str]
    policies: dict[str, Any]

    def to_payload(self, root: Path | None = None) -> dict[str, Any]:
        return {
            "id": self.id,
            "label": self.label,
            "kind": self.kind,
            "route": self.route,
            "menu_group": self.menu_group,
            "menu_group_label": self.menu_group_label,
            "menu_group_order": self.menu_group_order,
            "menu_order": self.menu_order,
            "template_version": self.template_version,
            "path": format_path(self.path, root),
            "manifest_path": format_path(self.manifest_path, root),
            "source_dir": format_path(self.source_dir, root),
            "source_primary": format_optional_path(self.source_primary, root),
            "talk_dir": format_path(self.talk_dir, root),
            "talk_primary": format_optional_path(self.talk_primary, root),
            "talk_conventions": format_optional_path(self.talk_conventions, root),
            "curator_prompt": format_optional_path(self.curator_prompt, root),
            "generated_dir": format_path(self.generated_dir, root),
            "jobs": dict(self.jobs),
            "policies": dict(self.policies),
        }


def discover_families(root: Path | str) -> tuple[WikiFamily, ...]:
    root = Path(root).resolve()
    menu_dir = root / "wiki" / "menu"
    if not menu_dir.is_dir():
        return ()

    families: list[WikiFamily] = []
    seen: dict[str, Path] = {}
    for manifest_path in sorted(menu_dir.rglob("family.toml")):
        family = load_family_manifest(root, manifest_path)
        if family.id in seen:
            raise WikiError(f"duplicate wiki family id {family.id!r}: {seen[family.id]} and {manifest_path}")
        seen[family.id] = manifest_path
        families.append(family)

    return tuple(sorted(families, key=lambda item: (item.menu_group_order, item.menu_order, item.id)))


def family_by_id(root: Path | str, family_id: str) -> WikiFamily:
    for family in discover_families(root):
        if family.id == family_id:
            return family
    raise WikiError(f"unknown wiki family {family_id!r}")


def load_family_manifest(root: Path, manifest_path: Path) -> WikiFamily:
    raw = read_toml(manifest_path)
    family_path = manifest_path.parent
    group_path = family_path.parent
    group = read_toml(group_path / "group.toml") if (group_path / "group.toml").exists() else {}

    family_id = str(raw.get("id") or strip_order_prefix(family_path.name)).strip()
    if not family_id:
        raise WikiError(f"{manifest_path} must declare a non-empty id")

    source = section(raw, "source")
    talk = section(raw, "talk")
    generated = section(raw, "generated")

    return WikiFamily(
        id=family_id,
        label=str(raw.get("label") or family_id),
        kind=str(raw.get("kind") or "page_family"),
        route=str(raw.get("route") or f"/{family_id}"),
        menu_group=str(raw.get("menu_group") or group.get("id") or strip_order_prefix(group_path.name)),
        menu_group_label=str(group.get("label") or raw.get("menu_group") or strip_order_prefix(group_path.name)),
        menu_group_order=parse_int(group.get("menu_order"), prefix_order(group_path.name)),
        menu_order=parse_int(raw.get("menu_order"), prefix_order(family_path.name)),
        template_version=str(raw.get("template_version") or ""),
        path=family_path,
        manifest_path=manifest_path,
        source_dir=resolve_family_path(family_path, str(source.get("dir") or "source")),
        source_primary=optional_family_path(family_path, source.get("primary")),
        talk_dir=resolve_family_path(family_path, str(talk.get("dir") or "talk")),
        talk_primary=optional_family_path(family_path, talk.get("primary")),
        talk_conventions=optional_family_path(family_path, talk.get("conventions")),
        curator_prompt=optional_family_path(family_path, talk.get("curator_prompt")),
        generated_dir=resolve_family_path(family_path, str(generated.get("dir") or "generated")),
        jobs={str(key): str(value) for key, value in section(raw, "jobs").items()},
        policies=dict(section(raw, "policies")),
    )


def read_toml(path: Path) -> dict[str, Any]:
    try:
        return tomllib.loads(path.read_text(encoding="utf-8"))
    except tomllib.TOMLDecodeError as exc:
        raise WikiError(f"invalid TOML in {path}: {exc}") from exc


def section(raw: dict[str, Any], key: str) -> dict[str, Any]:
    value = raw.get(key, {})
    return value if isinstance(value, dict) else {}


def resolve_family_path(family_path: Path, value: str) -> Path:
    path = Path(value).expanduser()
    if not path.is_absolute():
        path = family_path / path
    resolved = path.resolve()
    try:
        resolved.relative_to(family_path.resolve())
    except ValueError as exc:
        raise WikiError(f"{family_path / 'family.toml'} path escapes the wiki family directory: {value}") from exc
    return resolved


def optional_family_path(family_path: Path, value: Any) -> Path | None:
    if not value:
        return None
    if not isinstance(value, str):
        raise WikiError(f"{family_path / 'family.toml'} path value must be a string, got {value!r}")
    return resolve_family_path(family_path, value)


def prefix_order(name: str) -> int:
    head = name.split("-", 1)[0]
    return int(head) if head.isdigit() else 0


def strip_order_prefix(name: str) -> str:
    head, sep, tail = name.partition("-")
    return tail if sep and head.isdigit() else name


def parse_int(value: Any, default: int) -> int:
    if value is None:
        return default
    try:
        return int(value)
    except (TypeError, ValueError) as exc:
        raise WikiError(f"wiki menu order must be an integer, got {value!r}") from exc


def format_optional_path(path: Path | None, root: Path | None) -> str:
    return format_path(path, root) if path else ""


def format_path(path: Path, root: Path | None) -> str:
    if root:
        try:
            return str(path.relative_to(root))
        except ValueError:
            pass
    return str(path)
