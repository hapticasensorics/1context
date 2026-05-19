# Wiki Memory Runtime Bloat Inventory - 2026-05-14

Generated while closing
[`1context-wiki-memory-runtime-v0-goal.md`](../1context-wiki-memory-runtime-v0-goal.md).

Purpose: list the remaining old wiki/runtime paths before deletion so cleanup
can be aggressive without accidentally deleting the new runtime contract.

## Already Replaced In This Worktree

- `memory-core/wiki-engine/**` is staged for deletion.
  - Replacement: first-class `wiki-engine/` at repo root.
  - Proof: `./scripts/test-wiki-render-contract.sh`,
    `./scripts/test-wiki-browser-contract.sh`, and `npm test` in
    `wiki-engine`.
  - Count before deletion: `git ls-files 'memory-core/wiki-engine/**'`
    reported 38 tracked files.

## Delete Candidates

### Old Memory-Core Wiki Content Tree

- `memory-core/wiki/**`
  - Contains old menu-order layout such as
    `memory-core/wiki/menu/10-for-you/20-your-context`.
  - Contains old source/talk/template material that has been replaced by
    `runtime/1Context/user-wiki/wiki.toml`,
    `runtime/1Context/user-wiki/source/families/**`, and
    `runtime/1Context/user-wiki/templates/**`.
  - Count: `git ls-files 'memory-core/wiki/**'` reported 53 tracked files.
  - Deletion condition: memory-core authoring facade has tests for route plans,
    talk appends, proposals, decisions, and render requests, or old tests are
    intentionally deleted/replaced.

### Old Memory-Core Wiki Python Package

- `memory-core/src/onectx/wiki/**`
  - Current files: `cli.py`, `ensure.py`, `evidence.py`, `families.py`,
    `librarian.py`, `manifest.py`, `render.py`, `routes.py`, `site.py`,
    `state.py`.
  - Problem: this package still looks like a self-contained wiki engine and
    source tree owner.
  - Desired replacement: a narrow memory authoring facade that writes route
    plans, talk entries, proposals, decisions, previews, and render requests.
  - Count: `git ls-files 'memory-core/src/onectx/wiki/**'` reported 11 tracked
    files.

### Old Memory-Core Wiki Entry Points

- Former locations:
  - `memory-core/src/onectx/memory/wiki.py`
  - `memory-core/src/onectx/memory/wiki_apply.py`
  - `memory-core/src/onectx/memory/wiki_authoring.py`
  - `memory-core/src/onectx/memory/wiki_executor.py`
  - `memory-core/src/onectx/memory/wiki_validators.py`
- Replacement location:
  - `memory-core/src/onectx/wiki_interface/`
- Related tests:
  - `memory-core/tests/test_wiki_apply.py`
  - `memory-core/tests/test_wiki_ensure.py`
  - `memory-core/tests/test_wiki_inputs.py`
  - `memory-core/tests/test_wiki_route_executor.py`
  - `memory-core/tests/test_wiki_state.py`
  - `memory-core/tests/test_wiki_stats.py`
  - `memory-core/tests/test_wiki_tiers.py`
  - `memory-core/tests/test_wiki_validators.py`
  - plus the already-adjusted `memory-core/tests/test_wiki_tiers.py`.
  - Deletion condition: replace with wiki-interface tests and imports that do
    not invoke renderer internals directly or depend on `memory-core/wiki/**`.

### Placeholder Local Web Publication

- `macos/Sources/OneContextLocalWeb/LocalWeb.swift`
  - `ensurePlaceholderSite()`
  - `placeholderHTML()`
  - hardcoded placeholder pages for `/your-context` and `/for-you`
- `macos/Sources/OneContextDaemon/main.swift`
  - fallback call to `localWeb.ensurePlaceholderSite()`
- Related tests:
  - `macos/Tests/OneContextLocalWebTests/LocalWebTests.swift`
    `testEnsurePlaceholderSitePublishesPortableStaticWikiShell`
  - other tests that assert placeholder `your-context.html` and `for-you.html`
    exist.
  - Deletion condition: Open Wiki last-good-site integration proof exists and
    uninitialized state has an explicit diagnostic path.

Resolution in this branch: placeholder page publication was deleted after this
inventory was written. `CaddyManager.ensureStaticSupportFiles()` now writes only
support JSON/health files, and `wiki.prepare` enters
`WikiRenderCoordinator.renderAndPublish(trigger: "wiki.prepare")`.

### Stale Docs And URL Assumptions

- Docs still describing placeholder/local shell behavior need update or
  deletion after Open Wiki last-good proof:
  - `docs/development.md`
  - `docs/local-web-contract.md`
  - `docs/macos-release-runbook.md`
  - `docs/macos-app-architecture.md`
- Scripts/tests still assuming `/your-context` as the only canonical wiki entry
  should be revisited after `for-you` and custom pages are fully route-tested:
  - `scripts/test-release-app-product-https.sh`
  - `macos/Tests/OneContextLocalWebTests/LocalWebTests.swift`
  - `macos/Tests/OneContextSetupTests/AppSetupTests.swift`

## Keep

- `runtime/1Context/**`
  - Public-safe default runtime tree.
- `runtime-test/`
  - Ignored dev/personal runtime state only.
- `wiki-engine/**`
  - First-class renderer package.
- `wiki-engine/tools/materialize-wiki-pages.py`
  - Current materializer for configured user-owned pages.
- `wiki-engine/tools/serve-site.mjs`
  - Test/dev Caddy-style static route harness.

## Negative Checks To Preserve

- Missing routes must stay diagnostic/404, not hidden fallbacks.
  - Current proof: `./scripts/test-wiki-browser-contract.sh` and
    `./scripts/test-wiki-custom-pages-contract.sh`.
- Packaged app must not include:
  - `runtime-test`
  - generated runtime state
  - old `memory-core/wiki-engine`
  - old `memory-core/wiki/**`
  - a bundled `memory-core` source checkout
  - runtime `npm install` / `npm ci` assumptions
