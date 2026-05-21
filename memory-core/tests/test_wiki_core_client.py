from __future__ import annotations

import shutil
from pathlib import Path

import pytest

from onectx.wiki_interface import (
    WikiCoreClient,
    WikiCoreError,
    resolve_wiki_core_binary,
    wiki_agent_heartbeat,
    wiki_agent_identify,
    wiki_agent_inbox,
    wiki_agent_status_by_thread,
    wiki_asset_add,
    wiki_asset_list,
    wiki_ensure,
    wiki_list,
    wiki_mail_mark,
    wiki_mail_open,
    wiki_mail_record_injection,
    wiki_mail_send,
    wiki_notify_ack,
    wiki_notify_poll,
    wiki_page_create,
    wiki_page_delete,
    wiki_page_open,
    wiki_page_patch_body,
    wiki_page_restore,
    wiki_page_status,
    wiki_page_write_body,
    wiki_publish,
    wiki_publish_status,
    wiki_reference_list,
    wiki_status,
    wiki_talk_append,
    wiki_validate,
)


def test_wiki_publish_helper_passes_node_argument(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
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

    client.reference_list()
    assert calls[-1] == ("reference-list",)

    wiki_reference_list(tmp_path, "worker-bb")
    assert calls[-1] == ("reference-list", "worker-bb")

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
            body_markdown="inline",
            body_file=talk_body_file,
        )
    with pytest.raises(ValueError, match="requires --body or --body-file"):
        client.talk_append(
            page="worker-bb",
            subject="Missing talk body",
            from_address="agent://worker-be",
        )

    client.reference_list()
    assert calls[-1] == ("reference-list",)

    wiki_reference_list(tmp_path, "worker-bb")
    assert calls[-1] == ("reference-list", "worker-bb")

    wiki_asset_list(tmp_path, "worker-bb")
    assert calls[-1] == ("asset-list", "worker-bb")


def test_agent_mail_and_notification_helpers_are_thin_cli_wrappers(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    calls: list[tuple[str, ...]] = []

    def fake_call(self: WikiCoreClient, *args: str) -> dict[str, object]:
        calls.append(args)
        return {"operation": "fake", "args": list(args)}

    monkeypatch.setattr(WikiCoreClient, "call", fake_call)
    client = WikiCoreClient(runtime_home=tmp_path)

    client.agent_identify(
        thread_id="thread-mail-proof",
        roles=["role://topics.curator"],
        capabilities=["wiki.mail"],
        ttl_seconds=300,
    )
    assert calls[-1] == (
        "agent-identify",
        "--thread-id",
        "thread-mail-proof",
        "--role",
        "role://topics.curator",
        "--capability",
        "wiki.mail",
        "--ttl-seconds",
        "300",
    )

    wiki_agent_identify(tmp_path, thread_id="thread-helper", binary="unused")
    assert calls[-1] == ("agent-identify", "--thread-id", "thread-helper")

    client.agent_heartbeat("agent_codex_py", ttl_seconds=120)
    assert calls[-1] == ("agent-heartbeat", "agent_codex_py", "--ttl-seconds", "120")

    wiki_agent_heartbeat(tmp_path, "agent_codex_py", binary="unused")
    assert calls[-1] == ("agent-heartbeat", "agent_codex_py")

    client.agent_status("agent_codex_py")
    assert calls[-1] == ("agent-status", "agent_codex_py")

    client.agent_status_by_thread("thread-mail-proof")
    assert calls[-1] == ("agent-status-by-thread", "thread-mail-proof")

    wiki_agent_status_by_thread(tmp_path, "thread-helper", binary="unused")
    assert calls[-1] == ("agent-status-by-thread", "thread-helper")

    wiki_agent_inbox(tmp_path, "agent_codex_py", binary="unused")
    assert calls[-1] == ("agent-inbox", "agent_codex_py")

    wiki_mail_open(tmp_path, "delivery_py", agent_id="agent_codex_py", binary="unused")
    assert calls[-1] == ("mail-open", "delivery_py", "--agent-id", "agent_codex_py")

    client.mail_record_injection(
        "delivery_py",
        agent_id="agent_codex_py",
        thread_id="thread-mail-proof",
        item_count=1,
    )
    assert calls[-1] == (
        "mail-record-injection",
        "delivery_py",
        "--agent-id",
        "agent_codex_py",
        "--result",
        "ok",
        "--item-count",
        "1",
        "--thread-id",
        "thread-mail-proof",
    )

    wiki_mail_record_injection(
        tmp_path,
        "delivery_py",
        agent_id="agent_codex_py",
        result="failed",
        item_count=0,
        error="host adapter unavailable",
        binary="unused",
    )
    assert calls[-1] == (
        "mail-record-injection",
        "delivery_py",
        "--agent-id",
        "agent_codex_py",
        "--result",
        "failed",
        "--item-count",
        "0",
        "--error",
        "host adapter unavailable",
    )

    client.mail_claim("delivery_py", agent_id="agent_codex_py")
    assert calls[-1] == ("mail-claim", "delivery_py", "--agent-id", "agent_codex_py")

    wiki_mail_mark(tmp_path, "delivery_py", agent_id="agent_codex_py", state="done", binary="unused")
    assert calls[-1] == ("mail-mark", "delivery_py", "--agent-id", "agent_codex_py", "--state", "done")

    client.mail_snooze("delivery_py", agent_id="agent_codex_py", until="2026-05-21T08:00:00Z")
    assert calls[-1] == (
        "mail-snooze",
        "delivery_py",
        "--agent-id",
        "agent_codex_py",
        "--until",
        "2026-05-21T08:00:00Z",
    )

    wiki_notify_poll(tmp_path, "agent_codex_py", cursor="notifcur_1", binary="unused")
    assert calls[-1] == ("notify-poll", "agent_codex_py", "--cursor", "notifcur_1")

    wiki_notify_ack(tmp_path, "notif_py", agent_id="agent_codex_py", binary="unused")
    assert calls[-1] == ("notify-ack", "notif_py", "--agent-id", "agent_codex_py")

    client.notify_dispatch(
        "agent_codex_py",
        dry_run=True,
        steering_command="/usr/bin/true",
        steering_args=["--unused"],
        payload_format="json",
        limit=2,
    )
    assert calls[-1] == (
        "notify-dispatch",
        "agent_codex_py",
        "--dry-run",
        "--steering-command",
        "/usr/bin/true",
        "--steering-arg",
        "--unused",
        "--payload-format",
        "json",
        "--limit",
        "2",
    )

    wiki_mail_send(
        tmp_path,
        page="topics",
        subject="Python mail send",
        from_address="agent://codex/python-sender",
        to=["role://topics.curator"],
        body_markdown="Mail send should use talk append delivery.",
        operation_id="py-mail-001",
        binary="unused",
    )
    assert calls[-1] == (
        "talk-append",
        "--page",
        "topics",
        "--kind",
        "proposal",
        "--subject",
        "Python mail send",
        "--from",
        "agent://codex/python-sender",
        "--operation-id",
        "py-mail-001",
        "--delivery-mode",
        "mail",
        "--to",
        "role://topics.curator",
        "--body",
        "Mail send should use talk append delivery.",
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
    assert receipt["preflight"][0]["operation"] == "wiki.publish.preflight"
    assert receipt["preflight"][0]["action"] == "backfill_configured_pages"
    assert receipt["after"]["next_action"] == "none"

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
    assert stale_patch.value.operation == "wiki.page.patch_body"
    assert stale_patch.value.error_code == "source_hash_mismatch"
    assert "expected_source_sha256" in stale_patch.value.repair_hints[0]


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

    parent = client.talk_append(
        page="topics",
        kind="proposal",
        subject="Original adapter subject",
        from_address="agent://adapter-reply-thread",
        to=["role://topics.curator"],
        body_markdown="Parent message.",
    )
    reply = client.talk_append(
        page="topics",
        kind="reply",
        subject="Changed adapter subject",
        from_address="agent://adapter-reply-thread",
        to=["role://topics.curator"],
        body_markdown="Reply should not fork by subject.",
        reply_to=parent["message_id"],
    )
    thread_reply = wiki_talk_append(
        tmp_path,
        page="topics",
        kind="reply",
        subject="Explicit thread adapter subject",
        from_address="agent://adapter-reply-thread",
        body_markdown="Thread target should stay attached.",
        thread_id=parent["thread_id"],
        binary=binary,
    )

    assert reply["thread_id"] == parent["thread_id"]
    assert reply["reply_to"] == parent["message_id"]
    assert thread_reply["thread_id"] == parent["thread_id"]
    assert "reply_to" not in thread_reply

    talk_dir = tmp_path / "1Context/user-wiki/source/families/reference/topics/talk/topics.talk"
    messages = [
        path.read_text(encoding="utf-8")
        for path in talk_dir.glob("*.md")
        if not path.name.startswith("_")
    ]
    assert len(messages) == 3
    combined = "\n".join(messages)
    assert f'thread: "{parent["thread_id"]}"' in combined
    assert f'reply_to: "{parent["message_id"]}"' in combined
    assert "Explicit thread adapter subject" in combined


def test_wiki_module_helpers_cover_page_asset_and_lifecycle_metadata(tmp_path: Path) -> None:
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
        summary="Helper APIs stay thin and page scoped.",
        nav_section="utility",
        nav_order=44,
        binary=binary,
    )
    assert created["operation"] == "wiki.page.create"

    opened = wiki_page_open(tmp_path, "helper-test", binary=binary)
    assert opened["route"] == "/helper-test"
    written = wiki_page_write_body(
        tmp_path,
        "helper-test",
        body_markdown="# Helper Test\n\nInitial helper body.\n",
        expected_source_sha256=opened["hashes"]["source_sha256"],
        binary=binary,
    )
    patched = wiki_page_patch_body(
        tmp_path,
        "helper-test",
        find="Initial helper body.",
        replace="Initial helper body with a patch.",
        expected_source_sha256=written["hashes"]["source_sha256"],
        binary=binary,
    )
    assert patched["operation"] == "wiki.page.patch_body"

    image_path = tmp_path / "diagram.png"
    image_path.write_bytes(b"png fixture")
    asset = wiki_asset_add(
        tmp_path,
        "helper-test",
        file=image_path,
        filename="diagram.png",
        caption="Fixture diagram",
        alt_text="Fixture diagram",
        binary=binary,
    )
    assert asset["operation"] == "wiki.asset.add"
    assert wiki_asset_list(tmp_path, "helper-test", binary=binary)["asset_count"] == 1

    assert wiki_publish_status(tmp_path, binary=binary)["next_action"] == "publish"
    assert "helper-test" in {page["id"] for page in wiki_list(tmp_path, binary=binary)["pages"]}
    status = wiki_page_status(tmp_path, "helper-test", binary=binary)
    assert status["state"] == "needs_publish"
    deleted = wiki_page_delete(tmp_path, "helper-test", binary=binary)
    assert deleted["operation"] == "wiki.page.delete"
    restored = wiki_page_restore(tmp_path, "helper-test", binary=binary)
    assert restored["operation"] == "wiki.page.restore"
