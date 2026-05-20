from __future__ import annotations

import json
import shutil
from pathlib import Path

import pytest

from onectx.wiki_interface import (
    WikiCoreClient,
    WikiCoreError,
    resolve_wiki_core_binary,
    wiki_agent_claim,
    wiki_agent_identify,
    wiki_agent_inbox,
    wiki_ensure,
    wiki_list,
    wiki_list_create,
    wiki_list_members,
    wiki_list_status,
    wiki_lists,
    wiki_mail_inbox,
    wiki_mail_mark_all,
    wiki_mail_read,
    wiki_mail_subscribe,
    wiki_mail_subscriptions,
    wiki_mail_unsubscribe,
    wiki_notify_ack,
    wiki_notify_poll,
    wiki_page_assign_role,
    wiki_page_create,
    wiki_page_create_all,
    wiki_page_delete,
    wiki_page_open,
    wiki_page_patch_body,
    wiki_page_restore,
    wiki_page_status,
    wiki_page_unwatch,
    wiki_page_watch,
    wiki_page_write_body,
    wiki_publish,
    wiki_publish_status,
    wiki_status,
    wiki_talk_append,
    wiki_validate,
)


def test_wiki_publish_helper_passes_node_argument(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    seen: dict[str, object] = {}

    def fake_publish(
        self: WikiCoreClient,
        *,
        wiki_engine: Path | str | None = None,
        trigger: str = "agent",
        force: bool = False,
        node: str | None = None,
    ) -> dict[str, object]:
        seen.update(
            runtime_home=self.runtime_home,
            wiki_engine=wiki_engine,
            trigger=trigger,
            force=force,
            node=node,
        )
        return {"operation": "wiki.publish", "status": "published"}

    monkeypatch.setattr(WikiCoreClient, "publish", fake_publish)

    receipt = wiki_publish(
        tmp_path,
        wiki_engine="wiki-engine",
        trigger="probe",
        force=True,
        node="node",
    )

    assert receipt["operation"] == "wiki.publish"
    assert seen == {
        "runtime_home": tmp_path,
        "wiki_engine": "wiki-engine",
        "trigger": "probe",
        "force": True,
        "node": "node",
    }


def test_body_helpers_support_file_backed_inputs(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    calls: list[tuple[str, ...]] = []

    def fake_call(self: WikiCoreClient, *args: str) -> dict[str, object]:
        calls.append(args)
        return {"operation": "fake", "args": list(args)}

    monkeypatch.setattr(WikiCoreClient, "call", fake_call)

    body_file = tmp_path / "body.md"
    talk_body_file = tmp_path / "talk-body.md"
    find_file = tmp_path / "find.md"
    replace_file = tmp_path / "replace.md"
    client = WikiCoreClient(runtime_home=tmp_path)

    client.page_write_body(
        "worker-bb",
        body_file=body_file,
        expected_source_sha256="source-before",
    )
    assert calls[-1] == (
        "page-write-body",
        "worker-bb",
        "--body-file",
        str(body_file),
        "--expected-source-sha256",
        "source-before",
    )

    wiki_page_patch_body(
        tmp_path,
        "worker-bb",
        find_file=find_file,
        replace_file=replace_file,
        expected_source_sha256="source-after",
    )
    assert calls[-1] == (
        "page-patch-body",
        "worker-bb",
        "--find-file",
        str(find_file),
        "--replace-file",
        str(replace_file),
        "--expected-source-sha256",
        "source-after",
    )

    client.talk_append(
        page="worker-bb",
        kind="proposal",
        subject="File backed talk",
        from_address="agent://worker-be",
        to=["agent://reviewer"],
        body_file=talk_body_file,
    )
    assert calls[-1] == (
        "talk-append",
        "--page",
        "worker-bb",
        "--kind",
        "proposal",
        "--subject",
        "File backed talk",
        "--from",
        "agent://worker-be",
        "--to",
        "agent://reviewer",
        "--body-file",
        str(talk_body_file),
    )

    with pytest.raises(ValueError, match="either --body or --body-file"):
        client.page_write_body("worker-bb", body_markdown="inline", body_file=body_file)
    with pytest.raises(ValueError, match="requires --find or --find-file"):
        client.page_patch_body("worker-bb", replace="replacement")
    with pytest.raises(ValueError, match="either --replace or --replace-file"):
        client.page_patch_body(
            "worker-bb",
            find="needle",
            replace="inline",
            replace_file=replace_file,
        )
    with pytest.raises(ValueError, match="either --body or --body-file"):
        client.talk_append(
            page="worker-bb",
            subject="Ambiguous talk body",
            from_address="agent://worker-be",
            to=["agent://reviewer"],
            body_markdown="inline",
            body_file=talk_body_file,
        )
    with pytest.raises(ValueError, match="requires --body or --body-file"):
        client.talk_append(
            page="worker-bb",
            subject="Missing talk body",
            from_address="agent://worker-be",
            to=["agent://reviewer"],
        )


def test_wiki_publish_backfills_missing_configured_pages(tmp_path: Path) -> None:
    try:
        binary = resolve_wiki_core_binary()
    except WikiCoreError as exc:
        pytest.skip(str(exc))

    repo_root = Path(__file__).resolve().parents[2]
    shutil.copytree(repo_root / "runtime/1Context", tmp_path / "1Context")

    client = WikiCoreClient(runtime_home=tmp_path, binary=binary, timeout_seconds=180)
    assert client.ensure()["status"] == "ok"

    before = client.publish_status()
    assert before["operation"] == "wiki.publish.status"
    assert before["next_action"] == "publish"
    assert set(before["pages_missing_source"]) >= {"for-you", "your-context", "projects", "topics"}

    receipt = client.publish(wiki_engine=repo_root / "wiki-engine", trigger="pytest-backfill")
    assert receipt["operation"] == "wiki.publish"
    assert receipt["status"] == "published"
    assert receipt["before"]["next_action"] == "publish"
    assert receipt["render_input"]["next_action"] == "publish"
    assert set(receipt["render_input"]["pages_needing_publish"]) >= {
        "for-you",
        "your-context",
        "projects",
        "topics",
    }
    assert receipt["preflight"][0]["operation"] == "wiki.publish.preflight"
    assert receipt["preflight"][0]["action"] == "backfill_configured_pages"
    assert receipt["preflight"][0]["reason"] == "publish_missing_configured_pages"
    assert receipt["preflight"][0]["result"]["operation"] == "wiki.publish.preflight"
    assert receipt["after"]["next_action"] == "none"
    assert receipt["after"]["render_required"] is False

    listing = client.list_pages()
    pages = {page["id"]: page for page in listing["pages"]}
    for page_id in ("for-you", "your-context", "projects", "topics"):
        page = pages[page_id]
        assert page["state"] == "rendered"
        assert page["talk_state"] == "ready"
        assert page["flags"]["source_backed"] is True
        assert page["flags"]["rendered"] is True
        assert page["flags"]["template_derived"] is True
        assert page["next_action"] == "none"


def test_python_agent_adapter_full_lifecycle_keeps_publish_pressure_content_scoped(
    tmp_path: Path,
) -> None:
    try:
        binary = resolve_wiki_core_binary()
    except WikiCoreError as exc:
        pytest.skip(str(exc))

    repo_root = Path(__file__).resolve().parents[2]
    shutil.copytree(repo_root / "runtime/1Context", tmp_path / "1Context")
    route_manifest = tmp_path / "1Context/user-wiki/site/.1context/route-manifest.json"

    def manifest_routes() -> set[str]:
        return {
            route["route"]
            for route in json.loads(route_manifest.read_text(encoding="utf-8"))["routes"]
        }

    client = WikiCoreClient(runtime_home=tmp_path, binary=binary, timeout_seconds=180)
    assert client.ensure()["status"] == "ok"

    agent = client.agent_register(
        thread_id="worker-as-python-agent",
        roles=["role://worker-as-python.curator"],
        capabilities=["wiki.mail"],
    )
    assert agent["operation"] == "wiki.agent.register"
    assert agent["status"] == "registered"
    assert agent["primary_address"] == "agent://codex/worker-as-python-agent"

    created = client.page_create(
        "worker-as-python",
        title="Worker AS Python",
        route="/agent-lab/worker-as-python",
        family_group="80-agent-lab",
        family_group_title="Agent Lab",
        family_id="10-worker-as-python",
        family_title="Worker AS Python",
        nav_section="utility",
        nav_order=88,
        summary="Worker AS dogfoods the Python wiki adapter.",
    )
    assert created["operation"] == "wiki.page.create"
    assert created["status"] == "ok"
    assert created["page_status"]["route"] == "/agent-lab/worker-as-python"
    assert created["page_status"]["nav_section"] == "utility"
    assert created["page_status"]["next_action"] == "publish"
    assert created["hashes"]["source_sha256"]

    opened = client.page_open("worker-as-python")
    assert opened["operation"] == "wiki.page.open"
    assert opened["id"] == "worker-as-python"
    assert opened["title"] == "Worker AS Python"
    assert opened["route"] == "/agent-lab/worker-as-python"
    assert opened["collection"] == "80-agent-lab"
    assert opened["type"] == "context-page"
    assert opened["page_status"]["route"] == "/agent-lab/worker-as-python"
    assert opened["handles"]["published"].endswith("/agent-lab/worker-as-python")
    assert opened["edit"]["safe_to_edit"] is True
    source_resource = next(resource for resource in opened["resources"] if resource["surface"] == "source")
    assert source_resource["safe_to_edit"] is True
    assert source_resource["write_mode"] == "hash_checked_direct_edit"
    assert source_resource["sha256"] == opened["hashes"]["source_sha256"]

    written = client.page_write_body(
        "worker-as-python",
        body_markdown="# Worker AS Python\n\nInitial dogfood body.\n",
        expected_source_sha256=opened["hashes"]["source_sha256"],
    )
    assert written["operation"] == "wiki.page.write_body"
    assert written["page_status"]["next_action"] == "publish"
    patched = client.page_patch_body(
        "worker-as-python",
        find="Initial dogfood body.",
        replace="Initial dogfood body with a chained patch.",
        expected_source_sha256=written["hashes"]["source_sha256"],
    )
    assert patched["operation"] == "wiki.page.patch_body"
    assert patched["page_status"]["content_state"] == "edited"
    assert patched["hashes"]["source_sha256"] != written["hashes"]["source_sha256"]

    after_edit = client.publish_status()
    assert after_edit["next_action"] == "publish"
    assert after_edit["render_required"] is True
    assert after_edit["site_needs_publish"] is True
    assert "worker-as-python" in after_edit["pages_needing_publish"]
    published = client.publish(wiki_engine=repo_root / "wiki-engine", trigger="pytest-worker-as")
    assert published["operation"] == "wiki.publish"
    assert published["status"] == "published"
    assert client.publish_status()["next_action"] == "none"
    assert "/agent-lab/worker-as-python" in manifest_routes()

    talk_body_file = tmp_path / "worker-as-talk-body.md"
    talk_body_file.write_text(
        "Talk-only churn can come from a prepared body file without publish pressure.",
        encoding="utf-8",
    )
    talk = client.talk_append(
        page="worker-as-python",
        kind="proposal",
        subject="Worker AS mail-only proof",
        from_address=agent["primary_address"],
        to=[],
        to_roles=["curator"],
        body_file=talk_body_file,
    )
    assert talk["operation"] == "wiki.talk.append"
    assert talk["status"] == "appended"
    assert talk["render_required"] is False
    assert len(talk["deliveries"]) == 1
    thread = client.mail_read(thread_id=talk["thread_id"])
    assert thread["operation"] == "wiki.mail.read"
    assert thread["message_count"] == 1
    assert thread["messages"][0]["subject"] == "Worker AS mail-only proof"
    assert "prepared body file" in thread["messages"][0]["body_markdown"]

    notifications = client.notify_poll(agent["agent_id"])
    assert notifications["operation"] == "wiki.notify.poll"
    assert notifications["notification_count"] == 1
    assert notifications["notifications"][0]["route"] == "/agent-lab/worker-as-python"
    inbox = client.agent_inbox(agent["agent_id"])
    assert inbox["operation"] == "wiki.agent.inbox"
    assert inbox["message_count"] == 1
    assert inbox["summary"]["pages_requiring_action"] == 1
    after_talk = client.publish_status()
    assert after_talk["next_action"] == "none"
    assert after_talk["render_required"] is False
    assert after_talk["site_needs_publish"] is False

    done = client.mail_mark_all(talk["message_id"], state="done")
    assert done["operation"] == "wiki.mail.mark_all"
    assert done["render_required"] is False
    after_done = client.agent_inbox(agent["agent_id"], include_archived=True, include_snoozed=True)
    assert after_done["summary"]["pages_requiring_action"] == 0
    assert after_done["summary"]["notification_count"] == 0
    assert client.publish_status()["next_action"] == "none"

    deleted = client.page_delete("worker-as-python")
    assert deleted["operation"] == "wiki.page.delete"
    assert deleted["status"] == "ok"
    assert deleted["next_action"] == "publish"
    deleted_status = client.page_status("worker-as-python")
    assert deleted_status["state"] == "tombstoned"
    assert deleted_status["next_action"] == "publish"
    client.publish(wiki_engine=repo_root / "wiki-engine", trigger="pytest-worker-as-delete")
    assert "/agent-lab/worker-as-python" not in manifest_routes()
    clean_tombstone = client.page_status("worker-as-python")
    assert clean_tombstone["state"] == "tombstoned"
    assert clean_tombstone["next_action"] == "none"

    restored = client.page_restore("worker-as-python")
    assert restored["operation"] == "wiki.page.restore"
    assert restored["status"] == "ok"
    assert restored["next_action"] == "publish"
    client.publish(wiki_engine=repo_root / "wiki-engine", trigger="pytest-worker-as-restore")
    restored_status = client.page_status("worker-as-python")
    assert restored_status["state"] == "rendered"
    assert restored_status["next_action"] == "none"
    assert "/agent-lab/worker-as-python" in manifest_routes()

    with pytest.raises(WikiCoreError) as patch_error:
        client.page_patch_body("worker-as-python", find="not in body", replace="still absent")
    assert patch_error.value.operation == "wiki.page.patch_body"
    assert patch_error.value.error_code == "body_patch_not_found"
    assert "wiki.page.open" in patch_error.value.repair_hints[0]


def test_wiki_core_error_carries_json_payload_for_stale_page_edits(tmp_path: Path) -> None:
    try:
        binary = resolve_wiki_core_binary()
    except WikiCoreError as exc:
        pytest.skip(str(exc))

    repo_root = Path(__file__).resolve().parents[2]
    shutil.copytree(repo_root / "runtime/1Context", tmp_path / "1Context")

    client = WikiCoreClient(runtime_home=tmp_path, binary=binary)
    assert client.ensure()["status"] == "ok"
    client.page_create("topics")
    stale_open = client.page_open("topics")
    current_open = client.page_open("topics")
    client.page_write_body(
        "topics",
        body_markdown="# Topics\n\nA different agent already wrote this page.\n",
        expected_source_sha256=current_open["hashes"]["source_sha256"],
    )

    with pytest.raises(WikiCoreError) as stale_write:
        client.page_write_body(
            "topics",
            body_markdown="# Topics\n\nThis stale write should be rejected.\n",
            expected_source_sha256=stale_open["hashes"]["source_sha256"],
        )

    assert stale_write.value.payload is not None
    assert stale_write.value.payload["operation"] == "wiki.page.write_body"
    assert stale_write.value.payload["error"]["code"] == "source_hash_mismatch"
    assert "wiki.page.open" in stale_write.value.payload["repair_hints"][0]
    assert stale_write.value.operation == "wiki.page.write_body"
    assert stale_write.value.error_code == "source_hash_mismatch"
    assert stale_write.value.error_message is not None
    assert "wiki.page.open" in stale_write.value.repair_hints[0]

    with pytest.raises(WikiCoreError) as stale_patch:
        client.page_patch_body(
            "topics",
            find="different agent",
            replace="stale agent",
            expected_source_sha256=stale_open["hashes"]["source_sha256"],
        )

    assert stale_patch.value.payload is not None
    assert stale_patch.value.payload["operation"] == "wiki.page.patch_body"
    assert stale_patch.value.payload["error"]["code"] == "source_hash_mismatch"
    assert "expected_source_sha256" in stale_patch.value.payload["repair_hints"][0]
    assert stale_patch.value.operation == "wiki.page.patch_body"
    assert stale_patch.value.error_code == "source_hash_mismatch"
    assert "expected_source_sha256" in stale_patch.value.repair_hints[0]


def test_wiki_core_client_runs_agent_mail_loop_against_runtime_fixture(tmp_path: Path) -> None:
    try:
        binary = resolve_wiki_core_binary()
    except WikiCoreError as exc:
        pytest.skip(str(exc))

    repo_root = Path(__file__).resolve().parents[2]
    shutil.copytree(repo_root / "runtime/1Context", tmp_path / "1Context")

    client = WikiCoreClient(runtime_home=tmp_path, binary=binary)
    assert client.ensure()["status"] == "ok"

    created = client.page_create("topics")
    assert created["operation"] == "wiki.page.create"
    assert created["status"] == "ok"
    assert created["page_status"]["id"] == "topics"
    assert created["hashes"]["source_sha256"]

    listing = client.list_pages()
    assert listing["operation"] == "wiki.list"
    pages = {page["id"]: page for page in listing["pages"]}
    assert pages["topics"]["operation"] == "wiki.page.status"
    assert pages["topics"]["talk_state"] == "ready"
    assert pages["topics"]["flags"]["template_derived"] is True

    agent = client.agent_identify(
        thread_id="019e3f72-3471-7da1-92a8-56e5d25aaa01",
        roles=["role://topics.curator"],
        capabilities=["wiki.mail"],
    )
    assert agent["status"] == "registered"
    whoami = client.agent_whoami(thread_id="019e3f72-3471-7da1-92a8-56e5d25aaa01")
    assert whoami["operation"] == "wiki.agent.whoami"
    assert whoami["next_action"] == "none"
    agent_status = client.agent_status(agent["agent_id"])
    assert agent_status["operation"] == "wiki.agent.status"
    assert agent_status["agent"]["liveness"] == "active"
    agent_list = client.agent_list(include_stale=True, include_retired=True)
    assert agent_list["operation"] == "wiki.agent.list"
    assert agent_list["counts"]["active_count"] == 1

    attachment = tmp_path / "proof-note.txt"
    attachment.write_text("attachment proof\n")

    talk = client.talk_append(
        page="topics",
        kind="proposal",
        subject="Python adapter proof",
        from_address=agent["addresses"][0],
        to=[],
        to_roles=["curator"],
        body_markdown="The memory-side adapter can use the Rust core.",
        attachments=[attachment],
    )
    assert talk["status"] == "appended"
    assert len(talk["deliveries"]) == 1
    assert len(talk["notifications"]) == 1
    assert talk["attachments"][0]["filename"] == "proof-note.txt"
    assert talk["attachments"][0]["path"].startswith("attachments/")
    assert talk["notifications"][0]["attachment_count"] == 1
    assert (
        tmp_path
        / "1Context/user-wiki/source/families/reference/topics/talk/topics.talk"
        / talk["attachments"][0]["path"]
    ).is_file()
    assert client.agent_whoami(thread_id="019e3f72-3471-7da1-92a8-56e5d25aaa01")["next_action"] == "check_inbox"
    read = client.mail_read(message_id=talk["message_id"])
    assert read["operation"] == "wiki.mail.read"
    assert read["message_count"] == 1
    assert read["delivery_count"] == 1
    assert read["messages"][0]["body_markdown"].startswith("## Python adapter proof")
    assert read["messages"][0]["deliveries"][0]["recipient"] == "role://topics.curator"
    assert read["messages"][0]["attachments"][0]["filename"] == "proof-note.txt"
    notifications = client.notify_poll(agent["agent_id"])
    assert notifications["operation"] == "wiki.notify.poll"
    assert notifications["notification_count"] == len(notifications["notifications"])
    assert notifications["next_action"] == "notify_ack"
    assert len(notifications["notifications"]) >= 1
    client.notify_ack(
        notifications["notifications"][0]["notification_id"],
        agent_id=agent["agent_id"],
        state="delivered",
    )

    inbox = client.mail_inbox("role://topics.curator")
    assert inbox["operation"] == "wiki.mail.inbox"
    assert inbox["message_count"] == 1
    assert inbox["actionable_count"] == 1
    assert inbox["mailbox"]["unread_count"] == 1
    assert inbox["next_action"] == "mail_claim_or_mark"
    assert inbox["messages"][0]["subject"] == "Python adapter proof"
    assert inbox["messages"][0]["attachments"][0]["filename"] == "proof-note.txt"
    with pytest.raises(WikiCoreError) as unowned_claim:
        client.mail_mark(talk["message_id"], recipient="role://topics.curator", state="claimed")
    assert unowned_claim.value.error_code == "invalid_mail_state"
    assert "use wiki.mail.claim or wiki.agent.claim" in (unowned_claim.value.error_message or "")
    assert "Use mail-claim or agent-claim" in unowned_claim.value.repair_hints[0]

    agent_claim = client.agent_claim(agent["agent_id"], talk["message_id"])
    assert agent_claim["operation"] == "wiki.agent.claim"
    assert agent_claim["recipient"] == "role://topics.curator"
    assert agent_claim["state"] == "claimed"

    claim = client.mail_claim(
        talk["message_id"],
        recipient="role://topics.curator",
        agent_id=agent["agent_id"],
    )
    assert claim["status"] in {"claimed", "already_claimed"}
    client.mail_mark(talk["message_id"], recipient="role://topics.curator", state="done")
    assert client.mail_mark_all(talk["message_id"], state="done")["status"] == "ok"
    assert client.mail_inbox("role://topics.curator")["mailbox"]["unread_count"] == 0

    snoozed = client.talk_append(
        page="topics",
        kind="question",
        subject="Python adapter snooze proof",
        from_address=agent["addresses"][0],
        to=["role://topics.curator"],
        body_markdown="This message should hide until explicitly requested.",
    )
    client.mail_mark(
        snoozed["message_id"],
        recipient="role://topics.curator",
        state="snoozed",
        until="2099-01-01T00:00:00Z",
    )
    default_subjects = {
        message["subject"] for message in client.mail_inbox("role://topics.curator")["messages"]
    }
    assert "Python adapter snooze proof" not in default_subjects
    snoozed_inbox = client.mail_inbox("role://topics.curator", include_snoozed=True)
    snoozed_messages = [
        message for message in snoozed_inbox["messages"] if message["subject"] == "Python adapter snooze proof"
    ]
    assert snoozed_messages[0]["state"] == "snoozed"
    agent_inbox = client.agent_inbox(agent["agent_id"], include_snoozed=True)
    assert agent_inbox["operation"] == "wiki.agent.inbox"
    assert agent_inbox["message_count"] == agent_inbox["summary"]["message_count"]
    assert agent_inbox["pages_requiring_action"] == agent_inbox["summary"]["pages_requiring_action"]
    assert "claimable_count" in agent_inbox["summary"]
    assert agent_inbox["summary"]["message_count"] >= 1


def test_python_agent_adapter_can_retire_stale_identity(tmp_path: Path) -> None:
    try:
        binary = resolve_wiki_core_binary()
    except FileNotFoundError as exc:
        pytest.skip(str(exc))
    repo_root = Path(__file__).resolve().parents[2]
    shutil.copytree(repo_root / "runtime/1Context", tmp_path / "1Context")
    client = WikiCoreClient(runtime_home=tmp_path, binary=binary)

    agent = client.agent_register(thread_id="python-stale-retire-agent")
    agents_path = tmp_path / "1Context/context-engine/agents/directory/agents.jsonl"
    stale_record = json.loads(agents_path.read_text(encoding="utf-8").splitlines()[-1])
    stale_record["event"] = "agent.heartbeat"
    stale_record["at"] = "2000-01-01T00:00:00Z"
    stale_record["lease_expires_at"] = "2000-01-01T00:00:00Z"
    with agents_path.open("a", encoding="utf-8") as handle:
        handle.write(json.dumps(stale_record) + "\n")

    assert client.agent_status(agent["agent_id"])["agent"]["liveness"] == "stale"
    retired = client.agent_retire(agent["agent_id"], reason="stale adapter cleanup")
    assert retired["operation"] == "wiki.agent.retire"
    assert retired["status"] == "retired"
    whoami = client.agent_whoami(thread_id="python-stale-retire-agent")
    assert whoami["matches"][0]["liveness"] == "retired"
    assert whoami["next_action"] == "agent_register_new_thread"


def test_talk_append_supports_explicit_reply_targets(tmp_path: Path) -> None:
    try:
        binary = resolve_wiki_core_binary()
    except WikiCoreError as exc:
        pytest.skip(str(exc))

    repo_root = Path(__file__).resolve().parents[2]
    shutil.copytree(repo_root / "runtime/1Context", tmp_path / "1Context")

    client = WikiCoreClient(runtime_home=tmp_path, binary=binary)
    client.ensure()
    client.page_create("topics")
    agent = client.agent_identify(
        thread_id="adapter-reply-thread",
        roles=["role://topics.curator"],
        capabilities=["wiki.mail"],
    )

    parent = client.talk_append(
        page="topics",
        kind="proposal",
        subject="Original adapter subject",
        from_address=agent["primary_address"],
        to=[],
        to_roles=["curator"],
        body_markdown="Parent message.",
    )
    reply = client.talk_append(
        page="topics",
        kind="reply",
        subject="Changed adapter subject",
        from_address=agent["primary_address"],
        to=[],
        to_roles=["curator"],
        body_markdown="Reply should not fork by subject.",
        reply_to=parent["message_id"],
    )
    thread_reply = wiki_talk_append(
        tmp_path,
        page="topics",
        kind="reply",
        subject="Explicit thread adapter subject",
        from_address=agent["primary_address"],
        to=[],
        to_roles=["curator"],
        body_markdown="Thread target should stay attached.",
        thread_id=parent["thread_id"],
        binary=binary,
    )

    assert reply["thread_id"] == parent["thread_id"]
    assert reply["reply_to"] == parent["message_id"]
    assert thread_reply["thread_id"] == parent["thread_id"]
    assert "reply_to" not in thread_reply
    thread = client.mail_read(thread_id=parent["thread_id"])
    assert thread["message_count"] == 3
    reply_read = next(message for message in thread["messages"] if message["message_id"] == reply["message_id"])
    assert reply_read["reply_to"] == parent["message_id"]


def test_python_client_keeps_colliding_mailbox_addresses_isolated(tmp_path: Path) -> None:
    try:
        binary = resolve_wiki_core_binary()
    except WikiCoreError as exc:
        pytest.skip(str(exc))

    repo_root = Path(__file__).resolve().parents[2]
    shutil.copytree(repo_root / "runtime/1Context", tmp_path / "1Context")

    client = WikiCoreClient(runtime_home=tmp_path, binary=binary)
    client.ensure()
    client.page_create("topics")
    agent = client.agent_register(thread_id="python-mailbox-collision-agent")
    dot_recipient = "role://worker-cy.alpha"
    slash_recipient = "role://worker-cy/alpha"

    talk = client.talk_append(
        page="topics",
        kind="proposal",
        subject="Python mailbox collision",
        from_address=agent["primary_address"],
        to=[dot_recipient, slash_recipient],
        body_markdown="These addresses share the same mailbox directory key but are distinct recipients.",
    )

    dot_before = client.mail_inbox(dot_recipient)
    slash_before = client.mail_inbox(slash_recipient)
    assert [message["recipient"] for message in dot_before["messages"]] == [dot_recipient]
    assert [message["recipient"] for message in slash_before["messages"]] == [slash_recipient]

    client.mail_mark(talk["message_id"], recipient=dot_recipient, state="done")
    assert client.mail_inbox(dot_recipient)["messages"][0]["state"] == "done"
    assert client.mail_inbox(slash_recipient)["messages"][0]["state"] == "unread"

    claim = client.mail_claim(talk["message_id"], recipient=slash_recipient, agent_id=agent["agent_id"])
    assert claim["status"] == "claimed"
    assert client.mail_inbox(slash_recipient)["messages"][0]["claimed_by"] == agent["agent_id"]
    assert "claimed_by" not in client.mail_inbox(dot_recipient)["messages"][0]


def test_agent_mail_list_survives_page_tombstone_and_restore(tmp_path: Path) -> None:
    try:
        binary = resolve_wiki_core_binary()
    except WikiCoreError as exc:
        pytest.skip(str(exc))

    repo_root = Path(__file__).resolve().parents[2]
    shutil.copytree(repo_root / "runtime/1Context", tmp_path / "1Context")
    route_manifest = tmp_path / "1Context/user-wiki/site/.1context/route-manifest.json"

    def manifest_routes() -> set[str]:
        return {
            route["route"]
            for route in json.loads(route_manifest.read_text(encoding="utf-8"))["routes"]
        }

    client = WikiCoreClient(runtime_home=tmp_path, binary=binary, timeout_seconds=180)
    client.ensure()

    page_id = "worker-av-delete-restore"
    route = "/agent-lab/worker-av-delete-restore"
    page_list = f"list://{page_id}.reviewers"
    parent_attachment = tmp_path / "parent-proof.txt"
    reply_attachment = tmp_path / "reply-proof.json"
    parent_attachment.write_text("parent attachment survives tombstone\n", encoding="utf-8")
    reply_attachment.write_text('{"reply":"attachment survives restore loop"}\n', encoding="utf-8")

    curator = client.agent_register(
        thread_id="worker-av-curator",
        roles=[f"role://{page_id}.curator"],
        capabilities=["wiki.mail", "wiki.talk"],
    )
    reviewer = client.agent_register(
        thread_id="worker-av-reviewer",
        roles=[f"role://{page_id}.reviewer"],
        capabilities=["wiki.mail", "wiki.talk"],
    )
    assert curator["status"] == "registered"
    assert reviewer["status"] == "registered"

    created = client.page_create(
        page_id,
        title="Worker AV Delete Restore",
        route=route,
        family_group="80-agent-lab",
        family_group_title="Agent Lab",
        family_id="20-worker-av-delete-restore",
        family_title="Worker AV Delete Restore",
        nav_section="utility",
        nav_order=91,
        summary="Worker AV dogfoods inbox/list behavior across delete and restore.",
    )
    opened = client.page_open(page_id)
    client.page_write_body(
        page_id,
        body_markdown="# Worker AV Delete Restore\n\nDogfood source body.\n",
        expected_source_sha256=opened["hashes"]["source_sha256"],
    )
    assert created["page_status"]["next_action"] == "publish"
    client.publish(wiki_engine=repo_root / "wiki-engine", trigger="pytest-worker-av-initial")
    assert route in manifest_routes()
    assert client.publish_status()["next_action"] == "none"

    client.page_assign_role(page_id, agent_id=curator["agent_id"], role="role://curator")
    client.page_assign_role(page_id, agent_id=reviewer["agent_id"], role="role://reviewer")
    client.page_watch(page_id, agent_id=curator["agent_id"], kinds=["proposal", "reply"])
    client.page_watch(page_id, agent_id=reviewer["agent_id"], kinds=["proposal", "reply"])
    client.list_create(
        address=page_list,
        title="Worker AV Reviewers",
        page=page_id,
        owner=curator["agent_id"],
    )
    client.mail_subscribe(agent_id=reviewer["agent_id"], address=page_list, kinds=["proposal", "reply"])
    list_members = client.list_members(page_list)
    assert list_members["member_count"] == 1
    assert list_members["active_member_count"] == 1
    assert client.publish_status()["next_action"] == "none"

    parent = client.talk_append(
        page=page_id,
        kind="proposal",
        subject="Worker AV parent proof",
        from_address=curator["primary_address"],
        to=["page://" + page_id, page_list],
        to_roles=["curator"],
        body_markdown="Parent message fans out to page, role, and list recipients.",
        attachments=[parent_attachment],
    )
    reply = client.talk_append(
        page=page_id,
        kind="reply",
        subject="Worker AV reply proof",
        from_address=reviewer["primary_address"],
        to=[page_list],
        to_roles=["reviewer"],
        body_markdown="Reply targets the parent thread and carries its own attachment.",
        attachments=[reply_attachment],
        reply_to=parent["message_id"],
    )
    assert parent["thread_id"] == reply["thread_id"]
    assert parent["render_required"] is False
    assert reply["render_required"] is False
    assert client.publish_status()["next_action"] == "none"

    before_delete_thread = client.mail_read(thread_id=parent["thread_id"])
    assert before_delete_thread["message_count"] == 2
    assert before_delete_thread["delivery_count"] >= 4
    attachment_names = {
        attachment["filename"]
        for message in before_delete_thread["messages"]
        for attachment in message["attachments"]
    }
    assert attachment_names == {"parent-proof.txt", "reply-proof.json"}
    list_status = client.list_status(page_list)
    assert list_status["operation"] == "wiki.list.status"
    assert list_status["mailbox"]["total_count"] >= 2
    assert list_status["next_action"] == "mail_claim_or_mark"

    curator_notifications = client.notify_poll(curator["agent_id"])
    assert curator_notifications["notification_count"] >= 1
    client.notify_ack(
        curator_notifications["notifications"][0]["notification_id"],
        agent_id=curator["agent_id"],
    )
    claim = client.agent_claim(curator["agent_id"], parent["message_id"])
    assert claim["state"] == "claimed"
    mark = client.mail_mark(parent["message_id"], recipient=claim["recipient"], state="done")
    assert mark["state"] == "done"
    assert client.publish_status()["next_action"] == "none"

    deleted = client.page_delete(page_id)
    assert deleted["operation"] == "wiki.page.delete"
    assert deleted["next_action"] == "publish"
    tombstoned = client.page_status(page_id)
    assert tombstoned["state"] == "tombstoned"
    assert tombstoned["next_action"] == "publish"
    after_delete_publish_status = client.publish_status()
    assert after_delete_publish_status["next_action"] == "publish"
    assert after_delete_publish_status["pages_needing_publish"] == [page_id]

    with pytest.raises(WikiCoreError) as tombstoned_talk:
        client.talk_append(
            page=page_id,
            kind="reply",
            subject="Refused while tombstoned",
            from_address=curator["primary_address"],
            to=[page_list],
            body_markdown="Normal talk append should not write into a tombstoned page.",
            reply_to=parent["message_id"],
        )
    assert tombstoned_talk.value.error_code == "tombstoned_page"

    after_delete_thread = client.mail_read(thread_id=parent["thread_id"])
    assert after_delete_thread["message_count"] == 2
    assert {message["message_id"] for message in after_delete_thread["messages"]} == {
        parent["message_id"],
        reply["message_id"],
    }
    assert client.list_status(
        page_list,
        include_archived=True,
        include_snoozed=True,
    )["mailbox"]["total_count"] >= 2

    client.publish(wiki_engine=repo_root / "wiki-engine", trigger="pytest-worker-av-delete")
    assert route not in manifest_routes()
    clean_tombstone = client.page_status(page_id)
    assert clean_tombstone["state"] == "tombstoned"
    assert clean_tombstone["next_action"] == "none"
    assert client.mail_read(thread_id=parent["thread_id"])["message_count"] == 2

    restored = client.page_restore(page_id)
    assert restored["operation"] == "wiki.page.restore"
    assert restored["next_action"] == "publish"
    client.publish(wiki_engine=repo_root / "wiki-engine", trigger="pytest-worker-av-restore")
    assert route in manifest_routes()
    restored_status = client.page_status(page_id)
    assert restored_status["state"] == "rendered"
    assert restored_status["next_action"] == "none"

    followup = client.talk_append(
        page=page_id,
        kind="reply",
        subject="Worker AV restored followup",
        from_address=curator["primary_address"],
        to=[page_list],
        to_roles=["reviewer"],
        body_markdown="Restore reopens normal collaboration without content publish pressure.",
        reply_to=parent["message_id"],
    )
    assert followup["thread_id"] == parent["thread_id"]
    assert followup["render_required"] is False
    assert client.mail_read(thread_id=parent["thread_id"])["message_count"] == 3
    assert client.agent_inbox(reviewer["agent_id"])["summary"]["pages_requiring_action"] == 1
    assert client.publish_status()["next_action"] == "none"


def test_wiki_list_status_exposes_hidden_mail_audit_flags(tmp_path: Path) -> None:
    try:
        binary = resolve_wiki_core_binary()
    except WikiCoreError as exc:
        pytest.skip(str(exc))

    repo_root = Path(__file__).resolve().parents[2]
    shutil.copytree(repo_root / "runtime/1Context", tmp_path / "1Context")

    client = WikiCoreClient(runtime_home=tmp_path, binary=binary)
    client.ensure()
    client.page_create("topics")
    agent = client.agent_identify(
        thread_id="list-status-audit-flags",
        roles=["role://topics.curator"],
        capabilities=["wiki.mail"],
    )
    list_address = "list://topics.audit"
    client.list_create(
        address=list_address,
        title="Topics Audit",
        page="topics",
        owner=agent["agent_id"],
    )
    client.mail_subscribe(agent_id=agent["agent_id"], address=list_address, kinds=["proposal"])
    snoozed = client.talk_append(
        page="topics",
        kind="proposal",
        subject="Snoozed audit",
        from_address=agent["primary_address"],
        to=[list_address],
        body_markdown="Snoozed list mail should leave an audit trace.",
    )
    archived = client.talk_append(
        page="topics",
        kind="proposal",
        subject="Archived audit",
        from_address=agent["primary_address"],
        to=[list_address],
        body_markdown="Archived list mail should leave an audit trace.",
    )
    client.mail_mark(
        snoozed["message_id"],
        recipient=list_address,
        state="snoozed",
        until="2099-01-01T00:00:00Z",
    )
    client.mail_mark(archived["message_id"], recipient=list_address, state="archived")

    default_status = client.list_status(list_address)
    assert default_status["operation"] == "wiki.list.status"
    assert default_status["include_archived"] is False
    assert default_status["include_snoozed"] is False
    assert default_status["has_archived"] is True
    assert default_status["has_snoozed"] is True
    assert default_status["hidden_archived_count"] == 1
    assert default_status["hidden_snoozed_count"] == 1
    assert set(default_status["audit_flags"]) == {"archived_hidden", "snoozed_hidden"}
    assert default_status["messages"] == []
    assert default_status["next_action"] == "include_hidden_mail"

    audit_status = wiki_list_status(
        tmp_path,
        list_address,
        include_archived=True,
        include_snoozed=True,
        binary=binary,
    )
    assert audit_status["include_archived"] is True
    assert audit_status["include_snoozed"] is True
    assert audit_status["hidden_archived_count"] == 0
    assert audit_status["hidden_snoozed_count"] == 0
    assert {message["state"] for message in audit_status["messages"]} == {"archived", "snoozed"}


def test_wiki_module_helpers_cover_agent_mail_and_page_metadata(tmp_path: Path) -> None:
    try:
        binary = resolve_wiki_core_binary()
    except WikiCoreError as exc:
        pytest.skip(str(exc))

    repo_root = Path(__file__).resolve().parents[2]
    shutil.copytree(repo_root / "runtime/1Context", tmp_path / "1Context")

    assert wiki_ensure(tmp_path, binary=binary)["status"] == "ok"
    assert wiki_status(tmp_path, binary=binary)["operation"] == "wiki.status"
    assert wiki_validate(tmp_path, binary=binary)["operation"] == "wiki.validate"
    created = wiki_page_create(
        tmp_path,
        "helper-test",
        title="Helper Test",
        route="/helper-test",
        family_group="90-tests",
        family_group_title="Tests",
        family_id="10-helper-test",
        family_title="Helper Test",
        nav_section="utility",
        nav_order=1,
        binary=binary,
    )
    assert created["status"] == "ok"
    status = wiki_page_status(tmp_path, "helper-test", binary=binary)
    assert status["operation"] == "wiki.page.status"
    assert status["nav_section"] == "utility"
    opened = wiki_page_open(tmp_path, "helper-test", binary=binary)
    written = wiki_page_write_body(
        tmp_path,
        "helper-test",
        body_markdown="# Helper Test\n\nAdapter receipts should carry fresh edit context.\n",
        expected_source_sha256=opened["hashes"]["source_sha256"],
        binary=binary,
    )
    assert written["operation"] == "wiki.page.write_body"
    assert written["page_status"]["id"] == "helper-test"
    assert written["page_status"]["state"] == "needs_publish"
    assert written["hashes"]["source_sha256"] != opened["hashes"]["source_sha256"]
    patched = wiki_page_patch_body(
        tmp_path,
        "helper-test",
        find="fresh edit context",
        replace="fresh edit context and chainable hashes",
        expected_source_sha256=written["hashes"]["source_sha256"],
        binary=binary,
    )
    assert patched["operation"] == "wiki.page.patch_body"
    assert patched["hashes"]["source_sha256"] != written["hashes"]["source_sha256"]
    assert patched["page_status"]["content_state"] == "edited"
    listing = wiki_list(tmp_path, binary=binary)
    assert listing["operation"] == "wiki.list"
    pages = {page["id"]: page for page in listing["pages"]}
    assert pages["helper-test"]["operation"] == "wiki.page.status"
    assert pages["helper-test"]["nav_section"] == "utility"
    assert wiki_publish_status(tmp_path, binary=binary)["operation"] == "wiki.publish.status"

    agent = wiki_agent_identify(
        tmp_path,
        thread_id="module-helper-thread",
        roles=["role://helper-test.curator"],
        capabilities=["wiki.mail"],
        binary=binary,
    )
    assert agent["primary_address"] == "agent://codex/module-helper-thread"
    assert agent["addresses"][0] == agent["primary_address"]
    assigned = wiki_page_assign_role(
        tmp_path,
        "helper-test",
        agent_id=agent["agent_id"],
        role="role://curator",
        binary=binary,
    )
    assert assigned["subscription"]["address"] == "role://helper-test.curator"
    watched = wiki_page_watch(
        tmp_path,
        "helper-test",
        agent_id=agent["agent_id"],
        kinds=["proposal"],
        binary=binary,
    )
    assert watched["operation"] == "wiki.page.watch"
    assert watched["unsubscribe_plan"]["operation"] == "wiki.page.unwatch"
    assert watched["subscription"]["address"] == "list://helper-test.watchers"
    assert watched["page_mailbox_subscription"]["address"] == "mailbox://page/helper-test"
    unwatched = wiki_page_unwatch(
        tmp_path,
        "helper-test",
        agent_id=agent["agent_id"],
        kinds=["proposal"],
        binary=binary,
    )
    assert unwatched["operation"] == "wiki.page.unwatch"
    assert unwatched["status"] == "unwatched"
    assert unwatched["cancelled_count"] == 2

    list_address = "list://helper-test.watchers"
    mail_list = wiki_list_create(
        tmp_path,
        address=list_address,
        title="Helper Test Watchers",
        page="helper-test",
        owner=agent["agent_id"],
        binary=binary,
    )
    assert mail_list["list"]["owner"] == agent["addresses"][0]
    wiki_mail_subscribe(
        tmp_path,
        agent_id=agent["agent_id"],
        address=list_address,
        kinds=["proposal"],
        binary=binary,
    )
    assert wiki_lists(tmp_path, page="helper-test", binary=binary)["operation"] == "wiki.lists"
    members = wiki_list_members(tmp_path, list_address, binary=binary)
    assert members["operation"] == "wiki.list.members"
    assert members["exists"] is True
    assert members["member_count"] == 1
    assert members["list"]["member_count"] == 1
    list_status = wiki_list_status(tmp_path, list_address, binary=binary)
    assert list_status["operation"] == "wiki.list.status"
    assert list_status["exists"] is True
    assert list_status["member_count"] == 1
    subscriptions = wiki_mail_subscriptions(tmp_path, agent_id=agent["agent_id"], binary=binary)
    assert subscriptions["operation"] == "wiki.mail.subscriptions"
    assert subscriptions["subscription_count"] >= 2
    assert subscriptions["liveness_counts"]["active_agent_count"] == 1
    unsubscribed = wiki_mail_unsubscribe(
        tmp_path,
        agent_id=agent["agent_id"],
        address=list_address,
        kinds=["proposal"],
        binary=binary,
    )
    assert unsubscribed["operation"] == "wiki.mail.unsubscribe"
    assert unsubscribed["status"] == "unsubscribed"
    assert unsubscribed["cancelled_count"] == 1
    assert unsubscribed["remaining_count"] == 0
    resubscribed = wiki_mail_subscribe(
        tmp_path,
        agent_id=agent["agent_id"],
        address=list_address,
        kinds=["proposal"],
        binary=binary,
    )
    assert resubscribed["operation"] == "wiki.mail.subscribe"

    with pytest.raises(WikiCoreError, match="agent ids are not mail addresses"):
        wiki_talk_append(
            tmp_path,
            page="helper-test",
            kind="proposal",
            subject="Wrong address",
            from_address=agent["primary_address"],
            to=[f"agent://{agent['agent_id']}"],
            body_markdown="This should fail before creating a dead mailbox.",
            binary=binary,
        )

    page_mail = wiki_talk_append(
        tmp_path,
        page="helper-test",
        kind="proposal",
        subject="Page URI alias",
        from_address=agent["primary_address"],
        to=["page://helper-test"],
        body_markdown="Page URI recipients should resolve to the page mailbox.",
        binary=binary,
    )
    assert page_mail["deliveries"][0]["recipient"] == "mailbox://page/helper-test"
    page_mailbox = wiki_mail_inbox(tmp_path, "mailbox://page/helper-test", binary=binary)
    assert page_mailbox["operation"] == "wiki.mail.inbox"
    assert page_mailbox["message_count"] == 1

    talk = wiki_talk_append(
        tmp_path,
        page="helper-test",
        kind="proposal",
        subject="Module helpers",
        from_address=agent["primary_address"],
        to=[list_address],
        to_roles=["curator"],
        body_markdown="Module-level helpers should be enough for normal agent work.",
        binary=binary,
    )
    assert len(talk["deliveries"]) == 2
    thread_read = wiki_mail_read(tmp_path, thread_id=talk["thread_id"], binary=binary)
    assert thread_read["resolved_by"] == "thread_id"
    assert thread_read["message_count"] == 1
    assert thread_read["delivery_count"] == 2
    assert "Module-level helpers" in thread_read["messages"][0]["body_markdown"]
    notifications = wiki_notify_poll(tmp_path, agent["agent_id"], binary=binary)
    assert notifications["operation"] == "wiki.notify.poll"
    assert notifications["notification_count"] == len(notifications["notifications"])
    assert len(notifications["notifications"]) == 2
    first_notification = notifications["notifications"][0]
    assert first_notification["agent_address"] == agent["primary_address"]
    assert first_notification["delivery_recipient"] in {list_address, "role://helper-test.curator"}
    assert first_notification["route"] == "/helper-test"
    assert first_notification["subject"] == "Module helpers"
    assert "Module-level helpers" in first_notification["excerpt"]
    assert first_notification["attachment_count"] == 0
    wiki_notify_ack(
        tmp_path,
        first_notification["notification_id"],
        agent_id=agent["agent_id"],
        binary=binary,
    )

    list_inbox = wiki_mail_inbox(tmp_path, list_address, binary=binary)
    assert list_inbox["operation"] == "wiki.mail.inbox"
    assert list_inbox["message_count"] == 1
    assert list_inbox["actionable_count"] == 1
    inbox = wiki_agent_inbox(tmp_path, agent["agent_id"], binary=binary)
    assert inbox["operation"] == "wiki.agent.inbox"
    assert inbox["message_count"] == inbox["summary"]["message_count"]
    assert inbox["pages_requiring_action"] == inbox["summary"]["pages_requiring_action"]
    assert inbox["summary"]["thread_count"] == 1
    assert inbox["summary"]["pages_requiring_action"] == 1
    assert inbox["claimable_count"] == inbox["summary"]["claimable_count"]
    assert inbox["summary"]["claimable_count"] == 2
    assert inbox["threads"][0]["claimable_delivery_count"] == 2
    agent_claim = wiki_agent_claim(tmp_path, agent["agent_id"], talk["message_id"], binary=binary)
    assert agent_claim["operation"] == "wiki.agent.claim"
    assert agent_claim["recipient"] == "role://helper-test.curator"
    assert agent_claim["state"] == "claimed"
    assert wiki_mail_mark_all(tmp_path, talk["message_id"], state="done", binary=binary)["status"] == "ok"
    done_inbox = wiki_agent_inbox(
        tmp_path,
        agent["agent_id"],
        include_archived=True,
        include_snoozed=True,
        binary=binary,
    )
    assert done_inbox["summary"]["actionable_count"] == 0
    assert done_inbox["summary"]["claimable_count"] == 0
    assert done_inbox["summary"]["pages_requiring_action"] == 0

    assert wiki_page_delete(tmp_path, "helper-test", binary=binary)["status"] == "ok"
    with pytest.raises(WikiCoreError) as recreate_error:
        wiki_page_create(tmp_path, "helper-test", title="Helper Test Restored", binary=binary)
    assert recreate_error.value.error_code == "tombstoned_page"
    assert "page-create refused for tombstoned page" in (
        recreate_error.value.error_message or ""
    )
    with pytest.raises(WikiCoreError, match="route_already_exists"):
        wiki_page_create(
            tmp_path,
            "helper-test-replacement",
            title="Helper Test Replacement",
            route="/helper-test",
            binary=binary,
        )
    restored = wiki_page_restore(tmp_path, "helper-test", binary=binary)
    assert restored["operation"] == "wiki.page.restore"
    assert restored["next_action"] == "publish"
    restored_status = wiki_page_status(tmp_path, "helper-test", binary=binary)
    assert restored_status["operation"] == "wiki.page.status"
    assert restored_status["state"] == "needs_publish"
    assert restored_status["flags"]["enabled"] is True
    assert restored_status["flags"]["tombstoned"] is False


def test_wiki_page_unwatch_python_helpers_cover_scoped_and_broad_cleanup(tmp_path: Path) -> None:
    try:
        binary = resolve_wiki_core_binary()
    except WikiCoreError as exc:
        pytest.skip(str(exc))

    repo_root = Path(__file__).resolve().parents[2]
    shutil.copytree(repo_root / "runtime/1Context", tmp_path / "1Context")

    client = WikiCoreClient(runtime_home=tmp_path, binary=binary)
    assert client.ensure()["status"] == "ok"
    assert client.page_create_all()["operation"] == "wiki.page.create_all"
    assert wiki_page_create_all(tmp_path, binary=binary)["operation"] == "wiki.page.create_all"
    agent = client.agent_identify(
        thread_id="python-page-unwatch-thread",
        capabilities=["wiki.mail"],
    )

    proposal_watch = client.page_watch("topics", agent_id=agent["agent_id"], kinds=["proposal"])
    assert proposal_watch["operation"] == "wiki.page.watch"
    assert proposal_watch["unsubscribe_plan"]["operation"] == "wiki.page.unwatch"
    assert proposal_watch["unsubscribe_plan"]["list_address"] == "list://topics.watchers"
    assert proposal_watch["unsubscribe_plan"]["page_mailbox_address"] == "mailbox://page/topics"

    broad_watch = wiki_page_watch(
        tmp_path,
        "topics",
        agent_id=agent["agent_id"],
        kinds=["proposal", "question"],
        binary=binary,
    )
    assert broad_watch["operation"] == "wiki.page.watch"
    before = client.mail_subscriptions(agent_id=agent["agent_id"])
    assert before["subscription_count"] == 4

    scoped = client.page_unwatch("topics", agent_id=agent["agent_id"], kinds=["proposal"])
    assert scoped["operation"] == "wiki.page.unwatch"
    assert scoped["cancelled_count"] == 2
    assert scoped["remaining_count"] == 2
    assert scoped["next_action"] == "mail_subscriptions"
    after_scoped = wiki_mail_subscriptions(tmp_path, agent_id=agent["agent_id"], binary=binary)
    assert after_scoped["subscription_count"] == 2
    assert {
        tuple(subscription["kinds"]) for subscription in after_scoped["subscriptions"]
    } == {("proposal", "question")}

    broad = wiki_page_unwatch(tmp_path, "topics", agent_id=agent["agent_id"], binary=binary)
    assert broad["operation"] == "wiki.page.unwatch"
    assert broad["cancelled_count"] == 2
    assert broad["remaining_count"] == 0
    assert broad["next_action"] == "none"
    assert client.mail_subscriptions(agent_id=agent["agent_id"])["subscription_count"] == 0

    generated_status = client.page_status("home")
    assert generated_status["operation"] == "wiki.page.status"
    assert generated_status["kind"] == "generated_site_page"
    assert generated_status["flags"]["source_backed"] is False
    assert generated_status["allowed_actions"] == ["wiki.validate", "wiki.publish"]

    with pytest.raises(WikiCoreError) as open_error:
        wiki_page_open(tmp_path, "/this-week", binary=binary)
    assert open_error.value.error_code == "generated_site_page"
    assert open_error.value.operation == "wiki.page.open"
    assert open_error.value.repair_hints
