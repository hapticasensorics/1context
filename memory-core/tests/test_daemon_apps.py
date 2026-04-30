from __future__ import annotations

import json
import os
import sys
from pathlib import Path
from types import SimpleNamespace

import pytest

from onectx.daemon.apps import AppError, app_status, load_apps, load_registry, start_app, stop_app


def system_for_tmp(root: Path) -> SimpleNamespace:
    return SimpleNamespace(
        root=root,
        runtime_dir=root / "memory" / "runtime",
        storage_dir=root / "storage" / "lakestore",
    )


def test_start_app_reports_immediate_startup_exit(tmp_path: Path) -> None:
    command = [sys.executable, "-c", "import sys; print('boom from app'); sys.exit(7)"]
    (tmp_path / "apps").mkdir()
    (tmp_path / "apps" / "apps.toml").write_text(
        "\n".join(
            [
                "[[apps]]",
                'id = "fail-fast"',
                'label = "Fail Fast"',
                'path = "."',
                f"command = {json.dumps(command)}",
                'url = "http://127.0.0.1:9/"',
                'health_url = "http://127.0.0.1:9/__health"',
            ]
        ),
        encoding="utf-8",
    )
    system = system_for_tmp(tmp_path)

    with pytest.raises(AppError) as excinfo:
        start_app(system, "fail-fast")

    assert "exited during startup with code 7" in str(excinfo.value)
    assert "boom from app" in str(excinfo.value)
    registry = load_registry(system)
    assert registry["apps"]["fail-fast"]["status"] == "failed"
    assert registry["apps"]["fail-fast"]["exit_code"] == 7
    assert app_status(system)[0]["status"] == "failed"


def test_stop_app_refuses_stale_reused_pid(tmp_path: Path) -> None:
    (tmp_path / "apps").mkdir()
    (tmp_path / "apps" / "apps.toml").write_text(
        "\n".join(
            [
                "[[apps]]",
                'id = "stale"',
                'label = "Stale"',
                'path = "."',
                'command = ["definitely-not-this-process"]',
                'url = "http://127.0.0.1:9/"',
            ]
        ),
        encoding="utf-8",
    )
    system = system_for_tmp(tmp_path)
    registry = system.runtime_dir / "processes" / "apps.json"
    registry.parent.mkdir(parents=True)
    registry.write_text(
        json.dumps(
            {
                "version": "0.1",
                "apps": {
                    "stale": {
                        "pid": os.getpid(),
                        "status": "running",
                        "command": ["definitely-not-this-process"],
                    }
                },
            }
        ),
        encoding="utf-8",
    )

    result = stop_app(system, "stale")

    assert result["status"] == "stale_pid_refused"
    assert load_registry(system)["apps"]["stale"]["status"] == "stale_pid"


def test_corrupt_app_registry_does_not_crash_status(tmp_path: Path) -> None:
    (tmp_path / "apps").mkdir()
    (tmp_path / "apps" / "apps.toml").write_text(
        "\n".join(
            [
                "[[apps]]",
                'id = "ok"',
                'label = "OK"',
                'path = "."',
                f"command = {json.dumps([sys.executable, '-c', 'import time; time.sleep(30)'])}",
                'url = "http://127.0.0.1:9/"',
            ]
        ),
        encoding="utf-8",
    )
    system = system_for_tmp(tmp_path)
    path = system.runtime_dir / "processes" / "apps.json"
    path.parent.mkdir(parents=True)
    path.write_text("{bad json", encoding="utf-8")

    assert app_status(system)[0]["status"] == "stopped"


def test_app_command_must_be_array(tmp_path: Path) -> None:
    (tmp_path / "apps").mkdir()
    (tmp_path / "apps" / "apps.toml").write_text(
        "\n".join(
            [
                "[[apps]]",
                'id = "bad"',
                'path = "."',
                'command = "python -m http.server"',
            ]
        ),
        encoding="utf-8",
    )

    with pytest.raises(AppError, match="command must be an array"):
        load_apps(tmp_path)
