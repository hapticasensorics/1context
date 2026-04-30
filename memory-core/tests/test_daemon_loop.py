from __future__ import annotations

from pathlib import Path

from onectx.daemon import loop
from onectx.ports import PortDefinition


def test_daemon_run_once_isolates_one_bad_port(tmp_path: Path, monkeypatch) -> None:
    (tmp_path / "1context.toml").write_text(
        'active_plugin = "base-memory-v1"\n'
        f'plugin_dirs = ["{Path.cwd() / "memory" / "plugins"}"]\n'
        'runtime_dir = "memory/runtime"\n'
        'storage_dir = "storage/lakestore"\n',
        encoding="utf-8",
    )
    port = PortDefinition(
        id="bad",
        label="Bad",
        kind="session_log",
        adapter="codex_rollout_jsonl",
        enabled=True,
        directions=("input",),
        paths=(),
        stores=("storage.events",),
        purpose="test",
        source_path=tmp_path / "ports" / "bad.toml",
    )

    monkeypatch.setattr(loop, "load_ports", lambda _root: (port,))

    def fail_import(**_kwargs):
        raise RuntimeError("boom")

    monkeypatch.setattr(loop, "import_session_port", fail_import)

    result = loop.run_once(root=tmp_path)

    assert result.port_results[0]["skipped"] is True
    assert result.port_results[0]["error"]["type"] == "RuntimeError"
    assert result.tick_event_id
