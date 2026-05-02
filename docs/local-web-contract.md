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
- installs or repairs the required local HTTPS setup through an explicit admin
  authorization flow

Caddy owns serving:

- binds only to `127.0.0.1`
- serves the wiki on a high local TLS backend port owned by the user process
- serves static wiki files directly
- reverse-proxies dynamic `/api/wiki/*` routes to the Swift daemon adapter
- does not know about memory jobs, imports, screen capture, or agent state

The bundled 1Context ServiceManagement helper owns only `127.0.0.1:443`. It
forwards encrypted TCP traffic to the user-owned Caddy backend and does not read
wiki content. The app registers or repairs that helper through native setup UI
and macOS Login Items & Extensions approval. The same flow trusts the local
Caddy CA in the user's login keychain. Uninstall removes both.

The canonical product URL is:

```text
https://wiki.1context.localhost/your-context
```

High-port HTTP (`http://wiki.1context.localhost:<port>/your-context`) remains a
test and development harness mode only. Product code should not silently fall
back to it when the local HTTPS setup is missing.

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
be up after required setup is satisfied. If local HTTPS trust or the privileged
443 proxy is missing, the wiki is intentionally blocked and status/diagnose
should report the missing requirement instead of starting a fallback web edge.
Quitting 1Context stops Caddy; uninstall removes the ServiceManagement helper and
trusted local CA.

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

## Boundary Rules

- No Python HTTP server in public release.
- No direct serving from memory-core generated directories.
- No user-installed Caddy dependency; release artifacts bundle Caddy.
- No port fallback for the canonical product URL.
- No root-owned process reads user wiki files or memory content.
- No private stderr, prompts, or wiki source text should be surfaced through
  public CLI errors.
