from __future__ import annotations

from onectx.memory.invariants import build_runtime_invariant_report


def test_runtime_invariant_report_accepts_explicit_skip_and_no_change() -> None:
    report = build_runtime_invariant_report(
        run_id="test-invariants-explicit",
        mode="wiki_route_dry_run",
        status="planned",
        dry_run=True,
        preflight={
            "source_freshness": {
                "status": "skipped",
                "reason": "no source-derived route planning requested",
            }
        },
        route_preview={
            "planned_hires": [],
            "non_hire_outcomes": [
                {
                    "job_key": "reader-build",
                    "job": "memory.wiki.build_inputs",
                    "outcome": "no_change",
                    "reason": "reader surface already current",
                }
            ],
        },
        execute_render=False,
    )

    assert report["summary"]["passed"] is True
    assert report["summary"]["silent_noops"] == 0
    assert any(
        item["outcome"] == "no_change"
        for item in report["postflight_diff"]["explicit_outcomes"]
    )


def test_runtime_invariant_report_fails_unexplained_non_hire() -> None:
    report = build_runtime_invariant_report(
        run_id="test-invariants-missing-reason",
        mode="wiki_route_dry_run",
        status="planned",
        dry_run=True,
        preflight={"source_freshness": {"status": "passed"}},
        route_preview={
            "planned_hires": [],
            "non_hire_outcomes": [
                {
                    "job_key": "silent-skip",
                    "job": "memory.wiki.librarian",
                    "outcome": "skip",
                    "reason": "",
                }
            ],
        },
        execute_render=False,
    )

    assert report["summary"]["passed"] is False
    assert report["summary"]["silent_noops"] == 1
    assert report["postflight_diff"]["missing"][0]["kind"] == "route_non_hire"
