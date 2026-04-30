from __future__ import annotations

import json
from dataclasses import replace
from pathlib import Path

from onectx.config import load_system
from onectx.memory.wiki_apply import (
    PROMOTION_APPROVAL_TOKEN,
    apply_curator_decision_to_sandbox,
    promote_wiki_apply_result_to_source,
    write_wiki_apply_promotion_result,
    write_wiki_apply_result,
)


def write_article(path: Path, *, operator_touched: bool = False) -> None:
    marker = "<!-- operator-touched: 2026-04-29 -->\n" if operator_touched else ""
    path.write_text(
        f"""---
title: Week of 2026-04-20
slug: 2026-04-20
---
# Week of 2026-04-20

<!-- section:"2026-04-20" -->
{marker}## Monday
Old Monday prose.

<!-- section:"2026-04-21" -->
## Tuesday
Old Tuesday prose.
""",
        encoding="utf-8",
    )


def write_rich_marker_article(path: Path) -> None:
    path.write_text(
        """---
title: Week of 2026-04-20
slug: 2026-04-20
section: product
access: private
---
# Week of 2026-04-20

<!-- section: { slug: "2026-04-20", talk: true, date: "2026-04-20" } -->
## Monday · 2026-04-20
Old Monday prose.

<!-- section: { slug: "2026-04-21", talk: true, date: "2026-04-21" } -->
## Tuesday · 2026-04-21
Old Tuesday prose.
""",
        encoding="utf-8",
    )


def write_decision(path: Path, section: str = "2026-04-20") -> None:
    path.write_text(
        f"""---
kind: decided
author: claude-opus-curator
ts: 2026-04-21T00:00:00Z
target-section: {section}
outcome: applied
---

## Replacement

```markdown
<!-- section:"{section}" -->
## Monday
New Monday prose from the approved editor proposal.
```
""",
        encoding="utf-8",
    )


def route_row(article: Path, sections: list[str] | None = None) -> dict[str, object]:
    return {
        "route_id": "route-for-you-curator",
        "job": "memory.wiki.for_you_curator",
        "ownership": {
            "kind": "article_sections",
            "path": str(article),
            "sections": sections or ["2026-04-20"],
        },
    }


def test_curator_apply_replaces_one_section_in_sandbox(tmp_path: Path) -> None:
    source = tmp_path / "source"
    source.mkdir()
    article = source / "2026-04-20.md"
    write_article(article)
    decision = tmp_path / "decision.md"
    write_decision(decision)
    original_source = article.read_text(encoding="utf-8")

    result = apply_curator_decision_to_sandbox(
        source_workspace=source,
        decision_path=decision,
        route_row=route_row(article),
        sandbox_root=tmp_path / "sandbox-run",
    )

    sandbox_article = result.sandbox_path.read_text(encoding="utf-8")
    assert result.status == "applied"
    assert result.ok is True
    assert article.read_text(encoding="utf-8") == original_source
    assert "New Monday prose from the approved editor proposal." in sandbox_article
    assert "Old Monday prose." not in sandbox_article
    assert "Old Tuesday prose." in sandbox_article
    assert '<!-- section:"2026-04-20" -->' in sandbox_article
    assert "## Monday" in sandbox_article
    assert result.diff["changed_paths"] == ["2026-04-20.md"]
    assert "source workspace unchanged" in result.checks


def test_curator_apply_refuses_operator_touched_section(tmp_path: Path) -> None:
    source = tmp_path / "source"
    source.mkdir()
    article = source / "2026-04-20.md"
    write_article(article, operator_touched=True)
    decision = tmp_path / "decision.md"
    write_decision(decision)
    original_source = article.read_text(encoding="utf-8")

    result = apply_curator_decision_to_sandbox(
        source_workspace=source,
        decision_path=decision,
        route_row=route_row(article),
        sandbox_root=tmp_path / "sandbox-run",
    )

    assert result.status == "needs_approval"
    assert result.ok is True
    assert article.read_text(encoding="utf-8") == original_source
    assert result.sandbox_path.read_text(encoding="utf-8") == original_source
    assert "operator-touched marker found; refusing mutation" in result.checks
    assert result.diff["changed_paths"] == []


def test_curator_apply_preserves_wiki_engine_rich_section_marker(tmp_path: Path) -> None:
    source = tmp_path / "source"
    source.mkdir()
    article = source / "2026-04-20.md"
    write_rich_marker_article(article)
    decision = tmp_path / "decision.md"
    write_decision(decision)

    result = apply_curator_decision_to_sandbox(
        source_workspace=source,
        decision_path=decision,
        route_row=route_row(article),
        sandbox_root=tmp_path / "sandbox-run",
    )

    sandbox_article = result.sandbox_path.read_text(encoding="utf-8")
    assert result.status == "applied"
    assert '<!-- section: { slug: "2026-04-20", talk: true, date: "2026-04-20" } -->' in sandbox_article
    assert "## Monday · 2026-04-20" in sandbox_article
    assert "New Monday prose from the approved editor proposal." in sandbox_article
    assert "Old Tuesday prose." in sandbox_article


def test_curator_apply_enforces_ownership_scope(tmp_path: Path) -> None:
    source = tmp_path / "source"
    source.mkdir()
    article = source / "2026-04-20.md"
    write_article(article)
    decision = tmp_path / "decision.md"
    write_decision(decision, section="2026-04-21")

    result = apply_curator_decision_to_sandbox(
        source_workspace=source,
        decision_path=decision,
        route_row=route_row(article, sections=["2026-04-20"]),
        sandbox_root=tmp_path / "sandbox-run",
    )

    assert result.status == "failed"
    assert result.ok is False
    assert "target section '2026-04-21' is outside ownership scope ['2026-04-20']" in result.failures


def test_wiki_apply_result_can_be_recorded(tmp_path: Path) -> None:
    system = replace(
        load_system(Path.cwd()),
        runtime_dir=tmp_path / "runtime",
        storage_dir=tmp_path / "lakestore",
    )
    source = tmp_path / "source"
    source.mkdir()
    article = source / "2026-04-20.md"
    write_article(article)
    decision = tmp_path / "decision.md"
    write_decision(decision)
    result = apply_curator_decision_to_sandbox(
        source_workspace=source,
        decision_path=decision,
        route_row=route_row(article),
        sandbox_root=tmp_path / "sandbox-run",
    )

    record = write_wiki_apply_result(system, result, run_id="wiki-apply-record")

    payload = json.loads(Path(record["path"]).read_text(encoding="utf-8"))
    assert payload["run_id"] == "wiki-apply-record"
    assert payload["status"] == "applied"
    assert record["artifact_id"]
    assert record["evidence_id"]


def test_curator_apply_can_promote_valid_sandbox_result_to_source_with_backup(tmp_path: Path) -> None:
    system = replace(
        load_system(Path.cwd()),
        runtime_dir=tmp_path / "runtime",
        storage_dir=tmp_path / "lakestore",
    )
    source = tmp_path / "source"
    source.mkdir()
    article = source / "2026-04-20.md"
    write_article(article)
    decision = tmp_path / "decision.md"
    write_decision(decision)
    original_source = article.read_text(encoding="utf-8")
    apply_result = apply_curator_decision_to_sandbox(
        source_workspace=source,
        decision_path=decision,
        route_row=route_row(article),
        sandbox_root=tmp_path / "sandbox-run",
    )

    promotion = promote_wiki_apply_result_to_source(
        system,
        apply_result,
        run_id="wiki-apply-promote",
        operator_approval=PROMOTION_APPROVAL_TOKEN,
    )
    record = write_wiki_apply_promotion_result(system, promotion, run_id="wiki-apply-promote")

    promoted_source = article.read_text(encoding="utf-8")
    assert promotion.status == "promoted"
    assert promotion.ok is True
    assert "New Monday prose from the approved editor proposal." in promoted_source
    assert "Old Monday prose." not in promoted_source
    assert "Old Tuesday prose." in promoted_source
    assert promotion.backup_path.read_text(encoding="utf-8") == original_source
    assert promotion.diff["changed_paths"] == ["2026-04-20.md"]
    assert record["status"] == "promoted"
    payload = json.loads(Path(record["path"]).read_text(encoding="utf-8"))
    assert payload["status"] == "promoted"
    assert "source content now matches validated sandbox content" in payload["checks"]


def test_curator_apply_promotion_requires_operator_gate(tmp_path: Path) -> None:
    system = replace(
        load_system(Path.cwd()),
        runtime_dir=tmp_path / "runtime",
        storage_dir=tmp_path / "lakestore",
    )
    source = tmp_path / "source"
    source.mkdir()
    article = source / "2026-04-20.md"
    write_article(article)
    decision = tmp_path / "decision.md"
    write_decision(decision)
    original_source = article.read_text(encoding="utf-8")
    apply_result = apply_curator_decision_to_sandbox(
        source_workspace=source,
        decision_path=decision,
        route_row=route_row(article),
        sandbox_root=tmp_path / "sandbox-run",
    )

    promotion = promote_wiki_apply_result_to_source(
        system,
        apply_result,
        run_id="wiki-apply-blocked",
        operator_approval="",
    )

    assert promotion.status == "blocked"
    assert promotion.ok is False
    assert article.read_text(encoding="utf-8") == original_source
    assert any("operator approval" in failure for failure in promotion.failures)
    assert not promotion.backup_path.exists()


def test_curator_apply_promotion_refuses_stale_source(tmp_path: Path) -> None:
    system = replace(
        load_system(Path.cwd()),
        runtime_dir=tmp_path / "runtime",
        storage_dir=tmp_path / "lakestore",
    )
    source = tmp_path / "source"
    source.mkdir()
    article = source / "2026-04-20.md"
    write_article(article)
    decision = tmp_path / "decision.md"
    write_decision(decision)
    apply_result = apply_curator_decision_to_sandbox(
        source_workspace=source,
        decision_path=decision,
        route_row=route_row(article),
        sandbox_root=tmp_path / "sandbox-run",
    )
    article.write_text(article.read_text(encoding="utf-8") + "\nOperator edit after sandbox.\n", encoding="utf-8")
    stale_source = article.read_text(encoding="utf-8")

    promotion = promote_wiki_apply_result_to_source(
        system,
        apply_result,
        run_id="wiki-apply-stale-source",
        operator_approval=PROMOTION_APPROVAL_TOKEN,
    )

    assert promotion.status == "blocked"
    assert article.read_text(encoding="utf-8") == stale_source
    assert "source content changed since sandbox apply" in promotion.failures
