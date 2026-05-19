# 1Context Wiki Publishing System Runbook

- Status: operating guide for the canonical V0 publishing API
- Last updated: 2026-05-19

Use this with [Wiki Publishing System API](wiki-publishing-system-api.md). The
API doc defines the contract; this file shows how to use it from each side of
the system.

## End-To-End Publishing Loop

The normal closed-loop path is:

```text
edit user source/talk/wiki.toml
  -> materialize missing configured pages
  -> render to staging
  -> validate route manifest and markdown twins
  -> publish user-wiki/site
  -> mirror last-good site to Application Support
  -> serve with Local Web
  -> verify in browser or harness
```

RuntimeDefaults participate only at first run or upgrade. They seed and
backfill missing files, preserve existing user files, write conflict proposals,
and record a setup ledger before the render runs from actual user data.

## Quick Local Proof

```bash
npm ci --prefix wiki-engine
swift test --package-path macos
npm --prefix wiki-engine test
./scripts/test-release-train.sh
./scripts/test-wiki.sh
```

Package plus RuntimeDefaults proof:

```bash
./scripts/release-train.sh build --channel dev
ONECONTEXT_ALLOW_LAUNCH_AGENT_SMOKE=1 ./scripts/test-launch-agent-package.sh
./scripts/test-wiki-runtime-defaults-scenarios.sh
```

Expected steady-state timing:

- local dev app build: about 70 to 90 seconds on the current machine
- local wiki publishing proof: about 2 to 4 minutes
- push plus GitHub Actions green: about 5 minutes when no new failure appears

## Initialize A Dev Runtime Fixture

Use `runtime-test/` for private local scenarios. Do not wipe all of
`runtime-test`; create a named subfolder for destructive tests.

```bash
./scripts/init-dev-wiki-runtime.sh runtime-test/my-scenario
```

This creates:

```text
runtime-test/my-scenario/1Context/user-wiki/
runtime-test/my-scenario/1Context/context-engine/
runtime-test/my-scenario/Library/Application Support/1Context/
runtime-test/my-scenario/Library/Logs/1Context/
runtime-test/my-scenario/Library/Caches/1Context/
```

## Add A Configured Page

Edit:

```text
<runtime-home>/1Context/user-wiki/wiki.toml
```

Add:

```toml
[[pages]]
id = "dummy-custom"
enabled = true
title = "Dummy Custom"
slug = "dummy-custom"
route = "/dummy-custom"
family_group = "custom"
family_group_title = "Custom"
family_id = "dummy-custom"
family_title = "Dummy Custom"
type = "context-page"
template = "pages/context-page.md"
talk_conventions_template = "talk/conventions.md"
summary = "Fixture custom page generated from the fallback template."
nav_order = 900
```

Materialize:

```bash
python3 wiki-engine/tools/materialize-wiki-pages.py <runtime-home>
```

Verify:

```bash
test -f <runtime-home>/1Context/user-wiki/source/families/custom/dummy-custom/source/dummy-custom.md
test -f <runtime-home>/1Context/user-wiki/source/families/custom/dummy-custom/talk/dummy-custom.talk/_meta.yaml
```

## Render A Runtime Fixture

```bash
node wiki-engine/tools/render-site.mjs \
  --source-root <runtime-home>/1Context/user-wiki/source \
  --output /tmp/1context-wiki-site \
  --result-json /tmp/1context-wiki-render.json
```

Inspect:

```bash
python3 -m json.tool /tmp/1context-wiki-render.json
python3 -m json.tool /tmp/1context-wiki-site/.1context/route-manifest.json
```

Serve locally:

```bash
PORT_FILE=/tmp/1context-wiki-port \
  node wiki-engine/tools/serve-site.mjs /tmp/1context-wiki-site
```

## Prove RuntimeDefaults Behavior

Build first:

```bash
./scripts/release-train.sh build --channel dev
```

Run scenario harness:

```bash
./scripts/test-wiki-runtime-defaults-scenarios.sh
```

The harness writes ignored fixtures under:

```text
runtime-test/wiki-runtime-defaults-scenarios/
```

It proves:

- fresh user backfill copies missing defaults
- edited `wiki.toml` is preserved
- changed packaged defaults create conflict proposals
- custom configured pages materialize from fallback templates
- page and talk routes render and publish
- installer ledgers preserve packaged manifest identity

Summary artifact:

```text
/tmp/1ctx-runtime-defaults-scenarios/runtime-defaults-scenarios-summary.json
```

## Inspect Packaged Freshness

```bash
python3 -m json.tool \
  dist/1Context.app/Contents/Resources/RuntimeDefaults/1Context/.1context/runtime-defaults-manifest.json
```

Important fields:

- `release_version`
- `source_control.git_commit`
- `source_control.git_dirty`
- `hashes.runtime_defaults_source`
- `hashes.runtime_defaults_site`
- `hashes.wiki_engine`
- `hashes.materializer`
- `hashes.renderer`
- `render_summary.status`
- `render_summary.route_count`
- `render_summary.markdown_twin_count`

A clean package build should report:

```json
{
  "source_control": {
    "git_dirty": false
  },
  "render_summary": {
    "status": "published"
  }
}
```

## Operator Edit Recipe

Use source files for durable content edits:

```text
~/1Context/user-wiki/source/families/<group>/<family>/source/<slug>.md
```

Use talk folders for discussion, proposals, and review:

```text
~/1Context/user-wiki/source/families/<group>/<family>/talk/<slug>.talk/
```

After editing, request a render through the app or daemon. Do not edit
Application Support mirrors directly.

## Memory Agent Recipe

For a normal memory agent:

1. Read `wiki.toml` to resolve page ids, routes, families, and talk folders.
2. Append talk entries or write proposal artifacts under user-owned paths.
3. For source edits, preserve old hash and ownership scope in the proposal.
4. Promote accepted changes into `user-wiki/source`, `templates`, or
   `wiki.toml`.
5. Request `wiki.refresh`.
6. Read render/ledger evidence instead of assuming publication succeeded.

Agents may write under:

```text
~/1Context/user-wiki/
~/1Context/context-engine/
```

Agents must not write under:

```text
~/Library/Application Support/1Context/wiki-site/
```

## Swift App Recipe

On daemon startup:

1. `RuntimePaths.current()` resolves production paths.
2. `WikiRuntimeDefaultsInstaller.installMissingDefaults()` copies missing
   packaged defaults and writes conflict proposals for changed user files.
3. Setup ledger is written to
   `Library/Application Support/1Context/setup/runtime-defaults-install.json`.
4. The daemon queues `wiki.prepare`.
5. `WikiRenderCoordinator` renders from actual user data.
6. The coordinator validates and promotes last-good output.

Development override:

```bash
ONECONTEXT_DEV_RUNTIME_HOME=runtime-test/my-scenario \
ONECONTEXT_RUNTIME_DEFAULTS_DIR=dist/1Context.app/Contents/Resources/RuntimeDefaults \
ONECONTEXT_WIKI_ENGINE_DIR=wiki-engine \
swift test --package-path macos --filter WikiRuntimeDefaultsScenarioTests
```

## Release Recipe

Normal dev proof:

```bash
./scripts/release-train.sh validate --channel dev
./scripts/release-train.sh build --channel dev
ONECONTEXT_ALLOW_LAUNCH_AGENT_SMOKE=1 ./scripts/test-launch-agent-package.sh
```

Before trusting a package:

```bash
hdiutil info
```

There should be no lingering mounted `1Context-*.dmg` validation image.

## Browser Contract Recipe

```bash
./scripts/test-wiki.sh
```

This creates a fixture runtime, adds a dummy custom page, materializes, renders,
serves it, then uses Playwright to verify:

- page routes
- talk routes
- markdown twins
- brand menu links
- table-of-contents anchors
- Agent view markdown loading
- missing-route diagnostics
- no local path leakage

The timeout is intentionally larger than a unit test because it is an
exhaustive browser sweep, not a smoke check.

## Troubleshooting

### `render-site.mjs` says no source pages

Run materialization:

```bash
python3 wiki-engine/tools/materialize-wiki-pages.py <runtime-home>
```

Then check for:

```text
<runtime-home>/1Context/user-wiki/source/families/*/*/source/*.md
```

### User edits disappeared

This is a blocker. Defaults install must preserve user files. Check:

```text
~/Library/Application Support/1Context/setup/runtime-defaults-install.json
~/1Context/context-engine/proposals/wiki/runtime-defaults/
```

### CI build fails only on clean checkout

Reproduce with a clean tree and run:

```bash
./scripts/release-train.sh build --channel dev
```

The RuntimeDefaults manifest should produce `git_dirty=false`.

### Browser test times out

Use:

```bash
ONECONTEXT_WIKI_BROWSER_TIMEOUT_MS=180000 ./scripts/test-wiki.sh
```

Then inspect the artifact directory printed by the script.

### Packaged app includes private state

Run:

```bash
ONECONTEXT_ALLOW_LAUNCH_AGENT_SMOKE=1 ./scripts/test-launch-agent-package.sh
```

The package smoke rejects `runtime-test`, local developer paths, Python bytecode
caches in bundled WikiEngine, retired `memory-runtime`, and private fixtures.
