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
LEGACY_TALK_PROOF_JSON="$EVIDENCE_DIR/legacy-talk-alias-proof.json"

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

python3 - "$SCENARIO_ROOT" "$SUMMARY_JSON" "$DEFAULTS_DIR" <<'PY'
import json
import sys
from pathlib import Path

root = Path(sys.argv[1])
summary_path = Path(sys.argv[2])
defaults_dir = Path(sys.argv[3])
bundle_manifest_path = defaults_dir / "1Context/.1context/runtime-defaults-manifest.json"
bundle_manifest = json.loads(bundle_manifest_path.read_text(encoding="utf-8"))
required = {
    "fresh-user": {
        "ledger_status": "installed",
        "routes": ["/for-you", "/for-you/talk", "/topics", "/topics/talk"],
        "trigger": "runtime-test.fresh",
    },
    "preserve-user-edit": {
        "ledger_status": "installed_with_conflicts",
        "routes": ["/for-you", "/your-context", "/projects", "/topics"],
        "proposal": "1Context/context-engine/proposals/wiki/runtime-defaults/user-wiki__wiki.toml.proposal.json",
        "trigger": "runtime-test.preserve",
    },
    "custom-page": {
        "ledger_status": "installed",
        "routes": ["/dummy-custom", "/dummy-custom/talk"],
        "trigger": "runtime-test.custom-page",
    },
    "app-upgrade-user": {
        "ledger_status": "installed_with_conflicts",
        "routes": ["/dummy-custom", "/dummy-custom/talk", "/for-you", "/topics"],
        "trigger": "runtime-test.app-upgrade",
        "copied": ["context-engine/prompts/e08-for-you/hourly-answerer.md"],
        "proposals": [
            "1Context/context-engine/proposals/wiki/runtime-defaults/user-wiki__.1context__page-ledger.jsonl.proposal.json",
            "1Context/context-engine/proposals/wiki/runtime-defaults/user-wiki__templates__pages__context-page.md.proposal.json",
            "1Context/context-engine/proposals/wiki/runtime-defaults/user-wiki__wiki.toml.proposal.json",
        ],
    },
}
summary = {"scenario_root": str(root), "scenarios": {}}
for name, expectations in required.items():
    home = root / name
    ledger_path = home / "Library/Application Support/1Context/setup/runtime-defaults-install.json"
    route_manifest_path = home / "1Context/user-wiki/site/.1context/route-manifest.json"
    current_manifest_path = home / "Library/Application Support/1Context/wiki-site/current/.1context/route-manifest.json"
    current_render_path = home / "Library/Application Support/1Context/wiki-site/current/.1context/current-render.json"
    ledger = json.loads(ledger_path.read_text(encoding="utf-8"))
    manifest = json.loads(route_manifest_path.read_text(encoding="utf-8"))
    current_manifest = json.loads(current_manifest_path.read_text(encoding="utf-8"))
    current_render = json.loads(current_render_path.read_text(encoding="utf-8"))
    routes = {entry["route"] for entry in manifest["routes"]}
    current_routes = {entry["route"] for entry in current_manifest["routes"]}
    if ledger.get("status") != expectations["ledger_status"]:
        raise SystemExit(f"{name}: expected ledger {expectations['ledger_status']}, got {ledger.get('status')}")
    if current_routes != routes:
        missing = sorted(routes - current_routes)
        extra = sorted(current_routes - routes)
        raise SystemExit(f"{name}: app-support mirror route mismatch missing={missing} extra={extra}")
    if current_render.get("status") != "published":
        raise SystemExit(f"{name}: expected current render published, got {current_render.get('status')}")
    if current_render.get("trigger") != expectations["trigger"]:
        raise SystemExit(f"{name}: expected trigger {expectations['trigger']}, got {current_render.get('trigger')}")
    for route in expectations["routes"]:
        if route not in routes:
            raise SystemExit(f"{name}: source site missing route {route}")
        if route not in current_routes:
            raise SystemExit(f"{name}: published site missing route {route}")
    proposal = expectations.get("proposal")
    if proposal and not (home / proposal).exists():
        raise SystemExit(f"{name}: missing proposal {proposal}")
    for copied in expectations.get("copied", []):
        if copied not in ledger.get("copied", []):
            raise SystemExit(f"{name}: missing copied ledger entry {copied}")
    for proposal in expectations.get("proposals", []):
        if proposal not in ledger.get("proposals", []):
            raise SystemExit(f"{name}: missing proposal ledger entry {proposal}")
        if not (home / proposal).exists():
            raise SystemExit(f"{name}: missing proposal file {proposal}")
    packaged = ledger.get("packagedManifest") or {}
    for key in ["releaseVersion", "gitCommit", "runtimeDefaultsSourceHash", "wikiCoreHash", "rendererHash"]:
        if not packaged.get(key):
            raise SystemExit(f"{name}: ledger packagedManifest missing {key}")
    bundle_hashes = bundle_manifest.get("hashes") or {}
    freshness = {
        "releaseVersion": bundle_manifest.get("release_version"),
        "gitCommit": (bundle_manifest.get("source_control") or {}).get("git_commit"),
        "runtimeDefaultsSourceHash": bundle_hashes.get("runtime_defaults_source"),
        "wikiCoreHash": bundle_hashes.get("wiki_core"),
        "rendererHash": bundle_hashes.get("renderer"),
    }
    for key, expected in freshness.items():
        if packaged.get(key) != expected:
            raise SystemExit(f"{name}: stale packagedManifest {key}: expected {expected}, got {packaged.get(key)}")
    summary["scenarios"][name] = {
        "ledger_status": ledger["status"],
        "release_version": packaged["releaseVersion"],
        "git_commit": packaged["gitCommit"],
        "bundle_render_status": packaged.get("renderStatus"),
        "current_render_status": current_render["status"],
        "current_render_trigger": current_render["trigger"],
        "route_count": len(routes),
        "published_route_count": len(current_routes),
        "copied_count": len(ledger.get("copied", [])),
        "proposal_count": len(ledger.get("proposals", [])),
    }

summary_path.write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n", encoding="utf-8")
print(f"runtime_defaults_scenario_summary={summary_path}")
PY

python3 - "$ROOT" "$SCENARIO_ROOT" "$LEGACY_TALK_PROOF_JSON" "$DEFAULTS_DIR" <<'PY'
import hashlib
import json
import os
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request
from pathlib import Path

repo = Path(sys.argv[1])
scenario_root = Path(sys.argv[2])
proof_path = Path(sys.argv[3])
defaults_dir = Path(sys.argv[4])
serve_site = repo / "wiki-engine/tools/serve-site.mjs"

targets = {
    "packaged-defaults": {
        "site": defaults_dir / "1Context/user-wiki/site",
        "slugs": ["for-you", "topics"],
    },
    "fresh-user-current": {
        "site": scenario_root / "fresh-user/Library/Application Support/1Context/wiki-site/current",
        "slugs": ["for-you", "topics"],
    },
    "preserve-user-edit-current": {
        "site": scenario_root / "preserve-user-edit/Library/Application Support/1Context/wiki-site/current",
        "slugs": ["for-you", "your-context", "projects", "topics"],
    },
    "custom-page-current": {
        "site": scenario_root / "custom-page/Library/Application Support/1Context/wiki-site/current",
        "slugs": ["dummy-custom"],
    },
    "app-upgrade-user-current": {
        "site": scenario_root / "app-upgrade-user/Library/Application Support/1Context/wiki-site/current",
        "slugs": ["dummy-custom", "for-you", "topics"],
    },
}


def wait_for_port(port_file, proc):
    deadline = time.time() + 10
    while time.time() < deadline:
        if proc.poll() is not None:
            raise SystemExit(
                f"serve-site exited early for {port_file}: stdout={proc.stdout.read() if proc.stdout else ''}"
            )
        if port_file.exists():
            return port_file.read_text(encoding="utf-8").strip()
        time.sleep(0.05)
    raise SystemExit(f"Timed out waiting for serve-site port file: {port_file}")


def fetch(url):
    request = urllib.request.Request(url, headers={"User-Agent": "1context-runtime-defaults-scenario"})
    try:
        with urllib.request.urlopen(request, timeout=10) as response:
            body = response.read().decode("utf-8", errors="replace")
            return {
                "status": response.status,
                "content_type": response.headers.get("content-type", ""),
                "body_sha256": hashlib.sha256(body.encode("utf-8")).hexdigest(),
                "body_sample": body[:160],
            }
    except urllib.error.HTTPError as error:
        body = error.read().decode("utf-8", errors="replace")
        return {
            "status": error.code,
            "content_type": error.headers.get("content-type", ""),
            "body_sample": body[:160],
        }


proof = {"targets": {}}
for name, target in targets.items():
    site = target["site"]
    if not site.is_dir():
        raise SystemExit(f"{name}: missing site directory {site}")

    with tempfile.TemporaryDirectory(prefix=f"1ctx-{name}-") as tmp:
        port_file = Path(tmp) / "port.txt"
        env = os.environ.copy()
        env.update({"HOST": "127.0.0.1", "PORT": "0", "PORT_FILE": str(port_file)})
        proc = subprocess.Popen(
            ["node", str(serve_site), str(site)],
            cwd=str(repo),
            env=env,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )
        try:
            port = wait_for_port(port_file, proc)
            base = f"http://127.0.0.1:{port}"
            target_result = {"site": str(site), "checks": []}
            for slug in target["slugs"]:
                canonical_path = f"/{slug}/talk"
                canonical = fetch(base + canonical_path)
                if canonical["status"] != 200 or not canonical["content_type"].startswith("text/html"):
                    raise SystemExit(f"{name}: canonical talk route failed {canonical_path}: {canonical}")
                for suffix_path in [f"/{slug}.talk", f"/{slug}.talk/"]:
                    legacy = fetch(base + suffix_path)
                    if legacy["status"] != 200 or not legacy["content_type"].startswith("text/html"):
                        raise SystemExit(f"{name}: legacy talk alias failed {suffix_path}: {legacy}")
                    target_result["checks"].append(
                        {
                            "canonical": canonical_path,
                            "legacy": suffix_path,
                            "canonical_status": canonical["status"],
                            "canonical_content_type": canonical["content_type"],
                            "legacy_status": legacy["status"],
                            "legacy_content_type": legacy["content_type"],
                            "same_body_hash_as_canonical": legacy.get("body_sha256") == canonical.get("body_sha256"),
                        }
                    )
            proof["targets"][name] = target_result
        finally:
            proc.terminate()
            try:
                proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                proc.kill()
                proc.wait(timeout=5)

proof_path.write_text(json.dumps(proof, indent=2, sort_keys=True) + "\n", encoding="utf-8")
print(f"legacy_talk_alias_proof={proof_path}")
PY

echo "runtime_defaults_scenario_root=$SCENARIO_ROOT"
echo "runtime_defaults_scenario_log=$TEST_LOG"
echo "legacy_talk_alias_proof=$LEGACY_TALK_PROOF_JSON"
