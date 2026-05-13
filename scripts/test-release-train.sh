#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TMP_DIR="$(mktemp -d /tmp/1ctx-release-train-XXXXXX)"
trap 'rm -rf "$TMP_DIR"' EXIT
VERSION="$(tr -d '[:space:]' < "$ROOT/VERSION")"

bash -n \
  "$ROOT/scripts/check-release-manifest.sh" \
  "$ROOT/scripts/release-train.sh" \
  "$ROOT/scripts/redact-evidence.sh" \
  "$ROOT/scripts/audit-evidence-redaction.sh" \
  "$ROOT/scripts/write-runner-attestation.sh" \
  "$ROOT/scripts/package-macos-production-release.sh"
python3 -m py_compile "$ROOT/scripts/release-manifest.py"

"$ROOT/scripts/check-release-manifest.sh"
test "$("$ROOT/scripts/release-manifest.py" matrix-cases | wc -l | tr -d '[:space:]')" = "13"
"$ROOT/scripts/release-manifest.py" matrix-cases | grep -q "^login_restart_recovery$"

if "$ROOT/scripts/package-macos-production-release.sh" > "$TMP_DIR/package-guard.out" 2>&1; then
  echo "production package script should require release-train.sh." >&2
  exit 1
fi
grep -q "release-train.sh package" "$TMP_DIR/package-guard.out"

bad_manifest="$TMP_DIR/release-bad-version.toml"
cp "$ROOT/release/release.toml" "$bad_manifest"
python3 - "$bad_manifest" "$VERSION" <<'PY'
import sys
from pathlib import Path

path = Path(sys.argv[1])
version = sys.argv[2]
text = path.read_text(encoding="utf-8")
text = text.replace(f'version = "{version}"', 'version = "9.9.9"', 1)
path.write_text(text, encoding="utf-8")
PY
if "$ROOT/scripts/release-manifest.py" validate --manifest "$bad_manifest" > "$TMP_DIR/bad-version.out" 2>&1; then
  echo "release manifest validation should fail when manifest version drifts from VERSION." >&2
  exit 1
fi
grep -Eq "VERSION|tag" "$TMP_DIR/bad-version.out"

asset_dist="$TMP_DIR/asset-dist"
mkdir -p "$asset_dist"
printf 'versioned dmg\n' > "$asset_dist/1Context-$VERSION-macos-arm64.dmg"
printf 'versioned sha\n' > "$asset_dist/1Context-$VERSION-macos-arm64.dmg.sha256"
printf 'stable dmg\n' > "$asset_dist/1Context.dmg"
printf 'stable sha\n' > "$asset_dist/1Context.dmg.sha256"
printf '<rss />\n' > "$asset_dist/appcast.xml"
"$ROOT/scripts/release-manifest.py" write-asset-manifest \
  --dist-dir "$asset_dist" \
  --output "$TMP_DIR/asset-manifest-ok.json" > "$TMP_DIR/asset-manifest-ok.out"
if grep -Eq '"/|/Users/' "$TMP_DIR/asset-manifest-ok.json"; then
  echo "asset manifest should store only relative artifact paths." >&2
  exit 1
fi
grep -q "\"path\": \"dist/1Context-$VERSION-macos-arm64.dmg\"" "$TMP_DIR/asset-manifest-ok.json"

if "$ROOT/scripts/release-manifest.py" write-asset-manifest \
  --dist-dir "$TMP_DIR/missing-dist" \
  --output "$TMP_DIR/asset-manifest.json" > "$TMP_DIR/missing-assets.out" 2>&1; then
  echo "asset manifest generation should fail when release artifacts are missing." >&2
  exit 1
fi
grep -q "Missing release assets" "$TMP_DIR/missing-assets.out"

dirty_repo="$TMP_DIR/dirty-repo"
mkdir "$dirty_repo"
git -C "$dirty_repo" init -q
printf 'clean\n' > "$dirty_repo/tracked.txt"
git -C "$dirty_repo" add tracked.txt
git -C "$dirty_repo" -c user.name=Test -c user.email=test@example.com commit -qm init
printf 'dirty\n' > "$dirty_repo/untracked.txt"
if "$ROOT/scripts/release-manifest.py" check-clean-tree --root "$dirty_repo" > "$TMP_DIR/dirty-tree.out" 2>&1; then
  echo "clean-tree gate should fail on untracked release files." >&2
  exit 1
fi
grep -q "Release tree is dirty" "$TMP_DIR/dirty-tree.out"

helper_repo="$TMP_DIR/helper-repo"
mkdir -p "$helper_repo/scripts"
git -C "$helper_repo" init -q
cat > "$helper_repo/scripts/main.sh" <<'SH'
#!/usr/bin/env bash
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
source "$ROOT/scripts/helper.sh"
SH
chmod +x "$helper_repo/scripts/main.sh"
git -C "$helper_repo" add scripts/main.sh
git -C "$helper_repo" -c user.name=Test -c user.email=test@example.com commit -qm init
printf '# helper\n' > "$helper_repo/scripts/helper.sh"
if "$ROOT/scripts/release-manifest.py" check-sourced-helpers --root "$helper_repo" > "$TMP_DIR/helper.out" 2>&1; then
  echo "sourced-helper gate should fail when a sourced helper is untracked." >&2
  exit 1
fi
grep -q "not tracked by Git" "$TMP_DIR/helper.out"

evidence_dir="$TMP_DIR/evidence"
mkdir -p "$evidence_dir/proof-results"
cat > "$evidence_dir/raw.txt" <<'TXT'
/Users/paulhan/dev/1context-public-launch
GITHUB_TOKEN=redacted-by-test
Library/Application Support/1Context
TXT
cat > "$evidence_dir/proof-results/case.json" <<'JSON'
{
  "case": "already_current_manual_check",
  "expected_version": "__VERSION__",
  "actual_version": "__VERSION__",
  "status": "passed",
  "redaction_status": "pending"
}
JSON
python3 - "$evidence_dir/proof-results/case.json" "$VERSION" <<'PY'
import sys
from pathlib import Path

path = Path(sys.argv[1])
version = sys.argv[2]
path.write_text(path.read_text(encoding="utf-8").replace("__VERSION__", version), encoding="utf-8")
PY
"$ROOT/scripts/redact-evidence.sh" "$evidence_dir"
"$ROOT/scripts/audit-evidence-redaction.sh" "$evidence_dir" > "$TMP_DIR/redaction-audit.out"
if grep -q "/Users/paulhan" "$evidence_dir/raw.txt"; then
  echo "redaction script left a home path in evidence." >&2
  exit 1
fi
grep -q '"status": "passed"' "$evidence_dir/redaction-report.json"
grep -q '"redaction_status": "passed"' "$evidence_dir/proof-results/case.json"

echo "1Context release train checks passed."
