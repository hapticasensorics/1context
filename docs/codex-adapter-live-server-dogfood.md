# Codex Adapter Live App-Server Dogfood

This dogfood runner starts a real local Codex app-server over stdio and records
deterministic, redacted evidence for the 1Context Codex adapter boundary.

Run:

```bash
node scripts/test-codex-adapter-live-server-dogfood.mjs
```

The runner creates:

```text
test-results/codex-adapter-live-server-dogfood-<timestamp>/
  app-server-transcript.jsonl
  app-server.stderr.log
  commands.jsonl
  generated-schemas/
  proof-summary.json
  requests/
  runtime/1Context/
```

## Proven By Default

- `onecontext-codex-adapter live-server-plan` emits the live phase plan and
  expected artifact paths.
- `codex app-server generate-json-schema --experimental --out <dir>` writes
  local schema evidence.
- Required methods are discovered in generated schema files:
  `initialize`, `thread/start`, `turn/start`, `thread/inject_items`,
  `turn/steer`, and `thread/loaded/list`.
- `codex app-server --listen stdio://` starts as a real child process.
- `initialize` returns a JSON-RPC result over newline-delimited stdio.
- `thread/start` and `thread/loaded/list` are attempted as non-model-turn live
  operations unless disabled.
- `thread/inject_items` appends redacted, synthetic dogfood context to the
  ephemeral live thread without starting a model turn.
- `turn/start` starts a live low-effort model turn and returns an in-progress
  `turn.id`.
- The runner waits for the matching `turn/started` notification before sending
  `turn/steer`, then steers that active turn with the same redacted dogfood
  prompt, proving the wake primitive needed for real mail flow.
- A parent harness unit is born through the harness CLI.
- A child harness unit is born through
  `onecontext-codex-adapter spawn-child --root <runtime> --request-json <json>`.
- Redacted proof events are recorded through
  `onecontext-codex-adapter record-proof` for transport identity, schema/method
  visibility, and any live thread operations that completed.

The durable summary intentionally stores hashes, counts, method names, relative
artifact paths, and statuses. It does not store raw model-turn prompt bodies.

## Optional Skip

Model-turn operations are now part of the default live proof lane because the
mail wake flow depends on real `turn/start` and `turn/steer` behavior.

For non-model debugging only, skip them explicitly:

```bash
ONECONTEXT_CODEX_ADAPTER_SKIP_LIVE_TURN=1 node scripts/test-codex-adapter-live-server-dogfood.mjs
```

or:

```bash
node scripts/test-codex-adapter-live-server-dogfood.mjs --skip-model-turn
```

`thread/inject_items` is included in the default non-model lane using a
synthetic context marker that is redacted from persisted summaries and command
logs.

## Useful Environment

- `ONECONTEXT_CODEX_BIN`: Codex CLI binary, default `codex`.
- `ONECONTEXT_CODEX_ADAPTER_BIN`: adapter CLI binary.
- `ONECONTEXT_AGENT_HARNESS_BIN`: harness CLI binary.
- `ONECONTEXT_CODEX_ADAPTER_LIVE_DOGFOOD_DIR`: fixed evidence directory.
- `ONECONTEXT_CODEX_ADAPTER_LIVE_MODEL`: model passed to `thread/start`;
  default is `gpt-5.4-mini`.
- `ONECONTEXT_CODEX_ADAPTER_LIVE_THREAD_OPS=0`: skip `thread/start` and
  `thread/loaded/list`.
- `ONECONTEXT_CODEX_ADAPTER_SKIP_LIVE_TURN=1`: skip model-turn phases.

## Verification

Syntax check:

```bash
node --check scripts/test-codex-adapter-live-server-dogfood.mjs
```

Full default dogfood:

```bash
node scripts/test-codex-adapter-live-server-dogfood.mjs
```

Inspect `proof-summary.json` for pass/skip/fail status and
`app-server-transcript.jsonl` for the exact newline-delimited JSON-RPC exchange.
