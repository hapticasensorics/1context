from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
from typing import Any

from onectx.config import MemorySystem
from onectx.memory.jobs import MemoryJobError, prepare_memory_job
from onectx.memory.runner import HiredAgentExecutionResult, HiredAgentRunnerError, execute_hired_agent
from onectx.memory.talk import validate_talk_entry
from onectx.storage import LakeStore


class HourlyScribeLabError(RuntimeError):
    """Raised when the hourly scribe lab runner cannot complete."""


@dataclass(frozen=True)
class HourlyScribeLabResult:
    talk_folder: Path
    execution: HiredAgentExecutionResult

    def to_payload(self) -> dict[str, Any]:
        payload = self.execution.to_payload()
        payload["talk_folder"] = str(self.talk_folder)
        payload["claude_returncode"] = payload.pop("returncode")
        payload["claude_stdout_path"] = payload.pop("stdout_path")
        payload["claude_stderr_path"] = payload.pop("stderr_path")
        return payload

    @property
    def dry_run(self) -> bool:
        return bool(self.execution.dry_run)

    @property
    def workspace(self) -> Path:
        return self.execution.workspace

    @property
    def output_path(self) -> Path:
        return self.execution.output_path

    @property
    def prompt_path(self) -> Path:
        return self.execution.prompt_path

    @property
    def experience_packet(self) -> dict[str, Any]:
        return self.execution.experience_packet

    @property
    def hire(self) -> dict[str, Any]:
        return self.execution.hire

    @property
    def validation(self) -> dict[str, Any]:
        return self.execution.validation

    @property
    def claude_returncode(self) -> int | None:
        return self.execution.returncode

    @property
    def claude_stdout_path(self) -> Path | None:
        return self.execution.stdout_path

    @property
    def claude_stderr_path(self) -> Path | None:
        return self.execution.stderr_path


def run_hourly_scribe_lab(
    system: MemorySystem,
    *,
    date: str,
    hour: str,
    audience: str = "private",
    workspace: Path | None = None,
    run_claude: bool = False,
    model: str = "opus",
) -> HourlyScribeLabResult:
    hour_int = int(hour)
    workspace = workspace or Path("/tmp") / "onecontext-hourly-scribe-demo"
    page_slug = f"for-you-{date}"
    talk_folder = workspace / f"{page_slug}.{audience}.talk"
    talk_folder.mkdir(parents=True, exist_ok=True)
    output_filename = f"{date}T{hour_int:02d}-00Z.conversation.md"
    output_path = talk_folder / output_filename
    if run_claude and output_path.exists():
        output_path.unlink()

    try:
        prepared = prepare_memory_job(
            system,
            job_id="memory.hourly.scribe",
            params={
                "date": date,
                "hour": f"{hour_int:02d}",
                "audience": audience,
                "talk_folder": str(talk_folder),
                "output_path": str(output_path),
            },
            workspace=workspace,
            run_harness=run_claude,
            model=model,
            run_id="hourly-scribe-lab",
            completed_event="memory.hourly_scribe.lab_completed",
            validator=lambda path: validate_talk_entry(path, expected_ts=f"{date}T{hour_int:02d}:00:00Z"),
        )
        execution = execute_hired_agent(
            system,
            prepared.execution_spec,
        )
    except (HiredAgentRunnerError, MemoryJobError) as exc:
        raise HourlyScribeLabError(str(exc)) from exc

    if run_claude and execution.validation["ok"]:
        store = LakeStore(system.storage_dir)
        artifact = store.append_artifact(
            "hourly_talk_entry",
            path=str(output_path),
            source="hourly-scribe-lab",
            state="produced",
            text=output_path.read_text(encoding="utf-8"),
            metadata={
                "hired_agent_uuid": execution.hire["hired_agent_uuid"],
                "experience_sha256": execution.experience_packet.get("experience_sha256"),
            },
        )
        store.append_evidence(
            "hourly_talk_entry.valid",
            artifact_id=artifact["artifact_id"],
            checker="hourly-scribe-lab",
            status="passed",
            text="Hourly talk entry passed mechanical validation.",
            checks=execution.validation["checks"],
            payload={"hired_agent_uuid": execution.hire["hired_agent_uuid"]},
        )

    return HourlyScribeLabResult(talk_folder=talk_folder, execution=execution)
