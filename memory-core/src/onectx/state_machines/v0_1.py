from __future__ import annotations

from typing import Any

from . import (
    Action,
    ArtifactSpec,
    EvidenceSpec,
    EventSpec,
    LANGUAGE_ID,
    LanguageRuntime,
    Machine as BaseMachine,
    ScopeSpec,
    emit,
    event,
    expect,
    parallel,
    race,
    retry,
    sequence,
    set_state,
    signal_edge,
    spawn,
    step,
    tick,
    timeout,
    wait_for,
)

RUNTIME = LanguageRuntime(
    id=LANGUAGE_ID,
    version="0.1.0",
    compatible_spec=">=0.1.0,<0.2.0",
)


class Machine(BaseMachine):
    def __init__(
        self,
        machine_id: str,
        *,
        version: str,
        title: str = "",
        description: str = "",
        **kwargs: Any,
    ) -> None:
        kwargs.setdefault("language", RUNTIME)
        super().__init__(
            machine_id,
            version=version,
            title=title,
            description=description,
            **kwargs,
        )


__all__ = [
    "Action",
    "ArtifactSpec",
    "EvidenceSpec",
    "EventSpec",
    "Machine",
    "RUNTIME",
    "ScopeSpec",
    "emit",
    "event",
    "expect",
    "parallel",
    "race",
    "retry",
    "sequence",
    "set_state",
    "signal_edge",
    "spawn",
    "step",
    "tick",
    "timeout",
    "wait_for",
]
