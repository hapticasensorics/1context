# Context Engine Release Boundary

The release wiki company should be owned by Rust Context Engine code, not the
Python `memory-core` prototype.

## Names

`context-engine/` is the user-owned data folder under `~/1Context`.

`onecontext-context-engine` is the Rust binary/library that owns orchestration
inside that folder.

The duplicate name is intentional: the binary is the native owner of the data
tree.

## Runtime Root

The installed app uses:

```text
~/1Context/
  user-wiki/
  context-engine/
```

The dev app uses:

```text
~/1Context-Dev/
  user-wiki/
  context-engine/
```

`user-wiki/` is wiki truth: source pages, talk pages, templates, assets, and
rendered site output.

`context-engine/` is agent society truth: the shipped wiki-company pack,
persistent agent identities, harness receipts, lived experience, native-memory
continuity policy, the wiki-company orchestrator policy, and Agent Mail. It is
not a filesystem database for run history in the current release slice.

## Release Direction

The release path is:

```text
Swift daemon / Refresh Wiki
  -> onecontext-context-engine
  -> wiki-company-orchestrator-v1 policy
  -> wiki-company-v1 pack
  -> Perception DB packet planning
  -> onecontext-agent-harness
  -> onecontext-codex-adapter / Codex app-server
  -> Agent Mail + wiki talk receipts
  -> onecontext-wiki-core page writes
  -> wiki publish + validate
```

`memory-core` may remain in the repository temporarily as a prototype/reference
source, but it is not the release orchestrator. Release code should not depend
on a Python source checkout, `uv`, or `codex exec` as the primary agent runtime.

## First Rust Slice

The initial `onecontext-context-engine update-wiki` command appends a durable
wiki-company receipt under:

```text
context-engine/mail/threads/wiki-company.jsonl
```

That mail receipt records the release boundary and phase contract:

- import Perception events
- plan bounded scribe packets
- wake scribe agents
- run For You editor
- run biographer and librarian
- run page curators
- publish the wiki

This is intentionally a small start. The next work is to replace each planned
phase with real Rust-owned execution behind the Swift daemon's
`context_engine.update_wiki` RPC.

Do not add a `runs/` folder or top-level proposal/decision/artifact warehouses
as a placeholder. If execution history needs richer durability, put the real
design in Postgres/Timescale and keep mail as the human-readable audit trail.

The Swift process boundary is `ContextEngineProcessClient`. It launches the
bundled `onecontext-context-engine` binary and preserves the old JSON
process-runner pattern without Python root discovery or `uv run`.

## Cutover Rule

Do not polish Python `memory-core` into the release orchestrator. When a feature
is needed for the installed wiki company, add it to Rust Context Engine,
`onecontext-agent-harness`, `onecontext-codex-adapter`, `onecontext-wiki-core`,
or the Swift daemon boundary.
