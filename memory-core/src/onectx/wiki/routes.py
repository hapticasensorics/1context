from __future__ import annotations

import json
from dataclasses import dataclass
from pathlib import Path
from typing import Any
from urllib.parse import unquote, urlsplit

from .manifest import MANIFEST_FILENAME


@dataclass(frozen=True)
class RouteTarget:
    route: str
    path: Path
    family_id: str
    kind: str
    source: str
    content_type: str

    def to_payload(self, root: Path) -> dict[str, str]:
        return {
            "route": self.route,
            "path": format_path(self.path, root),
            "family_id": self.family_id,
            "kind": self.kind,
            "source": self.source,
            "content_type": self.content_type,
        }


@dataclass(frozen=True)
class RouteTable:
    root: Path
    routes: dict[str, RouteTarget]
    manifests: tuple[Path, ...]

    def resolve(self, raw_path: str) -> RouteTarget | None:
        route = normalize_route(raw_path)
        if route in self.routes:
            return self.routes[route]

        without_owner = strip_owner_prefix(route)
        if without_owner and without_owner in self.routes:
            return self.routes[without_owner]

        return None

    def to_payload(self) -> dict[str, Any]:
        return {
            "manifests": [format_path(path, self.root) for path in self.manifests],
            "routes": [target.to_payload(self.root) for _, target in sorted(self.routes.items())],
        }


def load_route_table(root: Path | str) -> RouteTable:
    root = Path(root).resolve()
    routes: dict[str, RouteTarget] = {}
    manifests = tuple(sorted((root / "wiki" / "menu").glob(f"**/generated/{MANIFEST_FILENAME}")))
    for manifest_path in manifests:
        add_manifest_routes(root, routes, manifest_path)
    return RouteTable(root=root, routes=routes, manifests=manifests)


def add_manifest_routes(root: Path, routes: dict[str, RouteTarget], manifest_path: Path) -> None:
    try:
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    except json.JSONDecodeError:
        return

    family = manifest.get("family", {})
    family_id = str(family.get("id") or manifest_path.parent.parent.name)

    for item in manifest.get("routes", []):
        if not isinstance(item, dict):
            continue
        route = item.get("route")
        output_path = item.get("output_path")
        if not isinstance(route, str) or not isinstance(output_path, str):
            continue
        path = resolve_manifest_path(root, manifest_path.parent, output_path)
        if not is_public_html_output(path):
            continue
        add_route(
            routes,
            route,
            path,
            family_id=family_id,
            kind=str(item.get("kind") or "html"),
            source=str(item.get("source") or "manifest_route"),
        )

    for item in manifest.get("outputs", []):
        if not isinstance(item, dict):
            continue
        output_path = item.get("path")
        if not isinstance(output_path, str):
            continue
        path = resolve_manifest_path(root, manifest_path.parent, output_path)
        add_output_routes(routes, manifest_path.parent, path, family_id=family_id)


def add_output_routes(routes: dict[str, RouteTarget], output_dir: Path, path: Path, *, family_id: str) -> None:
    if not path.is_file():
        return
    if not is_public_html_output(path):
        return
    try:
        relative = path.relative_to(output_dir)
    except ValueError:
        return

    route = "/" + relative.as_posix()
    add_route(
        routes,
        route,
        path,
        family_id=family_id,
        kind="generated_output",
        source="manifest_output",
    )
    add_route(
        routes,
        "/" + relative.with_suffix("").as_posix(),
        path,
        family_id=family_id,
        kind="html",
        source="manifest_output",
    )


def add_route(
    routes: dict[str, RouteTarget],
    route: str,
    path: Path,
    *,
    family_id: str,
    kind: str,
    source: str,
) -> None:
    if not path.is_file():
        return
    normalized = normalize_route(route)
    if normalized in routes:
        return
    routes[normalized] = RouteTarget(
        route=normalized,
        path=path.resolve(),
        family_id=family_id,
        kind=kind,
        source=source,
        content_type=content_type(path),
    )


def normalize_route(raw_path: str) -> str:
    route = unquote(urlsplit(raw_path).path or "/")
    if not route.startswith("/"):
        route = "/" + route
    if route != "/" and route.endswith("/"):
        route = route.rstrip("/")
    return route


def strip_owner_prefix(route: str) -> str:
    parts = route.split("/")
    if len(parts) < 3:
        return ""
    return "/" + "/".join(parts[2:])


def resolve_manifest_path(root: Path, manifest_dir: Path, value: str) -> Path:
    path = Path(value).expanduser()
    if path.is_absolute():
        resolved = path.resolve()
        return resolved if is_within_root(resolved, manifest_dir) else manifest_dir / "__outside_generated_outputs__"
    root_relative = (root / path).resolve()
    if is_within_root(root_relative, manifest_dir):
        return root_relative
    manifest_relative = (manifest_dir / path).resolve()
    return manifest_relative if is_within_root(manifest_relative, manifest_dir) else manifest_dir / "__outside_generated_outputs__"


def is_within_root(path: Path, root: Path) -> bool:
    try:
        path.resolve().relative_to(root.resolve())
        return True
    except ValueError:
        return False


def is_public_html_output(path: Path) -> bool:
    name = path.name.lower()
    return (
        path.suffix.lower() == ".html"
        and ".private." not in name
        and ".internal." not in name
        and ".talk." not in name
    )


def content_type(path: Path) -> str:
    suffix = path.suffix.lower()
    if suffix == ".html":
        return "text/html; charset=utf-8"
    if suffix == ".md":
        return "text/markdown; charset=utf-8"
    if suffix == ".json":
        return "application/json; charset=utf-8"
    if suffix == ".css":
        return "text/css; charset=utf-8"
    if suffix in {".js", ".mjs"}:
        return "text/javascript; charset=utf-8"
    if suffix == ".png":
        return "image/png"
    if suffix == ".ico":
        return "image/x-icon"
    if suffix == ".svg":
        return "image/svg+xml"
    if suffix == ".webmanifest":
        return "application/manifest+json; charset=utf-8"
    return "application/octet-stream"


def format_path(path: Path, root: Path) -> str:
    try:
        return str(path.relative_to(root))
    except ValueError:
        return str(path)
