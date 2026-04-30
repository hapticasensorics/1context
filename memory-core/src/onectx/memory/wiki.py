from __future__ import annotations

import json
import re
import shutil
import hashlib
from dataclasses import dataclass
from datetime import date, datetime, timedelta, timezone
from pathlib import Path
from typing import Any

from onectx.config import MemorySystem
from onectx.memory.prompt_stack import PromptPart, PromptStack, estimate_token_count, prompt_part_from_file, text_sha256
from onectx.storage import LakeStore, stable_id, utc_now
from onectx.storage.hour_events import HourEventError, normalize_source, parse_ts


FRONTMATTER_RE = re.compile(r"^---\n(.*?)\n---\n", re.DOTALL)
H2_RE = re.compile(r"^##\s+(.+)$", re.MULTILINE)
BRACKET_RE = re.compile(r"\[\[([^\[\]|]+?)(?:\|([^\[\]]*?))?\]\]")
FENCED_RE = re.compile(r"(```.*?```)", re.DOTALL)
BACKTICK_RE = re.compile(r"(`[^`\n]+?`)")
HTML_COMMENT_RE = re.compile(r"<!--.*?-->", re.DOTALL)
MD_LINK_RE = re.compile(r"\[[^\]]*\]\([^)]*\)")
EXISTING_BRACKETS_RE = re.compile(r"\[\[[^\]]+\]\]")
WIKI_SOURCE_PACKET_SHARD_JOB = "memory.wiki.source_packet_shard"
WIKI_SOURCE_PACKET_AGGREGATE_JOB = "memory.wiki.source_packet_aggregate"
DEFAULT_SOURCE_SHARD_TOKENS = 64000

TOPIC_CATEGORIES = [
    ("Engineering", "code-level, language-level, and framework-level concepts"),
    ("Infrastructure", "runtime, deployment, storage, and observability concepts"),
    ("Process", "methodology, collaboration, and project-management concepts"),
    ("Tools", "specific products and services the operator uses"),
    ("Domain", "domain-specific concepts particular to the operator's work"),
    ("Coworkers", "concept pages about specific people"),
    ("Organizations", "concept pages about companies, teams, or sub-organizations"),
]
DEFAULT_TOPIC_CATEGORY = "Tools"

PROJECT_STATUS_SECTIONS = [
    ("Active projects", "active", "projects currently in flight"),
    ("Paused or blocked", "paused", "projects with an explicit pause or block"),
    ("Recently completed", "completed", "projects wrapped within roughly the last quarter"),
    ("Archived", "archived", "older projects kept for reference"),
]

EXTERNAL_FALLBACKS = {
    "anthropic": "https://www.anthropic.com/",
    "caddy": "https://caddyserver.com/",
    "claude-mem": "https://github.com/thedotmack/claude-mem",
    "cloud-sql": "https://cloud.google.com/sql/docs",
    "cloudflare": "https://www.cloudflare.com/",
    "cloudflare-workers": "https://developers.cloudflare.com/workers/",
    "codex": "https://github.com/openai/codex",
    "fts5": "https://www.sqlite.org/fts5.html",
    "gemini": "https://ai.google.dev/",
    "github": "https://github.com/",
    "hydra": "https://github.com/ory/hydra",
    "lance": "https://lancedb.com/",
    "lancedb": "https://lancedb.com/",
    "llms-full-txt": "https://llmstxt.org/",
    "llms-txt": "https://llmstxt.org/",
    "postgres": "https://www.postgresql.org/",
    "postgresql": "https://www.postgresql.org/",
    "screencapturekit": "https://developer.apple.com/documentation/screencapturekit",
    "sqlite": "https://www.sqlite.org/",
    "sqlx": "https://github.com/launchbadge/sqlx",
    "tailscale": "https://tailscale.com/",
    "vertex": "https://cloud.google.com/vertex-ai",
    "vertex-ai": "https://cloud.google.com/vertex-ai",
    "wiki-js": "https://wiki.js.org/",
    "wikijs": "https://wiki.js.org/",
}


class WikiError(RuntimeError):
    pass


@dataclass(frozen=True)
class ConceptPage:
    slug: str
    display: str
    path: Path
    frontmatter: dict[str, Any]
    lede: str


@dataclass(frozen=True)
class WikiBuildResult:
    workspace: Path
    concept_dir: Path
    staging: Path
    web_base: str
    topics_path: Path
    projects_path: Path
    open_questions_path: Path
    this_week_path: Path
    landing_path: Path
    backlinks_path: Path
    concept_staging_dir: Path
    concept_count: int
    project_count: int
    open_question_count: int
    backlink_edge_count: int
    internal_link_count: int
    staged_concept_count: int

    def to_payload(self) -> dict[str, Any]:
        return {
            "workspace": str(self.workspace),
            "concept_dir": str(self.concept_dir),
            "staging": str(self.staging),
            "web_base": self.web_base,
            "topics_path": str(self.topics_path),
            "projects_path": str(self.projects_path),
            "open_questions_path": str(self.open_questions_path),
            "this_week_path": str(self.this_week_path),
            "landing_path": str(self.landing_path),
            "backlinks_path": str(self.backlinks_path),
            "concept_staging_dir": str(self.concept_staging_dir),
            "concept_count": self.concept_count,
            "project_count": self.project_count,
            "open_question_count": self.open_question_count,
            "backlink_edge_count": self.backlink_edge_count,
            "internal_link_count": self.internal_link_count,
            "staged_concept_count": self.staged_concept_count,
        }


ISO_DATE_RE = re.compile(r"^\d{4}-\d{2}-\d{2}$")


@dataclass(frozen=True)
class WikiRoutePlan:
    workspace: Path
    concept_dir: Path
    audience: str
    inventory: dict[str, Any]
    route_plan: dict[str, list[dict[str, Any]]]

    @property
    def route_counts(self) -> dict[str, int]:
        return {key: len(value) for key, value in self.route_plan.items()}

    @property
    def planned_hire_count(self) -> int:
        return sum(
            1
            for rows in self.route_plan.values()
            for row in rows
            if str(row.get("outcome", "hire")) == "hire"
        )

    def to_payload(self) -> dict[str, Any]:
        return {
            "workspace": str(self.workspace),
            "concept_dir": str(self.concept_dir),
            "audience": self.audience,
            "inventory": self.inventory,
            "route_plan": self.route_plan,
            "route_counts": self.route_counts,
            "planned_hire_count": self.planned_hire_count,
        }


@dataclass(frozen=True)
class WikiRouteExecutionPreview:
    workspace: Path
    concept_dir: Path
    audience: str
    planned_hires: tuple[dict[str, Any], ...]
    non_hire_outcomes: tuple[dict[str, Any], ...]
    freshness: dict[str, Any] | None = None
    route_execution_id: str = ""
    artifact_path: Path | None = None

    @property
    def planned_hire_count(self) -> int:
        return len(self.planned_hires)

    @property
    def non_hire_count(self) -> int:
        return len(self.non_hire_outcomes)

    def to_payload(self) -> dict[str, Any]:
        return {
            "workspace": str(self.workspace),
            "concept_dir": str(self.concept_dir),
            "audience": self.audience,
            "route_execution_id": self.route_execution_id,
            "artifact_path": str(self.artifact_path) if self.artifact_path else "",
            "freshness": self.freshness or {},
            "planned_hire_count": self.planned_hire_count,
            "non_hire_count": self.non_hire_count,
            "planned_hires": list(self.planned_hires),
            "non_hire_outcomes": list(self.non_hire_outcomes),
        }


@dataclass(frozen=True)
class WikiRouteExecutionArtifact:
    artifact_id: str
    path: Path
    content_hash: str
    bytes: int

    def to_payload(self) -> dict[str, Any]:
        return {
            "artifact_id": self.artifact_id,
            "path": str(self.path),
            "content_hash": self.content_hash,
            "bytes": self.bytes,
        }


@dataclass(frozen=True)
class WikiRoutePlanArtifact:
    artifact_id: str
    path: Path
    content_hash: str
    bytes: int
    route_plan_id: str

    def to_payload(self) -> dict[str, Any]:
        return {
            "artifact_id": self.artifact_id,
            "path": str(self.path),
            "content_hash": self.content_hash,
            "bytes": self.bytes,
            "route_plan_id": self.route_plan_id,
        }


def build_wiki_inputs(
    *,
    workspace: Path,
    concept_dir: Path,
    staging: Path,
    web_base: str = "/paul-demo2",
    today: date | None = None,
) -> WikiBuildResult:
    """Build the deterministic wiki input surface from markdown sources.

    The durable source tree keeps authored and agent-written prose. The staging
    tree carries render-only transformations: resolved brackets, backlinks, and
    concept pages wrapped with renderer frontmatter.
    """
    today = today or date.today()
    workspace = workspace.resolve()
    concept_dir = concept_dir.resolve()
    staging = staging.resolve()
    if not workspace.is_dir():
        raise WikiError(f"workspace not found: {workspace}")
    if not concept_dir.is_dir():
        raise WikiError(f"concept dir not found: {concept_dir}")
    ensure_non_overlapping_tree("staging", staging, "workspace", workspace)
    ensure_non_overlapping_tree("staging", staging, "concept_dir", concept_dir)

    concepts = collect_concepts(concept_dir)
    topics_path = workspace / "topics.md"
    projects_path = workspace / "projects.md"
    open_questions_path = workspace / "open-questions.md"
    this_week_path = workspace / "this-week.md"
    landing_path = workspace / "index.md"

    topics_path.write_text(render_topics_index(concepts, web_base=web_base, today=today), encoding="utf-8")
    projects = collect_projects(concepts)
    projects_path.write_text(render_projects_index(projects, web_base=web_base, today=today), encoding="utf-8")
    open_questions = collect_open_questions(workspace, concept_dir, web_base=web_base)
    open_questions_path.write_text(
        render_open_questions_page(open_questions, web_base=web_base, today=today),
        encoding="utf-8",
    )

    resolve_result = resolve_bracket_tree(
        src=workspace,
        dst=staging,
        concept_dir=concept_dir,
        web_base=web_base,
    )
    backlinks = resolve_result["backlinks"]

    this_week_path.write_text(
        render_this_week_digest(
            workspace=workspace,
            concept_dir=concept_dir,
            backlinks=backlinks,
            web_base=web_base,
            today=today,
        ),
        encoding="utf-8",
    )
    landing_path.write_text(
        render_landing_page(
            workspace=workspace,
            concept_dir=concept_dir,
            backlinks=backlinks,
            open_question_count=len(open_questions),
            web_base=web_base,
            today=today,
        ),
        encoding="utf-8",
    )

    # Re-resolve so generated landing and this-week pages are present in staging.
    resolve_result = resolve_bracket_tree(
        src=workspace,
        dst=staging,
        concept_dir=concept_dir,
        web_base=web_base,
    )
    backlinks = resolve_result["backlinks"]
    concept_staging_dir, staged_concepts = stage_concept_pages(
        concept_dir=concept_dir,
        staging=staging,
        backlinks=backlinks,
        web_base=web_base,
    )

    return WikiBuildResult(
        workspace=workspace,
        concept_dir=concept_dir,
        staging=staging,
        web_base=web_base,
        topics_path=topics_path,
        projects_path=projects_path,
        open_questions_path=open_questions_path,
        this_week_path=this_week_path,
        landing_path=landing_path,
        backlinks_path=staging / "_backlinks.json",
        concept_staging_dir=concept_staging_dir,
        concept_count=len(concepts),
        project_count=len(projects),
        open_question_count=len(open_questions),
        backlink_edge_count=sum(len(v) for v in backlinks.values()),
        internal_link_count=int(resolve_result["internal_link_count"]),
        staged_concept_count=staged_concepts,
    )


def plan_wiki_roles(
    *,
    workspace: Path,
    concept_dir: Path,
    audience: str = "private",
    target_redaction_tiers: tuple[str, ...] = ("internal", "public"),
) -> WikiRoutePlan:
    workspace = workspace.resolve()
    concept_dir = concept_dir.resolve()
    if not workspace.is_dir():
        raise WikiError(f"workspace not found: {workspace}")
    if not concept_dir.is_dir():
        raise WikiError(f"concept dir not found: {concept_dir}")
    inventory = scan_wiki_inventory(workspace=workspace, concept_dir=concept_dir, audience=audience)
    route_plan = derive_wiki_role_route_plan(
        inventory,
        workspace=workspace,
        concept_dir=concept_dir,
        audience=audience,
        target_redaction_tiers=target_redaction_tiers,
    )
    return WikiRoutePlan(
        workspace=workspace,
        concept_dir=concept_dir,
        audience=audience,
        inventory=inventory,
        route_plan=route_plan,
    )


def preview_wiki_route_execution(
    plan: WikiRoutePlan,
    *,
    system: MemorySystem | None = None,
    freshness: dict[str, Any] | None = None,
    route_execution_id: str = "",
    artifact_path: Path | None = None,
) -> WikiRouteExecutionPreview:
    """Build a dry-run execution view from a role route plan.

    This is intentionally not a live runner. It gives the state-machine layer a
    proof-shaped bridge from route rows to planned hires while the mutating role
    validators and harness adapters mature.
    """
    planned: list[dict[str, Any]] = []
    non_hires: list[dict[str, Any]] = []
    for group, rows in plan.route_plan.items():
        for row in rows:
            outcome = str(row.get("outcome", "hire"))
            if outcome != "hire":
                non_hires.append(
                    {
                        "route_group": group,
                        "job_key": row.get("job_key", ""),
                        "job": row.get("job", ""),
                        "outcome": outcome,
                        "reason": row.get("reason", ""),
                    }
                )
                continue
            prompt_stack_preview = (
                wiki_route_prompt_stack_preview(system, row)
                if system is not None
                else {
                    "parts": prompt_stack_parts_for_route(row),
                    "source_packet": row.get("source_packet", {}),
                    "task_contract": row.get("task_contract", ""),
                }
            )
            planned.append(
                {
                    "route_group": group,
                    "route_id": row.get("route_id", ""),
                    "job_key": row.get("job_key", ""),
                    "job_ids": [row.get("job", "")],
                    "job_params": route_job_params(row),
                    "birth_certificate_preview": {
                        "kind": "hired_agent_birth_certificate_preview",
                        "job_ids": [row.get("job", "")],
                        "job_key": row.get("job_key", ""),
                        "experience_packet": row.get("source_packet", {}),
                        "ownership": row.get("ownership", {}),
                        "validators": row.get("validators", []),
                        "expected_outputs": row.get("expected_outputs", []),
                    },
                    "prompt_stack_preview": prompt_stack_preview,
                    "budget": row.get("budget", {}),
                    "concurrency_group": row.get("concurrency_group", ""),
                }
            )
    return WikiRouteExecutionPreview(
        workspace=plan.workspace,
        concept_dir=plan.concept_dir,
        audience=plan.audience,
        planned_hires=tuple(planned),
        non_hire_outcomes=tuple(non_hires),
        freshness=freshness,
        route_execution_id=route_execution_id,
        artifact_path=artifact_path,
    )


def evaluate_wiki_route_source_freshness(
    store: LakeStore,
    *,
    required_sources: tuple[str, ...] = ("codex", "claude-code"),
    max_age_hours: int = 24,
    now: datetime | None = None,
) -> dict[str, Any]:
    """Summarize whether session-derived source rows are fresh enough.

    v0 treats the lakestore `sessions` table as the importer cursor surface. If
    sessions have not been materialized yet, it falls back to the `events` table.
    This keeps freshness visible without requiring a new table migration before
    the route executor exists.
    """
    now_dt = now or datetime.now(timezone.utc)
    required = tuple(normalize_source(source) for source in required_sources if str(source).strip())
    rows_by_source = freshness_rows_by_source(store)
    sources: dict[str, Any] = {}
    passed = True
    for source in required:
        rows = rows_by_source.get(source, [])
        latest_ts = latest_source_timestamp(rows)
        event_count = sum(int(row.get("event_count") or 0) for row in rows)
        if latest_ts is None:
            status = "missing"
            age_seconds = None
            passed = False
        else:
            age_seconds = max(0, int((now_dt - latest_ts).total_seconds()))
            status = "fresh" if age_seconds <= max_age_hours * 3600 else "stale"
            if status != "fresh":
                passed = False
        sources[source] = {
            "status": status,
            "latest_ts": format_freshness_ts(latest_ts),
            "age_seconds": age_seconds,
            "max_age_hours": max_age_hours,
            "session_or_event_rows": len(rows),
            "event_count": event_count,
        }
    return {
        "kind": "source_import_freshness",
        "checked_at": format_freshness_ts(now_dt),
        "required_sources": list(required),
        "max_age_hours": max_age_hours,
        "passed": passed,
        "sources": sources,
    }


def freshness_rows_by_source(store: LakeStore) -> dict[str, list[dict[str, Any]]]:
    by_source: dict[str, list[dict[str, Any]]] = {}
    sessions = store.rows("sessions", limit=0)
    if sessions:
        for row in sessions:
            source = normalize_source(str(row.get("source") or ""))
            if source:
                by_source.setdefault(source, []).append(row)
        return by_source

    # Fallback for stores that have imported events but not session summaries.
    for row in store.rows("events", limit=0):
        source = normalize_source(str(row.get("source") or ""))
        if not source:
            continue
        event_row = {
            "source": source,
            "last_ts": row.get("ts", ""),
            "event_count": 1,
        }
        by_source.setdefault(source, []).append(event_row)
    return by_source


def latest_source_timestamp(rows: list[dict[str, Any]]) -> datetime | None:
    latest: datetime | None = None
    for row in rows:
        candidate = str(row.get("last_ts") or row.get("ts") or "")
        if not candidate:
            continue
        try:
            parsed = parse_ts(candidate)
        except HourEventError:
            continue
        if latest is None or parsed > latest:
            latest = parsed
    return latest


def format_freshness_ts(value: datetime | None) -> str:
    if value is None:
        return ""
    return value.astimezone(timezone.utc).isoformat(timespec="seconds").replace("+00:00", "Z")


def write_wiki_route_plan_artifact(
    system: MemorySystem,
    plan: WikiRoutePlan,
    *,
    freshness: dict[str, Any] | None = None,
    route_plan_id: str = "",
    artifact_path: Path | None = None,
) -> WikiRoutePlanArtifact:
    """Persist the scanner/planner output before execution preview."""
    base_payload = plan.to_payload()
    freshness_payload = freshness or {}
    resolved_route_plan_id = route_plan_id or stable_id(
        "wiki-role-route-plan",
        plan.workspace,
        plan.concept_dir,
        plan.audience,
        json.dumps(base_payload.get("route_plan", {}), sort_keys=True, default=str),
        json.dumps(freshness_payload, sort_keys=True, default=str),
    )
    resolved_path = artifact_path or (
        system.runtime_dir / "wiki" / "route-plans" / f"{resolved_route_plan_id}.json"
    )
    payload = {
        **base_payload,
        "kind": "wiki_role_route_plan",
        "route_plan_id": resolved_route_plan_id,
        "artifact_path": str(resolved_path),
        "freshness": freshness_payload,
        "written_at": utc_now(),
    }
    text = json.dumps(payload, indent=2, sort_keys=True, default=str) + "\n"
    resolved_path.parent.mkdir(parents=True, exist_ok=True)
    resolved_path.write_text(text, encoding="utf-8")
    content_hash = hashlib.sha256(text.encode("utf-8")).hexdigest()
    artifact_id = stable_id("artifact", "wiki_role_route_plan", resolved_route_plan_id, content_hash)
    store = LakeStore(system.storage_dir)
    store.ensure()
    artifact_row = store.artifact_row(
        "wiki_role_route_plan",
        artifact_id=artifact_id,
        uri=f"file://{resolved_path}",
        path=str(resolved_path),
        content_type="application/json",
        content_hash=content_hash,
        bytes=len(text.encode("utf-8")),
        source="memory.wiki.route_planner",
        state="ready",
        text=f"wiki role route plan {resolved_route_plan_id}",
        metadata={
            "route_plan_id": resolved_route_plan_id,
            "workspace": str(plan.workspace),
            "concept_dir": str(plan.concept_dir),
            "audience": plan.audience,
            "planned_hire_count": plan.planned_hire_count,
            "route_counts": plan.route_counts,
            "freshness_passed": bool(freshness_payload.get("passed", True)),
        },
    )
    store.replace_rows("artifacts", "artifact_id", [artifact_row])
    store.append_evidence(
        "wiki_route_plan.ready",
        artifact_id=artifact_id,
        status="passed",
        checker="memory.wiki.route_planner",
        text="wiki role route plan artifact written",
        checks=[
            "artifact_path.exists",
            "route rows include ownership",
            "route rows include budgets",
            "route rows include source packet receipts",
        ],
        payload={
            "route_plan_id": resolved_route_plan_id,
            "path": str(resolved_path),
            "route_counts": plan.route_counts,
            "planned_hire_count": plan.planned_hire_count,
        },
    )
    store.append_event(
        "wiki.route_plan.ready",
        source="memory.wiki.route_planner",
        actor="plan-roles",
        subject=resolved_route_plan_id,
        artifact_id=artifact_id,
        text=f"Prepared wiki role route plan with {plan.planned_hire_count} planned hires.",
        payload={
            "route_plan_id": resolved_route_plan_id,
            "path": str(resolved_path),
            "planned_hire_count": plan.planned_hire_count,
            "route_counts": plan.route_counts,
        },
    )
    return WikiRoutePlanArtifact(
        artifact_id=artifact_id,
        path=resolved_path,
        content_hash=content_hash,
        bytes=len(text.encode("utf-8")),
        route_plan_id=resolved_route_plan_id,
    )


def write_wiki_route_execution_artifact(
    system: MemorySystem,
    preview: WikiRouteExecutionPreview,
) -> WikiRouteExecutionArtifact:
    """Persist the dry-run route execution as a first-class runtime artifact."""
    base_payload = preview.to_payload()
    route_execution_id = preview.route_execution_id or stable_id(
        "wiki-route-execution",
        preview.workspace,
        preview.concept_dir,
        preview.audience,
        json.dumps(base_payload.get("planned_hires", []), sort_keys=True, default=str),
        json.dumps(base_payload.get("non_hire_outcomes", []), sort_keys=True, default=str),
        json.dumps(base_payload.get("freshness", {}), sort_keys=True, default=str),
    )
    artifact_path = preview.artifact_path or (
        system.runtime_dir / "wiki" / "route-executions" / f"{route_execution_id}.json"
    )
    payload = {
        **base_payload,
        "route_execution_id": route_execution_id,
        "artifact_path": str(artifact_path),
        "written_at": utc_now(),
    }
    text = json.dumps(payload, indent=2, sort_keys=True, default=str) + "\n"
    artifact_path.parent.mkdir(parents=True, exist_ok=True)
    artifact_path.write_text(text, encoding="utf-8")
    content_hash = hashlib.sha256(text.encode("utf-8")).hexdigest()
    artifact_id = stable_id("artifact", "wiki_route_execution_preview", route_execution_id, content_hash)
    store = LakeStore(system.storage_dir)
    store.ensure()
    artifact_row = store.artifact_row(
        "wiki_route_execution_preview",
        artifact_id=artifact_id,
        uri=f"file://{artifact_path}",
        path=str(artifact_path),
        content_type="application/json",
        content_hash=content_hash,
        bytes=len(text.encode("utf-8")),
        source="memory.wiki.route_executor",
        state="planned",
        text=f"wiki route execution dry-run {route_execution_id}",
        metadata={
            "route_execution_id": route_execution_id,
            "workspace": str(preview.workspace),
            "concept_dir": str(preview.concept_dir),
            "audience": preview.audience,
            "planned_hire_count": preview.planned_hire_count,
            "non_hire_count": preview.non_hire_count,
            "freshness_passed": bool((preview.freshness or {}).get("passed", True)),
        },
    )
    store.replace_rows("artifacts", "artifact_id", [artifact_row])
    store.append_evidence(
        "wiki_route_execution.preview_written",
        artifact_id=artifact_id,
        status="passed",
        checker="memory.wiki.route_executor",
        text="wiki route execution dry-run artifact written",
        checks=["artifact_path.exists", "payload.planned_hire_count >= 0"],
        payload={"route_execution_id": route_execution_id, "path": str(artifact_path)},
    )
    freshness = preview.freshness or {}
    if freshness:
        store.append_evidence(
            "source_import.fresh",
            artifact_id=artifact_id,
            status="passed" if freshness.get("passed") else "failed",
            checker="memory.wiki.route_executor",
            text="source importer freshness checked for route dry-run",
            checks=["required source latest_ts within max_age_hours"],
            payload=freshness,
        )
    store.append_event(
        "wiki.route_execution.preview_written",
        source="memory.wiki.route_executor",
        actor="route-dry-run",
        subject=route_execution_id,
        artifact_id=artifact_id,
        text=f"Prepared wiki route dry-run with {preview.planned_hire_count} planned hires.",
        payload={
            "route_execution_id": route_execution_id,
            "path": str(artifact_path),
            "planned_hire_count": preview.planned_hire_count,
            "non_hire_count": preview.non_hire_count,
        },
    )
    return WikiRouteExecutionArtifact(
        artifact_id=artifact_id,
        path=artifact_path,
        content_hash=content_hash,
        bytes=len(text.encode("utf-8")),
    )


def scan_wiki_inventory(*, workspace: Path, concept_dir: Path, audience: str = "private") -> dict[str, Any]:
    concepts = collect_concepts(concept_dir)
    pages = [scan_wiki_page(path) for path in sorted(workspace.glob("*.md"))]
    talk_folders = [scan_talk_folder(path) for path in sorted(workspace.glob("*.talk")) if path.is_dir()]
    talk_folders.extend(
        scan_talk_folder(path)
        for path in sorted(workspace.glob(f"*.{audience}.talk"))
        if path.is_dir()
    )
    unique_talk_folders = {item["path"]: item for item in talk_folders}
    generated = {
        name: (workspace / name).is_file()
        for name in ("index.md", "topics.md", "projects.md", "open-questions.md", "this-week.md")
    }
    return {
        "kind": "wiki_inventory",
        "workspace": str(workspace),
        "concept_dir": str(concept_dir),
        "audience": audience,
        "pages": pages,
        "concepts": [
            {
                "slug": concept.slug,
                "display": concept.display,
                "path": str(concept.path),
                "categories": list_value(concept.frontmatter.get("categories")),
                "aliases": list_value(concept.frontmatter.get("aliases")),
                "subject_type": concept.frontmatter.get("subject-type", ""),
                "project_status": concept.frontmatter.get("project-status", ""),
                "operator_touched_count": count_operator_touched(concept.path.read_text(encoding="utf-8")),
            }
            for concept in concepts
        ],
        "talk_folders": list(unique_talk_folders.values()),
        "generated_pages": generated,
        "summary": {
            "page_count": len(pages),
            "concept_count": len(concepts),
            "talk_folder_count": len(unique_talk_folders),
            "operator_touched_count": sum(page["operator_touched_count"] for page in pages)
            + sum(count_operator_touched(concept.path.read_text(encoding="utf-8")) for concept in concepts),
        },
    }


def scan_wiki_page(path: Path) -> dict[str, Any]:
    text = path.read_text(encoding="utf-8")
    sections = scan_sections(text)
    tier_info = page_tier_info(path.stem)
    era = tier_info["era"]
    return {
        "path": str(path),
        "name": path.name,
        "stem": path.stem,
        "kind": "for_you" if era and tier_info["canonical"] else "tier_output" if era else "page",
        "era": era,
        "tier": tier_info["tier"],
        "tier_model": "separate_files_private_canonical" if era else "",
        "era_window": era_window_payload(era) if era else {},
        "tier_paths": tier_paths_payload(path.parent, era) if era else {},
        "operator_touched_count": count_operator_touched(text),
        "filled_day_section_count": sum(
            1 for section in sections if re.match(r"^\d{4}-\d{2}-\d{2}$", section["slug"]) and section["filled"]
        ),
        "biography_filled": any(section["slug"] == "biography" and section["filled"] for section in sections),
        "sections": sections,
        "mtime": path.stat().st_mtime,
    }


def scan_sections(text: str) -> list[dict[str, Any]]:
    section_re = re.compile(r'<!-- section:[^>]*?"([^"]+)"[^>]*-->')
    markers = list(section_re.finditer(text))
    sections: list[dict[str, Any]] = []
    for index, marker in enumerate(markers):
        slug = marker.group(1)
        start = marker.end()
        end = markers[index + 1].start() if index + 1 < len(markers) else len(text)
        block = text[start:end]
        filled = bool(block.strip()) and "<!-- empty:" not in block
        sections.append(
            {
                "slug": slug,
                "filled": filled,
                "operator_touched": "operator-touched:" in block,
            }
        )
    return sections


def scan_talk_folder(path: Path) -> dict[str, Any]:
    entries = [scan_talk_entry(entry) for entry in sorted(path.glob("*.md")) if not entry.name.startswith("_")]
    counts: dict[str, int] = {}
    for entry in entries:
        counts[entry["kind"]] = counts.get(entry["kind"], 0) + 1
    parsed = parse_talk_folder_name(path.name)
    return {
        "path": str(path),
        "name": path.name,
        "era": parsed["era"],
        "audience": parsed["audience"],
        "talk_kind": parsed["kind"],
        "era_window": era_window_payload(parsed["era"]) if parsed["era"] else {},
        "entries": entries,
        "entry_count": len(entries),
        "counts": counts,
    }


def scan_talk_entry(path: Path) -> dict[str, Any]:
    text = path.read_text(encoding="utf-8")
    frontmatter = parse_frontmatter(text)
    return {
        "path": str(path),
        "name": path.name,
        "stem": path.stem,
        "kind": str(frontmatter.get("kind") or kind_from_filename(path.name)),
        "parent": str(frontmatter.get("parent") or ""),
        "ts": str(frontmatter.get("ts") or timestamp_from_filename(path.name) or ""),
        "target_section": str(frontmatter.get("target-section") or frontmatter.get("target_section") or ""),
        "mtime": path.stat().st_mtime,
    }


def kind_from_filename(name: str) -> str:
    for kind in (
        "conversation",
        "reply",
        "proposal",
        "decided",
        "deferred",
        "concern",
        "contradiction",
        "redacted",
        "fading",
        "synthesis",
    ):
        if f".{kind}." in name or name.endswith(f".{kind}.md"):
            return kind
    return ""


def timestamp_from_filename(name: str) -> str | None:
    match = re.match(r"^(\d{4}-\d{2}-\d{2}T\d{2})-(\d{2})Z", name)
    if not match:
        return None
    return f"{match.group(1)}:{match.group(2)}:00Z"


def count_operator_touched(text: str) -> int:
    return len(re.findall(r"operator-touched:", text))


def parse_wiki_date(value: str | date) -> date:
    if isinstance(value, date):
        return value
    if not ISO_DATE_RE.match(value):
        raise WikiError(f"expected YYYY-MM-DD date, got {value!r}")
    return date.fromisoformat(value)


def monday_anchor(value: str | date) -> str:
    day = parse_wiki_date(value)
    return (day - timedelta(days=day.weekday())).isoformat()


def rolling_window_days(anchor: str | date, *, days: int = 14) -> list[str]:
    start = parse_wiki_date(anchor)
    return [(start + timedelta(days=offset)).isoformat() for offset in range(days)]


def adjacent_era_ids(anchor: str | date, *, before: int = 1, after: int = 1) -> list[str]:
    start = parse_wiki_date(monday_anchor(anchor))
    eras = [start + timedelta(days=7 * offset) for offset in range(-before, after + 1)]
    return [era.isoformat() for era in eras]


def era_window_payload(anchor: str) -> dict[str, Any]:
    era = monday_anchor(anchor)
    days = rolling_window_days(era)
    return {
        "era": era,
        "window_start": days[0],
        "window_end": days[-1],
        "window_days": days,
        "rolling_days": len(days),
        "adjacent_eras": adjacent_era_ids(era),
    }


def page_tier_info(stem: str) -> dict[str, Any]:
    parts = stem.split(".")
    era = parts[0]
    if not ISO_DATE_RE.match(era):
        return {"era": "", "tier": "", "canonical": False}
    tier = parts[1] if len(parts) > 1 else "private"
    return {"era": era, "tier": tier, "canonical": len(parts) == 1}


def tier_paths_payload(workspace: Path, era: str) -> dict[str, str]:
    return {
        "canonical_private": str(workspace / f"{era}.md"),
        "internal": str(workspace / f"{era}.internal.md"),
        "public": str(workspace / f"{era}.public.md"),
    }


def talk_folder_path(workspace: Path, era: str, audience: str) -> Path:
    return workspace / f"{era}.{audience}.talk"


def adjacent_talk_folder_payload(workspace: Path, era: str, audience: str) -> list[dict[str, Any]]:
    return [
        {
            "era": adjacent,
            "path": str(talk_folder_path(workspace, adjacent, audience)),
            "exists": talk_folder_path(workspace, adjacent, audience).is_dir(),
        }
        for adjacent in adjacent_era_ids(era)
    ]


def prior_biography_paths(workspace: Path, era: str, *, limit: int = 2) -> list[str]:
    anchor = parse_wiki_date(era)
    paths: list[str] = []
    for offset in range(1, limit + 1):
        prior = (anchor - timedelta(days=7 * offset)).isoformat()
        path = workspace / f"{prior}.md"
        if path.exists():
            paths.append(str(path))
    return paths


def parse_talk_folder_name(name: str) -> dict[str, str]:
    if name == "your-context.talk":
        return {"kind": "your_context", "era": "", "audience": ""}
    match = re.match(r"^(?P<era>\d{4}-\d{2}-\d{2})(?:\.(?P<audience>[^.]+))?\.talk$", name)
    if not match:
        return {"kind": "generic", "era": "", "audience": ""}
    return {
        "kind": "for_you",
        "era": match.group("era"),
        "audience": match.group("audience") or "private",
    }


def route_common(
    *,
    workspace: Path,
    concept_dir: Path,
    audience: str,
    job: str,
    job_key: str,
    reason: str,
    era: str = "",
    target_day: str = "",
    ownership: dict[str, Any] | None = None,
    source_paths: list[str] | None = None,
    expected_outputs: list[str] | None = None,
    validators: list[str] | None = None,
    budget: dict[str, Any] | None = None,
    concurrency_group: str = "wiki-agents",
    outcome: str = "hire",
) -> dict[str, Any]:
    packet_paths = list(source_paths or [])
    source_manifest = source_packet_manifest(packet_paths)
    source_manifest_text = json.dumps(source_manifest, sort_keys=True, default=str)
    source_packet_bytes = sum(int(item.get("bytes") or 0) for item in source_manifest)
    source_packet_tokens = sum(int(item.get("estimated_tokens") or 0) for item in source_manifest)
    budget_payload = budget or {
        "model_family": "claude-opus",
        "max_prompt_tokens": 128000,
        "timeout_minutes": 30,
        "priority": "normal",
        "can_batch": False,
    }
    row: dict[str, Any] = {
        "route_id": stable_id("wiki-route", job, job_key, audience, era, target_day),
        "job_key": job_key,
        "job": job,
        "outcome": outcome,
        "reason": reason,
        "audience": audience,
        "workspace": str(workspace),
        "concept_dir": str(concept_dir),
        "ownership": ownership or {},
        "source_packet": {
            "kind": "wiki_route_source_packet",
            "mode": "bounded_markdown_packet",
            "loaded_at_birth": True,
            "source_paths": packet_paths,
            "target_day": target_day,
            "source_manifest": source_manifest,
            "bytes": source_packet_bytes,
            "estimated_tokens": source_packet_tokens,
            "sha256": text_sha256(source_manifest_text),
            "requires_split": source_packet_tokens > int(budget_payload.get("max_prompt_tokens") or 0)
            if int(budget_payload.get("max_prompt_tokens") or 0) > 0
            else False,
        },
        "task_contract": f"{job} from wiki role route row",
        "expected_outputs": list(expected_outputs or []),
        "validators": list(validators or []),
        "budget": budget_payload,
        "concurrency_group": concurrency_group,
    }
    if era:
        row["era"] = era
        row["era_window"] = era_window_payload(era)
        row["adjacent_talk_folders"] = adjacent_talk_folder_payload(workspace, era, audience)
        row["tier_model"] = {
            "kind": "separate_files_private_canonical",
            "canonical_private_source": str(workspace / f"{era}.md"),
            "tier_outputs": {
                "internal": str(workspace / f"{era}.internal.md"),
                "public": str(workspace / f"{era}.public.md"),
            },
            "phantom_private_file_required": False,
        }
        row["source_packet"]["era"] = era
        row["source_packet"]["era_window"] = row["era_window"]
    return row


def route_job_params(row: dict[str, Any]) -> dict[str, Any]:
    ignored = {
        "job_key",
        "job",
        "outcome",
        "reason",
        "ownership",
        "source_packet",
        "task_contract",
        "expected_outputs",
        "validators",
        "budget",
        "concurrency_group",
        "route_id",
    }
    return {key: value for key, value in row.items() if key not in ignored}


def prompt_stack_parts_for_route(row: dict[str, Any]) -> list[dict[str, str]]:
    job = str(row.get("job", ""))
    role = job.rsplit(".", 1)[-1].replace("_", "-")
    return [
        {"name": "shared_agent_profile", "path": "memory/plugins/base-memory-v1/prompts/agent-profile.md"},
        {"name": f"{role}_role_prompt", "path": f"memory/plugins/base-memory-v1/prompts/{role}.md"},
        {"name": "birth_loaded_source_packet", "path": "<runtime-generated-source-packet.md>"},
        {"name": "route_task_prompt", "path": "<runtime-generated-task.md>"},
    ]


def wiki_route_prompt_stack_preview(system: MemorySystem, row: dict[str, Any]) -> dict[str, Any]:
    source_packet_text = render_wiki_route_source_packet(row)
    stack = build_wiki_route_prompt_stack(system, row=row, source_packet_text=source_packet_text)
    return {
        **stack.to_payload(),
        "source_packet": {
            **dict(row.get("source_packet", {})),
            "rendered_sha256": text_sha256(source_packet_text),
            "rendered_bytes": len(source_packet_text.encode("utf-8")),
            "rendered_estimated_tokens": estimate_token_count(source_packet_text),
        },
        "task_contract": row.get("task_contract", ""),
    }


def build_wiki_route_prompt_stack(system: MemorySystem, *, row: dict[str, Any], source_packet_text: str) -> PromptStack:
    job_id = str(row.get("job") or "")
    job = system.jobs.get(job_id, {})
    agent = system.agents.get(str(job.get("agent") or ""), {})
    parts: list[PromptPart] = [
        PromptPart(
            name="system_addition",
            text=(
                "# 1Context Wiki Role Agent System Addition\n\n"
                "You are a hired 1Context wiki role agent. Your source packet is loaded below "
                "as direct working context for this route. Stay inside the declared ownership "
                "scope. Skip, forget, defer, no_change, and needs_approval are valid outcomes "
                "when the route facts call for them."
            ),
        ),
        PromptPart(name="birth_loaded_source_packet", text=source_packet_text),
    ]
    for index, relative_path in enumerate(agent.get("prompt_paths", [])):
        parts.append(prompt_part_from_file(f"agent_prompt_{index + 1}", system.plugin_path / str(relative_path)))
    for index, relative_path in enumerate(job.get("prompt_paths", [])):
        parts.append(prompt_part_from_file(f"job_prompt_{index + 1}", system.plugin_path / str(relative_path)))
    parts.append(PromptPart(name="route_task_contract", text=render_wiki_route_task_contract(row)))
    return PromptStack(tuple(parts))


def render_wiki_route_task_contract(row: dict[str, Any]) -> str:
    payload = {
        "route_id": row.get("route_id", ""),
        "job_key": row.get("job_key", ""),
        "job": row.get("job", ""),
        "reason": row.get("reason", ""),
        "outcome": row.get("outcome", "hire"),
        "ownership": row.get("ownership", {}),
        "expected_outputs": row.get("expected_outputs", []),
        "validators": row.get("validators", []),
        "budget": row.get("budget", {}),
        "params": route_job_params(row),
    }
    return "# Route Task Contract\n\n```json\n" + json.dumps(payload, indent=2, sort_keys=True, default=str) + "\n```\n"


def render_wiki_route_source_packet(row: dict[str, Any]) -> str:
    source_packet = row.get("source_packet", {}) if isinstance(row.get("source_packet"), dict) else {}
    lines = [
        "---",
        "kind: birth_loaded_wiki_route_source_packet",
        f"route_id: {row.get('route_id', '')}",
        f"job: {row.get('job', '')}",
        f"job_key: {row.get('job_key', '')}",
        "loaded_at_birth: true",
        "---",
        "",
        "# Birth-Loaded Wiki Route Source Packet",
        "",
        f"Reason: {row.get('reason', '')}",
        "",
        "## Ownership",
        "",
        "```json",
        json.dumps(row.get("ownership", {}), indent=2, sort_keys=True, default=str),
        "```",
        "",
        "## Sources",
        "",
    ]
    for path_text in source_packet.get("source_paths", []):
        lines.extend(render_source_path_for_packet(Path(str(path_text))))
    for file_slice in source_packet.get("file_slices", []):
        if isinstance(file_slice, dict):
            lines.extend(render_source_slice_for_packet(file_slice))
    return "\n".join(lines).rstrip() + "\n"


def render_source_path_for_packet(path: Path) -> list[str]:
    lines = [f"### {path}", ""]
    if path.is_file():
        lines.extend([path.read_text(encoding="utf-8", errors="replace").rstrip(), ""])
        return lines
    if path.is_dir():
        all_files = sorted(child for child in path.rglob("*.md") if child.is_file())
        included_files = all_files[:40]
        lines.extend(
            [
                (
                    f"Directory source with {len(included_files)} of {len(all_files)} markdown files "
                    "included in this prompt preview."
                ),
                "",
            ]
        )
        if len(included_files) < len(all_files):
            lines.extend(["[preview truncated; route budget is computed from the full directory receipt]", ""])
        for child in included_files:
            lines.extend([f"#### {child.relative_to(path)}", "", child.read_text(encoding="utf-8", errors="replace").rstrip(), ""])
        return lines
    lines.extend(["[missing source path at prompt assembly time]", ""])
    return lines


def render_source_slice_for_packet(file_slice: dict[str, Any]) -> list[str]:
    path = Path(str(file_slice.get("path") or ""))
    start = max(0, int(file_slice.get("start_char") or 0))
    end = max(start, int(file_slice.get("end_char") or start))
    index = int(file_slice.get("slice_index") or 0)
    count = int(file_slice.get("slice_count") or 0)
    lines = [f"### {path} slice {index}/{count}", ""]
    if not path.is_file():
        lines.extend(["[missing source slice path at prompt assembly time]", ""])
        return lines
    text = path.read_text(encoding="utf-8", errors="replace")
    lines.extend(
        [
            f"Character window: {start}-{min(end, len(text))} of {len(text)}.",
            "",
            text[start:end].rstrip(),
            "",
        ]
    )
    return lines


def source_packet_manifest(paths: list[str]) -> list[dict[str, Any]]:
    return [source_path_receipt(Path(path)) for path in paths]


def source_path_receipt(path: Path) -> dict[str, Any]:
    if path.is_file():
        data = path.read_bytes()
        return {
            "path": str(path),
            "kind": "file",
            "exists": True,
            "bytes": len(data),
            "estimated_tokens": estimate_token_count(data.decode("utf-8", errors="replace")),
            "sha256": hashlib.sha256(data).hexdigest(),
        }
    if path.is_dir():
        files = sorted(child for child in path.rglob("*.md") if child.is_file())
        digest = hashlib.sha256()
        total_bytes = 0
        for child in files:
            data = child.read_bytes()
            total_bytes += len(data)
            digest.update(str(child.relative_to(path)).encode("utf-8"))
            digest.update(b"\0")
            digest.update(hashlib.sha256(data).hexdigest().encode("ascii"))
            digest.update(b"\n")
        return {
            "path": str(path),
            "kind": "directory",
            "exists": True,
            "file_count": len(files),
            "bytes": total_bytes,
            "estimated_tokens": max(0, (total_bytes + 3) // 4),
            "sha256": digest.hexdigest(),
        }
    return {
        "path": str(path),
        "kind": "missing",
        "exists": False,
        "bytes": 0,
        "estimated_tokens": 0,
        "sha256": "",
    }


def split_oversized_wiki_routes(
    route: dict[str, list[dict[str, Any]]],
    *,
    workspace: Path,
    concept_dir: Path,
    audience: str,
) -> dict[str, list[dict[str, Any]]]:
    shard_rows: list[dict[str, Any]] = []
    aggregate_rows: list[dict[str, Any]] = []
    excluded_groups = {"non_hire_outcomes", "source_packet_shard_jobs", "source_packet_aggregate_jobs"}
    for group, rows in list(route.items()):
        if group in excluded_groups:
            continue
        for row in rows:
            source_packet = row.get("source_packet") if isinstance(row.get("source_packet"), dict) else {}
            if row.get("outcome", "hire") != "hire" or not source_packet.get("requires_split"):
                continue
            parent_route_id = str(row.get("route_id") or "")
            parent_job_key = str(row.get("job_key") or parent_route_id)
            max_tokens = int(row.get("budget", {}).get("max_prompt_tokens") or DEFAULT_SOURCE_SHARD_TOKENS)
            chunks = source_packet_chunks(source_packet.get("source_paths", []), max_tokens=max_tokens)
            row["outcome"] = "split_parent"
            row["split"] = {
                "reason": "source packet exceeds route budget; route bounded shard witnesses before aggregate/relaunch",
                "source_packet_estimated_tokens": source_packet.get("estimated_tokens", 0),
                "max_prompt_tokens": row.get("budget", {}).get("max_prompt_tokens", 0),
                "shard_count": len(chunks),
                "shard_job_group": "source_packet_shard_jobs",
                "aggregate_job_group": "source_packet_aggregate_jobs",
            }
            for index, chunk in enumerate(chunks, start=1):
                shard_key = f"source-shard:{parent_job_key}:{index:02d}"
                shard = route_common(
                    workspace=workspace,
                    concept_dir=concept_dir,
                    audience=audience,
                    job=WIKI_SOURCE_PACKET_SHARD_JOB,
                    job_key=shard_key,
                    reason=f"shard {index}/{len(chunks)} for oversized route {parent_job_key}",
                    era=str(row.get("era") or ""),
                    target_day=str(source_packet.get("target_day") or ""),
                    ownership={
                        "kind": "source_packet_shard_note",
                        "parent_route_id": parent_route_id,
                        "parent_job": row.get("job", ""),
                        "parent_job_key": parent_job_key,
                        "shard_index": index,
                        "shard_count": len(chunks),
                    },
                    source_paths=list(chunk["source_paths"]),
                    expected_outputs=["source_packet_shard_note.valid"],
                    validators=["source_packet_shard_note.frontmatter", "source_packet_shard_note.body_nonempty"],
                    budget={
                        "model_family": "claude-opus",
                        "max_prompt_tokens": max_tokens,
                        "timeout_minutes": 20,
                        "priority": "normal",
                        "can_batch": True,
                    },
                    concurrency_group="wiki-source-shards",
                )
                shard.update(
                    {
                        "parent_route_id": parent_route_id,
                        "parent_job": row.get("job", ""),
                        "parent_job_key": parent_job_key,
                        "parent_route_group": group,
                        "shard_index": index,
                        "shard_count": len(chunks),
                        "source_packet_chunk": chunk,
                    }
                )
                if chunk.get("file_slices"):
                    shard["source_packet"].update(
                        {
                            "file_slices": chunk["file_slices"],
                            "bytes": chunk.get("bytes", 0),
                            "estimated_tokens": chunk.get("estimated_tokens", 0),
                            "source_manifest": chunk.get("source_manifest", []),
                            "sha256": text_sha256(json.dumps(chunk, sort_keys=True, default=str)),
                            "requires_split": False,
                        }
                    )
                shard_rows.append(shard)
            aggregate_key = f"source-aggregate:{parent_job_key}"
            aggregate = route_common(
                workspace=workspace,
                concept_dir=concept_dir,
                audience=audience,
                job=WIKI_SOURCE_PACKET_AGGREGATE_JOB,
                job_key=aggregate_key,
                reason=f"aggregate {len(chunks)} shard notes for oversized route {parent_job_key}",
                era=str(row.get("era") or ""),
                target_day=str(source_packet.get("target_day") or ""),
                ownership={
                    "kind": "source_packet_aggregate",
                    "parent_route_id": parent_route_id,
                    "parent_job": row.get("job", ""),
                    "parent_job_key": parent_job_key,
                    "shard_count": len(chunks),
                },
                source_paths=[],
                expected_outputs=["source_packet_aggregate.ready"],
                validators=["source_packet_aggregate.references_all_shards"],
                budget={
                    "model_family": "claude-opus",
                    "max_prompt_tokens": int(row.get("budget", {}).get("max_prompt_tokens") or 128000),
                    "timeout_minutes": 30,
                    "priority": "normal",
                    "can_batch": False,
                },
                concurrency_group="wiki-source-aggregates",
            )
            aggregate.update(
                {
                    "parent_route_id": parent_route_id,
                    "parent_job": row.get("job", ""),
                    "parent_job_key": parent_job_key,
                    "parent_route_group": group,
                    "shard_job_keys": [f"source-shard:{parent_job_key}:{i:02d}" for i in range(1, len(chunks) + 1)],
                    "original_route": row,
                }
            )
            aggregate_rows.append(aggregate)
    route.setdefault("source_packet_shard_jobs", []).extend(shard_rows)
    route.setdefault("source_packet_aggregate_jobs", []).extend(aggregate_rows)
    return route


def source_packet_chunks(paths: Any, *, max_tokens: int) -> list[dict[str, Any]]:
    chunks: list[dict[str, Any]] = []
    current_paths: list[str] = []
    current_tokens = 0
    current_bytes = 0

    def flush() -> None:
        nonlocal current_paths, current_tokens, current_bytes
        if not current_paths:
            return
        chunk = {
            "source_paths": current_paths,
            "estimated_tokens": current_tokens,
            "bytes": current_bytes,
        }
        chunks.append(chunk)
        current_paths = []
        current_tokens = 0
        current_bytes = 0

    for path_text in paths or []:
        path = Path(str(path_text))
        source_paths = sorted(child for child in path.rglob("*.md") if child.is_file()) if path.is_dir() else [path]
        for source_path in source_paths:
            receipt = source_path_receipt(source_path)
            tokens = int(receipt.get("estimated_tokens") or 0)
            bytes_count = int(receipt.get("bytes") or 0)
            if tokens > max_tokens and source_path.is_file():
                flush()
                chunks.extend(source_file_slice_chunks(source_path, max_tokens=max_tokens, receipt=receipt))
                continue
            if current_paths and current_tokens + tokens > max_tokens:
                flush()
            current_paths.append(str(source_path))
            current_tokens += tokens
            current_bytes += bytes_count
    flush()
    return chunks or [{"source_paths": [], "estimated_tokens": 0, "bytes": 0}]


def source_file_slice_chunks(path: Path, *, max_tokens: int, receipt: dict[str, Any]) -> list[dict[str, Any]]:
    text = path.read_text(encoding="utf-8", errors="replace")
    if not text:
        return [{"source_paths": [str(path)], "estimated_tokens": 0, "bytes": 0}]
    max_chars = max(1, max_tokens * 4)
    slices = [(start, min(len(text), start + max_chars)) for start in range(0, len(text), max_chars)]
    chunks: list[dict[str, Any]] = []
    for index, (start, end) in enumerate(slices, start=1):
        slice_text = text[start:end]
        chunks.append(
            {
                "source_paths": [],
                "file_slices": [
                    {
                        "path": str(path),
                        "start_char": start,
                        "end_char": end,
                        "slice_index": index,
                        "slice_count": len(slices),
                        "source_sha256": receipt.get("sha256", ""),
                    }
                ],
                "source_manifest": [receipt],
                "estimated_tokens": estimate_token_count(slice_text),
                "bytes": len(slice_text.encode("utf-8")),
                "single_source_split": True,
            }
        )
    return chunks


def derive_wiki_role_route_plan(
    inventory: dict[str, Any],
    *,
    workspace: Path,
    concept_dir: Path,
    audience: str,
    target_redaction_tiers: tuple[str, ...],
) -> dict[str, list[dict[str, Any]]]:
    route: dict[str, list[dict[str, Any]]] = {
        "historian_jobs": [],
        "hourly_answerer_jobs": [],
        "editor_jobs": [],
        "for_you_curator_jobs": [],
        "context_curator_jobs": [],
        "librarian_jobs": [],
        "librarian_sweep_jobs": [],
        "biographer_jobs": [],
        "contradiction_jobs": [],
        "redaction_jobs": [],
        "reader_build_jobs": [],
        "non_hire_outcomes": [],
    }
    talk_folders = inventory["talk_folders"]
    pages_by_stem = {page["stem"]: page for page in inventory["pages"]}

    for folder in talk_folders:
        entries = folder["entries"]
        conversations_by_date: dict[str, int] = {}
        for entry in entries:
            match = re.match(r"^(\d{4}-\d{2}-\d{2})T\d{2}-00Z\.conversation$", entry["stem"])
            if match:
                conversations_by_date[match.group(1)] = conversations_by_date.get(match.group(1), 0) + 1
        for day, count in sorted(conversations_by_date.items()):
            has_historian_output = any(
                entry["stem"].startswith(f"{day}T23-59Z.") and entry["kind"] in {"proposal", "question", "concern", "reply", "synthesis"}
                for entry in entries
            )
            if not has_historian_output:
                era = folder["era"] or monday_anchor(day)
                route["historian_jobs"].append(
                    {
                        **route_common(
                            workspace=workspace,
                            concept_dir=concept_dir,
                            audience=audience,
                            job="memory.wiki.historian",
                            job_key=f"historian:{day}:{folder['name']}",
                            reason="hourly conversations exist without historian synthesis for this day",
                            era=era,
                            target_day=day,
                            ownership={"kind": "talk_folder_append", "path": folder["path"]},
                            source_paths=[folder["path"]],
                            expected_outputs=["historian_entry.valid"],
                            validators=["talk_entry.frontmatter", "talk_entry.body_nonempty"],
                        ),
                        "date": day,
                        "talk_folder": folder["path"],
                        "hourly_entry_count": count,
                    }
                )
            has_editor_proposal = any(f".proposal.editor-day-{day}" in entry["name"] for entry in entries)
            if not has_editor_proposal:
                era = folder["era"] or monday_anchor(day)
                route["editor_jobs"].append(
                    {
                        **route_common(
                            workspace=workspace,
                            concept_dir=concept_dir,
                            audience=audience,
                            job="memory.daily.editor",
                            job_key=f"editor:{day}:{folder['name']}",
                            reason="hourly conversations exist without an editor day-section proposal",
                            era=era,
                            target_day=day,
                            ownership={"kind": "talk_folder_append", "path": folder["path"], "target_section": day},
                            source_paths=[folder["path"], str(workspace / f"{era}.md")],
                            expected_outputs=["daily_section_proposal.verified"],
                            validators=["proposal.frontmatter", "proposal.target_section"],
                        ),
                        "date": day,
                        "talk_folder": folder["path"],
                        "run_era": era,
                    }
                )

        for entry in entries:
            if entry["kind"] == "reply" and entry["parent"].endswith(".conversation"):
                answered = any(item["parent"] == entry["stem"] for item in entries)
                if not answered:
                    hour_match = re.match(r"^(\d{4}-\d{2}-\d{2})T(\d{2})-00Z", entry["parent"])
                    if hour_match:
                        era = folder["era"] or monday_anchor(hour_match.group(1))
                        route["hourly_answerer_jobs"].append(
                            {
                                **route_common(
                                    workspace=workspace,
                                    concept_dir=concept_dir,
                                    audience=audience,
                                    job="memory.hourly.answerer",
                                    job_key=f"hourly-answer:{entry['stem']}",
                                    reason="historian reply asks a question about a specific hourly conversation",
                                    era=era,
                                    target_day=hour_match.group(1),
                                    ownership={"kind": "talk_folder_append", "path": folder["path"], "parent": entry["stem"]},
                                    source_paths=[folder["path"], entry["path"]],
                                    expected_outputs=["hourly_answer_reply.valid"],
                                    validators=["reply.frontmatter", "reply.parent_matches_question"],
                                ),
                                "date": hour_match.group(1),
                                "hour": hour_match.group(2),
                                "talk_folder": folder["path"],
                                "question_path": entry["path"],
                                "parent_entry": entry["parent"],
                            }
                        )

        pending_editor = pending_by_slug(entries, proposal_marker=".proposal.editor-day-", decided_marker=".decided.editor-day-")
        if pending_editor and folder["name"].endswith(f".{audience}.talk"):
            era = folder["name"].split(f".{audience}.talk", 1)[0]
            route["for_you_curator_jobs"].append(
                {
                    **route_common(
                        workspace=workspace,
                        concept_dir=concept_dir,
                        audience=audience,
                        job="memory.wiki.for_you_curator",
                        job_key=f"for-you-curator:{era}:{audience}",
                        reason="pending editor proposals need curator decisions and article-section application",
                        era=era,
                        ownership={"kind": "article_sections", "path": str(workspace / f"{era}.md"), "sections": pending_editor},
                        source_paths=[
                            str(workspace / f"{era}.md"),
                            folder["path"],
                            *[item["path"] for item in adjacent_talk_folder_payload(workspace, era, audience) if item["exists"]],
                        ],
                        expected_outputs=["article_section.updated_or_skipped", "curator_decisions.recorded"],
                        validators=["operator_touched.preflight", "article_section.diff_scope", "curator_decision.frontmatter"],
                    ),
                    "era": era,
                    "article_path": str(workspace / f"{era}.md"),
                    "talk_folder": folder["path"],
                    "pending_count": len(pending_editor),
                    "pending_sections": pending_editor,
                }
            )

        pending_concepts = pending_by_slug(entries, proposal_marker=".proposal.concept-", decided_marker=".decided.concept-", alternate_decided_marker=".deferred.concept-")
        if pending_concepts:
            era = folder["era"] or folder["name"].split(f".{audience}.talk", 1)[0]
            route["librarian_jobs"].append(
                {
                    **route_common(
                        workspace=workspace,
                        concept_dir=concept_dir,
                        audience=audience,
                        job="memory.wiki.librarian",
                        job_key=f"librarian:{folder['name']}",
                        reason="pending concept proposals need notability decision, create, expand, or defer",
                        era=era if ISO_DATE_RE.match(era) else "",
                        ownership={"kind": "concept_pages_and_decisions", "concept_dir": str(concept_dir), "talk_folder": folder["path"]},
                        source_paths=[folder["path"], str(workspace / f"{era}.md"), str(workspace / "your-context.md"), str(concept_dir)],
                        expected_outputs=["concept_page.created_expanded_or_deferred", "librarian_decisions.recorded"],
                        validators=["concept.frontmatter", "expand_before_duplicate", "decision.frontmatter"],
                    ),
                    "era": era,
                    "talk_folder": folder["path"],
                    "article_path": str(workspace / f"{era}.md"),
                    "your_context_path": str(workspace / "your-context.md"),
                    "concept_dir": str(concept_dir),
                    "pending_count": len(pending_concepts),
                    "pending_concepts": pending_concepts,
                }
            )

        if folder["name"] == "your-context.talk":
            pending_ycx = pending_by_slug(entries, proposal_marker=".proposal.ycx-", decided_marker=".decided.")
            if pending_ycx:
                route["context_curator_jobs"].append(
                    {
                        **route_common(
                            workspace=workspace,
                            concept_dir=concept_dir,
                            audience=audience,
                            job="memory.wiki.context_curator",
                            job_key="context-curator:your-context",
                            reason="pending Your Context proposals need curator decisions and article updates",
                            ownership={"kind": "article_sections", "path": str(workspace / "your-context.md"), "sections": pending_ycx},
                            source_paths=[str(workspace / "your-context.md"), folder["path"]],
                            expected_outputs=["your_context.updated_or_skipped", "context_curator_decisions.recorded"],
                            validators=["operator_touched.preflight", "article_section.diff_scope", "curator_decision.frontmatter"],
                        ),
                        "article_path": str(workspace / "your-context.md"),
                        "talk_folder": folder["path"],
                        "pending_count": len(pending_ycx),
                        "pending_sections": pending_ycx,
                    }
                )

    for page in inventory["pages"]:
        if page["kind"] != "for_you":
            continue
        era = page["era"]
        if page["filled_day_section_count"] >= 3 and not page["biography_filled"]:
            route["biographer_jobs"].append(
                {
                    **route_common(
                        workspace=workspace,
                        concept_dir=concept_dir,
                        audience=audience,
                        job="memory.wiki.biographer",
                        job_key=f"biographer:{era}:{audience}",
                        reason="enough day sections exist and Biography is empty or stale",
                        era=era,
                        ownership={"kind": "article_section", "path": page["path"], "section": "biography"},
                        source_paths=[
                            page["path"],
                            str(workspace / "your-context.md"),
                            *prior_biography_paths(workspace, era, limit=2),
                        ],
                        expected_outputs=["biography.updated_or_skipped"],
                        validators=["operator_touched.preflight", "biography.section_only_diff"],
                    ),
                    "era": era,
                    "article_path": page["path"],
                    "your_context_path": str(workspace / "your-context.md"),
                    "prior_biography_paths": prior_biography_paths(workspace, era, limit=2),
                    "filled_day_section_count": page["filled_day_section_count"],
                }
            )
        if page["filled_day_section_count"] >= 2:
            for target_tier in target_redaction_tiers:
                target = workspace / f"{era}.{target_tier}.md"
                if not target.exists() or target.stat().st_mtime < page["mtime"]:
                    route["redaction_jobs"].append(
                        {
                            **route_common(
                                workspace=workspace,
                                concept_dir=concept_dir,
                                audience=audience,
                                job="memory.wiki.redactor",
                                job_key=f"redactor:{era}:{target_tier}",
                                reason=f"{target_tier} tier output is missing or older than canonical private source",
                                era=era,
                                ownership={"kind": "tier_output", "source_path": page["path"], "target_path": str(target)},
                                source_paths=[page["path"], str(workspace / f"{era}.{audience}.talk")],
                                expected_outputs=["redacted_tier_file.exists", "redaction_summary.recorded"],
                                validators=["source_hash.unchanged", "target_tier.frontmatter", "redaction_summary.frontmatter"],
                                concurrency_group="wiki-redaction",
                            ),
                            "era": era,
                            "source_tier": audience,
                            "target_tier": target_tier,
                            "source_path": page["path"],
                            "target_path": str(target),
                            "talk_folder": str(workspace / f"{era}.{audience}.talk"),
                        }
                    )

    if inventory["concepts"]:
        route["librarian_sweep_jobs"].append(
            {
                **route_common(
                    workspace=workspace,
                    concept_dir=concept_dir,
                    audience=audience,
                    job="memory.wiki.librarian_sweep",
                    job_key="librarian-sweep:default",
                    reason="concept corpus exists and needs periodic reinforcement/fading review",
                    ownership={"kind": "concept_sweep", "concept_dir": str(concept_dir)},
                    source_paths=[str(workspace), str(concept_dir)],
                    expected_outputs=["concept_sweep.decision_recorded"],
                    validators=["sweep_decisions.frontmatter", "forgetting_quota.review"],
                    budget={"model_family": "claude-opus", "max_prompt_tokens": 128000, "timeout_minutes": 45},
                    concurrency_group="wiki-maintenance",
                ),
                "workspace": str(workspace),
                "concept_dir": str(concept_dir),
                "concept_count": len(inventory["concepts"]),
            }
        )
        route["contradiction_jobs"].append(
            {
                **route_common(
                    workspace=workspace,
                    concept_dir=concept_dir,
                    audience=audience,
                    job="memory.wiki.contradiction_flagger",
                    job_key="contradiction:default-window",
                    reason="recent wiki changes may introduce claim drift or contradictions",
                    ownership={"kind": "talk_folder_append", "path": str(workspace)},
                    source_paths=[str(workspace), str(concept_dir)],
                    expected_outputs=["contradiction_flags.recorded_or_skipped"],
                    validators=["contradiction_flags.append_only", "source_hashes.unchanged"],
                    concurrency_group="wiki-maintenance",
                ),
                "workspace": str(workspace),
                "concept_dir": str(concept_dir),
                "window_days": 7,
            }
        )
    else:
        route["non_hire_outcomes"].append(
            {
                **route_common(
                    workspace=workspace,
                    concept_dir=concept_dir,
                    audience=audience,
                    job="memory.wiki.librarian_sweep",
                    job_key="librarian-sweep:no-concepts",
                    reason="no concept pages exist yet, so sweep would be empty",
                    outcome="skip",
                ),
            }
        )

    if not all(inventory["generated_pages"].values()):
        route["reader_build_jobs"].append(
            {
                **route_common(
                    workspace=workspace,
                    concept_dir=concept_dir,
                    audience=audience,
                    job="memory.wiki.build_inputs",
                    job_key="reader-build:generated-pages-missing",
                    reason="one or more generated reader pages are missing",
                    ownership={"kind": "deterministic_reader_surface", "workspace": str(workspace)},
                    source_paths=[str(workspace), str(concept_dir)],
                    expected_outputs=["wiki_reader_surface.ready"],
                    validators=["generated_pages.exist", "backlinks.valid"],
                    budget={"model_family": "deterministic", "max_prompt_tokens": 0, "timeout_minutes": 2},
                    concurrency_group="deterministic-wiki",
                ),
                "workspace": str(workspace),
                "concept_dir": str(concept_dir),
            }
        )
    else:
        route["non_hire_outcomes"].append(
            {
                **route_common(
                    workspace=workspace,
                    concept_dir=concept_dir,
                    audience=audience,
                    job="memory.wiki.build_inputs",
                    job_key="reader-build:generated-pages-fresh",
                    reason="generated reader pages already exist",
                    outcome="no_change",
                    budget={"model_family": "deterministic", "max_prompt_tokens": 0, "timeout_minutes": 2},
                    concurrency_group="deterministic-wiki",
                ),
            }
        )

    return split_oversized_wiki_routes(route, workspace=workspace, concept_dir=concept_dir, audience=audience)


def pending_by_slug(
    entries: list[dict[str, Any]],
    *,
    proposal_marker: str,
    decided_marker: str,
    alternate_decided_marker: str = "",
) -> list[str]:
    proposed: set[str] = set()
    decided: set[str] = set()
    for entry in entries:
        stem = entry["stem"]
        if proposal_marker in stem:
            proposed.add(stem.split(proposal_marker, 1)[1])
        if decided_marker in stem:
            decided.add(stem.split(decided_marker, 1)[1])
        if alternate_decided_marker and alternate_decided_marker in stem:
            decided.add(stem.split(alternate_decided_marker, 1)[1])
    return sorted(proposed - decided)


def parse_frontmatter(text: str) -> dict[str, Any]:
    match = FRONTMATTER_RE.match(text)
    if not match:
        return {}
    result: dict[str, Any] = {}
    current_key: str | None = None
    for raw in match.group(1).splitlines():
        line = raw.rstrip()
        if not line.strip():
            current_key = None
            continue
        if line.startswith("  - ") and current_key:
            result.setdefault(current_key, []).append(line[4:].strip().strip("\"'"))
            continue
        if ":" not in line:
            current_key = None
            continue
        key, _, value = line.partition(":")
        key = key.strip()
        value = value.strip()
        if value == "":
            result[key] = []
            current_key = key
        elif value.startswith("[") and value.endswith("]"):
            inner = value[1:-1].strip()
            result[key] = [item.strip().strip("\"'") for item in inner.split(",") if item.strip()]
            current_key = None
        elif value.lower() in {"true", "false"}:
            result[key] = value.lower() == "true"
            current_key = None
        else:
            result[key] = value.strip("\"'")
            current_key = None
    return result


def slugify(text: str) -> str:
    slug = re.sub(r"[^a-z0-9]+", "-", text.strip().lower())
    return re.sub(r"-+", "-", slug).strip("-")


def strip_frontmatter(text: str) -> str:
    return FRONTMATTER_RE.sub("", text)


def first_sentence(text: str, *, max_chars: int = 160) -> str:
    body = strip_frontmatter(text)
    lines = [line.strip() for line in body.splitlines() if line.strip() and not line.startswith("#")]
    if not lines:
        return ""
    paragraph = " ".join(lines[:6])
    match = re.search(r"(.+?[.!?])(?:\s|$)", paragraph)
    sentence = (match.group(1) if match else paragraph).strip()
    if len(sentence) > max_chars:
        sentence = sentence[: max_chars - 1].rsplit(" ", 1)[0] + "..."
    return sentence


def display_name_from_body(slug: str, text: str) -> str:
    body = strip_frontmatter(text)
    match = H2_RE.search(body)
    return match.group(1).strip() if match else slug


def collect_concepts(concept_dir: Path) -> list[ConceptPage]:
    concepts: list[ConceptPage] = []
    for path in sorted(concept_dir.glob("*.md")):
        if path.name.endswith(".stdout.md") or path.name.startswith("."):
            continue
        text = path.read_text(encoding="utf-8")
        frontmatter = parse_frontmatter(text)
        if frontmatter.get("archived") is True or frontmatter.get("archived") == "true":
            continue
        concepts.append(
            ConceptPage(
                slug=path.stem,
                display=display_name_from_body(path.stem, text),
                path=path,
                frontmatter=frontmatter,
                lede=first_sentence(text),
            )
        )
    return concepts


def concept_indexes(concept_dir: Path) -> tuple[set[str], dict[str, str], dict[str, str]]:
    slugs: set[str] = set()
    aliases: dict[str, str] = {}
    external: dict[str, str] = {}
    for concept in collect_concepts(concept_dir):
        slugs.add(concept.slug)
        for alias in list_value(concept.frontmatter.get("aliases")):
            aliases[slugify(alias)] = concept.slug
        external_url = concept.frontmatter.get("external_url")
        if external_url:
            external[concept.slug] = str(external_url)
            for alias in list_value(concept.frontmatter.get("aliases")):
                external[slugify(alias)] = str(external_url)
    return slugs, aliases, external


def list_value(value: Any) -> list[str]:
    if isinstance(value, list):
        return [str(item) for item in value if str(item)]
    if isinstance(value, str) and value:
        return [value]
    return []


def render_topics_index(concepts: list[ConceptPage], *, web_base: str, today: date) -> str:
    by_category: dict[str, list[ConceptPage]] = {category: [] for category, _ in TOPIC_CATEGORIES}
    for concept in concepts:
        categories = list_value(concept.frontmatter.get("categories")) or [DEFAULT_TOPIC_CATEGORY]
        for category in categories:
            if category in by_category:
                by_category[category].append(concept)

    lines = wiki_frontmatter(
        title="Topics",
        slug="topics",
        summary="Index of named subjects organized by concept-page frontmatter.",
        tags=["topics", "index", "generated"],
        web_base=web_base,
        today=today,
    )
    lines.extend(["# Topics", ""])
    for category, description in TOPIC_CATEGORIES:
        lines.extend([f"## {category}", ""])
        entries = sorted(by_category[category], key=lambda item: item.display.lower())
        if not entries:
            lines.append(f"<!-- empty: agent-populated - {description}. -->")
        for concept in entries:
            suffix = f" - {concept.lede}" if concept.lede else ""
            lines.append(f"- [[{concept.display}]]{suffix}")
        lines.append("")
    lines.extend(["## See also", "", "- [Open Questions](./open-questions)", "- [Projects](./projects)", ""])
    return "\n".join(lines)


def collect_projects(concepts: list[ConceptPage]) -> list[ConceptPage]:
    return [concept for concept in concepts if concept.frontmatter.get("subject-type") == "project"]


def render_projects_index(projects: list[ConceptPage], *, web_base: str, today: date) -> str:
    by_status: dict[str, list[ConceptPage]] = {status: [] for _title, status, _desc in PROJECT_STATUS_SECTIONS}
    for project in projects:
        status = str(project.frontmatter.get("project-status") or "active")
        if status in by_status:
            by_status[status].append(project)

    lines = wiki_frontmatter(
        title="Projects",
        slug="projects",
        summary="Index of project concept pages grouped by status.",
        tags=["projects", "index", "generated"],
        web_base=web_base,
        today=today,
    )
    lines.extend(["# Projects", ""])
    for title, status, description in PROJECT_STATUS_SECTIONS:
        lines.extend([f"## {title}", ""])
        entries = sorted(by_status[status], key=lambda item: item.display.lower())
        if not entries:
            lines.append(f"<!-- empty: agent-populated - {description}. -->")
        for project in entries:
            lines.extend([f"### [[{project.display}]]", ""])
            if project.lede:
                lines.extend([project.lede, ""])
        lines.append("")
    lines.extend(
        [
            "## Cross-project patterns",
            "",
            "<!-- empty: biographer-populated - longitudinal patterns across projects. -->",
            "",
            "## See also",
            "",
            "- [Topics](./topics)",
            "- [Your Context](./your-context)",
            "",
        ]
    )
    return "\n".join(lines)


OPEN_SECTION_RE = re.compile(
    r"^###\s+(?:Open\s+Questions?|Open\s+going\s+into|Open\s+threads?)\s*$",
    re.IGNORECASE | re.MULTILINE,
)
NEXT_HEADING_RE = re.compile(r"^#{1,3}\s+", re.MULTILINE)


def collect_open_questions(workspace: Path, concept_dir: Path, *, web_base: str) -> list[dict[str, str]]:
    items: list[dict[str, str]] = []
    for concept in collect_concepts(concept_dir):
        text = concept.path.read_text(encoding="utf-8")
        body = extract_open_section(text)
        if body:
            items.append(
                {
                    "source": "concept",
                    "title": concept.display,
                    "body": body,
                    "url": f"{web_base}/{concept.slug}",
                }
            )

    section_re = re.compile(r'<!-- section:[^>]*?"(\d{4}-\d{2}-\d{2}|biography)"[^>]*-->')
    for article in sorted(workspace.glob("*.md")):
        if not re.match(r"^\d{4}-\d{2}-\d{2}$", article.stem):
            continue
        text = article.read_text(encoding="utf-8")
        markers = list(section_re.finditer(text))
        for index, marker in enumerate(markers):
            section = marker.group(1)
            start = marker.end()
            end = markers[index + 1].start() if index + 1 < len(markers) else len(text)
            block = text[start:end]
            for paragraph in open_paragraphs(block):
                title = f"For You - {article.stem} - {section}"
                items.append(
                    {
                        "source": "for-you",
                        "title": title,
                        "body": paragraph,
                        "url": f"{web_base}/for-you-{article.stem}#{section}",
                    }
                )
    return items


def extract_open_section(text: str) -> str | None:
    match = OPEN_SECTION_RE.search(text)
    if not match:
        return None
    rest = text[match.end() :]
    next_heading = NEXT_HEADING_RE.search(rest)
    body = rest[: next_heading.start()] if next_heading else rest
    return body.strip() or None


def open_paragraphs(text: str) -> list[str]:
    paragraphs = re.split(r"\n\n+", text)
    out: list[str] = []
    for paragraph in paragraphs:
        first = paragraph.lstrip().split("\n", 1)[0].lower()
        if first.startswith(("open going into", "open thread", "open question")):
            out.append(paragraph.strip())
    return out


def render_open_questions_page(items: list[dict[str, str]], *, web_base: str, today: date) -> str:
    lines = wiki_frontmatter(
        title="Open Questions",
        slug="open-questions",
        summary="Every open thread and unresolved decision surfaced across the wiki.",
        tags=["open-questions", "worklist", "generated"],
        web_base=web_base,
        today=today,
    )
    lines.extend(
        [
            "# Open Questions",
            "",
            f"_{len(items)} open thread{'s' if len(items) != 1 else ''} across the wiki, aggregated at render time._",
            "",
        ]
    )
    for source, heading in (("for-you", "From For You day-sections"), ("concept", "From concept pages")):
        group = [item for item in items if item["source"] == source]
        if not group:
            continue
        lines.extend([f"## {heading}", ""])
        for item in sorted(group, key=lambda value: value["title"].lower()):
            lines.extend([f"### [{item['title']}]({item['url']})", "", item["body"], ""])
    lines.extend(["## See also", "", "- [Topics](./topics)", "- [Projects](./projects)", ""])
    return "\n".join(lines)


def resolve_bracket_tree(*, src: Path, dst: Path, concept_dir: Path, web_base: str) -> dict[str, Any]:
    src = src.resolve()
    dst = dst.resolve()
    concept_dir = concept_dir.resolve()
    ensure_non_overlapping_tree("dst", dst, "src", src)
    ensure_non_overlapping_tree("dst", dst, "concept_dir", concept_dir)
    slugs, aliases, external = concept_indexes(concept_dir)
    backlinks: dict[str, set[str]] = {}
    internal_link_count = 0
    if dst.exists():
        shutil.rmtree(dst)
    dst.mkdir(parents=True, exist_ok=True)

    for path in src.rglob("*"):
        rel = path.relative_to(src)
        if any(part.startswith(".") or part in {"__pycache__", "node_modules"} for part in rel.parts):
            continue
        target = dst / rel
        if path.is_dir():
            target.mkdir(parents=True, exist_ok=True)
            continue
        target.parent.mkdir(parents=True, exist_ok=True)
        if path.suffix == ".md":
            source_label = derive_source_label(rel)
            record = backlinks if source_label else {}
            rendered = render_brackets(
                path.read_text(encoding="utf-8"),
                slugs=slugs,
                aliases=aliases,
                external=external,
                backlinks=record,
                source_label=source_label or "",
                web_base=web_base,
            )
            internal_link_count += rendered.count(f"]({web_base}/")
            target.write_text(rendered, encoding="utf-8")
        else:
            try:
                shutil.copy2(path, target)
            except OSError:
                continue

    backlinks_index = {slug: sorted(refs) for slug, refs in backlinks.items()}
    (dst / "_backlinks.json").write_text(json.dumps(backlinks_index, indent=2, sort_keys=True), encoding="utf-8")
    return {
        "backlinks": backlinks_index,
        "internal_link_count": internal_link_count,
        "backlink_edge_count": sum(len(v) for v in backlinks_index.values()),
    }


def ensure_non_overlapping_tree(left_label: str, left: Path, right_label: str, right: Path) -> None:
    left = left.resolve()
    right = right.resolve()
    if left == right or left.is_relative_to(right) or right.is_relative_to(left):
        raise WikiError(
            f"{left_label} path {left} must not overlap {right_label} path {right}; "
            "use a separate staging directory"
        )


def render_brackets(
    text: str,
    *,
    slugs: set[str],
    aliases: dict[str, str],
    external: dict[str, str],
    backlinks: dict[str, set[str]],
    source_label: str,
    web_base: str,
) -> str:
    def transform(segment: str) -> str:
        def replace(match: re.Match[str]) -> str:
            subject = match.group(1).strip()
            display = (match.group(2) or subject).strip()
            slug = slugify(subject)
            if not slug:
                return display
            if slug in slugs:
                backlinks.setdefault(slug, set()).add(source_label)
                return f"[{display}]({web_base}/{slug})"
            if slug in aliases:
                canonical = aliases[slug]
                backlinks.setdefault(canonical, set()).add(source_label)
                return f"[{display}]({web_base}/{canonical})"
            if slug in external:
                return f"[{display}]({external[slug]})"
            if slug in EXTERNAL_FALLBACKS:
                return f"[{display}]({EXTERNAL_FALLBACKS[slug]})"
            return display

        return BRACKET_RE.sub(replace, segment)

    pieces: list[str] = []
    cursor = 0
    for fenced in FENCED_RE.finditer(text):
        pieces.append(transform_preserving_inline_code(text[cursor : fenced.start()], transform))
        pieces.append(fenced.group(0))
        cursor = fenced.end()
    pieces.append(transform_preserving_inline_code(text[cursor:], transform))
    return "".join(pieces)


def transform_preserving_inline_code(text: str, transform) -> str:
    pieces: list[str] = []
    cursor = 0
    for inline in BACKTICK_RE.finditer(text):
        pieces.append(transform(text[cursor : inline.start()]))
        pieces.append(inline.group(0))
        cursor = inline.end()
    pieces.append(transform(text[cursor:]))
    return "".join(pieces)


def derive_source_label(rel: Path) -> str | None:
    parts = rel.parts
    if not parts or parts[0] in {"lab", "prompts", "rendered", "screenshots", "images"}:
        return None
    if len(parts) == 1:
        stem = rel.stem
        if stem.startswith("_"):
            return None
        if re.match(r"^\d{4}-\d{2}-\d{2}$", stem):
            return f"for-you-{stem}"
        if re.match(r"^\d{4}-\d{2}-\d{2}\.", stem):
            return f"for-you-{stem}"
        return stem
    folder = parts[0]
    if folder.endswith(".talk"):
        if parts[-1].startswith("_"):
            return None
        if re.match(r"^\d{4}-\d{2}-\d{2}\.", folder):
            return f"for-you-{folder}"
        return folder
    return None


def stage_concept_pages(
    *,
    concept_dir: Path,
    staging: Path,
    backlinks: dict[str, list[str]],
    web_base: str,
) -> tuple[Path, int]:
    concept_staging = staging / "concept"
    if concept_staging.exists():
        shutil.rmtree(concept_staging)
    concept_staging.mkdir(parents=True, exist_ok=True)

    slugs, aliases, external = concept_indexes(concept_dir)
    staged = 0
    for concept in collect_concepts(concept_dir):
        body = strip_frontmatter(concept.path.read_text(encoding="utf-8")).rstrip()
        body = render_brackets(
            body,
            slugs=slugs,
            aliases=aliases,
            external=external,
            backlinks={},
            source_label="concept-internal",
            web_base=web_base,
        )
        backlink_section = render_backlinks_section(concept.slug, backlinks.get(concept.slug, []), web_base=web_base)
        wrapped = "\n".join(
            wiki_frontmatter(
                title=concept.display,
                slug=f"concept/{concept.slug}",
                summary=f"Concept page about {concept.display}.",
                tags=["concept", concept.slug],
                web_base=web_base,
                today=date.today(),
            )
        )
        text = wrapped + "\n" + body + "\n" + backlink_section
        (concept_staging / f"{concept.slug}.md").write_text(text, encoding="utf-8")
        staged += 1
    return concept_staging, staged


def render_backlinks_section(slug: str, refs: list[str], *, web_base: str, recent_threshold: int = 12) -> str:
    if not refs:
        return ""
    article_refs = [ref for ref in refs if not ref.startswith("concept/") and ".talk" not in ref]
    concept_refs = [ref for ref in refs if ref.startswith("concept/")]
    talk_refs = [ref for ref in refs if ".talk" in ref]
    article_refs.sort(key=lambda item: (_date_in_label(item), item), reverse=True)
    concept_refs.sort()
    talk_refs.sort(key=lambda item: (_date_in_label(item), item), reverse=True)
    lines = [
        "",
        "### What links here",
        "",
        f"_{len(refs)} page{'s' if len(refs) != 1 else ''} link to this concept across the wiki._",
        "",
    ]

    def emit(refs_to_emit: list[str]) -> None:
        for ref in refs_to_emit:
            lines.append(f"- [{label_to_display(ref)}]({web_base}/{ref})")

    if article_refs:
        emit(article_refs[:recent_threshold])
        rest = article_refs[recent_threshold:]
        if rest:
            lines.extend(["", f"<details><summary>Earlier references ({len(rest)})</summary>", ""])
            emit(rest)
            lines.extend(["", "</details>"])
    if concept_refs:
        lines.extend(["", "**Other concept pages:**", ""])
        emit(concept_refs)
    if talk_refs:
        lines.extend(["", f"**Talk pages:** {len(talk_refs)} entries reference this concept."])
    lines.append("")
    return "\n".join(lines)


def _date_in_label(label: str) -> str:
    match = re.search(r"\d{4}-\d{2}-\d{2}", label)
    return match.group(0) if match else "0000-00-00"


def label_to_display(label: str) -> str:
    if label == "your-context":
        return "Your Context"
    if label in {"topics", "projects", "open-questions", "this-week", "index"}:
        return label.replace("-", " ").title()
    match = re.match(r"^for-you-(\d{4})-(\d{2})-(\d{2})(\.[\w.]+)?$", label)
    if match:
        _year, month, day, suffix = match.groups()
        extra = " (talk)" if suffix and "talk" in suffix else ""
        return f"For You - {int(month)}/{int(day)}{extra}"
    if label.startswith("concept/"):
        return f"{label.removeprefix('concept/')} (concept)"
    return label


def render_landing_page(
    *,
    workspace: Path,
    concept_dir: Path,
    backlinks: dict[str, list[str]],
    open_question_count: int,
    web_base: str,
    today: date,
) -> str:
    concepts = collect_concepts(concept_dir)
    by_slug = {concept.slug: concept for concept in concepts}
    top = sorted(backlinks.items(), key=lambda item: (-len(item[1]), item[0]))[:6]
    latest = latest_for_you_era(workspace)
    lines = wiki_frontmatter(
        title="1Context",
        slug="index",
        summary="Generated front door for the operator context wiki.",
        tags=["landing", "generated"],
        web_base=web_base,
        today=today,
        toc_enabled=False,
    )
    lines.extend(
        [
            "# 1Context",
            "",
            "_Cross-agent memory layer populated from real session events, with a navigable concept graph, weekly narratives, and a generated worklist._",
            "",
            "## Start here",
            "",
        ]
    )
    if latest:
        lines.append(f"- **[For You - This week](./for-you-{latest})** - the latest weekly operator narrative.")
    lines.extend(
        [
            "- **[Your Context](./your-context)** - durable profile of how the operator works.",
            f"- **[Open Questions](./open-questions)** - _{open_question_count} open thread{'s' if open_question_count != 1 else ''} across the wiki._",
            "- **[This Week](./this-week)** - generated digest of recent wiki motion.",
            "",
            "## The Wiki At A Glance",
            "",
            f"- **{len(concepts)} concept pages** in the named-subject layer.",
            f"- **{sum(len(v) for v in backlinks.values())} inbound concept references** visible through backlinks.",
            "- **[Topics](./topics)** groups concepts by category.",
            "- **[Projects](./projects)** groups project concepts by status.",
            "",
            "## Most-Cited Concepts",
            "",
        ]
    )
    for slug, refs in top:
        concept = by_slug.get(slug)
        if not concept or not refs:
            continue
        lede = f" {concept.lede}" if concept.lede else ""
        lines.append(f"- **[{concept.display}](/{slug})** - _{len(refs)} inbound._{lede}")
    lines.extend(
        [
            "",
            "## What This Is",
            "",
            "The wiki uses Wikipedia-style collaboration surfaces: talk folders, concept pages, generated indexes, redirects/aliases, external references, backlinks, redaction tiers, and open-question worklists. Agentic parts write evidence and proposals; deterministic parts keep the graph coherent.",
            "",
        ]
    )
    return "\n".join(lines)


def latest_for_you_era(workspace: Path) -> str | None:
    eras = sorted(
        path.stem
        for path in workspace.glob("*.md")
        if re.match(r"^\d{4}-\d{2}-\d{2}$", path.stem)
    )
    return eras[-1] if eras else None


def render_this_week_digest(
    *,
    workspace: Path,
    concept_dir: Path,
    backlinks: dict[str, list[str]],
    web_base: str,
    today: date,
    window_days: int = 7,
) -> str:
    window_start = today - timedelta(days=window_days)
    promoted, deferred = collect_librarian_decisions(workspace, concept_dir, window_start)
    activity = collect_talk_activity(workspace, window_start)
    latest = latest_for_you_era(workspace)
    biography = biography_first_paragraph(workspace / f"{latest}.md") if latest else None
    open_count = count_open_questions(workspace / "open-questions.md")
    lines = wiki_frontmatter(
        title="This week on 1Context",
        slug="this-week",
        summary="Generated digest of recent wiki activity.",
        tags=["this-week", "recent-changes", "generated"],
        web_base=web_base,
        today=today,
    )
    lines.extend(
        [
            "# This week on 1Context",
            "",
            f"_Generated digest covering {window_start.isoformat()} to {today.isoformat()}._",
            "",
        ]
    )
    if biography:
        lines.extend(["## The Week In The Biographer's Voice", "", f"> {biography}", ""])
    lines.extend(["## Promoted Concepts", ""])
    if promoted:
        for item in promoted:
            lines.append(f"- **[{item['name']}](/{item['slug']})** - {item['lede']}")
    else:
        lines.append("_None recorded in the current window._")
    if deferred:
        lines.extend(["", "## Deferred Concepts", ""])
        for item in deferred:
            lines.append(f"- `{item['slug']}`")
    decisions = activity["counts"].get("decided", 0) + activity["counts"].get("deferred", 0)
    lines.extend(
        [
            "",
            f"## Decision Velocity ({decisions})",
            "",
            f"- `{activity['counts'].get('decided', 0)}` decided entries.",
            f"- `{activity['counts'].get('deferred', 0)}` deferred entries.",
            f"- `{activity['counts'].get('contradiction', 0)}` contradictions surfaced.",
            f"- `{activity['counts'].get('concern', 0)}` concerns flagged.",
            "",
            "## Open Questions Worklist",
            "",
            f"- Currently **{open_count} open thread{'s' if open_count != 1 else ''}** across the wiki.",
            "",
            "## Graph Motion",
            "",
            f"- **{sum(len(refs) for refs in backlinks.values())}** concept backlink edges currently visible.",
            "",
        ]
    )
    return "\n".join(lines)


def collect_librarian_decisions(
    workspace: Path,
    concept_dir: Path,
    window_start: date,
) -> tuple[list[dict[str, str]], list[dict[str, str]]]:
    concepts = {concept.slug: concept for concept in collect_concepts(concept_dir)}
    promoted: list[dict[str, str]] = []
    deferred: list[dict[str, str]] = []
    seen: set[tuple[str, str]] = set()
    for path in talk_entry_files(workspace):
        entry_date = date_from_filename(path)
        if entry_date is None or entry_date < window_start:
            continue
        verdict = None
        slug = None
        if ".decided.concept-" in path.stem:
            verdict = "promoted"
            slug = path.stem.split(".decided.concept-", 1)[1]
        elif ".deferred.concept-" in path.stem:
            verdict = "deferred"
            slug = path.stem.split(".deferred.concept-", 1)[1]
        if not verdict or not slug or (verdict, slug) in seen:
            continue
        seen.add((verdict, slug))
        concept = concepts.get(slug)
        item = {
            "slug": slug,
            "name": concept.display if concept else slug,
            "lede": concept.lede if concept else "",
            "date": entry_date.isoformat(),
        }
        if verdict == "promoted":
            promoted.append(item)
        else:
            deferred.append(item)
    promoted.sort(key=lambda item: item["date"], reverse=True)
    deferred.sort(key=lambda item: item["date"], reverse=True)
    return promoted, deferred


def collect_talk_activity(workspace: Path, window_start: date) -> dict[str, Any]:
    kinds = ["decided", "deferred", "concern", "contradiction", "redacted", "fading", "synthesis", "proposal"]
    counts = {kind: 0 for kind in kinds}
    for path in talk_entry_files(workspace):
        entry_date = date_from_filename(path)
        if entry_date is None or entry_date < window_start:
            continue
        for kind in kinds:
            if f".{kind}." in path.name or path.name.endswith(f".{kind}.md"):
                counts[kind] += 1
                break
    return {"counts": counts}


def talk_entry_files(workspace: Path) -> list[Path]:
    files: list[Path] = []
    for folder in workspace.glob("*.talk"):
        if folder.is_dir():
            files.extend(path for path in folder.glob("*.md") if not path.name.startswith("_"))
    for folder in workspace.glob("*.private.talk"):
        if folder.is_dir():
            files.extend(path for path in folder.glob("*.md") if not path.name.startswith("_"))
    for folder in workspace.glob("*.internal.talk"):
        if folder.is_dir():
            files.extend(path for path in folder.glob("*.md") if not path.name.startswith("_"))
    return sorted(set(files))


def date_from_filename(path: Path) -> date | None:
    match = re.match(r"^(\d{4})-(\d{2})-(\d{2})", path.name)
    if not match:
        return None
    return date(int(match.group(1)), int(match.group(2)), int(match.group(3)))


def biography_first_paragraph(article: Path) -> str | None:
    if not article.exists():
        return None
    text = article.read_text(encoding="utf-8")
    match = re.search(r'<!-- section:[^>]*?"biography"[^>]*-->', text)
    if not match:
        return None
    rest = text[match.end() :]
    next_section = re.search(r"<!-- section:", rest)
    block = rest[: next_section.start()] if next_section else rest
    paragraphs = [paragraph.strip() for paragraph in re.split(r"\n\n+", block) if paragraph.strip()]
    for paragraph in paragraphs:
        if not paragraph.startswith(("##", "<!--")):
            return paragraph
    return None


def count_open_questions(path: Path) -> int:
    if not path.exists():
        return 0
    match = re.search(r"^_(\d+)\s+open\s+thread", path.read_text(encoding="utf-8"), re.MULTILINE | re.IGNORECASE)
    return int(match.group(1)) if match else 0


def brackify_text(text: str, concepts: list[ConceptPage], *, bracket_all: bool = False) -> tuple[str, int, set[str]]:
    names = sorted([(concept.display, concept.slug) for concept in concepts], key=lambda item: -len(item[0]))
    protected = protected_ranges(text)
    parts: list[str] = []
    cursor = 0
    count = 0
    seen: set[str] = set()
    for start, end in protected:
        if cursor < start:
            chunk, added, chunk_seen = brackify_segment(text[cursor:start], names, bracket_all=bracket_all)
            parts.append(chunk)
            count += added
            seen.update(chunk_seen)
        parts.append(text[start:end])
        cursor = end
    if cursor < len(text):
        chunk, added, chunk_seen = brackify_segment(text[cursor:], names, bracket_all=bracket_all)
        parts.append(chunk)
        count += added
        seen.update(chunk_seen)
    return "".join(parts), count, seen


def protected_ranges(text: str) -> list[tuple[int, int]]:
    ranges: list[tuple[int, int]] = []
    for pattern in (FENCED_RE, BACKTICK_RE, HTML_COMMENT_RE, MD_LINK_RE, EXISTING_BRACKETS_RE):
        ranges.extend((match.start(), match.end()) for match in pattern.finditer(text))
    match = FRONTMATTER_RE.match(text)
    if match:
        ranges.append((match.start(), match.end()))
    ranges.sort()
    merged: list[tuple[int, int]] = []
    for start, end in ranges:
        if merged and start <= merged[-1][1]:
            merged[-1] = (merged[-1][0], max(merged[-1][1], end))
        else:
            merged.append((start, end))
    return merged


def brackify_segment(
    text: str,
    concepts: list[tuple[str, str]],
    *,
    bracket_all: bool,
) -> tuple[str, int, set[str]]:
    paragraphs = text.split("\n\n")
    total = 0
    seen_total: set[str] = set()
    rendered: list[str] = []
    for paragraph in paragraphs:
        seen_para: set[str] = set()
        for display, _slug in concepts:
            pattern = re.compile(r"(?<![\w-])" + re.escape(display) + r"(?![\w-])")

            def replace(match: re.Match[str]) -> str:
                nonlocal total
                if not bracket_all and display in seen_para:
                    return match.group(0)
                seen_para.add(display)
                seen_total.add(display)
                total += 1
                return f"[[{match.group(0)}]]"

            paragraph = pattern.sub(replace, paragraph)
        rendered.append(paragraph)
    return "\n\n".join(rendered), total, seen_total


def wiki_frontmatter(
    *,
    title: str,
    slug: str,
    summary: str,
    tags: list[str],
    web_base: str,
    today: date,
    toc_enabled: bool = True,
) -> list[str]:
    tag_text = ", ".join(tags)
    return [
        "---",
        f"title: {title}",
        f"slug: {slug}",
        "section: product",
        "access: public",
        f"summary: {summary}",
        "status: draft",
        f"asset_base: {web_base}/assets",
        f"home_href: {web_base}/",
        "md_url: ./index.md",
        f"toc_enabled: {'true' if toc_enabled else 'false'}",
        "talk_enabled: false",
        "agent_view_enabled: true",
        "copy_buttons_enabled: true",
        "footer_enabled: true",
        f"tags: [{tag_text}]",
        f"last_updated: {today.isoformat()}",
        "---",
        "",
    ]
