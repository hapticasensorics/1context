from __future__ import annotations

import json
import socket
import threading
import uuid
from pathlib import Path

from onectx.wiki_interface.authoring import (
    append_talk_entry,
    promote_source_edit,
    request_wiki_refresh,
    write_decision,
    write_preview_render_request,
    write_promotion_receipt,
    write_proposal,
    write_route_plan,
)


def read_json(path: Path) -> dict:
    return json.loads(path.read_text())


def test_wiki_authoring_facade_writes_route_plan_records_and_preview(tmp_path: Path) -> None:
    runtime_home = tmp_path / "runtime"
    route_plan = write_route_plan(
        runtime_home,
        plan_id="plan.context-refresh",
        target_route="/your-context",
        owner="agent:context-curator",
        inputs=[{"path": "1Context/user-wiki/source/families/context/your-context/source/your-context.md", "sha256": "abc"}],
        validators=["frontmatter", "route-manifest", "operator-touched"],
        expected_outputs=["your-context.md", "your-context.html"],
        idempotency_key="context-refresh:abc",
        promotion_preconditions=["decision.accepted", "validator.clean"],
    )
    proposal = write_proposal(
        runtime_home,
        proposal_id="proposal.context-refresh",
        route_plan_id="plan.context-refresh",
        target_route="/your-context",
        title="Refresh context page",
        proposed_patch="@@ fixture @@",
        rationale="Fixture",
        provenance={"agent": "context-curator", "run_id": "run-1"},
    )
    decision = write_decision(
        runtime_home,
        decision_id="decision.context-refresh",
        proposal_id="proposal.context-refresh",
        status="accepted",
        decided_by="operator",
        rationale="Looks good",
    )
    receipt = write_promotion_receipt(
        runtime_home,
        receipt_id="receipt.context-refresh",
        proposal_id="proposal.context-refresh",
        source_path="1Context/user-wiki/source/families/context/your-context/source/your-context.md",
        template_path="1Context/user-wiki/templates/pages/e08/your-context.md",
        prompt_path="1Context/user-wiki/source/families/context/your-context/talk/your-context.talk/_curator.md",
        output_paths=["your-context.html", "your-context.md"],
        hashes={"your-context.md": "abc"},
    )
    preview = write_preview_render_request(
        runtime_home,
        preview_id="preview.context-refresh",
        route_plan_id="plan.context-refresh",
        source_paths=["1Context/user-wiki/source/families/context/your-context/source/your-context.md"],
    )

    route_payload = read_json(route_plan)
    assert route_payload["kind"] == "wiki.route_plan"
    assert route_payload["target"]["route"] == "/your-context"
    assert route_payload["input_hash"]
    assert read_json(proposal)["kind"] == "wiki.proposal"
    assert read_json(decision)["status"] == "accepted"
    assert read_json(receipt)["kind"] == "wiki.promotion_receipt"
    assert read_json(preview)["artifact_root"] == "context-engine/artifacts/wiki/previews/preview.context-refresh"
    assert not (runtime_home / "1Context/user-wiki/site").exists()


def test_wiki_authoring_facade_appends_schema_valid_talk_entry(tmp_path: Path) -> None:
    runtime_home = tmp_path / "runtime"
    entry = append_talk_entry(
        runtime_home,
        family_group="context",
        family_id="your-context",
        page_slug="your-context",
        page_id="your-context",
        page_route="/your-context",
        kind="proposal",
        title="Update Working Style",
        body="The curator proposes a focused edit.",
        author="agent:context-curator",
        provenance={"run_id": "run-1", "route_plan_id": "plan.context-refresh"},
        timestamp="2026-05-14T00-00-00Z",
    )

    text = entry.read_text()
    assert entry.name == "2026-05-14T00-00-00Z.proposal.update-working-style.md"
    assert 'schema_version: 1' in text
    assert 'page_id: "your-context"' in text
    assert 'page_route: "/your-context"' in text
    assert 'talk_for: "page://your-context"' in text
    assert "The curator proposes a focused edit." in text


def test_wiki_authoring_facade_promotes_source_edit_with_receipt(tmp_path: Path) -> None:
    runtime_home = tmp_path / "runtime"
    source = runtime_home / "1Context/user-wiki/source/families/reference/topics/source/topics.md"
    source.parent.mkdir(parents=True)
    before = "# Topics\n"
    source.write_text(before, encoding="utf-8")

    receipt = promote_source_edit(
        runtime_home,
        receipt_id="receipt.topics-proof",
        proposal_id="proposal.topics-proof",
        source_path="1Context/user-wiki/source/families/reference/topics/source/topics.md",
        expected_sha256="e0dc79773f307828c04a3569fc2ba4ddd06dcb8169a994d440e8a3efb77a156e",
        new_text="# Topics\n\n## Agent Proof\n\nAccepted note.\n",
        template_path="1Context/user-wiki/templates/pages/e08/topics.md",
        prompt_path="1Context/user-wiki/source/families/reference/topics/talk/topics.talk/_curator.md",
        output_paths=["topics.html", "topics.md"],
    )

    assert "Accepted note." in source.read_text(encoding="utf-8")
    payload = read_json(receipt)
    assert payload["kind"] == "wiki.promotion_receipt"
    assert payload["accepted_source"] == "1Context/user-wiki/source/families/reference/topics/source/topics.md"
    assert payload["hashes"]["before"] == "e0dc79773f307828c04a3569fc2ba4ddd06dcb8169a994d440e8a3efb77a156e"
    assert payload["hashes"]["after"]


def test_wiki_authoring_facade_requests_daemon_refresh_without_renderer(tmp_path: Path) -> None:
    socket_path = Path("/tmp") / f"1ctx-wiki-refresh-{uuid.uuid4().hex}.sock"
    ready = threading.Event()
    seen: list[dict] = []

    def serve() -> None:
        with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as server:
            server.bind(str(socket_path))
            server.listen(1)
            ready.set()
            conn, _ = server.accept()
            with conn:
                data = b""
                while not data.endswith(b"\n"):
                    data += conn.recv(4096)
                request = json.loads(data.decode())
                seen.append(request)
                response = {
                    "jsonrpc": "2.0",
                    "id": request["id"],
                    "result": {"health": "refreshing", "render": {"state": "refreshing"}},
                }
                conn.sendall((json.dumps(response) + "\n").encode())

    thread = threading.Thread(target=serve)
    thread.start()
    assert ready.wait(2)

    response = request_wiki_refresh(socket_path, request_id="refresh-1")
    thread.join(2)
    socket_path.unlink(missing_ok=True)

    assert seen[0]["method"] == "wiki.refresh"
    assert "wiki-engine" not in json.dumps(seen[0])
    assert response["result"]["render"]["state"] == "refreshing"
