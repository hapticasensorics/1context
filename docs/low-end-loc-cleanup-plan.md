# Low-End LOC Cleanup Plan

## Goal

Meet the low end of the repo-size targets:

| Surface | Target |
| --- | ---: |
| Core product/runtime source | ~50k LOC |
| Tests and binding contracts | ~15k LOC |
| `scripts/` | <3k LOC |
| Generated/demo/proof data | 0 LOC counted as core |
| Legacy/migration/scaffold/fallback systems | near zero |

This is only achievable by retiring whole surfaces. It is not achievable by
renaming files, splitting large files, or faithfully porting every dogfood
script into tests.

## Measurement Policy

Use one checked counting policy for baseline and final proof.

Count as core:

- shipped `macos/Sources/**` product/runtime code
- the one chosen wiki implementation
- minimal Rust/Swift contracts required by the shipped app
- release code required to build the shipped app

Do not count as core:

- tests
- generated evidence, screenshots, videos, review packets, and fixture dumps
- demos and sample packs
- dogfood/proof harnesses
- debug dashboards and research tools
- migration runners, compatibility fallbacks, scaffold contracts, and legacy
  aliases

The plan should fail if any deleted subsystem is still referenced by build
scripts, package manifests, CI, root npm scripts, active docs, or tests.

## Baseline

The agents used different counting methods, but they agree on the shape:

| Surface | Approx Current Size | Notes |
| --- | ---: | --- |
| Product-ish source | 120k-143k LOC | Depends on whether demos/devtools are counted. |
| Product Rust crates | ~52.5k LOC | Excluding Rust tests. |
| macOS sources | ~29.5k LOC | Before trimming Capture/Memory/Agent runtime surfaces. |
| Tests/contracts | 22k-70k LOC | Low number is code tests; high includes docs/contracts/proof surfaces. |
| `scripts/` | 14,300 LOC | 33 files. |
| release/script-like tooling | ~6.8k LOC | Excludes Caddy tarball. |
| tracked generated/proof data | 430k+ LOC | Mostly attention-capture mockup JSON. |
| ignored local output | 100GB+ | Mostly `target/`, `dist/`, and Peekaboo evidence. |

The biggest tracked repo-accounting issue is
`docs/assets/attention-capture-mockup/**`: about 154 MB, 356 tracked files,
and roughly 422k generated text lines.

## Product Nucleus

The low-end product nucleus is:

- signed macOS app
- setup/readiness UI
- daemon health/status
- local web/wiki opens
- CLI supports `version`, `diagnose`, and `uninstall`
- release train can build/install/prove the app
- exactly one current wiki renderer/runtime path

Everything else is dev/lab/sample unless explicitly re-admitted after the LOC
gate passes.

### Aggressive Core Option

One agent’s strict cut lands at about 40,898 LOC including selected tests:

| Bucket | Classification | LOC |
| --- | --- | ---: |
| macOS product sources, excluding Capture/MemoryRuntime/AgentRuntime | core | 18,504 |
| macOS core tests + package files | core | 6,214 |
| `onecontext-wiki-core` + `onecontext-wiki-daemon` | core | 16,180 |
| **Total** | **core** | **40,898** |

This option keeps the signed macOS shell plus portable local wiki runtime and
moves passive capture, Perception DB, agent harnessing, browser ingestion,
demos, and generated evidence out of product core.

### Balanced 50k Budget

If a small amount of capture/schema integration must remain, use this budget:

| Core Slice | Target LOC |
| --- | ---: |
| macOS app/runtime shell | 20k-22k |
| local wiki runtime/rendering | 7k-9k |
| capture core + app bridge | 5k-6k |
| current schema/read-write core | 7k-9k |
| shared contracts/schema crate | 2k-3k |
| browser/native host, only if hardened and shipped | <1k |
| thin integration stubs | 4k-5k |
| **Total** | **49k-52k** |

## Non-Core Cut Lines

Move out of product core:

| Area | Action | Expected Impact |
| --- | --- | ---: |
| `memory-core/**` | Archive/delete from active repo. | ~19.6k LOC out of active core. |
| `crates/onecontext-memory-db` | Retire/dev-scope; delete migrations. | ~14.8k Rust + SQL out of core. |
| `crates/onecontext-attention-*` | Move to `devtools/attention` or experiment repo. | ~8.9k Rust out of core. |
| `onecontext-capture-dashboard` + capture bundler | Devtools only; stop packaging by default. | ~5.3k immediate. |
| Rust capture core exporter/spool fallback machinery | Shrink to tiny contract crate if needed. | Up to ~10k. |
| agent harness + Codex adapter crates | Devtools/agent lab unless next-launch product. | ~7.8k-12k. |
| `macos/Sources/OneContextCapture` proof surfaces | Keep only launch-required capture bridge, if any. | ~1.5k-3k. |
| `macos/Sources/OneContextMemoryRuntime` | Delete/dev-scope process fallback adapters. | ~1k+. |
| `macos/Sources/OneContextAgentRuntime` | Delete/dev-scope unless productized. | ~500+. |
| `browser-extension/**` | Archive until hardened as shipped feature. | ~500 LOC. |
| demos | Move to sample packs or separate repo. | ~7.5k LOC. |
| `wiki-engine` | Choose: product renderer or sample/build input, not both with Rust wiki. | Avoid quiet 50k overrun. |

Critical decision: choose one wiki implementation for the low-end core.
Keeping Swift wiki runtime, Rust wiki core/daemon, and JS `wiki-engine` as
separate active implementations is the quiet 50k killer.

## Scripts Plan

Current `scripts/` is 14,300 LOC. The low-end target is under 3,000 LOC; the
better target is 1,100-1,600 LOC.

Keep temporarily:

- `scripts/release-train.sh`
- `scripts/build-macos-app.sh` until Swift/release-runner packaging replaces it
- `scripts/check-macos-release-credentials.sh`
- `scripts/prepare-macos-release-keychain.sh`
- exactly one live permission probe
- `scripts/test.sh`, reduced to a thin smoke wrapper

Delete or merge:

- all agent-mail/Codex/wiki dogfood `.mjs` scripts
- `onecontext-wiki-mcp-server.mjs` unless it becomes a real product binary
- capture audit and benchmark shell scripts
- memory DB dev/benchmark shell scripts
- generated Playwright shell wrappers
- runtime-default upgrade/backfill scenario scripts
- dashboard launch helpers

Dogfood/proof/scaffold scripts are disposable scaffolding. Move only durable
assertions into compact typed tests; do not port every behavior.

Add a size gate:

```bash
find scripts -type f -print0 | xargs -0 wc -l
```

The pass condition is under 3,000 total lines.

## Tests And Contracts Budget

The low-end target is about 15k LOC. This only works if deleted systems lose
their tests instead of moving dogfood scripts wholesale into test suites.

Target allocation:

| Surface | Target LOC |
| --- | ---: |
| Rust tests, including active inline units | ~5,500 |
| Swift tests | ~3,800 |
| JS/release/scripts tests | ~2,100 |
| Python bridge smoke, only if still shipped | 0-300 |
| Docs/schemas contracts | ~3,200 |
| **Total** | **14,600-14,900** |

Delete or rewrite first:

- `crates/onecontext-capture-core/tests/scaffold_contract.rs`
  - replace with `ready_bundle_contract.rs` around 350 LOC
- `crates/onecontext-memory-db/tests/migration_contract.rs`
  - replace with current schema/bootstrap contract; no dirty/repair/reapply tests
- `crates/onecontext-capture-core/tests/window_jsonl_export_fallbacks.rs`
  - delete undated `legacy.windows.jsonl` fallback
- macOS AgentHarness/Memory process fallback tests
  - keep only current structured error and socket/current protocol tests
- wiki-engine legacy `.talk` alias tests
  - assert canonical routes only
- dogfood MJS scripts
  - delete or reduce to one optional harness, not full relocation
- docs/contracts that preserve old migration/scaffold/fallback behavior
  - archive or rewrite as current contracts

## Generated, Demo, And Proof Data

Move out of active core accounting:

| Path | Action | Impact |
| --- | --- | ---: |
| `docs/assets/attention-capture-mockup/attention-debug-20260524-215739` | External sample pack; keep tiny synthetic fixture. | ~108 MB, ~422k generated text LOC. |
| `docs/assets/attention-capture-mockup/activity-sample-*.mov` and frames | External media sample pack. | ~45 MB. |
| `demos/agent-mail-triad/static/fixtures/latest.json` | Ignore/regenerate. | 2,781 LOC removed. |
| `docs/review-packets/**` | Ignore/generate on demand. | ~30k untracked LOC avoided. |
| `release/tools/caddy/**/*.tar.gz` | Fetch pinned artifact; track checksum/license. | 15 MB binary out of Git. |
| `demos/peekaboo-evidence-wall` | Archive/separate lab repo. | ~3.3k tracked LOC; 97 GB local ignored evidence can be purged. |
| runtime template/prompt packs | Move to sample/default packs if not required for bootstrap. | ~2k LOC. |

Add package smoke checks that fail if generated evidence, source-checkout
artifacts, or demo fixtures appear in the shipped app.

## Release And Packaging Plan

Release/packaging currently has about 6.8k tracked LOC excluding the Caddy
tarball. Target 1.8k-2.3k LOC of shell/YAML/Node-facing release code.

Order:

1. Move manifest/evidence/redaction into a typed Swift release tool.
2. Move DMG, notarization, appcast, codesign, and Sparkle helpers into typed
   release code.
3. Replace `scripts/build-macos-app.sh` with Swift packaging.
4. Move runtime-defaults manifest generation out of Python/shell.
5. Collapse self-hosted proof orchestration last.
6. Delete the Node release runner only after Swift parity.
7. Thin workflows to checkout, credentials, command invocation, and artifacts.

Keep `scripts/release-train.sh` as the operator command, but make it a tiny shim.

## Legacy And Migration Policy

No compatibility shims, migration runners, old route aliases, scaffold receipts,
fallback parsers, or source-checkout packaging.

Every subsystem deletion must remove its:

- build references
- package references
- active docs
- root npm scripts
- CI hooks
- tests

Do not call capture, memory, adapter, or browser-extension code non-core while
the app bundle, CLI, release audit, or permission probe still depends on it.

## Execution Roadmap

### Milestone 0: Freeze The Launch Contract

Name the launchable product core: signed macOS app, setup/readiness, local
web/wiki, release train, and only Rust crates required by that path.

Gate:

- current smoke tests are green before deletion begins
- stable dev build installs and `1context-cli diagnose` passes

### Milestone 1: Add Accounting Gates

Add repeatable counters for:

- core source
- tests/contracts
- scripts
- generated/demo/proof data
- legacy/migration/scaffold references

Gate:

- same command is used for baseline and final proof
- generated/proof data is reported separately and not counted as core

### Milestone 2: Crush Scripts

Delete dogfood/proof/scaffold scripts first. Keep only shims and one live probe.

Gate:

- `scripts/` under 3k LOC
- no CI/build/docs references retired script names

### Milestone 3: Move Generated/Demo/Proof Data

Externalize attention mockups, review packets, stale demo fixtures, demos, and
large local evidence.

Gate:

- package audit proves no generated/demo/source-checkout artifacts ship

### Milestone 4: Retire Non-Launch Surfaces

Move or delete memory-core, memory-db, attention, capture dashboards, agent
harness, Codex adapter, browser extension, demos, and unchosen wiki
implementation.

Gate:

- stable dev build still launches
- `1context-cli diagnose` still passes
- no build/package references to retired surfaces

### Milestone 5: Delete Legacy Compatibility Paths

Remove migration runners, capture fallback exports, scaffold receipts, legacy
wiki route aliases, process/socket fallback ambiguity, and tests/docs pinning
those behaviors.

Gate:

- negative legacy scan is clean
- current schema/READY/current-route tests pass

### Milestone 6: Parser And Schema Consolidation

Only after the big cuts, replace remaining hand-rolled parsing where it reduces
net code and risk. Avoid adding parser-library adapter layers that increase LOC.

Gate:

- LOC does not rise outside the target budget
- no new compatibility layer is introduced

### Milestone 7: Final Low-End Proof

Acceptance:

- core source around 50k LOC
- tests/contracts around 15k LOC
- `scripts/` under 3k LOC
- generated/demo/proof data outside core
- stable dev app build/install/launch/diagnose works
- one live permission probe only when permissions are in scope

## Recycle-Bin Policy

Use `recycle-bin/<YYYYMMDD>/...` for potentially useful non-launch material.
Preserve original paths inside the recycle bin and log moves in `docs/retired.md`
with original path, recycle path, reason, and date.

Hard-delete only ignored/generated/vendor/build artifacts that are reproducible.
Rollback is restoring from recycle-bin plus reverting the ledger entry, not
recreating compatibility scaffolds.

## Agent Run Summary

This plan synthesizes 10 high-reasoning agents:

- LOC budget architecture
- Rust crate trimming
- macOS trimming
- scripts/harness replacement
- tests/contracts budget
- generated data/demos/runtime
- product-core definition
- release/packaging simplification
- sequencing/risk planning
- final plan critique

All agents were read-only. They did not edit files.
