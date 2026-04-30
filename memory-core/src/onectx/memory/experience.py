from __future__ import annotations

import fnmatch
from pathlib import Path
from typing import Any


class ExperienceError(RuntimeError):
    """Raised when runtime experience cannot be routed."""


def configured_native_memory_formats(system: Any) -> dict[str, dict[str, Any]]:
    return dict(system.native_memory_formats)


def configured_providers(system: Any) -> dict[str, dict[str, Any]]:
    return dict(system.providers)


def native_memory_paths_for_experience(system: Any, experience_path: Path) -> dict[str, Path]:
    return {
        format_id: native_memory_path(experience_path, memory_format)
        for format_id, memory_format in configured_native_memory_formats(system).items()
    }


def resolve_native_memory_route(
    system: Any,
    *,
    harness: str | None = None,
    provider: str | None = None,
    model: str | None = None,
    experience_path: Path | None = None,
) -> dict[str, Any]:
    routes = configured_providers(system)
    if not routes:
        raise ExperienceError("no providers configured")
    route = match_provider(routes, provider=provider, model=model)
    formats = configured_native_memory_formats(system)
    if not formats:
        raise ExperienceError("no native memory formats configured")
    harness_id = (harness or "").strip()
    harness_manifest = None
    if harness_id:
        harness_manifest = system.harnesses.get(harness_id)
        if not harness_manifest:
            raise ExperienceError(f"unknown harness {harness_id!r}")

    read_memory_format = str(
        (harness_manifest or {}).get("primary_memory_format") or route.get("read_memory_format", "")
    ).strip()
    if read_memory_format not in formats:
        owner = f"harness {harness_id!r}" if harness_id else f"provider {route['id']!r}"
        raise ExperienceError(f"{owner} reads unknown native memory format {read_memory_format!r}")
    write_memory_formats = (
        [read_memory_format] if harness_id else list(route.get("write_memory_formats") or [read_memory_format])
    )
    unknown_output_memory_formats = [format_id for format_id in write_memory_formats if format_id not in formats]
    if unknown_output_memory_formats:
        owner = f"harness {harness_id!r}" if harness_id else f"provider {route['id']!r}"
        raise ExperienceError(f"{owner} writes unknown native memory formats: {', '.join(unknown_output_memory_formats)}")

    result: dict[str, Any] = {
        "harness": harness_id,
        "provider": route["id"],
        "model": model or "",
        "read_memory_format": read_memory_format,
        "read_kind": formats[read_memory_format].get("kind", ""),
        "write_memory_formats": write_memory_formats,
    }
    if experience_path is not None:
        result["read_path"] = str(native_memory_path(experience_path, formats[read_memory_format]))
        result["native_memory_paths"] = {
            format_id: str(native_memory_path(experience_path, formats[format_id]))
            for format_id in write_memory_formats
        }
    return result


def match_provider(
    routes: dict[str, dict[str, Any]],
    *,
    provider: str | None,
    model: str | None,
) -> dict[str, Any]:
    provider_key = (provider or "").strip().lower()
    model_name = (model or "").strip()

    if provider_key:
        for route in routes.values():
            names = {str(route.get("id", "")).lower()}
            names.update(str(alias).lower() for alias in route.get("aliases", []))
            if provider_key in names:
                return route
        raise ExperienceError(f"unknown provider {provider!r}")

    if model_name:
        for route in routes.values():
            for pattern in route.get("model_patterns", []):
                if fnmatch.fnmatchcase(model_name.lower(), str(pattern).lower()):
                    return route
        raise ExperienceError(f"could not infer provider for model {model!r}; pass --provider or add model_patterns")

    return next(iter(routes.values()))


def native_memory_path(experience_path: Path, memory_format: dict[str, Any]) -> Path:
    relative = str(memory_format.get("path", "")).strip()
    if not relative:
        raise ExperienceError(f"native memory format {memory_format.get('id', '<unknown>')!r} is missing path")
    return experience_path / relative
