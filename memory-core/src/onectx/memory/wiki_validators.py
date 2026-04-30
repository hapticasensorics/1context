from __future__ import annotations

import hashlib
import re
from dataclasses import dataclass
from pathlib import Path
from typing import Any


TALK_KINDS = {
    "archival-proposal",
    "archived",
    "conversation",
    "contradiction",
    "concern",
    "decided",
    "deferred",
    "fading",
    "proposal",
    "question",
    "redacted",
    "rfc",
    "synthesis",
    "verify",
    "merge",
    "split",
    "move",
    "cleanup",
    "reply",
}

EXPLICIT_OUTCOMES = {
    "already_current",
    "defer",
    "deferred",
    "forget",
    "needs_approval",
    "needs_retry",
    "needs_wider_context",
    "no_change",
    "no-talk",
    "skip",
    "skipped",
}

CONCEPT_REQUIRED_KEYS = ("title", "slug", "subject-type", "categories")


@dataclass(frozen=True)
class MarkdownArtifact:
    path: Path
    text: str
    frontmatter: dict[str, Any]
    body: str

    @property
    def sha256(self) -> str:
        return hashlib.sha256(self.text.encode("utf-8")).hexdigest()

    @property
    def bytes(self) -> int:
        return len(self.text.encode("utf-8"))


def validate_wiki_route_output(path: Path, *, expected_kind: str | tuple[str, ...] | None = None) -> dict[str, Any]:
    """Validate a generic hired-agent route artifact.

    This is intentionally structural. It proves the route produced an artifact
    or an explicit first-class quiet outcome; it does not judge prose quality.
    """
    artifact, checks, failures = load_markdown_artifact(path)
    if artifact is None:
        return validation_payload(path=path, checks=checks, failures=failures)

    add_non_empty_body_check(artifact, checks, failures)
    outcome = explicit_outcome(artifact)
    if outcome:
        validate_explicit_outcome(artifact, outcome, checks, failures)
    elif expected_kind:
        validate_talk_artifact(path, expected_kind=expected_kind, artifact=artifact, checks=checks, failures=failures)
    else:
        if artifact.frontmatter:
            checks.append("frontmatter block present")
        else:
            checks.append("frontmatter block not required for generic route output")
    return validation_payload(path=path, artifact=artifact, checks=checks, failures=failures, outcome=outcome)


def validate_proposal(path: Path) -> dict[str, Any]:
    return validate_talk_artifact(path, expected_kind="proposal")


def validate_decided(path: Path) -> dict[str, Any]:
    artifact, checks, failures = load_markdown_artifact(path)
    if artifact is None:
        return validation_payload(path=path, checks=checks, failures=failures)
    validate_talk_artifact(path, expected_kind=("decided", "deferred"), artifact=artifact, checks=checks, failures=failures)
    if artifact.frontmatter.get("parent"):
        checks.append("frontmatter.parent exists")
    else:
        failures.append("frontmatter.parent is required for decision entries")
    if "opctx-talk-closure" in artifact.body or "<details" in artifact.body:
        checks.append("decision body has closure block")
    else:
        failures.append("decision body should include a closure block")
    return validation_payload(path=path, artifact=artifact, checks=checks, failures=failures)


def validate_concern(path: Path) -> dict[str, Any]:
    return validate_talk_artifact(path, expected_kind="concern")


def validate_contradiction(path: Path) -> dict[str, Any]:
    artifact, checks, failures = load_markdown_artifact(path)
    if artifact is None:
        return validation_payload(path=path, checks=checks, failures=failures)
    validate_talk_artifact(path, expected_kind="contradiction", artifact=artifact, checks=checks, failures=failures)
    body_lower = artifact.body.lower()
    if any(marker in body_lower for marker in ("evidence", "conflict", "contradiction", "mismatch", "drift")):
        checks.append("contradiction body names evidence or conflict")
    else:
        failures.append("contradiction body should name evidence, conflict, mismatch, or drift")
    return validation_payload(path=path, artifact=artifact, checks=checks, failures=failures)


def validate_redaction_summary(path: Path) -> dict[str, Any]:
    artifact, checks, failures = load_markdown_artifact(path)
    if artifact is None:
        return validation_payload(path=path, checks=checks, failures=failures)
    validate_talk_artifact(path, expected_kind="redacted", artifact=artifact, checks=checks, failures=failures)
    if artifact.frontmatter.get("target"):
        checks.append("frontmatter.target exists")
    else:
        failures.append("frontmatter.target is required for redaction summaries")
    if "**Source:**" in artifact.body or "Source:" in artifact.body:
        checks.append("redaction summary names source")
    else:
        failures.append("redaction summary should name source")
    if "**Output:**" in artifact.body or "Output:" in artifact.body:
        checks.append("redaction summary names output")
    else:
        failures.append("redaction summary should name output")
    return validation_payload(path=path, artifact=artifact, checks=checks, failures=failures)


def validate_concept_page(path: Path) -> dict[str, Any]:
    artifact, checks, failures = load_markdown_artifact(path)
    if artifact is None:
        return validation_payload(path=path, checks=checks, failures=failures)
    add_non_empty_body_check(artifact, checks, failures)
    for key in CONCEPT_REQUIRED_KEYS:
        if artifact.frontmatter.get(key):
            checks.append(f"frontmatter.{key} exists")
        else:
            failures.append(f"frontmatter.{key} is required for concept pages")
    if re.search(r"^##\s+\S", artifact.body, flags=re.MULTILINE):
        checks.append("concept body has section heading")
    else:
        failures.append("concept body should include at least one section heading")
    return validation_payload(path=path, artifact=artifact, checks=checks, failures=failures)


def validate_explicit_outcome_artifact(path: Path) -> dict[str, Any]:
    artifact, checks, failures = load_markdown_artifact(path)
    if artifact is None:
        return validation_payload(path=path, checks=checks, failures=failures)
    add_non_empty_body_check(artifact, checks, failures)
    outcome = explicit_outcome(artifact)
    if outcome:
        validate_explicit_outcome(artifact, outcome, checks, failures)
    else:
        failures.append("explicit outcome artifact must declare outcome or kind as a known quiet outcome")
    return validation_payload(path=path, artifact=artifact, checks=checks, failures=failures, outcome=outcome)


def validate_talk_artifact(
    path: Path,
    *,
    expected_kind: str | tuple[str, ...],
    artifact: MarkdownArtifact | None = None,
    checks: list[str] | None = None,
    failures: list[str] | None = None,
) -> dict[str, Any]:
    own_lists = checks is None or failures is None
    checks = checks if checks is not None else []
    failures = failures if failures is not None else []
    if artifact is None:
        artifact, checks, failures = load_markdown_artifact(path)
        if artifact is None:
            return validation_payload(path=path, checks=checks, failures=failures)

    expected_kinds = (expected_kind,) if isinstance(expected_kind, str) else expected_kind
    if artifact.frontmatter:
        checks.append("frontmatter parses")
    else:
        failures.append("frontmatter missing or invalid")
    kind = str(artifact.frontmatter.get("kind") or "")
    if kind in expected_kinds:
        checks.append(f"frontmatter.kind in {', '.join(expected_kinds)}")
    else:
        failures.append(f"frontmatter.kind must be one of {', '.join(expected_kinds)}")
    if kind in TALK_KINDS:
        checks.append("frontmatter.kind is known talk kind")
    else:
        failures.append("frontmatter.kind is not a known talk kind")
    if artifact.frontmatter.get("author"):
        checks.append("frontmatter.author exists")
    else:
        failures.append("frontmatter.author is missing")
    if artifact.frontmatter.get("ts"):
        checks.append("frontmatter.ts exists")
    else:
        failures.append("frontmatter.ts is missing")
    add_non_empty_body_check(artifact, checks, failures)

    if own_lists:
        return validation_payload(path=path, artifact=artifact, checks=checks, failures=failures)
    return {"ok": not failures, "checks": checks, "failures": failures}


def load_markdown_artifact(path: Path) -> tuple[MarkdownArtifact | None, list[str], list[str]]:
    checks: list[str] = []
    failures: list[str] = []
    if not path.is_file():
        return None, checks, [f"missing file: {path}"]
    text = path.read_text(encoding="utf-8", errors="replace")
    if text.strip():
        checks.append("artifact non-empty")
    else:
        failures.append("artifact is empty")
    frontmatter, body = split_frontmatter(text)
    if frontmatter:
        checks.append("frontmatter block present")
    return MarkdownArtifact(path=path, text=text, frontmatter=frontmatter, body=body.strip()), checks, failures


def split_frontmatter(text: str) -> tuple[dict[str, Any], str]:
    if not text.startswith("---\n"):
        return {}, text
    end = text.find("\n---", 4)
    if end < 0:
        return {}, text
    return parse_frontmatter(text[4:end]), text[end + 4 :]


def parse_frontmatter(text: str) -> dict[str, Any]:
    result: dict[str, Any] = {}
    current_key: str | None = None
    for raw in text.splitlines():
        line = raw.rstrip()
        if not line.strip() or line.lstrip().startswith("#"):
            current_key = None
            continue
        if line.startswith("  - ") and current_key:
            result.setdefault(current_key, []).append(parse_scalar(line[4:].strip()))
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
            result[key] = [parse_scalar(item.strip()) for item in inner.split(",") if item.strip()]
            current_key = None
        else:
            result[key] = parse_scalar(value)
            current_key = None
    return result


def parse_scalar(value: str) -> Any:
    stripped = value.strip().strip("\"'")
    if stripped.lower() == "true":
        return True
    if stripped.lower() == "false":
        return False
    return stripped


def explicit_outcome(artifact: MarkdownArtifact) -> str:
    for key in ("outcome", "status", "kind"):
        value = str(artifact.frontmatter.get(key) or "").strip()
        if value in EXPLICIT_OUTCOMES:
            return value
    body = artifact.body.strip().splitlines()
    if body:
        first = body[0].strip().strip("[]").lower().replace(" ", "_")
        if first in EXPLICIT_OUTCOMES:
            return first
    return ""


def validate_explicit_outcome(
    artifact: MarkdownArtifact,
    outcome: str,
    checks: list[str],
    failures: list[str],
) -> None:
    checks.append(f"explicit outcome recorded: {outcome}")
    reason = str(artifact.frontmatter.get("reason") or artifact.frontmatter.get("summary") or "").strip()
    if reason or len(artifact.body.strip()) >= 20:
        checks.append("explicit outcome has reason")
    else:
        failures.append("explicit outcome must include a reason in frontmatter or body")


def add_non_empty_body_check(artifact: MarkdownArtifact, checks: list[str], failures: list[str]) -> None:
    if artifact.body.strip():
        checks.append("body non-empty")
    else:
        failures.append("body is empty")


def validation_payload(
    *,
    path: Path,
    checks: list[str],
    failures: list[str],
    artifact: MarkdownArtifact | None = None,
    outcome: str = "",
) -> dict[str, Any]:
    payload = {
        "ok": not failures,
        "checks": checks,
        "failures": failures,
        "path": str(path),
    }
    if artifact is not None:
        payload.update(
            {
                "bytes": artifact.bytes,
                "sha256": artifact.sha256,
                "frontmatter": artifact.frontmatter,
                "body_bytes": len(artifact.body.encode("utf-8")),
            }
        )
    if outcome:
        payload["outcome"] = outcome
    return payload
