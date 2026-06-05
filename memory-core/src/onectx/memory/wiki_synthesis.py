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
WIKI_PAGE_DEFINITIONS = (
    ("for-you", "For You"),
    ("your-context", "Your Context"),
    ("projects", "Projects"),
    ("topics", "Topics"),
)
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
    source_store: str = "perception_db"
    source_status: str = ""
    window_days: int = 30
    prior_pages: tuple[WikiPriorPage, ...] = ()

    def to_payload(self, *, root: Path | None = None) -> dict[str, Any]:
        return {
            "status": self.status,
            "path": format_path(self.path, root),
            "source_event_count": self.source_event_count,
            "source_session_count": self.source_session_count,
            "source_store": self.source_store,
            "source_status": self.source_status,
            "window_days": self.window_days,
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
    prior_pages: Iterable[WikiPriorPage] | None = None,
) -> WikiSynthesisResult:
    source_events = meaningful_events(source_events, limit=2_400)
    source_sessions = meaningful_sessions(source_sessions, limit=160)
    resolved_prior_pages = tuple(prior_pages) if prior_pages is not None else load_prior_wiki_pages(runtime_root)
    drafts_dir = output_dir / "page-drafts"
    drafts = build_page_drafts(
        system,
        run_id=run_id,
        drafts_dir=drafts_dir,
        events=source_events,
        sessions=source_sessions,
        prior_pages=resolved_prior_pages,
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
        source_store=source_store,
        source_status=source_status,
        window_days=window_days,
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


def build_page_drafts(
    system: MemorySystem,
    *,
    run_id: str,
    drafts_dir: Path,
    events: list[dict[str, Any]],
    sessions: list[dict[str, Any]],
    prior_pages: tuple[WikiPriorPage, ...] = (),
) -> list[WikiPageDraft]:
    now = utc_now()
    context = build_synthesis_context(events, sessions, prior_pages=prior_pages, now=now)
    pages = [
        ("for-you", "For You", render_for_you(context)),
        ("your-context", "Your Context", render_your_context(context)),
        ("projects", "Projects", render_projects(context)),
        ("topics", "Topics", render_topics(context)),
    ]
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

    open_questions = extract_questions(events)
    terms = top_terms(events)
    return {
        "now": now,
        "events": events,
        "sessions": sessions,
        "recent_sessions": recent_sessions,
        "projects": project_rows[:10],
        "open_questions": open_questions,
        "terms": terms,
        "prior_pages": {page.page_id: page for page in prior_pages},
        "prior_summaries": {page.page_id: prior_page_summary(page.body_markdown) for page in prior_pages},
        "counts": {
            "source_events": len(events),
            "source_sessions": len(sessions),
            "projects": len(project_rows),
            "prior_pages": len(prior_pages),
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
            "## Current Orientation",
            "",
            *paragraphs(current),
            "",
            "## Recent Signals",
            "",
            *bullets([session_signal(session) for session in sessions[:8]] or ["No recent source-backed session signals were imported yet."]),
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
            "## Active",
            "",
            *bullets(active[:8] or ["No active project signals were imported yet."]),
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
    return "\n".join(
        [
            "# Topics",
            "",
            "## Engineering",
            "",
            *topic_bullets(terms, {"swift", "rust", "python", "state", "machine", "schema", "test", "build", "release", "runtime"}),
            "",
            "## Infrastructure",
            "",
            *topic_bullets(terms, {"sparkle", "permission", "daemon", "launchagent", "caddy", "local", "dev", "app", "wiki"}),
            "",
            "## Process",
            "",
            *bullets(
                [
                    "`closed-loop-testing` - implement, build, install, diagnose, and verify the app-visible surface.",
                    "`branch-reconciliation` - compare local folders and branches before deleting or merging.",
                    "`talk-folder-governance` - proposals and decisions stay visible as page history.",
                ]
            ),
            "",
            "## Tools",
            "",
            *topic_bullets(terms, {"codex", "claude", "uv", "playwright", "xcode", "swiftpm", "perception", "postgres", "timescale"}),
            "",
            "## Domain",
            "",
            *bullets(
                [
                    "`personal-wiki` - a local, source-backed memory surface for collaborators and agents.",
                    "`agent-orchestration` - hired roles such as scribes, historians, biographers, curators, librarians, and redactors.",
                ]
            ),
            "",
            "## People",
            "",
            "No people index entries were generated from this bounded import. The librarian should only add people when repeated evidence makes the relationship operationally useful.",
            "",
            "## Organizations",
            "",
            *topic_bullets(terms, {"haptica", "mox", "openai", "anthropic", "github", "apple"}),
            "",
            *prior_continuity_section(context, "topics"),
            "## See Also",
            "",
            "- [Your Context](./your-context)",
            "- [Projects](./projects)",
            "- [For You](./for-you)",
        ]
    )


def load_prior_wiki_pages(runtime_root: Path | None, page_ids: tuple[str, ...] | None = None) -> tuple[WikiPriorPage, ...]:
    if runtime_root is None:
        return ()
    source_root = runtime_root / "user-wiki" / "source"
    if not source_root.exists():
        return ()
    resolved_page_ids = page_ids or tuple(page_id for page_id, _ in WIKI_PAGE_DEFINITIONS)
    pages: list[WikiPriorPage] = []
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
    return tuple(pages)


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

    writes: list[WikiPageWrite] = []
    for draft in drafts:
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
        return text
    return text[: limit - 1].rstrip() + "..."


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
