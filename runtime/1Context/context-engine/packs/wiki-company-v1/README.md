# Wiki Company V1

This is the shipped default company for 1Context wiki generation.

It is a definition pack, not runtime state. The Context Engine reads this pack,
combines it with the current wiki, Perception history, agent identity, and mail
context, then wakes harness-born Codex agents.

## Folder Map

```text
prompts/          the words the agents read
agents/           role identities and defaults
jobs/             concrete tasks to run
harnesses/        how those jobs are executed
lived-experiences/ seed continuity for agent identities
linking.toml       how continuity is attached to a job turn
native-memory.toml native session/history formats the company understands
providers.toml    model/provider/account options
plugin.toml       pack metadata
```

## Release Harness

The active harness is:

```text
harnesses/codex-app-server.toml
```

That harness describes the `onecontext-agent-harness` +
`onecontext-codex-adapter` path. It does not run raw `codex exec` scripts.
`provider = "codex"` means the Codex app / ChatGPT Pro transport. Individual
agents still choose their model, reasoning effort, context budget, prompt paths,
and memory policy.

`native-memory.toml` may describe historical or importable native formats such
as Codex home directories, Claude project JSONL, or OpenAI-style message arrays.
Those formats are source/continuity surfaces. They do not make those providers
active release harnesses.

## Prompt Policy

The prompts were ported from the old `memory-core` `base-memory-v1` donor pack
and intentionally preserved as much as possible. Some examples inside prompt
text may still mention older model names because they are examples of talk-page
authorship or historical prompt language, not active release harness selection.

Do not scatter prompt copies into top-level runtime folders. Edit or fork this
pack when changing the default company.
