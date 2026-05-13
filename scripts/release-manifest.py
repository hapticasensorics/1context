#!/usr/bin/env python3
from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import os
import re
import shlex
import subprocess
import sys
import tomllib
import xml.etree.ElementTree as ET
from pathlib import Path
from typing import Any
from urllib.parse import urlparse


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_MANIFEST = ROOT / "release" / "release.toml"
DEFAULT_VERSION_FILE = ROOT / "VERSION"
DEFAULT_CORE = ROOT / "macos" / "Sources" / "OneContextCore" / "Core.swift"
DEFAULT_RELEASE_NOTES = ROOT / "RELEASE_NOTES.md"
DEFAULT_RELEASE_WORKFLOW = ROOT / ".github" / "workflows" / "release.yml"
DEFAULT_PROOF_WORKFLOW = ROOT / ".github" / "workflows" / "self-hosted-mac-update-proof.yml"
SCHEMA_VERSION = "1context.release.v1"
VERSION_RE = re.compile(r"^\d+\.\d+\.\d+$")
CORE_FALLBACK_RE = re.compile(r"fallback\s*=\s*\"([^\"]+)\"")
SPARKLE_NS = "http://www.andymatuschak.org/xml-namespaces/sparkle"
REQUIRED_PROOFS = {
  "clean_tree",
  "release_manifest",
  "version_consistency",
  "release_policy",
  "full_tests",
  "package_smoke",
  "asset_audit",
  "self_hosted_gui_update",
  "real_uninstall_reinstall",
  "evidence_redaction",
  "runner_attestation",
}
REQUIRED_RUNNER_LABELS = {"self-hosted", "macOS", "ARM64", "onecontext-update-runner"}
REQUIRED_REDACTION_PATTERNS = {
  "/Users/",
  "paulhan",
  ".codex",
  "Library/Application Support/1Context",
  "Library/Logs/1Context",
  "Library/Caches/1Context",
  "1context.sock",
  "SPARKLE_PRIVATE_ED_KEY",
  "GITHUB_TOKEN",
  "BEGIN PRIVATE KEY",
}
REQUIRED_MATRIX_CASES = {
  "already_current_manual_check",
  "mandatory_automatic_success",
  "optional_prompt",
  "broken_appcast_quiet_failure",
  "missing_dmg_retry_support_alert",
  "bad_signature",
  "interrupted_download",
  "try_again_repair",
  "offline_network",
  "stale_sparkle_defaults",
  "old_app_with_new_appcast",
  "app_relaunch_recovery",
  "login_restart_recovery",
}


class ManifestError(Exception):
  pass


def run_git(root: Path, args: list[str], *, check: bool = True) -> subprocess.CompletedProcess[str]:
  return subprocess.run(
    ["git", "-C", str(root), *args],
    check=check,
    text=True,
    stdout=subprocess.PIPE,
    stderr=subprocess.PIPE,
  )


def load_toml(path: Path, label: str) -> dict[str, Any]:
  try:
    with path.open("rb") as handle:
      return tomllib.load(handle)
  except FileNotFoundError as exc:
    raise ManifestError(f"{label} not found: {path}") from exc
  except tomllib.TOMLDecodeError as exc:
    raise ManifestError(f"{label} is not valid TOML: {exc}") from exc


def read_text(path: Path, label: str) -> str:
  try:
    return path.read_text(encoding="utf-8")
  except FileNotFoundError as exc:
    raise ManifestError(f"{label} not found: {path}") from exc


def string_value(mapping: dict[str, Any], key: str, *, required: bool = True) -> str:
  value = mapping.get(key)
  if value is None:
    if required:
      raise ManifestError(f"Missing required manifest key: {key}")
    return ""
  if not isinstance(value, str):
    raise ManifestError(f"Manifest key {key} must be a string.")
  return value.strip()


def bool_value(mapping: dict[str, Any], key: str) -> bool:
  value = mapping.get(key)
  if not isinstance(value, bool):
    raise ManifestError(f"Manifest key {key} must be a boolean.")
  return value


def string_list(mapping: dict[str, Any], key: str) -> list[str]:
  value = mapping.get(key)
  if not isinstance(value, list) or not all(isinstance(item, str) for item in value):
    raise ManifestError(f"Manifest key {key} must be a list of strings.")
  return [item.strip() for item in value]


def table(mapping: dict[str, Any], key: str) -> dict[str, Any]:
  value = mapping.get(key)
  if not isinstance(value, dict):
    raise ManifestError(f"Manifest key {key} must be a table.")
  return value


def semver_tuple(version: str) -> tuple[int, int, int]:
  if not VERSION_RE.match(version):
    raise ManifestError(f"Version must look like 0.1.64, got {version!r}.")
  return tuple(int(part) for part in version.split("."))  # type: ignore[return-value]


def manifest_version(manifest: dict[str, Any]) -> str:
  return string_value(manifest, "version")


def validate_manifest_shape(manifest: dict[str, Any]) -> None:
  schema_version = string_value(manifest, "schema_version")
  if schema_version != SCHEMA_VERSION:
    raise ManifestError(f"Unsupported release schema_version {schema_version!r}.")

  version = string_value(manifest, "version")
  previous_version = string_value(manifest, "previous_version")
  tag = string_value(manifest, "tag")
  update_class = string_value(manifest, "update_class")
  approved_by = string_value(manifest, "approved_by")
  reason = string_value(manifest, "reason")
  reason_detail = string_value(manifest, "reason_detail")
  minimum_autoupdate_version = string_value(manifest, "minimum_autoupdate_version", required=False)
  minimum_update_version = string_value(manifest, "minimum_update_version", required=False)
  critical_update_version = string_value(manifest, "critical_update_version", required=False)
  public_appcast_url = string_value(manifest, "public_appcast_url")
  stable_dmg_name = string_value(manifest, "stable_dmg_name")

  if not approved_by or not reason or not reason_detail:
    raise ManifestError("Manifest release approval, reason, and reason_detail must not be empty.")
  if semver_tuple(previous_version) >= semver_tuple(version):
    raise ManifestError(f"previous_version {previous_version} must be older than version {version}.")
  if tag != f"v{version}":
    raise ManifestError(f"Manifest tag {tag!r} must be v{version}.")
  if update_class not in {"mandatory", "optional"}:
    raise ManifestError("Manifest update_class must be mandatory or optional.")
  if public_appcast_url != "https://github.com/hapticasensorics/1context/releases/latest/download/appcast.xml":
    raise ManifestError("public_appcast_url must be the public latest/download appcast URL.")
  if stable_dmg_name != "1Context.dmg":
    raise ManifestError('stable_dmg_name must be "1Context.dmg".')

  if update_class == "mandatory":
    if minimum_autoupdate_version != previous_version:
      raise ManifestError("Mandatory releases must minimum-autoupdate from previous_version.")
    if critical_update_version != version:
      raise ManifestError("Mandatory releases must set critical_update_version to version.")
  else:
    if minimum_autoupdate_version:
      raise ManifestError("Optional releases must not set minimum_autoupdate_version.")
    if critical_update_version:
      raise ManifestError("Optional releases must not set critical_update_version.")
  if minimum_update_version:
    semver_tuple(minimum_update_version)

  required_proofs = set(string_list(manifest, "required_proofs"))
  missing_proofs = sorted(REQUIRED_PROOFS - required_proofs)
  if missing_proofs:
    raise ManifestError(f"Manifest is missing required proofs: {', '.join(missing_proofs)}")

  runner_labels = set(string_list(manifest, "required_runner_labels"))
  missing_labels = sorted(REQUIRED_RUNNER_LABELS - runner_labels)
  if missing_labels:
    raise ManifestError(f"Manifest is missing runner labels: {', '.join(missing_labels)}")

  notes_policy = table(manifest, "release_notes_policy")
  bool_value(notes_policy, "show_in_update_window")
  if not string_value(notes_policy, "public_notes_file"):
    raise ManifestError("release_notes_policy.public_notes_file must not be empty.")

  update_ui = table(manifest, "update_ui")
  optional_prompt = table(update_ui, "optional_prompt")
  if not string_value(optional_prompt, "title") or not string_value(optional_prompt, "body"):
    raise ManifestError("update_ui.optional_prompt title and body must not be empty.")

  failure_message = table(update_ui, "failure_message")
  failure_title = string_value(failure_message, "title")
  failure_body = string_value(failure_message, "body")
  if failure_title != "Update failed.":
    raise ManifestError('update_ui.failure_message.title must be "Update failed."')
  if failure_body != "Please contact support at paul@haptica.ai.":
    raise ManifestError("update_ui.failure_message.body must direct users to paul@haptica.ai.")

  post_install = table(update_ui, "post_install_message")
  bool_value(post_install, "enabled")
  if string_value(post_install, "title") != "1Context Improved!":
    raise ManifestError('update_ui.post_install_message.title must default to "1Context Improved!"')
  string_value(post_install, "body", required=False)

  redaction_policy = table(manifest, "evidence_redaction_policy")
  if not bool_value(redaction_policy, "require_redaction"):
    raise ManifestError("evidence_redaction_policy.require_redaction must be true.")
  forbidden_patterns = set(string_list(redaction_policy, "forbidden_patterns"))
  missing_patterns = sorted(REQUIRED_REDACTION_PATTERNS - forbidden_patterns)
  if missing_patterns:
    raise ManifestError(f"Evidence redaction policy is missing patterns: {', '.join(missing_patterns)}")

  matrix = manifest.get("updater_matrix")
  if not isinstance(matrix, list) or not all(isinstance(item, dict) for item in matrix):
    raise ManifestError("updater_matrix must be a list of case tables.")
  cases = {string_value(item, "case") for item in matrix}
  missing_cases = sorted(REQUIRED_MATRIX_CASES - cases)
  if missing_cases:
    raise ManifestError(f"Updater matrix is missing cases: {', '.join(missing_cases)}")
  for item in matrix:
    semver_tuple(string_value(item, "expected_version"))
    for key in ("proof", "description"):
      if not string_value(item, key):
        raise ManifestError(f"updater_matrix case {string_value(item, 'case')!r} has empty {key}.")


def validate_version_files(
  manifest: dict[str, Any],
  *,
  version_file: Path,
  core_file: Path,
  release_notes_file: Path,
) -> None:
  version = manifest_version(manifest)
  version_text = read_text(version_file, "VERSION file").strip()
  if version_text != version:
    raise ManifestError(f"VERSION {version_text!r} does not match release manifest {version!r}.")

  core_text = read_text(core_file, "Core.swift")
  match = CORE_FALLBACK_RE.search(core_text)
  if not match:
    raise ManifestError("Core.swift does not expose a fallback version.")
  if match.group(1) != version:
    raise ManifestError(f"Core.swift fallback {match.group(1)!r} does not match release manifest {version!r}.")

  release_notes = read_text(release_notes_file, "release notes")
  first_line = release_notes.splitlines()[0] if release_notes.splitlines() else ""
  if version not in first_line and f"v{version}" not in first_line:
    raise ManifestError(f"Release notes heading must mention {version}.")


def validate_appcast(manifest: dict[str, Any], appcast_path: Path) -> None:
  if not appcast_path.exists():
    raise ManifestError(f"Appcast not found: {appcast_path}")
  root = ET.parse(appcast_path).getroot()
  namespaces = {"sparkle": SPARKLE_NS}
  item = root.find("./channel/item")
  if item is None:
    raise ManifestError("Appcast is missing channel/item.")

  version = string_value(manifest, "version")
  update_class = string_value(manifest, "update_class")
  appcast_version = item.findtext("sparkle:version", namespaces=namespaces)
  if appcast_version != version:
    raise ManifestError(f"Appcast version {appcast_version!r} does not match manifest {version!r}.")

  critical = item.find("sparkle:criticalUpdate", namespaces=namespaces)
  if update_class == "mandatory":
    if critical is None:
      raise ManifestError("Mandatory manifest requires sparkle:criticalUpdate in appcast.")
    appcast_critical_version = critical.attrib.get(f"{{{SPARKLE_NS}}}version", "")
    if appcast_critical_version != string_value(manifest, "critical_update_version"):
      raise ManifestError("Appcast criticalUpdate version does not match manifest.")
  elif critical is not None:
    raise ManifestError("Optional manifest must not produce sparkle:criticalUpdate.")

  minimum_autoupdate = item.findtext("sparkle:minimumAutoupdateVersion", namespaces=namespaces) or ""
  if minimum_autoupdate != string_value(manifest, "minimum_autoupdate_version", required=False):
    raise ManifestError("Appcast minimumAutoupdateVersion does not match manifest.")

  enclosure = item.find("enclosure")
  if enclosure is None:
    raise ManifestError("Appcast is missing enclosure.")
  enclosure_url = enclosure.attrib.get("url", "")
  enclosure_length = enclosure.attrib.get("length", "")
  if not enclosure_length.isdigit() or int(enclosure_length) <= 0:
    raise ManifestError("Appcast enclosure must include a positive length.")
  ed_signature = enclosure.attrib.get(f"{{{SPARKLE_NS}}}edSignature", "")
  if not ed_signature.strip():
    raise ManifestError("Appcast enclosure is missing sparkle:edSignature.")

  expected_asset = f"1Context-{version}-macos-arm64.dmg"
  enclosure_asset = Path(urlparse(enclosure_url).path).name
  if enclosure_asset != expected_asset:
    raise ManifestError(f"Appcast enclosure asset {enclosure_asset!r} does not match {expected_asset!r}.")
  expected_url = f"https://github.com/hapticasensorics/1context/releases/download/v{version}/{expected_asset}"
  if enclosure_url != expected_url:
    raise ManifestError(f"Appcast enclosure url {enclosure_url!r} does not match {expected_url!r}.")

  notes_policy = table(manifest, "release_notes_policy")
  description = item.find("description")
  if not bool_value(notes_policy, "show_in_update_window") and description is not None:
    if (description.text or "").strip():
      raise ManifestError("Manifest hides updater release notes, but appcast contains a description.")


def validate_workflows(manifest: dict[str, Any], *, release_workflow: Path, proof_workflow: Path) -> None:
  release_text = read_text(release_workflow, "release workflow")
  for fragment in (
    "./scripts/release-train.sh validate",
    "./scripts/release-train.sh package",
    "./scripts/release-train.sh publish",
  ):
    if fragment not in release_text:
      raise ManifestError(f"Release workflow must invoke {fragment}.")
  for label in REQUIRED_RUNNER_LABELS:
    if label not in release_text:
      raise ManifestError(f"Release workflow is missing runner label {label}.")

  proof_text = read_text(proof_workflow, "self-hosted proof workflow")
  if "./scripts/release-train.sh prove --runner-execute" not in proof_text:
    raise ManifestError("Self-hosted proof workflow must execute through release-train.sh prove --runner-execute.")
  if "proof_reason:" not in proof_text:
    raise ManifestError("Self-hosted proof workflow must require a proof_reason input.")
  forbidden_inputs = (
    "old_version:",
    "new_version:",
    "staging_appcast_url:",
    "update_class:",
    "old_tag:",
    "old_dmg_url:",
    "update_timeout_seconds:",
    "steady_state_seconds:",
    "artifact_retention_days:",
  )
  for fragment in forbidden_inputs:
    if fragment in proof_text:
      raise ManifestError(f"Self-hosted proof workflow must not expose release fact input {fragment}")
  forbidden_envs = (
    "ONECONTEXT_OLD_VERSION:",
    "ONECONTEXT_NEW_VERSION:",
    "ONECONTEXT_OLD_TAG:",
    "ONECONTEXT_OLD_DMG_URL:",
    "ONECONTEXT_STAGING_APPCAST_URL:",
    "ONECONTEXT_EXPECTED_UPDATE_CLASS:",
    "ONECONTEXT_UPDATE_PROOF_TIMEOUT_SECONDS:",
    "ONECONTEXT_STEADY_STATE_SECONDS:",
  )
  for fragment in forbidden_envs:
    if fragment in proof_text:
      raise ManifestError(f"Self-hosted proof workflow must not pass manual release env {fragment}")
  for label in REQUIRED_RUNNER_LABELS:
    if label not in proof_text:
      raise ManifestError(f"Self-hosted proof workflow is missing runner label {label}.")


def check_clean_tree(root: Path) -> None:
  result = run_git(root, ["status", "--porcelain=v1", "--untracked-files=all"])
  dirty = [line for line in result.stdout.splitlines() if line.strip()]
  if dirty:
    preview = "\n".join(dirty[:20])
    raise ManifestError(f"Release tree is dirty; commit or remove changes before release:\n{preview}")


def check_sourced_helpers(root: Path) -> None:
  try:
    files = run_git(root, ["ls-files", "*.sh"]).stdout.splitlines()
  except subprocess.CalledProcessError as exc:
    raise ManifestError(f"Could not list tracked shell scripts: {exc.stderr.strip()}") from exc
  test_text = ""
  for test_path in sorted((root / "scripts").glob("test*.sh")) + [root / "scripts" / "test.sh"]:
    if test_path.exists():
      test_text += test_path.read_text(encoding="utf-8", errors="ignore") + "\n"

  references: set[str] = set()
  pattern = re.compile(r"(?:^|[;&|\s])(?:source|\.)\s+[\"']?(?:\$ROOT/)?([^\"'\s]+\.sh)")
  for relative in files:
    path = root / relative
    if not path.exists():
      continue
    text = path.read_text(encoding="utf-8", errors="ignore")
    for match in pattern.finditer(text):
      helper = match.group(1)
      if helper.startswith("./"):
        helper = helper[2:]
      if helper.startswith("scripts/"):
        references.add(helper)

  for helper in sorted(references):
    tracked = run_git(root, ["ls-files", "--error-unmatch", helper], check=False)
    if tracked.returncode != 0:
      raise ManifestError(f"Sourced shell helper is not tracked by Git: {helper}")
    if helper not in test_text and Path(helper).name not in test_text:
      raise ManifestError(f"Sourced shell helper is not referenced by a shell test: {helper}")


def validate_manifest(args: argparse.Namespace) -> dict[str, Any]:
  manifest = load_toml(args.manifest, "release manifest")
  validate_manifest_shape(manifest)
  validate_version_files(
    manifest,
    version_file=args.version_file,
    core_file=args.core_file,
    release_notes_file=args.release_notes,
  )
  validate_workflows(manifest, release_workflow=args.release_workflow, proof_workflow=args.proof_workflow)
  check_sourced_helpers(args.root)
  if args.appcast is not None:
    validate_appcast(manifest, args.appcast)
  if args.require_clean:
    check_clean_tree(args.root)
  return manifest


def env_for_manifest(manifest: dict[str, Any], manifest_path: Path) -> dict[str, str]:
  version = string_value(manifest, "version")
  tag = string_value(manifest, "tag")
  update_class = string_value(manifest, "update_class")
  notes_policy = table(manifest, "release_notes_policy")
  update_ui = table(manifest, "update_ui")
  optional_prompt = table(update_ui, "optional_prompt")
  failure_message = table(update_ui, "failure_message")
  post_install = table(update_ui, "post_install_message")
  return {
    "ONECONTEXT_RELEASE_MANIFEST": str(manifest_path),
    "ONECONTEXT_RELEASE_VERSION": version,
    "ONECONTEXT_RELEASE_PREVIOUS_VERSION": string_value(manifest, "previous_version"),
    "ONECONTEXT_RELEASE_TAG": tag,
    "ONECONTEXT_RELEASE_UPDATE_CLASS": update_class,
    "ONECONTEXT_RELEASE_PUBLIC_APPCAST_URL": string_value(manifest, "public_appcast_url"),
    "ONECONTEXT_RELEASE_STABLE_DMG_NAME": string_value(manifest, "stable_dmg_name"),
    "ONECONTEXT_SPARKLE_FEED_URL": string_value(manifest, "public_appcast_url"),
    "ONECONTEXT_SPARKLE_MANDATORY": "1" if update_class == "mandatory" else "0",
    "ONECONTEXT_SPARKLE_MANDATORY_FROM_VERSION": string_value(manifest, "critical_update_version", required=False),
    "ONECONTEXT_SPARKLE_MINIMUM_AUTOUPDATE_VERSION": string_value(manifest, "minimum_autoupdate_version", required=False),
    "ONECONTEXT_SPARKLE_MINIMUM_UPDATE_VERSION": string_value(manifest, "minimum_update_version", required=False),
    "ONECONTEXT_SPARKLE_SHOW_RELEASE_NOTES_IN_UPDATE_WINDOW": "1" if bool_value(notes_policy, "show_in_update_window") else "0",
    "ONECONTEXT_UPDATE_OPTIONAL_PROMPT_TITLE": string_value(optional_prompt, "title"),
    "ONECONTEXT_UPDATE_OPTIONAL_PROMPT_BODY": string_value(optional_prompt, "body"),
    "ONECONTEXT_UPDATE_FAILURE_TITLE": string_value(failure_message, "title"),
    "ONECONTEXT_UPDATE_FAILURE_BODY": string_value(failure_message, "body"),
    "ONECONTEXT_UPDATE_POST_INSTALL_MESSAGE_ENABLED": "1" if bool_value(post_install, "enabled") else "0",
    "ONECONTEXT_UPDATE_POST_INSTALL_TITLE": string_value(post_install, "title"),
    "ONECONTEXT_UPDATE_POST_INSTALL_BODY": string_value(post_install, "body", required=False),
    "SPARKLE_DOWNLOAD_URL_PREFIX": f"https://github.com/hapticasensorics/1context/releases/download/{tag}/",
    "SPARKLE_RELEASE_NOTES_URL_PREFIX": f"https://github.com/hapticasensorics/1context/releases/download/{tag}/",
    "SPARKLE_LINK_URL": f"https://github.com/hapticasensorics/1context/releases/tag/{tag}",
  }


def export_env(values: dict[str, str]) -> None:
  for key, value in values.items():
    print(f"export {key}={shlex.quote(value)}")


def sha256_file(path: Path) -> str:
  hasher = hashlib.sha256()
  with path.open("rb") as handle:
    for chunk in iter(lambda: handle.read(1024 * 1024), b""):
      hasher.update(chunk)
  return hasher.hexdigest()


def write_asset_manifest(manifest: dict[str, Any], dist_dir: Path, output: Path) -> None:
  version = string_value(manifest, "version")
  tag = string_value(manifest, "tag")
  asset_names = [
    f"1Context-{version}-macos-arm64.dmg",
    f"1Context-{version}-macos-arm64.dmg.sha256",
    string_value(manifest, "stable_dmg_name"),
    f"{string_value(manifest, 'stable_dmg_name')}.sha256",
    "appcast.xml",
  ]
  assets = []
  missing = []
  for name in asset_names:
    path = dist_dir / name
    if not path.exists():
      missing.append(name)
      continue
    assets.append({
      "name": name,
      "path": f"dist/{name}",
      "size": path.stat().st_size,
      "sha256": sha256_file(path),
    })
  if missing:
    raise ManifestError(f"Missing release assets: {', '.join(missing)}")
  output.parent.mkdir(parents=True, exist_ok=True)
  output.write_text(
    json.dumps({
      "schema_version": "1context.asset-manifest.v1",
      "version": version,
      "tag": tag,
      "generated_at": dt.datetime.now(dt.timezone.utc).isoformat(),
      "assets": assets,
    }, indent=2, sort_keys=True) + "\n",
    encoding="utf-8",
  )


def command_validate(args: argparse.Namespace) -> int:
  validate_manifest(args)
  print(f"release manifest valid: {args.manifest}")
  return 0


def command_export_env(args: argparse.Namespace) -> int:
  manifest = validate_manifest(args)
  export_env(env_for_manifest(manifest, args.manifest))
  return 0


def command_check_clean_tree(args: argparse.Namespace) -> int:
  check_clean_tree(args.root)
  print("release tree is clean")
  return 0


def command_check_sourced_helpers(args: argparse.Namespace) -> int:
  check_sourced_helpers(args.root)
  print("sourced shell helpers are tracked and tested")
  return 0


def command_write_asset_manifest(args: argparse.Namespace) -> int:
  manifest = validate_manifest(args)
  write_asset_manifest(manifest, args.dist_dir, args.output)
  print(f"wrote asset manifest: {args.output}")
  return 0


def command_forbidden_patterns(args: argparse.Namespace) -> int:
  manifest = load_toml(args.manifest, "release manifest")
  validate_manifest_shape(manifest)
  redaction_policy = table(manifest, "evidence_redaction_policy")
  for pattern in string_list(redaction_policy, "forbidden_patterns"):
    print(pattern)
  return 0


def command_matrix_cases(args: argparse.Namespace) -> int:
  manifest = load_toml(args.manifest, "release manifest")
  validate_manifest_shape(manifest)
  matrix = manifest.get("updater_matrix")
  assert isinstance(matrix, list)
  for item in matrix:
    assert isinstance(item, dict)
    print(string_value(item, "case"))
  return 0


def add_common_args(parser: argparse.ArgumentParser) -> None:
  parser.add_argument("--root", type=Path, default=ROOT)
  parser.add_argument("--manifest", type=Path, default=DEFAULT_MANIFEST)
  parser.add_argument("--version-file", type=Path, default=DEFAULT_VERSION_FILE)
  parser.add_argument("--core-file", type=Path, default=DEFAULT_CORE)
  parser.add_argument("--release-notes", type=Path, default=DEFAULT_RELEASE_NOTES)
  parser.add_argument("--release-workflow", type=Path, default=DEFAULT_RELEASE_WORKFLOW)
  parser.add_argument("--proof-workflow", type=Path, default=DEFAULT_PROOF_WORKFLOW)
  parser.add_argument("--appcast", type=Path)
  parser.add_argument("--require-clean", action="store_true")


def parse_args() -> argparse.Namespace:
  parser = argparse.ArgumentParser(description="Validate and export the 1Context release manifest.")
  subparsers = parser.add_subparsers(dest="command", required=True)

  validate_parser = subparsers.add_parser("validate")
  add_common_args(validate_parser)

  export_parser = subparsers.add_parser("export-env")
  add_common_args(export_parser)

  clean_parser = subparsers.add_parser("check-clean-tree")
  clean_parser.add_argument("--root", type=Path, default=ROOT)

  helpers_parser = subparsers.add_parser("check-sourced-helpers")
  helpers_parser.add_argument("--root", type=Path, default=ROOT)

  asset_parser = subparsers.add_parser("write-asset-manifest")
  add_common_args(asset_parser)
  asset_parser.add_argument("--dist-dir", type=Path, default=ROOT / "dist")
  asset_parser.add_argument("--output", type=Path, required=True)

  patterns_parser = subparsers.add_parser("forbidden-patterns")
  patterns_parser.add_argument("--manifest", type=Path, default=DEFAULT_MANIFEST)

  matrix_parser = subparsers.add_parser("matrix-cases")
  matrix_parser.add_argument("--manifest", type=Path, default=DEFAULT_MANIFEST)

  return parser.parse_args()


def main() -> int:
  args = parse_args()
  try:
    if args.command == "validate":
      return command_validate(args)
    if args.command == "export-env":
      return command_export_env(args)
    if args.command == "check-clean-tree":
      return command_check_clean_tree(args)
    if args.command == "check-sourced-helpers":
      return command_check_sourced_helpers(args)
    if args.command == "write-asset-manifest":
      return command_write_asset_manifest(args)
    if args.command == "forbidden-patterns":
      return command_forbidden_patterns(args)
    if args.command == "matrix-cases":
      return command_matrix_cases(args)
    raise ManifestError(f"Unknown command: {args.command}")
  except (ManifestError, subprocess.CalledProcessError, ET.ParseError) as exc:
    print(f"release manifest error: {exc}", file=sys.stderr)
    return 1


if __name__ == "__main__":
  raise SystemExit(main())
