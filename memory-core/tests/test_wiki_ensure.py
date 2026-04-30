from __future__ import annotations

from onectx.wiki.ensure import EnsureResult, archive_expired_conversations
from onectx.wiki.librarian import build_librarian_prompt, provider_environment, provider_failure_message, provider_path


def test_archive_expired_conversation_entries(tmp_path):
    talk_folder = tmp_path / "example.talk"
    archive_folder = talk_folder / "archive"
    archive_folder.mkdir(parents=True)
    (talk_folder / "_meta.yaml").write_text("archive_after_days: 90\n", encoding="utf-8")
    old_conversation = talk_folder / "2000-01-01T00-00Z.conversation.md"
    old_conversation.write_text(
        "---\nkind: conversation\nts: 2000-01-01T00:00:00Z\n---\n\nOld conversation.\n",
        encoding="utf-8",
    )
    current_conversation = talk_folder / "2999-01-01T00-00Z.conversation.md"
    current_conversation.write_text(
        "---\nkind: conversation\nts: 2999-01-01T00:00:00Z\n---\n\nCurrent conversation.\n",
        encoding="utf-8",
    )
    old_proposal = talk_folder / "2000-01-01T00-00Z.proposal.md"
    old_proposal.write_text(
        "---\nkind: proposal\nts: 2000-01-01T00:00:00Z\n---\n\nOld proposal.\n",
        encoding="utf-8",
    )

    result = EnsureResult(family=None)  # type: ignore[arg-type]
    archive_expired_conversations(talk_folder, result)

    assert not old_conversation.exists()
    assert (archive_folder / old_conversation.name).read_text(encoding="utf-8").endswith("Old conversation.\n")
    assert current_conversation.exists()
    assert old_proposal.exists()
    assert result.archived == [archive_folder / old_conversation.name]


def test_librarian_prompt_carries_session_metadata(tmp_path):
    (tmp_path / "wiki").mkdir()
    system = type(
        "System",
        (),
        {
            "root": tmp_path,
            "runtime_dir": tmp_path / "memory" / "runtime",
        },
    )()
    prompt = build_librarian_prompt(
        system,
        "Who am I?",
        {
            "route": "/for-you",
            "origin": "http://127.0.0.1:17319",
            "page": {"title": "For You"},
        },
        state={"thread_id": "thread-123", "claude_session_id": "claude-123"},
        turn={"turn_id": "turn-123", "thread_id": "thread-123"},
    )

    assert '"agent_role": "wiki.chat_librarian"' in prompt
    assert '"display_role": "1Context Librarian"' in prompt
    assert '"surface": "localhost_librarian"' in prompt
    assert '"wiki_route": "/for-you"' in prompt
    assert '"thread_id": "thread-123"' in prompt
    assert '"claude_session_id": "claude-123"' in prompt


def test_librarian_provider_path_ignores_ambient_path(tmp_path, monkeypatch):
    fake = tmp_path / "codex"
    fake.write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")
    fake.chmod(0o755)
    monkeypatch.setenv("PATH", str(tmp_path))

    assert provider_path("codex") != str(fake)


def test_librarian_provider_environment_drops_1context_overrides(monkeypatch):
    monkeypatch.setenv("ONECONTEXT_MEMORY_CORE_ROOT", "/tmp/private-root")
    monkeypatch.setenv("OPENAI_API_KEY", "secret")
    monkeypatch.setenv("HOME", "/Users/example")
    monkeypatch.setenv("LC_ALL", "C.UTF-8")
    monkeypatch.setenv("LC_CTYPE", "C.UTF-8")

    env = provider_environment({"CODEX_HOME": "/tmp/codex-home"})

    assert env["HOME"] == "/Users/example"
    assert env["CODEX_HOME"] == "/tmp/codex-home"
    assert env["LANG"] == "en_US.UTF-8"
    assert env["LC_CTYPE"] == "en_US.UTF-8"
    assert "LC_ALL" not in env
    assert "ONECONTEXT_MEMORY_CORE_ROOT" not in env
    assert "OPENAI_API_KEY" not in env


def test_librarian_claude_login_error_is_human_readable():
    message = provider_failure_message(
        "claude",
        '{"type":"result","is_error":true,"result":"Not logged in · Please run /login"}',
        "",
    )

    assert "Claude Code is not logged in" in message
    assert "{" not in message
