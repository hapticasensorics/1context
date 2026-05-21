#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
EXTENSION_DIR="$ROOT/browser-extension/extension"
HOST_SCRIPT="$ROOT/browser-extension/native-host/onecontext_browser_bridge.py"
HOST_NAME="com.haptica.onecontext_browser_bridge"
EXTENSION_ID="ijkabgddnhgkapedaloabgpcmpdhdhpb"
HOST_DIR="$HOME/Library/Application Support/Google/Chrome/NativeMessagingHosts"
HOST_MANIFEST="$HOST_DIR/$HOST_NAME.json"

fail() {
  echo "browser extension install failed: $*" >&2
  exit 1
}

[[ -f "$EXTENSION_DIR/manifest.json" ]] || fail "extension manifest missing: $EXTENSION_DIR/manifest.json"
[[ -f "$HOST_SCRIPT" ]] || fail "native host script missing: $HOST_SCRIPT"
python3 -m json.tool "$EXTENSION_DIR/manifest.json" >/dev/null
chmod +x "$HOST_SCRIPT"
mkdir -p "$HOST_DIR"

python3 - "$HOST_MANIFEST" "$HOST_NAME" "$HOST_SCRIPT" "$EXTENSION_ID" <<'PY'
import json
import sys
from pathlib import Path

manifest_path, host_name, host_script, extension_id = sys.argv[1:]
payload = {
    "name": host_name,
    "description": "1Context Browser Bridge native messaging host",
    "path": str(Path(host_script).resolve()),
    "type": "stdio",
    "allowed_origins": [f"chrome-extension://{extension_id}/"],
}
Path(manifest_path).write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n", encoding="utf-8")
PY

echo "Installed Chrome native messaging host:"
echo "  $HOST_MANIFEST"
echo
echo "Extension directory:"
echo "  $EXTENSION_DIR"
echo
echo "Extension id:"
echo "  $EXTENSION_ID"
echo
echo "Chrome is opening chrome://extensions. Enable Developer mode, click Load unpacked, and choose the extension directory above."
open -a "Google Chrome" "chrome://extensions/" >/dev/null 2>&1 || true
