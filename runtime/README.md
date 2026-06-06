# 1Context Repo Runtime Layout

This folder is the blessed public-safe runtime data tree for the installed
1Context file layout. It tracks the production-shaped directory structure plus
reviewed default wiki files, templates, configs, and guardrails. It must not
track generated local machinery or private personal data.

The production contract lives in
[`../docs/user-data-spec.md`](../docs/user-data-spec.md).

## Rule

```text
Use the same names in development that the installed app uses in production.
Do not make generated runtime state part of the source tree.
```

## Runtime Tree

The tracked tree mirrors the installed locations:

```text
runtime/
  1Context/
    user-wiki/
      templates/
      assets/
      source/
      site/

    context-engine/
      orchestrators/
        wiki-company-orchestrator-v1/
          orchestrator.toml
          phases.toml
          packet-policy.toml
          routing.toml
          receipts.toml
      agents/
        directory/
        harness/
        policies/
      mail/
        mailboxes/
        threads/
      packs/
        wiki-company-v1/
          plugin.toml
          providers.toml
          native-memory.toml
          linking.toml
          harnesses/
          agents/
          jobs/
          prompts/
          lived-experiences/

  Library/
    Application Support/
      1Context/
        wiki-site/
          current/
          next/
          previous/
        indexes/
        local-web/
          caddy/
        setup/
        sockets/
        staging/
        run/

    Logs/
      1Context/

    Caches/
      1Context/
```

The path names intentionally mirror the installed locations:

```text
runtime/1Context/user-wiki/
  mirrors ~/1Context/user-wiki/

runtime/1Context/context-engine/
  mirrors ~/1Context/context-engine/

runtime/Library/Application Support/1Context/
  mirrors ~/Library/Application Support/1Context/
```

## Ownership

`runtime/1Context/user-wiki/` is the blessed repo copy of the readable wiki:
source, talk pages, templates, assets, and static site export.

`runtime/1Context/context-engine/` is the blessed repo copy of the agent
workspace: the shipped wiki-company pack, the wiki-company orchestrator policy,
persistent agent identity/proof folders, and Agent Mail folders.

The Rust `onecontext-context-engine` binary is the release owner for orchestration
inside this folder. Python `memory-core` can remain as a prototype/reference
source during development, but release wiki-company updates should be expressed
through Context Engine plus Agent Mail, not a Python checkout.

`runtime/Library/Application Support/1Context/` is app machinery: local web
mirrors, sockets, staging, setup state, and rebuildable derived indexes.

For now, wiki-company execution history is mail-first:

```text
runtime/1Context/context-engine/mail/threads/wiki-company.jsonl
```

Do not reintroduce a `runs/` filesystem hierarchy, top-level proposal warehouse,
or context-engine index ledger until the Postgres/Timescale execution-history
design exists.

## Runtime Test

Development tools create an ignored working copy at the repo root:

```text
runtime-test/
```

`runtime-test/` is initialized from the tracked `runtime/` tree and may then
import a local fixture. It is safe to fill with personal review data because it
is ignored by git. Promote files back into `runtime/` only after they are
scrubbed and intended to become public-safe defaults.

## Activation

The runtime-test is activated by a debug-only runtime-home switch:

```bash
ONECONTEXT_DEV_RUNTIME_HOME=/absolute/path/to/runtime-test
```

Release builds ignore that switch. Shipped product code must not depend on broad
runtime path override environment variables or source-tree fallbacks.

The current wiki-engine V0 review slice uses:

```text
scripts/init-dev-wiki-runtime.sh
  copies missing files from the tracked runtime tree into runtime-test and
  optionally overlays a current-shape local fixture

scripts/test-wiki.sh
  creates a configured custom source/talk page, renders the site, and checks
  canonical source, talk, markdown-twin, and route-manifest output
```

Run:

```bash
./scripts/test-wiki.sh
```

The helper expects fixture imports to be shaped like `runtime-test/`, with
`1Context/` and optionally `Library/` at the fixture root. It copies fixture
files only when the destination is missing. Existing edited files are left in
place.

After copying defaults and fixture files, the initializer creates configured
source-backed pages from `runtime-test/1Context/user-wiki/wiki.toml`. The
registry is user data: it defines the available routes, family paths, templates,
and talk defaults. Page creation creates only missing files, writes lifecycle
evidence to:

```text
runtime-test/1Context/user-wiki/.1context/page-ledger.jsonl
```

and respects per-page tombstones such as:

```text
user-wiki/source/families/<group>/<family>/source/<slug>.tombstone.toml
```

`source/families/` is semantic source ownership, not menu order. Use stable
groups such as `context`, `work`, `reference`, `for-you`, and `system`; keep nav
placement in `wiki.toml` fields such as `navigation`, `primary_navigation`,
`utility_navigation`, `[[pages]]`, and `[[site_pages]]`.

Set `ONECONTEXT_SKIP_WIKI_MATERIALIZE=1` when you need to test just the raw
runtime folder copy.

Package smoke should continue to prove that the installed app uses production
paths and does not ship generated runtime state, source checkouts,
`node_modules`, runtime package installation, or a long-running Python web
server.

## Git Hygiene

Only reviewed public-safe data should be tracked here. That includes blessed
wiki source, talk, templates, assets, static site output, and public-safe
context-engine configuration.

Private or experimental work belongs in repo-root `runtime-test/`, which is
ignored by git. Promote from `runtime-test/` into `runtime/` only after review
and scrubbing.

`runtime/.gitignore` protects scratch import locations and app machinery under
this folder. It does not ignore `runtime/1Context/user-wiki/source`,
`runtime/1Context/user-wiki/site`, or `runtime/1Context/user-wiki/templates`,
because those are the blessed public data surfaces.
