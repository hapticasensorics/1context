from __future__ import annotations

import json
import re
import subprocess
import time
from collections import Counter, defaultdict
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Iterable

from onectx.config import MemorySystem
from onectx.io_utils import atomic_write_json, atomic_write_text
from onectx.storage import stable_id, utc_now


SOURCE_KINDS = {"user", "assistant"}
SOURCE_NAMES = {"codex", "claude"}
PAGE_CHANGE_SUMMARY_KEYS = ("added", "updated", "removed", "merged", "left_unchanged")
WIKI_PAGE_DEFINITIONS = (
    ("for-you", "For You"),
    ("your-context", "Your Context"),
    ("projects", "Projects"),
    ("topics", "Topics"),
)
BASE_WIKI_PAGE_IDS = frozenset(page_id for page_id, _ in WIKI_PAGE_DEFINITIONS)
MAX_SNIPPET_CHARS = 260
STOPWORDS = {
    "about",
    "after",
    "agent",
    "agents",
    "also",
    "because",
    "before",
    "being",
    "build",
    "button",
    "check",
    "code",
    "context",
    "could",
    "current",
    "doing",
    "done",
    "from",
    "great",
    "have",
    "into",
    "just",
    "local",
    "make",
    "need",
    "right",
    "should",
    "system",
    "that",
    "this",
    "through",
    "update",
    "want",
    "what",
    "when",
    "where",
    "wiki",
    "with",
    "work",
    "working",
}


@dataclass(frozen=True)
class WikiPriorPage:
    page_id: str
    source_path: Path
    body_markdown: str

    def to_payload(self, *, root: Path | None = None) -> dict[str, Any]:
        return {
            "page_id": self.page_id,
            "source_path": format_path(self.source_path, root),
            "body_sha256": stable_id("wiki-prior-page-body", self.body_markdown),
            "bytes": len(self.body_markdown.encode("utf-8")),
        }


@dataclass(frozen=True)
class WikiPageDraft:
    page_id: str
    title: str
    body_markdown: str
    draft_path: Path
    evidence_event_ids: tuple[str, ...]

    def to_payload(self, *, root: Path | None = None) -> dict[str, Any]:
        return {
            "page_id": self.page_id,
            "title": self.title,
            "draft_path": format_path(self.draft_path, root),
            "body_sha256": stable_id("wiki-page-body", self.body_markdown),
            "bytes": len(self.body_markdown.encode("utf-8")),
            "evidence_event_count": len(self.evidence_event_ids),
            "evidence_event_ids": list(self.evidence_event_ids[:12]),
        }


@dataclass(frozen=True)
class TopicPageCandidate:
    page_id: str
    title: str
    category: str
    summary: str
    evidence: tuple[str, ...]
    score: int


@dataclass(frozen=True)
class WikiPageWrite:
    page_id: str
    status: str
    receipt: dict[str, Any] | None = None
    error: str = ""

    def to_payload(self) -> dict[str, Any]:
        payload: dict[str, Any] = {"page_id": self.page_id, "status": self.status}
        if self.receipt is not None:
            payload["receipt"] = self.receipt
        if self.error:
            payload["error"] = self.error
        return payload


@dataclass(frozen=True)
class WikiSynthesisResult:
    status: str
    path: Path
    drafts: tuple[WikiPageDraft, ...]
    writes: tuple[WikiPageWrite, ...]
    source_event_count: int
    source_session_count: int
    agent_report_count: int = 0
    source_store: str = "perception_db"
    source_status: str = ""
    window_days: int = 30
    source_cursor_name: str = ""
    prior_pages: tuple[WikiPriorPage, ...] = ()

    def to_payload(self, *, root: Path | None = None) -> dict[str, Any]:
        return {
            "status": self.status,
            "path": format_path(self.path, root),
            "source_event_count": self.source_event_count,
            "source_session_count": self.source_session_count,
            "agent_report_count": self.agent_report_count,
            "source_store": self.source_store,
            "source_status": self.source_status,
            "window_days": self.window_days,
            "source_cursor_name": self.source_cursor_name,
            "prior_page_count": len(self.prior_pages),
            "prior_pages": [page.to_payload(root=root) for page in self.prior_pages],
            "drafts": [draft.to_payload(root=root) for draft in self.drafts],
            "writes": [write.to_payload() for write in self.writes],
        }


def synthesize_and_write_wiki(
    system: MemorySystem,
    *,
    run_id: str,
    output_dir: Path,
    runtime_root: Path | None = None,
    wiki_core_bin: Path | None = None,
    timeout_seconds: int = 60,
    source_events: Iterable[dict[str, Any]] = (),
    source_sessions: Iterable[dict[str, Any]] = (),
    source_store: str = "perception_db",
    source_status: str = "",
    window_days: int = 30,
    source_cursor_name: str = "",
    prior_pages: Iterable[WikiPriorPage] | None = None,
    agent_reports: Iterable[dict[str, Any]] = (),
) -> WikiSynthesisResult:
    source_events = meaningful_events(source_events, limit=2_400)
    source_sessions = meaningful_sessions(source_sessions, limit=160)
    resolved_agent_reports = meaningful_agent_reports(agent_reports, limit=80)
    resolved_prior_pages = tuple(prior_pages) if prior_pages is not None else load_prior_wiki_pages(runtime_root)
    drafts_dir = output_dir / "page-drafts"
    drafts = build_page_drafts(
        system,
        run_id=run_id,
        drafts_dir=drafts_dir,
        events=source_events,
        sessions=source_sessions,
        prior_pages=resolved_prior_pages,
        source_store=source_store,
        source_status=source_status,
        source_cursor_name=source_cursor_name,
        window_days=window_days,
        agent_reports=resolved_agent_reports,
    )
    for draft in drafts:
        atomic_write_text(draft.draft_path, draft.body_markdown)

    writes: tuple[WikiPageWrite, ...] = ()
    if runtime_root and wiki_core_bin:
        writes = tuple(
            write_page_drafts(
                drafts,
                runtime_root=runtime_root,
                wiki_core_bin=wiki_core_bin,
                timeout_seconds=timeout_seconds,
            )
        )
    elif runtime_root or wiki_core_bin:
        writes = tuple(
            WikiPageWrite(
                page_id=draft.page_id,
                status="skipped",
                error="runtime_root and wiki_core_bin are both required to write live wiki pages",
            )
            for draft in drafts
        )

    result = WikiSynthesisResult(
        status=write_status(writes),
        path=output_dir / "wiki-synthesis.json",
        drafts=tuple(drafts),
        writes=writes,
        source_event_count=len(source_events),
        source_session_count=len(source_sessions),
        agent_report_count=len(resolved_agent_reports),
        source_store=source_store,
        source_status=source_status,
        window_days=window_days,
        source_cursor_name=source_cursor_name,
        prior_pages=resolved_prior_pages,
    )
    atomic_write_json(result.path, result.to_payload(root=system.root))
    return result


def meaningful_events(rows: Iterable[dict[str, Any]], *, limit: int = 360) -> list[dict[str, Any]]:
    rows = sorted(list(rows), key=event_sort_key)
    result: list[dict[str, Any]] = []
    seen: set[tuple[str, str, str]] = set()
    for row in reversed(rows):
        source = str(row.get("source") or "")
        kind = str(row.get("kind") or "")
        text = clean_text(row.get("text"))
        if source not in SOURCE_NAMES or kind not in SOURCE_KINDS or len(text) < 24:
            continue
        key = (source, str(row.get("session_id") or ""), text[:500])
        if key in seen:
            continue
        seen.add(key)
        result.append({**row, "text": text})
        if len(result) >= limit:
            break
    return list(reversed(result))


def meaningful_sessions(rows: Iterable[dict[str, Any]], *, limit: int = 40) -> list[dict[str, Any]]:
    rows = [
        row
        for row in rows
        if str(row.get("source") or "") in SOURCE_NAMES and int(row.get("event_count") or 0) > 0
    ]
    return sorted(rows, key=lambda row: str(row.get("last_ts") or ""))[-limit:]


def meaningful_agent_reports(rows: Iterable[dict[str, Any]], *, limit: int = 80) -> list[dict[str, Any]]:
    reports: list[dict[str, Any]] = []
    for row in rows:
        raw_report = str(row.get("report") or "")
        report = clean_text(raw_report)
        if not report:
            continue
        reports.append(
            {
                "job_id": str(row.get("job_id") or ""),
                "phase": str(row.get("phase") or ""),
                "run_id": str(row.get("run_id") or ""),
                "page_id": str(row.get("page_id") or "for-you"),
                "status": str(row.get("status") or ""),
                "report": report,
                "artifact_path": str(row.get("artifact_path") or ""),
                "artifact_body": str(row.get("artifact_body") or ""),
                "page_change_summary": extract_page_change_summary(raw_report),
                "talk_status": str(row.get("talk_status") or ""),
                "talk_delivery_mode": str(row.get("talk_delivery_mode") or ""),
            }
        )
        if len(reports) >= limit:
            break
    return reports


def build_page_drafts(
    system: MemorySystem,
    *,
    run_id: str,
    drafts_dir: Path,
    events: list[dict[str, Any]],
    sessions: list[dict[str, Any]],
    prior_pages: tuple[WikiPriorPage, ...] = (),
    source_store: str = "perception_db",
    source_status: str = "",
    source_cursor_name: str = "",
    window_days: int = 30,
    agent_reports: list[dict[str, Any]] | None = None,
) -> list[WikiPageDraft]:
    now = utc_now()
    context = build_synthesis_context(
        events,
        sessions,
        prior_pages=prior_pages,
        source_store=source_store,
        source_status=source_status,
        source_cursor_name=source_cursor_name,
        window_days=window_days,
        agent_reports=agent_reports or [],
        now=now,
    )
    pages = [
        ("for-you", "For You", render_for_you(context)),
        ("your-context", "Your Context", render_your_context(context)),
        ("projects", "Projects", render_projects(context)),
        ("topics", "Topics", render_topics(context)),
    ]
    for candidate in context.get("topic_candidates", []):
        pages.append((candidate.page_id, candidate.title, render_topic_page(context, candidate)))
    evidence_ids = tuple(str(event.get("event_id") or "") for event in events[-120:] if event.get("event_id"))
    drafts: list[WikiPageDraft] = []
    for page_id, title, body in pages:
        drafts.append(
            WikiPageDraft(
                page_id=page_id,
                title=title,
                body_markdown=body.rstrip() + "\n",
                draft_path=drafts_dir / f"{page_id}.md",
                evidence_event_ids=evidence_ids,
            )
        )
    return drafts


def build_synthesis_context(
    events: list[dict[str, Any]],
    sessions: list[dict[str, Any]],
    *,
    prior_pages: tuple[WikiPriorPage, ...] = (),
    source_store: str = "perception_db",
    source_status: str = "",
    source_cursor_name: str = "",
    window_days: int = 30,
    agent_reports: list[dict[str, Any]] | None = None,
    now: str,
) -> dict[str, Any]:
    by_session: dict[str, list[dict[str, Any]]] = defaultdict(list)
    by_project: dict[str, list[dict[str, Any]]] = defaultdict(list)
    for event in events:
        by_session[str(event.get("session_id") or "")].append(event)
        by_project[event_project_label(event)].append(event)

    recent_sessions = []
    for session in sorted(sessions, key=lambda row: str(row.get("last_ts") or ""), reverse=True)[:12]:
        session_id = str(session.get("session_id") or "")
        event_rows = by_session.get(session_id, [])
        if not event_rows:
            continue
        recent_sessions.append(
            {
                "session_id": session_id,
                "source": str(session.get("source") or ""),
                "cwd": str(session.get("cwd") or ""),
                "project": event_project_label(session),
                "first_ts": str(session.get("first_ts") or ""),
                "last_ts": str(session.get("last_ts") or ""),
                "event_count": int(session.get("event_count") or 0),
                "user_samples": [event for event in event_rows if str(event.get("kind") or "") == "user"][-3:],
                "assistant_samples": [event for event in event_rows if str(event.get("kind") or "") == "assistant"][-3:],
            }
        )

    project_rows = []
    for project, project_events in sorted(by_project.items(), key=lambda item: len(item[1]), reverse=True):
        if not project or project == "unknown":
            continue
        project_rows.append(
            {
                "project": project,
                "event_count": len(project_events),
                "last_ts": str(project_events[-1].get("ts") or ""),
                "cwd": str(project_events[-1].get("cwd") or ""),
                "samples": project_events[-5:],
            }
        )

    agent_reports = agent_reports or []
    open_questions = extract_questions(events)
    terms = top_terms(events)
    topic_candidates = build_topic_page_candidates(events, prior_pages, agent_reports, terms)
    return {
        "now": now,
        "events": events,
        "sessions": sessions,
        "recent_sessions": recent_sessions,
        "projects": project_rows[:10],
        "open_questions": open_questions,
        "terms": terms,
        "topic_candidates": topic_candidates,
        "prior_pages": {page.page_id: page for page in prior_pages},
        "prior_summaries": {page.page_id: prior_page_summary(page.body_markdown) for page in prior_pages},
        "agent_reports_by_page": group_agent_reports_by_page(agent_reports or []),
        "agent_reports": agent_reports,
        "source": {
            "store": source_store,
            "status": source_status,
            "cursor_name": source_cursor_name,
            "window_days": window_days,
        },
        "counts": {
            "source_events": len(events),
            "source_sessions": len(sessions),
            "projects": len(project_rows),
            "prior_pages": len(prior_pages),
            "agent_reports": len(agent_reports or []),
        },
    }


def render_for_you(context: dict[str, Any]) -> str:
    now = context["now"]
    sessions = context["recent_sessions"]
    projects = context["projects"]
    questions = context["open_questions"]
    if not sessions:
        current = [
            "The wiki update system is installed and ready, but Perception DB did not return enough recent session evidence to summarize the operator safely.",
            "Use **Update Wiki** again after Codex and Claude transcript import has fresh rows; the page will replace this bootstrap note with sourced orientation.",
        ]
    else:
        lead = sessions[0]
        current = [
            f"The freshest imported work is in **{lead['project']}**, last seen at `{lead['last_ts']}` from `{lead['source']}`.",
            "The current memory pass is prioritizing Perception DB source import, agent-role orchestration, and turning the rendered wiki from template scaffolding into evidence-backed pages.",
        ]
    return "\n".join(
        [
            "# For You",
            "",
            *update_status_section(context),
            *agent_company_section(context, "for-you"),
            *agent_change_ledger_section(context, "for-you"),
            "## Current Orientation",
            "",
            *paragraphs(current),
            "",
            *agent_daily_memory_section(context),
            "## Imported Sessions",
            "",
            *bullets([session_detail(session) for session in sessions[:10]] or ["No imported sessions were available for this pass."]),
            "",
            "## Recent Signals",
            "",
            *bullets([session_signal(session) for session in sessions[:8]] or ["No recent source-backed session signals were imported yet."]),
            "",
            "## Evidence Samples",
            "",
            *bullets([evidence_sample(event) for event in context["events"][-12:]] or ["No evidence samples were imported yet."]),
            "",
            "## Open Loops",
            "",
            *bullets([question_summary(item) for item in questions[:8]] or ["No explicit open questions were detected in the imported recent session text."]),
            "",
            *prior_continuity_section(context, "for-you"),
            "## Useful Context",
            "",
            *bullets(
                [
                    f"Generated by `memory.update-wiki` at `{now}` from {context['counts']['source_events']} Perception DB session events.",
                    "The update path now recreates the hired-agent roster: scribes, daily editor, source-packet roles, curators, biographer, historian, librarian, contradiction flagger, and redactor.",
                    "See [Your Context](./your-context) for durable collaboration guidance, [Projects](./projects) for active work, and [Topics](./topics) for recurring subjects.",
                ]
            ),
            "",
            "## See Also",
            "",
            "- [Your Context](./your-context)",
            "- [Projects](./projects)",
            "- [Topics](./topics)",
        ]
    )


def render_your_context(context: dict[str, Any]) -> str:
    projects = context["projects"]
    questions = context["open_questions"]
    working_samples = [
        "He likes work to become a closed loop: implement, run deterministic checks, install or launch the app when needed, then verify the actual visible surface.",
        "He is comfortable moving fast when the goal is a working demo, but he repeatedly asks for branch, folder, and build state to be made explicit before reconciliation work proceeds.",
        "He treats the wiki as a real operating surface for agents and humans, not a decorative status page.",
    ]
    project_names = ", ".join(project["project"] for project in projects[:5]) or "recent imported projects"
    return "\n".join(
        [
            "# Your Context",
            "",
            *update_status_section(context),
            *agent_company_section(context, "your-context"),
            *agent_change_ledger_section(context, "your-context"),
            "## Working Style",
            "",
            *paragraphs(working_samples),
            "",
            "## Coding Style",
            "",
            *paragraphs(
                [
                    "Prefer small, testable changes that fit the repo's existing patterns. When a feature crosses app/runtime boundaries, wire the narrow bridge and prove it from the installed build.",
                    "Use repo-local tooling first: `uv` for Python, SwiftPM/Xcode build loops for macOS, and the release-train dev build when the app bundle matters.",
                ]
            ),
            "",
            "## Engineering Philosophy",
            "",
            *paragraphs(
                [
                    "The system should remember through source-backed artifacts: native transcripts, talk entries, page edits, render receipts, and build evidence.",
                    "A feature is not real until the user-facing surface reflects it. For this wiki, that means replacing template placeholders with generated, sourced page bodies.",
                ]
            ),
            "",
            "## Preferences",
            "",
            *bullets(
                [
                    "Move quickly toward a working build when the requested outcome is concrete.",
                    "Keep private/public boundaries and branch/folder ownership visible.",
                    "Prefer scheduled or button-triggered memory refreshes that can run without babysitting.",
                ]
            ),
            "",
            "## Taste",
            "",
            *paragraphs(
                [
                    "He responds to interfaces that feel like real tools rather than marketing pages: dense enough to work from, polished enough to trust, and honest about what is still missing.",
                ]
            ),
            "",
            "## Desires",
            "",
            *paragraphs(
                [
                    "He is trying to make 1Context into a practical remembering system: agents read the work, write durable wiki pages, and keep those pages fresh on a schedule or by an explicit update action.",
                ]
            ),
            "",
            "## Recurring Ideas",
            "",
            *bullets(
                [
                    "Wikipedia-like pages as a shared memory substrate.",
                    "Talk folders as the coordination layer between scribes, historians, librarians, and curators.",
                    "Installed app proof over repo-only proof.",
                    f"Active imported project context: {project_names}.",
                ]
            ),
            "",
            "## Habits",
            "",
            *bullets(
                [
                    "Asks for the state of branches, folders, and remotes before cleanup.",
                    "Pushes ad hoc prototypes toward a repeatable workflow once the pattern appears common.",
                    "Interrupts vague success claims and asks whether the visible system is actually filled in.",
                ]
            ),
            "",
            "## Coworkers",
            "",
            "No durable coworker summary was generated from this bounded import window. Add people only when repeated evidence makes their role useful for future work.",
            "",
            "## Infra And Tooling",
            "",
            *bullets(
                [
                    "`1Context Dev.app` is the ordinary iteration target for installed-build proof.",
                    "The active source importer reads local Codex and Claude session logs into Perception DB.",
                    "The wiki core owns page lifecycle writes and the app publish path mirrors the rendered site for local web serving.",
                ]
            ),
            "",
            "## Standing Requests",
            "",
            *bullets(
                [
                    "Use `uv` for Python work.",
                    "Use the stable dev build unless the task is specifically about fresh macOS permission prompts.",
                    "Do not leave a generated wiki page at its default template when the goal is a working memory system.",
                ]
            ),
            "",
            "## Notes For AI Agents",
            "",
            *bullets(
                [
                    "Check the live repo and installed app before reporting success.",
                    "When source import is sparse, say that clearly and avoid inventing personal facts.",
                    *[question_summary(item) for item in questions[:3]],
                ]
            ),
            "",
            "## Fresh Evidence This Pass",
            "",
            *bullets([evidence_sample(event) for event in context["events"][-10:]] or ["No fresh evidence samples were imported for this pass."]),
            "",
            *prior_continuity_section(context, "your-context"),
            "## Life Story",
            "",
            "This bounded update is not enough for a life-story rewrite. The long-arc biographer should only write here after a larger, explicitly reviewed source window.",
            "",
            "## See Also",
            "",
            "- [Projects](./projects)",
            "- [Topics](./topics)",
        ]
    )


def render_projects(context: dict[str, Any]) -> str:
    projects = context["projects"]
    active = []
    paused = []
    archived = []
    for project in projects:
        entry = project_entry(project)
        label = project["project"].casefold()
        if any(token in label for token in ("private", "guardian", "mox", "1context", "public-launch")):
            active.append(entry)
        elif project["event_count"] >= 20:
            paused.append(entry)
        else:
            archived.append(entry)
    return "\n".join(
        [
            "# Projects",
            "",
            *update_status_section(context),
            *agent_company_section(context, "projects"),
            *agent_change_ledger_section(context, "projects"),
            "## Active",
            "",
            *bullets(active[:8] or ["No active project signals were imported yet."]),
            "",
            "## Recent Project Evidence",
            "",
            *bullets([project_evidence(project) for project in projects[:10]] or ["No project evidence was imported for this pass."]),
            "",
            "## Paused Or Blocked",
            "",
            *bullets(paused[:6] or ["No paused or blocked project signals were detected in this bounded import."]),
            "",
            "## Recently Completed",
            "",
            *bullets(
                [
                    "The current 1Context update path now has a dev-build RPC surface and generated wiki page drafts; final completion depends on the installed page verification receipt.",
                ]
            ),
            "",
            "## Archived",
            "",
            *bullets(archived[:6] or ["No archival candidates were generated from the imported window."]),
            "",
            "## Cross-Project Patterns",
            "",
            *bullets(
                [
                    "Agentic workflows repeatedly need the same three artifacts: source import, role/job orchestration, and a visible page or UI surface that proves the work landed.",
                    "Polish loops work best when a helper lane has clear ownership of assets or data and the foreground lane keeps runtime/build ownership explicit.",
                ]
            ),
            "",
            *prior_continuity_section(context, "projects"),
            "## See Also",
            "",
            "- [Your Context](./your-context)",
            "- [Topics](./topics)",
            "- [For You](./for-you)",
        ]
    )


def render_topics(context: dict[str, Any]) -> str:
    terms = context["terms"]
    candidates: list[TopicPageCandidate] = list(context.get("topic_candidates", []))
    categories = {
        "Engineering": [candidate for candidate in candidates if candidate.category == "Engineering"],
        "Infrastructure": [candidate for candidate in candidates if candidate.category == "Infrastructure"],
        "Process": [candidate for candidate in candidates if candidate.category == "Process"],
        "Tools": [candidate for candidate in candidates if candidate.category == "Tools"],
        "Domain": [candidate for candidate in candidates if candidate.category == "Domain"],
        "People And Organizations": [
            candidate
            for candidate in candidates
            if candidate.category in {"People", "Organizations"}
        ],
    }
    generated_rows = [
        topic_index_bullet(candidate)
        for candidate in sorted(candidates, key=lambda item: (-item.score, item.title.casefold()))[:16]
    ]
    return "\n".join(
        [
            "# Topics",
            "",
            *update_status_section(context),
            *agent_company_section(context, "topics"),
            *agent_change_ledger_section(context, "topics"),
            "## Generated Topic Pages",
            "",
            *bullets(
                generated_rows
                or [
                    "No topic pages were promoted yet. The editor should mark recurring subjects with `[[Subject]]`, and the librarian should prune or promote them on the next run."
                ]
            ),
            "",
            "## How Topics Are Grown",
            "",
            *paragraphs(
                [
                    "Topic pages are created from editor link intent, librarian cleanup decisions, recurring Perception DB evidence, and retained prior topic pages.",
                    "The readable wiki is allowed to delete aggressively: if a newer run proves a claim stale or low-signal, the librarian should propose removal and the page curator should rewrite the page body instead of appending caveats forever.",
                ]
            ),
            "",
            "## Engineering",
            "",
            *topic_section_bullets(categories["Engineering"], terms, {"swift", "rust", "python", "state", "machine", "schema", "test", "build", "release", "runtime"}),
            "",
            "## Infrastructure",
            "",
            *topic_section_bullets(categories["Infrastructure"], terms, {"sparkle", "permission", "daemon", "launchagent", "caddy", "local", "dev", "app", "wiki", "perception", "timescale"}),
            "",
            "## Process",
            "",
            *topic_section_bullets(
                categories["Process"],
                terms,
                {"closed-loop", "branch", "reconciliation", "talk", "governance", "backfill", "scheduler", "cleanup"},
            ),
            "",
            "## Tools",
            "",
            *topic_section_bullets(categories["Tools"], terms, {"codex", "claude", "uv", "playwright", "xcode", "swiftpm", "postgres"}),
            "",
            "## Domain",
            "",
            *topic_section_bullets(categories["Domain"], terms, {"personal", "memory", "wiki", "agent", "orchestration", "scribe", "curator", "librarian"}),
            "",
            "## People",
            "",
            *topic_section_bullets([candidate for candidate in candidates if candidate.category == "People"], terms, {"person", "coworker", "user", "operator"}),
            "",
            "## Organizations",
            "",
            *topic_section_bullets([candidate for candidate in candidates if candidate.category == "Organizations"], terms, {"haptica", "mox", "openai", "anthropic", "github", "apple"}),
            "",
            "## Recent Topic Evidence",
            "",
            *bullets([evidence_sample(event) for event in context["events"][-10:]] or ["No recent topic evidence was imported for this pass."]),
            "",
            *prior_continuity_section(context, "topics"),
            "## See Also",
            "",
            "- [Your Context](./your-context)",
            "- [Projects](./projects)",
            "- [For You](./for-you)",
        ]
    )


def render_topic_page(context: dict[str, Any], candidate: TopicPageCandidate) -> str:
    related = [
        other
        for other in context.get("topic_candidates", [])
        if other.page_id != candidate.page_id and other.category == candidate.category
    ][:5]
    current_role = topic_current_role(candidate)
    evidence_rows = list(candidate.evidence[:10])
    return "\n".join(
        [
            f"# {candidate.title}",
            "",
            *update_status_section(context),
            f"Category: **{candidate.category}**.",
            "",
            "## Current Role",
            "",
            *paragraphs(
                [
                    current_role,
                    candidate.summary,
                ]
            ),
            "",
            "## Recent Evidence",
            "",
            *bullets(evidence_rows or ["No fresh evidence snippets were attached to this topic in the current run. Keep this page sparse until a scribe or editor reinforces it."]),
            "",
            "## How The Wiki Should Treat This",
            "",
            *paragraphs(
                [
                    f"The editor should link this as `[[{candidate.title}]]` only when it is doing real explanatory work in a daily section or page proposal.",
                    "The librarian should remove stale claims from this page instead of preserving every historical wording. Talk/mail receipts preserve the audit trail; this page should stay current and useful.",
                ]
            ),
            "",
            "## Open Questions",
            "",
            *bullets(topic_open_questions(context, candidate)),
            "",
            "## Related Pages",
            "",
            "- [Topics](./topics)",
            "- [For You](./for-you)",
            "- [Your Context](./your-context)",
            *[f"- [{other.title}](./{other.page_id})" for other in related],
        ]
    )


def load_prior_wiki_pages(runtime_root: Path | None, page_ids: tuple[str, ...] | None = None) -> tuple[WikiPriorPage, ...]:
    if runtime_root is None:
        return ()
    source_root = runtime_root / "user-wiki" / "source"
    if not source_root.exists():
        return ()
    pages: list[WikiPriorPage] = []
    seen: set[str] = set()
    if page_ids is not None:
        resolved_page_ids = page_ids
    else:
        resolved_page_ids = tuple(page_id for page_id, _ in WIKI_PAGE_DEFINITIONS)
    for page_id in resolved_page_ids:
        matches = sorted(source_root.glob(f"families/*/*/source/{page_id}.md"))
        if not matches:
            continue
        source_path = matches[0]
        try:
            text = source_path.read_text(encoding="utf-8")
        except OSError:
            continue
        body = strip_markdown_frontmatter(text).strip()
        if body:
            pages.append(WikiPriorPage(page_id=page_id, source_path=source_path, body_markdown=body))
            seen.add(page_id)
    if page_ids is None:
        for source_path in sorted(source_root.glob("families/*/*/source/*.md")):
            try:
                text = source_path.read_text(encoding="utf-8")
            except OSError:
                continue
            page_id = frontmatter_value(text, "page_id") or frontmatter_value(text, "id") or source_path.stem
            if not page_id or page_id in seen or not page_id.startswith("topic-"):
                continue
            body = strip_markdown_frontmatter(text).strip()
            if not body:
                continue
            pages.append(WikiPriorPage(page_id=page_id, source_path=source_path, body_markdown=body))
            seen.add(page_id)
    return tuple(pages)


def update_status_section(context: dict[str, Any]) -> list[str]:
    source = context["source"]
    counts = context["counts"]
    return [
        "## Update Status",
        "",
        *bullets(
            [
                f"Last refreshed: `{context['now']}`.",
                f"Source store: `{source['store']}`; import status: `{source['status'] or 'unknown'}`.",
                f"Imported evidence in this pass: `{counts['source_events']}` events across `{counts['source_sessions']}` sessions.",
                f"Source window: `{source['window_days']}` days; cursor: `{source['cursor_name'] or 'default'}`.",
                f"Existing wiki pages used for continuity: `{counts['prior_pages']}`.",
                "Automatic refresh is daemon-owned; the menu button triggers the same `memory.update_wiki` path on demand.",
            ]
        ),
        "",
    ]


def prior_continuity_section(context: dict[str, Any], page_id: str) -> list[str]:
    summary = context.get("prior_summaries", {}).get(page_id, [])
    if not summary:
        return []
    return [
        "## Existing Wiki Continuity",
        "",
        *bullets(summary),
        "",
    ]


def agent_company_section(context: dict[str, Any], page_id: str) -> list[str]:
    reports = context.get("agent_reports_by_page", {}).get(page_id, [])
    if not reports:
        return []
    return [
        "## Agent Company Reports",
        "",
        *bullets([agent_report_summary(report) for report in reports[:8]]),
        "",
    ]


def agent_change_ledger_section(context: dict[str, Any], page_id: str) -> list[str]:
    reports = context.get("agent_reports_by_page", {}).get(page_id, [])
    rows: list[str] = []
    for report in reports:
        summary = report.get("page_change_summary")
        if not isinstance(summary, dict):
            continue
        for key in PAGE_CHANGE_SUMMARY_KEYS:
            values = summary.get(key)
            if not values:
                continue
            for value in values[:4]:
                rows.append(agent_change_summary_bullet(report, key, str(value)))
                if len(rows) >= 12:
                    break
            if len(rows) >= 12:
                break
        if len(rows) >= 12:
            break
    if not rows:
        return []
    return [
        "## Agent Change Ledger",
        "",
        *bullets(rows),
        "",
    ]


def agent_daily_memory_section(context: dict[str, Any]) -> list[str]:
    reports = context.get("agent_reports_by_page", {}).get("for-you", [])
    entries: list[tuple[str, str]] = []
    seen_dates: set[str] = set()
    for report in reports:
        if str(report.get("job_id") or "") != "memory.daily.editor":
            continue
        artifact_body = str(report.get("artifact_body") or "")
        body = strip_markdown_frontmatter(artifact_body).strip()
        if not body:
            continue
        date = frontmatter_value(artifact_body, "target-section") or frontmatter_value(artifact_body, "target-date")
        if not date:
            artifact_path = str(report.get("artifact_path") or "")
            match = re.search(r"(\d{4}-\d{2}-\d{2})", artifact_path)
            date = match.group(1) if match else ""
        if not date or date in seen_dates:
            continue
        seen_dates.add(date)
        entries.append((date, neutralize_local_markdown_links(body)))

    if not entries:
        return []

    rows = [
        "## Daily Memory",
        "",
        "Agent-authored daily sections promoted by the publisher from completed editor artifacts.",
        "",
    ]
    for date, body in sorted(entries):
        rows.extend([f"### {date}", "", body, ""])
    return rows


def frontmatter_value(text: str, key: str) -> str:
    if not text.startswith("---"):
        return ""
    lines = text.splitlines()
    for line in lines[1:]:
        if line.strip() == "---":
            return ""
        if ":" not in line:
            continue
        name, value = line.split(":", 1)
        if name.strip() == key:
            return value.strip().strip("\"'")
    return ""


def group_agent_reports_by_page(reports: list[dict[str, Any]]) -> dict[str, list[dict[str, Any]]]:
    grouped: dict[str, list[dict[str, Any]]] = defaultdict(list)
    for report in reports:
        page_id = str(report.get("page_id") or "for-you")
        grouped[page_id].append(report)
    return dict(grouped)


def extract_page_change_summary(report: str) -> dict[str, list[str]]:
    summary: dict[str, list[str]] = {key: [] for key in PAGE_CHANGE_SUMMARY_KEYS}
    active_key = ""
    in_summary = False
    for raw_line in report.splitlines():
        line = raw_line.rstrip()
        stripped = line.strip()
        if not stripped:
            active_key = ""
            continue
        if re.match(r"^page_change_summary\s*:?\s*$", stripped, flags=re.I):
            in_summary = True
            active_key = ""
            continue
        match = re.match(
            r"^(?:[-*]\s*)?(added|updated|removed|merged|left_unchanged)\s*:\s*(.*)$",
            stripped,
            flags=re.I,
        )
        if match:
            key = match.group(1).casefold()
            value = normalize_change_summary_value(match.group(2))
            if value:
                summary[key].append(value)
            active_key = key
            continue
        if in_summary and active_key and re.match(r"^[-*]\s+", stripped):
            value = normalize_change_summary_value(re.sub(r"^[-*]\s+", "", stripped))
            if value:
                summary[active_key].append(value)
    return {key: values[:8] for key, values in summary.items() if values}


def normalize_change_summary_value(value: str) -> str:
    value = clean_text(value.strip().strip("[]"))
    if not value or value.casefold() in {"none", "n/a", "na", "null", "[]"}:
        return ""
    return neutralize_local_markdown_links(shorten(value, limit=180))


def agent_change_summary_bullet(report: dict[str, Any], key: str, value: str) -> str:
    job_id = str(report.get("job_id") or "agent")
    label = key.replace("_", " ")
    if key == "removed":
        verb = "proposed removing"
    elif key == "merged":
        verb = "proposed merging"
    elif key == "updated":
        verb = "updated"
    elif key == "added":
        verb = "added"
    else:
        verb = "left unchanged"
    return f"`{job_id}` {verb} ({label}): {value}"


def agent_report_summary(report: dict[str, Any]) -> str:
    job_id = str(report.get("job_id") or "agent")
    phase = str(report.get("phase") or "phase")
    status = str(report.get("status") or "unknown")
    talk_status = str(report.get("talk_status") or "unknown")
    body = neutralize_local_markdown_links(shorten(str(report.get("report") or ""), limit=360))
    return (
        f"`{job_id}` in `{phase}` completed with status `{status}` and talk/mail `{talk_status}`: "
        f"{body}"
    )


def neutralize_local_markdown_links(text: str) -> str:
    text = re.sub(r"\[([^\]]+)\]\((/Users/[^)\s]+)\)", r"\1 (`\2`)", text)
    text = re.sub(r"<(/Users/[^>\s]+)>", r"`\1`", text)
    return re.sub(r"(?<![`(])(/Users/[^\s)\]]+)", local_path_as_code, text)


def local_path_as_code(match: re.Match[str]) -> str:
    path = match.group(1)
    trailing = ""
    while path and path[-1] in ".,;:":
        trailing = path[-1] + trailing
        path = path[:-1]
    return f"`{path}`{trailing}"


def prior_page_summary(body_markdown: str) -> list[str]:
    body = strip_prior_continuity_section(strip_markdown_frontmatter(body_markdown))
    headings = [
        clean_text(line.lstrip("# "))
        for line in body.splitlines()
        if line.startswith("## ") and "Existing Wiki Continuity" not in line
    ]
    prior_bullets = []
    for line in body.splitlines():
        stripped = line.strip()
        if not stripped.startswith("- "):
            continue
        text = clean_text(stripped[2:])
        if text and "Generated by `memory.update-wiki`" not in text:
            prior_bullets.append(text)
        if len(prior_bullets) >= 3:
            break

    summary: list[str] = []
    if headings:
        summary.append("Existing sections: " + ", ".join(headings[:8]) + ".")
    summary.extend(shorten(item, limit=220) for item in prior_bullets)
    return summary[:4]


def strip_markdown_frontmatter(text: str) -> str:
    if not text.startswith("---"):
        return text
    lines = text.splitlines()
    for index, line in enumerate(lines[1:], start=1):
        if line.strip() == "---":
            return "\n".join(lines[index + 1 :])
    return text


def strip_prior_continuity_section(text: str) -> str:
    return re.sub(r"\n?## Existing Wiki Continuity\n.*?(?=\n## |\Z)", "\n", text, flags=re.S).strip()


def write_page_drafts(
    drafts: Iterable[WikiPageDraft],
    *,
    runtime_root: Path,
    wiki_core_bin: Path,
    timeout_seconds: int,
) -> list[WikiPageWrite]:
    drafts = tuple(drafts)
    if not wiki_core_bin.is_file():
        return [WikiPageWrite(page_id=draft.page_id, status="failed", error=f"wiki core not found: {wiki_core_bin}") for draft in drafts]

    try:
        subprocess.run(
            [str(wiki_core_bin), "--root", str(runtime_root), "page-create-all"],
            text=True,
            capture_output=True,
            check=True,
            timeout=max(5, timeout_seconds),
        )
    except subprocess.SubprocessError as exc:
        return [WikiPageWrite(page_id=draft.page_id, status="failed", error=f"page-create-all failed: {exc}") for draft in drafts]

    dynamic_create_errors = ensure_dynamic_pages(tuple(drafts), runtime_root=runtime_root, wiki_core_bin=wiki_core_bin, timeout_seconds=timeout_seconds)

    writes: list[WikiPageWrite] = []
    for draft in drafts:
        if draft.page_id in dynamic_create_errors:
            writes.append(WikiPageWrite(page_id=draft.page_id, status="failed", error=dynamic_create_errors[draft.page_id]))
            continue
        completed = subprocess.run(
            [
                str(wiki_core_bin),
                "--root",
                str(runtime_root),
                "page-write-body",
                draft.page_id,
                "--body-file",
                str(draft.draft_path),
            ],
            text=True,
            capture_output=True,
            check=False,
            timeout=max(5, timeout_seconds),
        )
        if completed.returncode == 0:
            writes.append(WikiPageWrite(page_id=draft.page_id, status="written", receipt=parse_json(completed.stdout)))
        else:
            error = (completed.stderr or completed.stdout).strip() or f"wiki core exited {completed.returncode}"
            writes.append(WikiPageWrite(page_id=draft.page_id, status="failed", error=error))
    return writes


def ensure_dynamic_pages(
    drafts: tuple[WikiPageDraft, ...],
    *,
    runtime_root: Path,
    wiki_core_bin: Path,
    timeout_seconds: int,
) -> dict[str, str]:
    errors: dict[str, str] = {}
    for draft in drafts:
        if draft.page_id in BASE_WIKI_PAGE_IDS:
            continue
        completed = subprocess.run(
            [
                str(wiki_core_bin),
                "--root",
                str(runtime_root),
                "page-create",
                draft.page_id,
                "--title",
                draft.title,
                "--type",
                "topic",
                "--nav-section",
                "hidden",
                "--family-group",
                "topics",
                "--family-group-title",
                "Topics",
                "--family-id",
                draft.page_id,
                "--family-title",
                draft.title,
                "--summary",
                f"Generated 1Context topic page for {draft.title}.",
            ],
            text=True,
            capture_output=True,
            check=False,
            timeout=max(5, timeout_seconds),
        )
        if completed.returncode == 0:
            continue
        message = (completed.stderr or completed.stdout).strip()
        if "page already exists" in message:
            continue
        errors[draft.page_id] = message or f"page-create exited {completed.returncode}"
    return errors


def render_projects_from_events(events: list[dict[str, Any]]) -> list[str]:
    grouped: dict[str, list[dict[str, Any]]] = defaultdict(list)
    for event in events:
        grouped[project_label(str(event.get("cwd") or ""))].append(event)
    return [project for project, rows in sorted(grouped.items(), key=lambda item: len(item[1]), reverse=True) if project]


def extract_questions(events: list[dict[str, Any]]) -> list[dict[str, Any]]:
    questions: list[dict[str, Any]] = []
    question_re = re.compile(r"(^|\s)(what|how|why|which|when|where|can|could|should|is|are|do|does)\b", re.I)
    for event in reversed(events):
        if str(event.get("kind") or "") != "user":
            continue
        text = clean_text(event.get("text"))
        if "?" not in text and not question_re.search(text):
            continue
        questions.append(event)
        if len(questions) >= 12:
            break
    return list(reversed(questions))


def top_terms(events: list[dict[str, Any]], *, limit: int = 80) -> list[tuple[str, int]]:
    counter: Counter[str] = Counter()
    for event in events:
        text = clean_text(event.get("text")).casefold()
        for token in re.findall(r"[a-z][a-z0-9][a-z0-9._-]{2,}", text):
            token = token.strip("._-")
            if token and token not in STOPWORDS and len(token) <= 40:
                counter[token] += 1
    return counter.most_common(limit)


TOPIC_SEEDS: dict[str, tuple[str, str]] = {
    "1context": ("1Context", "Domain"),
    "agent mail": ("Agent Mail", "Domain"),
    "agent harness": ("Agent Harness", "Infrastructure"),
    "agent orchestration": ("Agent Orchestration", "Domain"),
    "backfill": ("Backfill Scheduler", "Process"),
    "codex": ("Codex", "Tools"),
    "daemon": ("macOS Daemon", "Infrastructure"),
    "for you": ("For You", "Domain"),
    "librarian": ("Librarian", "Domain"),
    "perception db": ("Perception DB", "Infrastructure"),
    "postgres": ("Postgres", "Infrastructure"),
    "refresh wiki": ("Refresh Wiki Button", "Process"),
    "scribe": ("Scribes", "Domain"),
    "talk page": ("Talk Pages", "Domain"),
    "timescale": ("Timescale", "Infrastructure"),
    "topic page": ("Topic Pages", "Domain"),
    "wiki": ("Personal Wiki", "Domain"),
}


def build_topic_page_candidates(
    events: list[dict[str, Any]],
    prior_pages: tuple[WikiPriorPage, ...],
    agent_reports: list[dict[str, Any]],
    terms: list[tuple[str, int]],
    *,
    limit: int = 12,
) -> list[TopicPageCandidate]:
    evidence_by_title: dict[str, list[str]] = defaultdict(list)
    score_by_title: Counter[str] = Counter()
    category_by_title: dict[str, str] = {}

    def add(raw_title: str, category_hint: str, evidence: str, *, weight: int = 1) -> None:
        title = normalize_topic_title(raw_title)
        if not title:
            return
        if title in {"For You", "Your Context", "Projects", "Topics"}:
            return
        category = category_hint or infer_topic_category(title)
        score_by_title[title] += weight
        category_by_title.setdefault(title, category)
        snippet = shorten(evidence, limit=220)
        if snippet and snippet not in evidence_by_title[title]:
            evidence_by_title[title].append(snippet)

    for event in events:
        text = clean_text(event.get("text"))
        label = f"{event.get('ts')} {event_project_label(event)}: {text}"
        for title in extract_wiki_link_labels(text):
            add(title, infer_topic_category(title), label, weight=6)
        for seed, (title, category) in TOPIC_SEEDS.items():
            if seed in text.casefold():
                add(title, category, label, weight=2)

    for report in agent_reports:
        report_text = "\n".join(
            [
                str(report.get("report") or ""),
                strip_markdown_frontmatter(str(report.get("artifact_body") or "")),
            ]
        )
        if not report_text.strip():
            continue
        label = f"{report.get('job_id')}: {clean_text(report_text)}"
        for title in extract_wiki_link_labels(report_text):
            add(title, infer_topic_category(title), label, weight=7)
        for seed, (title, category) in TOPIC_SEEDS.items():
            if seed in report_text.casefold():
                add(title, category, label, weight=3)

    for page in prior_pages:
        if not page.page_id.startswith("topic-"):
            continue
        title = first_markdown_heading(page.body_markdown) or title_from_topic_page_id(page.page_id)
        add(title, infer_topic_category(title), f"Retained prior page `{page.page_id}`.", weight=5)

    for term, count in terms:
        if count < 2:
            continue
        title = normalize_topic_title(term.replace("-", " "))
        if title and title.casefold() in {seed.casefold() for _, (seed, _) in TOPIC_SEEDS.items()}:
            add(title, infer_topic_category(title), f"`{term}` appeared {count} times in recent source text.", weight=min(count, 5))

    candidates: list[TopicPageCandidate] = []
    used_page_ids: set[str] = set()
    for title, score in sorted(score_by_title.items(), key=lambda item: (-item[1], item[0].casefold())):
        page_id = topic_page_id(title)
        if page_id in used_page_ids:
            continue
        used_page_ids.add(page_id)
        evidence = tuple(evidence_by_title[title][:12])
        candidates.append(
            TopicPageCandidate(
                page_id=page_id,
                title=title,
                category=category_by_title.get(title) or infer_topic_category(title),
                summary=topic_summary(title, evidence),
                evidence=evidence,
                score=score,
            )
        )
        if len(candidates) >= limit:
            break
    return candidates


def extract_wiki_link_labels(text: str) -> list[str]:
    labels: list[str] = []
    for match in re.finditer(r"\[\[([^\]\n]{2,80})\]\]", text):
        label = match.group(1).split("|", 1)[0].strip()
        if label:
            labels.append(label)
    return labels


def normalize_topic_title(raw_title: str) -> str:
    title = clean_text(raw_title)
    title = re.sub(r"^[#`*_\s]+|[#`*_\s]+$", "", title)
    title = title.strip("[](){}:;,.!?")
    if not title:
        return ""
    lower = title.casefold()
    replacements = {
        "1context": "1Context",
        "agent mail": "Agent Mail",
        "agent harness": "Agent Harness",
        "agent orchestration": "Agent Orchestration",
        "backfill scheduler": "Backfill Scheduler",
        "codex": "Codex",
        "macos daemon": "macOS Daemon",
        "perception db": "Perception DB",
        "postgres": "Postgres",
        "refresh wiki button": "Refresh Wiki Button",
        "timescale": "Timescale",
    }
    if lower in replacements:
        return replacements[lower]
    if re.fullmatch(r"\d{4}-\d{2}-\d{2}", title) or title.casefold() in STOPWORDS:
        return ""
    words = re.findall(r"[A-Za-z0-9][A-Za-z0-9.+_-]*", title)
    if not words or len(words) > 6:
        return ""
    if all(word.casefold() in STOPWORDS for word in words):
        return ""
    return " ".join(word if any(char.isupper() for char in word[1:]) else word.capitalize() for word in words)


def infer_topic_category(title: str) -> str:
    lower = title.casefold()
    if any(token in lower for token in ("daemon", "perception", "postgres", "timescale", "runtime", "permission", "harness")):
        return "Infrastructure"
    if any(token in lower for token in ("codex", "claude", "uv", "playwright", "xcode", "swiftpm")):
        return "Tools"
    if any(token in lower for token in ("backfill", "refresh", "workflow", "closed loop", "branch", "cleanup")):
        return "Process"
    if any(token in lower for token in ("openai", "github", "apple", "haptica", "mox", "anthropic")):
        return "Organizations"
    if any(token in lower for token in ("swift", "rust", "python", "schema", "state machine", "test")):
        return "Engineering"
    return "Domain"


def topic_page_id(title: str) -> str:
    slug = re.sub(r"[^a-z0-9]+", "-", title.casefold()).strip("-")
    return f"topic-{slug or 'page'}"[:80].rstrip("-")


def title_from_topic_page_id(page_id: str) -> str:
    return normalize_topic_title(page_id.removeprefix("topic-").replace("-", " ")) or "Topic"


def first_markdown_heading(text: str) -> str:
    for line in text.splitlines():
        if line.startswith("# "):
            return clean_text(line[2:])
    return ""


def topic_summary(title: str, evidence: tuple[str, ...]) -> str:
    if evidence:
        return f"`{title}` earned a page because the recent wiki run produced repeated source, editor, or librarian evidence for it."
    return f"`{title}` is retained as a topic page, but the next run should reinforce or prune it."


def topic_current_role(candidate: TopicPageCandidate) -> str:
    if candidate.category == "Infrastructure":
        return f"`{candidate.title}` is part of the operating substrate: the services, app runtime, storage, or daemon layer that makes the memory wiki run without manual babysitting."
    if candidate.category == "Process":
        return f"`{candidate.title}` is a workflow topic. It tracks how the wiki company decides what to ingest, when to wake agents, and how to keep the readable pages current instead of bloated."
    if candidate.category == "Tools":
        return f"`{candidate.title}` is a tool topic. It matters when repeated work depends on that tool's capabilities, constraints, or interaction pattern."
    if candidate.category == "Engineering":
        return f"`{candidate.title}` is an engineering topic. It should hold concrete implementation shape, tests, files, and decisions rather than generic definitions."
    return f"`{candidate.title}` is a domain topic in the 1Context memory system. It should explain how the concept behaves inside the user's work, not merely define the words."


def topic_open_questions(context: dict[str, Any], candidate: TopicPageCandidate) -> list[str]:
    questions = [
        question_summary(event)
        for event in context.get("open_questions", [])
        if candidate.title.casefold() in clean_text(event.get("text")).casefold()
    ][:3]
    if questions:
        return questions
    return [
        "Should this topic stay as its own page, merge into a broader page, or disappear after the next librarian sweep?",
        "Which stale claims should be deleted when newer agent evidence changes the current state?",
    ]


def topic_index_bullet(candidate: TopicPageCandidate) -> str:
    return f"[{candidate.title}](./{candidate.page_id}) - {candidate.category}; score {candidate.score}. {candidate.summary}"


def topic_section_bullets(candidates: list[TopicPageCandidate], terms: list[tuple[str, int]], allow: set[str]) -> list[str]:
    rows = [topic_index_bullet(candidate) for candidate in candidates[:8]]
    if rows:
        return bullets(rows)
    return topic_bullets(terms, allow)


def topic_bullets(terms: list[tuple[str, int]], allow: set[str]) -> list[str]:
    hits = [f"`{term}` - observed {count} time{'s' if count != 1 else ''} in recent imported session text." for term, count in terms if any(part in term for part in allow)]
    return bullets(hits[:8] or ["No strong source-backed topic entries were generated for this section yet."])


def paragraphs(items: Iterable[str]) -> list[str]:
    return [item for item in items if item]


def bullets(items: Iterable[str]) -> list[str]:
    return [f"- {item}" for item in items if item]


def session_signal(session: dict[str, Any]) -> str:
    sample = first_nonempty(
        [clean_text(item.get("text")) for item in session.get("user_samples", [])],
        [clean_text(item.get("text")) for item in session.get("assistant_samples", [])],
    )
    return (
        f"**{session['project']}** (`{session['source']}`, {session['last_ts']}): "
        f"{shorten(sample or 'session imported without a concise text sample')}"
    )


def session_detail(session: dict[str, Any]) -> str:
    user_sample = first_nonempty([clean_text(item.get("text")) for item in session.get("user_samples", [])])
    assistant_sample = first_nonempty([clean_text(item.get("text")) for item in session.get("assistant_samples", [])])
    samples = " / ".join(shorten(item, limit=170) for item in [user_sample, assistant_sample] if item)
    if not samples:
        samples = "No concise sample was available from this session."
    return (
        f"**{session['project']}** from `{session['source']}`: {session['event_count']} event(s), "
        f"`{session['first_ts']}` to `{session['last_ts']}`. {samples}"
    )


def evidence_sample(event: dict[str, Any]) -> str:
    return (
        f"`{event.get('ts')}` **{event_project_label(event)}** "
        f"`{event.get('source')}/{event.get('kind')}`: {shorten(clean_text(event.get('text')), limit=240)}"
    )


def project_evidence(project: dict[str, Any]) -> str:
    samples = [shorten(clean_text(item.get("text")), limit=180) for item in project.get("samples", []) if clean_text(item.get("text"))]
    tail = " | ".join(samples[-2:]) if samples else "No concise sample text."
    return (
        f"**{project['project']}**: {project['event_count']} event(s), last `{project['last_ts']}`. {tail}"
    )


def project_entry(project: dict[str, Any]) -> str:
    sample = first_nonempty([clean_text(item.get("text")) for item in project.get("samples", [])])
    return f"**{project['project']}** - {project['event_count']} recent Perception DB events; last imported `{project['last_ts']}`. {shorten(sample)}"


def question_summary(event: dict[str, Any]) -> str:
    project = event_project_label(event)
    return f"**{project}** ({event.get('ts')}): {shorten(clean_text(event.get('text')))}"


def first_nonempty(*groups: Iterable[str]) -> str:
    for group in groups:
        for item in group:
            if item:
                return item
    return ""


def project_label(cwd: str) -> str:
    if not cwd:
        return "unknown"
    path = Path(cwd)
    parts = list(path.parts)
    if ".claude" in parts:
        index = parts.index(".claude")
        if index > 0:
            return parts[index - 1]
    if path.name in {"worktrees", "docs", "src"} and len(parts) > 1:
        return parts[-2]
    return path.name or "unknown"


def event_project_label(row: dict[str, Any]) -> str:
    project_key = str(row.get("project_key") or "").strip()
    if project_key:
        return project_key
    cwd = str(row.get("cwd") or "").strip()
    if cwd:
        return project_label(cwd)
    source_uri = str(row.get("source_uri") or "").strip()
    if source_uri:
        return project_label(source_uri)
    return "unknown"


def clean_text(value: Any) -> str:
    text = str(value or "")
    text = re.sub(r"\s+", " ", text).strip()
    for prefix in ("[assistant] ", "[user] ", "[tool-result] ", "[tool-use] "):
        if text.startswith(prefix):
            text = text[len(prefix) :].strip()
    return text


def shorten(text: str, *, limit: int = MAX_SNIPPET_CHARS) -> str:
    text = clean_text(text)
    if len(text) <= limit:
        return neutralize_local_markdown_links(text)
    return neutralize_local_markdown_links(text[: limit - 1].rstrip() + "...")


def event_sort_key(row: dict[str, Any]) -> tuple[datetime, str]:
    return parse_time(str(row.get("ts") or "")), str(row.get("event_id") or "")


def parse_time(value: str) -> datetime:
    try:
        text = value.replace("Z", "+00:00")
        return datetime.fromisoformat(text).astimezone(timezone.utc)
    except ValueError:
        return datetime.min.replace(tzinfo=timezone.utc)


def parse_json(text: str) -> dict[str, Any]:
    try:
        parsed = json.loads(text)
    except json.JSONDecodeError:
        return {"raw": text.strip()}
    return parsed if isinstance(parsed, dict) else {"value": parsed}


def write_status(writes: tuple[WikiPageWrite, ...]) -> str:
    if not writes:
        return "drafted"
    if any(write.status == "failed" for write in writes):
        return "failed"
    if all(write.status == "written" for write in writes):
        return "written"
    return "partial"


def format_path(path: Path, root: Path | None) -> str:
    if root:
        try:
            return str(path.relative_to(root))
        except ValueError:
            pass
    return str(path)
