from __future__ import annotations

import json
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from onectx.config import MemorySystem
from onectx.memory.hour_experience import DEFAULT_EXPERIENCE_MODE, render_hour_experience, render_hour_experience_from_events
from onectx.memory.ledger import Ledger, ledger_events_path
from onectx.memory.prompt_stack import PromptPart, PromptStack, prompt_part_from_file
from onectx.memory.runner import ArtifactSpec, HarnessLaunchSpec, HiredAgentExecutionSpec
from onectx.memory.talk import ensure_for_you_talk_folder, read_talk_entries, sha256_text, simple_yaml
from onectx.storage import LakeStore, stable_id


CLAUDE_ACCOUNT_CLEAN_TOOLS = "default"
HOURLY_SCRIBE_JOB_ID = "memory.hourly.scribe"
HOURLY_BLOCK_SCRIBE_JOB_ID = "memory.hourly.block_scribe"
HOURLY_SHARD_SCRIBE_JOB_ID = "memory.hourly.shard_scribe"
HOURLY_AGGREGATE_SCRIBE_JOB_ID = "memory.hourly.aggregate_scribe"
DAILY_EDITOR_JOB_ID = "memory.daily.editor"
CONCEPT_SCOUT_JOB_ID = "memory.concept.scout"


class MemoryJobError(RuntimeError):
    """Raised when a manifest-driven memory job cannot be prepared."""


@dataclass(frozen=True)
class PreparedMemoryJob:
    job_id: str
    job_params: dict[str, Any]
    prompt_stack: PromptStack
    execution_spec: HiredAgentExecutionSpec
    talk_folder: Path | None = None


def prepare_memory_job(
    system: MemorySystem,
    *,
    job_id: str,
    params: dict[str, Any],
    workspace: Path,
    run_harness: bool,
    model: str | None = None,
    run_id: str | None = None,
    completed_event: str | None = None,
    validator: Any = None,
) -> PreparedMemoryJob:
    if job_id == HOURLY_SCRIBE_JOB_ID:
        return prepare_hourly_scribe_job(
            system,
            params=params,
            workspace=workspace,
            run_harness=run_harness,
            model=model,
            run_id=run_id or "memory-job",
            completed_event=completed_event or "memory.hourly_scribe.completed",
            validator=validator,
        )
    if job_id == HOURLY_BLOCK_SCRIBE_JOB_ID:
        return prepare_hourly_block_scribe_job(
            system,
            params=params,
            workspace=workspace,
            run_harness=run_harness,
            model=model,
            run_id=run_id or "memory-job",
            completed_event=completed_event or "memory.hourly_block_scribe.completed",
            validator=validator,
        )
    if job_id == HOURLY_SHARD_SCRIBE_JOB_ID:
        return prepare_hourly_shard_scribe_job(
            system,
            params=params,
            workspace=workspace,
            run_harness=run_harness,
            model=model,
            run_id=run_id or "memory-job",
            completed_event=completed_event or "memory.hourly_shard_scribe.completed",
            validator=validator,
        )
    if job_id == HOURLY_AGGREGATE_SCRIBE_JOB_ID:
        return prepare_hourly_aggregate_scribe_job(
            system,
            params=params,
            workspace=workspace,
            run_harness=run_harness,
            model=model,
            run_id=run_id or "memory-job",
            completed_event=completed_event or "memory.hourly_aggregate_scribe.completed",
            validator=validator,
        )
    if job_id in {DAILY_EDITOR_JOB_ID, CONCEPT_SCOUT_JOB_ID}:
        return prepare_talk_folder_job(
            system,
            job_id=job_id,
            params=params,
            workspace=workspace,
            run_harness=run_harness,
            model=model,
            run_id=run_id or "memory-job",
            completed_event=completed_event or f"{job_id}.completed",
            validator=validator,
        )
    raise MemoryJobError(
        "manifest runner currently supports "
        f"{HOURLY_SCRIBE_JOB_ID}, {HOURLY_BLOCK_SCRIBE_JOB_ID}, "
        f"{HOURLY_SHARD_SCRIBE_JOB_ID}, {HOURLY_AGGREGATE_SCRIBE_JOB_ID}, "
        f"{DAILY_EDITOR_JOB_ID}, and {CONCEPT_SCOUT_JOB_ID}; got {job_id!r}"
    )


def prepare_hourly_scribe_job(
    system: MemorySystem,
    *,
    params: dict[str, Any],
    workspace: Path,
    run_harness: bool,
    model: str | None,
    run_id: str,
    completed_event: str,
    validator: Any = None,
) -> PreparedMemoryJob:
    job = required_manifest(system.jobs, HOURLY_SCRIBE_JOB_ID, "job")
    agent = required_manifest(system.agents, str(job.get("agent", "")), "agent")
    harness_id = str(agent.get("harness") or "")
    provider_id = str(agent.get("provider") or "")
    resolved_model = str(model or agent.get("model") or "")
    if not harness_id or not provider_id or not resolved_model:
        raise MemoryJobError("hourly scribe agent must resolve harness, provider, and model")

    date = str(params["date"])
    hour_int = int(str(params["hour"]))
    hour = f"{hour_int:02d}"
    audience = str(params.get("audience", "private"))
    page_slug = str(params.get("page_slug", f"for-you-{date}"))
    talk_folder = Path(str(params.get("talk_folder") or workspace / f"{page_slug}.{audience}.talk"))
    ensure_for_you_talk_folder(talk_folder, date=date, audience=audience, page_slug=page_slug)
    output_path = Path(str(params.get("output_path") or talk_folder / f"{date}T{hour}-00Z.conversation.md"))
    if run_harness and output_path.exists():
        output_path.unlink()

    experience_config = job.get("experience", {})
    if not isinstance(experience_config, dict):
        raise MemoryJobError(f"{HOURLY_SCRIBE_JOB_ID} experience config must be a table")
    builder = str(experience_config.get("builder", ""))
    if builder != "render_hour_experience":
        raise MemoryJobError(f"unsupported experience builder {builder!r}")
    experience_mode = str(params.get("experience_mode") or experience_config.get("mode") or DEFAULT_EXPERIENCE_MODE)
    resolved_source_harnesses = source_harnesses(params.get("source_harnesses", ("codex", "claude-code")))
    hour_events = params.get("_hour_events")
    if hour_events is not None:
        rendered = render_hour_experience_from_events(
            system,
            date=date,
            hour=hour,
            events=list(hour_events),
            source_harnesses=resolved_source_harnesses,
            experience_mode=experience_mode,
        )
    else:
        rendered = render_hour_experience(
            system,
            date=date,
            hour=hour,
            source_harnesses=resolved_source_harnesses,
            experience_mode=experience_mode,
        )
    packet = rendered.to_packet()
    ledger = Ledger(ledger_events_path(system.runtime_dir), storage_path=system.storage_dir)
    ledger.append(
        "memory.hour_experience.rendered",
        ledger_schema_version="0.1",
        plugin_id=system.active_plugin,
        run_id=run_id,
        summary=f"Rendered braided lived experience for {date}T{hour}.",
        experience_packet=packet,
        outcome="done",
    )

    job_params = {
        **public_job_params(params),
        "date": date,
        "hour": hour,
        "audience": audience,
        "talk_folder": str(talk_folder),
        "output_path": str(output_path),
        "experience_id": rendered.experience_id,
        "experience_mode": experience_mode,
        "experience_sha256": rendered.experience_sha256,
        "agent_context_sha256": rendered.agent_context_sha256,
    }
    prompt_stack = build_hourly_scribe_prompt_stack(system, job=job, agent=agent, rendered=rendered, job_params=job_params)
    job_params["prompt_stack_sha256"] = prompt_stack.sha256

    isolation_mode = str(system.runtime_policy.get("default_harness_isolation", "account_clean"))
    if isolation_mode != "account_clean":
        raise MemoryJobError(f"hourly scribe currently supports account_clean, got {isolation_mode!r}")
    launch = HarnessLaunchSpec(
        harness=harness_id,
        isolation_mode=isolation_mode,
        argv=claude_account_clean_args(model=resolved_model, workspace=workspace, experience_path=rendered.path),
        cwd=workspace,
    )

    return PreparedMemoryJob(
        job_id=HOURLY_SCRIBE_JOB_ID,
        job_params=job_params,
        prompt_stack=prompt_stack,
        talk_folder=talk_folder,
        execution_spec=HiredAgentExecutionSpec(
            run_id=run_id,
            job_ids=[HOURLY_SCRIBE_JOB_ID],
            job_params=job_params,
            experience_packet=packet,
            prompt=prompt_stack.text,
            prompt_stack=prompt_stack.to_payload(),
            workspace=workspace,
            artifact=ArtifactSpec(kind="hourly_talk_entry", path=output_path),
            harness_launch=launch,
            harness_id=harness_id,
            provider_id=provider_id,
            model=resolved_model,
            run_harness=run_harness,
            completed_event=completed_event,
            validator=validator,
        ),
    )


def prepare_hourly_block_scribe_job(
    system: MemorySystem,
    *,
    params: dict[str, Any],
    workspace: Path,
    run_harness: bool,
    model: str | None,
    run_id: str,
    completed_event: str,
    validator: Any = None,
) -> PreparedMemoryJob:
    job = required_manifest(system.jobs, HOURLY_BLOCK_SCRIBE_JOB_ID, "job")
    agent = required_manifest(system.agents, str(job.get("agent", "")), "agent")
    harness_id = str(agent.get("harness") or "")
    provider_id = str(agent.get("provider") or "")
    resolved_model = str(model or agent.get("model") or "")
    if not harness_id or not provider_id or not resolved_model:
        raise MemoryJobError("hourly block scribe agent must resolve harness, provider, and model")

    date = str(params["date"])
    block_start = f"{int(str(params['block_start'])):02d}"
    block_end = f"{int(block_start) + 3:02d}"
    hours = normalize_hours(params.get("hours") or [f"{hour:02d}" for hour in range(int(block_start), int(block_start) + 4)])
    audience = str(params.get("audience", "private"))
    page_slug = str(params.get("page_slug", f"for-you-{date}"))
    talk_folder = Path(str(params.get("talk_folder") or workspace / f"{page_slug}.{audience}.talk"))
    ensure_for_you_talk_folder(talk_folder, date=date, audience=audience, page_slug=page_slug)
    manifest_path = Path(
        str(params.get("manifest_path") or talk_folder / f"{date}T{block_start}-{block_end}Z.block-result.json")
    )
    if run_harness and manifest_path.exists():
        manifest_path.unlink()

    experience_config = job.get("experience", {})
    if not isinstance(experience_config, dict):
        raise MemoryJobError(f"{HOURLY_BLOCK_SCRIBE_JOB_ID} experience config must be a table")
    experience_mode = str(params.get("experience_mode") or experience_config.get("mode") or DEFAULT_EXPERIENCE_MODE)
    resolved_source_harnesses = source_harnesses(params.get("source_harnesses", ("codex", "claude-code")))
    events_by_hour = params.get("_hour_events_by_hour") or {}

    rendered_hours = []
    for hour in hours:
        if hour in events_by_hour:
            rendered = render_hour_experience_from_events(
                system,
                date=date,
                hour=hour,
                events=list(events_by_hour[hour]),
                source_harnesses=resolved_source_harnesses,
                experience_mode=experience_mode,
            )
        else:
            rendered = render_hour_experience(
                system,
                date=date,
                hour=hour,
                source_harnesses=resolved_source_harnesses,
                experience_mode=experience_mode,
            )
        rendered_hours.append(rendered)

    block_id = f"block-{date}T{block_start}-{block_end}-{experience_mode}"
    block_dir = system.runtime_dir / "experiences" / safe_id(block_id)
    store = LakeStore(system.storage_dir)
    store.ensure()
    hour_context_hashes = [rendered.agent_context_sha256 for rendered in rendered_hours]
    block_artifact_id = stable_id("artifact", "runtime_block_experience_packet", block_id, experience_mode)
    cached_block = cached_block_experience(
        store,
        artifact_id=block_artifact_id,
        block_id=block_id,
        experience_mode=experience_mode,
        date=date,
        block_start=block_start,
        block_end=block_end,
        hours=hours,
        hour_context_hashes=hour_context_hashes,
    )
    if cached_block:
        block_context_path = cached_block["block_context_path"]
        block_context_text = block_context_path.read_text(encoding="utf-8")
        block_sha = str(cached_block["block_sha"])
        block_cache_hit = True
    else:
        block_dir.mkdir(parents=True, exist_ok=True)
        block_context_text = render_block_context(date=date, block_start=block_start, block_end=block_end, rendered_hours=rendered_hours)
        block_context_path = block_dir / "block-context.md"
        block_context_path.write_text(block_context_text, encoding="utf-8")
        block_sha = sha256_text(block_context_text)
        block_cache_hit = False
    packet = {
        "kind": "block_experience_packet",
        "experience_id": block_id,
        "experience_mode": experience_mode,
        "path": str(block_dir),
        "block_context_path": str(block_context_path),
        "block_context_sha256": block_sha,
        "date": date,
        "block_start": block_start,
        "block_end": block_end,
        "hours": hours,
        "hour_packets": [rendered.to_packet() for rendered in rendered_hours],
        "cache": {"hit": block_cache_hit},
    }
    (block_dir / "meta.yaml").write_text(simple_yaml(packet), encoding="utf-8")
    if not block_cache_hit:
        artifact = store.artifact_row(
            "runtime_block_experience_packet",
            artifact_id=block_artifact_id,
            uri=f"file://{block_dir}",
            path=str(block_context_path),
            content_type="text/markdown",
            content_hash=block_sha,
            bytes=len(block_context_text.encode("utf-8")),
            source="render_hour_experience",
            state="rendered",
            text=f"{experience_mode} {date}T{block_start}-{block_end}Z",
            metadata={
                "experience_id": block_id,
                "experience_mode": experience_mode,
                "date": date,
                "block_start": block_start,
                "block_end": block_end,
                "hours": list(hours),
                "source_windows": [rendered.source_window for rendered in rendered_hours],
                "hour_experience_ids": [rendered.experience_id for rendered in rendered_hours],
                "hour_context_sha256": hour_context_hashes,
                "projection": {
                    "source_truth": "lakestore.events",
                    "destructive": False,
                    "hour_packets_are_materialized": True,
                },
            },
        )
        store.replace_rows("artifacts", "artifact_id", [artifact])
    Ledger(ledger_events_path(system.runtime_dir), storage_path=system.storage_dir).append(
        "memory.block_experience.rendered",
        ledger_schema_version="0.1",
        plugin_id=system.active_plugin,
        run_id=run_id,
        summary=f"Rendered fixed 4-hour lived experience block for {date}T{block_start}-{block_end}.",
        experience_packet=packet,
        outcome="done",
    )

    job_params = {
        **public_job_params(params),
        "date": date,
        "block_start": block_start,
        "block_end": block_end,
        "hours": ",".join(hours),
        "audience": audience,
        "talk_folder": str(talk_folder),
        "manifest_path": str(manifest_path),
        "experience_id": block_id,
        "experience_mode": experience_mode,
        "block_context_sha256": block_sha,
    }
    prompt_stack = build_hourly_block_scribe_prompt_stack(
        system,
        job=job,
        agent=agent,
        block_context_text=block_context_text,
        block_context_path=block_context_path,
        job_params=job_params,
    )
    job_params["prompt_stack_sha256"] = prompt_stack.sha256

    isolation_mode = str(system.runtime_policy.get("default_harness_isolation", "account_clean"))
    if isolation_mode != "account_clean":
        raise MemoryJobError(f"hourly block scribe currently supports account_clean, got {isolation_mode!r}")
    launch = HarnessLaunchSpec(
        harness=harness_id,
        isolation_mode=isolation_mode,
        argv=claude_account_clean_args(model=resolved_model, workspace=workspace, experience_path=block_dir),
        cwd=workspace,
    )

    return PreparedMemoryJob(
        job_id=HOURLY_BLOCK_SCRIBE_JOB_ID,
        job_params=job_params,
        prompt_stack=prompt_stack,
        talk_folder=talk_folder,
        execution_spec=HiredAgentExecutionSpec(
            run_id=run_id,
            job_ids=[HOURLY_BLOCK_SCRIBE_JOB_ID],
            job_params=job_params,
            experience_packet=packet,
            prompt=prompt_stack.text,
            prompt_stack=prompt_stack.to_payload(),
            workspace=workspace,
            artifact=ArtifactSpec(kind="hourly_block_result", path=manifest_path),
            harness_launch=launch,
            harness_id=harness_id,
            provider_id=provider_id,
            model=resolved_model,
            run_harness=run_harness,
            completed_event=completed_event,
            validator=validator,
        ),
    )


def build_hourly_block_scribe_prompt_stack(
    system: MemorySystem,
    *,
    job: dict[str, Any],
    agent: dict[str, Any],
    block_context_text: str,
    block_context_path: Path,
    job_params: dict[str, Any],
) -> PromptStack:
    parts: list[PromptPart] = [
        PromptPart(
            name="system_addition",
            text=(
                "# 1Context Hired Agent System Addition\n\n"
                "You are a hired 1Context memory agent. You inherit a fixed block of past operational "
                "life for this job. The lived-experience context is loaded below before the task. "
                "Preserve hour boundaries. Forgetting and no-talk are valid memory decisions; do not pad."
            ),
        ),
        PromptPart(
            name="birth_loaded_block_experience",
            text="# Birth-Loaded Fixed Block Experience Attachment\n\n" + block_context_text,
            path=block_context_path,
        ),
    ]
    for index, relative_path in enumerate(agent.get("prompt_paths", [])):
        parts.append(prompt_part_from_file(f"agent_prompt_{index + 1}", system.plugin_path / str(relative_path)))
    task_values = {
        "date": job_params["date"],
        "block_start": job_params["block_start"],
        "block_end": job_params["block_end"],
        "hours": job_params["hours"],
        "talk_folder": job_params["talk_folder"],
        "manifest_path": job_params["manifest_path"],
    }
    for index, relative_path in enumerate(job.get("prompt_paths", [])):
        parts.append(
            prompt_part_from_file(
                f"job_prompt_{index + 1}",
                system.plugin_path / str(relative_path),
                format_values=task_values,
            )
        )
    return PromptStack(tuple(parts))


def prepare_hourly_shard_scribe_job(
    system: MemorySystem,
    *,
    params: dict[str, Any],
    workspace: Path,
    run_harness: bool,
    model: str | None,
    run_id: str,
    completed_event: str,
    validator: Any = None,
) -> PreparedMemoryJob:
    job = required_manifest(system.jobs, HOURLY_SHARD_SCRIBE_JOB_ID, "job")
    agent = required_manifest(system.agents, str(job.get("agent", "")), "agent")
    harness_id = str(agent.get("harness") or "")
    provider_id = str(agent.get("provider") or "")
    resolved_model = str(model or agent.get("model") or "")
    if not harness_id or not provider_id or not resolved_model:
        raise MemoryJobError("hourly shard scribe agent must resolve harness, provider, and model")

    date = str(params["date"])
    hour = f"{int(str(params['hour'])):02d}"
    audience = str(params.get("audience", "private"))
    page_slug = str(params.get("page_slug", f"for-you-{date}"))
    talk_folder = Path(str(params.get("talk_folder") or workspace / f"{page_slug}.{audience}.talk"))
    ensure_for_you_talk_folder(talk_folder, date=date, audience=audience, page_slug=page_slug)
    shard_id = safe_id(str(params["shard_id"]))
    shard_label = str(params.get("shard_label") or shard_id)
    output_path = Path(str(params.get("output_path") or talk_folder / ".shards" / f"{date}T{hour}-00Z.{shard_id}.synthesis.md"))
    output_path.parent.mkdir(parents=True, exist_ok=True)
    if run_harness and output_path.exists():
        output_path.unlink()

    experience_config = job.get("experience", {})
    if not isinstance(experience_config, dict):
        raise MemoryJobError(f"{HOURLY_SHARD_SCRIBE_JOB_ID} experience config must be a table")
    builder = str(experience_config.get("builder", ""))
    if builder != "render_hour_experience":
        raise MemoryJobError(f"unsupported experience builder {builder!r}")
    experience_mode = str(params.get("experience_mode") or experience_config.get("mode") or DEFAULT_EXPERIENCE_MODE)
    resolved_source_harnesses = source_harnesses(params.get("source_harnesses", ("codex", "claude-code")))
    shard_events = params.get("_shard_events")
    if shard_events is None:
        raise MemoryJobError("hourly shard scribe requires _shard_events")
    rendered = render_hour_experience_from_events(
        system,
        date=date,
        hour=hour,
        events=list(shard_events),
        source_harnesses=resolved_source_harnesses,
        experience_mode=experience_mode,
        experience_id=f"hour-{date}T{hour}-{experience_mode}-shard-{shard_id}",
    )
    packet = {
        **rendered.to_packet(),
        "kind": "shard_experience_packet",
        "shard_id": shard_id,
        "shard_label": shard_label,
        "parent_hour": f"{date}T{hour}:00:00Z",
    }
    Ledger(ledger_events_path(system.runtime_dir), storage_path=system.storage_dir).append(
        "memory.hour_shard_experience.rendered",
        ledger_schema_version="0.1",
        plugin_id=system.active_plugin,
        run_id=run_id,
        summary=f"Rendered shard lived experience for {date}T{hour} {shard_id}.",
        experience_packet=packet,
        outcome="done",
    )
    job_params = {
        **public_job_params(params),
        "date": date,
        "hour": hour,
        "audience": audience,
        "talk_folder": str(talk_folder),
        "output_path": str(output_path),
        "shard_id": shard_id,
        "shard_label": shard_label,
        "experience_id": rendered.experience_id,
        "experience_mode": experience_mode,
        "experience_sha256": rendered.experience_sha256,
        "agent_context_sha256": rendered.agent_context_sha256,
    }
    prompt_stack = build_hourly_shard_scribe_prompt_stack(
        system,
        job=job,
        agent=agent,
        rendered=rendered,
        job_params=job_params,
    )
    job_params["prompt_stack_sha256"] = prompt_stack.sha256
    isolation_mode = str(system.runtime_policy.get("default_harness_isolation", "account_clean"))
    if isolation_mode != "account_clean":
        raise MemoryJobError(f"hourly shard scribe currently supports account_clean, got {isolation_mode!r}")
    launch = HarnessLaunchSpec(
        harness=harness_id,
        isolation_mode=isolation_mode,
        argv=claude_account_clean_args(model=resolved_model, workspace=workspace, experience_path=rendered.path),
        cwd=workspace,
    )
    return PreparedMemoryJob(
        job_id=HOURLY_SHARD_SCRIBE_JOB_ID,
        job_params=job_params,
        prompt_stack=prompt_stack,
        talk_folder=talk_folder,
        execution_spec=HiredAgentExecutionSpec(
            run_id=run_id,
            job_ids=[HOURLY_SHARD_SCRIBE_JOB_ID],
            job_params=job_params,
            experience_packet=packet,
            prompt=prompt_stack.text,
            prompt_stack=prompt_stack.to_payload(),
            workspace=workspace,
            artifact=ArtifactSpec(kind="hourly_shard_note", path=output_path),
            harness_launch=launch,
            harness_id=harness_id,
            provider_id=provider_id,
            model=resolved_model,
            run_harness=run_harness,
            completed_event=completed_event,
            validator=validator,
        ),
    )


def prepare_hourly_aggregate_scribe_job(
    system: MemorySystem,
    *,
    params: dict[str, Any],
    workspace: Path,
    run_harness: bool,
    model: str | None,
    run_id: str,
    completed_event: str,
    validator: Any = None,
) -> PreparedMemoryJob:
    job = required_manifest(system.jobs, HOURLY_AGGREGATE_SCRIBE_JOB_ID, "job")
    agent = required_manifest(system.agents, str(job.get("agent", "")), "agent")
    harness_id = str(agent.get("harness") or "")
    provider_id = str(agent.get("provider") or "")
    resolved_model = str(model or agent.get("model") or "")
    if not harness_id or not provider_id or not resolved_model:
        raise MemoryJobError("hourly aggregate scribe agent must resolve harness, provider, and model")

    date = str(params["date"])
    hour = f"{int(str(params['hour'])):02d}"
    audience = str(params.get("audience", "private"))
    page_slug = str(params.get("page_slug", f"for-you-{date}"))
    talk_folder = Path(str(params.get("talk_folder") or workspace / f"{page_slug}.{audience}.talk"))
    ensure_for_you_talk_folder(talk_folder, date=date, audience=audience, page_slug=page_slug)
    output_path = Path(str(params.get("output_path") or talk_folder / f"{date}T{hour}-00Z.conversation.md"))
    if run_harness and output_path.exists():
        output_path.unlink()
    shard_paths = tuple(Path(str(path)) for path in params.get("shard_paths", ()))
    shard_context = render_shard_notes_context(shard_paths)
    packet = {
        "kind": "hourly_shard_notes_packet",
        "date": date,
        "hour": hour,
        "shard_count": len(shard_paths),
        "shard_paths": [str(path) for path in shard_paths],
        "sha256": sha256_text(shard_context),
    }
    job_params = {
        **public_job_params(params),
        "date": date,
        "hour": hour,
        "audience": audience,
        "talk_folder": str(talk_folder),
        "output_path": str(output_path),
        "shard_count": len(shard_paths),
        "shard_paths": "\n".join(str(path) for path in shard_paths),
    }
    prompt_stack = build_hourly_aggregate_scribe_prompt_stack(
        system,
        job=job,
        agent=agent,
        shard_context=shard_context,
        talk_folder=talk_folder,
        job_params=job_params,
    )
    job_params["prompt_stack_sha256"] = prompt_stack.sha256
    isolation_mode = str(system.runtime_policy.get("default_harness_isolation", "account_clean"))
    if isolation_mode != "account_clean":
        raise MemoryJobError(f"hourly aggregate scribe currently supports account_clean, got {isolation_mode!r}")
    launch = HarnessLaunchSpec(
        harness=harness_id,
        isolation_mode=isolation_mode,
        argv=claude_account_clean_args(model=resolved_model, workspace=workspace, experience_path=talk_folder),
        cwd=workspace,
    )
    return PreparedMemoryJob(
        job_id=HOURLY_AGGREGATE_SCRIBE_JOB_ID,
        job_params=job_params,
        prompt_stack=prompt_stack,
        talk_folder=talk_folder,
        execution_spec=HiredAgentExecutionSpec(
            run_id=run_id,
            job_ids=[HOURLY_AGGREGATE_SCRIBE_JOB_ID],
            job_params=job_params,
            experience_packet=packet,
            prompt=prompt_stack.text,
            prompt_stack=prompt_stack.to_payload(),
            workspace=workspace,
            artifact=ArtifactSpec(kind="hourly_talk_entry", path=output_path),
            harness_launch=launch,
            harness_id=harness_id,
            provider_id=provider_id,
            model=resolved_model,
            run_harness=run_harness,
            completed_event=completed_event,
            validator=validator,
        ),
    )


def build_hourly_shard_scribe_prompt_stack(
    system: MemorySystem,
    *,
    job: dict[str, Any],
    agent: dict[str, Any],
    rendered: Any,
    job_params: dict[str, Any],
) -> PromptStack:
    parts: list[PromptPart] = [
        PromptPart(
            name="system_addition",
            text=(
                "# 1Context Hired Agent System Addition\n\n"
                "You are a hired 1Context memory shard agent. You inherit one shard of an oversized "
                "hour as direct lived experience. Write only the shard witness note requested below. "
                "Do not infer missing sibling shards."
            ),
        ),
        PromptPart(
            name="birth_loaded_shard_experience",
            text="# Birth-Loaded Hour Shard Experience Attachment\n\n"
            + rendered.agent_context_path.read_text(encoding="utf-8"),
            path=rendered.agent_context_path,
        ),
    ]
    for index, relative_path in enumerate(agent.get("prompt_paths", [])):
        parts.append(prompt_part_from_file(f"agent_prompt_{index + 1}", system.plugin_path / str(relative_path)))
    task_values = {
        "date": job_params["date"],
        "hour": job_params["hour"],
        "shard_id": job_params["shard_id"],
        "shard_label": job_params["shard_label"],
        "output_path": job_params["output_path"],
    }
    for index, relative_path in enumerate(job.get("prompt_paths", [])):
        parts.append(
            prompt_part_from_file(
                f"job_prompt_{index + 1}",
                system.plugin_path / str(relative_path),
                format_values=task_values,
            )
        )
    return PromptStack(tuple(parts))


def build_hourly_aggregate_scribe_prompt_stack(
    system: MemorySystem,
    *,
    job: dict[str, Any],
    agent: dict[str, Any],
    shard_context: str,
    talk_folder: Path,
    job_params: dict[str, Any],
) -> PromptStack:
    parts: list[PromptPart] = [
        PromptPart(
            name="system_addition",
            text=(
                "# 1Context Hourly Aggregate Agent System Addition\n\n"
                "You are a hired 1Context memory aggregator. The shard witness notes are loaded below "
                "before the task. They are your source of truth for this oversized hour."
            ),
        ),
        PromptPart(name="loaded_shard_witness_notes", text=shard_context, path=talk_folder / ".shards"),
    ]
    for index, relative_path in enumerate(agent.get("prompt_paths", [])):
        parts.append(prompt_part_from_file(f"agent_prompt_{index + 1}", system.plugin_path / str(relative_path)))
    task_values = {
        "date": job_params["date"],
        "hour": job_params["hour"],
        "output_path": job_params["output_path"],
        "shard_count": job_params["shard_count"],
    }
    for index, relative_path in enumerate(job.get("prompt_paths", [])):
        parts.append(
            prompt_part_from_file(
                f"job_prompt_{index + 1}",
                system.plugin_path / str(relative_path),
                format_values=task_values,
            )
        )
    return PromptStack(tuple(parts))


def render_shard_notes_context(shard_paths: tuple[Path, ...]) -> str:
    lines = [
        "# Loaded Hour Shard Witness Notes",
        "",
        f"shard_count: {len(shard_paths)}",
        "",
    ]
    for path in shard_paths:
        lines.extend(["## Shard: " + str(path), ""])
        if path.is_file():
            lines.append(path.read_text(encoding="utf-8").rstrip())
        else:
            lines.append("[missing shard note at prompt assembly time]")
        lines.append("")
    return "\n".join(lines).rstrip() + "\n"


def render_block_context(*, date: str, block_start: str, block_end: str, rendered_hours: list[Any]) -> str:
    lines = [
        "---",
        "kind: birth_loaded_fixed_block_experience",
        f"date: {date}",
        f"block_start: {block_start}",
        f"block_end: {block_end}",
        "loaded_at_birth: true",
        "---",
        "",
        "# Birth-Loaded Fixed 4-Hour Experience",
        "",
        "You are inheriting several adjacent hourly lives in one context. Treat each hour independently.",
        "Write separate hourly artifacts, or record no-talk / needs-retry per hour.",
        "",
    ]
    for rendered in rendered_hours:
        hour = rendered.source_window["start"][11:13]
        lines.extend(
            [
                f"<hour_experience hour=\"{hour}\" source_window=\"{rendered.source_window['start']}/{rendered.source_window['end']}\">",
                "",
                rendered.agent_context_path.read_text(encoding="utf-8").rstrip(),
                "",
                "</hour_experience>",
                "",
            ]
        )
    return "\n".join(lines).rstrip() + "\n"


def cached_block_experience(
    store: LakeStore,
    *,
    artifact_id: str,
    block_id: str,
    experience_mode: str,
    date: str,
    block_start: str,
    block_end: str,
    hours: tuple[str, ...],
    hour_context_hashes: list[str],
) -> dict[str, Any] | None:
    row = artifact_row_by_id(store, artifact_id)
    if not row:
        return None
    metadata = parse_json_object(row.get("metadata_json"))
    if metadata.get("experience_id") != block_id:
        return None
    if metadata.get("experience_mode") != experience_mode:
        return None
    if metadata.get("date") != date:
        return None
    if metadata.get("block_start") != block_start or metadata.get("block_end") != block_end:
        return None
    if tuple(str(hour) for hour in metadata.get("hours", [])) != tuple(hours):
        return None
    if list(metadata.get("hour_context_sha256", [])) != hour_context_hashes:
        return None
    block_context_path = Path(str(row.get("path") or ""))
    if not block_context_path.is_file():
        return None
    block_sha = str(row.get("content_hash") or "")
    if not block_sha:
        block_sha = sha256_text(block_context_path.read_text(encoding="utf-8"))
    return {
        "block_context_path": block_context_path,
        "block_sha": block_sha,
    }


def build_hourly_scribe_prompt_stack(
    system: MemorySystem,
    *,
    job: dict[str, Any],
    agent: dict[str, Any],
    rendered: Any,
    job_params: dict[str, Any],
) -> PromptStack:
    parts: list[PromptPart] = [
        PromptPart(
            name="system_addition",
            text=(
                "# 1Context Hired Agent System Addition\n\n"
                "You are a hired 1Context memory agent. You inherit a past operational life for this job. "
                "The full lived-experience context is already loaded below before the current task. "
                "Treat it like restored session continuity, not a file you still need to inspect. "
                "Tools may be available, but do not rediscover the hour by default. If the inherited "
                "hour is insufficient, write a NEEDS wider-window request instead of guessing."
            ),
        ),
        PromptPart(
            name="birth_loaded_lived_experience",
            text="# Birth-Loaded Lived Experience Attachment\n\n"
            + rendered.agent_context_path.read_text(encoding="utf-8"),
            path=rendered.agent_context_path,
        ),
    ]
    for index, relative_path in enumerate(agent.get("prompt_paths", [])):
        parts.append(prompt_part_from_file(f"agent_prompt_{index + 1}", system.plugin_path / str(relative_path)))
    task_values = {
        "output_path": job_params["output_path"],
        "date": job_params["date"],
        "hour": job_params["hour"],
    }
    for index, relative_path in enumerate(job.get("prompt_paths", [])):
        parts.append(
            prompt_part_from_file(
                f"job_prompt_{index + 1}",
                system.plugin_path / str(relative_path),
                format_values=task_values,
            )
        )
    return PromptStack(tuple(parts))


def prepare_talk_folder_job(
    system: MemorySystem,
    *,
    job_id: str,
    params: dict[str, Any],
    workspace: Path,
    run_harness: bool,
    model: str | None,
    run_id: str,
    completed_event: str,
    validator: Any = None,
) -> PreparedMemoryJob:
    job = required_manifest(system.jobs, job_id, "job")
    agent = required_manifest(system.agents, str(job.get("agent", "")), "agent")
    harness_id = str(agent.get("harness") or "")
    provider_id = str(agent.get("provider") or "")
    resolved_model = str(model or agent.get("model") or "")
    if not harness_id or not provider_id or not resolved_model:
        raise MemoryJobError(f"{job_id} agent must resolve harness, provider, and model")

    date = str(params["date"])
    audience = str(params.get("audience", "private"))
    page_slug = str(params.get("page_slug", f"for-you-{date}"))
    talk_folder = Path(str(params.get("talk_folder") or workspace / f"{page_slug}.{audience}.talk"))
    ensure_for_you_talk_folder(talk_folder, date=date, audience=audience, page_slug=page_slug)
    default_slug = f"editor-day-{date}" if job_id == DAILY_EDITOR_JOB_ID else "concept-candidates"
    default_kind = "proposal"
    output_path = Path(
        str(params.get("output_path") or talk_folder / f"{date}T23-59Z.{default_kind}.{default_slug}.md")
    )
    if run_harness and output_path.exists():
        output_path.unlink()

    talk_context = render_talk_folder_context(talk_folder)
    packet = {
        "kind": "talk_folder_context",
        "talk_folder": str(talk_folder),
        "entry_count": len(read_talk_entries(talk_folder)),
        "sha256": sha256_text(talk_context),
    }
    job_params = {
        **params,
        "date": date,
        "audience": audience,
        "talk_folder": str(talk_folder),
        "output_path": str(output_path),
    }
    prompt_stack = build_talk_folder_prompt_stack(
        system,
        job=job,
        agent=agent,
        talk_context=talk_context,
        talk_folder=talk_folder,
        job_params=job_params,
    )
    job_params["prompt_stack_sha256"] = prompt_stack.sha256

    isolation_mode = str(system.runtime_policy.get("default_harness_isolation", "account_clean"))
    if isolation_mode != "account_clean":
        raise MemoryJobError(f"{job_id} currently supports account_clean, got {isolation_mode!r}")
    launch = HarnessLaunchSpec(
        harness=harness_id,
        isolation_mode=isolation_mode,
        argv=claude_account_clean_args(model=resolved_model, workspace=workspace, experience_path=talk_folder),
        cwd=workspace,
    )
    artifact_kind = "daily_section_proposal" if job_id == DAILY_EDITOR_JOB_ID else "concept_candidate_proposal"
    return PreparedMemoryJob(
        job_id=job_id,
        job_params=job_params,
        prompt_stack=prompt_stack,
        talk_folder=talk_folder,
        execution_spec=HiredAgentExecutionSpec(
            run_id=run_id,
            job_ids=[job_id],
            job_params=job_params,
            experience_packet=packet,
            prompt=prompt_stack.text,
            prompt_stack=prompt_stack.to_payload(),
            workspace=workspace,
            artifact=ArtifactSpec(kind=artifact_kind, path=output_path),
            harness_launch=launch,
            harness_id=harness_id,
            provider_id=provider_id,
            model=resolved_model,
            run_harness=run_harness,
            completed_event=completed_event,
            validator=validator,
        ),
    )


def build_talk_folder_prompt_stack(
    system: MemorySystem,
    *,
    job: dict[str, Any],
    agent: dict[str, Any],
    talk_context: str,
    talk_folder: Path,
    job_params: dict[str, Any],
) -> PromptStack:
    parts: list[PromptPart] = [
        PromptPart(
            name="system_addition",
            text=(
                "# 1Context Talk-Folder Agent System Addition\n\n"
                "You are a hired 1Context memory agent. Your source context is a rendered talk-folder input, "
                "following the e08 For You wiki input convention: _meta.yaml plus timestamped entry files. "
                "Write exactly the requested new talk entry and do not edit existing entries."
            ),
        ),
        PromptPart(name="talk_folder_context", text=talk_context, path=talk_folder),
    ]
    for index, relative_path in enumerate(agent.get("prompt_paths", [])):
        parts.append(prompt_part_from_file(f"agent_prompt_{index + 1}", system.plugin_path / str(relative_path)))
    task_values = {
        "output_path": job_params["output_path"],
        "date": job_params["date"],
    }
    for index, relative_path in enumerate(job.get("prompt_paths", [])):
        parts.append(
            prompt_part_from_file(
                f"job_prompt_{index + 1}",
                system.plugin_path / str(relative_path),
                format_values=task_values,
            )
        )
    return PromptStack(tuple(parts))


def render_talk_folder_context(talk_folder: Path) -> str:
    entries = read_talk_entries(talk_folder)
    lines = [
        "# Loaded Talk Folder Input",
        "",
        f"talk_folder: {talk_folder}",
        f"entry_count: {len(entries)}",
        "",
        "## Folder metadata",
        "",
    ]
    meta_path = talk_folder / "_meta.yaml"
    if meta_path.is_file():
        lines.append(meta_path.read_text(encoding="utf-8").rstrip())
    else:
        lines.append("[missing _meta.yaml]")
    for entry in entries:
        lines.extend(
            [
                "",
                f"## Entry: {entry.path.name}",
                "",
                "```yaml",
                "\n".join(f"{key}: {value}" for key, value in entry.frontmatter.items()),
                "```",
                "",
                entry.body.rstrip(),
            ]
        )
    return "\n".join(lines).rstrip() + "\n"


def claude_account_clean_args(*, model: str, workspace: Path, experience_path: Path) -> list[str]:
    return [
        "claude",
        "-p",
        "--input-format",
        "text",
        "--output-format",
        "text",
        "--model",
        model,
        "--permission-mode",
        "acceptEdits",
        "--no-session-persistence",
        "--setting-sources",
        "project,local",
        "--disable-slash-commands",
        "--no-chrome",
        "--mcp-config",
        '{"mcpServers":{}}',
        "--strict-mcp-config",
        "--tools",
        CLAUDE_ACCOUNT_CLEAN_TOOLS,
        "--add-dir",
        str(workspace),
        str(experience_path),
    ]


def required_manifest(collection: dict[str, dict[str, Any]], manifest_id: str, kind: str) -> dict[str, Any]:
    manifest = collection.get(manifest_id)
    if not manifest:
        raise MemoryJobError(f"missing {kind} manifest {manifest_id!r}")
    return manifest


def source_harnesses(value: Any) -> tuple[str, ...]:
    if isinstance(value, str):
        return tuple(item.strip() for item in value.split(",") if item.strip())
    return tuple(str(item).strip() for item in (value or []) if str(item).strip())


def public_job_params(params: dict[str, Any]) -> dict[str, Any]:
    return {key: value for key, value in params.items() if not key.startswith("_")}


def normalize_hours(value: Any) -> tuple[str, ...]:
    if isinstance(value, str):
        items = [item.strip() for item in value.split(",") if item.strip()]
    else:
        items = [str(item).strip() for item in value if str(item).strip()]
    return tuple(f"{int(item):02d}" for item in items)


def safe_id(value: str) -> str:
    return "".join(char if char.isalnum() or char in "._-" else "-" for char in value).strip("-")


def artifact_row_by_id(store: LakeStore, artifact_id: str) -> dict[str, Any] | None:
    return store.row_by_value("artifacts", "artifact_id", artifact_id)


def parse_json_object(value: Any) -> dict[str, Any]:
    if isinstance(value, dict):
        return dict(value)
    if not isinstance(value, str) or not value:
        return {}
    try:
        parsed = json.loads(value)
    except json.JSONDecodeError:
        return {}
    return parsed if isinstance(parsed, dict) else {}
