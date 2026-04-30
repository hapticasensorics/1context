from __future__ import annotations

import contextlib
import io
import json
import os
import sys
from pathlib import Path
from typing import Any

from onectx.cli import main as onectx_main
from onectx.config import ConfigError, compile_system_map, load_system
from onectx.storage import LakeStore, TABLE_ORDER, StorageError


SCHEMA_VERSION = 1
DEFAULT_ROOT = Path(__file__).resolve().parents[2]
ALLOWED_SHAPES = {
    ("status", "--json"),
    ("storage", "init", "--json"),
    ("wiki", "list", "--json"),
    ("wiki", "ensure", "--json"),
    ("wiki", "render", "--json"),
    ("wiki", "render", "for-you", "--no-evidence", "--json"),
    ("wiki", "routes", "--json"),
    ("memory", "tick", "--wiki-only", "--json"),
    ("memory", "cycles", "list", "--json"),
}


def main(argv: list[str] | None = None) -> int:
    args = list(sys.argv[1:] if argv is None else argv)
    root = Path(os.environ.get("ONECONTEXT_MEMORY_CORE_ROOT") or DEFAULT_ROOT).expanduser().resolve()

    try:
      return dispatch(args, root=root)
    except (ConfigError, StorageError, MemoryCoreContractError) as exc:
      print_json(error_payload("contract_error", str(exc)))
      return 1
    except Exception as exc:  # pragma: no cover - final safety net for subprocess callers
      print_json(error_payload("unexpected_error", str(exc)))
      return 1


def dispatch(args: list[str], *, root: Path) -> int:
    shape = tuple(args)
    if shape not in ALLOWED_SHAPES and not is_allowed_parameterized_shape(args):
        raise MemoryCoreContractError(f"unsupported memory-core command: {' '.join(args) or '(empty)'}")

    if shape == ("status", "--json"):
        print_json(status_payload(root))
        return 0

    if shape == ("storage", "init", "--json"):
        system = load_system(root)
        store = LakeStore(system.storage_dir)
        counts = store.ensure()
        print_json(ok_payload("storage.init", {
            "storage_dir": str(system.storage_dir),
            "tables": {name: counts.get(name, 0) for name in TABLE_ORDER},
        }))
        return 0

    delegated_args = add_root_to_args(args, root)
    rc, stdout, stderr = run_private_cli(delegated_args)
    if rc != 0:
        print_json(error_payload("command_failed", stderr.strip() or stdout.strip() or f"exit {rc}"))
        return rc
    parsed = parse_json(stdout)
    print_json(ok_payload(".".join(args[:2]).replace(".--json", ""), parsed))
    return 0


def status_payload(root: Path) -> dict[str, Any]:
    system = load_system(root)
    system_map = compile_system_map(system)
    return ok_payload("status", {
        "root": str(system.root),
        "active_plugin": system.active_plugin,
        "storage_dir": str(system.storage_dir),
        "runtime_dir": str(system.runtime_dir),
        "capabilities": sorted(" ".join(shape) for shape in ALLOWED_SHAPES),
        "jobs": len(system_map.get("jobs", {})),
        "agents": len(system.agents),
        "state_machines": len(system.state_machines),
    })


def add_root_to_args(args: list[str], root: Path) -> list[str]:
    return ["--root", str(root), *args]


def is_allowed_parameterized_shape(args: list[str]) -> bool:
    if is_cycle_shape(args, "show") or is_cycle_shape(args, "validate"):
        return True
    if len(args) >= 6 and args[:2] == ["memory", "replay-dry-run"] and args[-1] == "--json":
        index = 2
        saw_start = False
        saw_end = False
        while index < len(args) - 1:
            if index + 1 >= len(args) - 1:
                return False
            option = args[index]
            value = args[index + 1]
            if option == "--start":
                if not safe_scalar(value):
                    return False
                saw_start = True
            elif option == "--end":
                if not safe_scalar(value):
                    return False
                saw_end = True
            elif option == "--sources":
                if not all(safe_identifier(part) for part in value.split(",")):
                    return False
            elif option == "--replay-run-id":
                if not safe_identifier(value):
                    return False
            else:
                return False
            index += 2
        return saw_start and saw_end
    return False


def is_cycle_shape(args: list[str], verb: str) -> bool:
    return (
        len(args) == 5
        and args[:3] == ["memory", "cycles", verb]
        and safe_identifier(args[3])
        and args[4] == "--json"
    )


def safe_scalar(value: str) -> bool:
    return bool(value) and len(value) <= 128 and "/" not in value and "\0" not in value and not value.startswith("-")


def safe_identifier(value: str) -> bool:
    allowed = set("abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789._:-,")
    return safe_scalar(value) and value not in {".", ".."} and all(character in allowed for character in value)


def run_private_cli(args: list[str]) -> tuple[int, str, str]:
    stdout = io.StringIO()
    stderr = io.StringIO()
    with contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
        rc = onectx_main(args)
    return int(rc or 0), stdout.getvalue(), stderr.getvalue()


def parse_json(stdout: str) -> Any:
    text = stdout.strip()
    if not text:
        return {}
    try:
        return json.loads(text)
    except json.JSONDecodeError as exc:
        raise MemoryCoreContractError(f"private CLI returned non-JSON output: {exc}") from exc


def ok_payload(command: str, result: Any) -> dict[str, Any]:
    return {
        "status": "ok",
        "schema_version": SCHEMA_VERSION,
        "command": command,
        "result": result,
    }


def error_payload(code: str, message: str) -> dict[str, Any]:
    return {
        "status": "error",
        "schema_version": SCHEMA_VERSION,
        "error": {
            "code": code,
            "message": message,
        },
    }


def print_json(payload: dict[str, Any]) -> None:
    print(json.dumps(payload, indent=2, sort_keys=True))


class MemoryCoreContractError(RuntimeError):
    pass


if __name__ == "__main__":
    raise SystemExit(main())
