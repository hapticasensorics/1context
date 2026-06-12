# onecontext-context-engine

Rust home for the installed 1Context context engine.

This crate owns the native wiki-company orchestration runtime contract under:

```text
<1Context>/context-engine/
  orchestrators/wiki-company-orchestrator-v1/
  mail/
    threads/
  state/
    harness/
    codex-app-server/threads/
    scheduler/
    runs/
  artifacts/
    <run-id>/
      source-packets/
      turns/<operation-id>/attempt-0001/final-message.md
  tmp/
  agents/
    directory/
    policies/
  packs/wiki-company-v1/
    plugin.toml
    providers.toml
    native-memory.toml
    linking.toml
    harnesses/
    agents/
    jobs/
    prompts/
    lived-experiences/
```

The current slice keeps static company configuration in `packs/` and
`orchestrators/`, live coordination in `state/` and Agent Mail, and per-run
worker products in `artifacts/<run-id>/`. Rich long-term history still belongs
in Postgres/Timescale when available; `tmp/` is scratch only.

`packs/wiki-company-v1` defines the company. `orchestrators/wiki-company-orchestrator-v1`
defines how that company runs. The Rust crate interprets both.

Local proof:

```bash
cargo test -p onecontext-context-engine
cargo run -q -p onecontext-context-engine -- describe
cargo run -q -p onecontext-context-engine -- update-wiki \
  --root /tmp/1Context \
  --run-id demo \
  --trigger manual \
  --max-concurrent 5 \
  --json
```
