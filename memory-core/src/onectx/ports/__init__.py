from __future__ import annotations

import glob
import tomllib
from dataclasses import dataclass
from pathlib import Path
from typing import Any


class PortError(RuntimeError):
    """Raised when a port definition cannot be loaded."""


@dataclass(frozen=True)
class PortDefinition:
    id: str
    label: str
    kind: str
    adapter: str
    enabled: bool
    directions: tuple[str, ...]
    paths: tuple[str, ...]
    stores: tuple[str, ...]
    purpose: str
    source_path: Path
    since: str = ""
    max_events_per_tick: int = 0
    max_lines_per_tick: int = 0
    settings_path: Path | None = None

    def to_payload(self) -> dict[str, Any]:
        return {
            "id": self.id,
            "label": self.label,
            "kind": self.kind,
            "adapter": self.adapter,
            "enabled": self.enabled,
            "directions": list(self.directions),
            "paths": list(self.paths),
            "stores": list(self.stores),
            "purpose": self.purpose,
            "source_path": str(self.source_path),
            "since": self.since,
            "max_events_per_tick": self.max_events_per_tick,
            "max_lines_per_tick": self.max_lines_per_tick,
            "settings_path": str(self.settings_path) if self.settings_path else "",
        }


def load_ports(root: Path | str) -> tuple[PortDefinition, ...]:
    root = Path(root)
    ports_dir = root / "ports"
    if not ports_dir.is_dir():
        return ()

    panel = load_ports_panel(root)
    panel_enabled = bool(panel.get("enabled", True))
    defaults = panel.get("defaults", {}) if isinstance(panel.get("defaults"), dict) else {}
    overrides = port_overrides(panel)
    ports: list[PortDefinition] = []
    seen: set[str] = set()
    for manifest in sorted(ports_dir.glob("*.toml")):
        raw = tomllib.loads(manifest.read_text(encoding="utf-8"))
        port_id = str(raw.get("id", manifest.stem)).strip()
        if not port_id:
            raise PortError(f"{manifest} is missing id")
        if port_id in seen:
            raise PortError(f"duplicate port id {port_id!r} in {manifest}")
        seen.add(port_id)
        override = overrides.get(port_id, {})
        enabled_value = override.get("enabled", defaults.get("enabled", raw.get("enabled", False)))
        paths_value = override.get("paths", raw.get("paths", []))
        since_value = override.get("since", defaults.get("since", raw.get("since", "")))
        max_events_value = override.get(
            "max_events_per_tick",
            defaults.get("max_events_per_tick", raw.get("max_events_per_tick", 0)),
        )
        max_lines_value = override.get(
            "max_lines_per_tick",
            defaults.get("max_lines_per_tick", raw.get("max_lines_per_tick", 0)),
        )
        ports.append(
            PortDefinition(
                id=port_id,
                label=str(override.get("label", raw.get("label", port_id))),
                kind=str(override.get("kind", raw.get("kind", ""))),
                adapter=str(raw.get("adapter", "")),
                enabled=panel_enabled and bool(enabled_value),
                directions=tuple(str(item) for item in typed_list(override.get("directions", raw.get("directions", [])), "directions")),
                paths=tuple(str(item) for item in typed_list(paths_value, "paths")),
                stores=tuple(str(item) for item in typed_list(override.get("stores", raw.get("stores", [])), "stores")),
                purpose=str(override.get("purpose", raw.get("purpose", ""))),
                source_path=manifest,
                since=str(since_value).strip(),
                max_events_per_tick=parse_nonnegative_int(max_events_value, "max_events_per_tick"),
                max_lines_per_tick=parse_nonnegative_int(max_lines_value, "max_lines_per_tick"),
                settings_path=(root / "ports.toml") if override or defaults else None,
            )
        )
    return tuple(ports)


def load_ports_panel(root: Path) -> dict[str, Any]:
    path = root / "ports.toml"
    if not path.exists():
        return {}
    raw = tomllib.loads(path.read_text(encoding="utf-8"))
    ports = raw.get("ports", [])
    if not isinstance(ports, list):
        raise PortError(f"{path} section 'ports' must be an array of tables")
    return raw


def ports_watch_interval(root: Path | str, default: float = 300.0) -> float:
    panel = load_ports_panel(Path(root))
    value = panel.get("watch_interval_seconds", default)
    try:
        interval = float(value)
    except (TypeError, ValueError) as exc:
        raise PortError("ports.toml watch_interval_seconds must be a number") from exc
    if interval <= 0:
        raise PortError("ports.toml watch_interval_seconds must be greater than 0")
    return interval


def port_overrides(panel: dict[str, Any]) -> dict[str, dict[str, Any]]:
    result: dict[str, dict[str, Any]] = {}
    for index, record in enumerate(panel.get("ports", [])):
        if not isinstance(record, dict):
            raise PortError(f"ports.toml ports item {index} must be a table")
        port_id = str(record.get("id", "")).strip()
        if not port_id:
            raise PortError(f"ports.toml ports item {index} is missing id")
        if port_id in result:
            raise PortError(f"duplicate port id {port_id!r} in ports.toml")
        result[port_id] = dict(record)
    return result


def parse_nonnegative_int(value: Any, label: str) -> int:
    try:
        parsed = int(value or 0)
    except (TypeError, ValueError) as exc:
        raise PortError(f"{label} must be a non-negative integer") from exc
    if parsed < 0:
        raise PortError(f"{label} must be a non-negative integer")
    return parsed


def typed_list(value: Any, label: str) -> list[Any]:
    if value is None:
        return []
    if not isinstance(value, list):
        raise PortError(f"{label} must be an array")
    return value


def resolve_port_files(root: Path | str, port: PortDefinition, *, source_root: Path | None = None) -> list[Path]:
    patterns = source_patterns(port, source_root) if source_root else list(port.paths)
    files: list[Path] = []
    for pattern in patterns:
        expanded = expand_pattern(Path(root), pattern)
        files.extend(path for path in expanded if path.is_file())
    return sorted(unique_paths(files))


def source_patterns(port: PortDefinition, source_root: Path | None) -> list[str]:
    if not source_root:
        return list(port.paths)
    subdir_by_adapter = {
        "codex_rollout_jsonl": ("codex", "rollout-*.jsonl"),
        "claude_code_jsonl": ("claude", "*.jsonl"),
    }
    source = subdir_by_adapter.get(port.adapter)
    if not source:
        return []
    subdir, filename = source
    return [str(source_root / subdir / "**" / filename)]


def expand_pattern(root: Path, pattern: str) -> list[Path]:
    path = Path(pattern).expanduser()
    if not path.is_absolute():
        path = root / path
    return [Path(match).resolve() for match in glob.glob(str(path), recursive=True)]


def unique_paths(paths: list[Path]) -> list[Path]:
    result: list[Path] = []
    seen: set[Path] = set()
    for path in paths:
        if path in seen:
            continue
        result.append(path)
        seen.add(path)
    return result
