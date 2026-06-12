from __future__ import annotations

import hashlib
import importlib.util
import sys
from collections.abc import Iterable, Mapping
from dataclasses import dataclass, field
from pathlib import Path
from types import ModuleType
from typing import Any

LANGUAGE_ID = "ai_state_machine"
LANGUAGE_VERSION = "0.1.0"
LANGUAGE_COMPATIBLE_SPEC = ">=0.1.0,<0.2.0"


class StateMachineError(RuntimeError):
    """Raised when a state-machine definition cannot be loaded."""


@dataclass(frozen=True)
class LanguageRuntime:
    id: str
    version: str
    compatible_spec: str

    def to_ir(self) -> dict[str, str]:
        return {
            "id": self.id,
            "version": self.version,
            "compatible_spec": self.compatible_spec,
        }


LANGUAGE_RUNTIME = LanguageRuntime(
    id=LANGUAGE_ID,
    version=LANGUAGE_VERSION,
    compatible_spec=LANGUAGE_COMPATIBLE_SPEC,
)
LANGUAGE_RUNTIMES: dict[tuple[str, str], LanguageRuntime] = {
    (LANGUAGE_RUNTIME.id, LANGUAGE_RUNTIME.version): LANGUAGE_RUNTIME,
}


@dataclass(frozen=True)
class EventSpec:
    kind: str
    name: str
    match: dict[str, Any] = field(default_factory=dict)

    def to_ir(self) -> dict[str, Any]:
        payload: dict[str, Any] = {"kind": self.kind, "name": self.name}
        if self.match:
            payload["match"] = dict(self.match)
        return payload


@dataclass(frozen=True)
class Action:
    kind: str
    data: dict[str, Any] = field(default_factory=dict)
    children: tuple["Action", ...] = ()

    def to_ir(self) -> dict[str, Any]:
        payload: dict[str, Any] = {"kind": self.kind}
        payload.update(serialize(self.data))
        if self.children:
            payload["actions"] = [child.to_ir() for child in self.children]
        return payload


@dataclass(frozen=True)
class ScopeSpec:
    name: str
    key: str
    states: tuple[str, ...]
    initial: str | None = None
    description: str = ""

    def to_ir(self) -> dict[str, Any]:
        payload: dict[str, Any] = {
            "name": self.name,
            "key": self.key,
            "states": list(self.states),
        }
        if self.initial:
            payload["initial"] = self.initial
        if self.description:
            payload["description"] = self.description
        return payload


@dataclass(frozen=True)
class ArtifactSpec:
    name: str
    kind: str = ""
    path: str = ""
    schema: str = ""
    policies: tuple[str, ...] = ()
    description: str = ""

    def to_ir(self) -> dict[str, Any]:
        payload: dict[str, Any] = {"name": self.name}
        if self.kind:
            payload["kind"] = self.kind
        if self.path:
            payload["path"] = self.path
        if self.schema:
            payload["schema"] = self.schema
        if self.policies:
            payload["policies"] = list(self.policies)
        if self.description:
            payload["description"] = self.description
        return payload


@dataclass(frozen=True)
class EvidenceSpec:
    name: str
    artifact: str = ""
    checks: tuple[str, ...] = ()
    requires: tuple[str, ...] = ()
    description: str = ""

    def to_ir(self) -> dict[str, Any]:
        payload: dict[str, Any] = {"name": self.name}
        if self.artifact:
            payload["artifact"] = self.artifact
        if self.checks:
            payload["checks"] = list(self.checks)
        if self.requires:
            payload["requires"] = list(self.requires)
        if self.description:
            payload["description"] = self.description
        return payload


class Machine:
    """Python-embedded authoring surface for versioned AI state machines."""

    def __init__(
        self,
        machine_id: str,
        *,
        version: str,
        title: str = "",
        description: str = "",
        language: LanguageRuntime | None = None,
    ) -> None:
        self.id = machine_id
        self.version = version
        self.title = title
        self.description = description
        self.language = language or LANGUAGE_RUNTIME
        self._scopes: list[ScopeSpec] = []
        self._clocks: list[dict[str, Any]] = []
        self._artifacts: list[ArtifactSpec] = []
        self._evidence: list[EvidenceSpec] = []
        self._signals: list[dict[str, Any]] = []
        self._transitions: list[dict[str, Any]] = []

    def scope(
        self,
        name: str,
        *,
        key: str,
        states: Iterable[str],
        initial: str | None = None,
        description: str = "",
    ) -> ScopeSpec:
        spec = ScopeSpec(
            name=name,
            key=key,
            states=tuple(states),
            initial=initial,
            description=description,
        )
        self._scopes.append(spec)
        return spec

    def clock(self, name: str, **definition: Any) -> None:
        payload = {"name": name}
        payload.update(definition)
        self._clocks.append(payload)

    def artifact(
        self,
        name: str,
        *,
        kind: str = "",
        path: str = "",
        schema: str = "",
        policies: Iterable[str] = (),
        description: str = "",
    ) -> ArtifactSpec:
        if any(item.name == name for item in self._artifacts):
            raise StateMachineError(f"duplicate artifact {name!r} in state machine {self.id!r}")
        spec = ArtifactSpec(
            name=name,
            kind=kind,
            path=path,
            schema=schema,
            policies=tuple(policies),
            description=description,
        )
        self._artifacts.append(spec)
        return spec

    def evidence(
        self,
        name: str,
        *,
        artifact: str = "",
        checks: Iterable[str] = (),
        requires: Iterable[str] = (),
        description: str = "",
    ) -> EvidenceSpec:
        if any(item.name == name for item in self._evidence):
            raise StateMachineError(f"duplicate evidence {name!r} in state machine {self.id!r}")
        spec = EvidenceSpec(
            name=name,
            artifact=artifact,
            checks=tuple(checks),
            requires=tuple(requires),
            description=description,
        )
        self._evidence.append(spec)
        return spec

    def signal(self, name: str, *, expr: str, reads: Iterable[str] = ()) -> None:
        payload: dict[str, Any] = {"name": name, "expr": expr}
        read_list = list(reads)
        if read_list:
            payload["reads"] = read_list
        self._signals.append(payload)

    def on(self, event_spec: EventSpec | str) -> "RuleBuilder":
        return RuleBuilder(self, coerce_event(event_spec))

    def from_(self, scope: ScopeSpec | str, state: str, *, key: str | None = None) -> "SourceRuleBuilder":
        return SourceRuleBuilder(self, target(scope, state, key=key))

    def add_transition(
        self,
        *,
        event_spec: EventSpec,
        source: dict[str, Any] | None = None,
        guard: str = "",
        target: dict[str, Any] | None = None,
        actions: Iterable[Action] = (),
    ) -> None:
        payload: dict[str, Any] = {"event": event_spec.to_ir()}
        if source:
            payload["source"] = source
        if guard:
            payload["guard"] = guard
        if target:
            payload["target"] = target
        action_list = [action.to_ir() for action in actions]
        if action_list:
            payload["actions"] = action_list
        self._transitions.append(payload)

    def to_ir(self) -> dict[str, Any]:
        return {
            "id": self.id,
            "version": self.version,
            "language": self.language.to_ir(),
            "title": self.title or self.id,
            "description": self.description,
            "scopes": [scope.to_ir() for scope in self._scopes],
            "clocks": [serialize(clock) for clock in self._clocks],
            "artifacts": [artifact.to_ir() for artifact in self._artifacts],
            "evidence": [evidence.to_ir() for evidence in self._evidence],
            "signals": [serialize(signal) for signal in self._signals],
            "transitions": [serialize(transition) for transition in self._transitions],
        }


class SourceRuleBuilder:
    def __init__(self, machine: Machine, source: dict[str, Any]) -> None:
        self.machine = machine
        self.source = source

    def on(self, event_spec: EventSpec | str) -> "RuleBuilder":
        return RuleBuilder(self.machine, coerce_event(event_spec), source=self.source)


class RuleBuilder:
    def __init__(self, machine: Machine, event_spec: EventSpec, *, source: dict[str, Any] | None = None) -> None:
        self.machine = machine
        self.event_spec = event_spec
        self.guard = ""
        self.source = source

    def when(self, guard: str) -> "RuleBuilder":
        self.guard = guard
        return self

    def do(self, *actions: Action) -> Machine:
        self.machine.add_transition(
            event_spec=self.event_spec,
            source=self.source,
            guard=self.guard,
            actions=actions,
        )
        return self.machine

    def to(
        self,
        scope: ScopeSpec | str,
        state: str,
        *,
        key: str | None = None,
        do: Action | Iterable[Action] | None = None,
    ) -> Machine:
        self.machine.add_transition(
            event_spec=self.event_spec,
            source=self.source,
            guard=self.guard,
            target=target(scope, state, key=key),
            actions=normalize_actions(do),
        )
        return self.machine

    def stay(self, *, do: Action | Iterable[Action] | None = None) -> Machine:
        self.machine.add_transition(
            event_spec=self.event_spec,
            source=self.source,
            guard=self.guard,
            target=stay_target(self.source),
            actions=normalize_actions(do),
        )
        return self.machine


def event(name: str, **match: Any) -> EventSpec:
    return EventSpec("event", name, dict(match))


def tick(name: str, **match: Any) -> EventSpec:
    return EventSpec("tick", name, dict(match))


def signal_edge(name: str, **match: Any) -> EventSpec:
    return EventSpec("signal", name, dict(match))


def step(name: str, **params: Any) -> Action:
    data: dict[str, Any] = {"name": name}
    if params:
        data["params"] = params
    return Action("step", data)


def spawn(
    job: str,
    *,
    params: Mapping[str, Any] | None = None,
    for_each: str = "",
    key: str = "",
    grants: Iterable[str] = (),
    denies: Iterable[str] = (),
    expects: Iterable[str] = (),
) -> Action:
    data: dict[str, Any] = {"job": job}
    if params:
        data["params"] = dict(params)
    if for_each:
        data["for_each"] = for_each
    if key:
        data["key"] = key
    grants_list = list(grants)
    denies_list = list(denies)
    expects_list = list(expects)
    if grants_list:
        data["grants"] = grants_list
    if denies_list:
        data["denies"] = denies_list
    if expects_list:
        data["expects"] = expects_list
    return Action("spawn", data)


def expect(evidence: str, **details: Any) -> Action:
    data = {"evidence": evidence}
    data.update(details)
    return Action("expect", data)


def wait_for(name: str, *, timeout: str = "") -> Action:
    data = {"event": name}
    if timeout:
        data["timeout"] = timeout
    return Action("wait_for", data)


def emit(name: str, **payload: Any) -> Action:
    data = {"event": name}
    if payload:
        data["payload"] = payload
    return Action("emit", data)


def set_state(scope: ScopeSpec | str, state: str, *, key: str | None = None) -> Action:
    return Action("set_state", target(scope, state, key=key))


def sequence(*actions: Action) -> Action:
    return Action("sequence", children=tuple(actions))


def parallel(*actions: Action, fail: str = "collect", max_concurrent: int | str | None = None) -> Action:
    data: dict[str, Any] = {"fail": fail}
    if max_concurrent is not None:
        data["max_concurrent"] = max_concurrent
    return Action("parallel", data, tuple(actions))


def race(*actions: Action) -> Action:
    return Action("race", children=tuple(actions))


def retry(action: Action, *, attempts: int, backoff: str = "") -> Action:
    data: dict[str, Any] = {"attempts": attempts}
    if backoff:
        data["backoff"] = backoff
    return Action("retry", data, (action,))


def timeout(action: Action, *, after: str) -> Action:
    return Action("timeout", {"after": after}, (action,))


def load_state_machine_dir(
    path: Path,
    *,
    language_runtime: LanguageRuntime | None = None,
) -> dict[str, dict[str, Any]]:
    machines: dict[str, dict[str, Any]] = {}
    runtime = language_runtime or LANGUAGE_RUNTIME
    if not path.is_dir():
        return machines
    for source in sorted(path.glob("*.py")):
        if source.name.startswith("_"):
            continue
        for index, machine_ir in enumerate(load_state_machine_file(source, language_runtime=runtime)):
            machine_id = str(machine_ir.get("id", "")).strip()
            if not machine_id:
                raise StateMachineError(f"{source} yielded a machine without id")
            if machine_id in machines:
                raise StateMachineError(f"duplicate state-machine id {machine_id!r} in {source}")
            machine_ir["id"] = machine_id
            machine_ir["source_path"] = str(source)
            machine_ir["source_index"] = index
            machines[machine_id] = machine_ir
    return machines


def machine_source_files(path: Path) -> list[Path]:
    if not path.is_dir():
        return []
    return [source for source in sorted(path.glob("*.py")) if not source.name.startswith("_")]


def available_language_runtimes(language: str | None = None) -> list[dict[str, str]]:
    runtimes = [
        runtime
        for runtime in LANGUAGE_RUNTIMES.values()
        if language is None or runtime.id == language
    ]
    return [
        runtime.to_ir()
        for runtime in sorted(runtimes, key=lambda item: parse_version(item.version), reverse=True)
    ]


def select_language_runtime(
    language: str,
    *,
    version_spec: str = "",
    version: str = "",
) -> LanguageRuntime:
    if version:
        runtime = LANGUAGE_RUNTIMES.get((language, version))
        if runtime is None:
            raise StateMachineError(f"state-machine language {language!r} version {version!r} is not available")
        return runtime

    candidates = [
        runtime
        for (runtime_language, _), runtime in LANGUAGE_RUNTIMES.items()
        if runtime_language == language and language_satisfies(version_spec, version=runtime.version)
    ]
    if not candidates:
        available = ", ".join(
            f"{item['id']} {item['version']}" for item in available_language_runtimes(language)
        ) or "none"
        spec = f" {version_spec}" if version_spec else ""
        raise StateMachineError(
            f"no available state-machine language satisfies {language!r}{spec}; available: {available}"
        )
    return sorted(candidates, key=lambda item: parse_version(item.version), reverse=True)[0]


def language_satisfies(version_spec: str, *, version: str = LANGUAGE_VERSION) -> bool:
    """Return whether this runtime version satisfies a small comma-separated range."""
    spec = version_spec.strip()
    if not spec:
        return True
    current = parse_version(version)
    for clause in (part.strip() for part in spec.split(",")):
        if not clause:
            continue
        op = "=="
        expected_raw = clause
        for candidate in (">=", "<=", "==", ">", "<", "="):
            if clause.startswith(candidate):
                op = "==" if candidate == "=" else candidate
                expected_raw = clause[len(candidate) :].strip()
                break
        expected = parse_version(expected_raw)
        if op == "==" and current != expected:
            return False
        if op == ">=" and current < expected:
            return False
        if op == "<=" and current > expected:
            return False
        if op == ">" and current <= expected:
            return False
        if op == "<" and current >= expected:
            return False
    return True


def parse_version(value: str) -> tuple[int, int, int]:
    cleaned = value.strip().lstrip("v")
    if not cleaned:
        raise StateMachineError("empty state-machine language version")
    parts = cleaned.split(".")
    if len(parts) > 3:
        raise StateMachineError(f"unsupported state-machine language version {value!r}")
    numbers: list[int] = []
    for part in parts:
        if not part.isdigit():
            raise StateMachineError(f"unsupported state-machine language version {value!r}")
        numbers.append(int(part))
    while len(numbers) < 3:
        numbers.append(0)
    return tuple(numbers)


def load_state_machine_file(
    path: Path,
    *,
    language_runtime: LanguageRuntime | None = None,
) -> list[dict[str, Any]]:
    runtime = language_runtime or LANGUAGE_RUNTIME
    module = load_python_module(path)
    if hasattr(module, "build"):
        return normalize_machine_result(module.build(), path, language_runtime=runtime)
    if hasattr(module, "MACHINES"):
        return normalize_machine_result(module.MACHINES, path, language_runtime=runtime)
    if hasattr(module, "machine"):
        return normalize_machine_result(module.machine, path, language_runtime=runtime)
    raise StateMachineError(f"{path} must expose build(), MACHINES, or machine")


def load_python_module(path: Path) -> ModuleType:
    digest = hashlib.sha256(str(path.resolve()).encode("utf-8")).hexdigest()[:12]
    module_name = f"_onectx_state_machine_{digest}"
    spec = importlib.util.spec_from_file_location(module_name, path)
    if spec is None or spec.loader is None:
        raise StateMachineError(f"cannot import state-machine file {path}")
    module = importlib.util.module_from_spec(spec)
    previous_dont_write_bytecode = sys.dont_write_bytecode
    sys.dont_write_bytecode = True
    try:
        spec.loader.exec_module(module)
    except Exception as exc:  # pragma: no cover - error path preserves source file context.
        raise StateMachineError(f"error loading {path}: {exc}") from exc
    finally:
        sys.dont_write_bytecode = previous_dont_write_bytecode
    return module


def normalize_machine_result(
    value: Any,
    path: Path,
    *,
    language_runtime: LanguageRuntime | None = None,
) -> list[dict[str, Any]]:
    runtime = language_runtime or LANGUAGE_RUNTIME
    values: Iterable[Any]
    if isinstance(value, (Machine, dict)):
        values = (value,)
    elif isinstance(value, Iterable) and not isinstance(value, (str, bytes)):
        values = value
    else:
        raise StateMachineError(f"{path} returned unsupported state-machine value {type(value).__name__}")

    result = []
    for item in values:
        if isinstance(item, Machine):
            machine_ir = item.to_ir()
        elif isinstance(item, dict):
            machine_ir = dict(item)
        else:
            raise StateMachineError(f"{path} yielded unsupported state-machine value {type(item).__name__}")
        result.append(validate_machine_language(machine_ir, path, runtime))
    return result


def validate_machine_language(
    machine_ir: dict[str, Any],
    path: Path,
    runtime: LanguageRuntime,
) -> dict[str, Any]:
    language = machine_ir.get("language")
    if language in (None, {}):
        machine_ir["language"] = runtime.to_ir()
        return machine_ir
    if not isinstance(language, dict):
        raise StateMachineError(f"{path} machine {machine_ir.get('id', '<unknown>')} has invalid language metadata")
    language_id = str(language.get("id", "")).strip()
    version = str(language.get("version", "")).strip()
    if language_id != runtime.id or version != runtime.version:
        raise StateMachineError(
            f"{path} machine {machine_ir.get('id', '<unknown>')} was authored with "
            f"{language_id or '<missing>'} {version or '<missing>'}, but plugin selected "
            f"{runtime.id} {runtime.version}"
        )
    machine_ir["language"] = runtime.to_ir()
    return machine_ir


def coerce_event(value: EventSpec | str) -> EventSpec:
    return value if isinstance(value, EventSpec) else event(str(value))


def normalize_actions(value: Action | Iterable[Action] | None) -> tuple[Action, ...]:
    if value is None:
        return ()
    if isinstance(value, Action):
        return (value,)
    return tuple(value)


def target(scope: ScopeSpec | str, state: str, *, key: str | None = None) -> dict[str, Any]:
    payload = {
        "scope": scope.name if isinstance(scope, ScopeSpec) else str(scope),
        "state": state,
    }
    if key is not None:
        payload["key"] = key
    return payload


def stay_target(source: dict[str, Any] | None = None) -> dict[str, Any]:
    payload: dict[str, Any] = {"stay": True}
    if source:
        if source.get("scope"):
            payload["scope"] = source.get("scope")
        if source.get("state"):
            payload["state"] = source.get("state")
        if source.get("key") is not None:
            payload["key"] = source.get("key")
    return payload


def serialize(value: Any) -> Any:
    if isinstance(value, Action):
        return value.to_ir()
    if isinstance(value, EventSpec):
        return value.to_ir()
    if isinstance(value, ScopeSpec):
        return value.to_ir()
    if isinstance(value, ArtifactSpec):
        return value.to_ir()
    if isinstance(value, EvidenceSpec):
        return value.to_ir()
    if isinstance(value, dict):
        return {str(key): serialize(item) for key, item in value.items()}
    if isinstance(value, (list, tuple)):
        return [serialize(item) for item in value]
    return value
