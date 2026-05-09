#!/usr/bin/env python3
from __future__ import annotations

import argparse
import os
import re
import shlex
import sys
import tomllib
import xml.etree.ElementTree as ET
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_POLICY = ROOT / "release" / "update-policy.toml"
DEFAULT_VERSION_FILE = ROOT / "VERSION"
SCHEMA_VERSION = "1context.update-policy.v1"
VERSION_RE = re.compile(r"^\d+\.\d+\.\d+$")


class PolicyError(Exception):
  pass


def load_policy(path: Path) -> dict[str, Any]:
  try:
    with path.open("rb") as handle:
      return tomllib.load(handle)
  except FileNotFoundError as exc:
    raise PolicyError(f"Update policy not found: {path}") from exc
  except tomllib.TOMLDecodeError as exc:
    raise PolicyError(f"Update policy is not valid TOML: {exc}") from exc


def read_version(path: Path) -> str:
  try:
    return path.read_text(encoding="utf-8").strip()
  except FileNotFoundError as exc:
    raise PolicyError(f"Version file not found: {path}") from exc


def string_value(policy: dict[str, Any], key: str, *, required: bool = True) -> str:
  value = policy.get(key)
  if value is None:
    if required:
      raise PolicyError(f"Missing required policy key: {key}")
    return ""
  if not isinstance(value, str):
    raise PolicyError(f"Policy key {key} must be a string.")
  return value.strip()


def table(policy: dict[str, Any], key: str) -> dict[str, Any]:
  value = policy.get(key)
  if not isinstance(value, dict):
    raise PolicyError(f"Policy key {key} must be a table.")
  return value


def bool_value(policy: dict[str, Any], key: str) -> bool:
  value = policy.get(key)
  if not isinstance(value, bool):
    raise PolicyError(f"Policy key {key} must be a boolean.")
  return value


def validate_policy(policy: dict[str, Any], *, expected_version: str) -> None:
  schema_version = string_value(policy, "schema_version")
  if schema_version != SCHEMA_VERSION:
    raise PolicyError(f"Unsupported policy schema_version {schema_version!r}.")

  version = string_value(policy, "version")
  if not VERSION_RE.match(version):
    raise PolicyError(f"Policy version must look like 0.1.59, got {version!r}.")
  if version != expected_version:
    raise PolicyError(f"Policy version {version} does not match VERSION {expected_version}.")

  update_class = string_value(policy, "update_class")
  if update_class not in {"mandatory", "optional"}:
    raise PolicyError("Policy update_class must be mandatory or optional.")

  for key in ("approved_by", "reason", "reason_detail"):
    if not string_value(policy, key):
      raise PolicyError(f"Policy key {key} must not be empty.")

  minimum_autoupdate_version = string_value(policy, "minimum_autoupdate_version", required=False)
  minimum_update_version = string_value(policy, "minimum_update_version", required=False)
  critical_update_version = string_value(policy, "critical_update_version", required=False)

  if update_class == "mandatory":
    if not minimum_autoupdate_version:
      raise PolicyError("Mandatory releases must declare minimum_autoupdate_version.")
    if not critical_update_version:
      raise PolicyError("Mandatory releases must declare critical_update_version.")
    if critical_update_version != version:
      raise PolicyError("Mandatory critical_update_version must match the release version.")
  else:
    if critical_update_version:
      raise PolicyError("Optional releases must not declare critical_update_version.")

  for optional_version_key, optional_version in (
    ("minimum_autoupdate_version", minimum_autoupdate_version),
    ("minimum_update_version", minimum_update_version),
  ):
    if optional_version and not VERSION_RE.match(optional_version):
      raise PolicyError(f"{optional_version_key} must look like 0.1.59 when set.")

  ui = table(policy, "ui")
  bool_value(ui, "show_release_notes_in_update_window")

  optional_prompt = table(ui, "optional_prompt")
  if not string_value(optional_prompt, "title") or not string_value(optional_prompt, "body"):
    raise PolicyError("Optional prompt title and body must not be empty.")

  failure_message = table(ui, "failure_message")
  failure_title = string_value(failure_message, "title")
  failure_body = string_value(failure_message, "body")
  if failure_title != "Update failed.":
    raise PolicyError('Failure title must be "Update failed."')
  if failure_body != "Please contact support at paul@haptica.ai.":
    raise PolicyError("Failure body must direct users to paul@haptica.ai.")

  post_install = table(ui, "post_install_message")
  bool_value(post_install, "enabled")
  if string_value(post_install, "title") != "1Context Improved!":
    raise PolicyError('Post-install title must default to "1Context Improved!"')
  string_value(post_install, "body", required=False)


def validate_appcast(policy: dict[str, Any], appcast_path: Path) -> None:
  if not appcast_path.exists():
    raise PolicyError(f"Appcast not found: {appcast_path}")

  root = ET.parse(appcast_path).getroot()
  namespaces = {"sparkle": "http://www.andymatuschak.org/xml-namespaces/sparkle"}
  item = root.find("./channel/item")
  if item is None:
    raise PolicyError("Appcast is missing channel/item.")

  version = string_value(policy, "version")
  update_class = string_value(policy, "update_class")
  ui = table(policy, "ui")
  show_notes = bool_value(ui, "show_release_notes_in_update_window")

  appcast_version = item.findtext("sparkle:version", namespaces=namespaces)
  if appcast_version != version:
    raise PolicyError(f"Appcast version {appcast_version!r} does not match policy {version}.")

  critical = item.find("sparkle:criticalUpdate", namespaces=namespaces)
  if update_class == "mandatory" and critical is None:
    raise PolicyError("Mandatory policy requires sparkle:criticalUpdate in appcast.")
  if update_class == "optional" and critical is not None:
    raise PolicyError("Optional policy must not produce sparkle:criticalUpdate.")

  description = item.find("description")
  if not show_notes and description is not None and (description.text or "").strip():
    raise PolicyError("Policy hides updater release notes, but appcast contains a description.")


def env(policy: dict[str, Any], policy_path: Path) -> dict[str, str]:
  ui = table(policy, "ui")
  optional_prompt = table(ui, "optional_prompt")
  failure_message = table(ui, "failure_message")
  post_install = table(ui, "post_install_message")
  update_class = string_value(policy, "update_class")
  version = string_value(policy, "version")
  return {
    "ONECONTEXT_RELEASE_POLICY_FILE": str(policy_path),
    "ONECONTEXT_RELEASE_POLICY_VERSION": version,
    "ONECONTEXT_RELEASE_POLICY_CLASS": update_class,
    "ONECONTEXT_SPARKLE_MANDATORY": "1" if update_class == "mandatory" else "0",
    "ONECONTEXT_SPARKLE_MANDATORY_FROM_VERSION": string_value(policy, "critical_update_version", required=False),
    "ONECONTEXT_SPARKLE_MINIMUM_AUTOUPDATE_VERSION": string_value(policy, "minimum_autoupdate_version", required=False),
    "ONECONTEXT_SPARKLE_MINIMUM_UPDATE_VERSION": string_value(policy, "minimum_update_version", required=False),
    "ONECONTEXT_SPARKLE_SHOW_RELEASE_NOTES_IN_UPDATE_WINDOW": "1" if bool_value(ui, "show_release_notes_in_update_window") else "0",
    "ONECONTEXT_UPDATE_OPTIONAL_PROMPT_TITLE": string_value(optional_prompt, "title"),
    "ONECONTEXT_UPDATE_OPTIONAL_PROMPT_BODY": string_value(optional_prompt, "body"),
    "ONECONTEXT_UPDATE_FAILURE_TITLE": string_value(failure_message, "title"),
    "ONECONTEXT_UPDATE_FAILURE_BODY": string_value(failure_message, "body"),
    "ONECONTEXT_UPDATE_POST_INSTALL_MESSAGE_ENABLED": "1" if bool_value(post_install, "enabled") else "0",
    "ONECONTEXT_UPDATE_POST_INSTALL_TITLE": string_value(post_install, "title"),
    "ONECONTEXT_UPDATE_POST_INSTALL_BODY": string_value(post_install, "body", required=False),
  }


def export_env(values: dict[str, str]) -> None:
  for key, value in values.items():
    print(f"export {key}={shlex.quote(value)}")


def parse_args() -> argparse.Namespace:
  parser = argparse.ArgumentParser(description="Validate and export 1Context release update policy.")
  parser.add_argument("command", choices=("validate", "export-env"))
  parser.add_argument("--policy", type=Path, default=Path(os.environ.get("ONECONTEXT_RELEASE_POLICY_FILE", DEFAULT_POLICY)))
  parser.add_argument("--version-file", type=Path, default=DEFAULT_VERSION_FILE)
  parser.add_argument("--appcast", type=Path)
  return parser.parse_args()


def main() -> int:
  args = parse_args()
  try:
    policy = load_policy(args.policy)
    version = read_version(args.version_file)
    validate_policy(policy, expected_version=version)
    if args.appcast is not None:
      validate_appcast(policy, args.appcast)
    if args.command == "export-env":
      export_env(env(policy, args.policy))
    return 0
  except PolicyError as exc:
    print(f"update policy error: {exc}", file=sys.stderr)
    return 1


if __name__ == "__main__":
  raise SystemExit(main())
