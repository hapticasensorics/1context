# Wiki Core Dogfood Summary - 2026-05-20

This summarizes the raw disposable `test-results/` dogfood runs from the
portable wiki core stabilization loop. The raw folders were intentionally
pruned before commit; the goal ledger keeps the named local evidence paths, and
this file keeps the durable public-repo summary.

## Strong Proofs

- Primary-address mail ergonomics: agents could claim and mark visible
  list/role/page deliveries from `agent-inbox` using their primary address,
  while the canonical recipient mailbox still recorded the state change.
- Custom page placement: `nav_order` round-tripped through create/status/list
  and page-open, then rendered brand-menu order matched configured page order.
- Browser menu/navigation: custom pages, talk routes, markdown twins, search,
  menu controls, and Reader/Agent toggles were verified in Playwright after the
  browser harness was fixed to use `.playwright-artifacts/`.
- Relationship graph: custom project/topic/tool pages cross-linked cleanly,
  published with no broken internal links, and returned HTTP 200 for page,
  talk, markdown, manifest, index, and search routes.
- Delete/restore lifecycle: tombstoned routes returned 404, normal talk append
  was refused, archive-maintenance talk was allowed, restore returned page and
  talk routes to HTTP 200, and final validation was clean.
- Talk attachments: proposals and replies preserved attachments through
  duplicate filename handling, delete, and restore.
- Stale agents: expired leases now return structured `stale_agent` errors for
  protected control commands; `agent-identify` on the same thread refreshes the
  session.
- Render/frontmatter validation: authored page frontmatter incompatible with
  the renderer is now caught by `wiki.validate` as blocking
  `invalid_page_frontmatter`, and `wiki.publish` stops before rendering with
  `next_action=repair_source`.

## Harness Findings

- Deep Unix-domain socket paths can fail; disposable daemon sockets should live
  under `/tmp` or another short path.
- Static HTML greps are insufficient for controls injected by `enhance.js`;
  browser-runtime checks are required.
- Raw Playwright output must not target repo `test-results/`; the project now
  uses `.playwright-artifacts/`.
- One sidecar browser-controls run failed because its disposable server was not
  reachable. This was treated as harness failure, not product failure.
- One tombstone/mail sidecar left only a Cargo build cache; the cache was
  deleted and the run was treated as inconclusive.

## Regression Commands

The stabilization loop repeatedly used these proof commands:

```sh
cargo fmt --check
cargo test -q -p onecontext-wiki-core stale_agent_control_commands_require_identify_refresh
cargo test -q -p onecontext-wiki-daemon stale_agent_errors_are_actionable
cargo test -q -p onecontext-wiki-core validate_blocks_renderer_incompatible_page_frontmatter
cargo test -q -p onecontext-wiki-daemon publish_uses_validation_repair_hints_for_source_frontmatter_blocks
npm test --prefix wiki-engine
swift build --package-path macos --product 1context
swift build --package-path macos --product 1contextd
swift test --package-path macos --filter 'WikiCoreRPCBridgeTests|WikiCoreProcessClientTests|LocalWebTests/testWikiLocalAPI'
```
