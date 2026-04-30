from __future__ import annotations

import json
from datetime import date, datetime, timezone
from pathlib import Path

from onectx.config import load_system
from onectx.memory.wiki import (
    WikiError,
    build_wiki_inputs,
    evaluate_wiki_route_source_freshness,
    monday_anchor,
    plan_wiki_roles,
    preview_wiki_route_execution,
    write_wiki_route_plan_artifact,
)
from onectx.storage import LakeStore


def test_wiki_era_helpers_use_dynamic_monday_anchor() -> None:
    assert monday_anchor("2026-04-27") == "2026-04-27"
    assert monday_anchor("2026-05-03") == "2026-04-27"
    assert monday_anchor("2026-05-04") == "2026-05-04"


def test_wiki_build_inputs_resolves_aliases_and_backlinks(tmp_path: Path) -> None:
    workspace = tmp_path / "wiki"
    concepts = tmp_path / "concepts"
    staging = tmp_path / "staged"
    workspace.mkdir()
    concepts.mkdir()

    (concepts / "1context.md").write_text(
        """---
aliases: [onecontext]
categories: [Engineering]
---

## 1Context

1Context is the memory layer.

### Open Questions

- How should the wiki renderer and redactor converge?
""",
        encoding="utf-8",
    )
    (concepts / "guardian.md").write_text(
        """---
aliases:
  - guardian-app
categories: [Domain]
---

## Guardian

Guardian is a product surface.
""",
        encoding="utf-8",
    )
    (workspace / "2026-04-20.md").write_text(
        """---
title: For You
---

# For You

<!-- section:"2026-04-20" -->

## Monday

The work connected [[onecontext]] to [[guardian-app]] and [[Postgres]].

Open going into Tuesday: decide where [[1Context]] redaction tiers live.
""",
        encoding="utf-8",
    )
    (workspace / "2026-04-20.private.talk").mkdir()
    (workspace / "2026-04-20.private.talk" / "2026-04-20T12-00Z.decided.concept-1context.md").write_text(
        "---\nkind: decided\n---\n\n[DECIDED] promote 1Context.\n",
        encoding="utf-8",
    )

    result = build_wiki_inputs(
        workspace=workspace,
        concept_dir=concepts,
        staging=staging,
        web_base="/demo",
        today=date(2026, 4, 29),
    )

    assert result.concept_count == 2
    assert result.open_question_count == 2
    assert result.backlink_edge_count >= 2

    staged_article = (staging / "2026-04-20.md").read_text(encoding="utf-8")
    assert "[onecontext](/demo/concept/1context)" in staged_article
    assert "[guardian-app](/demo/concept/guardian)" in staged_article
    assert "[Postgres](https://www.postgresql.org/)" in staged_article

    backlinks = json.loads((staging / "_backlinks.json").read_text(encoding="utf-8"))
    assert "for-you-2026-04-20" in backlinks["1context"]
    assert "for-you-2026-04-20" in backlinks["guardian"]

    staged_concept = (staging / "concept" / "1context.md").read_text(encoding="utf-8")
    assert "### What links here" in staged_concept
    assert "For You - 4/20" in staged_concept

    assert (workspace / "topics.md").exists()
    assert (workspace / "projects.md").exists()
    assert (workspace / "open-questions.md").exists()
    assert (workspace / "index.md").exists()
    assert (workspace / "this-week.md").exists()


def test_wiki_build_inputs_rejects_staging_that_overlaps_workspace(tmp_path: Path) -> None:
    workspace = tmp_path / "wiki"
    concepts = tmp_path / "concepts"
    workspace.mkdir()
    concepts.mkdir()
    (workspace / "2026-04-20.md").write_text("# For You\n", encoding="utf-8")

    try:
        build_wiki_inputs(
            workspace=workspace,
            concept_dir=concepts,
            staging=workspace,
            web_base="/demo",
            today=date(2026, 4, 29),
        )
    except WikiError as exc:
        assert "must not overlap" in str(exc)
    else:
        raise AssertionError("overlapping staging path should be rejected")

    assert (workspace / "2026-04-20.md").is_file()


def test_wiki_role_plan_routes_pending_circuits(tmp_path: Path) -> None:
    workspace = tmp_path / "wiki"
    concepts = tmp_path / "concepts"
    workspace.mkdir()
    concepts.mkdir()
    (concepts / "1context.md").write_text("## 1Context\n\nMemory layer.\n", encoding="utf-8")
    (workspace / "your-context.md").write_text("# Your Context\n", encoding="utf-8")
    (workspace / "2026-04-20.md").write_text(
        """# For You

<!-- section:"biography" -->
## Biography
<!-- empty: weekly-rewrite slot -->

<!-- section:"2026-04-20" -->
## Monday
Filled.

<!-- section:"2026-04-21" -->
## Tuesday
Filled.

<!-- section:"2026-04-22" -->
## Wednesday
Filled.
""",
        encoding="utf-8",
    )
    talk = workspace / "2026-04-20.private.talk"
    talk.mkdir()
    (talk / "2026-04-20T09-00Z.conversation.md").write_text(
        "---\nkind: conversation\nts: 2026-04-20T09:00:00Z\n---\n\nHour.\n",
        encoding="utf-8",
    )
    (talk / "2026-04-20T23-59Z.proposal.concept-1context.md").write_text(
        "---\nkind: proposal\nts: 2026-04-20T23:59:00Z\n---\n\nProposal.\n",
        encoding="utf-8",
    )
    (talk / "2026-04-20T23-59Z.proposal.editor-day-2026-04-20.md").write_text(
        "---\nkind: proposal\nts: 2026-04-20T23:59:00Z\n---\n\nProposal.\n",
        encoding="utf-8",
    )
    ycx_talk = workspace / "your-context.talk"
    ycx_talk.mkdir()
    (ycx_talk / "2026-04-20T23-59Z.proposal.ycx-working-style.md").write_text(
        "---\nkind: proposal\n---\n\nProposal.\n",
        encoding="utf-8",
    )

    plan = plan_wiki_roles(workspace=workspace, concept_dir=concepts)

    assert plan.route_counts["for_you_curator_jobs"] == 1
    assert plan.route_counts["context_curator_jobs"] == 1
    assert plan.route_counts["librarian_jobs"] == 1
    assert plan.route_counts["biographer_jobs"] == 1
    assert plan.route_counts["redaction_jobs"] == 2
    assert plan.route_counts["reader_build_jobs"] == 1

    curator = plan.route_plan["for_you_curator_jobs"][0]
    assert curator["era_window"]["window_start"] == "2026-04-20"
    assert curator["era_window"]["window_end"] == "2026-05-03"
    assert [item["era"] for item in curator["adjacent_talk_folders"]] == [
        "2026-04-13",
        "2026-04-20",
        "2026-04-27",
    ]
    assert curator["tier_model"]["canonical_private_source"].endswith("2026-04-20.md")
    assert curator["tier_model"]["tier_outputs"]["internal"].endswith("2026-04-20.internal.md")
    assert curator["tier_model"]["tier_outputs"]["public"].endswith("2026-04-20.public.md")
    assert curator["tier_model"]["phantom_private_file_required"] is False
    assert curator["source_packet"]["loaded_at_birth"] is True
    assert curator["source_packet"]["source_manifest"]
    assert curator["source_packet"]["sha256"]
    assert curator["ownership"]["kind"] == "article_sections"

    preview = preview_wiki_route_execution(plan, system=load_system(Path.cwd()))
    assert preview.planned_hire_count == plan.planned_hire_count
    assert any(hire["birth_certificate_preview"]["experience_packet"] for hire in preview.planned_hires)
    assert all(hire["route_id"] for hire in preview.planned_hires)
    assert all(hire["prompt_stack_preview"]["sha256"] for hire in preview.planned_hires)
    assert all(
        hire["prompt_stack_preview"]["source_packet"]["rendered_estimated_tokens"] > 0
        for hire in preview.planned_hires
    )

    artifact = write_wiki_route_plan_artifact(load_system(Path.cwd()), plan, route_plan_id="test-wiki-role-plan")
    payload = json.loads(artifact.path.read_text(encoding="utf-8"))
    assert payload["kind"] == "wiki_role_route_plan"
    assert payload["route_plan_id"] == "test-wiki-role-plan"
    assert payload["route_plan"]["for_you_curator_jobs"][0]["ownership"]["kind"] == "article_sections"


def test_wiki_route_freshness_summarizes_lakestore_importers(tmp_path: Path) -> None:
    store = LakeStore(tmp_path / "lakestore")
    store.ensure()
    store.append_session(
        "codex_recent",
        source="codex",
        first_ts="2026-04-29T00:00:00Z",
        last_ts="2026-04-29T05:00:00Z",
        event_count=10,
    )
    store.append_session(
        "claude_old",
        source="claude-code",
        first_ts="2026-04-27T00:00:00Z",
        last_ts="2026-04-27T05:00:00Z",
        event_count=4,
    )

    freshness = evaluate_wiki_route_source_freshness(
        store,
        required_sources=("codex", "claude-code"),
        max_age_hours=24,
        now=datetime(2026, 4, 29, 6, 0, tzinfo=timezone.utc),
    )

    assert freshness["passed"] is False
    assert freshness["sources"]["codex"]["status"] == "fresh"
    assert freshness["sources"]["claude-code"]["status"] == "stale"
    assert freshness["sources"]["codex"]["event_count"] == 10


def test_wiki_role_plan_splits_oversized_source_packets(tmp_path: Path) -> None:
    workspace = tmp_path / "wiki"
    concepts = tmp_path / "concepts"
    workspace.mkdir()
    concepts.mkdir()
    (concepts / "1context.md").write_text("## 1Context\n\nMemory layer.\n", encoding="utf-8")
    (workspace / "your-context.md").write_text("# Your Context\n", encoding="utf-8")
    (workspace / "2026-04-20.md").write_text(
        """# For You

<!-- section:"2026-04-20" -->
## Monday
Filled.
""",
        encoding="utf-8",
    )
    talk = workspace / "2026-04-20.private.talk"
    talk.mkdir()
    (talk / "2026-04-20T09-00Z.conversation.md").write_text(
        "---\nkind: conversation\nts: 2026-04-20T09:00:00Z\n---\n\nHour.\n",
        encoding="utf-8",
    )
    (talk / "2026-04-20T23-59Z.proposal.editor-day-2026-04-20.md").write_text(
        "---\nkind: proposal\nts: 2026-04-20T23:59:00Z\n---\n\nProposal.\n",
        encoding="utf-8",
    )
    for index in range(9):
        (talk / f"2026-04-20T23-5{index}Z.synthesis.large-{index}.md").write_text(
            "---\nkind: synthesis\n---\n\n" + ("large source line " * 12000),
            encoding="utf-8",
        )

    plan = plan_wiki_roles(workspace=workspace, concept_dir=concepts)

    curator = plan.route_plan["for_you_curator_jobs"][0]
    assert curator["outcome"] == "split_parent"
    assert curator["source_packet"]["requires_split"] is True
    assert curator["split"]["shard_count"] >= 2
    curator_shards = [
        row for row in plan.route_plan["source_packet_shard_jobs"] if row["parent_route_id"] == curator["route_id"]
    ]
    curator_aggregates = [
        row for row in plan.route_plan["source_packet_aggregate_jobs"] if row["parent_route_id"] == curator["route_id"]
    ]
    assert len(curator_shards) == curator["split"]["shard_count"]
    assert len(curator_aggregates) == 1

    preview = preview_wiki_route_execution(plan, system=load_system(Path.cwd()))
    assert any(hire["job_ids"] == ["memory.wiki.source_packet_shard"] for hire in preview.planned_hires)
    assert any(hire["job_ids"] == ["memory.wiki.source_packet_aggregate"] for hire in preview.planned_hires)
    assert any(outcome["outcome"] == "split_parent" for outcome in preview.non_hire_outcomes)


def test_wiki_role_plan_splits_single_oversized_source_file_into_slices(tmp_path: Path) -> None:
    workspace = tmp_path / "wiki"
    concepts = tmp_path / "concepts"
    workspace.mkdir()
    concepts.mkdir()
    (concepts / "1context.md").write_text("## 1Context\n\nMemory layer.\n", encoding="utf-8")
    (workspace / "your-context.md").write_text("# Your Context\n", encoding="utf-8")
    (workspace / "2026-04-20.md").write_text("# For You\n\n## Monday\nFilled.\n", encoding="utf-8")
    talk = workspace / "2026-04-20.private.talk"
    talk.mkdir()
    (talk / "2026-04-20T09-00Z.conversation.md").write_text(
        "---\nkind: conversation\nts: 2026-04-20T09:00:00Z\n---\n\n" + ("large source line " * 32000),
        encoding="utf-8",
    )
    (talk / "2026-04-20T23-59Z.proposal.editor-day-2026-04-20.md").write_text(
        "---\nkind: proposal\nts: 2026-04-20T23:59:00Z\n---\n\nProposal.\n",
        encoding="utf-8",
    )

    plan = plan_wiki_roles(workspace=workspace, concept_dir=concepts)
    shards = plan.route_plan["source_packet_shard_jobs"]

    assert shards
    assert any(shard["source_packet"].get("file_slices") for shard in shards)
    assert all(
        int(shard["source_packet"].get("estimated_tokens") or 0)
        <= int(shard["budget"].get("max_prompt_tokens") or 0)
        for shard in shards
    )
