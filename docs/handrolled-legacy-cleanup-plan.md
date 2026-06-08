# Hand-Rolled And Legacy Cleanup Plan

## Intent

This cleanup is deletion-first.

The project direction is to remove old implementation paths instead of carrying
migration systems, backwards-compatibility scaffolds, compatibility tables,
fallbacks, legacy upgrade paths, or scaffold implementations. If a stale system
is referenced only by tests, scripts, docs, or build wiring, the preferred fix is
to update or remove those references. Do not preserve old behavior merely
because it has a test.

The wording in this document is intentionally strong. Agents should treat
`legacy`, `fallback`, `scaffold`, `migration`, and `compatibility` as deletion
triggers, not as hints to build a nicer compatibility layer.

## Headline Targets

| Target | Current Size | Target Size | Primary Move |
| --- | ---: | ---: | --- |
| `scripts/` | 14,300 LOC | <= 2,860 LOC | Delete/merge dogfood scripts into one harness or typed tests. |
| Hand-rolled parsing | scattered | -1,800 to -2,700 LOC | Use typed schemas and real parsers. |
| Migration/backcompat tests | scattered | -400 to -800 LOC | Delete tests that only pin stale behavior. |
| Debug/demo surfaces | 10k+ LOC plus fixtures | separate from app core | Move to devtools, archive, or generated ignored output. |

## Scripts 5x Reduction

The three script audits converged: `scripts/` can be cut from 14,300 LOC to
roughly 2,000-2,800 LOC without losing meaningful verification.

The strictest target is to keep only `scripts/release-train.sh` as the stable
operator entrypoint and move every other behavior into typed Rust/Swift tests,
checked-in Playwright specs, or a single devtools/proof harness.

### P0: Delete Or Merge The MJS Dogfood Cluster

These files are the main bloat center: about 8,489 LOC. They repeat command
runners, JSON parsing, evidence writing, redaction, request-file logging,
wiki wrappers, harness wrappers, and JSON-RPC/MCP plumbing.

| Paths | Action | Replacement |
| --- | --- | --- |
| `scripts/generate-agent-mail-triad-demo.mjs` | Merge/delete | Harness scenario plus static fixture writer. |
| `scripts/test-agent-harness-boundary-dogfood.mjs` | Delete | Rust agent-harness tests plus one smoke scenario. |
| `scripts/test-agent-mail-dogfood.mjs` | Delete | Rust `agent_mail` contract tests. |
| `scripts/test-codex-adapter-live-server-dogfood.mjs` | Merge | Optional live-server harness scenario. |
| `scripts/test-codex-adapter-live-mail-flow.mjs` | Delete | Adapter wake/injection/proof tests. |
| `scripts/test-codex-adapter-harness-dogfood.mjs` | Delete | Rust `harness_bridge`, `cli`, and daemon tests. |
| `scripts/test-wiki-core-dogfood.mjs` | Merge | Swift wiki runtime tests and Playwright route spec. |
| `scripts/verify-agent-mail-triad-mcp-realism.mjs` | Delete | Assertions inside the harness scenario. |
| `scripts/onecontext-wiki-mcp-server.mjs` | Merge/delete | Productize as Rust/Swift if it is real; otherwise harness module only. |

Acceptable replacements:

- A single `scripts/verify.mjs` of roughly 600-800 LOC with shared command
  execution, temp runtime setup, JSON asserts, redaction, evidence output, and
  scenario loading.
- Better: a Rust `onecontext-devtools` CLI with `assert_cmd`, `tempfile`,
  typed JSON structs, and integration tests.

### P1: Move Shell Tests Into Real Test Suites

| Paths | Action | Replacement |
| --- | --- | --- |
| Retired release train shell test | Done | `release/runner` tests own manifest, appcast, dry-run, and workflow assertions. |
| Retired packaged launch agent shell smoke | Done | Release runner tests and `LaunchAgentManagerTests` own the current checks. |
| `scripts/test-wiki.sh` | Shrink/keep | Compact current-contract render/manifest smoke; route coverage lives in wiki-engine tests. |
| Retired RuntimeDefaults scenario wrapper | Done | Swift runtime-default tests; no generated shell Playwright. |
| Retired memory local-web shell harness | Done | Local-web contracts live in typed macOS tests. |

### P2: Collapse DB, Capture, Benchmark, And Permission Scripts

| Paths | Action | Replacement |
| --- | --- | --- |
| Retired Docker memory DB dev helper | Done | App-managed Postgres runtime and Rust memory DB tests own current schema. |
| Retired memory DB benchmark shell harnesses | Done | Rust memory DB tests own current schema and writer contracts. |
| Retired capture/attention benchmark and proof shell harnesses | Done | Rust capture/attention tests own current bundle and dashboard contracts. |
| Non-live installed-app permission package checkers | Delete | Permission proof should be covered by release package smoke plus Swift tests. |
| `scripts/test-installed-app-live-permission-capabilities.sh` | Keep/merge | One timestamped dev live TCC probe entrypoint only. |

### P3: Thin Operator Shims Only

Keep or reduce these to small shims:

- `scripts/release-train.sh`
- `scripts/test.sh`
- `scripts/check-macos-release-credentials.sh`
- `scripts/prepare-macos-release-keychain.sh`
- `scripts/install-browser-extension-dev.sh` only while browser proof remains explicitly dev-supported

The size gate should become explicit:

```bash
find scripts -type f -print0 | xargs -0 wc -l
```

The pass condition is `scripts/` under 2,860 total lines.

## Schema And Parsing Direction

The codebase has many parallel parsing systems. They should collapse into a
schema-first approach.

### Canonical Replacements

| Current Pattern | Replacement |
| --- | --- |
| `serde_json::Value` helper ladders | Typed `serde` structs plus `serde_path_to_error`. |
| Manual CLI parsing in Rust | `clap` derive and typed `ValueEnum`s. |
| Manual CLI parsing in Swift | `swift-argument-parser` subcommands. |
| Manual RFC3339 parsing | `chrono`/`jiff` in Rust, one shared `ISO8601DateFormatter` in Swift, `Date` parsing in JS. |
| Manual TOML mutation | `toml_edit` or typed `serde` TOML. |
| Markdown link regexes | `pulldown-cmark` in Rust, `remark`/`unified` in JS. |
| HTML attr/string mutation | `scraper`/`html5ever`/`lol_html` in Rust, `parse5`/`cheerio`/`linkedom` in JS. |
| Release XML regexes | Existing `fast-xml-parser` plus `zod`. |
| Duplicated process runners in Swift | One shared async `ProcessRunner`. |

Add a small `onecontext-contracts` or `onecontext-schema` crate that owns:

- capture event envelopes
- memory query requests and responses
- agent-harness request/receipt types
- appcast/proof summaries
- shared timestamp helpers

Raw JSON should remain only at explicit ingestion or forensic retention
boundaries. Everything after ingress should be typed.

## Deletion Targets By Subsystem

### Memory DB

Delete the migration subsystem rather than repairing it.

| Paths | Action | Notes |
| --- | --- | --- |
| `crates/onecontext-memory-db/src/migrations.rs` | Delete | Remove migration runner, repair SQL, dirty flags, checksum backfill, readiness APIs. |
| `crates/onecontext-memory-db/migrations/*.sql` | Delete or replace | Replace with one current `schema/perception.sql` bootstrap for empty dev/test DBs. |
| `crates/onecontext-memory-db/tests/migration_contract.rs` | Rewrite | Keep current schema assertions; delete dirty/repair/migrate-twice/live reapply tests. |
| `crates/onecontext-memory-db/src/db.rs` | Simplify | Accept only explicit `--database-url` and `ONECONTEXT_MEMORY_DB_URL`. |
| `crates/onecontext-memory-db/src/query_density.rs` | Simplify | Delete raw-table fallback; make density one schema-backed path or fail. |
| `codex_agent_ingest.rs`, `claude_agent_ingest.rs` | Tighten | Replace broad alias/fallback extraction with explicit source-format versions. |

Required wording in implementation briefs:

> Do not add a new migration runner. Do not preserve old schema upgrade paths.
> If tests reference the deleted runner, update or delete those tests.

### Wiki Core And Wiki Daemon

| Paths | Action | Notes |
| --- | --- | --- |
| `crates/onecontext-wiki-daemon/src/main.rs` | Replace parser | Use `clap`; delete command alias compatibility and string-matched error mapping. |
| `crates/onecontext-wiki-core/src/lib.rs` TOML helpers | Replace/delete | Use typed config or `toml_edit`; delete `toml_quote`, string array edits, enabled toggles. |
| frontmatter/template helpers | Replace/delete | Use YAML/frontmatter and template libraries; require templates instead of fallback templates. |
| Markdown/link extraction | Replace | Use Markdown parser events, not substring scanning. |
| HTML link diagnostics | Replace | Use DOM parser or renderer instrumentation. |
| tombstone/restore lifecycle | Decide and delete | Prefer one deletion model; do not carry tombstone plus restore plus repair lifecycle. |
| `agent_mail.rs` address parsing | Replace/tighten | Use URI parser; delete alternate address syntaxes like `page://` if noncanonical. |

### Capture And Attention

| Paths | Action | Notes |
| --- | --- | --- |
| capture bundle exporter inferred lanes | Delete | Do not manufacture AX/display/browser/editor/terminal lanes from old windows data. Missing lanes should degrade or fail. |
| `primary_time_from_json_prefix` copies | Replace | One typed envelope time probe via `serde_json` or `simd-json`. |
| `crates/onecontext-capture-core/src/fixtures.rs` | Move | Test/demo fixtures should not be public product API. |
| debug video media export | Move/delete | Debug recordings should not be normal READY bundle media. |
| `onecontext-capture-dashboard` | Retire or dev-scope | About 6k LOC; not app core if it is a diagnostic dashboard. |
| attention compatibility report/migration notes | Delete | READY bundle validation is the contract. |
| attention no-media fallback | Delete | Missing `frame_2fps` should be a typed input error. |

### macOS Swift

| Paths | Action | Notes |
| --- | --- | --- |
| `macos/Sources/OneContextCLI/main.swift` | Replace | Use `swift-argument-parser` and typed `Codable` requests. |
| process clients across Wiki/Agent/Memory/Supervisor | Consolidate | One shared `ProcessRunner` with timeout and bounded stdout/stderr. |
| memory protocol process fallback | Delete if socket is current | Do not silently shell out when current memoryd protocol is unavailable. |
| local-web render metadata fallback | Delete | Use only `.1context/current-render.json`; remove `publish-manifest.json` fallback. |
| `WikiInventory` TOML parsing | Replace | Use TOML library or delegate to Rust wiki core. |
| permission-test identity | Isolate | Keep timestamped TCC harness, but do not let it leak into normal install behavior. |
| LaunchAgent plist strings | Replace | Use `PropertyListEncoder` or `PropertyListSerialization`. |
| Caddy process `ps` matching | Replace | Track owned process/state or use process APIs. |

### Wiki Engine

| Paths | Action | Notes |
| --- | --- | --- |
| `frontmatter.mjs`, `sections.mjs`, `render-site.mjs`, `enhance.js` | Replace duplicated parsers | Use `gray-matter`, `js-yaml`, TOML parser, and AJV. |
| Markdown/link/citation regexes | Replace | Use `remark-parse`, `remark-gfm`, `unist-util-visit`, `github-slugger`, rehype/HAST. |
| HTML attr extraction/annotation | Replace | Use `parse5`, `cheerio`, or `linkedom`. |
| schemas under `wiki-engine/schemas` | Enforce | Add AJV validation for emitted render/index manifests. |
| `theme/js/enhance.js`, `theme/css/theme.css` | Split/delete | Delete unused AI panel styles, planned agent surfaces, and client-side legacy talk parser. |
| legacy `.talk` URLs | Delete | Tests should assert canonical routes, not compatibility aliases. |
| generated talk stubs | Delete or require explicit empty state | Avoid default content that hides missing discussion data. |

### Release, Demos, Runtime, Browser Extension

| Area | Action | Notes |
| --- | --- | --- |
| release GUI proof shell | Replace | Move shell/AppleScript greps into Swift/XCTest or machine-readable CLI proof runner. |
| release parser shims | Replace | Use structured YAML/XML/security output instead of regex prose checks. |
| synthetic fixture proof generation | Delete | Do not write `passed` proof JSON from manifest expectations. |
| vendored Caddy tarball | Move | Use checksum-pinned download/cache; keep SHA/license only. |
| root package demo scripts | Move | Root scripts should be repo-wide verification, not demo dogfood. |
| `demos/agent-mail-triad/static/fixtures/latest.json` | Delete | Generated demo snapshot should be ignored output. |
| `demos/peekaboo-evidence-wall` | Archive or separate repo | Experimental lab, not app core. Local ignored `.evidence` was reported as 97 GB. |
| browser extension | Dev-scope or harden | Broad permissions and mtime app discovery should not be app core without hardening. |
| runtime defaults | Reduce | Generate app-machine dirs at first run; move bulky prompt/template packs to sample packs. |
| self-hosted workflows | Deduplicate | Reusable workflow/composite action for shared guards/artifact shape. |

## Tests And Docs That Pin Stale Behavior

These should be changed before or alongside code deletion. If they fail after a
deletion, that is not automatically a regression.

| Path | Stale Behavior | Action |
| --- | --- | --- |
| `crates/onecontext-memory-db/tests/migration_contract.rs` | Migration runner, dirty flags, repair SQL, migrate-twice behavior. | Convert to current schema contract. |
| `docs/memory-db-design-spec.md`, `docs/memory-source-connectors-spec.md` | Allows old `capture.*` migrations and `CaptureEnvelope` naming. | Rewrite to reset transitional DBs; no active legacy capture naming. |
| `crates/onecontext-capture-core/tests/window_jsonl_export_fallbacks.rs` | Undated `legacy.windows.jsonl` fallback. | Delete or assert rejection. |
| capture/attention docs referencing scaffold/migration | Active docs describe placeholder/scaffold behavior. | Rewrite as READY bundle contract. |
| `scripts/test-agent-harness-boundary-dogfood.mjs` | Accepts `status: "scaffold"`. | Product proof must fail scaffold receipts. |
| `docs/agent-harness-implementation-scaffold.md` | Keeps scaffold contract alive. | Archive or rewrite as current contract. |
| `wiki-engine/src/renderer/index.test.mjs` | Requires legacy `.talk` route aliases. | Replace with canonical route tests. |
| the retired RuntimeDefaults scenario wrapper | Preserves upgrade/backfill scenario. | Archive one-time evidence; keep fresh install/idempotency only. |
| `docs/attention-bundle-migration-notes.md`, `bundle_input.rs` | No-frame compatibility report. | Missing frame media should be invalid input. |
| `docs/codex-adapter-harness-dogfood.md` | Still says scaffold behavior. | Rewrite as current CLI contract. |

## Execution Order

1. **Freeze the policy in docs and prompts.**
   Add this deletion-first language to cleanup tickets and agent prompts.

2. **Cut scripts first.**
   Build one harness or devtools CLI, move Playwright checks into specs, then
   retire the old dogfood scripts. Enforce the 2,860-line size gate.

3. **Delete memory-db migration scaffolding.**
   Replace the runner with one current schema bootstrap/check. Update tests and
   docs at the same time.

4. **Retire or dev-scope debug dashboards and generated demo fixtures.**
   The capture dashboard, stale demo fixtures, and experimental demos create a
   lot of apparent app surface without matching product value.

5. **Install schema/parsing libraries and remove hand-rolled parsers.**
   Do this in slices: capture event schema, CLI parsing, wiki TOML/Markdown/HTML,
   Swift process runner, release structured checks.

6. **Remove tests that only prove old behavior.**
   Tests should prove current contracts. They should not force legacy routes,
   migration repair paths, scaffold receipts, or fallback input support to live.

## Verification Matrix

Preserve behaviors, not files:

- Scripts size gate: `scripts/` under 2,860 LOC.
- Reference gate: no active CI/build references to retired script names.
- Rust: `cargo test` for affected crates, plus targeted package tests.
- Swift: `swift test --package-path macos`.
- Wiki engine: `npm test` in `wiki-engine` plus schema validation tests.
- Release: `npm --prefix release/runner run build`, release runner tests, and
  `./scripts/release-train.sh validate --channel dev`.
- macOS build: stable dev build and `1context-cli diagnose`.
- Permission work: keep exactly one timestamped dev-build live TCC probe path.
- Browser/GUI: checked-in Playwright specs instead of generated shell tests.

## Agent Run Summary

This plan synthesizes:

- 3 xhigh script-reduction agents.
- 8 high cleanup agents covering wiki, memory DB, capture/attention, macOS,
  wiki-engine, release/demos/runtime/browser, cross-repo schemas, and tests/docs.

All agents were read-only. They did not edit code. One script agent reported
`bash -n scripts/*.sh` and `node --check scripts/*.mjs` passing during its audit.
