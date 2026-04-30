from __future__ import annotations

import argparse
import json
import sys
import time
from pathlib import Path
from typing import Any

from .accounts import link_accounts
from .agent.integrations import build_install_plan, executable_command, write_default_startup_config
from .agent.startup_context import build_startup_context, read_hook_input
from .config import ConfigError, compile_system_map, list_plugins, load_system
from .daemon.apps import AppError, app_status, load_apps, open_app, start_app, stop_app
from .daemon.loop import DaemonError
from .daemon.loop import DEFAULT_DAEMON_INTERVAL_SECONDS
from .daemon.loop import run_once as daemon_run_once
from .daemon.loop import watch as daemon_watch
from .lab.hourly_scribe import HourlyScribeLabError, run_hourly_scribe_lab
from .memory.ledger import Ledger, ledger_events_path
from .memory.birth import BirthCertificateError, render_birth_certificate, select_birth_event
from .memory.invariants import build_runtime_invariant_report, write_runtime_invariant_report_artifact
from .memory.jobs import MemoryJobError, prepare_memory_job
from .memory.health import build_memory_health_payload, write_memory_health_artifact
from .memory.migrations import MigrationError, load_migration_definitions, run_contract_migrations
from .memory.quality import QualityError, run_quality_probes, write_quality_report
from .memory.runner import HiredAgentRunnerError, execute_hired_agent
from .memory.replay import ReplayError, run_replay_dry_run
from .memory.scheduler import SchedulerError, plan_scheduler_tick, write_scheduler_plan
from .memory.wiki_apply import (
    WikiApplyError,
    apply_curator_decision_to_sandbox,
    promote_wiki_apply_result_to_source,
    write_wiki_apply_promotion_result,
    write_wiki_apply_result,
)
from .memory.tick import (
    MemoryTickError,
    list_memory_cycles,
    load_memory_cycle,
    run_memory_tick,
    validate_memory_cycle,
)
from .memory.day_hourlies import (
    DEFAULT_PROMPT_WARNING_TOKENS,
    DayHourliesError,
    discover_month_active_hours,
    fixed_four_hour_blocks,
    hour_event_buckets,
    plan_month_hourly_routes,
    run_day_hourly_scribes,
    run_month_hourly_block_scribes,
    run_month_hourly_retries,
    run_month_hourly_scribes,
)
from .memory.for_you_runner import ForYouRunnerError, run_for_you_month
from .memory.talk import render_talk_folder
from .memory.wiki import (
    WikiError,
    brackify_text,
    build_wiki_inputs,
    collect_concepts,
    evaluate_wiki_route_source_freshness,
    plan_wiki_roles,
    preview_wiki_route_execution,
    write_wiki_route_execution_artifact,
    write_wiki_route_plan_artifact,
)
from .wiki.cli import (
    cmd_wiki_ensure as cmd_wiki_engine_ensure,
    cmd_wiki_list as cmd_wiki_engine_list,
    cmd_wiki_render as cmd_wiki_engine_render,
    cmd_wiki_routes as cmd_wiki_engine_routes,
    cmd_wiki_stats as cmd_wiki_engine_stats,
)
from .wiki.families import WikiError as WikiEngineError
from .memory.experience import (
    ExperienceError,
    configured_native_memory_formats,
    configured_providers,
    resolve_native_memory_route,
    native_memory_paths_for_experience,
)
from .memory.linker import (
    LEDGER_SCHEMA_VERSION,
    LINKER_ID,
    LINKER_VERSION,
    HireError,
    hire_agent,
    runtime_experience_dir,
)
from .storage import LakeStore, TABLE_ORDER, StorageError
from .ports import PortError, load_ports
from .state_machines.mermaid import StateMachineDiagramError, state_machine_to_mermaid
from .state_machines.production import (
    StateMachineProductionError,
    compile_state_machine_artifacts,
    verify_state_machine_artifacts,
)


def main(argv: list[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)
    if not hasattr(args, "func"):
        args.func = cmd_show
    try:
        return args.func(args)
    except ConfigError as exc:
        print(f"config error: {exc}", file=sys.stderr)
        return 1
    except WikiEngineError as exc:
        print(f"wiki error: {exc}", file=sys.stderr)
        return 1
    except StateMachineProductionError as exc:
        print(f"state-machine production error: {exc}", file=sys.stderr)
        return 1
    except (AppError, DaemonError, PortError) as exc:
        print(f"daemon error: {exc}", file=sys.stderr)
        return 1


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(prog="1context", description="1Context memory CLI")
    add_context_options(parser)
    sub = parser.add_subparsers(dest="command")

    p_show = sub.add_parser("show", help="show the active memory system")
    add_context_options(p_show, suppress_defaults=True)
    p_show.set_defaults(func=cmd_show)

    p_map = sub.add_parser("map", help="show the compiled system map")
    add_context_options(p_map, suppress_defaults=True)
    p_map.add_argument("--json", action="store_true", help="print machine-readable JSON")
    p_map.set_defaults(func=cmd_map)

    p_plugins = sub.add_parser("plugins", help="list discoverable memory plugins")
    add_context_options(p_plugins, suppress_defaults=True)
    p_plugins.set_defaults(func=cmd_plugins)

    p_host = sub.add_parser("host", help="show local host grants")
    add_context_options(p_host, suppress_defaults=True)
    p_host.set_defaults(func=cmd_host)

    p_accounts = sub.add_parser("accounts", help="show or regenerate system account links")
    add_context_options(p_accounts, suppress_defaults=True)
    p_accounts.set_defaults(func=cmd_accounts_show)
    accounts_sub = p_accounts.add_subparsers(dest="accounts_command")

    p_accounts_link = accounts_sub.add_parser("link", help="regenerate accounts.toml from active plugin needs")
    add_context_options(p_accounts_link, suppress_defaults=True)
    p_accounts_link.add_argument("--check", action="store_true", help="report whether accounts.toml would change")
    p_accounts_link.add_argument("--json", action="store_true", help="print machine-readable result")
    p_accounts_link.set_defaults(func=cmd_accounts_link)

    p_harnesses = sub.add_parser("harnesses", help="list agent harnesses")
    add_context_options(p_harnesses, suppress_defaults=True)
    p_harnesses.set_defaults(func=cmd_harnesses)

    p_state_machines = sub.add_parser("state-machines", help="list plugin state machines")
    add_context_options(p_state_machines, suppress_defaults=True)
    p_state_machines.add_argument("--json", action="store_true", help="print machine-readable JSON")
    p_state_machines.set_defaults(func=cmd_state_machines)
    state_machines_sub = p_state_machines.add_subparsers(dest="state_machine_command")

    p_state_machine_diagram = state_machines_sub.add_parser(
        "diagram",
        help="render a compiled state-machine IR transition diagram as Mermaid",
    )
    add_context_options(p_state_machine_diagram, suppress_defaults=True)
    p_state_machine_diagram.add_argument("machine_id", help="State machine id, e.g. wiki_growth_fabric")
    p_state_machine_diagram.add_argument("--scope", default="", help="Scope to diagram; defaults to first transition target")
    p_state_machine_diagram.add_argument("--output", type=Path, help="Write Mermaid source to this .mmd file")
    p_state_machine_diagram.set_defaults(func=cmd_state_machine_diagram)

    p_state_machine_compile = state_machines_sub.add_parser(
        "compile",
        help="write compiled state-machine IR and generated diagrams as production artifacts",
    )
    add_context_options(p_state_machine_compile, suppress_defaults=True)
    p_state_machine_compile.add_argument("--output", type=Path, help="Output directory; defaults under memory/runtime/state-machines")
    p_state_machine_compile.add_argument("--run-id", default="", help="Optional production run id")
    p_state_machine_compile.add_argument("--json", action="store_true", help="print machine-readable result")
    p_state_machine_compile.set_defaults(func=cmd_state_machine_compile)

    p_state_machine_verify = state_machines_sub.add_parser(
        "verify",
        help="compile and verify state-machine production artifacts",
    )
    add_context_options(p_state_machine_verify, suppress_defaults=True)
    p_state_machine_verify.add_argument("--output", type=Path, help="Output directory; defaults under memory/runtime/state-machines")
    p_state_machine_verify.add_argument("--run-id", default="", help="Optional verification run id")
    p_state_machine_verify.add_argument("--json", action="store_true", help="print machine-readable result")
    p_state_machine_verify.set_defaults(func=cmd_state_machine_verify)

    p_memory = sub.add_parser("memory", help="show runtime memory")
    add_context_options(p_memory, suppress_defaults=True)
    p_memory.set_defaults(func=cmd_memory_show)
    memory_sub = p_memory.add_subparsers(dest="memory_command")

    p_memory_replay = memory_sub.add_parser("replay-dry-run", help="schedule historic-event replay without launching agents")
    add_context_options(p_memory_replay, suppress_defaults=True)
    p_memory_replay.add_argument("--start", required=True, help="Replay window start timestamp")
    p_memory_replay.add_argument("--end", required=True, help="Replay window end timestamp")
    p_memory_replay.add_argument("--sources", default="codex,claude-code", help="Comma-separated source harnesses")
    p_memory_replay.add_argument("--replay-run-id", default="", help="Optional stable replay run id")
    p_memory_replay.add_argument("--sandbox", type=Path, help="Copy this workspace/file into the replay sandbox")
    p_memory_replay.add_argument(
        "--inject-failure",
        action="append",
        default=[],
        help="Apply a sandbox-only failure injection, e.g. agent_timeout or tool_failure:scribe@time",
    )
    p_memory_replay.add_argument(
        "--inject-operator-edit",
        action="append",
        default=[],
        help="Write an operator-touched fixture at this relative path inside the sandbox",
    )
    p_memory_replay.add_argument("--json", action="store_true", help="print machine-readable result")
    p_memory_replay.set_defaults(func=cmd_memory_replay_dry_run)

    p_memory_tick = memory_sub.add_parser("tick", help="execute one concrete memory state-machine tick")
    add_context_options(p_memory_tick, suppress_defaults=True)
    p_memory_tick.add_argument("--wiki-only", action="store_true", help="Run the wiki-only tick bridge")
    p_memory_tick.add_argument("--workspace", type=Path, help="Optional e08-style markdown wiki workspace for role planning")
    p_memory_tick.add_argument("--concept-dir", type=Path, help="Optional concept page directory for role planning")
    p_memory_tick.add_argument("--audience", default="private", help="Audience tier for wiki role planning")
    p_memory_tick.add_argument("--sources", default="codex,claude-code", help="Comma-separated source importers for freshness checks")
    p_memory_tick.add_argument(
        "--max-source-age-hours",
        type=int,
        default=None,
        help="Override runtime_policy.max_importer_staleness_hours for this tick",
    )
    p_memory_tick.add_argument("--require-fresh", action="store_true", help="Block the tick when required source imports are stale")
    p_memory_tick.add_argument(
        "--freshness-check",
        choices=["auto", "always", "skip"],
        default="auto",
        help="Source freshness preflight policy; auto checks when source-derived route planning is requested",
    )
    p_memory_tick.add_argument("--run-migrations", action="store_true", help="Run contract migration receipts during this tick")
    p_memory_tick.add_argument("--execute-render", action="store_true", help="Render wiki-engine families during this tick")
    p_memory_tick.add_argument(
        "--render-family",
        action="append",
        default=[],
        help="Wiki family id to render; repeatable. Defaults to all families when --execute-render is set",
    )
    p_memory_tick.add_argument(
        "--execute-route-hires",
        action="store_true",
        help="Birth the planned wiki route hired agents and write their prompt inputs",
    )
    p_memory_tick.add_argument(
        "--route-hire-limit",
        type=int,
        default=0,
        help="Limit route hired-agent births; 0 means all planned hire rows",
    )
    p_memory_tick.add_argument(
        "--run-route-harness",
        action="store_true",
        help="Actually launch Claude for route hires instead of only writing prompt inputs",
    )
    p_memory_tick.add_argument(
        "--promote-route-outputs",
        action="store_true",
        help="After successful route hires, apply validated curator outputs to the source workspace",
    )
    p_memory_tick.add_argument(
        "--operator-approval",
        default="",
        help="Required exact approval token for source promotion",
    )
    p_memory_tick.add_argument("--skip-talk", action="store_true", help="When rendering, render source pages only")
    p_memory_tick.add_argument("--no-evidence", action="store_true", help="When rendering, skip per-family render evidence rows")
    p_memory_tick.add_argument("--retry-budget", type=int, default=0, help="Mark retryable failures when retry budget remains")
    p_memory_tick.add_argument("--cycle-id", default="", help="Optional stable cycle id")
    p_memory_tick.add_argument("--json", action="store_true", help="print machine-readable result")
    p_memory_tick.set_defaults(func=cmd_memory_tick)

    p_memory_cycles = memory_sub.add_parser("cycles", help="inspect memory tick cycle artifacts")
    add_context_options(p_memory_cycles, suppress_defaults=True)
    p_memory_cycles.set_defaults(func=cmd_memory_cycles_list)
    cycles_sub = p_memory_cycles.add_subparsers(dest="cycles_command")

    p_memory_cycles_list = cycles_sub.add_parser("list", help="list recent memory cycles")
    add_context_options(p_memory_cycles_list, suppress_defaults=True)
    p_memory_cycles_list.add_argument("--limit", type=int, default=20)
    p_memory_cycles_list.add_argument("--json", action="store_true", help="print machine-readable result")
    p_memory_cycles_list.set_defaults(func=cmd_memory_cycles_list)

    p_memory_cycles_show = cycles_sub.add_parser("show", help="show one memory cycle artifact")
    add_context_options(p_memory_cycles_show, suppress_defaults=True)
    p_memory_cycles_show.add_argument("cycle_id")
    p_memory_cycles_show.add_argument("--json", action="store_true", help="print machine-readable result")
    p_memory_cycles_show.set_defaults(func=cmd_memory_cycles_show)

    p_memory_cycles_validate = cycles_sub.add_parser("validate", help="validate one memory cycle against lakestore evidence")
    add_context_options(p_memory_cycles_validate, suppress_defaults=True)
    p_memory_cycles_validate.add_argument("cycle_id")
    p_memory_cycles_validate.add_argument("--json", action="store_true", help="print machine-readable result")
    p_memory_cycles_validate.set_defaults(func=cmd_memory_cycles_validate)

    p_memory_migrations = memory_sub.add_parser("migrations", help="inspect or run memory contract migrations")
    add_context_options(p_memory_migrations, suppress_defaults=True)
    p_memory_migrations.set_defaults(func=cmd_memory_migrations_list)
    migrations_sub = p_memory_migrations.add_subparsers(dest="migrations_command")

    p_memory_migrations_list = migrations_sub.add_parser("list", help="list plugin migration manifests")
    add_context_options(p_memory_migrations_list, suppress_defaults=True)
    p_memory_migrations_list.add_argument("--json", action="store_true", help="print machine-readable result")
    p_memory_migrations_list.set_defaults(func=cmd_memory_migrations_list)

    p_memory_migrations_run = migrations_sub.add_parser("run", help="run idempotent contract migrations")
    add_context_options(p_memory_migrations_run, suppress_defaults=True)
    p_memory_migrations_run.add_argument("--run-id", default="", help="Optional stable migration run id")
    p_memory_migrations_run.add_argument("--json", action="store_true", help="print machine-readable result")
    p_memory_migrations_run.set_defaults(func=cmd_memory_migrations_run)

    p_memory_quality = memory_sub.add_parser("quality", help="run cheap structural memory/wiki quality probes")
    add_context_options(p_memory_quality, suppress_defaults=True)
    p_memory_quality.add_argument("path", type=Path, help="Markdown file or workspace root to probe")
    p_memory_quality.add_argument("--run-id", default="", help="Optional stable quality report run id")
    p_memory_quality.add_argument("--now", default="", help="Reference date for stale checks, YYYY-MM-DD")
    p_memory_quality.add_argument("--stale-current-state-days", type=int, default=30)
    p_memory_quality.add_argument("--no-record", action="store_true", help="Do not write runtime artifact/evidence")
    p_memory_quality.add_argument("--json", action="store_true", help="print machine-readable result")
    p_memory_quality.set_defaults(func=cmd_memory_quality)

    p_memory_wiki_apply = memory_sub.add_parser("wiki-apply", help="apply one curator decision to a sandboxed wiki copy")
    add_context_options(p_memory_wiki_apply, suppress_defaults=True)
    p_memory_wiki_apply.add_argument("--source-workspace", type=Path, required=True)
    p_memory_wiki_apply.add_argument("--decision", type=Path, required=True, help="Curator decision markdown artifact")
    p_memory_wiki_apply.add_argument("--route-row-json", type=Path, help="Route row JSON carrying ownership")
    p_memory_wiki_apply.add_argument("--article", type=Path, help="Article path for ad-hoc ownership")
    p_memory_wiki_apply.add_argument("--section", default="", help="Section slug for ad-hoc ownership")
    p_memory_wiki_apply.add_argument("--sandbox-root", type=Path, help="Sandbox parent directory")
    p_memory_wiki_apply.add_argument("--promote-to-source", action="store_true", help="After a successful sandbox apply, copy the validated sandbox change into the source workspace")
    p_memory_wiki_apply.add_argument("--operator-approval", default="", help="Required exact approval token for --promote-to-source")
    p_memory_wiki_apply.add_argument("--run-id", default="", help="Optional stable apply run id")
    p_memory_wiki_apply.add_argument("--no-record", action="store_true", help="Do not write runtime artifact/evidence")
    p_memory_wiki_apply.add_argument("--json", action="store_true", help="print machine-readable result")
    p_memory_wiki_apply.set_defaults(func=cmd_memory_wiki_apply)

    p_memory_schedule = memory_sub.add_parser("schedule", help="plan memory cadence fires with source freshness gating")
    add_context_options(p_memory_schedule, suppress_defaults=True)
    p_memory_schedule.add_argument("--start", required=True)
    p_memory_schedule.add_argument("--end", required=True)
    p_memory_schedule.add_argument("--sources", default="codex,claude-code")
    p_memory_schedule.add_argument("--max-source-age-hours", type=int, default=24)
    p_memory_schedule.add_argument("--allow-stale", action="store_true", help="Do not block fires when sources are stale")
    p_memory_schedule.add_argument("--now", default="", help="Reference timestamp for freshness checks")
    p_memory_schedule.add_argument("--run-id", default="", help="Optional stable scheduler run id")
    p_memory_schedule.add_argument("--no-record", action="store_true", help="Do not write runtime artifact/evidence")
    p_memory_schedule.add_argument("--json", action="store_true", help="print machine-readable result")
    p_memory_schedule.set_defaults(func=cmd_memory_schedule)

    p_memory_health = memory_sub.add_parser("health", help="write a memory runtime health payload")
    add_context_options(p_memory_health, suppress_defaults=True)
    p_memory_health.add_argument("--run-id", default="", help="Optional stable health run id")
    p_memory_health.add_argument("--no-record", action="store_true", help="Do not write runtime artifact/evidence")
    p_memory_health.add_argument("--json", action="store_true", help="print machine-readable result")
    p_memory_health.set_defaults(func=cmd_memory_health)

    p_storage = sub.add_parser("storage", help="show or initialize the LanceDB lakestore")
    add_context_options(p_storage, suppress_defaults=True)
    p_storage.set_defaults(func=cmd_storage_show)
    storage_sub = p_storage.add_subparsers(dest="storage_command")

    p_storage_init = storage_sub.add_parser("init", help="create lakestore tables")
    add_context_options(p_storage_init, suppress_defaults=True)
    p_storage_init.set_defaults(func=cmd_storage_init)

    p_storage_smoke = storage_sub.add_parser("smoke", help="write one event/artifact/evidence smoke row")
    add_context_options(p_storage_smoke, suppress_defaults=True)
    p_storage_smoke.set_defaults(func=cmd_storage_smoke)

    p_storage_events = storage_sub.add_parser("events", help="show recent lakestore events")
    add_context_options(p_storage_events, suppress_defaults=True)
    p_storage_events.add_argument("--limit", type=int, default=10)
    p_storage_events.set_defaults(func=cmd_storage_events)

    p_storage_search = storage_sub.add_parser("search", help="search lakestore rows by text")
    add_context_options(p_storage_search, suppress_defaults=True)
    p_storage_search.add_argument("query")
    p_storage_search.add_argument(
        "--table",
        choices=list(TABLE_ORDER),
        default="events",
        help="Lakestore table to search",
    )
    p_storage_search.add_argument("--limit", type=int, default=20)
    p_storage_search.set_defaults(func=cmd_storage_search)

    p_storage_export = storage_sub.add_parser("export", help="export lakestore snapshot JSON")
    add_context_options(p_storage_export, suppress_defaults=True)
    p_storage_export.add_argument("--output", type=Path, required=True)
    p_storage_export.add_argument("--limit", type=int, default=250)
    p_storage_export.set_defaults(func=cmd_storage_export)

    p_ports = sub.add_parser("ports", help="list daemon port definitions")
    add_context_options(p_ports, suppress_defaults=True)
    p_ports.add_argument("--json", action="store_true", help="print machine-readable JSON")
    p_ports.set_defaults(func=cmd_ports)

    p_daemon = sub.add_parser("daemon", help="run the local daemon pulse")
    add_context_options(p_daemon, suppress_defaults=True)
    p_daemon.add_argument(
        "--experience-source",
        help="Use source-sessions from a lived/runtime experience id or path instead of real port paths",
    )
    p_daemon.add_argument("--json", action="store_true", help="print machine-readable result")
    p_daemon.set_defaults(func=cmd_daemon_once, experience_source=None, json=False)
    daemon_sub = p_daemon.add_subparsers(dest="daemon_command")

    p_daemon_once = daemon_sub.add_parser("once", help="scan ports, import observations, emit one tick")
    add_context_options(p_daemon_once, suppress_defaults=True)
    p_daemon_once.add_argument(
        "--experience-source",
        help="Use source-sessions from a lived/runtime experience id or path instead of real port paths",
    )
    p_daemon_once.add_argument("--json", action="store_true", help="print machine-readable result")
    p_daemon_once.set_defaults(func=cmd_daemon_once)

    p_daemon_watch = daemon_sub.add_parser("watch", help="run repeated daemon ticks in the foreground")
    add_context_options(p_daemon_watch, suppress_defaults=True)
    p_daemon_watch.add_argument(
        "--experience-source",
        help="Use source-sessions from a lived/runtime experience id or path instead of real port paths",
    )
    p_daemon_watch.add_argument(
        "--interval",
        type=float,
        default=None,
        help=(
            "Seconds between ticks; defaults to ports.toml watch_interval_seconds "
            f"({int(DEFAULT_DAEMON_INTERVAL_SECONDS)} when unset)"
        ),
    )
    p_daemon_watch.add_argument("--ticks", type=int, default=0, help="Stop after N ticks; 0 means forever")
    p_daemon_watch.set_defaults(func=cmd_daemon_watch)

    p_daemon_backfill = daemon_sub.add_parser(
        "backfill",
        help="run bounded daemon ticks repeatedly until port backfill catches up",
    )
    add_context_options(p_daemon_backfill, suppress_defaults=True)
    p_daemon_backfill.add_argument(
        "--experience-source",
        help="Use source-sessions from a lived/runtime experience id or path instead of real port paths",
    )
    p_daemon_backfill.add_argument(
        "--max-ticks",
        type=int,
        default=0,
        help="Safety cap; 0 means run until no port reports limited",
    )
    p_daemon_backfill.add_argument("--json", action="store_true", help="print machine-readable result")
    p_daemon_backfill.set_defaults(func=cmd_daemon_backfill)

    p_apps = sub.add_parser("apps", help="manage supervised local apps")
    add_context_options(p_apps, suppress_defaults=True)
    p_apps.set_defaults(func=cmd_apps_status)
    apps_sub = p_apps.add_subparsers(dest="apps_command")

    p_apps_list = apps_sub.add_parser("list", help="list app definitions")
    add_context_options(p_apps_list, suppress_defaults=True)
    p_apps_list.add_argument("--json", action="store_true", help="print machine-readable JSON")
    p_apps_list.set_defaults(func=cmd_apps_list)

    p_apps_status = apps_sub.add_parser("status", help="show supervised app status")
    add_context_options(p_apps_status, suppress_defaults=True)
    p_apps_status.add_argument("--json", action="store_true", help="print machine-readable JSON")
    p_apps_status.set_defaults(func=cmd_apps_status)

    p_apps_start = apps_sub.add_parser("start", help="start a supervised app")
    add_context_options(p_apps_start, suppress_defaults=True)
    p_apps_start.add_argument("app_id")
    p_apps_start.add_argument("--json", action="store_true", help="print machine-readable result")
    p_apps_start.set_defaults(func=cmd_apps_start)

    p_apps_stop = apps_sub.add_parser("stop", help="stop a supervised app")
    add_context_options(p_apps_stop, suppress_defaults=True)
    p_apps_stop.add_argument("app_id")
    p_apps_stop.add_argument("--json", action="store_true", help="print machine-readable result")
    p_apps_stop.set_defaults(func=cmd_apps_stop)

    p_apps_open = apps_sub.add_parser("open", help="open a supervised app URL")
    add_context_options(p_apps_open, suppress_defaults=True)
    p_apps_open.add_argument("app_id")
    p_apps_open.set_defaults(func=cmd_apps_open)

    p_native_route = sub.add_parser("native-route", help="show harness/provider native-memory routing")
    add_context_options(p_native_route, suppress_defaults=True)
    p_native_route.add_argument("--harness", help="Harness id whose native memory surface should win")
    p_native_route.add_argument("--provider", help="Provider id or alias to route")
    p_native_route.add_argument("--model", help="Model name to route")
    p_native_route.add_argument(
        "--memory-id",
        "--experience-id",
        dest="experience_id",
        help="Show the selected file inside a runtime memory item",
    )
    p_native_route.set_defaults(func=cmd_native_route)

    p_hire = sub.add_parser("hire", help="hire an agent and attach runtime memory")
    add_context_options(p_hire, suppress_defaults=True)
    p_hire.add_argument(
        "--job",
        action="append",
        default=[],
        help="Job id for the hire; repeat or comma-separate to attach multiple ids",
    )
    p_hire.add_argument("--agent", default="", help="Agent id to hire")
    p_hire.add_argument("--mode", choices=["new", "last_for_job", "manual", "none"], help="Attachment mode")
    p_hire.add_argument(
        "--memory-id",
        "--experience-id",
        dest="experience_id",
        help="Runtime memory id for manual mode",
    )
    p_hire.add_argument("--run-id", help="Optional run id to record with the hire")
    p_hire.add_argument(
        "--job-param",
        action="append",
        default=[],
        metavar="KEY=VALUE",
        help="Concrete job invocation parameter to record in the birth certificate; repeat as needed",
    )
    p_hire.add_argument("--harness", help="Harness id to route after hire")
    p_hire.add_argument("--provider", help="Provider id or alias to route after hire")
    p_hire.add_argument("--model", help="Model name to route after hire")
    p_hire.add_argument("--json", action="store_true", help="print machine-readable result")
    p_hire.set_defaults(func=cmd_hire)

    p_ledger = sub.add_parser("ledger", help="show append-only runtime ledger events")
    add_context_options(p_ledger, suppress_defaults=True)
    p_ledger.add_argument("--limit", type=int, default=20)
    p_ledger.set_defaults(func=cmd_ledger)

    p_birth = sub.add_parser("birth", help="render a hired-agent birth certificate")
    add_context_options(p_birth, suppress_defaults=True)
    p_birth.add_argument("--uuid", help="Hired-agent UUID to render; defaults to the latest birth")
    p_birth.add_argument("--json", action="store_true", help="print the raw birth event JSON")
    p_birth.set_defaults(func=cmd_birth)

    p_job = sub.add_parser("job", help="prepare or run manifest-driven memory jobs")
    add_context_options(p_job, suppress_defaults=True)
    p_job.set_defaults(func=cmd_job_help)
    job_sub = p_job.add_subparsers(dest="job_command")

    p_job_run = job_sub.add_parser("run", help="prepare or run one declared memory job")
    add_context_options(p_job_run, suppress_defaults=True)
    p_job_run.add_argument("job_id", help="Job id, e.g. memory.hourly.scribe")
    p_job_run.add_argument("--date", help="UTC date YYYY-MM-DD")
    p_job_run.add_argument("--hour", help="UTC hour 00-23")
    p_job_run.add_argument("--audience", default="private")
    p_job_run.add_argument("--workspace", type=Path, help="Temporary workspace path")
    p_job_run.add_argument("--run-harness", "--run-claude", action="store_true", help="Launch the configured harness")
    p_job_run.add_argument("--model", help="Override the manifest model")
    p_job_run.add_argument("--run-id", default="memory-job-cli")
    p_job_run.add_argument(
        "--job-param",
        action="append",
        default=[],
        metavar="KEY=VALUE",
        help="Additional concrete job parameter; repeat as needed",
    )
    p_job_run.add_argument("--json", action="store_true", help="print machine-readable result")
    p_job_run.set_defaults(func=cmd_job_run)

    p_job_day = job_sub.add_parser("run-day-hourlies", help="run the for_you_day hourly scribe fanout")
    add_context_options(p_job_day, suppress_defaults=True)
    p_job_day.add_argument("--date", required=True, help="UTC date YYYY-MM-DD")
    p_job_day.add_argument("--audience", default="private")
    p_job_day.add_argument("--workspace", type=Path, help="Temporary workspace path")
    p_job_day.add_argument("--run-harness", "--run-claude", action="store_true", help="Launch the configured harnesses")
    p_job_day.add_argument("--model", help="Override the manifest model")
    p_job_day.add_argument("--max-concurrent", type=int, help="Override runtime_policy.max_concurrent_agents")
    p_job_day.add_argument("--limit-hours", type=int, help="Only prepare/run this many non-skipped hourly jobs")
    p_job_day.add_argument("--no-skip-existing", action="store_true", help="Re-run even if a valid hourly entry already exists")
    p_job_day.add_argument("--sources", default="codex,claude-code", help="Comma-separated source harnesses")
    p_job_day.add_argument("--json", action="store_true", help="print machine-readable result")
    p_job_day.set_defaults(func=cmd_job_run_day_hourlies)

    p_job_month = job_sub.add_parser("run-month-hourlies", help="run a month of hourly scribe fanout")
    add_context_options(p_job_month, suppress_defaults=True)
    p_job_month.add_argument("--month", required=True, help="UTC month YYYY-MM")
    p_job_month.add_argument("--audience", default="private")
    p_job_month.add_argument("--workspace", type=Path, help="Temporary workspace path")
    p_job_month.add_argument("--run-harness", "--run-claude", action="store_true", help="Launch the configured harnesses")
    p_job_month.add_argument("--plan-only", action="store_true", help="Only discover active days/hours; do not render packets or create hires")
    p_job_month.add_argument("--model", help="Override the manifest model")
    p_job_month.add_argument("--max-concurrent", type=int, help="Override runtime_policy.max_concurrent_agents")
    p_job_month.add_argument("--limit-hours", type=int, help="Only prepare/run this many non-skipped hourly jobs")
    p_job_month.add_argument("--no-skip-existing", action="store_true", help="Re-run even if valid hourly entries already exist")
    p_job_month.add_argument("--sources", default="codex,claude-code", help="Comma-separated source harnesses")
    p_job_month.add_argument("--json", action="store_true", help="print machine-readable result")
    p_job_month.set_defaults(func=cmd_job_run_month_hourlies)

    p_job_month_blocks = job_sub.add_parser(
        "run-month-hourly-blocks",
        help="run a month of fixed 4-hour hourly scribe blocks",
    )
    add_context_options(p_job_month_blocks, suppress_defaults=True)
    p_job_month_blocks.add_argument("--month", required=True, help="UTC month YYYY-MM")
    p_job_month_blocks.add_argument("--audience", default="private")
    p_job_month_blocks.add_argument("--workspace", type=Path, help="Temporary workspace path")
    p_job_month_blocks.add_argument("--run-harness", "--run-claude", action="store_true", help="Launch the configured harnesses")
    p_job_month_blocks.add_argument("--plan-only", action="store_true", help="Only discover fixed 4-hour blocks; do not render packets or create hires")
    p_job_month_blocks.add_argument("--experience-mode", help="Renderer mode, e.g. braided_lived_messages")
    p_job_month_blocks.add_argument("--model", help="Override the manifest model")
    p_job_month_blocks.add_argument("--max-concurrent", type=int, help="Override runtime_policy.max_concurrent_agents")
    p_job_month_blocks.add_argument("--limit-blocks", type=int, help="Only prepare/run this many non-skipped 4-hour block jobs")
    p_job_month_blocks.add_argument("--no-skip-existing", action="store_true", help="Re-run even if valid hourly entries already exist")
    p_job_month_blocks.add_argument(
        "--split-large-blocks",
        action="store_true",
        help="Non-lossy fallback: split oversized 4-hour prompts into one-hour scribe jobs",
    )
    p_job_month_blocks.add_argument(
        "--max-prompt-tokens",
        type=int,
        default=DEFAULT_PROMPT_WARNING_TOKENS,
        help="Estimated-token warning/split threshold; defaults to 128000",
    )
    p_job_month_blocks.add_argument(
        "--max-prompt-bytes",
        type=int,
        default=None,
        help="Optional byte warning/split threshold; token threshold is primary",
    )
    p_job_month_blocks.add_argument("--sources", default="codex,claude-code", help="Comma-separated source harnesses")
    p_job_month_blocks.add_argument("--json", action="store_true", help="print machine-readable result")
    p_job_month_blocks.set_defaults(func=cmd_job_run_month_hourly_blocks)

    p_job_plan_routes = job_sub.add_parser(
        "plan-month-routes",
        help="cheaply plan block/hour/shard hires for a month without rendering prompts",
    )
    add_context_options(p_job_plan_routes, suppress_defaults=True)
    p_job_plan_routes.add_argument("--month", required=True, help="UTC month YYYY-MM")
    p_job_plan_routes.add_argument("--audience", default="private")
    p_job_plan_routes.add_argument("--workspace", type=Path, help="Temporary workspace path used for skip checks")
    p_job_plan_routes.add_argument("--limit-blocks", type=int, help="Only plan this many non-skipped 4-hour blocks")
    p_job_plan_routes.add_argument("--no-skip-existing", action="store_true", help="Plan even if valid hourly entries already exist")
    p_job_plan_routes.add_argument(
        "--split-large-blocks",
        action="store_true",
        help="Plan non-lossy oversized block/hour splitting",
    )
    p_job_plan_routes.add_argument(
        "--max-prompt-tokens",
        type=int,
        default=DEFAULT_PROMPT_WARNING_TOKENS,
        help="Estimated-token route threshold; defaults to 128000",
    )
    p_job_plan_routes.add_argument("--experience-mode", help="Renderer mode, e.g. braided_lived_messages")
    p_job_plan_routes.add_argument("--sources", default="codex,claude-code", help="Comma-separated source harnesses")
    p_job_plan_routes.add_argument("--json", action="store_true", help="print machine-readable result")
    p_job_plan_routes.set_defaults(func=cmd_job_plan_month_routes)

    p_job_month_retries = job_sub.add_parser(
        "run-month-hourly-retries",
        help="run single-hour retries requested by fixed 4-hour block manifests",
    )
    add_context_options(p_job_month_retries, suppress_defaults=True)
    p_job_month_retries.add_argument("--month", required=True, help="UTC month YYYY-MM")
    p_job_month_retries.add_argument("--audience", default="private")
    p_job_month_retries.add_argument("--workspace", type=Path, help="Month block workspace containing talk folders")
    p_job_month_retries.add_argument("--run-harness", "--run-claude", action="store_true", help="Launch the configured harnesses")
    p_job_month_retries.add_argument("--model", help="Override the manifest model")
    p_job_month_retries.add_argument("--max-concurrent", type=int, help="Override runtime_policy.max_concurrent_agents")
    p_job_month_retries.add_argument("--limit-hours", type=int, help="Only prepare/run this many retry jobs")
    p_job_month_retries.add_argument("--no-skip-existing", action="store_true", help="Re-run even if valid hourly entries already exist")
    p_job_month_retries.add_argument("--sources", default="codex,claude-code", help="Comma-separated source harnesses")
    p_job_month_retries.add_argument("--json", action="store_true", help="print machine-readable result")
    p_job_month_retries.set_defaults(func=cmd_job_run_month_hourly_retries)

    p_job_for_you_month = job_sub.add_parser(
        "run-for-you-month",
        help="execute the proved for_you_day state-machine slice for one month",
    )
    add_context_options(p_job_for_you_month, suppress_defaults=True)
    p_job_for_you_month.add_argument("--month", required=True, help="UTC month YYYY-MM")
    p_job_for_you_month.add_argument("--audience", default="private")
    p_job_for_you_month.add_argument("--workspace", type=Path, help="Temporary workspace path")
    p_job_for_you_month.add_argument("--run-harness", "--run-claude", action="store_true", help="Launch configured harnesses")
    p_job_for_you_month.add_argument("--model", help="Override manifest models")
    p_job_for_you_month.add_argument("--max-concurrent", type=int, help="Override runtime_policy.max_concurrent_agents")
    p_job_for_you_month.add_argument("--limit-blocks", type=int, help="Only prepare/run this many block jobs")
    p_job_for_you_month.add_argument("--limit-days", type=int, help="Only prepare/run this many day review folders")
    p_job_for_you_month.add_argument("--no-day-layer", action="store_true", help="Skip daily editor/concept scout layer")
    p_job_for_you_month.add_argument("--no-skip-existing", action="store_true", help="Re-run even if valid hourly entries already exist")
    p_job_for_you_month.add_argument(
        "--split-large-blocks",
        action="store_true",
        help="Non-lossy fallback: split oversized 4-hour prompts into one-hour scribe jobs",
    )
    p_job_for_you_month.add_argument(
        "--max-prompt-tokens",
        type=int,
        default=DEFAULT_PROMPT_WARNING_TOKENS,
        help="Estimated-token warning/split threshold; defaults to 128000",
    )
    p_job_for_you_month.add_argument(
        "--max-prompt-bytes",
        type=int,
        default=None,
        help="Optional byte warning/split threshold; token threshold is primary",
    )
    p_job_for_you_month.add_argument("--sources", default="codex,claude-code", help="Comma-separated source harnesses")
    p_job_for_you_month.add_argument("--json", action="store_true", help="print machine-readable result")
    p_job_for_you_month.set_defaults(func=cmd_job_run_for_you_month)

    p_job_render_talk = job_sub.add_parser("render-talk-folder", help="assemble a markdown-only talk folder view")
    add_context_options(p_job_render_talk, suppress_defaults=True)
    p_job_render_talk.add_argument("talk_folder", type=Path)
    p_job_render_talk.add_argument("--output", type=Path, help="Output markdown path; defaults to TALK_FOLDER/index.md")
    p_job_render_talk.add_argument("--json", action="store_true", help="print machine-readable result")
    p_job_render_talk.set_defaults(func=cmd_job_render_talk_folder)

    p_install = sub.add_parser("install", help="install 1Context host integrations")
    add_context_options(p_install, suppress_defaults=True)
    p_install.set_defaults(func=cmd_install_help)
    install_sub = p_install.add_subparsers(dest="install_command")

    p_install_agent = install_sub.add_parser(
        "agent-integrations",
        help="plan or initialize global Claude/Codex startup integrations",
    )
    add_context_options(p_install_agent, suppress_defaults=True)
    add_agent_install_options(p_install_agent)
    p_install_agent.set_defaults(func=cmd_agent_install_integrations)

    p_agent = sub.add_parser("agent", help="external agent integration helpers")
    add_context_options(p_agent, suppress_defaults=True)
    p_agent.set_defaults(func=cmd_agent_help)
    agent_sub = p_agent.add_subparsers(dest="agent_command")

    p_agent_startup_context = agent_sub.add_parser(
        "startup-context",
        help="render hook-compatible startup context for Claude Code, Codex, or another agent",
    )
    add_context_options(p_agent_startup_context, suppress_defaults=True)
    p_agent_startup_context.add_argument(
        "--provider",
        default="generic",
        choices=["generic", "claude", "claude-code", "codex"],
        help="Agent provider requesting startup context",
    )
    p_agent_startup_context.add_argument("--cwd", type=Path, help="Working directory from the hook payload")
    p_agent_startup_context.add_argument("--wiki-url", help="Override the local wiki URL")
    p_agent_startup_context.add_argument("--template", help="Override the startup message template")
    p_agent_startup_context.add_argument(
        "--hook-event-name",
        default="SessionStart",
        help="Hook event name to emit in hookSpecificOutput",
    )
    p_agent_startup_context.add_argument(
        "--format",
        choices=["hook-json", "text", "json"],
        default="hook-json",
        help="Output hook JSON, plain message text, or diagnostic JSON",
    )
    p_agent_startup_context.set_defaults(func=cmd_agent_startup_context)

    p_agent_install = agent_sub.add_parser(
        "install-integrations",
        help="plan or initialize global Claude/Codex startup integrations",
    )
    add_context_options(p_agent_install, suppress_defaults=True)
    add_agent_install_options(p_agent_install)
    p_agent_install.set_defaults(func=cmd_agent_install_integrations)

    p_wiki = sub.add_parser("wiki", help="build deterministic wiki input surfaces")
    add_context_options(p_wiki, suppress_defaults=True)
    p_wiki.set_defaults(func=cmd_wiki_help)
    wiki_sub = p_wiki.add_subparsers(dest="wiki_command")

    p_wiki_list = wiki_sub.add_parser("list", help="list wiki page families")
    add_context_options(p_wiki_list, suppress_defaults=True)
    p_wiki_list.add_argument("--json", action="store_true", help="print machine-readable JSON")
    p_wiki_list.set_defaults(func=cmd_wiki_engine_list)

    p_wiki_ensure = wiki_sub.add_parser("ensure", help="create missing wiki pages, talk folders, and templates")
    add_context_options(p_wiki_ensure, suppress_defaults=True)
    p_wiki_ensure.add_argument("family_id", nargs="?", help="Family id to ensure; defaults to all families")
    p_wiki_ensure.add_argument("--json", action="store_true", help="print machine-readable JSON")
    p_wiki_ensure.set_defaults(func=cmd_wiki_engine_ensure)

    p_wiki_render = wiki_sub.add_parser("render", help="render one or all wiki families")
    add_context_options(p_wiki_render, suppress_defaults=True)
    p_wiki_render.add_argument("family_id", nargs="?", help="Family id to render; defaults to all families")
    p_wiki_render.add_argument("--output-dir", type=Path, help="Override the family generated/ output directory")
    p_wiki_render.add_argument("--skip-talk", action="store_true", help="Render source pages only")
    p_wiki_render.add_argument("--no-evidence", action="store_true", help="Do not write lakestore evidence rows")
    p_wiki_render.add_argument("--json", action="store_true", help="print machine-readable JSON")
    p_wiki_render.set_defaults(func=cmd_wiki_engine_render)

    p_wiki_routes = wiki_sub.add_parser("routes", help="show rendered wiki localhost route table")
    add_context_options(p_wiki_routes, suppress_defaults=True)
    p_wiki_routes.add_argument("--json", action="store_true", help="print machine-readable JSON")
    p_wiki_routes.set_defaults(func=cmd_wiki_engine_routes)

    p_wiki_stats = wiki_sub.add_parser("stats", help="show generated wiki health and content stats")
    add_context_options(p_wiki_stats, suppress_defaults=True)
    p_wiki_stats.add_argument("--json", action="store_true", help="print machine-readable JSON")
    p_wiki_stats.set_defaults(func=cmd_wiki_engine_stats)

    p_wiki_build = wiki_sub.add_parser(
        "build-inputs",
        help="generate indexes, resolve brackets, stage backlinks, and write landing/digest pages",
    )
    add_context_options(p_wiki_build, suppress_defaults=True)
    p_wiki_build.add_argument("--workspace", type=Path, required=True, help="Markdown wiki workspace")
    p_wiki_build.add_argument("--concept-dir", type=Path, required=True, help="Directory of concept pages")
    p_wiki_build.add_argument("--staging", type=Path, required=True, help="Render staging directory")
    p_wiki_build.add_argument("--web-base", default="/paul-demo2", help="Public URL base for generated links")
    p_wiki_build.add_argument("--json", action="store_true", help="print machine-readable result")
    p_wiki_build.set_defaults(func=cmd_wiki_build_inputs)

    p_wiki_plan = wiki_sub.add_parser(
        "plan-roles",
        help="scan a wiki workspace and derive the dynamic agent role route plan",
    )
    add_context_options(p_wiki_plan, suppress_defaults=True)
    p_wiki_plan.add_argument("--workspace", type=Path, required=True, help="Markdown wiki workspace")
    p_wiki_plan.add_argument("--concept-dir", type=Path, required=True, help="Directory of concept pages")
    p_wiki_plan.add_argument("--audience", default="private", help="Audience tier for talk-folder routing")
    p_wiki_plan.add_argument(
        "--write-artifact",
        action="store_true",
        help="Persist the route plan under memory/runtime and lakestore",
    )
    p_wiki_plan.add_argument("--json", action="store_true", help="print machine-readable result")
    p_wiki_plan.set_defaults(func=cmd_wiki_plan_roles)

    p_wiki_route_dry_run = wiki_sub.add_parser(
        "route-dry-run",
        help="derive a route plan and preview the hired-agent births without launching agents",
    )
    add_context_options(p_wiki_route_dry_run, suppress_defaults=True)
    p_wiki_route_dry_run.add_argument("--workspace", type=Path, required=True, help="Markdown wiki workspace")
    p_wiki_route_dry_run.add_argument("--concept-dir", type=Path, required=True, help="Directory of concept pages")
    p_wiki_route_dry_run.add_argument("--audience", default="private", help="Audience tier for talk-folder routing")
    p_wiki_route_dry_run.add_argument(
        "--sources",
        default="codex,claude-code",
        help="Comma-separated source importers that must be fresh enough for session-derived work",
    )
    p_wiki_route_dry_run.add_argument(
        "--max-source-age-hours",
        type=int,
        default=None,
        help="Override runtime_policy.max_importer_staleness_hours for this freshness check",
    )
    p_wiki_route_dry_run.add_argument(
        "--require-fresh",
        action="store_true",
        help="Return a non-zero exit if required source importers are stale or missing",
    )
    p_wiki_route_dry_run.add_argument(
        "--freshness-check",
        choices=["always", "skip"],
        default="always",
        help="Source freshness preflight policy for route dry-runs",
    )
    p_wiki_route_dry_run.add_argument(
        "--write-artifact",
        action="store_true",
        help="Persist the dry-run execution report under memory/runtime and lakestore",
    )
    p_wiki_route_dry_run.add_argument("--json", action="store_true", help="print machine-readable result")
    p_wiki_route_dry_run.set_defaults(func=cmd_wiki_route_dry_run)

    p_wiki_brackify = wiki_sub.add_parser(
        "brackify",
        help="conservatively add [[Subject]] brackets to a markdown file",
    )
    add_context_options(p_wiki_brackify, suppress_defaults=True)
    p_wiki_brackify.add_argument("file", type=Path, help="Markdown file to scan")
    p_wiki_brackify.add_argument("--concept-dir", type=Path, required=True, help="Directory of concept pages")
    p_wiki_brackify.add_argument("--bracket-all", action="store_true", help="Bracket every occurrence, not first per paragraph")
    p_wiki_brackify.add_argument("--write", action="store_true", help="Write changes; default is dry-run")
    p_wiki_brackify.add_argument("--json", action="store_true", help="print machine-readable result")
    p_wiki_brackify.set_defaults(func=cmd_wiki_brackify)

    p_lab = sub.add_parser("lab", help="run small local 1Context lab loops")
    add_context_options(p_lab, suppress_defaults=True)
    p_lab.set_defaults(func=cmd_lab_help)
    lab_sub = p_lab.add_subparsers(dest="lab_command")

    p_lab_hourly = lab_sub.add_parser("hourly-scribe", help="render one hour and prepare or run the Claude hourly scribe")
    add_context_options(p_lab_hourly, suppress_defaults=True)
    p_lab_hourly.add_argument("--date", required=True, help="UTC date YYYY-MM-DD")
    p_lab_hourly.add_argument("--hour", required=True, help="UTC hour 00-23")
    p_lab_hourly.add_argument("--audience", default="private")
    p_lab_hourly.add_argument("--workspace", type=Path, help="Temporary workspace path")
    p_lab_hourly.add_argument("--run-claude", action="store_true", help="Launch Claude Code instead of only preparing the prompt")
    p_lab_hourly.add_argument("--model", default="opus", help="Claude model alias or full model name")
    p_lab_hourly.add_argument("--json", action="store_true", help="print machine-readable result")
    p_lab_hourly.set_defaults(func=cmd_lab_hourly_scribe)

    return parser


def add_context_options(parser: argparse.ArgumentParser, *, suppress_defaults: bool = False) -> None:
    default = argparse.SUPPRESS if suppress_defaults else None
    parser.add_argument("--root", type=Path, default=default, help="Repository root containing 1context.toml")
    parser.add_argument("--plugin", default=default, help="Use a plugin without editing 1context.toml")


def add_agent_install_options(parser: argparse.ArgumentParser) -> None:
    parser.add_argument(
        "--command",
        default="",
        help="Installed 1Context command for hooks to call; defaults to discovered 1context",
    )
    parser.add_argument("--apply", action="store_true", help="Write the default 1Context startup-message config")
    parser.add_argument("--overwrite-config", action="store_true", help="Replace existing startup-message config")
    parser.add_argument("--json", action="store_true", help="print machine-readable result")


def cmd_install_help(args: argparse.Namespace) -> int:
    print("install commands: agent-integrations")
    return 0


def cmd_agent_help(args: argparse.Namespace) -> int:
    print("agent commands: startup-context, install-integrations")
    return 0


def cmd_agent_startup_context(args: argparse.Namespace) -> int:
    hook_input = read_hook_input()
    context = build_startup_context(
        provider=args.provider,
        cwd=args.cwd,
        hook_input=hook_input,
        wiki_url=args.wiki_url,
        template=args.template,
    )
    if args.format == "text":
        if context.enabled:
            print(context.message)
        return 0
    if args.format == "json":
        print(json.dumps(context.diagnostic_payload(), indent=2))
        return 0
    print(json.dumps(context.hook_payload(hook_event_name=args.hook_event_name), separators=(",", ":")))
    return 0


def cmd_agent_install_integrations(args: argparse.Namespace) -> int:
    command = args.command or executable_command()
    plan = build_install_plan(command=command)
    wrote_config = False
    if args.apply:
        wrote_config = write_default_startup_config(plan.startup_config_path, overwrite=args.overwrite_config)
    payload = plan.to_payload()
    payload["applied"] = bool(args.apply)
    payload["wrote_startup_config"] = wrote_config
    payload["note"] = (
        "Prototype writes only the 1Context startup-message config. Public release installer should also merge "
        "Claude settings/plugin and Codex config globally."
    )
    if args.json:
        print(json.dumps(payload, indent=2))
        return 0
    print("1Context agent integration plan")
    print(f"  hook command: {plan.command}")
    print(f"  startup config: {plan.startup_config_path}")
    print(f"  startup config written: {wrote_config}")
    print(f"  claude: {'available' if payload['claude']['available'] else 'not found'}")
    print(f"    settings: {payload['claude']['settings_path']}")
    print("    strategy: installer-managed global settings or Claude plugin shim")
    print(f"  codex: {'available' if payload['codex']['available'] else 'not found'}")
    print(f"    config: {payload['codex']['config_path']}")
    print("    strategy: installer-managed global TOML merge")
    return 0


def cmd_show(args: argparse.Namespace) -> int:
    system = load_system(args.root, args.plugin)
    system_map = compile_system_map(system)

    print("1Context")
    print(f"  root: {system.root}")
    print(f"  config: {system.config_path.name}")
    print(f"  active plugin: {system.plugin['id']} ({system.plugin.get('version', 'unversioned')})")
    print(f"  host: {system.host.get('id', 'host')} trust={system.host.get('trust_mode', 'unspecified')}")
    print(f"  runtime: {system.runtime_dir}")
    print(f"  lakestore: {system.storage_dir}")
    print(
        "  policy: "
        f"max_concurrent_agents={system.runtime_policy['max_concurrent_agents']} "
        f"default_harness_isolation={system.runtime_policy['default_harness_isolation']}"
    )
    print()

    print("Agents")
    if not system.agents:
        print("  (none yet)")
    for agent_id, agent in sorted(system.agents.items()):
        provider = agent.get("provider", "provider")
        model = agent.get("model", "model")
        harness = agent.get("harness", "harness")
        effort = agent.get("effort", "standard")
        print(f"  {agent_id:<22} harness={harness} model={provider}/{model} effort={effort}")
        if agent.get("purpose"):
            print(f"    {agent['purpose']}")
    print()

    print("Harnesses")
    if not system.harnesses:
        print("  (none yet)")
    for harness_id, harness in sorted(system.harnesses.items()):
        primary = harness.get("primary_memory_format", "-")
        runner = harness.get("runner", "-")
        print(f"  {harness_id:<22} runner={runner:<14} memory={primary}")
    print()

    print("Accounts")
    if not system.accounts:
        print("  (none yet)")
    for account_id, account in sorted(system.accounts.items()):
        modes = ", ".join(account.get("modes", [])) or "-"
        default_mode = account.get("default_mode", "-")
        selected = account.get("selected_mode", default_mode)
        status = account.get("selected_mode_status", "-")
        print(f"  {account_id:<22} selected={selected:<22} default={default_mode:<22} status={status} modes={modes}")
    print()

    print("Plugin Dependencies")
    if not system.dependencies:
        print("  (none yet)")
    for dependency_id, dependency in sorted(system.dependencies.items()):
        kind = dependency.get("kind", "dependency")
        required = "required" if dependency.get("required", True) else "optional"
        print(f"  {dependency_id:<22} kind={kind:<18} {required}")
    print()

    print("Custom Tools")
    if not system.custom_tools:
        print("  (none yet)")
    for tool_id, tool in sorted(system.custom_tools.items()):
        kind = tool.get("kind", "custom")
        print(f"  {tool_id:<22} kind={kind}")
    print()

    print("Jobs")
    if not system_map["jobs"]:
        print("  (none yet)")
    for job_id, job in system_map["jobs"].items():
        status = job["status"]
        agent = job.get("agent") or "-"
        custom_tools = ", ".join(job.get("custom_tools", [])) or "-"
        print(f"  {job_id:<22} agent={agent:<22} custom_tools={custom_tools:<20} status={status}")
        if job["missing_host_grants"]:
            print(f"    missing host grants: {', '.join(job['missing_host_grants'])}")
    print()

    print("State Machines")
    if not system.state_machines:
        print("  (none yet)")
    for machine_id, machine in sorted(system.state_machines.items()):
        scopes = ", ".join(scope.get("name", "") for scope in machine.get("scopes", [])) or "-"
        print(f"  {machine_id:<22} v{machine.get('version', '-')} scopes={scopes}")
    print()

    print("Lived Experience")
    for experience_id, experience in sorted(system.lived_experience.items()):
        print(f"  {experience_id:<22} {experience.get('title', '')}")
    return 0


def cmd_map(args: argparse.Namespace) -> int:
    system = load_system(args.root, args.plugin)
    system_map = compile_system_map(system)
    if args.json:
        print(json.dumps(system_map, indent=2))
        return 0

    if not system_map["jobs"] and not system_map["state_machines"]:
        print("(no jobs or state machines defined yet)")
        return 0
    if system_map["jobs"]:
        print("Jobs")
        for job_id, job in system_map["jobs"].items():
            print(job_id)
            print(f"  agent: {job.get('agent') or '-'}")
            print(f"  harness: {job.get('harness') or '-'}")
            print(f"  model: {provider_model(job)}")
            print(f"  harness tools: {', '.join(job.get('harness_tools', [])) or '-'}")
            print(f"  custom tools: {', '.join(job.get('custom_tools', [])) or '-'}")
            print(f"  required accounts: {', '.join(job.get('required_accounts', [])) or '-'}")
            print(f"  required dependencies: {', '.join(job.get('required_dependencies', [])) or '-'}")
            experience_config = job.get("experience_config", {})
            if isinstance(experience_config, dict) and experience_config:
                mode = experience_config.get("mode", "-")
                builder = experience_config.get("builder", "-")
                window = experience_config.get("window", "-")
                print(f"  experience: builder={builder} mode={mode} window={window}")
            print(f"  read: {', '.join(job['permissions'].get('read', [])) or '-'}")
            print(f"  write: {', '.join(job['permissions'].get('write', [])) or '-'}")
            print(f"  deny: {', '.join(job['permissions'].get('deny', [])) or '-'}")
            print(f"  status: {job['status']}")
            for label, values in job["missing"].items():
                if values:
                    print(f"  missing {label}: {', '.join(values)}")
    if system_map["state_machines"]:
        print("State Machines")
        for machine_id, machine in sorted(system_map["state_machines"].items()):
            print(f"  {machine_id:<22} v{machine.get('version', '-')}")
            clocks = ", ".join(clock.get("name", "") for clock in machine.get("clocks", [])) or "-"
            scopes = ", ".join(scope.get("name", "") for scope in machine.get("scopes", [])) or "-"
            print(f"    clocks: {clocks}")
            print(f"    scopes: {scopes}")
            print(f"    artifacts: {len(machine.get('artifacts', []))}")
            print(f"    evidence: {len(machine.get('evidence', []))}")
            print(f"    transitions: {len(machine.get('transitions', []))}")
    return 0


def cmd_plugins(args: argparse.Namespace) -> int:
    system = load_system(args.root, args.plugin)
    for plugin in list_plugins(system.root):
        active = "*" if plugin["id"] == system.active_plugin else " "
        print(f"{active} {plugin['id']:<22} {plugin.get('version', 'unversioned'):<10} {plugin['path']}")
    return 0


def cmd_host(args: argparse.Namespace) -> int:
    system = load_system(args.root, args.plugin)
    print(f"{system.host.get('id', 'host')} trust={system.host.get('trust_mode', 'unspecified')}")
    print("accounts")
    if not system.accounts:
        print("  (none)")
    for account_id, account in sorted(system.accounts.items()):
        modes = ", ".join(account.get("modes", [])) or "-"
        default_mode = account.get("default_mode", "-")
        selected = account.get("selected_mode", default_mode)
        status = account.get("selected_mode_status", "-")
        env = account.get("api_key_env") or account.get("token_env") or "-"
        print(f"  {account_id:<22} selected={selected:<22} status={status:<9} env={env:<24} modes={modes}")
    print("plugin dependencies")
    if not system.dependencies:
        print("  (none)")
    for dependency_id, dependency in sorted(system.dependencies.items()):
        kind = dependency.get("kind", "dependency")
        required = "required" if dependency.get("required", True) else "optional"
        print(f"  {dependency_id:<22} kind={kind:<18} {required}")
    print("allow")
    for grant in system.host.get("allow", []):
        print(f"  {grant}")
    print("deny")
    for grant in system.host.get("deny", []):
        print(f"  {grant}")
    return 0


def cmd_accounts_show(args: argparse.Namespace) -> int:
    system = load_system(args.root, args.plugin)
    if not system.accounts:
        print("(no accounts linked)")
        return 0
    for account_id, account in sorted(system.accounts.items()):
        modes = ", ".join(account.get("modes", [])) or "-"
        selected = account.get("selected_mode", account.get("default_mode", "-"))
        status = account.get("selected_mode_status", "-")
        env = account.get("api_key_env") or account.get("token_env") or "-"
        print(account_id)
        print(f"  selected: {selected} ({status})")
        print(f"  modes: {modes}")
        print(f"  env: {env}")
        for requirement in account.get("required_by", []):
            dependency = requirement.get("dependency", "-")
            auth_modes = ", ".join(requirement.get("auth_modes", [])) or "-"
            models = ", ".join(requirement.get("models", [])) or "-"
            print(f"  required by: {dependency} auth={auth_modes} models={models}")
    return 0


def cmd_accounts_link(args: argparse.Namespace) -> int:
    result = link_accounts(args.root, args.plugin, write=not args.check)
    if args.json:
        payload = {
            "path": str(result.path),
            "changed": result.changed,
            "written": result.changed and not args.check,
            "accounts": result.accounts,
        }
        print(json.dumps(payload, indent=2))
        return 0
    verb = "would update" if args.check else "updated"
    if result.changed:
        print(f"accounts: {verb} {result.path}")
    else:
        print(f"accounts: already current at {result.path}")
    for account_id, account in sorted(result.accounts.items()):
        selected = account.get("selected_mode", account.get("default_mode", "-"))
        status = account.get("selected_mode_status", "-")
        print(f"  {account_id:<22} selected={selected:<22} status={status}")
    return 0


def cmd_harnesses(args: argparse.Namespace) -> int:
    system = load_system(args.root, args.plugin)
    if not system.harnesses:
        print("(no harnesses defined)")
        return 0
    for harness_id, harness in sorted(system.harnesses.items()):
        print(harness_id)
        print(f"  label: {harness.get('label', harness_id)}")
        print(f"  runner: {harness.get('runner', '-')}")
        print(f"  command: {harness.get('command', '-')}")
        print(f"  primary memory: {harness.get('primary_memory_format', '-')}")
        protocols = ", ".join(harness.get("endpoint_protocols", [])) or "-"
        captures = ", ".join(harness.get("captures", [])) or "-"
        default_tools = ", ".join(harness.get("default_tools", [])) or "-"
        optional_tools = ", ".join(harness.get("optional_tools", [])) or "-"
        print(f"  endpoint protocols: {protocols}")
        print(f"  default tools: {default_tools}")
        print(f"  optional tools: {optional_tools}")
        print(f"  captures: {captures}")
        if harness.get("purpose"):
            print(f"  purpose: {harness['purpose']}")
    return 0


def cmd_state_machines(args: argparse.Namespace) -> int:
    system = load_system(args.root, args.plugin)
    system_map = compile_system_map(system)
    payload = {
        "language": system_map["state_machine_language"],
        "state_machines": system.state_machines,
    }
    if args.json:
        print(json.dumps(payload, indent=2))
        return 0
    language = system_map["state_machine_language"]
    selected_runtime = language.get("selected_runtime")
    available = language.get("available_runtimes", [])
    if selected_runtime:
        print(
            f"selected language runtime: {selected_runtime['id']} "
            f"{selected_runtime['version']} ({selected_runtime['compatible_spec']})"
        )
    else:
        print("selected language runtime: none")
    if available:
        print("available language runtimes:")
        for runtime in available:
            print(f"  {runtime['id']:<18} {runtime['version']:<8} {runtime['compatible_spec']}")
    requirements = language.get("requirements", [])
    if requirements:
        print("plugin requirement:")
        for requirement in requirements:
            version_spec = requirement.get("version_spec") or requirement.get("version") or "-"
            required = "required" if requirement.get("required", True) else "optional"
            print(f"  {requirement['id']:<22} {requirement.get('language', '-'):<18} {version_spec:<16} {required}")
    if not system.state_machines:
        print("(no state machines defined)")
        return 0
    for machine_id, machine in sorted(system.state_machines.items()):
        print(machine_id)
        print(f"  title: {machine.get('title', machine_id)}")
        print(f"  version: {machine.get('version', '-')}")
        print(f"  source: {machine.get('source_path', '-')}")
        clocks = ", ".join(clock.get("name", "") for clock in machine.get("clocks", [])) or "-"
        scopes = ", ".join(scope.get("name", "") for scope in machine.get("scopes", [])) or "-"
        signals = ", ".join(signal.get("name", "") for signal in machine.get("signals", [])) or "-"
        print(f"  clocks: {clocks}")
        print(f"  scopes: {scopes}")
        print(f"  artifacts: {len(machine.get('artifacts', []))}")
        print(f"  evidence: {len(machine.get('evidence', []))}")
        print(f"  signals: {signals}")
        print(f"  transitions: {len(machine.get('transitions', []))}")
    return 0


def cmd_state_machine_diagram(args: argparse.Namespace) -> int:
    system = load_system(args.root, args.plugin)
    system_map = compile_system_map(system)
    machine = system_map["state_machines"].get(args.machine_id)
    if not machine:
        print(f"state-machine error: unknown state machine {args.machine_id!r}", file=sys.stderr)
        return 1
    try:
        source = state_machine_to_mermaid(machine, scope_name=args.scope)
    except StateMachineDiagramError as exc:
        print(f"state-machine error: {exc}", file=sys.stderr)
        return 1
    if args.output:
        target = args.output.resolve()
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(source, encoding="utf-8")
        print(f"wrote {target}")
        return 0
    print(source, end="")
    return 0


def cmd_state_machine_compile(args: argparse.Namespace) -> int:
    system = load_system(args.root, args.plugin)
    result = compile_state_machine_artifacts(system, output_dir=args.output, run_id=args.run_id)
    payload = result.to_payload(system.root)
    if args.json:
        print(json.dumps(payload, indent=2))
        return 0
    print("state-machine production compile")
    print(f"run id: {result.run_id}")
    print(f"path: {result.path}")
    print(f"machines: {', '.join(result.machines)}")
    print(f"files: {len(result.files)}")
    return 0


def cmd_state_machine_verify(args: argparse.Namespace) -> int:
    system = load_system(args.root, args.plugin)
    result = verify_state_machine_artifacts(system, output_dir=args.output, run_id=args.run_id)
    payload = result.to_payload(system.root)
    if args.json:
        print(json.dumps(payload, indent=2))
        return 0 if result.passed else 2
    print("state-machine production verification")
    print(f"run id: {result.run_id}")
    print(f"path: {result.path}")
    print(f"passed: {'yes' if result.passed else 'no'}")
    failed = [check for check in result.checks if check["status"] == "failed"]
    warnings = [check for check in result.checks if check["status"] == "warning"]
    print(f"checks: {len(result.checks)}")
    print(f"failed: {len(failed)}")
    print(f"warnings: {len(warnings)}")
    for check in result.checks:
        prefix = "pass" if check["status"] == "passed" else check["status"]
        print(f"  {prefix:<7} {check['id']}: {check['detail']}")
    return 0 if result.passed else 2


def cmd_memory_show(args: argparse.Namespace) -> int:
    system = load_system(args.root, args.plugin)
    print(f"runtime memory: {system.runtime_dir}")
    print(f"ledger: {ledger_events_path(system.runtime_dir)}")
    print(f"lakestore: {system.storage_dir}")
    print(f"runs: {system.runtime_dir / 'runs'}")
    print(f"experiences: {system.runtime_dir / 'experiences'}")
    print(f"proposals: {system.runtime_dir / 'proposals'}")
    print()
    policy = dict(system.linking)
    print(f"linker: {policy.get('linker')} {policy.get('linker_version')}")
    print(f"ledger schema: {policy.get('ledger_schema_version')} (writer {LEDGER_SCHEMA_VERSION})")
    print(f"implementation: {LINKER_ID} {LINKER_VERSION}")
    if policy.get("linker") == LINKER_ID and str(policy.get("linker_version")) == LINKER_VERSION:
        print("linker status: active")
    else:
        print("linker status: mismatch")
    print(f"default attach: {policy.get('default_attach')}")
    print(f"create if missing: {policy.get('create_if_missing')}")
    print(f"lived-experience start: {policy.get('lived_experience_start')}")
    print(f"inject order: {' -> '.join(policy.get('inject_order', []))}")
    scope = policy.get("scope", {})
    if scope:
        print("scope")
        for key, value in scope.items():
            print(f"  {key}: {value}")
    print()
    print("native memory formats:")
    for format_id, memory_format in configured_native_memory_formats(system).items():
        print(f"  {format_id:<22} {memory_format.get('path')}")
    print()
    print("attach modes")
    print("  new             create a fresh runtime experience id for this run")
    print("  last_for_job    attach to the last experience matched by the configured scope")
    print("  manual          attach to an explicit runtime experience id")
    print("  none            run without runtime experience")
    return 0


def cmd_memory_replay_dry_run(args: argparse.Namespace) -> int:
    system = load_system(args.root, args.plugin)
    sources = tuple(item.strip() for item in args.sources.split(",") if item.strip())
    try:
        result = run_replay_dry_run(
            system,
            start=args.start,
            end=args.end,
            sources=sources,
            replay_run_id=args.replay_run_id,
            sandbox=args.sandbox,
            failure_injections=tuple(args.inject_failure),
            operator_edit_injections=tuple(args.inject_operator_edit),
        )
    except ReplayError as exc:
        print(f"memory replay error: {exc}", file=sys.stderr)
        return 1

    payload = result.to_payload()
    if args.json:
        print(json.dumps(payload, indent=2))
        return 0

    print("memory replay dry-run")
    print(f"run id: {result.replay_run_id}")
    print(f"window: {result.start} -> {result.end}")
    print(f"sources: {', '.join(result.sources) or '-'}")
    print(f"events: {result.event_count}")
    print(f"fires: {result.fire_count}")
    print(f"sandbox: {'yes' if result.sandbox.get('enabled') else 'no'}")
    print(f"injections: {len(result.injections)}")
    for agent, count in payload["fires_by_agent"].items():
        print(f"  {agent}: {count}")
    print(f"path: {result.path}")
    print(f"artifact id: {result.artifact_id}")
    return 0


def cmd_memory_tick(args: argparse.Namespace) -> int:
    system = load_system(args.root, args.plugin)
    sources = tuple(item.strip() for item in args.sources.split(",") if item.strip())
    try:
        result = run_memory_tick(
            system,
            wiki_only=args.wiki_only,
            workspace=args.workspace,
            concept_dir=args.concept_dir,
            audience=args.audience,
            sources=sources,
            max_source_age_hours=args.max_source_age_hours,
            require_fresh=args.require_fresh,
            freshness_check=args.freshness_check,
            execute_render=args.execute_render,
            execute_route_hires=args.execute_route_hires,
            route_hire_limit=args.route_hire_limit,
            route_hire_run_harness=args.run_route_harness,
            promote_route_outputs=args.promote_route_outputs,
            route_promotion_operator_approval=args.operator_approval,
            render_family_ids=tuple(args.render_family),
            include_talk=not args.skip_talk,
            record_evidence=not args.no_evidence,
            retry_budget=args.retry_budget,
            execute_migrations=args.run_migrations,
            cycle_id=args.cycle_id,
        )
    except MemoryTickError as exc:
        print(f"memory tick error: {exc}", file=sys.stderr)
        return 1

    payload = result.to_payload()
    if args.json:
        print(json.dumps(payload, indent=2))
        return 2 if result.status in {"blocked", "retryable"} else (1 if result.status == "failed" else 0)

    print("memory tick")
    print(f"cycle id: {result.cycle_id}")
    print(f"mode: {result.mode}")
    print(f"status: {result.status}")
    print(f"dry run: {'yes' if result.dry_run else 'no'}")
    print(f"planned hires: {result.planned_hire_count}")
    print(f"non-hire outcomes: {result.non_hire_count}")
    print(f"route hire dry-runs: {result.route_hire_count}")
    print(f"route hire errors: {result.route_hire_error_count}")
    print(f"renders: {result.render_count}")
    print(f"manifests: {result.manifest_count}")
    print(f"routes: {result.route_count}")
    print(f"path: {result.path}")
    print(f"artifact id: {result.artifact_id}")
    return 2 if result.status in {"blocked", "retryable"} else (1 if result.status == "failed" else 0)


def cmd_memory_cycles_list(args: argparse.Namespace) -> int:
    system = load_system(args.root, args.plugin)
    cycles = list_memory_cycles(system, limit=getattr(args, "limit", 20))
    payload = {"cycles": [cycle.to_payload() for cycle in cycles]}
    if getattr(args, "json", False):
        print(json.dumps(payload, indent=2))
        return 0
    if not cycles:
        print("(no memory cycles)")
        return 0
    print("memory cycles")
    for cycle in cycles:
        dry = "dry" if cycle.dry_run else "run"
        print(
            f"  {cycle.cycle_id:<32} {cycle.status:<10} {dry:<3} "
            f"renders={cycle.render_count:<2} routes={cycle.route_count:<3} {cycle.created_at}"
        )
    return 0


def cmd_memory_cycles_show(args: argparse.Namespace) -> int:
    system = load_system(args.root, args.plugin)
    try:
        payload = load_memory_cycle(system, args.cycle_id)
    except MemoryTickError as exc:
        print(f"memory cycle error: {exc}", file=sys.stderr)
        return 1
    if args.json:
        print(json.dumps(payload, indent=2))
        return 0
    print(f"cycle id: {payload.get('cycle_id')}")
    print(f"status: {payload.get('status')}")
    print(f"mode: {payload.get('mode')}")
    print(f"dry run: {'yes' if payload.get('dry_run') else 'no'}")
    route_table = payload.get("route_table", {})
    print(f"manifests: {route_table.get('manifest_count', 0)}")
    print(f"routes: {route_table.get('route_count', 0)}")
    print("steps:")
    for step in payload.get("steps", []):
        print(f"  {step.get('id')}: {step.get('status')}")
    print(f"path: {system.runtime_dir / 'cycles' / args.cycle_id / 'cycle.json'}")
    return 0


def cmd_memory_cycles_validate(args: argparse.Namespace) -> int:
    system = load_system(args.root, args.plugin)
    try:
        result = validate_memory_cycle(system, args.cycle_id)
    except MemoryTickError as exc:
        print(f"memory cycle error: {exc}", file=sys.stderr)
        return 1
    payload = result.to_payload()
    if args.json:
        print(json.dumps(payload, indent=2))
        return 0 if result.passed else 2
    print("memory cycle validation")
    print(f"cycle id: {result.cycle_id}")
    print(f"passed: {'yes' if result.passed else 'no'}")
    print(f"artifact id: {result.artifact_id or '-'}")
    print(f"event id: {result.event_id or '-'}")
    for check in result.checks:
        status = "pass" if check["passed"] else "fail"
        print(f"  {status:<4} {check['id']}: {check['detail']}")
    return 0 if result.passed else 2


def cmd_memory_migrations_list(args: argparse.Namespace) -> int:
    system = load_system(args.root, args.plugin)
    try:
        definitions = load_migration_definitions(system)
    except MigrationError as exc:
        print(f"memory migration error: {exc}", file=sys.stderr)
        return 1
    payload = {"migrations": [definition.to_payload() for definition in definitions]}
    if getattr(args, "json", False):
        print(json.dumps(payload, indent=2))
        return 0
    if not definitions:
        print("(no memory migrations)")
        return 0
    print("memory migrations")
    for definition in definitions:
        print(f"  {definition.migration_id:<48} {definition.kind:<12} {definition.title}")
    return 0


def cmd_memory_migrations_run(args: argparse.Namespace) -> int:
    system = load_system(args.root, args.plugin)
    try:
        result = run_contract_migrations(system, run_id=getattr(args, "run_id", ""))
    except MigrationError as exc:
        print(f"memory migration error: {exc}", file=sys.stderr)
        return 1
    payload = result.to_payload()
    if getattr(args, "json", False):
        print(json.dumps(payload, indent=2))
        return 0 if result.status == "passed" else 1
    print("memory migrations")
    print(f"run id: {result.run_id}")
    print(f"status: {result.status}")
    print(f"applied: {result.applied_count}")
    print(f"already current: {result.already_current_count}")
    print(f"failed: {result.failed_count}")
    print(f"path: {result.receipt_dir}")
    print(f"artifact id: {result.artifact_id}")
    return 0 if result.status == "passed" else 1


def cmd_memory_quality(args: argparse.Namespace) -> int:
    system = load_system(args.root, args.plugin)
    try:
        report = run_quality_probes(
            args.path,
            now=args.now or None,
            stale_current_state_days=args.stale_current_state_days,
        )
        record = {} if args.no_record else write_quality_report(system, report, run_id=args.run_id)
    except QualityError as exc:
        print(f"memory quality error: {exc}", file=sys.stderr)
        return 1
    payload = report.to_payload()
    if record:
        payload["record"] = record
    if args.json:
        print(json.dumps(payload, indent=2))
        return 0 if report.passed else 2
    print("memory quality")
    print(f"path: {report.root}")
    print(f"passed: {'yes' if report.passed else 'no'}")
    print(f"files: {report.file_count}")
    print(f"issues: {report.issue_count}")
    if record:
        print(f"artifact id: {record['artifact_id']}")
    for issue in report.issues[:20]:
        print(f"  {issue.severity:<7} {issue.code:<28} {issue.path}:{issue.line} {issue.title}")
    if report.issue_count > 20:
        print(f"  ... {report.issue_count - 20} more")
    return 0 if report.passed else 2


def cmd_memory_wiki_apply(args: argparse.Namespace) -> int:
    system = load_system(args.root, args.plugin)
    try:
        route_row = load_apply_route_row(args)
        run_id = args.run_id or f"wiki-apply-{int(time.time())}"
        sandbox_root = args.sandbox_root or (system.runtime_dir / "wiki" / "apply-sandboxes" / run_id)
        result = apply_curator_decision_to_sandbox(
            source_workspace=args.source_workspace,
            decision_path=args.decision,
            route_row=route_row,
            sandbox_root=sandbox_root,
        )
        record = {} if args.no_record else write_wiki_apply_result(system, result, run_id=run_id)
        promotion = None
        promotion_record = {}
        if args.promote_to_source:
            promotion = promote_wiki_apply_result_to_source(
                system,
                result,
                run_id=run_id,
                operator_approval=args.operator_approval,
            )
            promotion_record = {} if args.no_record else write_wiki_apply_promotion_result(system, promotion, run_id=run_id)
    except (OSError, WikiApplyError, json.JSONDecodeError) as exc:
        print(f"memory wiki-apply error: {exc}", file=sys.stderr)
        return 1
    payload = result.to_payload()
    if record:
        payload["record"] = record
    if promotion is not None:
        payload["promotion"] = promotion.to_payload()
    if promotion_record:
        payload["promotion_record"] = promotion_record
    if args.json:
        print(json.dumps(payload, indent=2))
        return 0 if result.ok and (promotion is None or promotion.ok) else 2
    print("memory wiki-apply")
    print(f"status: {result.status}")
    print(f"section: {result.section}")
    print(f"sandbox: {result.sandbox_workspace}")
    print(f"changed paths: {', '.join(result.diff.get('changed_paths', [])) or '-'}")
    if record:
        print(f"artifact id: {record['artifact_id']}")
    if promotion is not None:
        print(f"promotion: {promotion.status}")
        print(f"backup: {promotion.backup_path}")
    if promotion_record:
        print(f"promotion artifact id: {promotion_record['artifact_id']}")
    for failure in result.failures:
        print(f"  failure: {failure}")
    if promotion is not None:
        for failure in promotion.failures:
            print(f"  promotion failure: {failure}")
    return 0 if result.ok and (promotion is None or promotion.ok) else 2


def load_apply_route_row(args: argparse.Namespace) -> dict[str, Any]:
    if args.route_row_json:
        raw = json.loads(args.route_row_json.read_text(encoding="utf-8"))
        if not isinstance(raw, dict):
            raise WikiApplyError("--route-row-json must contain a JSON object")
        return raw
    if not args.article or not args.section:
        raise WikiApplyError("either --route-row-json or both --article and --section are required")
    article_path = args.article
    article_value = str(article_path.resolve()) if not article_path.is_absolute() and article_path.exists() else str(article_path)
    return {
        "route_id": "ad-hoc-wiki-apply",
        "job": "memory.wiki.curator",
        "ownership": {
            "kind": "article_sections",
            "path": article_value,
            "sections": [args.section],
        },
    }


def cmd_memory_schedule(args: argparse.Namespace) -> int:
    system = load_system(args.root, args.plugin)
    store = LakeStore(system.storage_dir)
    sources = tuple(item.strip() for item in args.sources.split(",") if item.strip())
    try:
        plan = plan_scheduler_tick(
            store,
            start=args.start,
            end=args.end,
            sources=sources,
            max_source_age_hours=args.max_source_age_hours,
            require_fresh=not args.allow_stale,
            now=args.now or None,
        )
        record = {} if args.no_record else write_scheduler_plan(system, plan, run_id=args.run_id)
    except SchedulerError as exc:
        print(f"memory schedule error: {exc}", file=sys.stderr)
        return 1
    payload = plan.to_payload()
    if record:
        payload["record"] = record
    if args.json:
        print(json.dumps(payload, indent=2))
        return 2 if plan.blocked_count else 0
    print("memory schedule")
    print(f"window: {plan.start} -> {plan.end}")
    print(f"fires: {plan.fire_count}")
    print(f"ready: {plan.ready_count}")
    print(f"blocked: {plan.blocked_count}")
    if record:
        print(f"artifact id: {record['artifact_id']}")
    return 2 if plan.blocked_count else 0


def cmd_memory_health(args: argparse.Namespace) -> int:
    system = load_system(args.root, args.plugin)
    store = LakeStore(system.storage_dir)
    payload = build_memory_health_payload(store)
    record = {} if args.no_record else write_memory_health_artifact(system, payload, run_id=args.run_id)
    if record:
        payload["record"] = record
    if args.json:
        print(json.dumps(payload, indent=2))
        return 0 if payload.get("status") in {"healthy", "blocked"} else 2
    print("memory health")
    print(f"status: {payload.get('status')}")
    print(f"success phases: {payload.get('summary', {}).get('success_phase_count', 0)}")
    print(f"failure phases: {payload.get('summary', {}).get('failure_phase_count', 0)}")
    if record:
        print(f"artifact id: {record['artifact_id']}")
    return 0 if payload.get("status") in {"healthy", "blocked"} else 2


def cmd_storage_show(args: argparse.Namespace) -> int:
    system = load_system(args.root, args.plugin)
    store = LakeStore(system.storage_dir)
    print(f"lakestore: {system.storage_dir}")
    print("engine: lancedb")
    try:
        counts = store.counts()
    except StorageError as exc:
        print(f"storage error: {exc}", file=sys.stderr)
        return 1
    print("tables")
    for table_name in TABLE_ORDER:
        count = counts.get(table_name)
        state = "missing" if count is None else f"{count} row(s)"
        print(f"  {table_name:<12} {state}")
    return 0


def cmd_storage_init(args: argparse.Namespace) -> int:
    system = load_system(args.root, args.plugin)
    store = LakeStore(system.storage_dir)
    counts = store.ensure()
    print(f"initialized lakestore: {system.storage_dir}")
    for table_name in TABLE_ORDER:
        print(f"  {table_name:<12} {counts.get(table_name, 0)} row(s)")
    return 0


def cmd_storage_smoke(args: argparse.Namespace) -> int:
    system = load_system(args.root, args.plugin)
    store = LakeStore(system.storage_dir)
    store.ensure()
    event = store.append_event(
        "storage.smoke",
        source="1context-cli",
        actor="local",
        subject="lakestore",
        text="Storage smoke event written by the 1Context CLI.",
        payload={"plugin": system.active_plugin},
    )
    artifact = store.append_artifact(
        "storage_smoke_artifact",
        uri=f"lancedb://{system.storage_dir}/events/{event['event_id']}",
        source="1context-cli",
        state="produced",
        text="Smoke artifact proving the lakestore can record a durable thing.",
        metadata={"event_id": event["event_id"]},
    )
    evidence = store.append_evidence(
        "storage_smoke.valid",
        artifact_id=artifact["artifact_id"],
        checker="1context-cli",
        text="Smoke artifact row exists and was written after table initialization.",
        checks=["events table accepts rows", "artifacts table accepts rows", "evidence table accepts rows"],
    )
    print(f"lakestore: {system.storage_dir}")
    print(f"event: {event['event_id']}")
    print(f"artifact: {artifact['artifact_id']}")
    print(f"evidence: {evidence['evidence_id']}")
    return 0


def cmd_storage_events(args: argparse.Namespace) -> int:
    system = load_system(args.root, args.plugin)
    store = LakeStore(system.storage_dir)
    rows = store.rows("events", limit=args.limit)
    if not rows:
        print("(no lakestore events)")
        return 0
    for row in rows:
        kind = row.get("kind") or row.get("actor") or "-"
        print(
            f"{row.get('ts', '')} {row.get('event', ''):<28} "
            f"source={row.get('source') or '-'} kind={kind} "
            f"subject={row.get('subject') or '-'} id={row.get('event_id')}"
        )
    return 0


def cmd_storage_search(args: argparse.Namespace) -> int:
    system = load_system(args.root, args.plugin)
    store = LakeStore(system.storage_dir)
    rows = store.search(args.table, args.query, limit=args.limit)
    if not rows:
        print(f"(no {args.table} rows matched {args.query!r})")
        return 0
    for row in rows:
        row_id = row_id_for_table(args.table, row)
        ts = row.get("ts") or row.get("first_ts") or row.get("last_ts") or "-"
        title = row.get("event") or row.get("kind") or row.get("session_id") or row.get("title") or args.table
        source = row.get("source") or row.get("checker") or "-"
        snippet = search_snippet(row, args.query)
        print(f"{ts} {args.table:<10} {title:<28} source={source} id={row_id}")
        if snippet:
            print(f"  {snippet}")
    return 0


def row_id_for_table(table_name: str, row: dict[str, Any]) -> str:
    id_fields = {
        "events": "event_id",
        "sessions": "session_id",
        "artifacts": "artifact_id",
        "evidence": "evidence_id",
        "documents": "document_id",
    }
    preferred = id_fields.get(table_name)
    return str(
        row.get(preferred or "")
        or row.get("event_id")
        or row.get("session_id")
        or row.get("artifact_id")
        or row.get("evidence_id")
        or row.get("document_id")
        or "-"
    )


def search_snippet(row: dict[str, Any], query: str, width: int = 180) -> str:
    text = row.get("text") or row.get("payload_json") or row.get("metadata_json") or json.dumps(row, sort_keys=True)
    text = str(text).replace("\n", " ")
    index = text.casefold().find(query.casefold())
    if index < 0:
        return text[:width]
    start = max(0, index - 50)
    end = min(len(text), index + width)
    prefix = "..." if start else ""
    suffix = "..." if end < len(text) else ""
    return prefix + text[start:end] + suffix


def cmd_storage_export(args: argparse.Namespace) -> int:
    system = load_system(args.root, args.plugin)
    store = LakeStore(system.storage_dir)
    store.ensure()
    payload = store.snapshot(limit=args.limit)
    payload["root"] = str(system.root)
    payload["active_plugin"] = system.active_plugin
    payload["state_machines"] = {
        machine_id: {
            "version": machine.get("version"),
            "title": machine.get("title", machine_id),
            "artifacts": len(machine.get("artifacts", [])),
            "evidence": len(machine.get("evidence", [])),
            "transitions": len(machine.get("transitions", [])),
        }
        for machine_id, machine in sorted(system.state_machines.items())
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(payload, indent=2), encoding="utf-8")
    print(f"wrote {args.output}")
    return 0


def cmd_ports(args: argparse.Namespace) -> int:
    system = load_system(args.root, args.plugin)
    ports = [port.to_payload() for port in load_ports(system.root)]
    if args.json:
        print(json.dumps({"ports": ports}, indent=2))
        return 0
    if not ports:
        print("(no ports defined)")
        return 0
    for port in ports:
        enabled = "enabled" if port["enabled"] else "disabled"
        directions = ",".join(port["directions"]) or "-"
        stores = ",".join(port["stores"]) or "-"
        print(f"{port['id']:<18} {enabled:<8} adapter={port['adapter']} directions={directions}")
        print(f"  paths: {', '.join(port['paths']) or '-'}")
        print(f"  stores: {stores}")
        if port.get("since"):
            print(f"  since: {port['since']}")
        caps = []
        if port.get("max_events_per_tick"):
            caps.append(f"events/tick={port['max_events_per_tick']}")
        if port.get("max_lines_per_tick"):
            caps.append(f"lines/tick={port['max_lines_per_tick']}")
        if caps:
            print(f"  caps: {', '.join(caps)}")
        if port.get("settings_path"):
            print(f"  settings: {port['settings_path']}")
        if port.get("purpose"):
            print(f"  purpose: {port['purpose']}")
    return 0


def cmd_daemon_once(args: argparse.Namespace) -> int:
    result = daemon_run_once(root=args.root, active_plugin=args.plugin, experience_source=args.experience_source)
    if args.json:
        print(json.dumps(result.to_payload(), indent=2))
        return 0
    print_daemon_tick(result.to_payload())
    return 0


def cmd_daemon_watch(args: argparse.Namespace) -> int:
    for result in daemon_watch(
        root=args.root,
        active_plugin=args.plugin,
        experience_source=args.experience_source,
        interval=args.interval,
        ticks=args.ticks,
    ):
        print_daemon_tick(result.to_payload())
    return 0


def cmd_daemon_backfill(args: argparse.Namespace) -> int:
    ticks: list[dict[str, Any]] = []
    count = 0
    while True:
        result = daemon_run_once(root=args.root, active_plugin=args.plugin, experience_source=args.experience_source)
        payload = result.to_payload()
        ticks.append(payload)
        count += 1
        if not args.json:
            print_daemon_tick(payload)
        if not payload.get("limited"):
            break
        if args.max_ticks and count >= args.max_ticks:
            break
    if args.json:
        print(
            json.dumps(
                {
                    "ticks": ticks,
                    "tick_count": len(ticks),
                    "limited": bool(ticks[-1].get("limited")) if ticks else False,
                },
                indent=2,
            )
        )
    return 0


def print_daemon_tick(payload: dict[str, Any]) -> None:
    limited = " limited=true" if payload.get("limited") else ""
    print(
        f"daemon tick {payload['ts']} "
        f"lines={payload.get('lines_scanned', 0)} events={payload['events_imported']} "
        f"sessions={payload['sessions_imported']} artifacts={payload['artifacts_imported']}"
        f"{limited} id={payload['tick_event_id']}"
    )
    for port in payload["port_results"]:
        state = "skipped" if port.get("skipped") else "scanned"
        if port.get("limited"):
            state = "limited"
        reason = f" ({port['reason']})" if port.get("reason") else ""
        print(
            f"  {port['port_id']:<18} {state:<7}{reason} "
            f"files={port['files_seen']} changed={port['files_changed']} "
            f"lines={port.get('lines_scanned', 0)} "
            f"events={port['events_imported']} dupes={port.get('duplicate_events', 0)} "
            f"skipped={port.get('events_skipped', 0)} "
            f"sessions={port['sessions_imported']}"
        )


def cmd_apps_list(args: argparse.Namespace) -> int:
    system = load_system(args.root, args.plugin)
    apps = [app.to_payload() for app in load_apps(system.root)]
    if args.json:
        print(json.dumps({"apps": apps}, indent=2))
        return 0
    if not apps:
        print("(no apps defined)")
        return 0
    for app in apps:
        print(f"{app['id']:<12} {app['label']}")
        print(f"  path: {app['path']}")
        print(f"  command: {' '.join(app['command'])}")
        print(f"  url: {app['url'] or '-'}")
    return 0


def cmd_apps_status(args: argparse.Namespace) -> int:
    system = load_system(args.root, args.plugin)
    statuses = app_status(system)
    if getattr(args, "json", False):
        print(json.dumps({"apps": statuses}, indent=2))
        return 0
    if not statuses:
        print("(no apps defined)")
        return 0
    for item in statuses:
        pid = item["pid"] or "-"
        print(f"{item['id']:<12} {item['status']:<8} health={item['health']:<8} pid={pid} url={item['url'] or '-'}")
        print(f"  log: {item['log_path']}")
    return 0


def cmd_apps_start(args: argparse.Namespace) -> int:
    system = load_system(args.root, args.plugin)
    store = LakeStore(system.storage_dir)
    store.ensure()
    result = start_app(system, args.app_id, store=store)
    if args.json:
        print(json.dumps(result, indent=2))
        return 0
    print(f"{result['status']}: {args.app_id}")
    print(f"  pid: {result.get('pid', '-')}")
    print(f"  url: {result.get('url', '-')}")
    print(f"  log: {result.get('log_path', '-')}")
    return 0


def cmd_apps_stop(args: argparse.Namespace) -> int:
    system = load_system(args.root, args.plugin)
    store = LakeStore(system.storage_dir)
    store.ensure()
    result = stop_app(system, args.app_id, store=store)
    if args.json:
        print(json.dumps(result, indent=2))
        return 0
    print(f"{result['status']}: {args.app_id}")
    if result.get("pid"):
        print(f"  pid: {result['pid']}")
    return 0


def cmd_apps_open(args: argparse.Namespace) -> int:
    system = load_system(args.root, args.plugin)
    result = open_app(system, args.app_id)
    print(f"opened: {result['url']}")
    return 0


def provider_model(job: dict[str, object]) -> str:
    provider = job.get("provider") or "-"
    model = job.get("model") or "-"
    return f"{provider}/{model}" if provider != "-" or model != "-" else "-"


def cmd_native_route(args: argparse.Namespace) -> int:
    system = load_system(args.root, args.plugin)
    formats = configured_native_memory_formats(system)
    providers = configured_providers(system)
    experience_path = runtime_experience_dir(system, args.experience_id) if args.experience_id else None
    if experience_path and not experience_path.is_dir():
        print(f"memory error: runtime memory {args.experience_id!r} does not exist", file=sys.stderr)
        return 1

    if experience_path and not (args.harness or args.provider or args.model):
        print(f"runtime memory: {args.experience_id}")
        print("native memory is selected by harness, provider, or model.")
        for format_id, path in native_memory_paths_for_experience(system, experience_path).items():
            state = "exists" if path.exists() else "not created"
            print(f"memory {format_id}: {path} ({state})")
        return 0

    if args.harness or args.provider or args.model or args.experience_id:
        try:
            route = resolve_native_memory_route(
                system,
                harness=args.harness,
                provider=args.provider,
                model=args.model,
                experience_path=experience_path,
            )
        except ExperienceError as exc:
            print(f"memory error: {exc}", file=sys.stderr)
            return 1
        if route.get("harness"):
            print(f"harness: {route['harness']}")
        print(f"provider: {route['provider']}")
        if route.get("model"):
            print(f"model: {route['model']}")
        print(f"native format: {route['read_memory_format']}")
        if route.get("read_kind"):
            print(f"native kind: {route['read_kind']}")
        if route.get("read_path"):
            print(f"native path: {route['read_path']}")
        print(f"writes: {', '.join(route['write_memory_formats'])}")
        return 0

    print("Native Memory Formats")
    for format_id, memory_format in formats.items():
        print(f"  {format_id:<22} {memory_format.get('path')}")
    print()
    print("Provider Routes")
    for provider_id, provider in providers.items():
        aliases = ", ".join(provider.get("aliases", [])) or "-"
        patterns = ", ".join(provider.get("model_patterns", [])) or "-"
        print(f"  {provider_id:<22} reads={provider.get('read_memory_format')} aliases={aliases}")
        print(f"    models: {patterns}")
    return 0


def cmd_hire(args: argparse.Namespace) -> int:
    system = load_system(args.root, args.plugin)
    try:
        result = hire_agent(
            system,
            job_ids=args.job,
            agent_id=args.agent,
            harness_id=args.harness or "",
            provider_id=args.provider or "",
            model=args.model or "",
            mode=args.mode,
            experience_id=args.experience_id,
            run_id=args.run_id,
            job_params=parse_key_values(args.job_param),
        )
    except HireError as exc:
        print(f"hire error: {exc}", file=sys.stderr)
        return 1

    payload = result.asdict()
    if result.path and (args.harness or args.provider or args.model):
        try:
            payload["native_memory_route"] = resolve_native_memory_route(
                system,
                harness=args.harness,
                provider=args.provider,
                model=args.model,
                experience_path=result.path,
            )
        except ExperienceError as exc:
            print(f"memory error: {exc}", file=sys.stderr)
            return 1
    if args.json:
        print(json.dumps(payload, indent=2))
        return 0

    if result.mode == "none":
        print("runtime memory: none")
    else:
        created = " created" if result.created else ""
        print(f"runtime memory: {result.experience_id}{created}")
        print(f"hired-agent uuid: {result.hired_agent_uuid}")
        print(f"path: {result.path}")
        if "native_memory_route" in payload:
            route = payload["native_memory_route"]
            print(f"selected native memory: {route['read_memory_format']} -> {route['read_path']}")
        else:
            print("native memory: selected by provider/model when a harness is not specified")
    print(f"mode: {result.mode}")
    print(f"ledger: {ledger_events_path(system.runtime_dir)}")
    return 0


def parse_key_values(values: list[str]) -> dict[str, str]:
    parsed: dict[str, str] = {}
    for raw in values:
        if "=" not in raw:
            raise HireError(f"--job-param must be KEY=VALUE, got {raw!r}")
        key, value = raw.split("=", 1)
        key = key.strip()
        if not key:
            raise HireError(f"--job-param key cannot be empty: {raw!r}")
        parsed[key] = value.strip()
    return parsed


def format_key_values(value: object) -> str:
    if not isinstance(value, dict) or not value:
        return "-"
    return ",".join(f"{key}={item}" for key, item in sorted(value.items()))


def cmd_ledger(args: argparse.Namespace) -> int:
    system = load_system(args.root, args.plugin)
    ledger = Ledger(ledger_events_path(system.runtime_dir))
    events = ledger.read(limit=args.limit)
    if not events:
        print("(ledger empty)")
        return 0
    for event in events:
        ts = event.get("ts", "")
        event_name = event.get("event", "")
        attachment = event.get("attachment", {})
        experience_id = attachment.get("experience_id", "-") if isinstance(attachment, dict) else "-"
        hired_agent_uuid = event.get("hired_agent_uuid", "-")
        job_ids = event.get("job_ids", [])
        job_label = ",".join(str(item) for item in job_ids) or "-"
        job_params = format_key_values(event.get("job_params", {}))
        plugin_id = event.get("plugin_id", event.get("plugin", "-"))
        print(
            f"{ts} {event_name:<28} "
            f"hired_agent={hired_agent_uuid} experience={experience_id} jobs={job_label} "
            f"params={job_params} plugin={plugin_id}"
        )
    return 0


def cmd_birth(args: argparse.Namespace) -> int:
    system = load_system(args.root, args.plugin)
    ledger = Ledger(ledger_events_path(system.runtime_dir))
    try:
        event = select_birth_event(ledger.read(), hired_agent_uuid=args.uuid)
    except BirthCertificateError as exc:
        if not args.uuid:
            print("(no birth certificates)")
            return 0
        print(f"birth certificate error: {exc}", file=sys.stderr)
        return 1
    if args.json:
        print(json.dumps(event, indent=2))
        return 0
    print(render_birth_certificate(event))
    return 0


def cmd_lab_help(args: argparse.Namespace) -> int:
    print("lab commands:")
    print("  hourly-scribe    render one hour into lived experience and prepare/run a Claude hourly scribe")
    return 0


def cmd_job_help(args: argparse.Namespace) -> int:
    print("job commands:")
    print("  run JOB_ID       prepare or run one manifest-driven memory job")
    return 0


def cmd_wiki_help(args: argparse.Namespace) -> int:
    print("wiki commands:")
    print("  list             list wiki page families")
    print("  ensure           create missing page/talk/template scaffolding")
    print("  render           render wiki families and record render evidence")
    print("  routes           show rendered localhost route table")
    print("  serve            serve rendered wiki pages over localhost")
    print("  open             open or print a rendered wiki URL")
    print("  build-inputs     generate wiki indexes, links, backlinks, landing, and digest")
    print("  plan-roles       derive the dynamic agent role route plan")
    print("  route-dry-run    preview hired-agent births from the route plan")
    print("  brackify         add [[Subject]] brackets to markdown")
    return 0


def cmd_wiki_build_inputs(args: argparse.Namespace) -> int:
    try:
        result = build_wiki_inputs(
            workspace=args.workspace,
            concept_dir=args.concept_dir,
            staging=args.staging,
            web_base=args.web_base,
        )
    except WikiError as exc:
        print(f"wiki error: {exc}", file=sys.stderr)
        return 1

    payload = result.to_payload()
    if args.json:
        print(json.dumps(payload, indent=2))
        return 0

    print("wiki inputs built")
    print(f"workspace: {result.workspace}")
    print(f"staging: {result.staging}")
    print(f"concepts: {result.concept_count}")
    print(f"projects: {result.project_count}")
    print(f"open questions: {result.open_question_count}")
    print(f"internal links: {result.internal_link_count}")
    print(f"backlink edges: {result.backlink_edge_count}")
    print(f"staged concepts: {result.staged_concept_count}")
    print(f"landing: {result.landing_path}")
    print(f"this-week: {result.this_week_path}")
    print(f"backlinks: {result.backlinks_path}")
    return 0


def cmd_wiki_plan_roles(args: argparse.Namespace) -> int:
    try:
        result = plan_wiki_roles(
            workspace=args.workspace,
            concept_dir=args.concept_dir,
            audience=args.audience,
        )
        written = write_wiki_route_plan_artifact(load_system(args.root, args.plugin), result) if args.write_artifact else None
    except WikiError as exc:
        print(f"wiki error: {exc}", file=sys.stderr)
        return 1

    payload = result.to_payload()
    if written:
        payload["written_artifact"] = written.to_payload()
    if args.json:
        print(json.dumps(payload, indent=2))
        return 0

    print("wiki role route plan")
    print(f"workspace: {result.workspace}")
    print(f"concept dir: {result.concept_dir}")
    print(f"audience: {result.audience}")
    print(f"planned hires: {result.planned_hire_count}")
    if written:
        print(f"artifact: {written.path}")
        print(f"artifact id: {written.artifact_id}")
    for key, count in result.route_counts.items():
        print(f"  {key}: {count}")
    return 0


def cmd_wiki_route_dry_run(args: argparse.Namespace) -> int:
    system = load_system(args.root, args.plugin)
    max_source_age_hours = int(
        args.max_source_age_hours
        if args.max_source_age_hours is not None
        else system.runtime_policy.get("max_importer_staleness_hours", 24)
    )
    sources = tuple(item.strip() for item in args.sources.split(",") if item.strip())
    try:
        plan = plan_wiki_roles(
            workspace=args.workspace,
            concept_dir=args.concept_dir,
            audience=args.audience,
        )
        if args.freshness_check == "skip":
            freshness = {}
            freshness_status = "skipped"
            freshness_reason = "freshness_check=skip"
        else:
            store = LakeStore(system.storage_dir)
            store.ensure()
            freshness = evaluate_wiki_route_source_freshness(
                store,
                required_sources=sources,
                max_age_hours=max_source_age_hours,
            )
            freshness_status = "passed" if freshness.get("passed") else "failed"
            freshness_reason = "checked source importer freshness"
        plan_artifact = write_wiki_route_plan_artifact(system, plan, freshness=freshness) if args.write_artifact else None
        result = preview_wiki_route_execution(plan, system=system, freshness=freshness)
        written = write_wiki_route_execution_artifact(system, result) if args.write_artifact else None
        invariant_report = build_runtime_invariant_report(
            run_id=(written.artifact_id if written else "wiki-route-dry-run"),
            mode="wiki_route_dry_run",
            status="planned",
            dry_run=True,
            preflight={
                "source_freshness": {
                    "status": freshness_status,
                    "mode": args.freshness_check,
                    "required": bool(args.require_fresh),
                    "reason": freshness_reason,
                    "freshness": freshness,
                }
            },
            steps=[
                {
                    "id": "wiki_route_dry_run",
                    "status": "passed",
                    "planned_hire_count": result.planned_hire_count,
                    "non_hire_count": result.non_hire_count,
                }
            ],
            route_preview=result.to_payload(),
            route_artifact=written.to_payload() if written else {},
            execute_render=False,
        )
        invariant_artifact = (
            write_runtime_invariant_report_artifact(
                system,
                invariant_report,
                run_id=written.artifact_id if written else "wiki-route-dry-run",
                checker="memory.wiki.route_executor",
            )
            if args.write_artifact
            else None
        )
    except WikiError as exc:
        print(f"wiki error: {exc}", file=sys.stderr)
        return 1

    payload = result.to_payload()
    payload["runtime_invariant_report"] = {
        "summary": invariant_report.get("summary", {}),
        **(invariant_artifact.to_payload() if invariant_artifact else {}),
    }
    if args.write_artifact and plan_artifact:
        payload["route_plan_artifact"] = plan_artifact.to_payload()
    if written:
        payload["written_artifact"] = written.to_payload()
    if args.json:
        print(json.dumps(payload, indent=2))
        return 2 if args.require_fresh and args.freshness_check != "skip" and not payload["freshness"].get("passed", True) else 0

    print("wiki route dry-run")
    print(f"workspace: {result.workspace}")
    print(f"concept dir: {result.concept_dir}")
    print(f"audience: {result.audience}")
    if result.freshness:
        status = "fresh" if result.freshness.get("passed") else "stale/missing"
        print(f"source freshness: {status} (max age {result.freshness.get('max_age_hours')}h)")
        for source, item in result.freshness.get("sources", {}).items():
            latest = item.get("latest_ts") or "-"
            print(f"  {source}: {item.get('status')} latest={latest} rows={item.get('session_or_event_rows', 0)}")
    print(f"planned hires: {result.planned_hire_count}")
    print(f"non-hire outcomes: {result.non_hire_count}")
    invariant_summary = invariant_report.get("summary", {})
    print(
        "runtime invariants: "
        f"{'passed' if invariant_summary.get('passed') else 'failed'} "
        f"(silent no-ops {invariant_summary.get('silent_noops', 0)})"
    )
    if written:
        if plan_artifact:
            print(f"route plan artifact: {plan_artifact.path}")
        print(f"artifact: {written.path}")
        print(f"artifact id: {written.artifact_id}")
        if invariant_artifact:
            print(f"invariant report: {invariant_artifact.path}")
    for hire in result.planned_hires:
        print(f"  hire {hire['job_key']}: {', '.join(hire['job_ids'])}")
    for outcome in result.non_hire_outcomes:
        print(f"  {outcome['outcome']} {outcome['job_key']}: {outcome['reason']}")
    return 2 if args.require_fresh and args.freshness_check != "skip" and result.freshness and not result.freshness.get("passed", True) else 0


def cmd_wiki_brackify(args: argparse.Namespace) -> int:
    target = args.file.resolve()
    concept_dir = args.concept_dir.resolve()
    if not target.is_file():
        print(f"wiki error: file not found: {target}", file=sys.stderr)
        return 1
    if not concept_dir.is_dir():
        print(f"wiki error: concept dir not found: {concept_dir}", file=sys.stderr)
        return 1
    concepts = collect_concepts(concept_dir)
    text = target.read_text(encoding="utf-8")
    rendered, count, seen = brackify_text(text, concepts, bracket_all=args.bracket_all)
    if args.write and rendered != text:
        target.write_text(rendered, encoding="utf-8")
    payload = {
        "file": str(target),
        "concept_dir": str(concept_dir),
        "concept_count": len(concepts),
        "brackets_added": count,
        "bracketed_terms": sorted(seen),
        "written": bool(args.write and rendered != text),
        "dry_run": not args.write,
    }
    if args.json:
        print(json.dumps(payload, indent=2))
        return 0
    print("wiki brackify")
    print(f"file: {target}")
    print(f"concepts: {len(concepts)}")
    print(f"brackets added: {count}")
    if seen:
        print(f"terms: {', '.join(sorted(seen))}")
    print("written: yes" if payload["written"] else "written: no")
    return 0


def cmd_job_run(args: argparse.Namespace) -> int:
    system = load_system(args.root, args.plugin)
    params = parse_key_values(args.job_param)
    if args.date:
        params["date"] = args.date
    if args.hour:
        params["hour"] = args.hour
    if args.audience:
        params["audience"] = args.audience
    workspace = args.workspace or Path("/tmp") / "onecontext-job-run"
    try:
        expected_ts = None
        if args.date and args.hour is not None:
            expected_ts = f"{args.date}T{int(args.hour):02d}:00:00Z"
        elif args.date and args.job_id in {"memory.daily.editor", "memory.concept.scout"}:
            expected_ts = f"{args.date}T23:59:00Z"
        validator = None
        if expected_ts:
            from .memory.talk import validate_talk_entry

            expected_kind = {
                "memory.hourly.scribe": "conversation",
                "memory.hourly.shard_scribe": "synthesis",
                "memory.hourly.aggregate_scribe": "conversation",
                "memory.daily.editor": "proposal",
                "memory.concept.scout": ("proposal", "question", "concern"),
            }.get(args.job_id)
            if expected_kind:
                validator = lambda path, kind=expected_kind: validate_talk_entry(
                    path,
                    expected_ts=expected_ts,
                    expected_kind=kind,
                )
        prepared = prepare_memory_job(
            system,
            job_id=args.job_id,
            params=params,
            workspace=workspace,
            run_harness=args.run_harness,
            model=args.model,
            run_id=args.run_id,
            completed_event=f"{args.job_id}.completed",
            validator=validator,
        )
        result = execute_hired_agent(system, prepared.execution_spec)
    except (MemoryJobError, HiredAgentRunnerError, ValueError, KeyError) as exc:
        print(f"job error: {exc}", file=sys.stderr)
        return 1

    payload = result.to_payload()
    payload["job_id"] = prepared.job_id
    payload["job_params"] = prepared.job_params
    payload["talk_folder"] = str(prepared.talk_folder) if prepared.talk_folder else None
    if args.json:
        print(json.dumps(payload, indent=2))
        return 0

    mode = "live harness" if not result.dry_run else "dry run"
    print(f"job run: {args.job_id} ({mode})")
    print(f"workspace: {result.workspace}")
    print(f"hired-agent uuid: {result.hire.get('hired_agent_uuid')}")
    print(f"prompt: {result.prompt_path}")
    print(f"prompt stack sha256: {result.prompt_stack.get('sha256') if result.prompt_stack else '-'}")
    print(f"output: {result.output_path}")
    print(f"validation: {'ok' if result.validation.get('ok') else 'not ok'}")
    if result.validation.get("failures"):
        for failure in result.validation["failures"]:
            print(f"  - {failure}")
    if result.returncode is not None:
        print(f"harness returncode: {result.returncode}")
        print(f"harness stdout: {result.stdout_path}")
        print(f"harness stderr: {result.stderr_path}")
    return 0


def cmd_job_run_day_hourlies(args: argparse.Namespace) -> int:
    system = load_system(args.root, args.plugin)
    sources = tuple(item.strip() for item in args.sources.split(",") if item.strip())
    try:
        result = run_day_hourly_scribes(
            system,
            date=args.date,
            audience=args.audience,
            workspace=args.workspace,
            run_harness=args.run_harness,
            model=args.model,
            max_concurrent=args.max_concurrent,
            limit_hours=args.limit_hours,
            skip_existing=not args.no_skip_existing,
            sources=sources,
        )
    except (DayHourliesError, MemoryJobError, HiredAgentRunnerError, ValueError, KeyError) as exc:
        print(f"job error: {exc}", file=sys.stderr)
        return 1

    payload = result.to_payload()
    if args.json:
        print(json.dumps(payload, indent=2))
        return 0

    mode = "live harness" if args.run_harness else "dry run"
    hours = ", ".join(hour.hour for hour in result.active_hours) or "-"
    print(f"day hourly fanout: {args.date} ({mode})")
    print(f"active hours: {hours}")
    print(f"prepared jobs: {len(result.prepared_jobs)}")
    print(f"skipped existing: {len(result.skipped_existing)}")
    print(f"max concurrent agents: {result.batch.max_concurrent}")
    print(f"duration: {result.batch.duration_ms}ms")
    print(f"batch: {'ok' if result.batch.ok else 'not ok'}")
    if result.batch.errors:
        for error in result.batch.errors:
            print(f"  - #{error['index']} {error['error_type']}: {error['message']}")
    return 0


def cmd_job_run_month_hourlies(args: argparse.Namespace) -> int:
    system = load_system(args.root, args.plugin)
    sources = tuple(item.strip() for item in args.sources.split(",") if item.strip())
    if args.plan_only:
        started = time.perf_counter()
        try:
            active_days = discover_month_active_hours(system, month=args.month, sources=sources)
        except (DayHourliesError, ValueError) as exc:
            print(f"job error: {exc}", file=sys.stderr)
            return 1
        payload = {
            "month": args.month,
            "active_day_count": len(active_days),
            "active_hour_count": sum(day.hour_count for day in active_days),
            "event_count": sum(day.event_count for day in active_days),
            "max_concurrent_agents": args.max_concurrent or system.runtime_policy["max_concurrent_agents"],
            "hour_event_buckets": hour_event_buckets(active_days),
            "active_days": [day.to_payload() for day in active_days],
            "plan_only": True,
            "duration_ms": int((time.perf_counter() - started) * 1000),
        }
        if args.json:
            print(json.dumps(payload, indent=2))
            return 0
        print(f"month hourly plan: {args.month}")
        print(f"active days: {payload['active_day_count']}")
        print(f"active hours: {payload['active_hour_count']}")
        print(f"events: {payload['event_count']}")
        print(f"hour event buckets: {payload['hour_event_buckets']}")
        print(f"max concurrent agents: {payload['max_concurrent_agents']}")
        print(f"duration: {payload['duration_ms']}ms")
        return 0
    try:
        result = run_month_hourly_scribes(
            system,
            month=args.month,
            audience=args.audience,
            workspace=args.workspace,
            run_harness=args.run_harness,
            model=args.model,
            max_concurrent=args.max_concurrent,
            limit_hours=args.limit_hours,
            skip_existing=not args.no_skip_existing,
            sources=sources,
        )
    except (DayHourliesError, MemoryJobError, HiredAgentRunnerError, ValueError, KeyError) as exc:
        print(f"job error: {exc}", file=sys.stderr)
        return 1

    payload = result.to_payload()
    if args.json:
        print(json.dumps(payload, indent=2))
        return 0

    mode = "live harness" if args.run_harness else "dry run"
    print(f"month hourly fanout: {args.month} ({mode})")
    print(f"active days: {len(result.active_days)}")
    print(f"active hours: {result.active_hour_count}")
    print(f"events: {result.event_count}")
    print(f"prepared jobs: {len(result.prepared_jobs)}")
    print(f"skipped existing: {len(result.skipped_existing)}")
    print(f"max concurrent agents: {result.batch.max_concurrent}")
    print(f"duration: {result.batch.duration_ms}ms")
    print(f"validation failures: {result.batch.to_payload()['validation_failure_count']}")
    print(f"batch: {'ok' if result.batch.ok else 'not ok'}")
    if result.batch.errors:
        for error in result.batch.errors:
            print(f"  - #{error['index']} {error['error_type']}: {error['message']}")
    return 0


def cmd_job_run_month_hourly_blocks(args: argparse.Namespace) -> int:
    system = load_system(args.root, args.plugin)
    sources = tuple(item.strip() for item in args.sources.split(",") if item.strip())
    if args.plan_only:
        started = time.perf_counter()
        try:
            active_days = discover_month_active_hours(system, month=args.month, sources=sources)
        except (DayHourliesError, ValueError) as exc:
            print(f"job error: {exc}", file=sys.stderr)
            return 1
        active_blocks = fixed_four_hour_blocks(active_days)
        payload = {
            "month": args.month,
            "active_day_count": len(active_days),
            "active_hour_count": sum(day.hour_count for day in active_days),
            "active_block_count": len(active_blocks),
            "event_count": sum(day.event_count for day in active_days),
            "max_concurrent_agents": args.max_concurrent or system.runtime_policy["max_concurrent_agents"],
            "hour_event_buckets": hour_event_buckets(active_days),
            "active_blocks": [block.to_payload() for block in active_blocks],
            "plan_only": True,
            "duration_ms": int((time.perf_counter() - started) * 1000),
        }
        if args.json:
            print(json.dumps(payload, indent=2))
            return 0
        print(f"month hourly block plan: {args.month}")
        print(f"active days: {payload['active_day_count']}")
        print(f"active hours: {payload['active_hour_count']}")
        print(f"active 4-hour blocks: {payload['active_block_count']}")
        print(f"events: {payload['event_count']}")
        print(f"hour event buckets: {payload['hour_event_buckets']}")
        print(f"max concurrent agents: {payload['max_concurrent_agents']}")
        print(f"duration: {payload['duration_ms']}ms")
        return 0
    try:
        result = run_month_hourly_block_scribes(
            system,
            month=args.month,
            audience=args.audience,
            workspace=args.workspace,
            run_harness=args.run_harness,
            model=args.model,
            max_concurrent=args.max_concurrent,
            limit_blocks=args.limit_blocks,
            skip_existing=not args.no_skip_existing,
            experience_mode=args.experience_mode,
            split_large_blocks=args.split_large_blocks,
            max_prompt_tokens=args.max_prompt_tokens,
            max_prompt_bytes=args.max_prompt_bytes,
            sources=sources,
        )
    except (DayHourliesError, MemoryJobError, HiredAgentRunnerError, ValueError, KeyError) as exc:
        print(f"job error: {exc}", file=sys.stderr)
        return 1

    payload = result.to_payload()
    if args.json:
        print(json.dumps(payload, indent=2))
        return 0

    mode = "live harness" if args.run_harness else "dry run"
    print(f"month hourly block fanout: {args.month} ({mode})")
    print(f"active days: {len(result.active_days)}")
    print(f"active hours: {result.active_hour_count}")
    print(f"prepared hours: {result.prepared_hour_count}")
    print(f"events: {result.event_count}")
    print(f"prepared hired-agent jobs: {len(result.prepared_jobs)}")
    print(f"skipped existing hourly entries: {len(result.skipped_existing)}")
    print(f"large prompts: {result.large_prompt_count} over {result.prompt_warning_tokens} estimated tokens")
    if result.oversized_single_hour_count:
        print(f"oversized single-hour prompts: {result.oversized_single_hour_count}")
    if result.split_large_blocks:
        print(f"split large blocks: {len(result.split_large_blocks)}")
    if result.sharded_hours:
        print(f"sharded hours: {len(result.sharded_hours)}")
    print(f"max concurrent agents: {result.batch.max_concurrent}")
    print(f"duration: {result.batch.duration_ms}ms")
    print(f"validation failures: {result.batch.to_payload()['validation_failure_count']}")
    print(f"batch: {'ok' if result.batch.ok else 'not ok'}")
    if result.batch.errors:
        for error in result.batch.errors:
            print(f"  - #{error['index']} {error['error_type']}: {error['message']}")
    return 0


def cmd_job_plan_month_routes(args: argparse.Namespace) -> int:
    system = load_system(args.root, args.plugin)
    sources = tuple(item.strip() for item in args.sources.split(",") if item.strip())
    try:
        plan = plan_month_hourly_routes(
            system,
            month=args.month,
            audience=args.audience,
            workspace=args.workspace,
            limit_blocks=args.limit_blocks,
            skip_existing=not args.no_skip_existing,
            split_large_blocks=args.split_large_blocks,
            max_prompt_tokens=args.max_prompt_tokens,
            experience_mode=args.experience_mode,
            sources=sources,
        )
    except (DayHourliesError, ValueError) as exc:
        print(f"job error: {exc}", file=sys.stderr)
        return 1

    payload = plan.to_payload()
    if args.json:
        print(json.dumps(payload, indent=2))
        return 0

    print(f"month route plan: {args.month}")
    print(f"active days: {payload['active_day_count']}")
    print(f"active hours: {payload['active_hour_count']}")
    print(f"prepared hours: {payload['prepared_hour_count']}")
    print(f"events: {payload['event_count']}")
    print(f"planned hired-agent jobs: {payload['planned_hire_count']}")
    print(f"route counts: {payload['route_counts']}")
    print(f"split 4-hour blocks: {payload['split_large_block_count']}")
    print(f"sharded hours: {payload['sharded_hour_count']}")
    print(f"artifact: {payload['artifact_path']}")
    print(f"cache hit: {payload['cache']['hit']}")
    print(f"duration: {payload['duration_ms']}ms")
    return 0


def cmd_job_run_month_hourly_retries(args: argparse.Namespace) -> int:
    system = load_system(args.root, args.plugin)
    sources = tuple(item.strip() for item in args.sources.split(",") if item.strip())
    try:
        result = run_month_hourly_retries(
            system,
            month=args.month,
            audience=args.audience,
            workspace=args.workspace,
            run_harness=args.run_harness,
            model=args.model,
            max_concurrent=args.max_concurrent,
            limit_hours=args.limit_hours,
            skip_existing=not args.no_skip_existing,
            sources=sources,
        )
    except (DayHourliesError, MemoryJobError, HiredAgentRunnerError, ValueError, KeyError) as exc:
        print(f"job error: {exc}", file=sys.stderr)
        return 1

    payload = result.to_payload()
    if args.json:
        print(json.dumps(payload, indent=2))
        return 0

    mode = "live harness" if args.run_harness else "dry run"
    print(f"month hourly retries: {args.month} ({mode})")
    print(f"retry hours discovered: {len(result.retry_hours)}")
    print(f"prepared retry jobs: {len(result.prepared_jobs)}")
    print(f"skipped existing hourly entries: {len(result.skipped_existing)}")
    print(f"max concurrent agents: {result.batch.max_concurrent}")
    print(f"duration: {result.batch.duration_ms}ms")
    print(f"validation failures: {result.batch.to_payload()['validation_failure_count']}")
    print(f"batch: {'ok' if result.batch.ok else 'not ok'}")
    if result.retry_hours:
        for retry_hour in result.retry_hours[:20]:
            print(f"  - {retry_hour.date}T{retry_hour.hour}:00Z {retry_hour.reason}")
    if result.batch.errors:
        for error in result.batch.errors:
            print(f"  - #{error['index']} {error['error_type']}: {error['message']}")
    return 0


def cmd_job_run_for_you_month(args: argparse.Namespace) -> int:
    system = load_system(args.root, args.plugin)
    sources = tuple(item.strip() for item in args.sources.split(",") if item.strip())
    try:
        result = run_for_you_month(
            system,
            month=args.month,
            audience=args.audience,
            workspace=args.workspace,
            run_harness=args.run_harness,
            model=args.model,
            max_concurrent=args.max_concurrent,
            limit_blocks=args.limit_blocks,
            limit_days=args.limit_days,
            skip_existing=not args.no_skip_existing,
            run_day_layer=not args.no_day_layer,
            split_large_blocks=args.split_large_blocks,
            max_prompt_tokens=args.max_prompt_tokens,
            max_prompt_bytes=args.max_prompt_bytes,
            sources=sources,
        )
    except (
        DayHourliesError,
        ForYouRunnerError,
        MemoryJobError,
        HiredAgentRunnerError,
        ValueError,
        KeyError,
    ) as exc:
        print(f"job error: {exc}", file=sys.stderr)
        return 1

    payload = result.to_payload()
    if args.json:
        print(json.dumps(payload, indent=2))
        return 0

    mode = "live harness" if args.run_harness else "dry run"
    print(f"for_you_day month run: {args.month} ({mode})")
    print(f"state machine: {result.state_machine.get('id')} v{result.state_machine.get('version')}")
    print(f"workspace: {result.workspace}")
    print(f"block jobs: {len(result.blocks.prepared_jobs)}")
    print(f"large block prompts: {result.blocks.large_prompt_count} over {result.blocks.prompt_warning_tokens} estimated tokens")
    if result.blocks.oversized_single_hour_count:
        print(f"oversized single-hour prompts: {result.blocks.oversized_single_hour_count}")
    if result.blocks.split_large_blocks:
        print(f"split large blocks: {len(result.blocks.split_large_blocks)}")
    if result.blocks.sharded_hours:
        print(f"sharded hours: {len(result.blocks.sharded_hours)}")
    print(f"retry jobs: {len(result.retries.prepared_jobs)}")
    print(f"day reviews: {len(result.day_reviews)}")
    print(f"duration: {result.duration_ms}ms")
    print(f"blocks batch: {'ok' if result.blocks.batch.ok else 'not ok'}")
    print(f"retries batch: {'ok' if result.retries.batch.ok else 'not ok'}")
    if result.day_reviews:
        failures = sum(review.batch.to_payload()["validation_failure_count"] for review in result.day_reviews)
        print(f"day review validation failures: {failures}")
    return 0


def cmd_job_render_talk_folder(args: argparse.Namespace) -> int:
    try:
        result = render_talk_folder(args.talk_folder, output_path=args.output)
    except OSError as exc:
        print(f"job error: {exc}", file=sys.stderr)
        return 1
    if args.json:
        print(json.dumps(result, indent=2))
        return 0
    print("rendered talk folder")
    print(f"talk folder: {result['talk_folder']}")
    print(f"output: {result['output_path']}")
    print(f"entries: {result['entry_count']}")
    print(f"sha256: {result['sha256']}")
    return 0


def cmd_lab_hourly_scribe(args: argparse.Namespace) -> int:
    system = load_system(args.root, args.plugin)
    try:
        result = run_hourly_scribe_lab(
            system,
            date=args.date,
            hour=args.hour,
            audience=args.audience,
            workspace=args.workspace,
            run_claude=args.run_claude,
            model=args.model,
        )
    except (HourlyScribeLabError, ValueError) as exc:
        print(f"lab error: {exc}", file=sys.stderr)
        return 1

    payload = result.to_payload()
    if args.json:
        print(json.dumps(payload, indent=2))
        return 0

    mode = "live claude" if not result.dry_run else "dry run"
    print(f"hourly scribe lab: {mode}")
    print(f"workspace: {result.workspace}")
    print(f"experience: {result.experience_packet.get('path')}")
    print(f"experience sha256: {result.experience_packet.get('experience_sha256')}")
    print(f"agent context: {result.experience_packet.get('agent_context_path')}")
    print(f"agent context sha256: {result.experience_packet.get('agent_context_sha256')}")
    print(f"agent context bytes: {result.experience_packet.get('agent_context_bytes')}")
    print(f"harness isolation: {result.hire.get('event', {}).get('job_params', {}).get('harness_isolation')}")
    print(f"max concurrent agents: {result.hire.get('event', {}).get('job_params', {}).get('max_concurrent_agents')}")
    print(f"hired-agent uuid: {result.hire.get('hired_agent_uuid')}")
    print(f"prompt: {result.prompt_path}")
    print(f"output: {result.output_path}")
    print(f"validation: {'ok' if result.validation.get('ok') else 'not ok'}")
    if result.validation.get("failures"):
        for failure in result.validation["failures"]:
            print(f"  - {failure}")
    if result.claude_returncode is not None:
        print(f"claude returncode: {result.claude_returncode}")
        print(f"claude stdout: {result.claude_stdout_path}")
        print(f"claude stderr: {result.claude_stderr_path}")
    return 0

if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
