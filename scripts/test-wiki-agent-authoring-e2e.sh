#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
RUNTIME_HOME="$(mktemp -d /tmp/1ctx-wiki-agent-e2e-XXXXXX)"
SOCKET_PATH="/tmp/1ctx-wiki-agent-$USER-$$.sock"

cleanup() {
  rm -rf "$RUNTIME_HOME"
  rm -f "$SOCKET_PATH"
}
trap cleanup EXIT

"$ROOT/scripts/init-dev-wiki-runtime.sh" "$RUNTIME_HOME" >/tmp/1ctx-wiki-agent-init.out

uv run --project "$ROOT/memory-core" python - "$RUNTIME_HOME" "$SOCKET_PATH" <<'PY'
from __future__ import annotations

import hashlib
import json
import socket
import sys
import threading
from pathlib import Path

from onectx.wiki_interface.authoring import (
    append_talk_entry,
    promote_source_edit,
    request_wiki_refresh,
    write_decision,
    write_preview_render_request,
    write_proposal,
    write_route_plan,
)

runtime_home = Path(sys.argv[1])
socket_path = Path(sys.argv[2])
socket_path.unlink(missing_ok=True)
seen: list[dict] = []
ready = threading.Event()


def serve_once() -> None:
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as server:
        server.bind(str(socket_path))
        server.listen(1)
        ready.set()
        conn, _ = server.accept()
        with conn:
            data = b""
            while not data.endswith(b"\n"):
                data += conn.recv(4096)
            request = json.loads(data.decode("utf-8"))
            seen.append(request)
            response = {
                "jsonrpc": "2.0",
                "id": request["id"],
                "result": {"health": "refreshing", "render": {"state": "queued"}},
            }
            conn.sendall((json.dumps(response, sort_keys=True) + "\n").encode("utf-8"))


thread = threading.Thread(target=serve_once)
thread.start()
if not ready.wait(2):
    raise SystemExit("fake daemon socket did not start")

source = runtime_home / "1Context/user-wiki/source/families/reference/topics/source/topics.md"
source_rel = "1Context/user-wiki/source/families/reference/topics/source/topics.md"
before = source.read_text(encoding="utf-8")
before_hash = hashlib.sha256(before.encode("utf-8")).hexdigest()

route_plan = write_route_plan(
    runtime_home,
    plan_id="plan.agent-proof",
    target_route="/topics",
    owner="agent:wiki-proof",
    inputs=[{"path": source_rel, "sha256": before_hash}],
    validators=["frontmatter", "route-manifest", "operator-touched"],
    expected_outputs=["topics.html", "topics.md", "topics.talk.html"],
    idempotency_key=f"agent-proof:{before_hash}",
    promotion_preconditions=["decision.accepted", "validator.clean"],
)
talk_entry = append_talk_entry(
    runtime_home,
    family_group="reference",
    family_id="topics",
    page_slug="topics",
    page_id="topics",
    page_route="/topics",
    kind="proposal",
    title="Agent Proof Update",
    body="Agent proof proposal entry.",
    author="agent:wiki-proof",
    provenance={"route_plan": "plan.agent-proof", "source": source_rel},
    timestamp="2026-05-14T00-00-00Z",
)
proposal = write_proposal(
    runtime_home,
    proposal_id="proposal.agent-proof",
    route_plan_id="plan.agent-proof",
    target_route="/topics",
    title="Add agent proof section",
    proposed_patch="append section: Agent Proof",
    rationale="Deterministic fixture proving the Python wiki interface can promote source and request a refresh.",
    provenance={"talk_entry": str(talk_entry.relative_to(runtime_home))},
)
decision = write_decision(
    runtime_home,
    decision_id="decision.agent-proof",
    proposal_id="proposal.agent-proof",
    status="accepted",
    decided_by="operator-fixture",
    rationale="Accepted by deterministic e2e harness.",
)
write_preview_render_request(
    runtime_home,
    preview_id="preview.agent-proof",
    route_plan_id="plan.agent-proof",
    source_paths=[source_rel],
)

after = before.rstrip() + "\n\n## Agent Proof\n\nAgent proof accepted note.\n"
receipt = promote_source_edit(
    runtime_home,
    receipt_id="receipt.agent-proof",
    proposal_id="proposal.agent-proof",
    source_path=source_rel,
    new_text=after,
    expected_sha256=before_hash,
    template_path="1Context/user-wiki/templates/pages/e08/topics.md",
    prompt_path="1Context/user-wiki/source/families/reference/topics/talk/topics.talk/_curator.md",
    output_paths=["topics.html", "topics.md", "topics.talk.html"],
)
response = request_wiki_refresh(socket_path, request_id="agent-proof-refresh")
thread.join(2)

if seen[0]["method"] != "wiki.refresh":
    raise SystemExit("wiki interface did not request wiki.refresh")
if response["result"]["render"]["state"] != "queued":
    raise SystemExit("unexpected fake daemon response")

for path in [route_plan, talk_entry, proposal, decision, receipt]:
    if not path.exists():
        raise SystemExit(f"missing authoring artifact: {path}")
print("agent authoring artifacts written")
PY

node "$ROOT/wiki-engine/tools/render-site.mjs" \
  --source-root "$RUNTIME_HOME/1Context/user-wiki/source" \
  --output "$RUNTIME_HOME/1Context/user-wiki/site" \
  --result-json "$RUNTIME_HOME/render-agent.json" >/tmp/1ctx-wiki-agent-render.out

grep -q "Agent proof accepted note" "$RUNTIME_HOME/1Context/user-wiki/source/families/reference/topics/source/topics.md"
grep -q "Agent proof accepted note" "$RUNTIME_HOME/1Context/user-wiki/site/topics.html"
grep -q "Agent proof proposal entry" "$RUNTIME_HOME/1Context/user-wiki/site/topics.talk.html"
test -f "$RUNTIME_HOME/1Context/context-engine/proposals/wiki/proposal.agent-proof.json"
test -f "$RUNTIME_HOME/1Context/context-engine/decisions/wiki/decision.agent-proof.json"
test -f "$RUNTIME_HOME/1Context/context-engine/artifacts/wiki/promotion-receipts/receipt.agent-proof.json"
test -f "$RUNTIME_HOME/1Context/context-engine/artifacts/wiki/previews/preview.agent-proof/render-request.json"

echo "wiki agent authoring e2e proof passed."
