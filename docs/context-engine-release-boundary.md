# Context Engine Release Boundary

The release wiki company is owned by Rust Context Engine code.

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
continuity policy, the wiki-company orchestrator policy, Agent Mail, compact
runtime state, and bounded run artifacts. It is not a filesystem database for
rich run history in the current release slice.

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

The old Python prototype has been retired from the active tree. Release code
must not depend on a Python source checkout, `uv`, or `codex exec` as the
primary agent runtime. Historical procedure notes live in the orchestration
spec and git history.

## First Rust Slice

The initial `onecontext-context-engine update-wiki` command appends an audit
mirror under:

```text
context-engine/live/mail/threads/wiki-company.jsonl
```

That JSONL file is not the mail receipt of record. Coordination identity comes
from wiki-core Agent Mail `message_id` and `delivery_id` values; page talk files
are the user-readable projection of those messages.

The update plan records this release boundary and phase contract:

- import Perception events
- plan bounded scribe packets
- wake scribe agents
- run For You editor
- run biographer and librarian
- run page curators
- publish the wiki

The next work is to keep replacing planned phases with real Rust-owned
execution behind the Swift daemon's `context_engine.update_wiki` RPC.

Do not recreate the old unbounded Python run-history blob. The release shape is
a bounded, inspectable Rust run envelope:

```text
context-engine/live/runs/<run-id>/
```

That folder owns the execution's source packets, turn attempts, artifacts,
publish proof, run-local state, mail index, and receipt hydration proof. Failed
runs move as whole envelopes to
`archive/failed-runs/<archive-id>/runs/<run-id>`. Authoritative coordination
receipts remain Agent Mail first, with Postgres/Timescale available for rich
queryable execution history when needed.

Agent Mail registrations and leases live under `context-engine/live/agents/directory`.
Mail delivery, claims, idempotency, dead letters, injection receipts, and
notification attempts are separate append-only runtime ledgers. Harness state
under `state/harness` is not a substitute for those ledgers.

Generated talk files are readable projections into `user-wiki/source`; human
talk edits and accepted page bodies are durable wiki truth. Cleanup or archive
work must prove quiescence and hydrate every moved receipt, or preserve an
archive-manifest mapping from old path to hashed archived content.

The Swift process boundary is `ContextEngineProcessClient`. It launches the
bundled `onecontext-context-engine` binary without Python root discovery or
`uv run`.

## Cutover Rule

When a feature is needed for the installed wiki company, add it to Rust Context
Engine, `onecontext-agent-harness`, `onecontext-codex-adapter`,
`onecontext-wiki-core`, or the Swift daemon boundary.
