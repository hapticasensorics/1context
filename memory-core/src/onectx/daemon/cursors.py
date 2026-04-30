from __future__ import annotations

from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

from onectx.io_utils import atomic_write_json, exclusive_file_lock, read_json_object


@dataclass
class CursorStore:
    path: Path
    data: dict[str, Any] = field(default_factory=dict)

    @classmethod
    def load(cls, path: Path) -> "CursorStore":
        if not path.exists():
            return cls(path=path, data={"version": "0.1", "cursors": {}})
        payload = read_json_object(path)
        if payload is None:
            return cls(path=path, data={"version": "0.1", "cursors": {}, "corrupt_cursor_path": str(path)})
        payload.setdefault("version", "0.1")
        payload.setdefault("cursors", {})
        return cls(path=path, data=payload)

    def get(self, key: str) -> dict[str, Any]:
        cursors = self.data.setdefault("cursors", {})
        value = cursors.get(key, {})
        return value if isinstance(value, dict) else {}

    def set(self, key: str, value: dict[str, Any]) -> None:
        cursors = self.data.setdefault("cursors", {})
        cursors[key] = value

    def save(self) -> None:
        with exclusive_file_lock(self.path.with_suffix(self.path.suffix + ".lock")):
            atomic_write_json(self.path, self.data)

    def to_payload(self) -> dict[str, Any]:
        return {
            "path": str(self.path),
            "cursor_count": len(self.data.get("cursors", {})),
        }
