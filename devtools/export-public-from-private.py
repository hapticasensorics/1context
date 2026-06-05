#!/usr/bin/env python3
from __future__ import annotations

import argparse
import fnmatch
import io
import os
import shutil
import subprocess
import sys
import tarfile
import tempfile
from pathlib import Path

try:
    import tomllib
except ModuleNotFoundError:
    print("Python 3.11+ is required for tomllib. Run with `uv run python`.", file=sys.stderr)
    sys.exit(2)


def run(args: list[str], cwd: Path | None = None, input_bytes: bytes | None = None) -> subprocess.CompletedProcess[bytes]:
    return subprocess.run(args, cwd=cwd, input=input_bytes, stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=True)


def git_lines(repo: Path, args: list[str]) -> list[str]:
    result = run(["git", *args], cwd=repo)
    text = result.stdout.decode()
    return [line for line in text.splitlines() if line]


def git_bytes(repo: Path, args: list[str]) -> bytes:
    return run(["git", *args], cwd=repo).stdout


def repo_root(path: Path) -> Path:
    return Path(git_bytes(path, ["rev-parse", "--show-toplevel"]).decode().strip())


def load_manifest(path: Path) -> dict:
    with path.open("rb") as fh:
        manifest = tomllib.load(fh)
    if "source" not in manifest or "export" not in manifest:
        raise SystemExit("manifest must contain [source] and [export] sections")
    return manifest


def resolve_source_repo(manifest_path: Path, raw: str) -> Path:
    path = Path(raw).expanduser()
    if not path.is_absolute():
        path = (manifest_path.parent / path).resolve()
    return repo_root(path)


def normalize_rel_path(raw: str) -> str:
    if not raw or raw.startswith("/"):
        raise SystemExit(f"invalid export path: {raw!r}")
    path = Path(raw)
    if any(part == ".." for part in path.parts):
        raise SystemExit(f"export path cannot escape repo root: {raw!r}")
    return path.as_posix().rstrip("/")


def normalize_patterns(raw_patterns: list[str]) -> list[str]:
    return [normalize_rel_path(pattern) for pattern in raw_patterns]


def path_matches(path: str, pattern: str) -> bool:
    if pattern.endswith("/**"):
        base = pattern[:-3].rstrip("/")
        return path == base or path.startswith(f"{base}/")
    return fnmatch.fnmatch(path, pattern)


def any_match(path: str, patterns: list[str]) -> bool:
    return any(path_matches(path, pattern) for pattern in patterns)


def list_source_files(source_repo: Path, source_ref: str, allow: list[str]) -> set[str]:
    return set(git_lines(source_repo, ["ls-tree", "-r", "--name-only", source_ref, "--", *allow]))


def list_destination_files(dest_repo: Path, allow: list[str]) -> set[str]:
    return set(git_lines(dest_repo, ["ls-files", "--", *allow]))


def source_blob(source_repo: Path, source_ref: str, rel_path: str) -> bytes:
    return git_bytes(source_repo, ["show", f"{source_ref}:{rel_path}"])


def destination_blob(dest_repo: Path, rel_path: str) -> bytes:
    return (dest_repo / rel_path).read_bytes()


def blocked_paths(paths: set[str], deny: list[str]) -> list[str]:
    return sorted(path for path in paths if any_match(path, deny))


def planned_changes(source_repo: Path, source_ref: str, dest_repo: Path, allow: list[str], deny: list[str]) -> list[tuple[str, str]]:
    source_files = list_source_files(source_repo, source_ref, allow)
    blocked = blocked_paths(source_files, deny)
    if blocked:
        sample = "\n".join(f"  {path}" for path in blocked[:40])
        raise SystemExit(f"source ref contains denied public-export paths:\n{sample}")

    dest_files = list_destination_files(dest_repo, allow)
    changes: list[tuple[str, str]] = []
    for rel_path in sorted(source_files - dest_files):
        changes.append(("A", rel_path))
    for rel_path in sorted(dest_files - source_files):
        changes.append(("D", rel_path))
    for rel_path in sorted(source_files & dest_files):
        if source_blob(source_repo, source_ref, rel_path) != destination_blob(dest_repo, rel_path):
            changes.append(("M", rel_path))
    return changes


def ensure_clean_destination(dest_repo: Path) -> None:
    status = git_bytes(dest_repo, ["status", "--short"]).decode().strip()
    if status:
        raise SystemExit("destination working tree must be clean before applying an export")


def extract_source(source_repo: Path, source_ref: str, allow: list[str], target: Path) -> None:
    archive = git_bytes(source_repo, ["archive", "--format=tar", source_ref, "--", *allow])
    with tarfile.open(fileobj=io.BytesIO(archive), mode="r:") as tar:
        tar.extractall(target)


def remove_destination_path(dest_repo: Path, rel_path: str) -> None:
    target = dest_repo / rel_path
    if not target.exists() and not target.is_symlink():
        return
    if target.is_dir() and not target.is_symlink():
        shutil.rmtree(target)
    else:
        target.unlink()


def copy_source_path(source_root: Path, dest_repo: Path, rel_path: str) -> None:
    source = source_root / rel_path
    if not source.exists() and not source.is_symlink():
        return
    target = dest_repo / rel_path
    target.parent.mkdir(parents=True, exist_ok=True)
    if source.is_dir() and not source.is_symlink():
        shutil.copytree(source, target, symlinks=True)
    else:
        shutil.copy2(source, target, follow_symlinks=False)


def apply_export(source_repo: Path, source_ref: str, dest_repo: Path, allow: list[str], deny: list[str]) -> None:
    source_files = list_source_files(source_repo, source_ref, allow)
    blocked = blocked_paths(source_files, deny)
    if blocked:
        sample = "\n".join(f"  {path}" for path in blocked[:40])
        raise SystemExit(f"source ref contains denied public-export paths:\n{sample}")

    with tempfile.TemporaryDirectory(prefix="1context-public-export-") as temp_name:
        temp_root = Path(temp_name)
        extract_source(source_repo, source_ref, allow, temp_root)
        for rel_path in allow:
            remove_destination_path(dest_repo, rel_path)
        for rel_path in allow:
            copy_source_path(temp_root, dest_repo, rel_path)


def print_change_summary(changes: list[tuple[str, str]], max_changes: int) -> None:
    print(f"planned changes: {len(changes)}")
    counts: dict[str, int] = {}
    for status, _ in changes:
        counts[status] = counts.get(status, 0) + 1
    for status in ("A", "M", "D"):
        if status in counts:
            print(f"  {status}: {counts[status]}")
    for status, rel_path in changes[:max_changes]:
        print(f"{status}\t{rel_path}")
    if len(changes) > max_changes:
        print(f"... {len(changes) - max_changes} more")


def main() -> int:
    parser = argparse.ArgumentParser(description="Export the public 1Context tree from the private source repo.")
    parser.add_argument("--manifest", default="public-export.toml")
    parser.add_argument("--source-repo", default=os.environ.get("ONECONTEXT_PRIVATE_REPO"))
    parser.add_argument("--source-ref", default=os.environ.get("ONECONTEXT_PUBLIC_SOURCE_REF"))
    parser.add_argument("--apply", action="store_true", help="rewrite allowlisted public paths from the source ref")
    parser.add_argument("--allow-dirty", action="store_true", help="allow applying into a dirty destination tree")
    parser.add_argument("--max-changes", type=int, default=120)
    args = parser.parse_args()

    dest_repo = repo_root(Path.cwd())
    manifest_path = (dest_repo / args.manifest).resolve()
    manifest = load_manifest(manifest_path)

    source_repo = resolve_source_repo(manifest_path, args.source_repo or manifest["source"]["repo"])
    source_ref = args.source_ref or manifest["source"]["ref"]
    allow = normalize_patterns(manifest["export"].get("allow", []))
    deny = normalize_patterns(manifest["export"].get("deny", []))
    if not allow:
        raise SystemExit("manifest export.allow cannot be empty")

    git_bytes(source_repo, ["rev-parse", "--verify", f"{source_ref}^{{commit}}"])
    changes = planned_changes(source_repo, source_ref, dest_repo, allow, deny)
    print(f"source repo: {source_repo}")
    print(f"source ref:  {source_ref}")
    print(f"dest repo:   {dest_repo}")
    print_change_summary(changes, args.max_changes)

    if not args.apply:
        print("dry run only; pass --apply to rewrite allowlisted paths")
        return 0

    if not args.allow_dirty:
        ensure_clean_destination(dest_repo)
    apply_export(source_repo, source_ref, dest_repo, allow, deny)
    print("export applied; inspect `git status --short`")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
