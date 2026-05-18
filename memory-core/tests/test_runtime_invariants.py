from __future__ import annotations

from onectx.memory.invariants import build_runtime_invariant_report


def test_runtime_invariant_report_accepts_explicit_skip_with_no_render_request() -> None:
    report = build_runtime_invariant_report(
        run_id="test-invariants-explicit",
        mode="wiki_only",
        status="completed",
        dry_run=True,
        preflight={
            "source_freshness": {
                "status": "skipped",
                "reason": "no source-derived route planning requested",
            }
        },
        steps=[{"id": "wiki_render", "status": "skipped", "reason": "execute_render=false"}],
        execute_render=False,
    )

    assert report["summary"]["passed"] is True
    assert report["summary"]["silent_noops"] == 0
    assert any(
        item["outcome"] == "skipped"
        for item in report["postflight_diff"]["explicit_outcomes"]
    )


def test_runtime_invariant_report_fails_missing_refresh_request() -> None:
    report = build_runtime_invariant_report(
        run_id="test-invariants-missing-reason",
        mode="wiki_only",
        status="completed",
        dry_run=False,
        preflight={"source_freshness": {"status": "passed"}},
        steps=[{"id": "wiki_render", "status": "requested"}],
        execute_render=True,
        render_count=0,
    )

    assert report["summary"]["passed"] is False
    assert report["summary"]["silent_noops"] == 1
    assert report["postflight_diff"]["missing"][0]["kind"] == "wiki_refresh_request"
