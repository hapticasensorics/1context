#!/usr/bin/env bash
set -euo pipefail

OUTPUT="${1:-}"
if [[ -z "$OUTPUT" ]]; then
  echo "Usage: scripts/write-runner-attestation.sh <output.json>" >&2
  exit 2
fi
mkdir -p "$(dirname "$OUTPUT")"

python3 - "$OUTPUT" <<'PY'
from __future__ import annotations

import datetime as dt
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
from pathlib import Path

output = Path(sys.argv[1])


def run(args: list[str]) -> str:
  try:
    return subprocess.run(args, check=False, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT).stdout.strip()
  except FileNotFoundError:
    return ""


def sha256_text(value: str) -> str:
  return hashlib.sha256(value.encode("utf-8")).hexdigest() if value else ""


keychain_path = os.environ.get("ONECONTEXT_RELEASE_KEYCHAIN") or os.environ.get("CODESIGN_KEYCHAIN") or ""
identity_output = run(["security", "find-identity", "-v", "-p", "codesigning", keychain_path] if keychain_path else ["security", "find-identity", "-v", "-p", "codesigning"])
fingerprint = ""
for line in identity_output.splitlines():
  match = re.search(r"\b([0-9A-F]{40})\b", line)
  if match:
    fingerprint = match.group(1)
    break

sparkle_public_key = os.environ.get("ONECONTEXT_SPARKLE_PUBLIC_ED_KEY", "")
attestation = {
  "schema_version": "1context.runner-attestation.v1",
  "generated_at": dt.datetime.now(dt.timezone.utc).isoformat(),
  "os_version": run(["sw_vers"]),
  "xcode_version": run(["xcodebuild", "-version"]),
  "swift_version": run(["swift", "--version"]).splitlines()[0] if run(["swift", "--version"]) else "",
  "runner": {
    "name": os.environ.get("RUNNER_NAME", ""),
    "os": os.environ.get("RUNNER_OS", ""),
    "arch": os.environ.get("RUNNER_ARCH", ""),
    "workflow": os.environ.get("GITHUB_WORKFLOW", ""),
    "ref": os.environ.get("GITHUB_REF", ""),
    "sha": os.environ.get("GITHUB_SHA", ""),
  },
  "keychain": {
    "path": "[REDACTED:keychain-path]" if keychain_path else "",
    "path_sha256": sha256_text(keychain_path),
  },
  "signing_identity": {
    "fingerprint": fingerprint,
    "configured_name_present": bool(os.environ.get("CODESIGN_IDENTITY")),
  },
  "notary": {
    "profile_name_present": bool(os.environ.get("NOTARYTOOL_PROFILE")),
    "profile_name_sha256": sha256_text(os.environ.get("NOTARYTOOL_PROFILE", "")),
  },
  "sparkle": {
    "public_key_present": bool(sparkle_public_key),
    "public_key_sha256": sha256_text(sparkle_public_key),
  },
  "capabilities": {
    "screencapture": bool(shutil.which("screencapture")),
    "osascript": bool(shutil.which("osascript")),
    "hammerspoon_cli": bool(shutil.which("hs")),
  },
}
output.write_text(json.dumps(attestation, indent=2, sort_keys=True) + "\n", encoding="utf-8")
PY
