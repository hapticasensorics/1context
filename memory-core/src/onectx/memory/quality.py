from __future__ import annotations

import hashlib
import json
import re
from dataclasses import dataclass
from datetime import date, datetime, timezone
from pathlib import Path
from typing import Any

from onectx.config import MemorySystem
from onectx.storage import LakeStore, stable_id, utc_now


class QualityError(RuntimeError):
    """Raised when memory quality probes cannot run."""


@dataclass(frozen=True)
class QualityIssue:
    code: str
    severity: str
    path: str
    line: int
    title: str
    detail: str

    def to_payload(self) -> dict[str, Any]:
        return {
            "code": self.code,
            "severity": self.severity,
            "path": self.path,
            "line": self.line,
            "title": self.title,
            "detail": self.detail,
        }


@dataclass(frozen=True)
class QualityReport:
    root: Path
    generated_at: str
    file_count: int
    issues: tuple[QualityIssue, ...]

    @property
    def issue_count(self) -> int:
        return len(self.issues)

    @property
    def passed(self) -> bool:
        return not any(issue.severity == "error" for issue in self.issues)

    def to_payload(self) -> dict[str, Any]:
        by_code: dict[str, int] = {}
        by_severity: dict[str, int] = {}
        for issue in self.issues:
            by_code[issue.code] = by_code.get(issue.code, 0) + 1
            by_severity[issue.severity] = by_severity.get(issue.severity, 0) + 1
        return {
            "kind": "memory_quality_report.v1",
            "root": str(self.root),
            "generated_at": self.generated_at,
            "passed": self.passed,
            "file_count": self.file_count,
            "issue_count": self.issue_count,
            "summary": {
                "by_code": dict(sorted(by_code.items())),
                "by_severity": dict(sorted(by_severity.items())),
            },
            "issues": [issue.to_payload() for issue in self.issues],
        }


def run_quality_probes(
    root: Path | str,
    *,
    now: date | datetime | str | None = None,
    stale_current_state_days: int = 30,
) -> QualityReport:
    root_path = Path(root).expanduser().resolve()
    if not root_path.exists():
        raise QualityError(f"quality root does not exist: {root_path}")
    reference_date = parse_reference_date(now)
    paths = markdown_paths(root_path)
    issues: list[QualityIssue] = []
    for path in paths:
        try:
            text = path.read_text(encoding="utf-8")
        except UnicodeDecodeError:
            continue
        relative = path.relative_to(root_path).as_posix() if path != root_path else path.name
        issues.extend(probe_markdown(path=relative, text=text, reference_date=reference_date, stale_days=stale_current_state_days))
    issues.sort(key=lambda issue: (issue.path, issue.line, issue.code))
    return QualityReport(root=root_path, generated_at=utc_now(), file_count=len(paths), issues=tuple(issues))


def write_quality_report(system: MemorySystem, report: QualityReport, *, run_id: str = "") -> dict[str, Any]:
    resolved_run_id = run_id or stable_id("quality", str(report.root), report.generated_at)
    out_dir = system.runtime_dir / "quality" / resolved_run_id
    out_dir.mkdir(parents=True, exist_ok=True)
    path = out_dir / "quality.json"
    payload = report.to_payload()
    payload["run_id"] = resolved_run_id
    text = json.dumps(payload, indent=2, sort_keys=True, default=str) + "\n"
    path.write_text(text, encoding="utf-8")
    content_hash = hashlib.sha256(text.encode("utf-8")).hexdigest()

    store = LakeStore(system.storage_dir)
    store.ensure()
    artifact = store.append_artifact(
        "memory_quality_report",
        uri=path.as_uri(),
        path=str(path),
        content_type="application/json",
        content_hash=content_hash,
        bytes=path.stat().st_size,
        source="memory.quality",
        state="passed" if report.passed else "failed",
        text=f"memory quality report {resolved_run_id}",
        metadata={
            "run_id": resolved_run_id,
            "root": str(report.root),
            "file_count": report.file_count,
            "issue_count": report.issue_count,
        },
    )
    evidence = store.append_evidence(
        "memory_quality.probed",
        artifact_id=artifact["artifact_id"],
        status="passed" if report.passed else "failed",
        checker="memory.quality",
        text="memory structural quality probes completed",
        checks=list(payload["summary"]["by_code"].keys()) or ["no structural quality issues"],
        payload=payload,
    )
    event = store.append_event(
        "memory.quality.probed",
        source="memory.quality",
        kind="quality_report",
        subject=resolved_run_id,
        artifact_id=artifact["artifact_id"],
        evidence_id=evidence["evidence_id"],
        payload={
            "run_id": resolved_run_id,
            "passed": report.passed,
            "file_count": report.file_count,
            "issue_count": report.issue_count,
        },
    )
    return {
        "run_id": resolved_run_id,
        "path": str(path),
        "artifact_id": artifact["artifact_id"],
        "evidence_id": evidence["evidence_id"],
        "event_id": event["event_id"],
        "passed": report.passed,
    }


def probe_markdown(*, path: str, text: str, reference_date: date, stale_days: int) -> list[QualityIssue]:
    issues: list[QualityIssue] = []
    if not has_frontmatter(text):
        issues.append(
            QualityIssue(
                code="missing_frontmatter",
                severity="warning",
                path=path,
                line=1,
                title="Missing frontmatter",
                detail="Markdown file has no leading frontmatter block.",
            )
        )

    sections = extract_sections(text)
    for section in sections:
        title_key = section["title"].casefold()
        body = str(section["body"])
        if 2 <= int(section["level"]) <= 3 and not body.strip():
            issues.append(
                QualityIssue(
                    code="empty_section",
                    severity="warning",
                    path=path,
                    line=int(section["line"]),
                    title="Empty section",
                    detail=f"Section {section['title']!r} has no body text.",
                )
            )
        if title_key == "current state":
            issues.extend(probe_current_state(path, section, reference_date=reference_date, stale_days=stale_days))
        if title_key in {"open questions", "open question", "questions"}:
            issues.extend(probe_open_questions(path, section))

    for line_no, line in enumerate(text.splitlines(), start=1):
        if is_initial_fill_marker(line):
            issues.append(
                QualityIssue(
                    code="initial_fill_marker",
                    severity="error",
                    path=path,
                    line=line_no,
                    title="Persistent initial-fill marker",
                    detail="Initial-fill placeholder language should not remain in production wiki pages.",
                )
            )
    return issues


def probe_current_state(path: str, section: dict[str, Any], *, reference_date: date, stale_days: int) -> list[QualityIssue]:
    dates = [parse_iso_date(match.group(0)) for match in re.finditer(r"\b20\d\d-\d\d-\d\d\b", str(section["body"]))]
    dates = [value for value in dates if value is not None]
    if not dates:
        return []
    newest = max(dates)
    age_days = (reference_date - newest).days
    if age_days <= stale_days:
        return []
    return [
        QualityIssue(
            code="stale_current_state",
            severity="error",
            path=path,
            line=int(section["line"]),
            title="Stale Current State",
            detail=f"Newest Current State date is {newest.isoformat()}, {age_days} days before {reference_date.isoformat()}.",
        )
    ]


def probe_open_questions(path: str, section: dict[str, Any]) -> list[QualityIssue]:
    issues: list[QualityIssue] = []
    start_line = int(section["line"])
    for offset, line in enumerate(str(section["body"]).splitlines(), start=1):
        normalized = line.casefold()
        if "[x]" in normalized or any(word in normalized for word in ("resolved", "answered", "decided", "closed")):
            issues.append(
                QualityIssue(
                    code="resolved_open_question",
                    severity="error",
                    path=path,
                    line=start_line + offset,
                    title="Resolved item still listed as open",
                    detail="Open Questions contains an item that appears resolved or closed.",
                )
            )
    return issues


def markdown_paths(root: Path) -> tuple[Path, ...]:
    if root.is_file():
        return (root,) if root.suffix == ".md" else ()
    ignored = {".git", ".venv", "__pycache__", "node_modules", "memory/runtime", "wiki/generated"}
    paths = []
    for path in root.rglob("*.md"):
        relative = path.relative_to(root).as_posix()
        if any(relative == item or relative.startswith(f"{item}/") for item in ignored):
            continue
        paths.append(path)
    return tuple(sorted(paths))


def extract_sections(text: str) -> list[dict[str, Any]]:
    lines = text.splitlines()
    headings: list[dict[str, Any]] = []
    for index, line in enumerate(lines, start=1):
        match = re.match(r"^(#{1,6})\s+(.+?)\s*$", line)
        if match:
            headings.append({"level": len(match.group(1)), "title": match.group(2).strip(), "line": index})
    sections: list[dict[str, Any]] = []
    for index, heading in enumerate(headings):
        start = int(heading["line"])
        end = int(headings[index + 1]["line"]) - 1 if index + 1 < len(headings) else len(lines)
        body = "\n".join(lines[start:end]).strip()
        sections.append({**heading, "body": body})
    return sections


def has_frontmatter(text: str) -> bool:
    if not text.startswith("---\n"):
        return False
    return text.find("\n---", 4) != -1


def is_initial_fill_marker(line: str) -> bool:
    normalized = line.casefold()
    return "initial fill" in normalized or "initial-fill" in normalized or "first-fill placeholder" in normalized


def parse_reference_date(value: date | datetime | str | None) -> date:
    if value is None:
        return datetime.now(timezone.utc).date()
    if isinstance(value, datetime):
        return value.date()
    if isinstance(value, date):
        return value
    parsed = parse_iso_date(value[:10])
    if parsed is None:
        raise QualityError(f"invalid reference date: {value}")
    return parsed


def parse_iso_date(value: str) -> date | None:
    try:
        return date.fromisoformat(value)
    except ValueError:
        return None
