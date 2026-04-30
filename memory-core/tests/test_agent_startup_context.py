from __future__ import annotations

import json

from onectx.agent.startup_context import build_startup_context


def test_startup_context_renders_default_hook_payload(tmp_path, monkeypatch):
    monkeypatch.setenv("ONECTX_STARTUP_CONTEXT_CONFIG", str(tmp_path / "missing-global.json"))
    (tmp_path / "1context.toml").write_text('active_plugin = "base-memory-v1"\n', encoding="utf-8")

    context = build_startup_context(provider="claude", cwd=tmp_path)

    assert context.repo_root == tmp_path
    assert "Local wiki: http://127.0.0.1:17319/for-you" in context.message
    assert "1Context Librarian" in context.message
    assert context.hook_payload() == {
        "hookSpecificOutput": {
            "hookEventName": "SessionStart",
            "additionalContext": context.message,
        }
    }


def test_startup_context_uses_runtime_template_config(tmp_path, monkeypatch):
    monkeypatch.setenv("ONECTX_STARTUP_CONTEXT_CONFIG", str(tmp_path / "missing-global.json"))
    (tmp_path / "AGENTS.md").write_text("# notes\n", encoding="utf-8")
    config_path = tmp_path / "memory" / "runtime" / "agent" / "startup-context.json"
    config_path.parent.mkdir(parents=True)
    config_path.write_text(
        json.dumps(
            {
                "wiki_url": "http://127.0.0.1:9999/custom",
                "message_template": "{provider}:{repo_root}:{wiki_url}:{search_url}",
            }
        ),
        encoding="utf-8",
    )

    context = build_startup_context(provider="codex", cwd=tmp_path)

    assert context.config_paths == (config_path,)
    assert context.message == (
        f"codex:{tmp_path}:http://127.0.0.1:9999/custom:"
        "http://127.0.0.1:9999/custom/api/wiki/search?q=wiki"
    )


def test_disabled_startup_context_returns_empty_hook_payload(tmp_path, monkeypatch):
    monkeypatch.setenv("ONECTX_STARTUP_CONTEXT_CONFIG", str(tmp_path / "missing-global.json"))
    (tmp_path / "1context.toml").write_text('active_plugin = "base-memory-v1"\n', encoding="utf-8")
    config_path = tmp_path / "memory" / "runtime" / "agent" / "startup-context.json"
    config_path.parent.mkdir(parents=True)
    config_path.write_text('{"enabled": false}', encoding="utf-8")

    context = build_startup_context(provider="claude", cwd=tmp_path)

    assert context.enabled is False
    assert context.hook_payload() == {}
