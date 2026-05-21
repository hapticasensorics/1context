#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
HOST_SCRIPT="$ROOT/browser-extension/native-host/onecontext_browser_bridge.py"
EXTENSION_ID="ijkabgddnhgkapedaloabgpcmpdhdhpb"

python3 - "$HOST_SCRIPT" "$EXTENSION_ID" <<'PY'
import json
import struct
import subprocess
import sys

host_script, extension_id = sys.argv[1:]
payload = {
    "type": "ONECONTEXT_BROWSER_EXTENSION_PROOF",
    "userNote": "native host smoke test",
    "extension": {
        "id": extension_id,
        "name": "1Context Browser Bridge",
        "version": "0.1.0",
        "permissions": ["activeTab", "nativeMessaging", "scripting", "storage", "tabs"],
        "hostPermissions": ["<all_urls>"],
    },
    "tab": {
        "id": 1,
        "windowId": 1,
        "url": "https://example.test/",
        "title": "Example",
    },
    "pageContext": {
        "url": "https://example.test/",
        "title": "Example",
        "selectedText": "",
        "visibleText": "Example page",
        "domExcerpt": "<!doctype html><title>Example</title>",
        "viewport": {"width": 800, "height": 600, "devicePixelRatio": 2, "scrollX": 0, "scrollY": 0},
        "timestamp": "2026-05-21T00:00:00Z",
    },
}
encoded = json.dumps(payload).encode("utf-8")
request = struct.pack("@I", len(encoded)) + encoded
proc = subprocess.run([host_script], input=request, stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=True)
if len(proc.stdout) < 4:
    raise SystemExit("native host returned no framed response")
length = struct.unpack("@I", proc.stdout[:4])[0]
response = json.loads(proc.stdout[4:4 + length].decode("utf-8"))
print(json.dumps(response, indent=2, sort_keys=True))
if not response.get("ok"):
    raise SystemExit(response)
PY
