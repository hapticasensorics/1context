from __future__ import annotations

import hashlib
import json
import re
import shutil
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from onectx.config import MemorySystem
from onectx.memory.wiki_validators import load_markdown_artifact
from onectx.storage import LakeStore, stable_id, utc_now


class WikiApplyError(RuntimeError):
    """Raised when a curator apply request is structurally invalid."""


PROMOTION_APPROVAL_TOKEN = "promote-source"


@dataclass(frozen=True)
class SectionPatch:
    section: str
    start: int
    end: int
    before: str
    after: str
    operator_touched: bool

    def to_payload(self) -> dict[str, Any]:
        return {
            "section": self.section,
            "start": self.start,
            "end": self.end,
            "before_sha256": sha256_text(self.before),
            "after_sha256": sha256_text(self.after),
            "before_bytes": len(self.before.encode("utf-8")),
            "after_bytes": len(self.after.encode("utf-8")),
            "operator_touched": self.operator_touched,
        }


@dataclass(frozen=True)
class WikiApplyResult:
    status: str
    source_workspace: Path
    sandbox_workspace: Path
    source_path: Path
    sandbox_path: Path
    decision_path: Path
    section: str
    checks: tuple[str, ...]
    failures: tuple[str, ...]
    diff: dict[str, Any]
    patch: SectionPatch | None

    @property
    def ok(self) -> bool:
        return self.status in {"applied", "needs_approval", "defer", "no_change"}

    def to_payload(self) -> dict[str, Any]:
        return {
            "kind": "wiki_apply_result.v1",
            "status": self.status,
            "ok": self.ok,
            "source_workspace": str(self.source_workspace),
            "sandbox_workspace": str(self.sandbox_workspace),
            "source_path": str(self.source_path),
            "sandbox_path": str(self.sandbox_path),
            "decision_path": str(self.decision_path),
            "section": self.section,
            "checks": list(self.checks),
            "failures": list(self.failures),
            "diff": self.diff,
            "patch": self.patch.to_payload() if self.patch else {},
        }


@dataclass(frozen=True)
class WikiApplyPromotionResult:
    status: str
    source_workspace: Path
    sandbox_workspace: Path
    source_path: Path
    sandbox_path: Path
    backup_path: Path
    promotion_dir: Path
    decision_path: Path
    section: str
    checks: tuple[str, ...]
    failures: tuple[str, ...]
    diff: dict[str, Any]
    snapshot_paths: tuple[Path, ...]

    @property
    def ok(self) -> bool:
        return self.status == "promoted"

    def to_payload(self) -> dict[str, Any]:
        return {
            "kind": "wiki_apply_promotion_result.v1",
            "status": self.status,
            "ok": self.ok,
            "source_workspace": str(self.source_workspace),
            "sandbox_workspace": str(self.sandbox_workspace),
            "source_path": str(self.source_path),
            "sandbox_path": str(self.sandbox_path),
            "backup_path": str(self.backup_path),
            "promotion_dir": str(self.promotion_dir),
            "decision_path": str(self.decision_path),
            "section": self.section,
            "checks": list(self.checks),
            "failures": list(self.failures),
            "diff": self.diff,
            "snapshot_paths": [str(path) for path in self.snapshot_paths],
        }


def apply_curator_decision_to_sandbox(
    *,
    source_workspace: Path,
    decision_path: Path,
    route_row: dict[str, Any],
    sandbox_root: Path,
) -> WikiApplyResult:
    source_workspace = source_workspace.expanduser().resolve()
    decision_path = decision_path.expanduser().resolve()
    sandbox_root = sandbox_root.expanduser().resolve()
    if not source_workspace.is_dir():
        raise WikiApplyError(f"source workspace must be a directory: {source_workspace}")
    if not decision_path.is_file():
        raise WikiApplyError(f"decision artifact does not exist: {decision_path}")

    checks: list[str] = []
    failures: list[str] = []
    ownership = route_row.get("ownership") if isinstance(route_row.get("ownership"), dict) else {}
    target_source_path = resolve_owned_path(source_workspace, ownership)
    target_section = resolve_owned_section(decision_path, ownership)
    validate_ownership(ownership, target_source_path=target_source_path, source_workspace=source_workspace, section=target_section, checks=checks, failures=failures)

    sandbox_workspace = sandbox_root / source_workspace.name
    if sandbox_workspace.exists():
        raise WikiApplyError(f"sandbox workspace already exists: {sandbox_workspace}")
    sandbox_workspace.parent.mkdir(parents=True, exist_ok=True)
    shutil.copytree(source_workspace, sandbox_workspace)
    checks.append("source workspace copied to sandbox")

    relative_target = target_source_path.relative_to(source_workspace)
    target_sandbox_path = sandbox_workspace / relative_target
    if failures:
        return build_apply_result(
            status="failed",
            source_workspace=source_workspace,
            sandbox_workspace=sandbox_workspace,
            source_path=target_source_path,
            sandbox_path=target_sandbox_path,
            decision_path=decision_path,
            section=target_section,
            checks=checks,
            failures=failures,
            before_text=target_source_path.read_text(encoding="utf-8") if target_source_path.is_file() else "",
            after_text=target_sandbox_path.read_text(encoding="utf-8") if target_sandbox_path.is_file() else "",
            patch=None,
        )

    before_text = target_source_path.read_text(encoding="utf-8")
    sandbox_before_text = target_sandbox_path.read_text(encoding="utf-8")
    if before_text != sandbox_before_text:
        failures.append("sandbox target content does not match source after copy")
    section_span = find_section_span(sandbox_before_text, target_section)
    if section_span is None:
        failures.append(f"section {target_section!r} not found in target article")
        return build_apply_result(
            status="failed",
            source_workspace=source_workspace,
            sandbox_workspace=sandbox_workspace,
            source_path=target_source_path,
            sandbox_path=target_sandbox_path,
            decision_path=decision_path,
            section=target_section,
            checks=checks,
            failures=failures,
            before_text=before_text,
            after_text=sandbox_before_text,
            patch=None,
        )
    section_start, section_end = section_span
    section_block = sandbox_before_text[section_start:section_end]
    if section_has_operator_touched(sandbox_before_text, section_start, section_block):
        checks.append("operator-touched marker found; refusing mutation")
        return build_apply_result(
            status="needs_approval",
            source_workspace=source_workspace,
            sandbox_workspace=sandbox_workspace,
            source_path=target_source_path,
            sandbox_path=target_sandbox_path,
            decision_path=decision_path,
            section=target_section,
            checks=checks,
            failures=failures,
            before_text=before_text,
            after_text=sandbox_before_text,
            patch=SectionPatch(target_section, section_start, section_end, section_block, section_block, True),
        )

    replacement_body = replacement_body_from_decision(decision_path, target_section)
    if not replacement_body.strip():
        failures.append("decision artifact does not contain replacement body")
        status = "failed"
        patch = None
        after_text = sandbox_before_text
    else:
        new_section = replace_section_body(section_block, replacement_body, target_section)
        after_text = sandbox_before_text[:section_start] + new_section + sandbox_before_text[section_end:]
        if after_text == sandbox_before_text:
            status = "no_change"
            checks.append("replacement produced no content change")
        else:
            target_sandbox_path.write_text(after_text, encoding="utf-8")
            status = "applied"
            checks.append("sandbox article section replaced")
        patch = SectionPatch(target_section, section_start, section_end, section_block, new_section, False)

    source_after_text = target_source_path.read_text(encoding="utf-8")
    if source_after_text == before_text:
        checks.append("source workspace unchanged")
    else:
        failures.append("source workspace changed during sandbox apply")
        status = "failed"

    return build_apply_result(
        status=status,
        source_workspace=source_workspace,
        sandbox_workspace=sandbox_workspace,
        source_path=target_source_path,
        sandbox_path=target_sandbox_path,
        decision_path=decision_path,
        section=target_section,
        checks=checks,
        failures=failures,
        before_text=before_text,
        after_text=after_text,
        patch=patch,
    )


def promote_wiki_apply_result_to_source(
    system: MemorySystem,
    result: WikiApplyResult,
    *,
    run_id: str,
    operator_approval: str,
) -> WikiApplyPromotionResult:
    checks: list[str] = []
    failures: list[str] = []
    promotion_dir = system.runtime_dir / "wiki" / "apply-runs" / run_id / "promotion"
    promotion_dir.mkdir(parents=True, exist_ok=True)

    if operator_approval != PROMOTION_APPROVAL_TOKEN:
        failures.append(f"operator approval must exactly equal {PROMOTION_APPROVAL_TOKEN!r}")
    else:
        checks.append("operator promotion gate approved")

    if result.status != "applied":
        failures.append(f"sandbox apply status must be 'applied', got {result.status!r}")
    else:
        checks.append("sandbox apply status is promotable")
    if result.patch is None:
        failures.append("sandbox apply result has no section patch")
    elif result.patch.operator_touched:
        failures.append("operator-touched patch cannot be promoted")
    else:
        checks.append("sandbox patch is not operator-touched")

    if not has_apply_ownership_checks(result):
        failures.append("sandbox apply result does not carry ownership validation checks")
    else:
        checks.append("sandbox apply result carries ownership validation checks")

    source_path = result.source_path.expanduser().resolve()
    sandbox_path = result.sandbox_path.expanduser().resolve()
    source_workspace = result.source_workspace.expanduser().resolve()
    sandbox_workspace = result.sandbox_workspace.expanduser().resolve()
    relative_source = safe_relative_path(source_path, source_workspace)
    relative_sandbox = safe_relative_path(sandbox_path, sandbox_workspace)
    if relative_source is None:
        failures.append("source path must stay inside source workspace")
        relative_source = Path(source_path.name)
    if relative_sandbox is None:
        failures.append("sandbox path must stay inside sandbox workspace")
        relative_sandbox = Path(sandbox_path.name)
    if relative_source != relative_sandbox:
        failures.append(f"sandbox target {relative_sandbox.as_posix()} does not match source target {relative_source.as_posix()}")
    changed_paths = result.diff.get("changed_paths")
    if changed_paths != [relative_source.as_posix()]:
        failures.append(f"sandbox diff must change exactly {relative_source.as_posix()!r}, got {changed_paths!r}")
    else:
        checks.append("sandbox diff scope matches owned source path")

    source_text = source_path.read_text(encoding="utf-8") if source_path.is_file() else ""
    sandbox_text = sandbox_path.read_text(encoding="utf-8") if sandbox_path.is_file() else ""
    source_before_sha = sha256_text(source_text)
    sandbox_after_sha = sha256_text(sandbox_text)
    expected_before_sha = str(result.diff.get("before_sha256") or "")
    expected_after_sha = str(result.diff.get("after_sha256") or "")
    if expected_before_sha and source_before_sha != expected_before_sha:
        failures.append("source content changed since sandbox apply")
    else:
        checks.append("source content still matches sandbox apply preimage")
    if expected_after_sha and sandbox_after_sha != expected_after_sha:
        failures.append("sandbox content changed since sandbox apply")
    else:
        checks.append("sandbox content still matches sandbox apply postimage")

    backup_path = promotion_dir / "source-backup" / relative_source
    source_before_snapshot = promotion_dir / "source-before" / relative_source
    sandbox_after_snapshot = promotion_dir / "sandbox-after" / relative_source
    source_after_snapshot = promotion_dir / "source-after" / relative_source
    snapshot_paths = (backup_path, source_before_snapshot, sandbox_after_snapshot, source_after_snapshot)

    if failures:
        return build_promotion_result(
            status="blocked",
            source_workspace=source_workspace,
            sandbox_workspace=sandbox_workspace,
            source_path=source_path,
            sandbox_path=sandbox_path,
            backup_path=backup_path,
            promotion_dir=promotion_dir,
            decision_path=result.decision_path,
            section=result.section,
            checks=checks,
            failures=failures,
            before_text=source_text,
            after_text=source_text,
            sandbox_text=sandbox_text,
            snapshot_paths=(),
        )

    for path in (backup_path, source_before_snapshot, sandbox_after_snapshot):
        path.parent.mkdir(parents=True, exist_ok=True)
    backup_path.write_text(source_text, encoding="utf-8")
    source_before_snapshot.write_text(source_text, encoding="utf-8")
    sandbox_after_snapshot.write_text(sandbox_text, encoding="utf-8")
    checks.append("source backup and pre/post snapshots written")

    source_path.write_text(sandbox_text, encoding="utf-8")
    source_after_text = source_path.read_text(encoding="utf-8")
    source_after_snapshot.parent.mkdir(parents=True, exist_ok=True)
    source_after_snapshot.write_text(source_after_text, encoding="utf-8")
    if source_after_text != sandbox_text:
        failures.append("source content does not match sandbox content after promotion")
        status = "failed"
    else:
        checks.append("source content now matches validated sandbox content")
        status = "promoted"

    return build_promotion_result(
        status=status,
        source_workspace=source_workspace,
        sandbox_workspace=sandbox_workspace,
        source_path=source_path,
        sandbox_path=sandbox_path,
        backup_path=backup_path,
        promotion_dir=promotion_dir,
        decision_path=result.decision_path,
        section=result.section,
        checks=checks,
        failures=failures,
        before_text=source_text,
        after_text=source_after_text,
        sandbox_text=sandbox_text,
        snapshot_paths=snapshot_paths,
    )


def write_wiki_apply_result(system: MemorySystem, result: WikiApplyResult, *, run_id: str = "") -> dict[str, Any]:
    resolved_run_id = run_id or stable_id("wiki_apply", str(result.sandbox_path), result.section, utc_now())
    out_dir = system.runtime_dir / "wiki" / "apply-runs" / resolved_run_id
    out_dir.mkdir(parents=True, exist_ok=True)
    path = out_dir / "apply-result.json"
    payload = result.to_payload()
    payload["run_id"] = resolved_run_id
    text = json.dumps(payload, indent=2, sort_keys=True, default=str) + "\n"
    path.write_text(text, encoding="utf-8")
    content_hash = sha256_text(text)

    store = LakeStore(system.storage_dir)
    store.ensure()
    artifact = store.append_artifact(
        "wiki_apply_result",
        uri=path.as_uri(),
        path=str(path),
        content_type="application/json",
        content_hash=content_hash,
        bytes=path.stat().st_size,
        source="memory.wiki_apply",
        state=result.status,
        text=f"wiki apply {resolved_run_id}: {result.status}",
        metadata={
            "run_id": resolved_run_id,
            "section": result.section,
            "status": result.status,
            "source_workspace": str(result.source_workspace),
            "sandbox_workspace": str(result.sandbox_workspace),
        },
    )
    evidence = store.append_evidence(
        "wiki_apply.sandbox_checked",
        artifact_id=artifact["artifact_id"],
        status="passed" if result.ok else "failed",
        checker="memory.wiki_apply",
        text="curator apply sandbox result recorded",
        checks=result.checks,
        payload=payload,
    )
    event = store.append_event(
        "memory.wiki_apply.completed",
        source="memory.wiki_apply",
        kind="wiki_apply",
        subject=resolved_run_id,
        artifact_id=artifact["artifact_id"],
        evidence_id=evidence["evidence_id"],
        payload={"run_id": resolved_run_id, "status": result.status, "section": result.section},
    )
    return {
        "run_id": resolved_run_id,
        "path": str(path),
        "artifact_id": artifact["artifact_id"],
        "evidence_id": evidence["evidence_id"],
        "event_id": event["event_id"],
        "status": result.status,
    }


def write_wiki_apply_promotion_result(
    system: MemorySystem,
    result: WikiApplyPromotionResult,
    *,
    run_id: str,
) -> dict[str, Any]:
    out_dir = system.runtime_dir / "wiki" / "apply-runs" / run_id
    out_dir.mkdir(parents=True, exist_ok=True)
    path = out_dir / "promotion-result.json"
    payload = result.to_payload()
    payload["run_id"] = run_id
    text = json.dumps(payload, indent=2, sort_keys=True, default=str) + "\n"
    path.write_text(text, encoding="utf-8")
    content_hash = sha256_text(text)

    store = LakeStore(system.storage_dir)
    store.ensure()
    artifact = store.append_artifact(
        "wiki_apply_promotion_result",
        uri=path.as_uri(),
        path=str(path),
        content_type="application/json",
        content_hash=content_hash,
        bytes=path.stat().st_size,
        source="memory.wiki_apply",
        state=result.status,
        text=f"wiki apply promotion {run_id}: {result.status}",
        metadata={
            "run_id": run_id,
            "section": result.section,
            "status": result.status,
            "source_workspace": str(result.source_workspace),
            "sandbox_workspace": str(result.sandbox_workspace),
            "backup_path": str(result.backup_path),
        },
    )
    evidence = store.append_evidence(
        "wiki_apply.source_promotion",
        artifact_id=artifact["artifact_id"],
        status="passed" if result.ok else "failed",
        checker="memory.wiki_apply",
        text="curator apply source promotion result recorded",
        checks=result.checks,
        payload=payload,
    )
    event = store.append_event(
        "memory.wiki_apply.promoted",
        source="memory.wiki_apply",
        kind="wiki_apply_promotion",
        subject=run_id,
        artifact_id=artifact["artifact_id"],
        evidence_id=evidence["evidence_id"],
        payload={"run_id": run_id, "status": result.status, "section": result.section},
    )
    return {
        "run_id": run_id,
        "path": str(path),
        "artifact_id": artifact["artifact_id"],
        "evidence_id": evidence["evidence_id"],
        "event_id": event["event_id"],
        "status": result.status,
    }


def resolve_owned_path(source_workspace: Path, ownership: dict[str, Any]) -> Path:
    raw = str(ownership.get("path") or "").strip()
    if not raw:
        raise WikiApplyError("route ownership.path is required")
    path = Path(raw).expanduser()
    if not path.is_absolute():
        path = source_workspace / path
    return path.resolve()


def resolve_owned_section(decision_path: Path, ownership: dict[str, Any]) -> str:
    artifact, _checks, failures = load_markdown_artifact(decision_path)
    if failures:
        raise WikiApplyError("; ".join(failures))
    if artifact is None:
        raise WikiApplyError(f"decision artifact could not be loaded: {decision_path}")
    section = str(
        artifact.frontmatter.get("target-section")
        or artifact.frontmatter.get("target_section")
        or artifact.frontmatter.get("section")
        or ownership.get("section")
        or ""
    ).strip()
    if not section:
        sections = ownership.get("sections")
        if isinstance(sections, list) and len(sections) == 1:
            section = str(sections[0]).strip()
    if not section:
        raise WikiApplyError("target section is required in decision frontmatter or ownership")
    return section


def validate_ownership(
    ownership: dict[str, Any],
    *,
    target_source_path: Path,
    source_workspace: Path,
    section: str,
    checks: list[str],
    failures: list[str],
) -> None:
    kind = str(ownership.get("kind") or "").strip()
    if kind in {"article_sections", "article_section"}:
        checks.append(f"ownership.kind allows article section apply: {kind}")
    else:
        failures.append(f"ownership.kind must be article_sections or article_section, got {kind or 'missing'}")
    if source_workspace not in (target_source_path, *target_source_path.parents):
        failures.append("ownership.path must stay inside source workspace")
    elif target_source_path.is_file():
        checks.append("ownership.path exists inside source workspace")
    else:
        failures.append(f"ownership.path does not exist: {target_source_path}")

    allowed = ownership.get("sections")
    if allowed is None:
        allowed = [ownership.get("section")] if ownership.get("section") else []
    if isinstance(allowed, str):
        allowed_sections = {allowed}
    elif isinstance(allowed, list):
        allowed_sections = {str(item) for item in allowed}
    else:
        allowed_sections = set()
    if not allowed_sections or section in allowed_sections:
        checks.append("target section is within ownership scope")
    else:
        failures.append(f"target section {section!r} is outside ownership scope {sorted(allowed_sections)}")


def find_section_span(text: str, section: str) -> tuple[int, int] | None:
    section_re = re.compile(r'<!--\s*section:[^>]*?"([^"]+)"[^>]*-->')
    markers = list(section_re.finditer(text))
    for index, marker in enumerate(markers):
        if marker.group(1) != section:
            continue
        end = markers[index + 1].start() if index + 1 < len(markers) else len(text)
        return marker.start(), end
    return None


def replacement_body_from_decision(decision_path: Path, section: str) -> str:
    artifact, _checks, failures = load_markdown_artifact(decision_path)
    if failures or artifact is None:
        raise WikiApplyError("; ".join(failures) or f"decision artifact could not be loaded: {decision_path}")
    body = artifact.body.strip()
    fenced = re.search(r"```(?:markdown|md)?\s*\n(.*?)\n```", body, flags=re.DOTALL | re.IGNORECASE)
    if fenced:
        body = fenced.group(1).strip()
    replacement_heading = re.search(r"^##\s+Replacement\s*$", body, flags=re.MULTILINE | re.IGNORECASE)
    if replacement_heading:
        body = body[replacement_heading.end() :].strip()

    span = find_section_span(body, section)
    if span is not None:
        body = body[span[0] : span[1]].strip()
    body = re.sub(r'^\s*<!--\s*section:[^>]*?"[^"]+"[^>]*-->\s*', "", body, count=1, flags=re.DOTALL)
    body = re.sub(r"^\s*##\s+.+?\n", "", body, count=1, flags=re.DOTALL).strip()
    return body


def replace_section_body(section_block: str, replacement_body: str, section: str) -> str:
    heading = re.search(r"^##\s+.+?$", section_block, flags=re.MULTILINE)
    if heading is None:
        raise WikiApplyError(f"section {section!r} does not have an H2 heading to preserve")
    prefix = section_block[: heading.end()].rstrip()
    return f"{prefix}\n{replacement_body.strip()}\n"


def section_has_operator_touched(text: str, section_start: int, section_block: str) -> bool:
    if "operator-touched:" in section_block:
        return True
    prefix = text[:section_start].splitlines()
    for line in reversed(prefix[-3:]):
        if not line.strip():
            continue
        return "operator-touched:" in line
    return False


def build_apply_result(
    *,
    status: str,
    source_workspace: Path,
    sandbox_workspace: Path,
    source_path: Path,
    sandbox_path: Path,
    decision_path: Path,
    section: str,
    checks: list[str],
    failures: list[str],
    before_text: str,
    after_text: str,
    patch: SectionPatch | None,
) -> WikiApplyResult:
    diff = {
        "source_unchanged": source_path.read_text(encoding="utf-8") == before_text if source_path.is_file() else False,
        "changed_paths": [sandbox_path.relative_to(sandbox_workspace).as_posix()] if before_text != after_text else [],
        "before_sha256": sha256_text(before_text),
        "after_sha256": sha256_text(after_text),
        "before_bytes": len(before_text.encode("utf-8")),
        "after_bytes": len(after_text.encode("utf-8")),
    }
    if failures and status not in {"needs_approval", "defer"}:
        status = "failed"
    return WikiApplyResult(
        status=status,
        source_workspace=source_workspace,
        sandbox_workspace=sandbox_workspace,
        source_path=source_path,
        sandbox_path=sandbox_path,
        decision_path=decision_path,
        section=section,
        checks=tuple(checks),
        failures=tuple(failures),
        diff=diff,
        patch=patch,
    )


def build_promotion_result(
    *,
    status: str,
    source_workspace: Path,
    sandbox_workspace: Path,
    source_path: Path,
    sandbox_path: Path,
    backup_path: Path,
    promotion_dir: Path,
    decision_path: Path,
    section: str,
    checks: list[str],
    failures: list[str],
    before_text: str,
    after_text: str,
    sandbox_text: str,
    snapshot_paths: tuple[Path, ...],
) -> WikiApplyPromotionResult:
    diff = {
        "changed_paths": [relative_source_for_payload(source_path, source_workspace)] if before_text != after_text else [],
        "before_sha256": sha256_text(before_text),
        "after_sha256": sha256_text(after_text),
        "sandbox_sha256": sha256_text(sandbox_text),
        "before_bytes": len(before_text.encode("utf-8")),
        "after_bytes": len(after_text.encode("utf-8")),
        "sandbox_bytes": len(sandbox_text.encode("utf-8")),
    }
    if failures and status == "promoted":
        status = "failed"
    return WikiApplyPromotionResult(
        status=status,
        source_workspace=source_workspace,
        sandbox_workspace=sandbox_workspace,
        source_path=source_path,
        sandbox_path=sandbox_path,
        backup_path=backup_path,
        promotion_dir=promotion_dir,
        decision_path=decision_path,
        section=section,
        checks=tuple(checks),
        failures=tuple(failures),
        diff=diff,
        snapshot_paths=snapshot_paths,
    )


def has_apply_ownership_checks(result: WikiApplyResult) -> bool:
    return any(check.startswith("ownership.kind allows") for check in result.checks) and (
        "target section is within ownership scope" in result.checks
    )


def safe_relative_path(path: Path, base: Path) -> Path | None:
    try:
        return path.relative_to(base)
    except ValueError:
        return None


def relative_source_for_payload(path: Path, base: Path) -> str:
    relative = safe_relative_path(path, base)
    return relative.as_posix() if relative is not None else path.name


def sha256_text(text: str) -> str:
    return hashlib.sha256(text.encode("utf-8")).hexdigest()
