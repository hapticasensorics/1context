from __future__ import annotations

import base64
import hashlib
import json
import re
from dataclasses import dataclass
from pathlib import Path
from typing import Any


_MAX_TEXT_CHARS = 32000


@dataclass(frozen=True)
class ParsedSessionEvent:
    event: str
    session_id: str
    ts: str
    source: str
    kind: str
    text: str
    cwd: str
    payload: dict[str, Any]


def parse_row(adapter: str, raw: dict[str, Any], *, path: Path, state: dict[str, str]) -> ParsedSessionEvent | None:
    if adapter == "codex_rollout_jsonl":
        return parse_codex_row(raw, path=path, state=state)
    if adapter == "claude_code_jsonl":
        return parse_claude_row(raw, path=path, state=state)
    return None


def parse_claude_row(raw: dict[str, Any], *, path: Path, state: dict[str, str]) -> ParsedSessionEvent | None:
    if raw.get("type") == "progress":
        inner = (raw.get("data") or {}).get("message")
        if isinstance(inner, dict):
            wrapped = {
                **raw,
                **inner,
                "timestamp": inner.get("timestamp") or raw.get("timestamp"),
                "sessionId": inner.get("sessionId") or raw.get("sessionId"),
                "cwd": inner.get("cwd") or raw.get("cwd"),
            }
            return parse_claude_row(wrapped, path=path, state=state)
        return None

    ts = raw.get("timestamp")
    if not ts:
        return None

    msg = raw.get("message") if isinstance(raw.get("message"), dict) else {}
    role = msg.get("role") or raw.get("type")
    if role not in ("user", "assistant"):
        return None

    content = msg.get("content")
    text = _claude_assemble_text(content)
    if not text:
        return None

    session_id = str(raw.get("sessionId") or state.get("session_id") or default_session_id("claude_code_jsonl", path))
    cwd = str(raw.get("cwd") or state.get("cwd") or cwd_from_claude_path(path) or "")
    if session_id:
        state["session_id"] = session_id
    if cwd:
        state["cwd"] = cwd

    return ParsedSessionEvent(
        event="session.claude_code.imported",
        session_id=session_id,
        ts=str(ts),
        source="claude-code",
        kind=_claude_kind(content, str(role)),
        text=text,
        cwd=cwd,
        payload={
            "raw_type": raw.get("type", ""),
            "uuid": raw.get("uuid", ""),
            "parent_uuid": raw.get("parentUuid", ""),
            "is_sidechain": raw.get("isSidechain", False),
        },
    )


def parse_codex_row(raw: dict[str, Any], *, path: Path, state: dict[str, str]) -> ParsedSessionEvent | None:
    row_type = raw.get("type")
    payload = raw.get("payload") if isinstance(raw.get("payload"), dict) else {}

    if row_type == "session_meta":
        if payload.get("id"):
            state["session_id"] = str(payload["id"])
        if payload.get("cwd"):
            state["cwd"] = str(payload["cwd"])
        return None

    if row_type != "response_item":
        return None

    ts = raw.get("timestamp")
    if not ts:
        return None

    parsed = extract_codex_payload(payload, str(ts), state.get("cwd", ""))
    if not parsed:
        return None

    session_id = state.get("session_id") or default_session_id("codex_rollout_jsonl", path)
    return ParsedSessionEvent(
        event="session.codex.imported",
        session_id=session_id,
        ts=parsed["ts"],
        source=parsed["source"],
        kind=parsed["kind"],
        text=parsed["text"],
        cwd=parsed.get("cwd", ""),
        payload={
            "raw_type": row_type,
            "item_type": payload.get("type", ""),
            "call_id": payload.get("call_id", ""),
            "name": payload.get("name", ""),
        },
    )


def extract_codex_payload(payload: dict[str, Any], ts: str | None, cwd: str) -> dict[str, str] | None:
    if not ts:
        return None
    ptype = payload.get("type")

    if ptype == "message":
        role = payload.get("role")
        if role not in ("user", "assistant"):
            return None
        text = _codex_message_text(payload)
        if role == "user" and text.startswith((
            "# AGENTS.md instructions",
            "You are the agentic wiki writer",
        )):
            return None
        if role == "assistant" and text.startswith("---") and "slug:" in text[:200]:
            return None
        kind = role
    elif ptype in ("function_call", "custom_tool_call"):
        kind = "tool_use"
        text = _codex_fmt_function_call(payload)
    elif ptype in ("function_call_output", "custom_tool_call_output"):
        kind = "tool_result"
        text = _codex_fmt_function_output(payload)
    elif ptype == "web_search_call":
        kind = "tool_use"
        text = _codex_fmt_web_search(payload)
    else:
        return None

    if not text:
        return None
    return {"ts": ts, "kind": kind, "text": text, "source": "codex", "cwd": cwd}


def source_for_adapter(adapter: str) -> str:
    return {
        "codex_rollout_jsonl": "codex",
        "claude_code_jsonl": "claude-code",
    }.get(adapter, adapter)


def default_session_id(adapter: str, path: Path) -> str:
    stem = path.stem
    if adapter == "codex_rollout_jsonl":
        match = re.search(
            r"([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12})$",
            stem,
        )
        if match:
            return match.group(1)
    return stem


def clamp_text(text: str, limit: int = _MAX_TEXT_CHARS) -> str:
    return text[:limit]


def hash_event(session_id: str, ts: str, kind: str, text: str) -> str:
    digest = hashlib.sha256()
    digest.update(f"{session_id}|{ts}|{kind}|{text}".encode("utf-8"))
    return digest.hexdigest()[:16]


def sha256(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def file_sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def cwd_from_claude_path(path: Path) -> str:
    project = path.parent.name
    if not project.startswith("-"):
        return ""
    return "/" + project.strip("-").replace("-", "/")


def _preview(text: str, max_lines: int = 100, max_line_chars: int = 240) -> str:
    if not text:
        return ""
    lines = text.split("\n")
    capped = [
        (line if len(line) <= max_line_chars else line[:max_line_chars] + "…")
        for line in lines[:max_lines]
    ]
    out = "\n".join(capped)
    if len(lines) > max_lines:
        out += f"\n… [truncated at {max_lines} lines]"
    return out


def _cli_truncate(
    text: str,
    max_lines: int = 20,
    head: int = 3,
    tail: int = 3,
    max_line_chars: int = 500,
) -> str:
    if not text:
        return ""

    def clip_line(line: str) -> str:
        if len(line) <= max_line_chars:
            return line
        return line[:max_line_chars] + f"… [+{len(line) - max_line_chars} chars]"

    lines = [clip_line(line) for line in text.split("\n")]
    if len(lines) <= max_lines:
        return "\n".join(lines)
    hidden = len(lines) - head - tail
    return (
        "\n".join(lines[:head])
        + f"\n… +{hidden} lines (hidden; matches CLI ctrl+o collapse)\n"
        + "\n".join(lines[-tail:])
    )


_BASE64_IMAGE_RE = re.compile(r"data:image/(\w+);base64,([A-Za-z0-9+/=]+)")


def _replace_inline_images(text: str) -> str:
    if "data:image/" not in text:
        return text

    def replace(match: re.Match[str]) -> str:
        fmt = match.group(1)
        b64 = match.group(2)
        try:
            raw = base64.b64decode(b64, validate=False)
        except Exception:
            return match.group(0)
        digest = hashlib.sha256(raw).hexdigest()[:12]
        return f"[image: inline-data; fmt={fmt}; bytes={len(raw)}; sha256={digest}]"

    return _BASE64_IMAGE_RE.sub(replace, text)


_TRANSIENT_TOOLS = {
    "ToolSearch",
    "LSP",
    "Monitor",
    "PushNotification",
    "RemoteTrigger",
}


def _claude_assemble_text(content: Any) -> str:
    if isinstance(content, str):
        return content.strip()
    if not isinstance(content, list):
        return ""
    parts: list[str] = []
    for item in content:
        if not isinstance(item, dict):
            continue
        block_type = item.get("type")
        if block_type == "text":
            parts.append(item.get("text", ""))
        elif block_type == "tool_use":
            parts.append(_claude_fmt_tool_use(item))
        elif block_type == "tool_result":
            parts.append(_claude_fmt_tool_result(item))
    return "\n".join(part for part in parts if part).strip()


def _claude_kind(content: Any, role: str) -> str:
    if isinstance(content, list):
        types = {item.get("type") for item in content if isinstance(item, dict)}
        if "tool_result" in types:
            return "tool_result"
        if "tool_use" in types:
            return "tool_use"
    return role


def _claude_fmt_tool_use(item: dict[str, Any]) -> str:
    name = item.get("name", "?")
    if name in _TRANSIENT_TOOLS:
        return ""
    inp = item.get("input") or {}

    if name == "Bash":
        cmd = _cli_truncate(str(inp.get("command", "")))
        desc = str(inp.get("description") or "").strip()
        line = f"[Bash] {cmd}"
        return f'{line}\n  → "{desc}"' if desc else line
    if name == "Read":
        bits = [f"[Read {inp.get('file_path', '')}"]
        if inp.get("offset") is not None:
            bits.append(f"offset={inp['offset']}")
        if inp.get("limit") is not None:
            bits.append(f"limit={inp['limit']}")
        return " ".join(bits) + "]"
    if name == "Edit":
        old = _preview(str(inp.get("old_string", "")), max_lines=20, max_line_chars=160)
        new = _preview(str(inp.get("new_string", "")), max_lines=20, max_line_chars=160)
        return f"[Edit {inp.get('file_path', '')}]\n--- old\n{old}\n+++ new\n{new}"
    if name == "Write":
        body = _preview(str(inp.get("content", "")), max_lines=20, max_line_chars=160)
        return f"[Write {inp.get('file_path', '')}]\n{body}"
    if name == "NotebookEdit":
        body = _preview(str(inp.get("new_source", "")), max_lines=20, max_line_chars=160)
        return f"[NotebookEdit {inp.get('notebook_path', '')} cell={inp.get('cell_id', '')}]\n{body}"
    if name == "Glob":
        return f"[Glob pattern={inp.get('pattern','')!r} path={inp.get('path','')!r}]"
    if name == "Grep":
        bits = [f"[Grep pattern={inp.get('pattern','')!r}"]
        if inp.get("path"):
            bits.append(f"path={inp['path']!r}")
        if inp.get("glob"):
            bits.append(f"glob={inp['glob']!r}")
        if inp.get("output_mode"):
            bits.append(f"mode={inp['output_mode']}")
        return " ".join(bits) + "]"
    if name in ("Task", "Agent"):
        prompt = _preview(str(inp.get("prompt", "")), max_lines=20, max_line_chars=200)
        head = f"[Task:{inp.get('subagent_type', 'general-purpose')}] {str(inp.get('description') or '').strip()}"
        return f"{head}\n{prompt}" if prompt else head
    if name == "TodoWrite":
        todos = inp.get("todos") or []
        current = next((t.get("content", "") for t in todos if t.get("status") == "in_progress"), "")
        done = sum(1 for t in todos if t.get("status") == "completed")
        pending = sum(1 for t in todos if t.get("status") == "pending")
        return f"[TodoWrite {len(todos)} tasks, {done} done, {pending} pending] current: {current}"
    if name == "ExitPlanMode":
        return f"[ExitPlanMode]\n{_preview(str(inp.get('plan', '')), max_lines=100, max_line_chars=240)}"
    if name == "WebFetch":
        return f"[WebFetch {inp.get('url', '')}] {str(inp.get('prompt') or '')[:200]}"
    if name == "WebSearch":
        return f"[WebSearch] {str(inp.get('query') or '')[:200]}"
    if name == "ScheduleWakeup":
        return f"[ScheduleWakeup {inp.get('delaySeconds','?')}s] {str(inp.get('reason') or '')[:200]}"
    if name == "Skill":
        return f"[Skill {inp.get('skill','?')}] {str(inp.get('args') or '')[:200]}"
    if name == "SlashCommand":
        return f"[SlashCommand {inp.get('command','?')}] {str(inp.get('args') or '')[:200]}"
    if str(name).startswith("Task"):
        subject = str(inp.get("subject") or inp.get("description") or "")[:200]
        bits = [f"[{name}"]
        if inp.get("taskId"):
            bits.append(f"id={inp['taskId']}")
        if inp.get("status"):
            bits.append(f"status={inp['status']}")
        return " ".join(bits) + f"] {subject}"

    for key, value in inp.items():
        if isinstance(value, str) and value.strip():
            return f"[tool:{name}] {key}={value[:200]!r}"
    return f"[tool:{name}]"


def _claude_fmt_tool_result(item: dict[str, Any]) -> str:
    content = item.get("content", "")
    if isinstance(content, list):
        content = "\n".join(
            sub.get("text", "")
            for sub in content
            if isinstance(sub, dict) and sub.get("type") == "text"
        )
    if not isinstance(content, str):
        content = str(content)
    stripped = _replace_inline_images(content.strip())
    return f"[tool-result] {_cli_truncate(stripped)}" if stripped else ""


_CODEX_ENVELOPE_RE = re.compile(
    r"^(?:Command:.*?\n)?Chunk ID:.*?Process "
    r"(?:exited with code (-?\d+)|running with session ID \d+)"
    r".*?Output:\s*",
    re.DOTALL,
)


def _strip_codex_envelope(text: str) -> str:
    match = _CODEX_ENVELOPE_RE.match(text)
    if not match:
        return text
    exit_code = match.group(1)
    body = text[match.end():].rstrip()
    if exit_code is None:
        return body
    if not body:
        return f"[exit={exit_code}]" if exit_code != "0" else ""
    return f"[exit={exit_code}] {body}" if exit_code != "0" else body


def _codex_message_text(payload: dict[str, Any]) -> str:
    parts = []
    for item in payload.get("content") or []:
        if isinstance(item, dict) and item.get("type") in ("input_text", "output_text"):
            parts.append(item.get("text", ""))
    return "\n".join(part for part in parts if part).strip()


def _codex_call_args(payload: dict[str, Any]) -> dict[str, Any]:
    if "arguments" in payload:
        raw = payload.get("arguments") or ""
        try:
            return json.loads(raw) if raw else {}
        except Exception:
            return {"_raw": raw}
    if "input" in payload:
        inp = payload.get("input", "")
        return {"input": inp} if payload.get("name") == "apply_patch" else {"_raw": inp}
    return {}


def _codex_fmt_function_call(payload: dict[str, Any]) -> str:
    name = payload.get("name", "?")
    args = _codex_call_args(payload)
    if name in ("exec_command", "shell", "local_shell_call"):
        cmd = _cli_truncate(str(args.get("cmd") or args.get("command") or args.get("_raw") or ""))
        head = f"[exec_command] {cmd}"
        return f"{head}\n  workdir={args['workdir']}" if args.get("workdir") else head
    if name == "apply_patch":
        patch = str(args.get("input") or args.get("patch") or args.get("_raw") or "")
        files = re.findall(r"^\*\*\*\s+(?:Add|Update|Delete)\s+File:\s*(.+)$", patch, flags=re.MULTILINE)
        suffix = "…" if len(files) > 6 else ""
        return f"[apply_patch] files=[{', '.join(files[:6])}{suffix}]\n{_preview(patch, max_lines=60, max_line_chars=200)}"
    if name == "read_file":
        return f"[read_file {args.get('path') or args.get('file') or args.get('file_path', '')}]"
    if name == "write_file":
        body = _preview(str(args.get("contents") or args.get("content") or ""), max_lines=20, max_line_chars=160)
        return f"[write_file {args.get('path') or args.get('file_path', '')}]\n{body}"
    for key, value in args.items():
        if isinstance(value, str) and value.strip():
            return f"[{name}] {key}={value[:200]!r}"
    return f"[{name}]"


def _codex_fmt_web_search(payload: dict[str, Any]) -> str:
    action = payload.get("action") or {}
    query = str(action.get("query") or "").strip()
    queries = action.get("queries") or []
    extra = [q for q in queries if q != query]
    head = f"[web_search] {query[:240]}"
    if extra:
        head += "\n  also: " + " / ".join(str(q)[:80] for q in extra[:4])
    return head


def _codex_fmt_function_output(payload: dict[str, Any]) -> str:
    out = payload.get("output", "")
    if isinstance(out, str) and out.startswith("{") and '"output"' in out[:80]:
        try:
            parsed = json.loads(out)
            if isinstance(parsed, dict) and "output" in parsed:
                body = parsed.get("output") or ""
                exit_code = (parsed.get("metadata") or {}).get("exit_code")
                out = f"[exit={exit_code}] {body}" if exit_code is not None and exit_code != 0 else body
        except Exception:
            pass
    if isinstance(out, dict):
        out = out.get("output") or out.get("text") or str(out)
    if not isinstance(out, str):
        out = str(out)
    stripped = _replace_inline_images(_strip_codex_envelope(out.strip()))
    return f"[tool-result] {_cli_truncate(stripped)}" if stripped else ""
