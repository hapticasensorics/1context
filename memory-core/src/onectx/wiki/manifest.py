from __future__ import annotations

import hashlib
import json
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

from .families import WikiFamily, format_path


MANIFEST_SCHEMA_VERSION = "wiki.render-manifest.v1"
MANIFEST_FILENAME = "render-manifest.json"
AUDIENCE_SOURCE_SUFFIXES = (".private.md", ".internal.md", ".public.md")


def build_render_manifest(
    *,
    root: Path,
    family: WikiFamily,
    engine_root: Path,
    output_dir: Path,
    invocations: tuple[Any, ...],
    outputs: tuple[Path, ...],
    include_talk: bool,
) -> dict[str, Any]:
    return {
        "schema_version": MANIFEST_SCHEMA_VERSION,
        "rendered_at": utc_now(),
        "family": family.to_payload(root),
        "engine": engine_payload(root, engine_root),
        "output_dir": format_path(output_dir, root),
        "include_talk": include_talk,
        "invocations": [item.to_payload(root) for item in invocations],
        "inputs": input_records(root, family, engine_root, include_talk=include_talk),
        "tier_sources": tier_source_records(root, family),
        "outputs": output_records(root, outputs),
        "routes": route_records(root, family, output_dir, outputs),
        "checks": checks_for_manifest(family, outputs),
    }


def write_render_manifest(path: Path, manifest: dict[str, Any]) -> Path:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(stable_json(manifest) + "\n", encoding="utf-8")
    return path


def input_records(root: Path, family: WikiFamily, engine_root: Path, *, include_talk: bool) -> list[dict[str, Any]]:
    records: list[dict[str, Any]] = []
    maybe_add_file(records, root, family.manifest_path, "family_manifest")
    group_manifest = family.path.parent / "group.toml"
    maybe_add_file(records, root, group_manifest, "menu_group_manifest")
    maybe_add_file(records, root, root / "wiki" / "wiki.toml", "wiki_manifest")
    maybe_add_file(records, root, engine_root / "package.json", "engine_package")
    maybe_add_file(records, root, engine_root / "package-lock.json", "engine_lock")

    for source in canonical_source_paths(family):
        maybe_add_file(records, root, source, "source_canonical_private")
        for tier, tier_path in tier_variant_paths(source).items():
            if tier_path != source:
                maybe_add_file(records, root, tier_path, f"source_tier_{tier}")

    if include_talk:
        if family.talk_primary:
            add_talk_folder_records(records, root, family.talk_primary)
        elif family.talk_dir.is_dir():
            for path in sorted(family.talk_dir.rglob("*.talk")):
                if path.is_dir():
                    add_talk_folder_records(records, root, path)

    return records


def add_talk_folder_records(records: list[dict[str, Any]], root: Path, folder: Path) -> None:
    if not folder.is_dir():
        return
    for path in sorted(item for item in folder.rglob("*") if item.is_file()):
        maybe_add_file(records, root, path, talk_role(path))


def talk_role(path: Path) -> str:
    if path.name == "_meta.yaml":
        return "talk_meta"
    if path.name == "_conventions.md":
        return "talk_conventions"
    if path.name == "_curator.md":
        return "curator_prompt"
    return "talk_entry"


def output_records(root: Path, outputs: tuple[Path, ...]) -> list[dict[str, Any]]:
    records = []
    for path in outputs:
        if path.name == MANIFEST_FILENAME:
            continue
        record = file_record(root, path, "generated_output")
        tier = tier_from_output_path(path)
        if tier:
            record["audience_tier"] = tier
        records.append(record)
    return records


def route_records(root: Path, family: WikiFamily, output_dir: Path, outputs: tuple[Path, ...]) -> list[dict[str, Any]]:
    output_set = {path.resolve() for path in outputs}
    records: list[dict[str, Any]] = []

    latest = latest_family_output(family, output_dir)
    if latest and latest.resolve() in output_set:
        records.append(
            {
                "route": family.route,
                "output_path": format_path(latest, root),
                "kind": "latest_family_html",
                "source": "latest_for_family.json",
            }
        )

    for path in sorted(output_set):
        if path.suffix != ".html":
            continue
        route = "/" + path.relative_to(output_dir).with_suffix("").as_posix()
        records.append(
            {
                "route": route,
                "output_path": format_path(path, root),
                "kind": "html",
                "source": "generated_output",
            }
        )
    return records


def tier_source_records(root: Path, family: WikiFamily) -> list[dict[str, Any]]:
    records = []
    for source in canonical_source_paths(family):
        variants = tier_variant_paths(source)
        tiers: dict[str, Any] = {}
        for tier, path in variants.items():
            tiers[tier] = {
                "path": format_path(path.resolve(), root),
                "exists": path.is_file(),
                "sha256": file_sha256(path) if path.is_file() else "",
                "selected_for_render": path.is_file() or tier == "private",
            }
        records.append(
            {
                "base": source.stem,
                "canonical_private_source": format_path(source.resolve(), root),
                "model": "canonical-private-with-explicit-tier-siblings",
                "public_reads_private": variants["internal"].is_file() and not variants["public"].is_file(),
                "tiers": tiers,
            }
        )
    return records


def canonical_source_paths(family: WikiFamily) -> tuple[Path, ...]:
    if family.source_primary:
        return (family.source_primary,)
    if not family.source_dir.is_dir():
        return ()
    return tuple(
        sorted(
            path
            for path in family.source_dir.rglob("*.md")
            if path.is_file() and not path.name.endswith(AUDIENCE_SOURCE_SUFFIXES)
        )
    )


def tier_variant_paths(source: Path) -> dict[str, Path]:
    return {
        "private": source,
        "internal": source.with_name(f"{source.stem}.internal.md"),
        "public": source.with_name(f"{source.stem}.public.md"),
    }


def tier_from_output_path(path: Path) -> str:
    stem = path.stem
    if stem.endswith(".private"):
        return "private"
    if stem.endswith(".internal"):
        return "internal"
    return "public" if path.suffix in {".html", ".md"} else ""


def latest_family_output(family: WikiFamily, output_dir: Path) -> Path | None:
    latest_path = output_dir / "latest_for_family.json"
    if not latest_path.exists():
        fallback = output_dir / f"{family.id}.html"
        return fallback if fallback.exists() else None
    try:
        latest = json.loads(latest_path.read_text(encoding="utf-8"))
    except json.JSONDecodeError:
        return None
    record = latest.get(family.id)
    if not isinstance(record, dict):
        return None
    slug = record.get("slug")
    if not isinstance(slug, str) or not slug:
        return None
    return output_dir / f"{slug}.html"


def checks_for_manifest(family: WikiFamily, outputs: tuple[Path, ...]) -> list[dict[str, str]]:
    html_outputs = [path for path in outputs if path.suffix == ".html"]
    return [
        {
            "id": "wiki.render.succeeded",
            "status": "passed",
            "text": f"Rendered wiki family {family.id}.",
        },
        {
            "id": "wiki.generated.available",
            "status": "passed" if html_outputs else "failed",
            "text": f"{len(outputs)} generated file(s), {len(html_outputs)} HTML file(s).",
        },
        {
            "id": "wiki.manifest.written",
            "status": "passed",
            "text": f"{MANIFEST_FILENAME} written beside generated outputs.",
        },
    ]


def engine_payload(root: Path, engine_root: Path) -> dict[str, str]:
    package_path = engine_root / "package.json"
    payload = {
        "name": "@1context/wiki-engine",
        "version": "local",
        "path": format_path(engine_root, root),
        "manifest_schema": MANIFEST_SCHEMA_VERSION,
    }
    if not package_path.exists():
        return payload
    try:
        package = json.loads(package_path.read_text(encoding="utf-8"))
    except json.JSONDecodeError:
        return payload
    payload["name"] = str(package.get("name") or payload["name"])
    payload["version"] = str(package.get("version") or payload["version"])
    return payload


def maybe_add_file(records: list[dict[str, Any]], root: Path, path: Path | None, role: str) -> None:
    if path and path.is_file():
        records.append(file_record(root, path, role))


def file_record(root: Path, path: Path, role: str) -> dict[str, Any]:
    data = path.read_bytes()
    return {
        "role": role,
        "path": format_path(path.resolve(), root),
        "sha256": hashlib.sha256(data).hexdigest(),
        "bytes": len(data),
        "content_type": content_type(path),
    }


def file_sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def content_type(path: Path) -> str:
    suffix = path.suffix.lower()
    if suffix == ".html":
        return "text/html"
    if suffix == ".md":
        return "text/markdown"
    if suffix == ".json":
        return "application/json"
    if suffix in {".yaml", ".yml"}:
        return "application/yaml"
    if suffix == ".toml":
        return "application/toml"
    if suffix == ".css":
        return "text/css"
    if suffix == ".js" or suffix == ".mjs":
        return "text/javascript"
    return "application/octet-stream"


def stable_json(value: Any) -> str:
    return json.dumps(value, indent=2, sort_keys=True)


def utc_now() -> str:
    return datetime.now(timezone.utc).isoformat(timespec="seconds").replace("+00:00", "Z")
