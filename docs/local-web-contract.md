# 1Context Wiki Web Contract

This contract keeps the public Swift shell protected from the experimental
memory core while still letting the wiki improve quickly. It is intentionally a
local-first version of the cloud web contract: the browser sees portable static
artifacts and stable `/api/wiki/*` routes, while the host adapter can be local
Swift/Caddy today or cloud CDN/API infrastructure later.

## Ownership

Swift owns local web infrastructure:

- starts and stops the packaged Caddy process
- chooses and reports the canonical local URL
- writes Caddy config, pid, state, and logs under 1Context app paths
- publishes the last successful wiki render into `wiki-site/current`
- keeps `wiki-site/previous` and `wiki-site/next` for atomic publish safety

Caddy owns serving:

- binds only to `127.0.0.1`
- serves `http://wiki.1context.localhost:17319/your-context`
- serves static wiki files directly
- reverse-proxies dynamic `/api/wiki/*` routes to the Swift daemon adapter
- does not know about memory jobs, imports, screen capture, or agent state

The Swift daemon owns the local dynamic wiki API:

- `GET /api/wiki/health`
- `GET /api/wiki/search?q=...`
- `GET /api/wiki/bookmarks`
- `GET`, `PATCH`, and `POST /api/wiki/state`
- `GET /api/wiki/chat/config`
- `POST /api/wiki/chat`, `/api/wiki/chat/provider`, and `/api/wiki/chat/reset`

These routes are product contract, not Caddy contract. Browser code should only
call relative `/api/wiki/*` paths so the same static site can run behind local
Caddy or a future cloud host.

The Python memory core owns rendering:

- creates or updates wiki source scaffolds
- renders markdown into themed static HTML and local static JSON artifacts
- never starts a long-lived web server in public release
- never owns the canonical local URL

## Serving Invariant

The browser always sees the last successful published render from
`wiki-site/current`. If no render has ever been published, Swift writes a themed
fallback shell using the same 1Context CSS, JS, and icons.

Published static artifacts remain portable. `site-manifest.json`,
`content-index.json`, `wiki-stats.json`, and static `api/wiki/*.json` files are
kept in the site root for cloud export, static fallback, and inspection. Local
dynamic behavior comes from the Swift daemon API adapter.

Refresh does not blank the site. It publishes existing rendered artifacts when
they are still valid, and rerenders only when there is no servable render or
the render manifest no longer matches its inputs.

## Lifecycle

The menu bar owns the local web edge. If the menu bar is present, Caddy should
be up. Quitting 1Context stops Caddy.

The daemon owns remembering work: screen capture, importers, memory jobs, and
agentic memory orchestration. It also owns the local wiki API adapter because
search state and future librarian chat belong with memory/runtime state.
Stopping the daemon must not tear down Caddy; static pages should still load,
with dynamic API calls degrading cleanly.

## Cloud Compatibility

The local adapter must not leak into the web contract:

- no browser-visible socket paths or loopback-only URLs
- no Caddy-specific behavior required by `enhance.js`
- no render-on-request behavior from API routes
- local-only capabilities must be explicit in API capability responses
- cloud can replace the host adapter with object storage/CDN plus cloud APIs
  without changing page routes or browser API paths

## Agent Hooks

Claude and Codex hooks should read the wiki URL from 1Context-managed config or
daemon RPC. They should not hardcode ports or start web servers themselves.

Message and context improvements should ship through 1Context updates and
daemon state, not through separate hook plugin releases.

## Temporary Migrations Pattern

Because public release is still pre-user, backwards compatibility is not a
standing product goal. When a migration is needed only to clean up our own
developer machines or a short-lived pre-release shape, keep it contained and
easy to delete:

- put the cleanup in a `Migrations/` folder owned by the module that needs it
- name the migration after the retired surface, not the new architecture
- keep matching narrow and explicit; prefer deleting one known bad shape over
  supporting many historical variants
- call the migration from install/repair/startup paths only where stale state
  can actually interfere with the current product
- do not let temporary migration code become the normal abstraction boundary
- delete the migration after both founder machines and the release package have
  been through the new install/repair path

Current example: `OneContextAgent/Migrations/LegacyPrivateAgentHookMigration.swift`
removes the old private-4 Python hook commands from Claude and Codex configs
so both agents use the installed first-party `1context agent hook` command and
therefore read the live canonical wiki URL from 1Context config.

## Boundary Rules

- No Python HTTP server in public release.
- No direct serving from memory-core generated directories.
- No user-installed Caddy dependency; release artifacts bundle Caddy.
- No port fallback for the canonical product URL.
- No private stderr, prompts, or wiki source text should be surfaced through
  public CLI errors.
