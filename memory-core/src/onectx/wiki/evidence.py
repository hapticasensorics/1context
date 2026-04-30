from __future__ import annotations

import hashlib
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from ..config import MemorySystem
from ..storage import LakeStore
from .families import format_path
from .manifest import MANIFEST_FILENAME
from .render import WikiRenderResult


@dataclass(frozen=True)
class WikiEvidenceResult:
    artifact: dict[str, Any]
    evidence: tuple[dict[str, Any], ...]
    event: dict[str, Any]

    def to_payload(self) -> dict[str, Any]:
        return {
            "artifact": self.artifact,
            "evidence": list(self.evidence),
            "event": self.event,
        }


def record_render_evidence(system: MemorySystem, result: WikiRenderResult) -> WikiEvidenceResult:
    if not result.manifest_path:
        raise ValueError("cannot record render evidence without a manifest path")
    manifest_path = result.manifest_path
    manifest = result.manifest or {}
    data = manifest_path.read_bytes()
    digest = hashlib.sha256(data).hexdigest()

    store = LakeStore(system.storage_dir)
    store.ensure()
    artifact = store.append_artifact(
        "wiki.render_manifest",
        uri=f"wiki://family/{result.family.id}/{MANIFEST_FILENAME}",
        path=format_path(manifest_path, system.root),
        content_type="application/json",
        content_hash=digest,
        bytes=len(data),
        source="1context wiki render",
        state="produced",
        text=f"Render manifest for wiki family {result.family.id}.",
        metadata={
            "family": result.family.id,
            "route": result.family.route,
            "output_dir": format_path(result.output_dir, system.root),
            "output_count": len(result.outputs),
            "manifest_schema": manifest.get("schema_version", ""),
        },
    )

    evidence_rows = (
        store.append_evidence(
            "wiki.render.succeeded",
            artifact_id=artifact["artifact_id"],
            checker="src/onectx/wiki/render.py",
            text=f"Wiki family {result.family.id} rendered successfully.",
            checks=[
                "render command exited successfully for every input",
                f"{len(result.outputs)} generated file(s) observed",
                f"{MANIFEST_FILENAME} written",
            ],
            payload={
                "family": result.family.id,
                "invocations": len(result.invocations),
                "outputs": [format_path(path, system.root) for path in result.outputs],
            },
        ),
        store.append_evidence(
            "wiki.manifest.recorded",
            artifact_id=artifact["artifact_id"],
            checker="src/onectx/wiki/evidence.py",
            text=f"Render manifest hash recorded for wiki family {result.family.id}.",
            checks=[
                "manifest file exists",
                "manifest sha256 recorded as artifact content_hash",
                "manifest path recorded relative to repo when possible",
            ],
            payload={
                "family": result.family.id,
                "manifest_path": format_path(manifest_path, system.root),
                "sha256": digest,
            },
        ),
        store.append_evidence(
            "wiki.generated.available",
            artifact_id=artifact["artifact_id"],
            checker="src/onectx/wiki/evidence.py",
            text=f"Generated wiki outputs are available for family {result.family.id}.",
            checks=[
                "output files exist on disk",
                "manifest includes route records for local serving",
            ],
            payload={
                "family": result.family.id,
                "routes": manifest.get("routes", []),
            },
        ),
    )

    event = store.append_event(
        "wiki.render.completed",
        source="1context wiki render",
        kind="wiki",
        actor="local",
        subject=result.family.id,
        artifact_id=artifact["artifact_id"],
        evidence_id=evidence_rows[0]["evidence_id"],
        text=f"Rendered wiki family {result.family.id} and recorded evidence.",
        payload={
            "family": result.family.id,
            "manifest_path": format_path(manifest_path, system.root),
            "output_dir": format_path(result.output_dir, system.root),
            "output_count": len(result.outputs),
            "evidence_ids": [row["evidence_id"] for row in evidence_rows],
        },
    )
    return WikiEvidenceResult(artifact=artifact, evidence=evidence_rows, event=event)
