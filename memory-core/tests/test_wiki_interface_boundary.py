from __future__ import annotations

import importlib.util
from pathlib import Path


def test_wiki_interface_replaces_memory_wiki_compat_modules() -> None:
    package_root = Path(__file__).resolve().parents[1]
    memory_root = package_root / "src/onectx/memory"
    interface_root = package_root / "src/onectx/wiki_interface"

    old_modules = [
        "onectx.memory.wiki",
        "onectx.memory.wiki_apply",
        "onectx.memory.wiki_authoring",
        "onectx.memory.wiki_executor",
        "onectx.memory.wiki_validators",
    ]
    for module in old_modules:
        assert importlib.util.find_spec(module) is None
    for module in [
        "onectx.wiki_interface.apply",
        "onectx.wiki_interface.executor",
        "onectx.wiki_interface.planning",
        "onectx.wiki_interface.validators",
    ]:
        assert importlib.util.find_spec(module) is None

    old_files = [
        "wiki.py",
        "wiki_apply.py",
        "wiki_authoring.py",
        "wiki_executor.py",
        "wiki_validators.py",
    ]
    for filename in old_files:
        assert not (memory_root / filename).exists()

    readme = (interface_root / "README.md").read_text(encoding="utf-8")
    assert "Python boundary for wiki-facing memory work" in readme
    assert "This folder does not own" in readme
