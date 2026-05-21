#!/usr/bin/env python3
import base64
import hashlib
import json
import os
import plistlib
import re
import struct
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path


HOST_NAME = "com.haptica.onecontext_browser_bridge"
PROOF_SCHEMA = "1context.permission-proof.v2"
PROOF_KEY = "RememberingBrowserExtensionProof"
REQUIRED_PERMISSIONS = {"nativeMessaging", "scripting", "storage", "tabs"}
REQUIRED_HOST_PERMISSIONS = {"<all_urls>"}


def send_message(message):
    encoded = json.dumps(message, separators=(",", ":")).encode("utf-8")
    sys.stdout.buffer.write(struct.pack("@I", len(encoded)))
    sys.stdout.buffer.write(encoded)
    sys.stdout.buffer.flush()


def read_message():
    raw_length = sys.stdin.buffer.read(4)
    if not raw_length:
        raise EOFError("No native message was received.")
    message_length = struct.unpack("@I", raw_length)[0]
    if message_length <= 0 or message_length > 32 * 1024 * 1024:
        raise ValueError(f"Invalid native message length: {message_length}")
    raw_message = sys.stdin.buffer.read(message_length)
    if len(raw_message) != message_length:
        raise EOFError("Native message ended before the declared length.")
    return json.loads(raw_message.decode("utf-8"))


def latest_app_bundle():
    override = os.environ.get("ONECONTEXT_BROWSER_BRIDGE_APP")
    if override:
        return Path(override).expanduser().resolve()

    candidates = []
    for pattern in [
        "/Applications/1Context Dev - *.app",
        "/Applications/1Context Dev.app",
        "/Applications/1Context.app"
    ]:
        candidates.extend(Path("/").glob(pattern.lstrip("/")))

    existing = [candidate for candidate in candidates if candidate.is_dir()]
    if not existing:
        raise FileNotFoundError("No installed 1Context app bundle was found.")
    return max(existing, key=lambda path: path.stat().st_mtime).resolve()


def read_info_plist(app_bundle):
    info_path = app_bundle / "Contents" / "Info.plist"
    with info_path.open("rb") as handle:
        return plistlib.load(handle)


def designated_requirement(app_bundle):
    proc = subprocess.run(
        ["/usr/bin/codesign", "-dr", "-", str(app_bundle)],
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        check=True,
    )
    for line in proc.stdout.splitlines():
        match = re.search(r"designated => (.+)$", line)
        if match:
            return match.group(1)
    raise ValueError(f"Could not read designated requirement for {app_bundle}.")


def permission_subject(app_bundle, info):
    requirement = designated_requirement(app_bundle)
    app_version = info.get("CFBundleVersion") or info.get("CFBundleShortVersionString") or "unknown"
    bundle_identifier = info.get("CFBundleIdentifier")
    if not bundle_identifier:
        raise ValueError(f"{app_bundle} is missing CFBundleIdentifier.")

    return {
        "bundle_identifier": bundle_identifier,
        "designated_requirement_sha256": hashlib.sha256(requirement.encode("utf-8")).hexdigest(),
        "designated_requirement": requirement,
        "executable_path": str(app_bundle / "Contents" / "MacOS" / "1Context"),
        "app_version": str(app_version),
    }


def preferences_path(bundle_identifier):
    return Path.home() / "Library" / "Preferences" / f"{bundle_identifier}.plist"


def read_preferences(path):
    if not path.exists():
        return {}
    with path.open("rb") as handle:
        return plistlib.load(handle)


def write_preferences(path, preferences):
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_suffix(path.suffix + ".tmp")
    with temporary.open("wb") as handle:
        plistlib.dump(preferences, handle, sort_keys=True)
    temporary.replace(path)


def utc_now():
    return datetime.now(timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z")


def decode_png_data_url(data_url, output_path):
    prefix = "data:image/png;base64,"
    if not data_url or not data_url.startswith(prefix):
        return None
    raw = base64.b64decode(data_url[len(prefix):])
    output_path.write_bytes(raw)
    return output_path


def capture_directory(info):
    app_support_name = info.get("CFBundleDisplayName") or info.get("CFBundleName") or "1Context"
    timestamp = datetime.now().strftime("%Y%m%d-%H%M%S")
    path = Path.home() / "Library" / "Application Support" / app_support_name / "browser-extension-captures" / timestamp
    path.mkdir(parents=True, exist_ok=True)
    return path


def save_capture_bundle(message, info):
    directory = capture_directory(info)
    page = message.get("pageContext") or {}
    metadata = {
        "schema": "1context.browser-extension-capture.v1",
        "captured_at": utc_now(),
        "user_note": message.get("userNote") or "",
        "extension": message.get("extension") or {},
        "tab": message.get("tab") or {},
        "page": {
            "url": page.get("url"),
            "title": page.get("title"),
            "selectedText": page.get("selectedText"),
            "viewport": page.get("viewport"),
            "timestamp": page.get("timestamp"),
        },
    }
    (directory / "meta.json").write_text(json.dumps(metadata, indent=2, sort_keys=True), encoding="utf-8")
    (directory / "dom.html").write_text(page.get("domExcerpt") or "", encoding="utf-8")
    (directory / "visible-text.txt").write_text(page.get("visibleText") or "", encoding="utf-8")
    decode_png_data_url(message.get("screenshotDataUrl"), directory / "screenshot.png")
    return directory


def validate_extension(extension):
    extension_id = extension.get("id") or ""
    extension_version = extension.get("version") or ""
    permissions = set(extension.get("permissions") or [])
    host_permissions = set(extension.get("hostPermissions") or [])

    missing_permissions = sorted(REQUIRED_PERMISSIONS - permissions)
    missing_host_permissions = sorted(REQUIRED_HOST_PERMISSIONS - host_permissions)
    if not extension_id:
        raise ValueError("Extension proof is missing chrome.runtime.id.")
    if not extension_version:
        raise ValueError("Extension proof is missing manifest version.")
    if missing_permissions or missing_host_permissions:
        raise PermissionError(
            "Extension is missing required permissions: "
            + json.dumps(
                {
                    "permissions": missing_permissions,
                    "host_permissions": missing_host_permissions,
                },
                sort_keys=True,
            )
        )
    return extension_id, extension_version, sorted(permissions), sorted(host_permissions)


def record_browser_extension_proof(message):
    if message.get("type") != "ONECONTEXT_BROWSER_EXTENSION_PROOF":
        raise ValueError(f"Unsupported message type: {message.get('type')!r}")

    app_bundle = latest_app_bundle()
    info = read_info_plist(app_bundle)
    subject = permission_subject(app_bundle, info)
    extension_id, extension_version, permissions, host_permissions = validate_extension(message.get("extension") or {})
    bundle_dir = save_capture_bundle(message, info)

    path = preferences_path(subject["bundle_identifier"])
    preferences = read_preferences(path)
    preferences[PROOF_KEY] = {
        "schema_version": PROOF_SCHEMA,
        "subject": subject,
        "method": "browser-extension-native-handshake",
        "proved_at": utc_now(),
        "details": {
            "host_name": HOST_NAME,
            "extension_identifier": extension_id,
            "extension_version": extension_version,
            "extension_name": (message.get("extension") or {}).get("name") or "",
            "granted_permissions": permissions,
            "granted_host_permissions": host_permissions,
            "capture_bundle": str(bundle_dir),
            "page_url": ((message.get("pageContext") or {}).get("url") or ""),
            "page_title": ((message.get("pageContext") or {}).get("title") or ""),
        },
    }
    write_preferences(path, preferences)

    return {
        "ok": True,
        "message": "1Context browser extension proof recorded.",
        "app": str(app_bundle),
        "bundleIdentifier": subject["bundle_identifier"],
        "preferencesPath": str(path),
        "extensionId": extension_id,
        "extensionVersion": extension_version,
        "captureBundle": str(bundle_dir),
    }


def main():
    try:
      send_message(record_browser_extension_proof(read_message()))
    except Exception as error:
      send_message({"ok": False, "error": str(error), "errorType": type(error).__name__})


if __name__ == "__main__":
    main()
