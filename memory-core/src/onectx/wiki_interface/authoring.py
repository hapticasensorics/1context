from __future__ import annotations

import datetime as dt
import hashlib
import json
import re
import socket
from pathlib import Path
from typing import Any


SCHEMA_VERSION = 1
SAFE_ID_RE = re.compile(r"^[a-z0-9][a-z0-9._:-]{0,127}$")
SAFE_SLUG_RE = re.compile(r"^[a-z0-9][a-z0-9-]{0,79}$")


class WikiAuthoringError(RuntimeError):
    pass


def write_route_plan(
    runtime_home: Path,
    *,
    plan_id: str,
    target_route: str,
    owner: str,
    inputs: list[dict[str, Any]],
    validators: list[str],
    expected_outputs: list[str],
    idempotency_key: str,
    promotion_preconditions: list[str],
) -> Path:
    _validate_id(plan_id, "plan_id")
    _validate_route(target_route)
    payload = {
        "schema_version": SCHEMA_VERSION,
        "kind": "wiki.route_plan",
        "plan_id": plan_id,
        "target": {"route": target_route},
        "ownership": {"owner": owner},
        "inputs": inputs,
        "input_hash": _hash_json(inputs),
        "validators": validators,
        "expected_outputs": expected_outputs,
        "idempotency_key": idempotency_key,
        "promotion_preconditions": promotion_preconditions,
        "created_at": _now(),
    }
    return _write_json(_context_engine(runtime_home) / "artifacts/wiki/route-plans" / f"{plan_id}.json", payload)


def append_talk_entry(
    runtime_home: Path,
    *,
    family_group: str,
    family_id: str,
    page_slug: str,
    page_id: str,
    page_route: str,
    kind: str,
    title: str,
    body: str,
    author: str,
    provenance: dict[str, Any],
    timestamp: str | None = None,
) -> Path:
    for value, label in [
        (family_group, "family_group"),
        (family_id, "family_id"),
        (page_slug, "page_slug"),
        (kind, "kind"),
    ]:
        _validate_slug(value, label)
    _validate_id(page_id, "page_id")
    _validate_route(page_route)
    if not body.strip():
        raise WikiAuthoringError("talk entry body must not be empty")

    timestamp = timestamp or _now(compact=True)
    short = _slugify(title)[:48] or "entry"
    filename = f"{timestamp}.{kind}.{short}.md"
    talk_folder = (
        _user_wiki(runtime_home)
        / "source/families"
        / family_group
        / family_id
        / "talk"
        / f"{page_slug}.talk"
    )
    payload = {
        "schema_version": SCHEMA_VERSION,
        "kind": kind,
        "title": title,
        "author": author,
        "page_id": page_id,
        "page_route": page_route,
        "talk_for": f"page://{page_id}",
        "provenance": provenance,
        "created_at": timestamp,
    }
    text = _frontmatter(payload) + "\n" + body.rstrip() + "\n"
    path = talk_folder / filename
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8")
    return path


def write_proposal(
    runtime_home: Path,
    *,
    proposal_id: str,
    route_plan_id: str,
    target_route: str,
    title: str,
    proposed_patch: str,
    rationale: str,
    provenance: dict[str, Any],
) -> Path:
    _validate_id(proposal_id, "proposal_id")
    _validate_id(route_plan_id, "route_plan_id")
    _validate_route(target_route)
    payload = {
        "schema_version": SCHEMA_VERSION,
        "kind": "wiki.proposal",
        "proposal_id": proposal_id,
        "route_plan_id": route_plan_id,
        "target_route": target_route,
        "title": title,
        "status": "proposed",
        "proposed_patch": proposed_patch,
        "rationale": rationale,
        "provenance": provenance,
        "created_at": _now(),
    }
    return _write_json(_context_engine(runtime_home) / "proposals/wiki" / f"{proposal_id}.json", payload)


def write_decision(
    runtime_home: Path,
    *,
    decision_id: str,
    proposal_id: str,
    status: str,
    decided_by: str,
    rationale: str,
) -> Path:
    _validate_id(decision_id, "decision_id")
    _validate_id(proposal_id, "proposal_id")
    if status not in {"accepted", "rejected", "deferred", "needs_changes"}:
        raise WikiAuthoringError(f"invalid decision status: {status}")
    payload = {
        "schema_version": SCHEMA_VERSION,
        "kind": "wiki.decision",
        "decision_id": decision_id,
        "proposal_id": proposal_id,
        "status": status,
        "decided_by": decided_by,
        "rationale": rationale,
        "decided_at": _now(),
    }
    return _write_json(_context_engine(runtime_home) / "decisions/wiki" / f"{decision_id}.json", payload)


def write_promotion_receipt(
    runtime_home: Path,
    *,
    receipt_id: str,
    proposal_id: str,
    source_path: str,
    template_path: str,
    prompt_path: str,
    output_paths: list[str],
    hashes: dict[str, str],
) -> Path:
    _validate_id(receipt_id, "receipt_id")
    _validate_id(proposal_id, "proposal_id")
    payload = {
        "schema_version": SCHEMA_VERSION,
        "kind": "wiki.promotion_receipt",
        "receipt_id": receipt_id,
        "proposal_id": proposal_id,
        "accepted_source": source_path,
        "template": template_path,
        "prompt": prompt_path,
        "outputs": output_paths,
        "hashes": hashes,
        "promoted_at": _now(),
    }
    return _write_json(_context_engine(runtime_home) / "artifacts/wiki/promotion-receipts" / f"{receipt_id}.json", payload)


def write_preview_render_request(
    runtime_home: Path,
    *,
    preview_id: str,
    route_plan_id: str,
    source_paths: list[str],
) -> Path:
    _validate_id(preview_id, "preview_id")
    _validate_id(route_plan_id, "route_plan_id")
    payload = {
        "schema_version": SCHEMA_VERSION,
        "kind": "wiki.preview_render_request",
        "preview_id": preview_id,
        "route_plan_id": route_plan_id,
        "source_paths": source_paths,
        "artifact_root": f"context-engine/artifacts/wiki/previews/{preview_id}",
        "created_at": _now(),
    }
    return _write_json(_context_engine(runtime_home) / "artifacts/wiki/previews" / preview_id / "render-request.json", payload)


def promote_source_edit(
    runtime_home: Path,
    *,
    receipt_id: str,
    proposal_id: str,
    source_path: str,
    new_text: str,
    expected_sha256: str = "",
    template_path: str = "",
    prompt_path: str = "",
    output_paths: list[str] | None = None,
) -> Path:
    _validate_id(receipt_id, "receipt_id")
    _validate_id(proposal_id, "proposal_id")
    target = _resolve_user_path(runtime_home, source_path, label="source_path")
    before = target.read_text(encoding="utf-8") if target.exists() else ""
    before_hash = _hash_text(before)
    if expected_sha256 and expected_sha256 != before_hash:
        raise WikiAuthoringError("source file changed since proposal was written")
    if not new_text.strip():
        raise WikiAuthoringError("new source text must not be empty")

    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text(new_text, encoding="utf-8")
    return write_promotion_receipt(
        runtime_home,
        receipt_id=receipt_id,
        proposal_id=proposal_id,
        source_path=source_path,
        template_path=template_path,
        prompt_path=prompt_path,
        output_paths=output_paths or [],
        hashes={"before": before_hash, "after": _hash_text(new_text)},
    )


def request_wiki_refresh(socket_path: Path, *, request_id: str = "wiki-refresh") -> dict[str, Any]:
    request = {
        "jsonrpc": "2.0",
        "id": request_id,
        "method": "wiki.refresh",
        "params": {},
    }
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as client:
        client.connect(str(socket_path))
        client.sendall((json.dumps(request, sort_keys=True) + "\n").encode("utf-8"))
        response = _readline(client)
    return json.loads(response.decode("utf-8"))


def _readline(client: socket.socket) -> bytes:
    data = bytearray()
    while True:
        chunk = client.recv(1)
        if not chunk:
            break
        if chunk == b"\n":
            break
        data.extend(chunk)
    return bytes(data)


def _user_wiki(runtime_home: Path) -> Path:
    return runtime_home / "1Context/user-wiki"


def _context_engine(runtime_home: Path) -> Path:
    return runtime_home / "1Context/context-engine"


def _write_json(path: Path, payload: dict[str, Any]) -> Path:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    return path


def _hash_json(value: Any) -> str:
    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(",", ":")).encode("utf-8")).hexdigest()


def _hash_text(value: str) -> str:
    return hashlib.sha256(value.encode("utf-8")).hexdigest()


def _frontmatter(payload: dict[str, Any]) -> str:
    return "---\n" + "\n".join(f"{key}: {_yaml_value(value)}" for key, value in payload.items()) + "\n---\n"


def _yaml_value(value: Any) -> str:
    if isinstance(value, bool):
        return "true" if value else "false"
    if isinstance(value, int):
        return str(value)
    if isinstance(value, dict) or isinstance(value, list):
        return json.dumps(value, sort_keys=True)
    return json.dumps(str(value))


def _validate_id(value: str, label: str) -> None:
    if not SAFE_ID_RE.match(value):
        raise WikiAuthoringError(f"{label} is not a safe id: {value}")


def _validate_slug(value: str, label: str) -> None:
    if not SAFE_SLUG_RE.match(value):
        raise WikiAuthoringError(f"{label} is not a safe slug: {value}")


def _validate_route(value: str) -> None:
    if not value.startswith("/") or "//" in value or ".." in value or "\0" in value:
        raise WikiAuthoringError(f"invalid route: {value}")


def _resolve_user_path(runtime_home: Path, relative_path: str, *, label: str) -> Path:
    path = Path(relative_path)
    if path.is_absolute() or any(part in {"", ".", ".."} for part in path.parts):
        raise WikiAuthoringError(f"{label} must be a safe relative runtime path")
    allowed_roots = [("1Context", "user-wiki", "source"), ("1Context", "user-wiki", "wiki.toml")]
    parts = path.parts
    if parts[:3] != allowed_roots[0] and parts != allowed_roots[1]:
        raise WikiAuthoringError(f"{label} must target user-wiki source or wiki.toml")
    resolved = (runtime_home / path).resolve(strict=False)
    root = runtime_home.resolve(strict=False)
    try:
        resolved.relative_to(root)
    except ValueError as exc:
        raise WikiAuthoringError(f"{label} escapes runtime_home") from exc
    return resolved


def _slugify(value: str) -> str:
    return re.sub(r"[^a-z0-9-]+", "-", value.lower()).strip("-")


def _now(*, compact: bool = False) -> str:
    value = dt.datetime.now(dt.UTC).replace(microsecond=0).isoformat().replace("+00:00", "Z")
    if compact:
        return value.replace(":", "-")
    return value
