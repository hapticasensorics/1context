from __future__ import annotations

import json
import os
import sys
from pathlib import Path
from typing import Any

from onectx.config import ConfigError, compile_system_map, load_system
from onectx.agent.launch_plan import (
    AgentLaunchPlanError,
    build_agent_launch_plan,
    parse_param_pairs,
)
from onectx.memory.replay import ReplayError, run_replay_dry_run
from onectx.memory.tick import (
    MemoryTickError,
    list_memory_cycles,
    load_memory_cycle,
    run_memory_tick,
    validate_memory_cycle,
)
from onectx.storage import LakeStore, TABLE_ORDER, StorageError


SCHEMA_VERSION = 1
DEFAULT_ROOT = Path(__file__).resolve().parents[2]
ALLOWED_SHAPES = {
    ("status", "--json"),
    ("storage", "init", "--json"),
    ("memory", "tick", "--wiki-only", "--json"),
    ("memory", "cycles", "list", "--json"),
}
PARAMETERIZED_CAPABILITIES = {
    "agent launch-plan <job-id> [--provider declared|codex|claude] [--model <model>] [--run-id <run-id>] [--param key=value] --json",
    "memory cycles show <cycle-id> --json",
    "memory cycles validate <cycle-id> --json",
    "memory replay-dry-run --start <date> --end <date> [--sources codex,claude-code] [--replay-run-id <id>] --json",
}


def main(argv: list[str] | None = None) -> int:
    args = list(sys.argv[1:] if argv is None else argv)
    root = Path(os.environ.get("ONECONTEXT_MEMORY_CORE_ROOT") or DEFAULT_ROOT).expanduser().resolve()

    try:
      return dispatch(args, root=root)
    except (ConfigError, StorageError, MemoryCoreContractError, AgentLaunchPlanError) as exc:
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

    try:
        command, result, exit_code = execute_allowed_shape(args, root=root)
    except (MemoryTickError, ReplayError) as exc:
        print_json(error_payload("command_failed", str(exc)))
        return 1
    print_json(ok_payload(command, result))
    return exit_code


def status_payload(root: Path) -> dict[str, Any]:
    system = load_system(root)
    system_map = compile_system_map(system)
    return ok_payload("status", {
        "root": str(system.root),
        "active_plugin": system.active_plugin,
        "storage_dir": str(system.storage_dir),
        "runtime_dir": str(system.runtime_dir),
        "capabilities": sorted({" ".join(shape) for shape in ALLOWED_SHAPES} | PARAMETERIZED_CAPABILITIES),
        "jobs": len(system_map.get("jobs", {})),
        "agents": len(system.agents),
        "state_machines": len(system.state_machines),
    })


def execute_allowed_shape(args: list[str], *, root: Path) -> tuple[str, Any, int]:
    system = load_system(root)

    if args == ["memory", "tick", "--wiki-only", "--json"]:
        result = run_memory_tick(system, wiki_only=True)
        return "memory.tick", result.to_payload(), memory_tick_exit_code(result.status)

    if args == ["memory", "cycles", "list", "--json"]:
        cycles = list_memory_cycles(system, limit=20)
        return "memory.cycles", {"cycles": [cycle.to_payload() for cycle in cycles]}, 0

    if is_cycle_shape(args, "show"):
        return "memory.cycles", load_memory_cycle(system, args[3]), 0

    if is_cycle_shape(args, "validate"):
        result = validate_memory_cycle(system, args[3])
        return "memory.cycles", result.to_payload(), 0 if result.passed else 2

    if len(args) >= 6 and args[:2] == ["memory", "replay-dry-run"] and args[-1] == "--json":
        replay_args = parse_replay_args(args[2:-1])
        result = run_replay_dry_run(system, **replay_args)
        return "memory.replay-dry-run", result.to_payload(), 0

    if is_agent_launch_plan_shape(args):
        launch_args = parse_agent_launch_plan_args(args[2:-1])
        result = build_agent_launch_plan(system, **launch_args)
        return "agent.launch-plan", result.to_payload(root=system.root), 0

    raise MemoryCoreContractError(f"unsupported memory-core command: {' '.join(args) or '(empty)'}")


def memory_tick_exit_code(status: str) -> int:
    if status in {"blocked", "retryable"}:
        return 2
    if status == "failed":
        return 1
    return 0


def parse_replay_args(args: list[str]) -> dict[str, Any]:
    parsed: dict[str, Any] = {
        "start": "",
        "end": "",
        "sources": ("codex", "claude-code"),
        "replay_run_id": "",
    }
    index = 0
    while index < len(args):
        option = args[index]
        value = args[index + 1]
        if option == "--start":
            parsed["start"] = value
        elif option == "--end":
            parsed["end"] = value
        elif option == "--sources":
            parsed["sources"] = tuple(item.strip() for item in value.split(",") if item.strip())
        elif option == "--replay-run-id":
            parsed["replay_run_id"] = value
        else:
            raise MemoryCoreContractError(f"unsupported replay option: {option}")
        index += 2
    return parsed


def is_allowed_parameterized_shape(args: list[str]) -> bool:
    if is_agent_launch_plan_shape(args):
        return True
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


def is_agent_launch_plan_shape(args: list[str]) -> bool:
    if len(args) < 4 or args[:2] != ["agent", "launch-plan"] or args[-1] != "--json":
        return False
    if not safe_identifier(args[2]):
        return False
    index = 3
    while index < len(args) - 1:
        option = args[index]
        if option in {"--provider", "--model", "--run-id"}:
            if index + 1 >= len(args) - 1 or not safe_identifier(args[index + 1]):
                return False
            index += 2
            continue
        if option == "--param":
            if index + 1 >= len(args) - 1 or not safe_param_pair(args[index + 1]):
                return False
            index += 2
            continue
        return False
    return True


def parse_agent_launch_plan_args(args: list[str]) -> dict[str, Any]:
    parsed: dict[str, Any] = {
        "job_id": args[0],
        "provider": "declared",
        "model": "",
        "run_id": "",
        "params": {},
    }
    params: list[str] = []
    index = 1
    while index < len(args):
        option = args[index]
        value = args[index + 1]
        if option == "--provider":
            parsed["provider"] = value
        elif option == "--model":
            parsed["model"] = value
        elif option == "--run-id":
            parsed["run_id"] = value
        elif option == "--param":
            params.append(value)
        else:
            raise MemoryCoreContractError(f"unsupported launch-plan option: {option}")
        index += 2
    parsed["params"] = parse_param_pairs(params)
    return parsed


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


def safe_param_pair(value: str) -> bool:
    if "=" not in value or "\0" in value or len(value) > 512:
        return False
    key, _ = value.split("=", 1)
    return safe_identifier(key)


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
