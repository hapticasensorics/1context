from __future__ import annotations

from pathlib import Path

from onectx.memory.wiki_validators import (
    validate_concept_page,
    validate_concern,
    validate_contradiction,
    validate_decided,
    validate_explicit_outcome_artifact,
    validate_proposal,
    validate_redaction_summary,
    validate_wiki_route_output,
)


def test_validators_accept_core_talk_artifact_families(tmp_path: Path) -> None:
    proposal = write(
        tmp_path / "2026-04-20T23-59Z.proposal.editor-day.md",
        """---
kind: proposal
author: claude-opus-daily-editor
ts: 2026-04-20T23:59:00Z
target-section: 2026-04-20
---

Proposed section body.
""",
    )
    decided = write(
        tmp_path / "2026-04-21T00-00Z.decided.editor-day.md",
        """---
kind: decided
author: claude-opus-curator
ts: 2026-04-21T00:00:00Z
parent: 2026-04-20T23-59Z.proposal.editor-day
---

<details class="opctx-talk-closure" open>
<summary><strong>Closed.</strong></summary>
Applied.
</details>
""",
    )
    concern = write(
        tmp_path / "2026-04-21T00-10Z.concern.voice.md",
        """---
kind: concern
author: claude-opus-curator
ts: 2026-04-21T00:10:00Z
---

Concern body.
""",
    )
    contradiction = write(
        tmp_path / "2026-04-21T00-20Z.contradiction.count.md",
        """---
kind: contradiction
author: claude-opus-contradiction-flagger
ts: 2026-04-21T00:20:00Z
---

Evidence shows a numerical mismatch between two entries.
""",
    )
    redacted = write(
        tmp_path / "2026-04-21T00-30Z.redacted.public.md",
        """---
kind: redacted
author: claude-opus-redactor
ts: 2026-04-21T00:30:00Z
parent: 2026-04-20.md
target: public
---

**Source:** `2026-04-20.md`
**Output:** `2026-04-20.public.md`
""",
    )

    assert validate_proposal(proposal)["ok"] is True
    assert validate_decided(decided)["ok"] is True
    assert validate_concern(concern)["ok"] is True
    assert validate_contradiction(contradiction)["ok"] is True
    assert validate_redaction_summary(redacted)["ok"] is True


def test_concept_validator_requires_schema_fields(tmp_path: Path) -> None:
    valid = write(
        tmp_path / "1context.md",
        """---
title: 1Context
slug: 1context
subject-type: project
categories: [Engineering]
---

## Current State

1Context is the memory layer.
""",
    )
    invalid = write(
        tmp_path / "bare.md",
        """---
title: Bare
---

No section heading.
""",
    )

    assert validate_concept_page(valid)["ok"] is True
    result = validate_concept_page(invalid)
    assert result["ok"] is False
    assert "frontmatter.slug is required for concept pages" in result["failures"]
    assert "concept body should include at least one section heading" in result["failures"]


def test_explicit_outcomes_are_valid_artifacts_not_missing_outputs(tmp_path: Path) -> None:
    path = write(
        tmp_path / "no-change.md",
        """---
outcome: no_change
reason: Existing proposal is already current.
---

No new article mutation needed.
""",
    )

    result = validate_explicit_outcome_artifact(path)
    assert result["ok"] is True
    assert result["outcome"] == "no_change"
    assert "explicit outcome recorded: no_change" in result["checks"]
    assert validate_wiki_route_output(path)["ok"] is True


def test_explicit_outcome_without_reason_fails_loud(tmp_path: Path) -> None:
    path = write(
        tmp_path / "silent-skip.md",
        """---
outcome: skip
---

short
""",
    )

    result = validate_explicit_outcome_artifact(path)
    assert result["ok"] is False
    assert "explicit outcome must include a reason in frontmatter or body" in result["failures"]


def test_route_output_can_validate_expected_kind(tmp_path: Path) -> None:
    path = write(
        tmp_path / "proposal.md",
        """---
kind: proposal
author: claude-opus
ts: 2026-04-21T00:00:00Z
---

Proposal body.
""",
    )

    assert validate_wiki_route_output(path, expected_kind="proposal")["ok"] is True
    result = validate_wiki_route_output(path, expected_kind="decided")
    assert result["ok"] is False
    assert "frontmatter.kind must be one of decided" in result["failures"]


def write(path: Path, text: str) -> Path:
    path.write_text(text, encoding="utf-8")
    return path
