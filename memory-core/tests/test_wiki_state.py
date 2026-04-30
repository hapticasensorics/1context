from __future__ import annotations

from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
from types import SimpleNamespace

from onectx.wiki.state import load_user_state, save_user_state


def system_for_tmp(root: Path) -> SimpleNamespace:
    return SimpleNamespace(runtime_dir=root / "memory" / "runtime")


def test_user_state_concurrent_writes_do_not_corrupt_file(tmp_path: Path) -> None:
    system = system_for_tmp(tmp_path)

    def write(index: int) -> None:
        save_user_state(system, {"settings": {"theme": "dark" if index % 2 else "light"}})

    with ThreadPoolExecutor(max_workers=8) as pool:
        list(pool.map(write, range(24)))

    state, exists = load_user_state(system)
    assert exists is True
    assert state["schema_version"] == "wiki.user-state.v1"
    assert state["settings"]["theme"] in {"light", "dark"}
