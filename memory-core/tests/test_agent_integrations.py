from __future__ import annotations

from onectx.agent.integrations import build_install_plan, codex_hook_snippet


def test_install_plan_routes_hooks_to_onecontext_cli() -> None:
    plan = build_install_plan(command="/opt/1context/bin/1context")
    payload = plan.to_payload()

    claude_hooks = payload["claude"]["hook_payload"]["hooks"]["SessionStart"]
    assert {hook["matcher"] for hook in claude_hooks} == {"startup", "resume", "clear", "compact"}
    assert claude_hooks[0]["hooks"][0]["command"] == (
        "/opt/1context/bin/1context agent startup-context --provider claude"
    )
    assert "/opt/1context/bin/1context agent startup-context --provider codex" in payload["codex"]["toml_snippet"]
    assert payload["startup_config"]["message_template"]


def test_codex_hook_snippet_enables_hooks_feature() -> None:
    snippet = codex_hook_snippet("1context")

    assert "[features]" in snippet
    assert "codex_hooks = true" in snippet
    assert "[hooks]" in snippet
    assert 'matcher = "startup"' in snippet
    assert "1context agent startup-context --provider codex" in snippet
