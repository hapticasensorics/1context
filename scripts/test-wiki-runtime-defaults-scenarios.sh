#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APP="${ONECONTEXT_PACKAGE_APP:-$ROOT/dist/1Context.app}"
DEFAULTS_DIR="$APP/Contents/Resources/RuntimeDefaults"
SCENARIO_ROOT="${ONECONTEXT_RUNTIME_DEFAULTS_SCENARIO_ROOT:-$ROOT/runtime-test/wiki-runtime-defaults-scenarios}"
EVIDENCE_DIR="${ONECONTEXT_RUNTIME_DEFAULTS_EVIDENCE_DIR:-/tmp/1ctx-runtime-defaults-scenarios}"
BUILD_EVIDENCE_DIR="${ONECONTEXT_RELEASE_EVIDENCE_DIR:-$EVIDENCE_DIR/dev-build}"
TEST_LOG="$EVIDENCE_DIR/swift-runtime-defaults-scenarios.log"
SUMMARY_JSON="$EVIDENCE_DIR/runtime-defaults-scenarios-summary.json"

mkdir -p "$EVIDENCE_DIR"

if [[ "${ONECONTEXT_BUILD_DEV_APP:-0}" == "1" || ! -d "$APP" ]]; then
  ONECONTEXT_RELEASE_EVIDENCE_DIR="$BUILD_EVIDENCE_DIR" "$ROOT/scripts/release-train.sh" build --channel dev
fi

if [[ ! -d "$APP" ]]; then
  echo "Missing app package: $APP" >&2
  exit 1
fi

if [[ ! -f "$DEFAULTS_DIR/1Context/.1context/runtime-defaults-manifest.json" ]]; then
  echo "Missing packaged RuntimeDefaults manifest under: $DEFAULTS_DIR" >&2
  exit 1
fi

if [[ ! -d "$ROOT/wiki-engine/node_modules" ]]; then
  npm ci --prefix "$ROOT/wiki-engine" >/dev/null
fi

ONECONTEXT_RUNTIME_DEFAULTS_SCENARIOS=1 \
ONECONTEXT_RUNTIME_DEFAULTS_SCENARIO_ROOT="$SCENARIO_ROOT" \
ONECONTEXT_RUNTIME_DEFAULTS_DIR="$DEFAULTS_DIR" \
ONECONTEXT_WIKI_ENGINE_DIR="$ROOT/wiki-engine" \
  swift test --package-path "$ROOT/macos" --filter WikiRuntimeDefaultsScenarioTests 2>&1 \
  | tee "$TEST_LOG"

python3 - "$SCENARIO_ROOT" "$SUMMARY_JSON" <<'PY'
import json
import sys
from pathlib import Path

root = Path(sys.argv[1])
summary_path = Path(sys.argv[2])
required = {
    "fresh-user": {
        "ledger_status": "installed",
        "routes": ["/for-you", "/for-you/talk", "/topics", "/topics/talk"],
    },
    "preserve-user-edit": {
        "ledger_status": "installed_with_conflicts",
        "routes": ["/for-you", "/your-context", "/projects", "/topics"],
        "proposal": "1Context/context-engine/proposals/wiki/runtime-defaults/user-wiki__wiki.toml.proposal.json",
    },
    "custom-page": {
        "ledger_status": "installed",
        "routes": ["/dummy-custom", "/dummy-custom/talk"],
    },
}
summary = {"scenario_root": str(root), "scenarios": {}}
for name, expectations in required.items():
    home = root / name
    ledger_path = home / "Library/Application Support/1Context/setup/runtime-defaults-install.json"
    route_manifest_path = home / "1Context/user-wiki/site/.1context/route-manifest.json"
    current_manifest_path = home / "Library/Application Support/1Context/wiki-site/current/.1context/route-manifest.json"
    ledger = json.loads(ledger_path.read_text(encoding="utf-8"))
    manifest = json.loads(route_manifest_path.read_text(encoding="utf-8"))
    current_manifest = json.loads(current_manifest_path.read_text(encoding="utf-8"))
    routes = {entry["route"] for entry in manifest["routes"]}
    current_routes = {entry["route"] for entry in current_manifest["routes"]}
    if ledger.get("status") != expectations["ledger_status"]:
        raise SystemExit(f"{name}: expected ledger {expectations['ledger_status']}, got {ledger.get('status')}")
    for route in expectations["routes"]:
        if route not in routes:
            raise SystemExit(f"{name}: source site missing route {route}")
        if route not in current_routes:
            raise SystemExit(f"{name}: published site missing route {route}")
    proposal = expectations.get("proposal")
    if proposal and not (home / proposal).exists():
        raise SystemExit(f"{name}: missing proposal {proposal}")
    packaged = ledger.get("packagedManifest") or {}
    for key in ["releaseVersion", "gitCommit", "runtimeDefaultsSourceHash", "materializerHash", "rendererHash"]:
        if not packaged.get(key):
            raise SystemExit(f"{name}: ledger packagedManifest missing {key}")
    summary["scenarios"][name] = {
        "ledger_status": ledger["status"],
        "release_version": packaged["releaseVersion"],
        "git_commit": packaged["gitCommit"],
        "render_status": packaged.get("renderStatus"),
        "route_count": len(routes),
        "published_route_count": len(current_routes),
    }

summary_path.write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n", encoding="utf-8")
print(f"runtime_defaults_scenario_summary={summary_path}")
PY

echo "runtime_defaults_scenario_root=$SCENARIO_ROOT"
echo "runtime_defaults_scenario_log=$TEST_LOG"
