from __future__ import annotations

import hashlib
from dataclasses import dataclass
from pathlib import Path
from typing import Any


@dataclass(frozen=True)
class PromptPart:
    name: str
    text: str
    path: Path | None = None

    @property
    def sha256(self) -> str:
        return text_sha256(self.text)

    @property
    def bytes(self) -> int:
        return len(self.text.encode("utf-8"))

    @property
    def estimated_tokens(self) -> int:
        return estimate_token_count(self.text)

    def to_payload(self) -> dict[str, Any]:
        return {
            "name": self.name,
            "path": str(self.path) if self.path else None,
            "sha256": self.sha256,
            "bytes": self.bytes,
            "estimated_tokens": self.estimated_tokens,
        }


@dataclass(frozen=True)
class PromptStack:
    parts: tuple[PromptPart, ...]

    @property
    def text(self) -> str:
        return "\n\n".join(part.text.rstrip() for part in self.parts).rstrip() + "\n"

    @property
    def sha256(self) -> str:
        return text_sha256(self.text)

    @property
    def bytes(self) -> int:
        return len(self.text.encode("utf-8"))

    @property
    def estimated_tokens(self) -> int:
        return estimate_token_count(self.text)

    def to_payload(self) -> dict[str, Any]:
        return {
            "kind": "prompt_stack",
            "sha256": self.sha256,
            "bytes": self.bytes,
            "estimated_tokens": self.estimated_tokens,
            "parts": [part.to_payload() for part in self.parts],
        }


def prompt_part_from_file(name: str, path: Path, *, format_values: dict[str, Any] | None = None) -> PromptPart:
    text = path.read_text(encoding="utf-8")
    if format_values:
        text = text.format(**format_values)
    return PromptPart(name=name, text=text, path=path)


def text_sha256(text: str) -> str:
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


def estimate_token_count(text: str) -> int:
    """Fast conservative-ish token estimate for routing decisions.

    This is not model-tokenizer exact; it is intentionally cheap for hot-path
    scheduling. Claude/Codex prompts in this system are mostly markdown/log
    text, where four characters per token is a useful first approximation.
    """
    if not text:
        return 0
    return max(1, (len(text) + 3) // 4)
