from __future__ import annotations

import re
from typing import Any


class StateMachineDiagramError(RuntimeError):
    """Raised when a state-machine IR cannot be rendered as a diagram."""


def state_machine_to_mermaid(machine: dict[str, Any], *, scope_name: str = "") -> str:
    """Render a compiled state-machine IR as a compact Mermaid transition diagram."""
    scope = select_scope(machine, scope_name=scope_name)
    scope_id = str(scope["name"])
    states = list(scope.get("states", []))
    if not states:
        raise StateMachineDiagramError(f"scope {scope_id!r} has no states")

    title = str(machine.get("title") or machine.get("id") or "State Machine")
    if scope_name:
        title = f"{title} / {scope_id}"
    lines = [
        f"%% title: {escape_metadata(title)} IR",
        "%% description: Generated from compiled 1Context state-machine IR; edit the DSL source, not this diagram.",
        "%% order: 40",
        "flowchart LR",
    ]

    initial = str(scope.get("initial") or "")
    for state in states:
        node = state_node_id(scope_id, str(state))
        label = html_escape(str(state))
        if state == initial:
            lines.append(f'  {node}(["{label}<br/><br/>initial"])')
        else:
            lines.append(f'  {node}["{label}"]')

    emitted_events: list[tuple[str, str, str]] = []
    for index, transition in enumerate(machine.get("transitions", []), start=1):
        source = transition.get("source") or {}
        target = transition.get("target") or {}
        event = transition.get("event") or {}
        event_label = event_name(event)
        action_label = summarize_actions(transition.get("actions", []))
        label = edge_label(event_label, action_label)

        if source.get("scope") and source.get("scope") != scope_id:
            continue

        if target.get("stay"):
            from_state = str(source.get("state") or initial or states[0])
            to_state = from_state
        elif target.get("scope") == scope_id:
            to_state = str(target.get("state") or "")
            from_state = str(source.get("state") or infer_from_state(event_label, to_state, states, initial))
        else:
            continue

        if not from_state or not to_state:
            continue
        edge = "-->"
        if str(event.get("kind")) == "signal":
            edge = "-.->"
        lines.append(
            f"  {state_node_id(scope_id, from_state)} {edge}|\"{label}\"| {state_node_id(scope_id, to_state)}"
        )
        for emitted in emitted_action_events(transition.get("actions", [])):
            emitted_events.append((f"emit_{index}_{len(emitted_events)}", event_label, emitted))

    if emitted_events:
        lines.append("")
        lines.append("  subgraph emitted_events[\"Emitted events\"]")
        for node_id, source, emitted in emitted_events:
            lines.append(f'    {node_id}["{html_escape(emitted)}"]')
        lines.append("  end")

    signals = machine.get("signals", [])
    if signals:
        lines.append("")
        lines.append("  subgraph signals[\"Signals\"]")
        for signal in signals:
            name = str(signal.get("name", "signal"))
            expr = shorten(str(signal.get("expr", "")), 120)
            lines.append(f'    {safe_id("signal_" + name)}["{html_escape(name)}<br/><br/>{html_escape(expr)}"]')
        lines.append("  end")

    lines.append("")
    lines.append("  classDef state fill:#eef6ff,stroke:#2f6fa3,stroke-width:1px,color:#102033;")
    lines.append("  classDef initial fill:#eaf8ef,stroke:#2d7d46,stroke-width:2px,color:#102033;")
    for state in states:
        node = state_node_id(scope_id, str(state))
        lines.append(f"  class {node} {'initial' if state == initial else 'state'};")

    return "\n".join(line.rstrip() for line in lines) + "\n"


def select_scope(machine: dict[str, Any], *, scope_name: str = "") -> dict[str, Any]:
    scopes = machine.get("scopes", [])
    if not scopes:
        raise StateMachineDiagramError("state machine has no scopes")
    if scope_name:
        for scope in scopes:
            if scope.get("name") == scope_name:
                return scope
        raise StateMachineDiagramError(f"scope {scope_name!r} not found")
    for transition in machine.get("transitions", []):
        target = transition.get("target") or {}
        target_scope = target.get("scope")
        if target_scope:
            for scope in scopes:
                if scope.get("name") == target_scope:
                    return scope
    return scopes[0]


def infer_from_state(event_label: str, to_state: str, states: list[str], initial: str) -> str:
    index = states.index(to_state) if to_state in states else -1
    if index > 0:
        return states[index - 1]
    return initial or states[0]


def summarize_actions(actions: Any) -> str:
    labels: list[str] = []
    collect_action_labels(actions, labels)
    if not labels:
        return ""
    return "<br/>".join(labels[:12])


def collect_action_labels(actions: Any, labels: list[str]) -> None:
    if isinstance(actions, dict):
        kind = str(actions.get("kind", ""))
        if kind == "step":
            labels.append(f"step: {actions.get('name', '')}")
        elif kind == "spawn":
            target = str(actions.get("for_each") or "")
            labels.append(f"spawn: {actions.get('job', '')}{' x ' + target if target else ''}")
        elif kind == "expect":
            labels.append(f"expect: {actions.get('evidence', '')}")
        elif kind == "emit":
            labels.append(f"emit: {actions.get('event', '')}")
        elif kind == "parallel":
            labels.append(f"parallel: max {actions.get('max_concurrent', 'unbounded')}")
        for child in actions.get("actions", []):
            collect_action_labels(child, labels)
        return
    if isinstance(actions, list):
        for action in actions:
            collect_action_labels(action, labels)


def emitted_action_events(actions: Any) -> list[str]:
    events: list[str] = []
    collect_emits(actions, events)
    return events


def collect_emits(actions: Any, events: list[str]) -> None:
    if isinstance(actions, dict):
        if actions.get("kind") == "emit":
            events.append(str(actions.get("event", "")))
        for child in actions.get("actions", []):
            collect_emits(child, events)
        return
    if isinstance(actions, list):
        for action in actions:
            collect_emits(action, events)


def event_name(event: dict[str, Any]) -> str:
    return str(event.get("name") or event.get("kind") or "event")


def edge_label(event: str, actions: str) -> str:
    pieces = [html_escape(event)]
    if actions:
        pieces.append(html_escape(actions))
    return "<br/><br/>".join(pieces)


def state_node_id(scope: str, state: str) -> str:
    return safe_id(f"{scope}_{state}")


def safe_id(value: str) -> str:
    cleaned = re.sub(r"[^a-zA-Z0-9_]", "_", value)
    if not cleaned or cleaned[0].isdigit():
        cleaned = f"n_{cleaned}"
    return cleaned


def html_escape(value: str) -> str:
    return value.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;").replace('"', "&quot;")


def escape_metadata(value: str) -> str:
    return value.replace("\n", " ").strip()


def shorten(value: str, limit: int) -> str:
    if len(value) <= limit:
        return value
    return value[: limit - 3].rstrip() + "..."
