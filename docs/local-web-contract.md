# 1Context Wiki Web Contract

This contract keeps the public macOS shell small while still letting the wiki
improve quickly. It is intentionally a local-first version of the cloud web
contract: the browser sees portable static artifacts and stable `/api/wiki/*`
routes, while the host adapter can be local Swift/Caddy plus the portable wiki
core today or cloud CDN/API infrastructure later.

## Ownership

The macOS host owns local web infrastructure and served-site uptime:

- starts and stops the packaged Caddy process
- chooses and reports the canonical local URL
- writes Caddy config, pid, state, and logs under 1Context app paths
- prepares the private local wiki shell in `wiki-site/current`
- delegates wiki lifecycle, talk/mail, notification, validation, and
  publication semantics to the portable wiki core
- installs or repairs the required local HTTPS setup through an explicit admin
  authorization flow
- does not decide whether wiki prose, frontmatter, routes, or agent edits are
  semantically correct

The portable wiki core owns mechanical safe-to-serve publication:

- validates candidate static bundles before promotion
- publishes the last-good rendered wiki artifacts in `wiki-site/current`
- exposes redacted dynamic `/api/wiki/*` behavior through the daemon adapter
- preserves last-good output on failure

Caddy owns serving:

- binds only to `127.0.0.1`
- serves the wiki on a high local TLS backend port owned by the user process
- serves static wiki files directly
- reverse-proxies dynamic `/api/wiki/*` routes to the wiki daemon adapter
- does not know about memory jobs, imports, screen capture, or agent state

The bundled 1Context ServiceManagement helper owns only `127.0.0.1:443`. It
forwards encrypted TCP traffic to the user-owned Caddy backend and does not read
wiki content. The app registers or repairs that helper through native setup UI
and macOS Login Items & Extensions approval. The same flow trusts the local
Caddy CA in the user's login keychain. Uninstall removes both.

The default browser-open product URL is:

```text
https://localhost/your-context
```

The branded alias `https://wiki.1context.localhost/your-context` is still served
by Caddy and reported by diagnose, but product UI must not depend on that
multi-label `.localhost` name for either app readiness or default browser-open
behavior. Some macOS networking clients and browsers do not resolve it
consistently.

Browser-visible high-port HTTPS
(`https://localhost:<port>/your-context` or
`https://wiki.1context.localhost:<port>/your-context`) remains a test and
development harness mode only. Product UI should not silently fall back to it
when the local HTTPS setup is missing.

The app's internal health probe uses literal loopback
`https://127.0.0.1:<port>/__1context/health` against the user-owned Caddy edge.
That keeps app readiness off local DNS entirely. Diagnose additionally probes
`https://localhost/__1context/health` through the privileged proxy and the
branded host health URL, but those probes are classified separately.

The wiki daemon owns the local dynamic wiki API:

- `GET /api/wiki/health`
- `GET /api/wiki/search?q=...`
- `GET /api/wiki/bookmarks`
- `GET`, `PATCH`, and `POST /api/wiki/state`
- future inbox/status routes only after the agent mail protocol is implemented

These routes are product contract, not Caddy contract. Browser code should only
call relative `/api/wiki/*` paths so the same static site can run behind local
Caddy or a future cloud host.

Future memory publication must enter through explicit app-owned artifacts:

- no bundled source checkout in the signed app
- no generated developer pages in the installed user wiki
- no long-lived Python web server in public release
- no alternate owner for the canonical local URL
- no agent or memory job writes directly into `wiki-site/current`

Development and operator planning documents belong under `docs/`, not in the
installed user's wiki. The active release cleanup goal is
`docs/goals/1context-delete-bloat-goal.md`, and historical Sparkle lockdown
evidence is archived under `docs/goals/archive/`; normal `wiki-site/current`
publishes must not expose either as `/goal`.

This cleanup is a product boundary decision first. The repo may still carry
working-tree implementation hardening while we decide when to promote it into a
signed release, but release packages should converge on the rule above.

## Serving Invariant

The browser always sees the last successful published render from
`wiki-site/current`. If no render has ever been published, setup and status
report an uninitialized or failed wiki state instead of hiding it behind a
placeholder page.

Published static artifacts remain portable. `.1context/route-manifest.json`,
`.1context/content-index.json`, markdown twins, and any static
`api/wiki/*.json` files are kept with the site for cloud export, static
fallback, and inspection. Local dynamic behavior comes from the wiki daemon API
adapter, currently hosted by Swift and targeted to move into the portable core.

Refresh does not blank the site. It publishes existing rendered artifacts when
they are still valid, and rerenders only when there is no servable render or
the render manifest no longer matches its inputs.

Agents and memory jobs do not publish. They may read the wiki, propose or apply
authorized source edits, leave reasoning in talk folders, and cite evidence.
Memory agents own semantic proposals and accepted source changes. The portable
wiki core owns mechanical safe-to-serve checks and the atomic swap. Swift hosts
that core on macOS. If source edits fail semantic validation or a candidate
bundle fails mechanical checks, the browser continues serving the last good
`wiki-site/current`.

Agent-facing status should be concise. Startup context may say that the current
site is healthy, that source has unpublished edits, that inbox mail is waiting,
or that a targeted semantic repair is needed. Agents should not receive renderer
logs or publisher internals as ordinary work.

## Lifecycle

The menu bar owns the local web edge. If the menu bar is present, Caddy should
be up after required setup is satisfied. If local HTTPS trust or the privileged
443 proxy is missing, the wiki is intentionally blocked and status/diagnose
should report the missing requirement instead of starting a fallback web edge.
Quitting 1Context stops Caddy; uninstall removes the ServiceManagement helper and
trusted local CA.

The daemon owns runtime state, the local wiki API adapter, inbox/notification
API state, and the `wiki.publish` publication entrypoint. Memory publication
extends the same static site and `/api/wiki/*` contract without changing the
local HTTPS edge. Stopping the daemon must not tear down Caddy;
already-published static pages should still load, with dynamic API calls
degrading cleanly.

## Cloud Compatibility

The local adapter must not leak into the web contract:

- no browser-visible socket paths or high-port backend URLs
- no Caddy-specific behavior required by browser JavaScript
- no render-on-request behavior from API routes
- local-only capabilities must be explicit in API capability responses
- cloud can replace the host adapter with object storage/CDN plus cloud APIs
  without changing page routes or browser API paths

## Boundary Rules

- No Python HTTP server in public release.
- No bundled Python orchestration source checkout in the app release.
- No direct serving from generated source directories.
- No development/operator goal pages in the installed user wiki.
- No semantic wiki validation in the macOS host.
- No agent or hired job directly publishes the served wiki.
- No user-installed Caddy dependency; release artifacts bundle Caddy.
- No port fallback for the canonical product URL.
- No root-owned process reads user wiki files or memory content.
- No private stderr, prompts, or wiki source text should be surfaced through
  public CLI errors.
