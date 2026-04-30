## What This Map Shows

This map explains how 1context chooses the memory format a running agent should actually use. The goal is not to invent a universal transcript yet. The goal is to let each harness use the memory shape it naturally understands.

## How to read it

- `native-memory.toml` names the supported memory surfaces.
- `providers.toml` says what raw provider routes read and write by default.
- When a harness is selected, its `primary_memory_format` wins over the provider default.
- Codex harness uses `codex-home`, which is an isolated `CODEX_HOME`.
- Claude Code uses `claude-project-jsonl`.
- OpenAI-compatible API routes can use `openai-chat-messages`.

## Why it matters

This keeps the system simple. Codex sessions remain Codex-native, Claude sessions remain Claude-native, and translation becomes an explicit future state machine instead of hidden background magic.
