#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
VERSION="${ONECONTEXT_VERSION:-$(tr -d '[:space:]' < "$ROOT/VERSION")}"
ARCH="${ONECONTEXT_ARCH:-arm64}"
DMG="${1:-$ROOT/dist/1Context-$VERSION-macos-$ARCH.dmg}"
STAMP="$(date +%Y%m%d-%H%M%S)"
EVIDENCE_DIR="${ONECONTEXT_CLEAN_MACHINE_EVIDENCE_DIR:-$ROOT/dist/clean-machine-evidence/$STAMP}"

mkdir -p "$EVIDENCE_DIR"

{
  echo "1Context clean-machine acceptance"
  echo "date=$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo "host=$(scutil --get ComputerName 2>/dev/null || hostname)"
  echo "macos=$(sw_vers -productVersion 2>/dev/null || true)"
  echo "version=$VERSION"
  echo "dmg=$DMG"
} > "$EVIDENCE_DIR/environment.txt"

if [[ -f "$DMG" ]]; then
  shasum -a 256 "$DMG" > "$EVIDENCE_DIR/dmg.sha256"
  if command -v codesign >/dev/null 2>&1; then
    codesign --verify --strict "$DMG" >"$EVIDENCE_DIR/dmg-codesign.txt" 2>&1 || true
  fi
  if command -v xcrun >/dev/null 2>&1; then
    xcrun stapler validate "$DMG" >"$EVIDENCE_DIR/dmg-stapler.txt" 2>&1 || true
  fi
fi

cat > "$EVIDENCE_DIR/checklist.md" <<EOF
# 1Context Clean-Machine Acceptance

DMG: \`$DMG\`
Evidence: \`$EVIDENCE_DIR\`

## Steps

- [ ] Open the DMG.
- [ ] Launch \`1Context.app\` from the DMG.
- [ ] Accept "Install and Open" so the app moves itself to Applications.
- [ ] Confirm \`/Applications/1Context.app\` launches.
- [ ] Complete required setup for Local Wiki Access.
- [ ] Open \`https://wiki.1context.localhost/your-context\`.
- [ ] Quit and relaunch 1Context.
- [ ] Choose "Check for Updates" and verify Sparkle reports the feed state.
- [ ] Run app uninstall from Settings > Uninstall 1Context...
- [ ] Verify no 1Context LaunchAgents, privileged helper, or local trust entry remains.

## Useful Checks

\`\`\`bash
/Applications/1Context.app/Contents/MacOS/1context-cli diagnose --no-redact
/Applications/1Context.app/Contents/MacOS/1context-cli setup local-web status
launchctl print gui/\$(id -u)/com.haptica.1context 2>/dev/null || true
launchctl print gui/\$(id -u)/com.haptica.1context.menu 2>/dev/null || true
launchctl print system/com.haptica.1context.local-web-proxy 2>/dev/null || true
curl -k --resolve wiki.1context.localhost:443:127.0.0.1 -I https://wiki.1context.localhost/your-context
\`\`\`
EOF

echo "$EVIDENCE_DIR"
