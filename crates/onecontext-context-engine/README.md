# onecontext-context-engine

Rust home for the installed 1Context context engine.

This crate is the release replacement boundary for the old Python
`memory-core` wiki orchestration path. It owns the durable runtime contract
under:

```text
<1Context>/context-engine/
  orchestrators/wiki-company-orchestrator-v1/
  agents/
  mail/
    threads/
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

The first slice is deliberately deterministic: it records a wiki-company update
plan as a mail receipt that Swift can call without invoking Python. There is no
file-run hierarchy yet; execution history should move to Postgres/Timescale
when that design is ready. Subsequent slices should add Perception DB packet
planning, harness-born Codex app-server agent turns, Agent Mail coordination,
curator promotion, and wiki publication.

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
