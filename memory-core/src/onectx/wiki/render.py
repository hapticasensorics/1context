from __future__ import annotations

import subprocess
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from .families import WikiError, WikiFamily, family_by_id, format_path


AUDIENCE_SOURCE_SUFFIXES = (".private.md", ".internal.md", ".public.md")


@dataclass(frozen=True)
class RenderInvocation:
    input_path: Path
    stdout: str
    stderr: str

    def to_payload(self, root: Path) -> dict[str, Any]:
        return {
            "input_path": format_path(self.input_path, root),
            "stdout": self.stdout,
            "stderr": self.stderr,
        }


@dataclass(frozen=True)
class WikiRenderResult:
    family: WikiFamily
    output_dir: Path
    invocations: tuple[RenderInvocation, ...]
    outputs: tuple[Path, ...]
    manifest_path: Path | None = None
    manifest: dict[str, Any] | None = None

    def to_payload(self, root: Path) -> dict[str, Any]:
        return {
            "family": self.family.to_payload(root),
            "output_dir": format_path(self.output_dir, root),
            "invocations": [item.to_payload(root) for item in self.invocations],
            "outputs": [format_path(path, root) for path in self.outputs],
            "manifest_path": format_path(self.manifest_path, root) if self.manifest_path else "",
            "manifest": self.manifest or {},
        }


def render_family(
    root: Path | str,
    family: WikiFamily | str,
    *,
    output_dir: Path | str | None = None,
    include_talk: bool = True,
) -> WikiRenderResult:
    from .manifest import MANIFEST_FILENAME, build_render_manifest, write_render_manifest

    root = Path(root).resolve()
    resolved_family = family_by_id(root, family) if isinstance(family, str) else family
    engine_root = root / "wiki-engine"
    render_tool = engine_root / "tools" / "render-to-dir.mjs"
    if not render_tool.exists():
        raise WikiError(f"missing wiki engine render tool: {render_tool}")

    output_path = resolve_output_dir(root, resolved_family, output_dir)
    output_path.mkdir(parents=True, exist_ok=True)

    inputs = family_render_inputs(resolved_family, include_talk=include_talk)
    if not inputs:
        raise WikiError(f"wiki family {resolved_family.id!r} has no renderable source or talk inputs")

    invocations: list[RenderInvocation] = []
    for input_path in inputs:
        result = subprocess.run(
            ["node", str(render_tool), str(input_path), str(output_path)],
            cwd=engine_root,
            check=False,
            capture_output=True,
            text=True,
        )
        if result.returncode != 0:
            detail = (result.stderr or result.stdout).strip()
            raise WikiError(f"render failed for {input_path}: {detail}")
        invocations.append(
            RenderInvocation(
                input_path=input_path,
                stdout=result.stdout.strip(),
                stderr=result.stderr.strip(),
            )
        )

    outputs = tuple(sorted(path for path in output_path.rglob("*") if generated_output_file(path, MANIFEST_FILENAME)))
    manifest = build_render_manifest(
        root=root,
        family=resolved_family,
        engine_root=engine_root,
        output_dir=output_path,
        invocations=tuple(invocations),
        outputs=outputs,
        include_talk=include_talk,
    )
    manifest_path = write_render_manifest(output_path / MANIFEST_FILENAME, manifest)
    return WikiRenderResult(
        family=resolved_family,
        output_dir=output_path,
        invocations=tuple(invocations),
        outputs=outputs,
        manifest_path=manifest_path,
        manifest=manifest,
    )


def family_render_inputs(family: WikiFamily, *, include_talk: bool) -> tuple[Path, ...]:
    inputs: list[Path] = []
    inputs.extend(source_inputs(family))
    if include_talk:
        inputs.extend(talk_inputs(family))
    return tuple(inputs)


def source_inputs(family: WikiFamily) -> tuple[Path, ...]:
    if family.source_primary:
        ensure_exists(family.source_primary, f"primary source for wiki family {family.id!r}")
        return (family.source_primary,)
    if not family.source_dir.is_dir():
        return ()
    return tuple(
        sorted(
            path
            for path in family.source_dir.rglob("*.md")
            if path.is_file() and not path.name.endswith(AUDIENCE_SOURCE_SUFFIXES)
        )
    )


def talk_inputs(family: WikiFamily) -> tuple[Path, ...]:
    if family.talk_primary:
        ensure_exists(family.talk_primary, f"primary talk folder for wiki family {family.id!r}")
        return (family.talk_primary,)
    if not family.talk_dir.is_dir():
        return ()
    return tuple(sorted(path for path in family.talk_dir.rglob("*.talk") if path.is_dir()))


def resolve_output_dir(root: Path, family: WikiFamily, output_dir: Path | str | None) -> Path:
    if output_dir is None:
        return family.generated_dir
    path = Path(output_dir).expanduser()
    if not path.is_absolute():
        path = root / path
    return path.resolve()


def ensure_exists(path: Path, label: str) -> None:
    if not path.exists():
        raise WikiError(f"missing {label}: {path}")


def generated_output_file(path: Path, manifest_filename: str) -> bool:
    if not path.is_file():
        return False
    if path.name == manifest_filename:
        return False
    if path.name.startswith("."):
        return False
    return True
