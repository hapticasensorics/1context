#!/usr/bin/env python3
"""Write the portable freshness manifest for bundled RuntimeDefaults."""

from __future__ import annotations

import argparse
import hashlib
import json
from datetime import datetime, timezone
from pathlib import Path
from typing import Iterable


SCHEMA_VERSION = "1context.runtime-defaults-manifest.v1"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Write RuntimeDefaults manifest with release, hash, and render proof metadata."
    )
    parser.add_argument("--runtime-defaults-root", required=True, type=Path)
    parser.add_argument("--wiki-engine-root", required=True, type=Path)
    parser.add_argument("--render-result", required=True, type=Path)
    parser.add_argument("--version", required=True)
    parser.add_argument("--git-commit", default="unknown")
    parser.add_argument("--git-dirty", action="store_true")
    parser.add_argument("--output", required=True, type=Path)
    return parser.parse_args()


def portable_relative(root: Path, path: Path) -> str:
    return path.relative_to(root).as_posix()


def should_skip_runtime_defaults(rel: str) -> bool:
    return (
        rel == ".DS_Store"
        or rel.startswith("user-wiki/site/")
        or rel == ".1context/runtime-defaults-manifest.json"
    )


def should_skip_wiki_engine(rel: str) -> bool:
    return (
        rel == ".DS_Store"
        or rel.endswith(".pyc")
        or rel == "README.md"
        or rel == "package-lock.json"
        or rel == "node_modules/.package-lock.json"
        or "/__pycache__/" in f"/{rel}/"
        or rel.startswith("node_modules/.bin/")
        or (rel.startswith("node_modules/") and "/bin/" in rel)
    )


def iter_files(root: Path, skip) -> Iterable[Path]:
    for path in sorted(root.rglob("*")):
        if not path.is_file():
            continue
        rel = portable_relative(root, path)
        if skip(rel):
            continue
        yield path


def tree_hash(root: Path, skip) -> tuple[str, int, int]:
    digest = hashlib.sha256()
    file_count = 0
    total_bytes = 0
    for path in iter_files(root, skip):
        rel = portable_relative(root, path)
        data = path.read_bytes()
        digest.update(rel.encode("utf-8"))
        digest.update(b"\0")
        digest.update(data)
        digest.update(b"\0")
        file_count += 1
        total_bytes += len(data)
    return digest.hexdigest(), file_count, total_bytes


def file_identity(root: Path, path: Path) -> dict[str, object]:
    data = path.read_bytes()
    return {
        "path": portable_relative(root, path),
        "sha256": hashlib.sha256(data).hexdigest(),
        "bytes": len(data),
    }


def sanitized_render_result(path: Path) -> dict[str, object]:
    payload = json.loads(path.read_text(encoding="utf-8"))
    return {
        "schema_version": payload.get("schema_version"),
        "status": payload.get("status"),
        "route_count": payload.get("route_count"),
        "markdown_twin_count": payload.get("markdown_twin_count"),
        "source_input_count": payload.get("source_input_count"),
        "talk_input_count": payload.get("talk_input_count"),
        "route_manifest": payload.get("route_manifest"),
        "content_index": payload.get("content_index"),
    }


def main() -> None:
    args = parse_args()
    defaults_root = args.runtime_defaults_root.resolve()
    wiki_engine_root = args.wiki_engine_root.resolve()
    site_root = defaults_root / "user-wiki" / "site"

    source_hash, source_files, source_bytes = tree_hash(defaults_root, should_skip_runtime_defaults)
    site_hash, site_files, site_bytes = tree_hash(site_root, lambda _rel: False)
    renderer_hash, renderer_files, renderer_bytes = tree_hash(wiki_engine_root, should_skip_wiki_engine)
    materializer = file_identity(wiki_engine_root, wiki_engine_root / "tools" / "materialize-wiki-pages.py")
    renderer = file_identity(wiki_engine_root, wiki_engine_root / "tools" / "render-site.mjs")
    manifest_writer = file_identity(wiki_engine_root, Path(__file__).resolve())
    render_result = sanitized_render_result(args.render_result)

    manifest = {
        "schema_version": SCHEMA_VERSION,
        "release_version": args.version,
        "generated_at": datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
        "source_control": {
            "git_commit": args.git_commit,
            "git_dirty": bool(args.git_dirty),
        },
        "runtime_defaults": "app-bundle://RuntimeDefaults/1Context",
        "wiki_engine": "app-bundle://WikiEngine",
        "hashes": {
            "runtime_defaults_source": source_hash,
            "runtime_defaults_site": site_hash,
            "wiki_engine": renderer_hash,
            "materializer": materializer["sha256"],
            "renderer": renderer["sha256"],
            "manifest_writer": manifest_writer["sha256"],
        },
        "file_counts": {
            "runtime_defaults_source": source_files,
            "runtime_defaults_site": site_files,
            "wiki_engine": renderer_files,
        },
        "byte_counts": {
            "runtime_defaults_source": source_bytes,
            "runtime_defaults_site": site_bytes,
            "wiki_engine": renderer_bytes,
        },
        "tools": {
            "materializer": materializer,
            "renderer": renderer,
            "manifest_writer": manifest_writer,
        },
        "render_summary": {
            "status": render_result.get("status"),
            "route_count": render_result.get("route_count"),
            "markdown_twin_count": render_result.get("markdown_twin_count"),
            "source_input_count": render_result.get("source_input_count"),
            "talk_input_count": render_result.get("talk_input_count"),
        },
        "render_result": render_result,
    }

    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
