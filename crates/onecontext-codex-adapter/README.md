# onecontext-codex-adapter

Rust scaffold for the Codex-specific 1Context runtime adapter.

This crate owns the adapter seams described in
`docs/1context-codex-adapter-spec.md`:

- generated app-server schema/capability loading
- durable Codex thread to 1Context agent binding records
- mail notification wake strategies
- `wiki.mail.open` to `thread/inject_items` injection jobs
- hook intent records
- Codex event mirroring
- policy/proof helpers that produce redacted harness adapter events
- an in-process harness bridge that records proof plans and spawns governed
  child agent units through the harness store

It does not own mail truth, wiki truth, MCP tool behavior, or Codex native
session files. Those stay in the Rust core, MCP gateway, and Codex runtime.
Codex subagents should be represented as harness child units; the adapter may
use Codex-native subagent mechanics under the hood, but 1Context identity,
lineage, and lifecycle stay in the harness.

## Local Proof

The scaffold is pure Rust and does not start Codex:

```bash
cargo fmt -p onecontext-codex-adapter
cargo check -p onecontext-codex-adapter
cargo test -p onecontext-codex-adapter
```

When the shared workspace target directory is busy, use an isolated target dir
for adapter-only proof:

```bash
CARGO_TARGET_DIR=/tmp/onecontext-codex-adapter-target \
  cargo test -p onecontext-codex-adapter
```

The adapter CLI exposes the harness bridge proof surface:

```bash
cargo run -q -p onecontext-codex-adapter -- describe
cargo run -q -p onecontext-codex-adapter -- live-server-plan \
  --evidence-dir /tmp/1context-codex-live/evidence \
  --runtime-root /tmp/1context-codex-live/runtime/1Context
cargo run -q -p onecontext-codex-adapter -- spawn-child \
  --root /tmp/1Context \
  --request-json '<governed-child-agent-request-json>'
cargo run -q -p onecontext-codex-adapter -- record-proof \
  --root /tmp/1Context \
  --request-json '<harness-proof-record-plan-json>'
```

`live-server-plan` is a deterministic JSON contract for the live app-server
runner. It prints the Codex app-server command, schema generation command,
required methods, proof artifacts, and a `cli_contract_version` so scripts can
consume it without scraping text. It does not start Codex. Optional flags:

- `--codex-bin <path>` changes the executable used in generated commands.
- `--listen-url <url>` changes the app-server listen URL; default is
  `stdio://`.
- Model-consuming `turn/start` and `turn/steer` are enabled by default so the
  live proof can exercise the same wake primitive used by mail dispatch.
- `--skip-model-turns` disables that model-consuming lane for non-model
  debugging only; the runner still starts app-server, generates schema,
  initializes, proves `thread/start`, proves `thread/loaded/list`, and records
  redacted harness evidence.

The deterministic bridge dogfood drives the real harness CLI plus the adapter
CLI and writes redacted proof evidence:

```bash
node scripts/test-codex-adapter-harness-dogfood.mjs
```

The live mail flow dogfood proves the next boundary: real 1Context mail creates
a durable notification, `wiki.notify.dispatch` invokes a host command bridge,
the bridge calls live Codex `turn/steer`, and the opened body is delivered with
`thread/inject_items`:

```bash
node scripts/test-codex-adapter-live-mail-flow.mjs
```
