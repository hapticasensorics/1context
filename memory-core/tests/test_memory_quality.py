from __future__ import annotations

import json
from dataclasses import replace
from pathlib import Path

from onectx.config import load_system
from onectx.memory.quality import run_quality_probes, write_quality_report


def test_quality_probe_flags_stale_current_state(tmp_path: Path) -> None:
    page = tmp_path / "concept.md"
    page.write_text(
        """---
title: Concept
slug: concept
---
# Concept

## Current State

Last reviewed: 2026-03-01.
""",
        encoding="utf-8",
    )

    report = run_quality_probes(tmp_path, now="2026-04-29", stale_current_state_days=30)

    issues = {issue.code: issue for issue in report.issues}
    assert "stale_current_state" in issues
    assert issues["stale_current_state"].severity == "error"
    assert report.passed is False


def test_quality_probe_flags_resolved_open_question(tmp_path: Path) -> None:
    page = tmp_path / "questions.md"
    page.write_text(
        """---
title: Questions
slug: questions
---
# Questions

## Open Questions

- [x] Resolved: should the librarian expand the existing page?
- Still open: how often should this run?
""",
        encoding="utf-8",
    )

    report = run_quality_probes(tmp_path, now="2026-04-29")

    assert "resolved_open_question" in {issue.code for issue in report.issues}


def test_quality_probe_flags_initial_fill_marker(tmp_path: Path) -> None:
    page = tmp_path / "initial.md"
    page.write_text(
        """---
title: Initial
slug: initial
---
# Initial

This is an initial fill and should be replaced after synthesis.
""",
        encoding="utf-8",
    )

    report = run_quality_probes(tmp_path, now="2026-04-29")

    assert "initial_fill_marker" in {issue.code for issue in report.issues}
    assert report.passed is False


def test_quality_probe_flags_missing_frontmatter(tmp_path: Path) -> None:
    page = tmp_path / "plain.md"
    page.write_text("# Plain\n\nBody.\n", encoding="utf-8")

    report = run_quality_probes(tmp_path, now="2026-04-29")

    issues = {issue.code: issue for issue in report.issues}
    assert "missing_frontmatter" in issues
    assert issues["missing_frontmatter"].severity == "warning"


def test_quality_report_can_be_recorded(tmp_path: Path) -> None:
    system = replace(
        load_system(Path.cwd()),
        runtime_dir=tmp_path / "runtime",
        storage_dir=tmp_path / "lakestore",
    )
    page = tmp_path / "wiki" / "concept.md"
    page.parent.mkdir()
    page.write_text(
        """---
title: Concept
slug: concept
---
# Concept

## Current State

Last reviewed: 2026-03-01.
""",
        encoding="utf-8",
    )

    report = run_quality_probes(page.parent, now="2026-04-29", stale_current_state_days=30)
    record = write_quality_report(system, report, run_id="quality-record")

    path = Path(record["path"])
    assert path.is_file()
    payload = json.loads(path.read_text(encoding="utf-8"))
    assert payload["run_id"] == "quality-record"
    assert payload["issue_count"] == 1
    assert record["artifact_id"]
    assert record["evidence_id"]
