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

Current caveat: app startup renders existing source, but production does not
yet expose a bundled materialize-then-publish action. Dev/build harnesses call
the materializer directly. The target API is `wiki.publish` with
`materialize=true` so agents can add pages through one consumer-facing action.

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

This is the template fallback path. The page did not exist as source yet; the
materializer used `templates/pages/context-page.md` and
`templates/talk/conventions.md` to create missing user-owned files. If any of
those destination files already exist, the materializer leaves them alone and
records `skipped_existing` in:

```text
<runtime-home>/Library/Application Support/1Context/setup/wiki-page-materialize.toml
```

If a page has been intentionally removed, add a tombstone:

```text
<runtime-home>/1Context/user-wiki/source/families/custom/dummy-custom/source/dummy-custom.tombstone.toml
```

Materialization must then report the page as tombstoned and must not recreate
`dummy-custom.md`.

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

Direct Node rendering is for fixture/debug proof. It does not update
`~/1Context/user-wiki/site`, does not mirror Application Support, and does not
represent the app's last-good publish behavior.

## Trigger The Current Daemon Render

The current app trigger is newline-delimited JSON-RPC over the daemon Unix
socket. It is asynchronous and whole-site scoped.

```bash
python3 - <<'PY'
import json
import os
import socket

socket_path = os.path.expanduser("~/Library/Application Support/1Context/run/1context.sock")
payload = {"jsonrpc": "2.0", "id": 1, "method": "wiki.refresh", "params": {}}
with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as client:
    client.connect(socket_path)
    client.sendall((json.dumps(payload) + "\n").encode("utf-8"))
    print(client.recv(65536).decode("utf-8").strip())
PY
```

Poll status:

```bash
python3 - <<'PY'
import json
import os
import socket

socket_path = os.path.expanduser("~/Library/Application Support/1Context/run/1context.sock")
payload = {"jsonrpc": "2.0", "id": 1, "method": "wiki.status", "params": {}}
with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as client:
    client.connect(socket_path)
    client.sendall((json.dumps(payload) + "\n").encode("utf-8"))
    print(client.recv(65536).decode("utf-8").strip())
PY
```

Interpretation:

- `render.state = refreshing` means a manual refresh is queued or running.
- `render.state = starting` means automatic startup preparation is queued or
  running.
- `render.last.status = skipped` means Swift accepted unchanged source inputs
  and republished the validated existing site.
- `render.last.status = failed` means Local Web should still serve the previous
  last-good site.

The public CLI intentionally exposes only support-oriented wiki commands today.
Do not treat `1context wiki refresh` as a shipped interface unless we
explicitly add it back as part of the greenfield consumer API.

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

To prove the app publish path after an edit:

```bash
python3 -m json.tool ~/1Context/user-wiki/site/.1context/current-render.json
python3 -m json.tool ~/1Context/user-wiki/site/.1context/route-manifest.json
python3 -m json.tool "$HOME/Library/Application Support/1Context/wiki-site/current/.1context/route-manifest.json"
```

The source-site route manifest and the app-support mirror route manifest should
both contain the edited page route.

## Memory Agent Recipe

For a normal memory agent:

1. Read `wiki.toml` to resolve page ids, routes, families, and talk folders.
2. Append talk entries or write proposal artifacts under user-owned paths.
3. For source edits, preserve old hash and ownership scope in the proposal.
4. Promote accepted changes into `user-wiki/source`, `templates`, or
   `wiki.toml`.
5. If the change adds a configured page, materialize missing files before
   publishing. In the target API this is `wiki.publish(materialize=true)`;
   today dev harnesses call `materialize-wiki-pages.py`.
6. Request `wiki.refresh` through the daemon for existing materialized source.
7. Read render/ledger evidence instead of assuming publication succeeded.

Agents may write under:

```text
~/1Context/user-wiki/
~/1Context/context-engine/
```

Agents must not write under:

```text
~/Library/Application Support/1Context/wiki-site/
```

If render fails, the agent should not retry blindly. It should read:

```text
~/1Context/user-wiki/site/.1context/current-render.json
~/1Context/user-wiki/site/.1context/render-events.jsonl
~/Library/Logs/1Context/1contextd.log
```

Then write a repair proposal under `~/1Context/context-engine/proposals/` or a
talk entry on the affected page.

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

Important startup limitation: `wiki.prepare` currently prepares/publishes
existing user source. It does not yet run a production-bundled materializer for
new user-authored `[[pages]]` entries. That belongs in the greenfield
`wiki.publish` action.

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

### Added page does not render

Check whether it is registered and materialized:

```bash
rg -n 'id = "dummy-custom"|route = "/dummy-custom"' ~/1Context/user-wiki/wiki.toml
test -f ~/1Context/user-wiki/source/families/custom/dummy-custom/source/dummy-custom.md
test -f ~/1Context/user-wiki/source/families/custom/dummy-custom/talk/dummy-custom.talk/_meta.yaml
```

If only `wiki.toml` changed, the page has not become a render input yet. Run
the materializer in dev, or use the future `wiki.publish(materialize=true)`
action once it exists in the app.

### Render fails

Expected behavior:

- failed staging output is discarded
- `wiki-site/current` keeps serving the last-good site
- `current-render.json` records failure details when a source site exists
- `wiki.status` reports the failed queue history and backoff

Inspect:

```bash
python3 -m json.tool ~/1Context/user-wiki/site/.1context/current-render.json
tail -n 20 ~/1Context/user-wiki/site/.1context/render-events.jsonl
tail -n 80 ~/Library/Logs/1Context/1contextd.log
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
