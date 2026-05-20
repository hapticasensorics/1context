#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
VERSION="$(tr -d '[:space:]' < "$ROOT/VERSION")"
APP="${ONECONTEXT_PACKAGE_APP:-$ROOT/dist/1Context.app}"
EVIDENCE_DIR="${ONECONTEXT_PACKAGE_SMOKE_EVIDENCE_DIR:-/tmp}"

if [[ ! -d "$APP" ]]; then
  echo "Packaged app not found: $APP" >&2
  echo "Run ./scripts/release-train.sh build --channel dev first." >&2
  exit 1
fi
mkdir -p "$EVIDENCE_DIR"

INFO="$APP/Contents/Info.plist"
DAEMON_PLIST="$APP/Contents/Library/LaunchDaemons/com.haptica.1context.local-web-proxy.plist"
MEMORY_CORE="$APP/Contents/Resources/memory-core"
RUNTIME_DEFAULTS="$APP/Contents/Resources/RuntimeDefaults/1Context"
RUNTIME_DEFAULTS_MANIFEST="$RUNTIME_DEFAULTS/.1context/runtime-defaults-manifest.json"
WIKI_ENGINE="$APP/Contents/Resources/WikiEngine"

plutil -lint "$INFO" >/dev/null
plutil -lint "$DAEMON_PLIST" >/dev/null

test "$(plutil -extract CFBundleShortVersionString raw "$INFO")" = "$VERSION"
test "$(plutil -extract CFBundleIdentifier raw "$INFO")" = "com.haptica.1context"
if plutil -extract SUFeedURL raw "$INFO" >/dev/null 2>&1; then
  test "$(plutil -extract SUVerifyUpdateBeforeExtraction raw "$INFO")" = "true"
fi
test "$(plutil -extract Label raw "$DAEMON_PLIST")" = "com.haptica.1context.local-web-proxy"
test "$(plutil -extract BundleProgram raw "$DAEMON_PLIST")" = "Contents/Resources/1context-local-web-proxy"

for executable in \
  "$APP/Contents/MacOS/1Context" \
  "$APP/Contents/MacOS/1context-cli" \
  "$APP/Contents/MacOS/1contextd" \
  "$APP/Contents/MacOS/onecontext-wiki" \
  "$APP/Contents/Resources/1context-local-web-proxy" \
  "$APP/Contents/Resources/local-web/caddy/caddy"; do
  if [[ ! -x "$executable" ]]; then
    echo "Packaged executable is missing or not executable: $executable" >&2
    exit 1
  fi
done

if [[ -e "$MEMORY_CORE" ]]; then
  echo "Packaged app must not include the memory-core source checkout." >&2
  exit 1
fi
if [[ -e "$APP/Contents/Resources/memory-runtime" ]]; then
  echo "Packaged app must not include the retired memory-runtime artifact." >&2
  exit 1
fi
if [[ ! -f "$RUNTIME_DEFAULTS/user-wiki/wiki.toml" ]]; then
  echo "Packaged app must include user-wiki runtime defaults." >&2
  exit 1
fi
if [[ ! -f "$RUNTIME_DEFAULTS/user-wiki/site/.1context/route-manifest.json" ]]; then
  echo "Packaged runtime defaults must include a pre-rendered last-good wiki site." >&2
  exit 1
fi
if [[ ! -f "$RUNTIME_DEFAULTS/user-wiki/site/.1context/source-fingerprint.txt" ]]; then
  echo "Packaged runtime defaults must include the wiki-core source freshness marker." >&2
  exit 1
fi
if [[ ! -f "$RUNTIME_DEFAULTS/user-wiki/site/.1context/page-fingerprints.json" ]]; then
  echo "Packaged runtime defaults must include page freshness markers." >&2
  exit 1
fi
if [[ ! -f "$RUNTIME_DEFAULTS_MANIFEST" ]]; then
  echo "Packaged runtime defaults must include a freshness manifest." >&2
  exit 1
fi
RUNTIME_DEFAULTS_PUBLISH_STATUS="$("$APP/Contents/MacOS/onecontext-wiki" --root "$RUNTIME_DEFAULTS" publish-status)"
printf '%s\n' "$RUNTIME_DEFAULTS_PUBLISH_STATUS" >"$EVIDENCE_DIR/runtime-defaults-publish-status.json"
python3 - "$EVIDENCE_DIR/runtime-defaults-publish-status.json" <<'PY'
import json
import sys
from pathlib import Path

status = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
if status.get("operation") != "wiki.publish.status":
    raise SystemExit("packaged runtime defaults publish status must use wiki.publish.status")
if status.get("render_required") is not False:
    raise SystemExit("packaged runtime defaults must not require an immediate re-render")
if status.get("site_needs_publish") is not False:
    raise SystemExit("packaged runtime defaults site freshness marker is stale")
if status.get("next_action") != "none":
    raise SystemExit("packaged runtime defaults must be idle")
link_health = status.get("link_health") or {}
if link_health.get("status") != "ok" or int(link_health.get("broken_internal_count") or 0) != 0:
    raise SystemExit("packaged runtime defaults must have clean link health")
PY
python3 - "$RUNTIME_DEFAULTS_MANIFEST" "$VERSION" "$APP" "$RUNTIME_DEFAULTS" "$WIKI_ENGINE" "$ROOT/wiki-engine/tools/write-runtime-defaults-manifest.py" <<'PY'
import json
import hashlib
import re
import sys
from pathlib import Path

HEX64 = re.compile(r"[0-9a-f]{64}")
manifest = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
version = sys.argv[2]
app = Path(sys.argv[3])
runtime_defaults = Path(sys.argv[4])
wiki_engine = Path(sys.argv[5])
manifest_writer_source = Path(sys.argv[6])


def should_skip_runtime_defaults(rel):
    return (
        rel == ".DS_Store"
        or rel.startswith("user-wiki/site/")
        or rel == ".1context/runtime-defaults-manifest.json"
    )


def should_skip_wiki_engine(rel):
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


def iter_files(root, skip):
    for path in sorted(root.rglob("*")):
        if not path.is_file():
            continue
        rel = path.relative_to(root).as_posix()
        if skip(rel):
            continue
        yield rel, path


def tree_identity(root, skip):
    digest = hashlib.sha256()
    file_count = 0
    byte_count = 0
    for rel, path in iter_files(root, skip):
        data = path.read_bytes()
        digest.update(rel.encode("utf-8"))
        digest.update(b"\0")
        digest.update(data)
        digest.update(b"\0")
        file_count += 1
        byte_count += len(data)
    return digest.hexdigest(), file_count, byte_count


def file_identity(path):
    data = path.read_bytes()
    return hashlib.sha256(data).hexdigest(), len(data)

if manifest.get("schema_version") != "1context.runtime-defaults-manifest.v1":
    raise SystemExit("runtime defaults manifest schema mismatch")
if manifest.get("release_version") != version:
    raise SystemExit("runtime defaults manifest version mismatch")
source_control = manifest.get("source_control") or {}
git_commit = source_control.get("git_commit")
if git_commit != "unknown" and not (isinstance(git_commit, str) and re.fullmatch(r"[0-9a-f]{40}", git_commit)):
    raise SystemExit("runtime defaults manifest must record a git commit or unknown")
if not isinstance(source_control.get("git_dirty"), bool):
    raise SystemExit("runtime defaults manifest must record git_dirty as a boolean")
if manifest.get("runtime_defaults") != "app-bundle://RuntimeDefaults/1Context":
    raise SystemExit("runtime defaults manifest must use portable defaults identity")
if manifest.get("wiki_engine") != "app-bundle://WikiEngine":
    raise SystemExit("runtime defaults manifest must use portable renderer identity")
hashes = manifest.get("hashes") or {}
for key in ["runtime_defaults_source", "runtime_defaults_site", "wiki_engine", "wiki_core", "renderer", "manifest_writer"]:
    value = hashes.get(key)
    if not isinstance(value, str) or not HEX64.fullmatch(value):
        raise SystemExit(f"runtime defaults manifest has invalid hash: {key}")
tools = manifest.get("tools") or {}
for key, expected_path in [
    ("wiki_core", "Contents/MacOS/onecontext-wiki"),
    ("renderer", "tools/render-site.mjs"),
    ("manifest_writer", "wiki-engine/tools/write-runtime-defaults-manifest.py"),
]:
    tool = tools.get(key) or {}
    if tool.get("path") != expected_path:
        raise SystemExit(f"runtime defaults manifest has invalid tool path: {key}")
    if tool.get("sha256") != hashes.get(key):
        raise SystemExit(f"runtime defaults manifest tool hash mismatch: {key}")
    if int(tool.get("bytes") or 0) <= 0:
        raise SystemExit(f"runtime defaults manifest has invalid tool byte count: {key}")
summary = manifest.get("render_summary") or {}
render = manifest.get("render_result") or {}
if summary.get("status") != "published" or render.get("status") != "published":
    raise SystemExit("runtime defaults manifest must record a successful render")
if summary.get("route_count") != render.get("route_count"):
    raise SystemExit("runtime defaults manifest render summary route count drifted")
if int(summary.get("route_count") or 0) < 5:
    raise SystemExit("runtime defaults manifest route count is too small")
if int(summary.get("markdown_twin_count") or 0) < 5:
    raise SystemExit("runtime defaults manifest markdown twin count is too small")
expected_trees = {
    "runtime_defaults_source": tree_identity(runtime_defaults, should_skip_runtime_defaults),
    "runtime_defaults_site": tree_identity(runtime_defaults / "user-wiki" / "site", lambda _rel: False),
    "wiki_engine": tree_identity(wiki_engine, should_skip_wiki_engine),
}
file_counts = manifest.get("file_counts") or {}
byte_counts = manifest.get("byte_counts") or {}
for key, (actual_hash, actual_files, actual_bytes) in expected_trees.items():
    if hashes.get(key) != actual_hash:
        raise SystemExit(f"runtime defaults manifest hash does not match packaged payload: {key}")
    if file_counts.get(key) != actual_files:
        raise SystemExit(f"runtime defaults manifest file count does not match packaged payload: {key}")
    if byte_counts.get(key) != actual_bytes:
        raise SystemExit(f"runtime defaults manifest byte count does not match packaged payload: {key}")
expected_files = {
    "wiki_core": app / "Contents" / "MacOS" / "onecontext-wiki",
    "renderer": wiki_engine / "tools" / "render-site.mjs",
    "manifest_writer": manifest_writer_source,
}
for key, path in expected_files.items():
    actual_hash, actual_bytes = file_identity(path)
    tool = tools.get(key) or {}
    if tool.get("sha256") != actual_hash:
        raise SystemExit(f"runtime defaults manifest tool hash does not match packaged/build payload: {key}")
    if tool.get("bytes") != actual_bytes:
        raise SystemExit(f"runtime defaults manifest tool byte count does not match packaged/build payload: {key}")
PY
if [[ ! -f "$WIKI_ENGINE/tools/render-site.mjs" ]]; then
  echo "Packaged app must include the first-class wiki renderer source." >&2
  exit 1
fi
if [[ ! -d "$WIKI_ENGINE/node_modules/gray-matter" || ! -d "$WIKI_ENGINE/node_modules/marked" ]]; then
  echo "Packaged wiki renderer must include vendored production dependencies." >&2
  exit 1
fi
if [[ -e "$WIKI_ENGINE/package-lock.json" || -e "$WIKI_ENGINE/node_modules/.bin" || -e "$WIKI_ENGINE/node_modules/.package-lock.json" ]]; then
  echo "Packaged wiki renderer must not include package locks or executable npm bin shims." >&2
  exit 1
fi
if find "$WIKI_ENGINE/node_modules" -path '*/bin/*' -print -quit | grep -q .; then
  echo "Packaged wiki renderer must not include executable npm package bin directories." >&2
  exit 1
fi
if find "$WIKI_ENGINE" \( -name '__pycache__' -o -name '*.pyc' \) -print -quit | grep -q .; then
  echo "Packaged wiki renderer must not include Python bytecode caches." >&2
  exit 1
fi
if find "$APP/Contents/Resources" -path '*/runtime-test/*' -print -quit | grep -q .; then
  echo "Packaged app must not include generated runtime-test state." >&2
  exit 1
fi
if find "$APP/Contents/Resources" -path '*/generated/*' -print -quit | grep -q .; then
  echo "Packaged app must not include generated wiki source output." >&2
  exit 1
fi
if find "$APP/Contents/Resources" -path '*/context-engine/observations/*' -type f -print -quit | grep -q .; then
  echo "Packaged app must not include raw observations." >&2
  exit 1
fi
if find "$APP/Contents/Resources" -path '*/context-engine/runs/*' -type f -print -quit | grep -q .; then
  echo "Packaged app must not include run transcripts." >&2
  exit 1
fi
if find "$APP/Contents/Resources" -path '*/context-engine/artifacts/wiki/previews/*' -type f -print -quit | grep -q .; then
  echo "Packaged app must not include private preview artifacts." >&2
  exit 1
fi
if find "$APP/Contents/Resources" -type f -print0 \
  | xargs -0 grep -I -n -E '/api/wiki/chat|chat_available|ai-provider|Chat about this page' >"$EVIDENCE_DIR/1context-package-chat-surface.txt"; then
  echo "Packaged resources must not include dead chat/provider surfaces. Matches:" >&2
  cat "$EVIDENCE_DIR/1context-package-chat-surface.txt" >&2
  exit 1
fi
if find "$APP/Contents/Resources" -type f -print0 \
  | xargs -0 grep -I -n -E '/Users/paulhan|paulhan|/dev/1context|runtime-test|1context-private|(^|[^[:alnum:]_])/goal([^[:alnum:]_]|$)|goal\\.html|goal\\.md' >"$EVIDENCE_DIR/1context-package-local-paths.txt"; then
  echo "Packaged resources must not include local developer paths, private fixtures, runtime-test, or development goal routes. Matches:" >&2
  cat "$EVIDENCE_DIR/1context-package-local-paths.txt" >&2
  exit 1
fi
if grep -R -a -n -E '/opt/homebrew|/usr/local/Cellar|/Cellar/caddy' "$APP" >"$EVIDENCE_DIR/1context-package-homebrew-paths.txt"; then
  echo "Packaged app must not include Homebrew or host Caddy paths. Matches:" >&2
  cat "$EVIDENCE_DIR/1context-package-homebrew-paths.txt" >&2
  exit 1
fi
"$ROOT/macos/tools/audit-app-dependencies.sh" "$APP"

echo "Packaged LaunchAgent smoke passed."
