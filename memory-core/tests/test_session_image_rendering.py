from __future__ import annotations

import json
from dataclasses import replace
from pathlib import Path

from onectx.config import load_system
from onectx.daemon.cursors import CursorStore
from onectx.memory.hour_experience import render_hour_experience_from_events
from onectx.ports import PortDefinition
from onectx.ports.sessions import import_session_port
from onectx.storage import LakeStore
from onectx.storage.hour_events import events_between


PNG_1X1 = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+/p9sAAAAASUVORK5CYII="


def test_session_inline_image_renders_above_message_text(tmp_path: Path) -> None:
    store = LakeStore(tmp_path / "lakestore")
    store.ensure()
    source = tmp_path / "source" / "rollout-image.jsonl"
    source.parent.mkdir()
    source.write_text(
        json.dumps(
            {
                "timestamp": "2026-04-29T12:00:00Z",
                "type": "response_item",
                "payload": {
                    "type": "message",
                    "role": "user",
                    "content": [
                        {"type": "input_text", "text": "Attached image validation after fix."},
                        {"type": "input_image", "image_url": f"data:image/png;base64,{PNG_1X1}"},
                    ],
                },
            }
        )
        + "\n",
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
        stores=("storage.events", "storage.artifacts"),
        purpose="test",
        source_path=tmp_path / "codex_sessions.toml",
        since="all",
    )

    result = import_session_port(root=tmp_path, port=port, store=store, cursors=CursorStore.load(tmp_path / "cursors.json"))

    assert result.events_imported == 1
    assert result.artifacts_imported >= 1
    events = events_between(store, start="2026-04-29T12:00:00Z", end="2026-04-29T13:00:00Z", sources=("codex",))
    assert len(events) == 1
    assert len(events[0].image_artifacts) == 1
    assert Path(events[0].image_artifacts[0].path).is_file()

    system = replace(
        load_system(Path.cwd()),
        runtime_dir=tmp_path / "runtime",
        storage_dir=tmp_path / "lakestore",
    )
    rendered = render_hour_experience_from_events(
        system,
        date="2026-04-29",
        hour="12",
        events=events,
    )

    stream = next((rendered.path / "streams").glob("*.md")).read_text(encoding="utf-8")
    image_index = stream.index("![Attached image 1](../assets/session-images/")
    text_index = stream.index("Attached image validation after fix.")
    assert image_index < text_index
    copied_images = list((rendered.path / "assets" / "session-images").glob("*.png"))
    assert len(copied_images) == 1
    assert copied_images[0].stat().st_size > 0


def test_codex_local_image_event_msg_is_materialized_and_rendered(tmp_path: Path) -> None:
    store = LakeStore(tmp_path / "lakestore")
    store.ensure()
    local_image = tmp_path / "codex-clipboard.png"
    local_image.write_bytes(_decode_png())
    source = tmp_path / "source" / "rollout-local-image.jsonl"
    source.parent.mkdir()
    source.write_text(
        "\n".join(
            [
                json.dumps(
                    {
                        "timestamp": "2026-04-29T12:00:00Z",
                        "type": "session_meta",
                        "payload": {"id": "local-image-session", "cwd": str(tmp_path)},
                    }
                ),
                json.dumps(
                    {
                        "timestamp": "2026-04-29T12:01:00Z",
                        "type": "event_msg",
                        "payload": {
                            "type": "user_message",
                            "message": "Attached image validation after fix.",
                            "images": [],
                            "local_images": [str(local_image)],
                            "text_elements": [{"placeholder": "[Image #1]"}],
                        },
                    }
                ),
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
        stores=("storage.events", "storage.artifacts"),
        purpose="test",
        source_path=tmp_path / "codex_sessions.toml",
        since="all",
    )

    result = import_session_port(root=tmp_path, port=port, store=store, cursors=CursorStore.load(tmp_path / "cursors.json"))

    assert result.events_imported == 1
    assert result.artifacts_imported >= 1
    events = events_between(store, start="2026-04-29T12:00:00Z", end="2026-04-29T13:00:00Z", sources=("codex",))
    assert len(events) == 1
    assert events[0].text == "Attached image validation after fix."
    assert len(events[0].image_artifacts) == 1

    system = replace(
        load_system(Path.cwd()),
        runtime_dir=tmp_path / "runtime",
        storage_dir=tmp_path / "lakestore",
    )
    rendered = render_hour_experience_from_events(system, date="2026-04-29", hour="12", events=events)
    stream = next((rendered.path / "streams").glob("*.md")).read_text(encoding="utf-8")

    assert stream.index("![Attached image 1](../assets/session-images/") < stream.index("Attached image validation after fix.")
    assert list((rendered.path / "assets" / "session-images").glob("*.png"))


def _decode_png() -> bytes:
    import base64

    return base64.b64decode(PNG_1X1)
