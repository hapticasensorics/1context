from __future__ import annotations

from dataclasses import dataclass
import re
from pathlib import Path
from typing import Any

from onectx.config import MemorySystem, compile_system_map
from onectx.io_utils import atomic_write_json, exclusive_file_lock, read_json_object
from onectx.storage import utc_now


class StateMachineRuntimeError(RuntimeError):
    """Raised when a compiled state-machine IR cannot be traversed."""


@dataclass(frozen=True)
class TransitionPlan:
    machine_id: str
    scope: str
    transition_id: str
    transition_index: int
    event: str
    event_kind: str
    source: dict[str, Any]
    target: dict[str, Any]
    guard: str
    actions: tuple[dict[str, Any], ...]
    summary: dict[str, tuple[str, ...]]

    def to_payload(self) -> dict[str, Any]:
        return {
            "machine": self.machine_id,
            "scope": self.scope,
            "transition": self.transition_id,
            "transition_index": self.transition_index,
            "event": self.event,
            "event_kind": self.event_kind,
            "source": dict(self.source),
            "target": dict(self.target),
            "guard": self.guard,
            "steps": list(self.summary["steps"]),
            "expects": list(self.summary["expects"]),
            "emits": list(self.summary["emits"]),
            "spawns": list(self.summary["spawns"]),
            "waits": list(self.summary["waits"]),
            "sets": list(self.summary["sets"]),
            "action_kinds": list(self.summary["action_kinds"]),
        }


@dataclass(frozen=True)
class TransitionExecution:
    plan: TransitionPlan
    status: str
    produced_evidence: tuple[str, ...] = ()
    completed_steps: tuple[str, ...] = ()
    emitted_events: tuple[str, ...] = ()
    note: str = ""

    @property
    def missing_expected_evidence(self) -> tuple[str, ...]:
        produced = set(self.produced_evidence)
        return tuple(item for item in self.plan.summary["expects"] if item not in produced)

    @property
    def target_state(self) -> str:
        if self.plan.target.get("stay"):
            return str(self.plan.source.get("state") or "")
        return str(self.plan.target.get("state") or "")

    def to_payload(self) -> dict[str, Any]:
        payload = {
            **self.plan.to_payload(),
            "status": self.status,
            "target_state": self.target_state,
            "completed_steps": list(self.completed_steps),
            "produced_evidence": list(self.produced_evidence),
            "missing_expected_evidence": list(self.missing_expected_evidence),
            "emitted_events": list(self.emitted_events),
        }
        if self.note:
            payload["note"] = self.note
        return payload


def scope_state_path(system: MemorySystem, *, machine_id: str, scope: str, key: str) -> Path:
    return (
        system.runtime_dir
        / "state-machines"
        / "scope-state"
        / safe_component(machine_id)
        / safe_component(scope)
        / f"{safe_component(key)}.json"
    )


def load_scope_state(system: MemorySystem, *, machine_id: str, scope: str, key: str) -> dict[str, Any]:
    path = scope_state_path(system, machine_id=machine_id, scope=scope, key=key)
    payload = read_json_object(path)
    if not payload:
        return {}
    return payload


def persist_scope_state(
    system: MemorySystem,
    *,
    machine_id: str,
    scope: str,
    key: str,
    initial_state: str,
    terminal_state: str,
    transitions: tuple[TransitionExecution, ...] = (),
    status: str = "",
    dry_run: bool = False,
    note: str = "",
) -> dict[str, Any]:
    path = scope_state_path(system, machine_id=machine_id, scope=scope, key=key)
    lock_path = path.with_suffix(path.suffix + ".lock")
    with exclusive_file_lock(lock_path):
        previous = read_json_object(path) or {}
        previous_history = (
            previous.get("history")
            if isinstance(previous.get("history"), list)
            else []
        )
        history = [
            *previous_history[-50:],
            {
                "ts": utc_now(),
                "from": initial_state,
                "to": terminal_state,
                "status": status,
                "dry_run": dry_run,
                "transition_count": len(transitions),
                "transitions": [execution.plan.transition_id for execution in transitions],
            },
        ]
        payload = {
            "kind": "state_machine_scope_state",
            "machine": machine_id,
            "scope": scope,
            "key": key,
            "state": terminal_state,
            "previous_state": str(previous.get("state") or ""),
            "initial_state": initial_state,
            "status": status,
            "dry_run": dry_run,
            "transition_count": len(transitions),
            "transitions": [execution.to_payload() for execution in transitions],
            "history": history,
            "created_at": str(previous.get("created_at") or utc_now()),
            "updated_at": utc_now(),
            "path": str(path),
        }
        if note:
            payload["note"] = note
        atomic_write_json(path, payload)
    return payload


def select_transition(
    system: MemorySystem,
    *,
    machine_id: str,
    scope: str,
    source_state: str,
    event_name: str,
    target_state: str = "",
    event_kind: str = "",
) -> TransitionPlan:
    machine = compile_system_map(system)["state_machines"].get(machine_id)
    if not isinstance(machine, dict):
        raise StateMachineRuntimeError(f"state-machine {machine_id!r} is not available")
    return select_transition_from_ir(
        machine,
        machine_id=machine_id,
        scope=scope,
        source_state=source_state,
        event_name=event_name,
        target_state=target_state,
        event_kind=event_kind,
    )


def select_transition_from_ir(
    machine: dict[str, Any],
    *,
    machine_id: str = "",
    scope: str,
    source_state: str,
    event_name: str,
    target_state: str = "",
    event_kind: str = "",
) -> TransitionPlan:
    resolved_machine_id = machine_id or str(machine.get("id") or "")
    if not resolved_machine_id:
        raise StateMachineRuntimeError("machine id is required")

    matches: list[TransitionPlan] = []
    for index, transition in enumerate(machine.get("transitions", []), start=1):
        if not isinstance(transition, dict):
            continue
        source = transition.get("source") if isinstance(transition.get("source"), dict) else {}
        target = transition.get("target") if isinstance(transition.get("target"), dict) else {}
        event = transition.get("event") if isinstance(transition.get("event"), dict) else {}
        if source.get("scope") != scope or source.get("state") != source_state:
            continue
        if event.get("name") != event_name:
            continue
        if event_kind and event.get("kind") != event_kind:
            continue
        if target_state:
            if target.get("stay"):
                candidate_target_state = str(source.get("state") or "")
            else:
                candidate_target_state = str(target.get("state") or "")
            if target.get("scope") != scope or candidate_target_state != target_state:
                continue
        actions = normalize_actions(transition.get("actions", []))
        transition_id = transition_identifier(
            resolved_machine_id,
            scope,
            source_state,
            event_name,
            target,
        )
        matches.append(
            TransitionPlan(
                machine_id=resolved_machine_id,
                scope=scope,
                transition_id=transition_id,
                transition_index=index,
                event=event_name,
                event_kind=str(event.get("kind") or ""),
                source=dict(source),
                target=dict(target),
                guard=str(transition.get("guard") or ""),
                actions=actions,
                summary=collect_action_summary(actions),
            )
        )

    if not matches:
        target_suffix = f" -> {target_state}" if target_state else ""
        raise StateMachineRuntimeError(
            f"transition not found: {resolved_machine_id}.{scope}.{source_state} --{event_name}-->{target_suffix}"
        )
    if len(matches) > 1:
        ids = ", ".join(item.transition_id for item in matches)
        raise StateMachineRuntimeError(f"ambiguous transition for {event_name!r}: {ids}")
    return matches[0]


def record_transition_execution(
    system: MemorySystem,
    *,
    machine_id: str,
    scope: str,
    source_state: str,
    event_name: str,
    target_state: str = "",
    event_kind: str = "",
    status: str = "passed",
    produced_evidence: tuple[str, ...] = (),
    completed_steps: tuple[str, ...] = (),
    emitted_events: tuple[str, ...] = (),
    note: str = "",
) -> TransitionExecution:
    plan = select_transition(
        system,
        machine_id=machine_id,
        scope=scope,
        source_state=source_state,
        event_name=event_name,
        target_state=target_state,
        event_kind=event_kind,
    )
    return TransitionExecution(
        plan=plan,
        status=status,
        produced_evidence=produced_evidence,
        completed_steps=completed_steps,
        emitted_events=emitted_events,
        note=note,
    )


def collect_action_summary(actions: Any) -> dict[str, tuple[str, ...]]:
    collected: dict[str, list[str]] = {
        "steps": [],
        "expects": [],
        "emits": [],
        "spawns": [],
        "waits": [],
        "sets": [],
        "action_kinds": [],
    }
    for action in normalize_actions(actions):
        collect_action(action, collected)
    return {key: tuple(values) for key, values in collected.items()}


def normalize_actions(actions: Any) -> tuple[dict[str, Any], ...]:
    if isinstance(actions, dict):
        return (actions,)
    if isinstance(actions, list):
        return tuple(action for action in actions if isinstance(action, dict))
    if isinstance(actions, tuple):
        return tuple(action for action in actions if isinstance(action, dict))
    return ()


def collect_action(action: dict[str, Any], collected: dict[str, list[str]]) -> None:
    kind = str(action.get("kind") or "").strip()
    append_unique(collected["action_kinds"], kind)
    if kind == "step":
        append_unique(collected["steps"], str(action.get("name") or ""))
    elif kind == "expect":
        append_unique(collected["expects"], str(action.get("evidence") or ""))
    elif kind == "emit":
        append_unique(collected["emits"], str(action.get("event") or ""))
    elif kind == "spawn":
        append_unique(collected["spawns"], str(action.get("job") or ""))
        for expected in action.get("expects", []) if isinstance(action.get("expects"), list) else []:
            append_unique(collected["expects"], str(expected))
    elif kind == "wait_for":
        append_unique(collected["waits"], str(action.get("event") or ""))
    elif kind == "set_state":
        append_unique(collected["sets"], format_scope_state(action))
    for child in action.get("actions", []) if isinstance(action.get("actions"), list) else []:
        if isinstance(child, dict):
            collect_action(child, collected)


def transition_identifier(
    machine_id: str,
    scope: str,
    source_state: str,
    event_name: str,
    target: dict[str, Any],
) -> str:
    if target.get("stay"):
        target_state = source_state
    else:
        target_state = str(target.get("state") or "")
    return f"{machine_id}.{scope}.{source_state}--{event_name}--{target_state}"


def format_scope_state(action: dict[str, Any]) -> str:
    scope = str(action.get("scope") or "")
    state = str(action.get("state") or "")
    if scope and state:
        return f"{scope}.{state}"
    return state or scope


def append_unique(items: list[str], value: str) -> None:
    cleaned = value.strip()
    if cleaned and cleaned not in items:
        items.append(cleaned)


def safe_component(value: str) -> str:
    cleaned = re.sub(r"[^A-Za-z0-9._-]+", "-", str(value).strip()).strip(".-")
    return cleaned or "default"
