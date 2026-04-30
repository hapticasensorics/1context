from __future__ import annotations

from pathlib import Path

import pytest

from onectx.daemon.cursors import CursorStore
from onectx.ports import PortDefinition
from onectx.ports import PortError, load_ports
from onectx.ports import sessions as session_import
from onectx.ports.sessions import import_session_port
from onectx.storage import LakeStore


def test_since_filters_sources_but_does_not_prune_existing_events(tmp_path: Path) -> None:
    """The port `since` value is an import horizon, not retention."""

    store = LakeStore(tmp_path / "lakestore")
    store.ensure()
    store.append_event(
        "session.codex.imported",
        event_id="old-already-imported",
        hash="old-hash",
        session_id="existing-session",
        ts="2000-01-01T00:00:00Z",
        source="codex",
        kind="user",
        text="This old imported row must stay in the lake.",
    )

    source = tmp_path / "source" / "rollout-test.jsonl"
    source.parent.mkdir()
    source.write_text(
        '{"timestamp":"2000-01-01T00:00:00Z","type":"response_item",'
        '"payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"old source row"}]}}\n',
        encoding="utf-8",
    )

    port = PortDefinition(
        id="codex_sessions",
        label="Codex Sessions",
        kind="session_log",
        adapter="codex_rollout_jsonl",
        enabled=True,
        directions=("input",),
        paths=(str(source),),
        stores=("storage.events", "storage.sessions", "storage.artifacts"),
        purpose="test",
        source_path=tmp_path / "codex_sessions.toml",
        since="2099-01-01T00:00:00Z",
    )
    cursors = CursorStore.load(tmp_path / "cursors" / "daemon.json")

    result = import_session_port(root=tmp_path, port=port, store=store, cursors=cursors)

    assert result.events_imported == 0
    assert store.counts()["events"] == 1
    rows = store.rows("events", limit=0)
    assert [row["event_id"] for row in rows] == ["old-already-imported"]


def test_unchanged_session_file_does_not_load_dedupe_sets(tmp_path: Path, monkeypatch) -> None:
    """A quiet daemon tick should not scan large Lance columns."""

    store = LakeStore(tmp_path / "lakestore")
    store.ensure()

    source = tmp_path / "source" / "rollout-test.jsonl"
    source.parent.mkdir()
    source.write_text(
        '{"timestamp":"2026-04-01T00:00:00Z","type":"response_item",'
        '"payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"hello"}]}}\n',
        encoding="utf-8",
    )

    port = PortDefinition(
        id="codex_sessions",
        label="Codex Sessions",
        kind="session_log",
        adapter="codex_rollout_jsonl",
        enabled=True,
        directions=("input",),
        paths=(str(source),),
        stores=("storage.events", "storage.sessions", "storage.artifacts"),
        purpose="test",
        source_path=tmp_path / "codex_sessions.toml",
        since="all",
    )
    cursors = CursorStore.load(tmp_path / "cursors" / "daemon.json")
    cursors.set(
        f"{port.id}:{source.resolve()}",
        {
            "offset": source.stat().st_size,
            "path": str(source.resolve()),
            "port_id": port.id,
            "adapter": port.adapter,
        },
    )

    def fail_existing_values(*args, **kwargs):
        raise AssertionError("quiet import should not load dedupe columns")

    monkeypatch.setattr(session_import, "existing_values", fail_existing_values)

    result = import_session_port(root=tmp_path, port=port, store=store, cursors=cursors)

    assert result.files_seen == 1
    assert result.events_imported == 0


def test_malformed_jsonl_line_is_skipped_without_blocking_later_events(tmp_path: Path) -> None:
    store = LakeStore(tmp_path / "lakestore")
    store.ensure()
    source = tmp_path / "source" / "rollout-test.jsonl"
    source.parent.mkdir()
    source.write_text(
        "\n".join(
            [
                '{"timestamp":"2026-04-01T00:00:00Z","type":"response_item",'
                '"payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"first"}]}}',
                "{bad json",
                '{"timestamp":"2026-04-01T00:01:00Z","type":"response_item",'
                '"payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"second"}]}}',
                "",
            ]
        ),
        encoding="utf-8",
    )
    port = PortDefinition(
        id="codex_sessions",
        label="Codex Sessions",
        kind="session_log",
        adapter="codex_rollout_jsonl",
        enabled=True,
        directions=("input",),
        paths=(str(source),),
        stores=("storage.events",),
        purpose="test",
        source_path=tmp_path / "codex_sessions.toml",
        since="all",
    )

    result = import_session_port(root=tmp_path, port=port, store=store, cursors=CursorStore.load(tmp_path / "cursors.json"))

    assert result.events_imported == 2
    assert result.events_skipped == 1
    assert store.counts()["events"] == 2


def test_bad_cursor_offset_is_treated_as_zero(tmp_path: Path) -> None:
    store = LakeStore(tmp_path / "lakestore")
    store.ensure()
    source = tmp_path / "source" / "rollout-test.jsonl"
    source.parent.mkdir()
    source.write_text(
        '{"timestamp":"2026-04-01T00:00:00Z","type":"response_item",'
        '"payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"hello"}]}}\n',
        encoding="utf-8",
    )
    port = PortDefinition(
        id="codex_sessions",
        label="Codex Sessions",
        kind="session_log",
        adapter="codex_rollout_jsonl",
        enabled=True,
        directions=("input",),
        paths=(str(source),),
        stores=("storage.events",),
        purpose="test",
        source_path=tmp_path / "codex_sessions.toml",
        since="all",
    )
    cursors = CursorStore.load(tmp_path / "cursors.json")
    cursors.set(f"{port.id}:{source}", {"offset": "not-an-int"})

    result = import_session_port(root=tmp_path, port=port, store=store, cursors=cursors)

    assert result.events_imported == 1


def test_corrupt_cursor_json_loads_empty_store(tmp_path: Path) -> None:
    path = tmp_path / "storage" / "cursors" / "daemon.json"
    path.parent.mkdir(parents=True)
    path.write_text("{bad json", encoding="utf-8")

    store = CursorStore.load(path)

    assert store.get("anything") == {}
    assert store.data["corrupt_cursor_path"] == str(path)


def test_port_paths_must_be_array(tmp_path: Path) -> None:
    ports = tmp_path / "ports"
    ports.mkdir()
    (ports / "codex.toml").write_text(
        "\n".join(
            [
                'id = "codex"',
                'adapter = "codex_rollout_jsonl"',
                "enabled = true",
                'directions = ["input"]',
                'paths = "not-an-array"',
                'stores = ["storage.events"]',
            ]
        ),
        encoding="utf-8",
    )

    with pytest.raises(PortError, match="paths must be an array"):
        load_ports(tmp_path)
