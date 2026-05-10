#!/usr/bin/env python3
from __future__ import annotations

import argparse
from pathlib import Path


def read_text(path: Path) -> str:
  if not path.exists():
    return ""
  return path.read_text(errors="replace")


def field_value(text: str, label: str) -> str:
  prefix = f"{label}:"
  for line in text.splitlines():
    stripped = line.strip()
    if stripped.startswith(prefix):
      return stripped[len(prefix):].strip()
  return "unknown"


def yes_no_from_line(text: str, label: str) -> str:
  value = field_value(text, label).lower()
  if value in {"yes", "no"}:
    return value
  return "unknown"


def has_failed_update_evidence(failed_update_dir: Path | None) -> bool:
  if failed_update_dir is None:
    return False
  result = read_text(failed_update_dir / "result.txt")
  failure_message = read_text(failed_update_dir / "failure-message.txt")
  return "failure_case=" in result and "Update failed." in failure_message


def classify(evidence_dir: Path, failed_update_dir: Path | None) -> list[str]:
  status = read_text(evidence_dir / "status-debug.txt")
  diagnose = read_text(evidence_dir / "diagnose.txt")
  update = read_text(evidence_dir / "update-status.txt")
  failed_update = has_failed_update_evidence(failed_update_dir)

  runtime_running = "yes" if "1Context is running." in status else "no" if "1Context is not running." in status else "unknown"
  runtime_health = field_value(status, "Health")
  desired_state = field_value(diagnose, "Desired State")
  update_available = yes_no_from_line(update, "Update Available")
  mandatory_update = yes_no_from_line(update, "Mandatory Update")

  if failed_update:
    diagnostic_state = "failed_update"
  elif runtime_running == "no" and desired_state == "stopped":
    diagnostic_state = "stopped_by_user"
  elif runtime_health == "Needs Setup" or "Required Setup:" in status or field_value(diagnose, "Required Setup") == "needs setup":
    diagnostic_state = "needs_setup"
  elif update_available == "yes" or mandatory_update == "yes":
    diagnostic_state = "needs_update"
  elif runtime_running == "yes" and runtime_health == "OK":
    diagnostic_state = "healthy"
  else:
    diagnostic_state = "unknown"

  lines = [
    f"diagnostic_state={diagnostic_state}",
    f"runtime_running={runtime_running}",
    f"runtime_health={runtime_health}",
    f"desired_state={desired_state}",
    f"update_available={update_available}",
    f"mandatory_update={mandatory_update}",
    f"failed_update_evidence={'yes' if failed_update else 'no'}",
  ]
  if failed_update_dir is not None:
    lines.append(f"failed_update_evidence_dir={failed_update_dir}")
  return lines


def main() -> int:
  parser = argparse.ArgumentParser(description="Classify a 1Context release-lockdown evidence bundle.")
  parser.add_argument("--evidence-dir", required=True, type=Path)
  parser.add_argument("--failed-update-dir", type=Path)
  args = parser.parse_args()

  for line in classify(args.evidence_dir, args.failed_update_dir):
    print(line)
  return 0


if __name__ == "__main__":
  raise SystemExit(main())
