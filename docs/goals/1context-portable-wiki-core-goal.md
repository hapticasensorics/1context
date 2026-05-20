# Milestone: Portable Wiki Core

## Goal

Build the greenfield wiki core as portable infrastructure that Swift, the
memory system, agents, tests, and future hosts can all use. The reason to use
Rust is ownership clarity: wiki semantics are product logic, while Swift is
Apple platform logic. The first milestone is not full app parity; it is a real
Rust core with page lifecycle, inventory, talk, inbox, agent directory,
notifications, and proof that those operations work against production-shaped
user data.

Swift remains the macOS host. The memory system remains the authoring and
reasoning system. Both call the same portable wiki core instead of reimplementing
wiki rules.

## Done When

- A Rust `onecontext-wiki-core` library owns inventory, page lifecycle,
  page ledger, agent directory, talk append, mail inbox, notification outbox,
  and typed JSON results.
- Swift can call the Rust core for app/daemon behavior, and the memory system
  can call the Rust core for authoring/publishing behavior, without either one
  duplicating wiki placement, fallback, talk, or inbox rules.
- A thin Rust CLI/daemon adapter exposes the same consumer verbs agents should
  use: list, page status, page open, page create, page delete, publish,
  agent register, talk append, mail inbox, and notification poll.
- The runtime layout includes durable agent directory, mail, and notification
  folders under `context-engine`.
- A dev fixture can start from `runtime/1Context`, create configured pages from
  templates, register an agent, append a talk proposal, deliver inbox mail,
  enqueue a notification, and render/publish or report the next publish action.
- Closed-loop tests cover the Rust core and the CLI path using only file-backed
  user data.
- The old Swift-only wiki semantic path is no longer the only implementation
  of inventory/list/page status.

## Checklist

### 1. Baseline

- [x] Architecture docs define the target core as portable/Rust with Swift as
  the macOS host. Evidence: `docs/wiki-system-architecture.md` and
  `docs/wiki-publishing-system-api.md`.
- [x] Current code still has wiki list/status/refresh in Swift and no Rust
  workspace. Evidence: repository scan on 2026-05-19 found `macos/Package.swift`
  and no `Cargo.toml`.
- [x] Runtime defaults include the V0 folders needed by the directory, mail,
  and notification model. Evidence:
  `runtime/1Context/context-engine/agents/directory/`,
  `runtime/1Context/context-engine/mail/mailboxes/`, and
  `runtime/1Context/context-engine/notifications/cursors/`.

### 2. Portable Core

- [x] Add a root Cargo workspace and `crates/onecontext-wiki-core`. Evidence:
  `Cargo.toml` and `cargo test --workspace`.
- [x] Implement runtime path resolution for production-shaped `1Context` roots.
  Evidence: `onecontext-wiki --root runtime-test/portable-wiki-core-smoke/1Context`.
- [x] Implement `wiki.list` and `wiki.page.status` over `wiki.toml`, source,
  talk folders, ledgers, and rendered site evidence. Evidence: closed-loop CLI
  proof reported four configured pages before creation and `topics` as
  `rendered` after render.
- [x] Implement `wiki.page.create` with no-overwrite template fallback,
  source/talk placement, family-local templates, and page-ledger events.
  Evidence: `page-create topics` produced 10 evidence rows and created source,
  talk metadata, curator, conventions, and family-local templates.
- [x] Implement agent register, heartbeat, and retire ledgers plus current
  directory view. Evidence: CLI proof registered
  `agent_codex_019e3f723471`, heartbeated it, then retired it.
- [x] Implement talk append, delivery records, mailbox views, and notification
  outbox. Evidence: CLI proof appended a Topics proposal, delivered one inbox
  message, queued one notification, and marked the message `claimed`.

### 3. Adapter Surface

- [x] Add `crates/onecontext-wiki-daemon` as a thin CLI/JSON adapter over the
  core. Evidence: `cargo run -q -p onecontext-wiki-daemon -- ...`.
- [x] Expose agent-facing commands for list, page status, page create,
  page open, page delete, publish, register, heartbeat, retire, talk append,
  mail inbox, mail mark, notify poll, notify ack, and publish status.
- [x] Keep adapter output as stable JSON so Swift, agents, and tests can call
  the same surface. Evidence: closed-loop proof parsed CLI JSON with Node.
- [x] Document the bridge policy: Swift uses the core for platform-hosted wiki
  behavior; memory uses the core for wiki semantics; neither owns a parallel
  wiki implementation. Evidence: `docs/wiki-system-architecture.md` and
  `docs/wiki-publishing-system-api.md`.

### 4. Closed-Loop Proof

- [x] Unit tests cover inventory, page creation, ledger writes, agent directory,
  talk append, mail inbox, notification polling, mail mark, and publish status.
  Evidence: `cargo test --workspace`.
- [x] CLI proof runs against a disposable `runtime-test/portable-wiki-core-*`
  fixture copied from `runtime/1Context`. Evidence:
  `runtime-test/portable-wiki-core-smoke/1Context`.
- [x] The agent loop is proven end to end: create pages, register agent, append
  proposal, read inbox, poll notification, mark mail, inspect page status.
  Evidence: closed-loop proof output:
  `talk deliveries=1`, `notifications=1`, `inbox unread=1 -> 0`,
  `render routes=8 md=8`, and `topics state=rendered`.
- [x] Dogfood proof uses the Python memory adapter as an agent would. Evidence:
  disposable `/tmp/onecontext-agent-dogfood-v2` run listed pages, registered
  `agent_codex_019e3f723471`, appended a Topics proposal, delivered role and
  page mail, cleared notification poll after ack (`1 -> 0`), rendered 8 routes
  and 8 markdown twins, and ended with `publish_status.next_action=none`.
- [x] Python adapter tracks the current Rust agent/mail collaboration surface.
  Evidence: `uv run --with pytest --project memory-core pytest
  memory-core/tests/test_wiki_core_client.py` passed after dogfooding
  `agent-identify`, `whoami`, `agent-list`, `agent-status`, role shorthand talk
  delivery, `mail-claim`, `mail-mark-all`, timed snooze with
  `include_snoozed`, `agent-inbox`, and notification ack through
  `WikiCoreClient`.
- [x] Python module-level helpers cover the normal agent collaboration loop.
  Evidence: Chandrasekhar's `/tmp/1context-pattern-f-wikicoreclient-9h3clhg0`
  run found the class client worked but module-level helpers were missing for
  talk append, mail inbox/subscribe/mark, page watch/assign role, and list
  create/status. The adapter now exports those helper functions alongside the
  existing page, publish, agent, claim, and mark-all helpers.
- [x] Agent lifecycle proof covers `wiki.page.open` and `wiki.page.delete`.
  Evidence: disposable `/tmp/onecontext-agent-lifecycle-dogfood` run created a
  scratch page with complete talk surface (`evidence=10`), opened it with
  source/talk edit hashes, rendered it (`routes=10`), tombstoned it, observed
  `publish_after_delete next=publish required=true`, rendered again
  (`routes=8`), and confirmed `/scratch-delete` was absent from the route
  manifest.
- [x] Agent full-flow proof covers configured site placement and real
  `wiki.publish`. Evidence: disposable
  `/tmp/onecontext-agent-full-flow-dogfood` run created
  `/agent-created-page` with `family_group=agent-lab`, appended it to the
  primary navigation, opened it with `safe_to_edit=true`, published it, found
  the route in `.1context/route-manifest.json`, and verified the rendered brand
  menu contained `Agent Created Page`.
- [x] Talk/mail operations are separated from page-content publish triggers.
  Evidence: the same dogfood run appended a talk message and claimed the mail;
  `talk.render_required=false`, `mail_mark.render_required=false`,
  `after_talk_next_action=none`, and `after_mark_next_action=none`. A direct
  source edit then correctly returned `after_edit_next_action=publish`.
- [x] Visible dogfood covers create, edit, inbox, notification ack, archive,
  tombstone, route removal, and archived-mail durability. Evidence: served dev
  wiki at `http://127.0.0.1:64345/codex-field-notes` shows `Codex Field
  Notes`, `Dogfood Observations`, and `Dogfood Pass 193146`; browser confirmed
  deleted route `/delete-drill-193146` reports missing. CLI evidence showed
  `talk_render_required=false`, `after_mail_next_action=none`,
  `notifications_before=3 -> after=0`, `delete_next_action=wiki.publish`,
  `final_next_action=none`, and archived page mailbox remained visible with
  `include_archived` after tombstone.
- [x] Status semantics now distinguish clean tombstones from failures and expose
  page thread counts. Evidence: `page-status codex-field-notes` reports
  `state=rendered`, `mail.open_thread_count=1`, `validation.status=ok`;
  `page-status delete-drill-193146` reports `state=tombstoned`,
  `validation.status=ok`, and `next_action=none`.
- [x] Delete removes stale navigation links, not only route files. Evidence:
  `/tmp/onecontext-delete-nav-regression` created a visible
  `/visible-delete-nav-proof` page, proved it was in the menu before delete,
  tombstoned it, published, confirmed route removal, removed it from
  `navigation` and `primary_navigation`, set `enabled = false`, and scanned
  every rendered HTML file with `stale_nav_link_files_after_delete=[]`.
- [x] Notification IDs are unique per message/agent/recipient delivery.
  Evidence: `/tmp/onecontext-notification-id-regression` delivered one message
  to three addresses for the same live agent, observed
  `notifications=3`, `unique_ids=3`, cleared all with ack
  (`after_ack=0`), and confirmed mail-only work left
  `publish_status.next_action=none`.
- [x] Tombstoned pages reject new talk by default while allowing explicit
  archive maintenance. Evidence: `/tmp/onecontext-tombstone-talk-policy`
  tombstoned a page, confirmed default `talk_append` failed with an
  `--allow-tombstoned` repair hint, then appended with
  `allow_tombstoned=true` without triggering publish.
- [x] `wiki.page.open` exposes unified edit resources. Evidence:
  `/tmp/onecontext-page-create-atomic-regression` opened `resource-proof` and
  verified four resources with `surface`, `uri`, `path`, `absolute_path`,
  `sha256`, `safe_to_edit`, and `write_mode`; all `absolute_path` values were
  directly usable absolute paths.
- [x] `wiki.page.create` preflights templates before mutating the registry.
  Evidence: the same regression tried `template=pages/does-not-exist.md`,
  observed an error mentioning the missing template, and confirmed
  `wiki.toml` was unchanged, the page id was absent from config, and no source
  file was created.
- [x] `wiki.page.write_body` and `wiki.page.patch_body` give agents safe body
  edit ergonomics. Evidence: `/tmp/onecontext-page-body-edit-dogfood` wrote
  and patched `body-edit-proof`, preserved frontmatter, rendered the edited
  links, rejected stale source hashes, rejected missing patch text, and refused
  body edits after tombstone. Receipts returned
  `operation=wiki.page.write_body`, `operation=wiki.page.patch_body`, and
  `next_action=wiki.publish` only for changed content.
- [x] Dogfood agents can use `page-open` hashes directly for body writes.
  Evidence: `/tmp/onecontext-main-dogfood-060506` created linked pages,
  verified `page-open.hashes.source_sha256` matched the edit precondition and
  source resource hash, wrote and patched bodies, published routes/menu links,
  then tombstoned one page and confirmed route/menu removal after publish.
- [x] Failed publish is a failed CLI command while preserving JSON evidence.
  Evidence: `/tmp/onecontext-publish-exit-regression-061010` corrupted a page
  frontmatter status, ran publish, observed process `exit_code=2`,
  `stdout_status=failed`, empty stderr, and structured repair hints.
- [x] Publish reports stale internal links after page deletion. Evidence:
  `/tmp/onecontext-link-diagnostics-061751` created a source page linking to a
  target page, published with `link_diagnostics.status=ok`, tombstoned the
  target, republished successfully, and received
  `link_diagnostics.status=warning` with one `broken_internal_link` including
  `page_id=link-source`, `route=/link-source`, `markdown_path=link-source.md`,
  `target=/link-target`, and suggested actions
  `edit_source`, `replace_link`, `publish`.
- [x] `wiki.list` and `wiki.page.status` surface last-publish link warnings.
  Evidence: `/tmp/onecontext-link-status-065115` created a source page linking
  to a target page, tombstoned the target, published with
  `link_diagnostics.status=warning`, persisted
  `context-engine/runs/wiki-publish-receipt.json`, then confirmed both
  `wiki.list` and `page-status link-status-source` returned
  `links.status=warning`, `broken_internal_count=1`,
  `broken_internal_targets=["/link-status-target"]`, `validation.status=warning`,
  and `next_action=repair_links`. Follow-up proof on the same fixture confirmed
  `publish-status` now returns `render_required=false`,
  `next_action=repair_links`, and `link_health.pages_with_broken_links` with
  `link-status-source`.
- [x] Mail and notification commands reject invalid states and unknown queue
  targets. Evidence: `/tmp/onecontext-mail-validation-062205` proved
  `mail-mark --state banana`, `notify-ack notif_not_real`, `notify-poll
  agent_not_real`, and `notify-ack --state banana` all fail with repairable
  errors, while valid `mail-mark claimed` and `notify-ack delivered` still
  return `wiki.mail.mark` and `wiki.notify.ack` receipts.
- [x] Mail and agent commands reject malformed addresses. Evidence:
  `/tmp/onecontext-address-validation-065731` proved `agent-register --role
  "not a uri"`, `talk-append --to "not a uri"`, and `mail-inbox "not a uri"`
  all fail with an `invalid address` error, while valid
  `role://topics.curator` registration, talk append, and inbox delivery still
  work.
- [x] Agent directory state is rebuilt from append-only events and survives
  concurrent registration. Evidence:
  `/tmp/onecontext-agentdir-concurrency-063536` launched 12 parallel
  `agent-register` calls, parsed 12 valid JSONL events, found 12 active agents
  in `current.json`, then heartbeated and retired one agent and confirmed 14
  parseable events with 11 active agents.
- [x] Agent leases are enforced for notification delivery. Evidence:
  `/tmp/onecontext-agent-lease-064242` registered one active and one
  one-second agent, waited for expiry, observed `agents_summary.active_count=1`
  and `stale_count=1`, delivered role mail, confirmed the active agent received
  one notification, and confirmed polling the expired agent failed as unknown
  active.
- [x] Mail subscriptions are durable, filtered, and lease-bound. Evidence:
  `/tmp/onecontext-mail-subscribe-070959` registered a live agent, subscribed it
  to `list://wiki.reviewers` with `--kind review`, appended one `review` and one
  `proposal` message to the list, confirmed the list inbox held both messages,
  confirmed the agent received exactly one `review` notification, confirmed a
  one-second subscription disappeared from `mail-subscriptions` and did not
  notify after expiry, and confirmed an invalid relation was rejected.
  Follow-up evidence:
  `/tmp/onecontext-agent-inbox-kind-filter-e3Wn0v/evidence/summary.json` proved
  the durable list mailbox still exposed both `proposal` and `review`, while the
  review-scoped subscriber's `agent-inbox`, notification poll, subscribe
  backfill, and agent status only surfaced `review` work.
- [x] Nested configured routes are render truth, not slug-only suggestions.
  Evidence: `/tmp/onecontext-route-truth-071847` created
  `topic-route-truth` at `/topics/route-truth` and `project-route-truth` at
  `/projects/route-truth`, published with route overrides from `wiki.toml`,
  confirmed route-manifest entries for both configured routes and
  `/topics/route-truth/talk`, confirmed old slug-only routes were absent,
  confirmed menu/body links used the configured nested paths, and confirmed
  post-render link diagnostics were `ok` with zero issues.
- [x] CLI failures return structured JSON error envelopes. Evidence:
  `/tmp/onecontext-json-errors-071907` proved invalid address, missing argument,
  and unknown page failures all exited `1` with stdout JSON containing
  `status=error`, the correct `wiki.*` operation, an error code, and repair
  hints, with stderr empty.
- [x] Page/list/agent inbox metadata is visible from normal consumer calls.
  Evidence: `/tmp/onecontext-agent-inbox-072615` registered an agent with
  direct `role://topics.curator` mail, subscribed it to
  `mailbox://page/topics` and `list://wiki.reviewers`, appended direct role,
  page watcher, and list member talk messages, then confirmed `wiki.list`,
  `page-status topics`, and `page-open topics` all returned
  `mail.watcher_count=1` and the page subscription, confirmed
  `mail-subscriptions --address list://wiki.reviewers` returned the list
  roster, confirmed `agent-inbox` returned all three messages and three pending
  notifications, and confirmed archived mail is hidden unless
  `--include-archived` is passed.
- [x] Agent inbox subscriptions are liveness-enriched like page/list rosters.
  Evidence: Averroes's `/tmp/onecontext-pattern-b.EtOAPI` run found
  `agent-inbox.subscriptions` used raw records without the
  `agent_liveness` enrichment already present in `page-status` and
  `list-status`. Regression fixture
  `/tmp/onecontext-agent-inbox-subscription-liveness-3nrBCQ` now proves
  `agent-inbox` returns subscription summaries with `agent_liveness=active`,
  `agent_lease_expires_at`, `agent_retired_at`, and `agent_retire_reason`
  while preserving message routing through `list://wiki.reviewers`.
- [x] Agent inbox has a thread-first workbench view for multi-surface delivery.
  Evidence: Hubble's `/tmp/onecontext-pattern-d-20260519T180832Z` run found one
  talk entry delivered through role, page mailbox, and watcher list produced
  three inbox rows and three notifications for the same active agent. Regression
  fixture `/tmp/onecontext-agent-inbox-thread-proof-rRCOUi` now proves
  `agent-inbox.summary.thread_count=1`,
  `actionable_thread_count=1`, `notification_thread_count=1`, while preserving
  the three raw per-recipient delivery rows under `messages`.
- [x] List rosters and inbox page context match agent intuition. Evidence:
  `/tmp/onecontext-list-members-072841` proved `list-members
  list://topics.watchers` returns the two active list subscriptions, that both
  subscribed agents receive the list message through `agent-inbox`, and that
  `page-status topics` relates `list://topics.watchers` back to the page.
  `/tmp/onecontext-agent-inbox-pages-072930` proved `agent-inbox` embeds a
  compact referenced-page summary with unread/message counts, page state, and
  `next_action=publish`.
- [x] Lists have first-class metadata, not only address conventions. Evidence:
  `/tmp/onecontext-list-objects-074355` created `list://topics.watchers` with
  title, description, owner, and `page_id=topics`, subscribed two agents,
  confirmed `lists --page topics` returned the list with `member_count=2`,
  confirmed `list-members list://topics.watchers` returned metadata plus both
  subscriptions, confirmed both subscribers received the list mail through
  `agent-inbox`, confirmed `page-status topics` related the list subscriptions
  back to the page, patched the Topics page to mention the list, published, and
  confirmed the `/topics` route rendered with zero link diagnostics.
- [x] List lifecycle and role/page participant semantics are stricter and more
  inspectable. Evidence: `/tmp/onecontext-list-role-strict-074947` proved
  subscribing to an uncreated `list://missing/not-created` fails with
  `unknown_list`, extra args to `list` and `list-members` fail with
  `unexpected_arguments`, duplicate `list-create list://topics.watchers`
  returns `already_exists` without overwriting the original title,
  `page-status topics` relates both `role://topics.curator` and
  `list://topics.watchers` to the page, `agent-inbox` separates
  `owned_addresses`, `subscribed_addresses`, and `effective_mailboxes`, and
  `list-status list://topics.watchers` returns list metadata, mailbox counts,
  members, and recent messages in one envelope.
  Heartbeat replay evidence: `/tmp/onecontext-heartbeat-list-FH8kZf` started
  from raw `runtime/1Context`, proved a talk append refuses missing talk files
  until `page-create-all`/page lifecycle runs, then created
  `list://topics.watchers`, rejected a phantom list with `unknown_list`,
  preserved duplicate list metadata with `already_exists`, delivered one review
  message to the subscribed agent, and showed the same list relation in
  `mail-subscriptions`, `agent-inbox`, `page-status topics`, and `list-status`.
- [x] New topic/project pages publish and navigate in a browser-visible loop.
  Evidence: `/tmp/1ctx-wiki-dogfood-ndng62` created
  `/projects/dogfood-browser-publish` and `/topics/wiki-publish-links`, wrote
  project/topic cross-links, published with `link_diagnostics.status=ok` and
  `issue_count=0`, served the rendered site on port `54638`, verified six HTML
  routes and markdown twins returned 200, clicked projects -> project -> topic
  with Playwright, and captured screenshots under the fixture artifacts.
- [x] Page participation no longer requires agents to memorize role/list
  addresses. Evidence: `/tmp/onecontext-page-participants-cdLd9E` proved
  `page-status topics` exposes `mail.page_mailbox=mailbox://page/topics`,
  `mail.curator_address=role://topics.curator`, and
  `mail.default_watchers_list=list://topics.watchers`; `page-assign-role
  topics --role curator` subscribed the agent as an assignee,
  `page-watch topics` created `list://topics.watchers`, `talk-append
  --to-role curator` delivered to `role://topics.curator`, `mail-subscribe`
  reported `backfill.surfaced_message_count=1` for an existing page-mailbox
  thread, and `agent-inbox` showed three messages with two live notifications.
- [x] Publish receipts report fresh link health in the same receipt that found
  stale links. Evidence: Arendt's `/tmp/onecontext-wiki-dogfood.IWBFjn`
  browser pass found a lag where tombstone publish `link_diagnostics` warned
  but embedded `after.link_health` stayed `ok` until a fresh `publish-status`.
  Regression fixture `/tmp/onecontext-publish-receipt-links-uwNs28` now proves
  the first post-delete publish receipt returns
  `link_diagnostics.status=warning`, `issue_count=1`,
  `after.link_health.status=warning`,
  `after.link_health.broken_internal_count=1`, and
  `next_action=repair_links`, matching immediate `publish-status`.
- [x] Shared mail work can be claimed without duplicate agent effort.
  Evidence: `/tmp/onecontext-mail-claim-IGTk06` registered two agents,
  subscribed both to `mailbox://page/topics`, appended one proposal, claimed it
  with agent A via `mail-claim`, observed repeat claim by agent A returned
  `already_claimed`, observed agent B exited with typed
  `mail_already_claimed`, and confirmed both agents' inbox views show
  `claimed_by=agent_codex_c4d370b94af2db2e` with unread counts cleared.
- [x] Page watch now includes direct page-mailbox work and page status counts
  role/list/page mail as page work. Evidence:
  `/tmp/onecontext-page-participants-status-938wZA` proved `page-watch` now
  reports `list.member_count=1` in its own receipt and `page-status topics`
  counts role/list mail with `mail.message_count=2`, `unread_count=2`, and
  `open_thread_count=2`. `/tmp/onecontext-page-watch-mailbox-sd6gq8` proved
  `page-watch topics` subscribes the agent to both `list://topics.watchers`
  and `mailbox://page/topics`, direct page-mailbox talk reaches
  `agent-inbox` with one notification, and `mail.watcher_count=1` avoids
  double-counting the same agent.
- [x] Link repair receipts close back to clean immediately. Evidence:
  Anscombe's `/tmp/onecontext-publish-link-repair-VXgxEI/1Context` run found
  post-repair publish receipts could show `link_diagnostics.status=ok` while
  `next_action=repair_links`. Regression fixture
  `/tmp/onecontext-link-repair-receipt-B54C5W` now proves delete publish
  returns `next_action=repair_links`, the repair publish returns
  `link_diagnostics.status=ok`, `next_action=none`, and
  `after.next_action=none`, matching fresh `publish-status`.
- [x] Delete receipts and post-publish repair receipts share one link-repair
  lifecycle shape. Evidence: `/tmp/onecontext-delete-link-lifecycle-src-JMTqLR`
  created `lifecycle-source -> lifecycle-target`, published clean, tombstoned
  the target, then proved delete
  `link_repair_lifecycle.next_action=publish_then_repair_links` with
  `pre_delete_preview -> post_publish_expected_repair`, post-delete publish
  `timeline=["post_publish_link_check"]`, `repair_tasks[0].page_id=lifecycle-source`,
  and `publish-status.next_action=repair_links`.
- [x] Broken internal links are reader-visible and relative links are checked.
  Evidence: Peirce's `/tmp/1ctx-rendered-reader-links-flXQkP` run proved a
  normal served route can now show a broken-link banner, then
  `/tmp/onecontext-reader-link-warning-5NCjcO` proved both canonical HTML and
  route-index HTML are annotated. Regression fixture
  `/tmp/onecontext-relative-link-warning-FrBcUP` proved relative href
  `./relative-target` is diagnosed as target `/relative-target`, writes
  `.1context/link-diagnostics.json`, annotates both route-index and canonical
  HTML with one warning and one broken-link marker, produces no duplicate
  `opctx-broken-link` class, and renders the singular warning sentence.
- [x] Done/archive lifecycle clears page and notification pressure. Evidence:
  Halley's `/tmp/onecontext-mail-lifecycle-mx86gB` run found `done` left
  `page-status.mail.open_thread_count=1` and archive left a stale notification
  in `agent-inbox`. Regression fixture
  `/tmp/onecontext-mail-lifecycle-terminal-cdVQvt` now proves claimed mail has
  `page_open_threads=1` and one notification, `done` changes
  `page_open_threads=0` with `agent-inbox.notification_count=0`, and `archived`
  hides the message from default inbox while `--include-archived` still shows
  it for audit.
- [x] Utility navigation renders, and tombstone page status distinguishes
  "needs publish to remove old route" from "cleanly retired". Evidence:
  `/tmp/onecontext-nav-status-semantic-regression` created a utility page and
  confirmed it appeared in the rendered menu; then tombstoned a visible page
  and observed `page.status.next_action=publish`,
  `validation.status=warning`, and `allowed_actions=["validate","publish"]`
  while the old route was still rendered. After publish, the route was gone,
  `page.status.next_action=none`, and `validation.status=ok`.
- [x] Publish status is page-scoped and honest about stale link evidence.
  Evidence: Beauvoir's `/tmp/onecontext-publish-status-dogfood-L3fBuX` probe
  edited only `topics` and observed `pages_needing_publish=["topics"]` while
  untouched `projects` stayed `state=rendered` and
  `dirty_since_publish=false`. Regression fixture
  `/tmp/onecontext-status-freshness-wFWCuL` now proves one body edit returns
  `site_needs_publish=true`, `pages_needing_publish=["topics"]`,
  `next_action=publish`, and `link_health.fresh=false` while untouched
  `projects` stays `dirty_since_publish=false`; after publish, link health
  returns `fresh=true`.
- [x] Page creation placement honors `nav_order` in the rendered menu.
  Evidence: Confucius' `/tmp/1ctx-wiki-placement-Awqjc7` probe found
  `nav_order` was accepted but ignored by the renderer. Regression fixture
  `/tmp/onecontext-status-freshness-wFWCuL` created
  `/projects/dogfood-before-projects` with `nav_order=12` in the Work group;
  rendered menu labels were `For You`, `Your Context`,
  `Dogfood Before Projects`, `Projects`, `Topics`, and Browser verification at
  `http://127.0.0.1:62964/projects/dogfood-before-projects/` confirmed H1
  `Dogfood Before Projects`, no broken-link banner, and
  `dogfoodBeforeProjects=true`.
- [x] Utility navigation no longer sorts ahead of primary navigation. Evidence:
  Banach's `/tmp/1ctx-pattern-e-nav-CPy61P` run found utility pages with low
  `nav_order` could jump before primary groups, and `nav_section` was not
  persisted on `[[pages]]`. Regression fixture
  `/tmp/onecontext-nav-section-proof-fb08Io` now proves a utility page with
  `nav_order=1` renders after primary groups (`For You`, `Context`, `Work`,
  `Reference`, `Utility`) and `wiki.toml` persists `nav_section = "utility"`.
- [x] Navigation placement is visible to consumers, not only the renderer.
  Evidence: Leibniz's `/tmp/onecontext-probe-c-20260519T182931Z` run confirmed
  rendered primary/utility ordering and found `wiki list` did not expose
  `nav_section`. Regression fixture `/tmp/onecontext-list-nav-owner-proof-XBKr2L`
  now proves `wiki list` returns `nav_section=utility`, `list-create --owner`
  accepts an active `agent_id` and stores the primary `agent://...` address,
  and `page-assign-role --role role://curator` normalizes to the page-local
  `role://nav-owner.curator`.
- [x] Module-level Python helpers cover the normal agent loop without dropping
  to the class client. Evidence: Hegel's
  `/tmp/onecontext-probe-b-module-helpers-full-20260519T113120-75078` run found
  missing module helpers for ensure, registration, subscriptions, list members,
  and notifications. The Python adapter now exports those helpers, and
  `memory-core/tests/test_wiki_core_client.py` proves page create/status,
  agent identify, role assignment, list create, subscription, list members,
  notifications, talk append, `agent-inbox`, and `mail-mark-all` through the
  module-level surface.
- [x] Python adapter dogfood covers the full agent lifecycle and content-scoped
  publish pressure. Evidence:
  `test-results/wiki-core-worker-as-20260520T073122Z/` proves register,
  nested-route create/open, body write/patch, publish, talk/mail/inbox/notify,
  mail-only no-publish, tombstone/remove, restore, and typed patch errors.
- [x] Agent inbox thread and page pressure stay honest after mail state
  transitions. Evidence: Faraday's `/tmp/onecontext-thread-lifecycle-A-NUnAKv`
  run proved a four-recipient thread updates correctly through claim, read,
  snooze, done, and `mail-mark-all`, ending with
  `actionable_count=0`, `actionable_thread_count=0`, and
  `notification_count=0`. Local helper proof
  `/tmp/onecontext-module-helper-dogfood-4o2ndtf6` then found
  `pages_requiring_action=1` after all deliveries were `done`; the core now
  derives `pages_requiring_action` from open mail pressure, and the Python
  regression asserts it returns `0`.
- [x] Generated site pages render as real routes. Evidence:
  `/tmp/onecontext-home-render-proof-OC2lvE` published from raw
  `runtime/1Context` and proved `site_input_count=1`, root `index.html` and
  `index.md` exist, the route manifest contains `/` with
  `route_index_path=index.html`, and `/for-you` remains present. This closes
  Leibniz's static-server root directory-listing defect for enabled
  `[[site_pages]].home`.
- [x] Generated site pages are navigable without broken default links. Evidence:
  Mendel's `/tmp/onecontext-site-pages-probe-A-v91aqt` run found generated
  `[[site_pages]]` could render as routes while staying out of configured
  navigation, and also exposed a disabled `/this-week` link from
  `/open-questions`. The runtime now enables `this-week` and `open-questions`
  by default, and regression fixture
  `/tmp/onecontext-site-pages-nav-talk-proof-yQtUYE` proves
  `site_input_count=3`, routes `/`, `/this-week`, `/open-questions`, and the
  four talk routes are present, duplicate route count is empty,
  `link_diagnostics.status=ok`, `issue_count=0`, and the home page has no
  broken-link marker.
- [x] Direct agent mail uses explicit primary addresses, not agent ids.
  Evidence: Maxwell's `/tmp/onecontext-probe-b-address-notify-qp6xl7` run
  found `agent://<agent_id>` was accepted but created an orphan mailbox
  invisible to `agent-inbox` and `notify-poll`. Regression fixture
  `/tmp/onecontext-agent-address-proof-HccGft` now proves
  `agent-identify.primary_address=agent://codex/address-proof-thread`,
  `addresses[0]` matches it, direct mail to that address notifies, and
  `agent://agent_codex_...` fails with `error.code=invalid_agent_address` plus
  a repair hint to use `agent_id` for control commands and `primary_address`
  for mail.
- [x] Notification wakeups carry enough triage context for pushed agents.
  Evidence: Descartes' `/tmp/onecontext-probe-b-notify-address-20260519T185828Z-2842`
  run proved direct, role, and list wakeups expose `agent_address`,
  `delivery_recipient`, and `mailbox`; follow-up fixture
  `/tmp/onecontext-notify-context-proof-k0saw0` proves `notify-poll` also
  returns `route=/notify-context`, `subject=Notify context proof`, and an
  `excerpt` preview; attachment fixture
  `/tmp/onecontext-talk-attachment-proof-fBR1Zq` proves `attachment_count=1`
  reaches `notify-poll`, so a pushed agent can decide whether to open the full
  inbox without first loading every mailbox.
- [x] Talk attachments are copied into talk folders and surfaced in inboxes.
  Evidence: fixture `/tmp/onecontext-talk-attachment-proof-fBR1Zq` used
  `talk-append --attachment dropdown-proof.txt`; the core copied the file under
  `attachment-proof.talk/attachments/<message-id>/`, the talk markdown includes
  a relative attachment link, `agent-inbox.messages[0].attachments[0]` matches
  the copied path, and no absolute fixture path leaked into talk/inbox JSON.
- [x] Talk attachment edge cases are handled without orphan state. Evidence:
  Ampere's `/tmp/onecontext-probe-c-talk-attachments.RkKDpY` run proved
  text/json/PNG-extension attachments get copied with expected media types,
  duplicate filenames become `duplicate.txt` and `duplicate-2.txt`, inbox
  attachment records match copied files, `notify-poll.attachment_count=6`, and
  missing or unsafe attachment inputs fail with `invalid_attachment` without
  leaving orphan `attachments/<message-id>/` directories.
- [x] Agents can hydrate one mail message or thread without scraping talk
  folders. Evidence: Curie's
  `/tmp/onecontext-probe-a-notify-thread-TcLpObr5CM` run showed the old path
  required `notify-poll`, full `agent-inbox`, and client filtering to claim one
  target thread. Regression fixture `/tmp/onecontext-mail-read-proof-RinZJE`
  now proves `mail-read --message-id`, `mail-read --thread-id`, and
  `wiki-talk-thread --thread-id` return `operation=wiki.mail.read`,
  `message_count=1`, `delivery_count=2`, hydrated `body_markdown`, delivery
  states, safe `source_path`, and attachment records without leaking absolute
  fixture paths.
- [x] Page creation refuses tombstoned/disabled ids until restore exists.
  Evidence: Newton's `/tmp/onecontext-probe-c-relations-20260519T184534Z` run
  found `page-create` after tombstone exited zero with a misleading receipt
  while the page stayed tombstoned and disabled. Regression fixture
  `/tmp/onecontext-tombstone-recreate-proof-9lDGxw` now proves recreating the
  same id exits `1` with `error.code=tombstoned_page`, keeps
  `page_state=tombstoned`, keeps `enabled=false`, and returns a repair hint to
  use a new id or future explicit restore operation.
- [x] CLI aliases and receipt evidence are agent-friendly. Evidence:
  `/tmp/onecontext-receipt-alias-oj0c63` proved `wiki-page-create-all`,
  `wiki-publish`, `wiki-page-create`, `wiki-page-delete`, and `wiki-list`
  all resolve to the expected operations; create receipt evidence had
  `count=10` with `absolute=false`, delete evidence had `count=2` with
  `absolute=false`, and samples used root-relative paths such as
  `user-wiki/source/families/primary/receipt-proof/source/receipt-proof.md`
  and `user-wiki/wiki.toml`.
- [x] Delete freshness is page-scoped while still requiring a site publish.
  Evidence: `/tmp/onecontext-delete-freshness-Iucr8s` linked `for-you` to
  `/projects`, tombstoned `projects`, and observed pre-publish
  `site_needs_publish=true`, `pages_needing_publish=["projects"]`,
  `link_health.fresh=false`; `for-you` stayed `state=rendered` and
  `dirty_since_publish=false`. After publish, route removal completed and
  `link_health.fresh=true`, `status=warning`, `next_action=repair_links`.
- [x] First-run publish status reports missing site output as site pressure.
  Evidence: Lorentz's `/tmp/1context-publish-status-honesty-20260519-090042`
  probe found the raw pre-publish fixture could show page publish pressure
  while `site_needs_publish=false`. Regression fixture
  `/tmp/onecontext-initial-site-needed-u0KHUm` now reports before publish
  `render_required=true`, `site_needs_publish=true`, all four configured pages
  in `pages_needing_publish`, `next_action=publish`, and
  `link_health.fresh=false`; after publish it reports `site_needs_publish=false`
  and `link_health.fresh=true`.
- [x] Tombstoned talk refusal is typed and repairable. Evidence: Volta's
  `/tmp/onecontext-talk-mail-lifecycle-20260519T160505Z-NnRbIY` probe proved
  mail/talk/notification churn left `publish_status.after_mail.render_required=false`
  and tombstone delete returned `pages_needing_publish=["topics"]`, but the
  default tombstoned talk refusal used generic `command_failed`. Regression
  fixture `/tmp/onecontext-tombstone-error-code-84J62U` now returns
  `operation=wiki.talk.append`, `error.code=tombstoned_page`, and a repair hint
  explaining `--allow-tombstoned` is only for archive-maintenance talk.
- [x] Browser-visible delete proof covers custom project/topic pages, route
  removal, stale-link banners, and menu cleanup. Evidence: Linnaeus'
  `/tmp/1context-wiki-dogfood-20260519T160325Z` run created project/topic/custom
  dogfood pages with `nav_order`, published cleanly with `route_count=14`, then
  tombstoned `/dogfood/topic-beta`; republish returned
  `link_diagnostics.issue_count=2`, `next_action=repair_links`, route manifest
  no longer included `/dogfood/topic-beta`, and screenshots proved the broken
  link banner plus no `Dogfood Topic Beta` menu link. Local in-app browser
  verification on `/tmp/onecontext-delete-freshness-Iucr8s` also confirmed
  `/projects/` returned 404, `for-you` had one broken-link warning, and the
  menu labels excluded `Projects`.
- [x] Multi-recipient talk messages can be resolved without chasing each
  delivery manually. Evidence: `/tmp/onecontext-mail-mark-all-5Vr2ak`
  delivered one Topics talk message to `list://topics.watchers`,
  `mailbox://page/topics`, and `role://topics.curator`. Marking only the role
  delivery `done` left `page_status.mail.open_thread_count=1` and
  `unread_count=2`. New `mail-mark-all <message-id> --state done` returned
  `operation=wiki.mail.mark_all`, updated three root-relative mailbox evidence
  paths, left `render_required=false`, and reduced page mail to
  `open_thread_count=0`, `unread_count=0`; both agents' inbox summaries ended
  with `unread_count=0`, `notification_count=0`, and publish status remained
  `next_action=none`.
- [x] API-shaped aliases and common edit/list errors are typed enough for
  agents to recover. Evidence: Carson's `/tmp/onecontext-wiki-ergonomics.twz7a4`
  probe proved `wiki-list`, `wiki-page-status`, `wiki-page-open`,
  `wiki-page-create-all`, `wiki-page-write-body`, and `wiki-mail-subscribe`
  work. It also found stale source hashes returned generic `command_failed`,
  patch misses returned a less-specific code, and missing `list-status`
  returned only `list=null`. Regression fixture
  `/tmp/onecontext-ergonomic-errors-3XJXzx` now proves stale writes return
  `error.code=source_hash_mismatch` with a `wiki.page.open` retry hint, missing
  patch text returns `error.code=body_patch_not_found`, and missing list status
  returns `status=missing`, `exists=false`, `next_action=list_create`.
- [x] Link repair loop is browser-visible and closes cleanly after hashed
  source patches. Evidence: Franklin's `/tmp/onecontext-link-dogfood.ALX7XU`
  created disposable project/topic/custom pages, published cleanly, tombstoned
  `dogfood-topic-links`, and received two `broken_internal_link` diagnostics
  pointing at `dogfood-project-alpha` and `dogfood-custom-lab`. It repaired
  both with `page-patch-body` plus `page-open` source hashes, republished, and
  confirmed `render_required=false`, `link_health.status=ok`,
  `broken_internal_count=0`, `next_action=none`, no
  `opctx-link-warning`/`opctx-broken-link` matches in the affected rendered
  pages, and screenshots before/after the repair.
- [x] RuntimeDefaults/dev app proof uses the packaged Rust core and rendered
  site. Evidence: `ONECONTEXT_RELEASE_CHANNEL=dev bash
  scripts/build-macos-app.sh` completed in `real 72.81`; packaged
  `onecontext-wiki --root dist/1Context.app/.../RuntimeDefaults/1Context
  publish-status` returned `render_required=false`; browser verification at
  `http://127.0.0.1:64345/for-you` confirmed H1s `For You`, `Topics`,
  `Talk - Topics`, `Projects`, dynamic menu entries for all four pages, and
  `topicsIsNotOperator=true`.
- [x] Swift bridge dogfood consumes the Rust core for lifecycle/status without
  duplicating wiki semantics. Evidence:
  `test-results/wiki-core-worker-ar-20260520T073305Z` ran an opt-in
  `WikiCoreProcessClient` pass against disposable user data: create-all,
  publish with trigger `swift-bridge.initial`, talk-only mail pressure with
  `publish-status.render_required=false`, page-create
  `swift-bridge-proof`, publish with trigger `swift-bridge.lifecycle`, and
  final `publish-status.next_action=none`.
- [x] Agent-facing mail counters separate durable history from live work.
  Evidence: `/tmp/onecontext-counter-dogfood-5J6Qgq` delivered one Topics talk
  message to `list://topics.watchers`, `mailbox://page/topics`, and
  `role://topics.curator`; page status now showed
  `open_delivery_count=3`, `open_thread_count=1`, then after one recipient was
  marked `done`, `open_delivery_count=2`, `open_thread_count=1`, and after
  `mail-mark-all`, both open counts were `0`. Regression fixture
  `/tmp/onecontext-inbox-open-mail-6bX8fW` proved `agent-inbox.summary` now
  exposes `actionable_count` and `pages_with_open_mail_count`: default inbox
  had `message_count=2`, `actionable_count=1`,
  `pages_with_open_mail_count=1`; after the final open delivery was marked
  `done`, durable `message_count=3` remained while `actionable_count=0`,
  `notification_count=0`, and `pages_with_open_mail_count=0`. A helper probe at
  `/tmp/onecontext-mail-counters.4kZJXW` independently confirmed the same page
  mail counters through role/list/page delivery fanout.
- [x] Mark-all receipts and raw notification polling now agree with delivery
  actionability. Evidence: helper probe
  `/tmp/onecontext-dogfood-probe-a.gFO60X` found `mail-mark-all` reported
  `changed_delivery_count=4` even though one delivery was already `done`, and
  raw `notify-poll` still returned stale wakeups after all deliveries were
  terminal. Regression fixture `/tmp/onecontext-markall-notify-2c0UCp` now
  proves one pre-closed page delivery plus two open deliveries returns
  `changed_delivery_count=2`, `before.open_delivery_count=2`,
  `after.open_delivery_count=0`, evidence statuses
  `["updated","updated","unchanged"]`, and raw notification poll counts fall
  from `alpha=1,beta=2` to `alpha=0,beta=0` after `mail-mark-all` without
  explicit notification ack.
- [x] Page create now preflights template renderability before registry
  mutation. Evidence: Euler's
  `/tmp/onecontext-route-link-repair-20260519-hutr5C` run found a created
  custom project/topic fixture failed publish until disposable frontmatter was
  manually patched with renderer-required `section` and `access`. Regression
  fixture `/tmp/onecontext-page-create-preflight-yqS3NP` now proves a valid
  custom-family template renders `section=context`, `access=private`, publishes
  with `issue_count=0`, and routes `/custom/deep/valid`; invalid templates
  missing `access` or using bad `section` exit before writing `wiki.toml`.
  `/tmp/onecontext-template-error-code-4Rk9qR` proves the failure is typed as
  `error.code=invalid_page_template` with repair hints and
  `config_entries=0`. Gauss's `/tmp/1ctx-dogfood-probe-A` run found that
  preconfigured `[[pages]]` entries could bypass new-page validation; regression
  fixture `/tmp/onecontext-configured-template-preflight-ulCUee` now proves a
  bad configured template returns `invalid_page_template` before creating the
  source file.
- [x] Rendered route manifests point to link diagnostics. Evidence:
  `/tmp/onecontext-route-manifest-linkdiag-uZvTDy` published a source/target
  pair cleanly, tombstoned the target, and republished with one broken
  `/dogfood/target` link. The rendered
  `.1context/route-manifest.json` now includes
  `link_diagnostics.path=".1context/link-diagnostics.json"`,
  `status=warning`, `issue_count=1`, and health
  `pages_with_broken_links=["dogfood-source"]`, while the sibling diagnostics
  file carries the detailed target list.
- [x] Page/list subscription rosters are liveness-aware. Evidence:
  Heisenberg's `/tmp/1ctx-dogfood-probe-b.bYLmqW` run found retired and stale
  agents still looked like ordinary active members in `list-members`,
  `list-status`, and `page-status`. Regression fixture
  `/tmp/onecontext-subscription-liveness-8W7c3N` now proves list and page mail
  surfaces retain durable roster totals while exposing active/inactive counts:
  after one stale agent, `active_member_count=2`,
  `inactive_member_count=1`; after retiring another agent,
  `active_member_count=1`, `inactive_member_count=2`, and subscriptions are
  labeled `active`, `stale`, and `retired` with retirement metadata.
- [x] Duplicate mail subscriptions renew instead of adding roster rows.
  Evidence: Worker B's
  `/tmp/onecontext-worker-b-subscription-dogfood-HjowtV/evidence/summary.json`
  proved repeat `mail-subscribe` returned `status=renewed`, kept the same
  subscription id, left `subscription_count_after_duplicate=1`, pointed
  backfilled agents at `next_action=agent_inbox`, returned
  `next_action=mail_subscriptions` when unsubscribe left related subscriptions,
  and preserved active/stale/retired list liveness counts.
- [x] Page mail pressure includes explicitly page-associated lists. Evidence:
  Averroes's `/tmp/onecontext-pattern-b.EtOAPI` run found
  `list://wiki.reviewers` had `page_id=topics` and page messages, but
  `page-status topics` only counted conventional page mailbox/role deliveries.
  Regression fixture `/tmp/onecontext-page-associated-list-proof-wk7egU` now
  proves `page-status topics` exposes
  `mail.associated_lists=[list://wiki.reviewers]`, counts that list delivery in
  `message_count=1`, `open_delivery_count=1`, `unread_count=1`, and includes
  the enriched list subscription with `agent_liveness=active`.
- [x] Agent discovery is a first-class read surface. Evidence: Nietzsche's
  `/tmp/1ctx-dogfood-probe-a.taecMS` run confirmed `agent-list`,
  `agent-status`, and `agent-whoami` style commands were missing. Local
  regression fixture `/tmp/onecontext-agent-discovery-mJk55G` now proves
  `whoami --thread-id`, `whoami --agent-id`, `agent-list --include-stale
  --include-retired`, and `agent-status` distinguish active, stale, and retired
  agents. Follow-up fixture `/tmp/onecontext-retired-register-proof-P4oF5R`
  proved active agents with no mail return `next_action=none`, active agents
  with role mail return `check_inbox`, stale agents return `agent_identify`,
  retired agents return `agent_register_new_thread`, and `agent-register`
  refuses retired thread ids instead of resurrecting them.
- [x] Agent registration is create-only and does not narrow active identities.
  Evidence: Bernoulli's `/tmp/1ctx-agent-pattern-a-IoEgap` run found a second
  `agent-register` on an active thread stripped `role://topics.curator` and
  `wiki.mail`. Regression fixture
  `/tmp/onecontext-agent-register-create-only-0oyVdd` now proves active
  re-register returns `error.code=agent_already_registered` with a hint to use
  `agent-identify`, `agent-identify` preserves the existing curator role and
  merges `wiki.curator.apply`, retired re-register returns
  `error.code=retired_agent`, and heartbeat/retire on inactive agents now gives
  liveness-aware repair hints.
- [x] Snoozed mail is timed and suppresses default pressure until due. Evidence:
  Schrodinger's
  `/tmp/onecontext-dogfood-probe-b-20260519T170707Z` run showed bare
  `state=snoozed` cleared unread count but did not hide inbox, notification,
  or page pressure. Regression fixture `/tmp/onecontext-snooze-until-FKnkOo`
  now proves `mail-mark-all --state snoozed` without `--until` returns
  `invalid_snooze_until`; with a future `--until`, default agent inbox drops
  from `actionable_count=2` to `0`, `notify-poll` drops from `2` to `0`, and
  page open delivery/thread counts drop from `2/1` to `0/0`. The same fixture's
  `due-summary.json` proves the mail becomes actionable and pollable again
  after the due time.
- [x] Page lifecycle receipts use the same action token as page status. Evidence:
  Dalton's `/tmp/1context-dogfood-probe-c-20260519` run found tombstone delete
  receipts said `next_action="wiki.publish"` while page status said
  `next_action="publish"`. Page create, page body edit/patch, and page delete
  receipts now use `next_action="publish"` consistently.
- [x] Page delete previews inbound link breakage before publish. Evidence:
  Epicurus's
  `/tmp/onecontext-dogfood-probe-a-delete-inbound-20260519T172405Z` run
  verified the need for route-vs-markdown target detail. Regression fixture
  `/tmp/onecontext-delete-link-preview-sdEj20` now proves a clean pre-delete
  publish with `issue_count=0`, then `page-delete` returns
  `link_impact.status=warning`, `deleted_route`,
  `deleted_markdown_path`, `post_publish_expected_next_action=repair_links`,
  `inbound_link_count=3`, `source_page_count=2`, and per-issue
  `target_kind=route|markdown_twin` before publish. The post-delete publish
  then returns `next_action=repair_links` with three broken links, matching the
  preview.
- [x] Agent status now counts role-owned mailboxes. Evidence: Hypatia's
  `/tmp/1ctx-dogfood-probe-b.vbSc9Q` run found `whoami` / `agent-status`
  showed mailbox pressure as zero even when role-delivered mail existed. The
  core now treats all registered addresses, including `role://...`, as owned
  addresses for agent status, and the unit test asserts active agent status
  reports role mail `actionable_count=1`.

### 5. Integration And Cleanup

- [x] Decide whether Swift calls the Rust core by subprocess JSON first or FFI
  later; document the chosen V0 bridge. Evidence: V0 uses subprocess JSON via
  `onecontext-wiki`; FFI is deferred until the consumer API is stable.
- [x] Replace Swift-owned inventory/list/page-status implementation with the
  Rust adapter or mark it transitional behind one bridge. Evidence:
  `OneContextDaemon` calls `WikiCoreProcessClient` for `wiki.list` and
  `wiki.page.status`; dev daemon proof returned
  `daemon wiki.list: pages=4, core_mail_deliveries=1`.
- [x] Replace memory-side wiki placement/fallback/talk routing helpers with the
  Rust adapter or mark them transitional behind one bridge. Evidence:
  `memory-core/src/onectx/wiki_interface/core_client.py` exposes the same
  `onecontext-wiki` JSON surface to Python; legacy `authoring.py` is documented
  as transitional receipt/proposal support, not wiki semantics.
- [x] Delete repo-only wiki helper paths that duplicate the new core once the
  Rust slice proves equivalent behavior. Evidence:
  `wiki-engine/tools/materialize-wiki-pages.py` was removed; dev init,
  wiki browser smoke, app build, and RuntimeDefaults packaging now call
  `onecontext-wiki page-create` / `page-create-all`.
- [x] Expose whole-wiki health as first-class `wiki.status` and
  `wiki.validate` commands instead of forcing agents to compose list,
  page-status, publish-status, and publish receipts by hand. Evidence:
  `/tmp/onecontext-status-validate-proof-2dJn5a` started from raw
  `runtime/1Context`; initial `status` returned `state=blocked`,
  `next_action=page_create`, and validation returned 5 issues. After
  `page-create-all` and publish, `status` returned `state=idle`,
  `next_action=none`, `page_count=4`, `last_publish.status=published`,
  `last_publish.route_count=11`, and validation returned `status=ok`,
  `issue_count=0`, `can_publish=true`. Boyle's
  `runtime-test/probe-a-wiki-health-20260519T194909Z` also proved a broken
  internal link now surfaces as `status.state=attention`,
  `next_action=repair_links`, and a typed `page_has_broken_internal_links`
  validation issue.
- [x] Include actionable link diagnostics directly in `wiki.validate` repair
  issues. Evidence: `/tmp/onecontext-validate-diagnostics-proof-vxxcJ7`
  patched `topics` with `./missing-diagnostic-target`, published to
  `next_action=repair_links`, and verified `wiki.validate` returned
  `page_has_broken_internal_links` with `diagnostics[0].href`,
  `diagnostics[0].target`, `markdown_path=topics.md`,
  `source_path=topics.html`, and `route=/topics`; `wiki.status` returned
  `state=attention`, `next_action=repair_links`.
- [x] Keep list workbench receipts operation-shaped. Evidence: Fermat's
  `/tmp/onecontext-probe-b-talk-mail-20260519T200626Z-94BixA` run found
  `list-status` lacked a top-level operation token. The Rust result now emits
  `operation="wiki.list.status"`, and the Python adapter regression asserts
  that shape.
- [x] Make list roster receipts branchable without nested-shape knowledge.
  Evidence: `/tmp/onecontext-list-workbench-proof-mxdCaS` created
  `list://topics.reviewers`, subscribed two active agents, and verified
  `wiki.lists`, `wiki.list.members`, and `wiki.list.status` all return
  operation tokens; `list-members` and `list-status` return `exists=true`,
  top-level `member_count=2`, `active_member_count=2`, and
  `next_action=none`.
- [x] `wiki.list.status` exposes hidden snoozed/archived audit state and can
  include it on demand. Evidence:
  `/tmp/onecontext-list-status-audit-current` reproduced the complaint:
  default `list-status` hid one snoozed and one archived delivery with
  `message_count=0`, exposed no snooze/archive audit flags, and rejected
  `--include-archived --include-snoozed` as unexpected args. Regression fixture
  `/tmp/onecontext-list-status-audit-flags` now proves default
  `list-status list://topics.audit` reports `has_archived=true`,
  `has_snoozed=true`, `hidden_archived_count=1`,
  `hidden_snoozed_count=1`, and
  `audit_flags=["archived_hidden","snoozed_hidden"]`, while
  `wiki_list_status(..., include_archived=True, include_snoozed=True)` returns
  two messages with states `archived` and `snoozed` and clears both hidden
  counts.
- [x] Use canonical page ids in publish link diagnostics for nested routes.
  Evidence: Bacon's
  `/tmp/onecontext-probe-a-lifecycle-20260519T202403Z-STyJe9` run found that
  broken-link diagnostics on nested pages could identify rendered slugs rather
  than canonical page ids. The daemon now enriches link diagnostics from
  inventory; `/tmp/onecontext-canonical-link-pageid-proof-wMIKB4` verified a
  deleted `/canonical/hub/leaf` target reports `page_id=canonical-hub` in both
  publish diagnostics and `wiki.validate` diagnostics.
- [x] Make mail/inbox/notification receipts branchable for agents. Evidence:
  Mill's `/tmp/onecontext-probe-b-list-mail-after-20260519T202416Z-6QjLXj`
  run added and verified operation/next-action/top-level counts on
  `wiki.mail.subscriptions`, `wiki.mail.inbox`, `wiki.agent.inbox`, and
  `wiki.notify.poll`. The proof closed duplicate deliveries with
  `mail-mark-all` and ended with watcher `actionable_count=0`,
  `notification_count=0`, and `pages_requiring_action=0`.
- [x] Fix grown-reader nested talk and large-menu browser edges. Evidence:
  Socrates'
  `/tmp/onecontext-probe-c-reader-20260519T202119Z-QDVFB3` run published a
  29-route grown mini wiki, crawled 659 internal links, verified static-mode
  host-state backoff, verified reader-visible broken-link markers, and patched
  nested article/talk toggles plus desktop brand-menu scrolling. Final publish
  returned `next_action=none`, final validation was `ok`, and final broken
  link count was 0.
- [x] Accept page URI shorthand for page-mail recipients in talk append.
  Evidence: `/tmp/onecontext-page-uri-recipient-proof-Cv3QIN` sent
  `talk-append --to page://topics`; the receipt, `mail-inbox
  mailbox://page/topics`, and `mail-read` all reported the canonical delivery
  recipient `mailbox://page/topics`.
- [x] Sitemap/fallback ergonomics hold through nested custom page lifecycle.
  Evidence:
  `/var/folders/1b/r_tcp0rj2vd_7hn6j859tq1c0000gn/T/onecontext-sitemap-fallback-dogfood-xWkxsy`
  started from raw `runtime/1Context`; first publish preflight safe-created
  missing configured pages, invalid template creation failed before mutating
  `wiki.toml` with `error.code=invalid_page_template`, default-template page
  creation wrote source plus family-local page/talk templates, and nested
  custom pages at `/dogfood/sitemap/fallback`,
  `/dogfood/sitemap/tools/utility`, and
  `/dogfood/sitemap/hidden/private` exposed `wiki.list` / `page-status`
  `nav_section=primary|utility|hidden`. Delete publish removed all custom page
  and talk routes plus menu links; restore publish returned all routes, kept the
  hidden page out of the rendered brand menu, and ended with
  `publish-status.next_action=none`, `validate.status=ok`, and
  `route_count=17`.
- [x] Curator-talk workbench proof covers roles, lists, attachments, claims,
  terminal mail states, notification wakeups, and page-status pressure.
  Evidence: `/tmp/onecontext-curator-talk-dogfood-20260520T002706Z`
  registered curator/reviewer/watcher agents, assigned `topics` curator and
  reviewer roles, created `list://topics.reviewers`,
  `list://topics.watchers`, and `list://topics.concerns`, then appended
  question/proposal/reply/concern messages with copied attachments. The
  proposal/reply pair shared one subject-derived thread and `mail-read`
  hydrated it as 2 messages / 5+ deliveries. Triage claimed proposal and reply
  deliveries, marked proposal/reply done, snoozed the question until 2099, and
  archived the concern. Page mail pressure went `open_delivery_count=11` to
  `0`, notification wakeups went curator/reviewer/watcher `4/4/3` to `0`,
  default agent inboxes ended with `actionable_count=0` and
  `pages_requiring_action=0`, reviewer audit preserved
  `done/done/archived`, watcher audit preserved `snoozed`, and concern-list
  audit preserved `archived`. Follow-up evidence below closed the reply-targeting
  complaint by adding explicit `reply_to` and `thread_id` paths; untargeted
  replies still preserve the legacy subject-derived behavior.
- [x] Explicit talk reply targeting is dogfooded through the portable core and
  Python adapter. Evidence: disposable fixture
  `/tmp/onecontext-reply-threading-dogfood.SZqV0c` used
  `WikiCoreClient.talk_append(reply_to=...)` and
  `WikiCoreClient.talk_append(thread_id=...)` against
  `target/debug/onecontext-wiki`. The changed-subject `reply_to` message
  returned `thread_id=thread_topics_dogfood-parent-subject`,
  `reply_to=talkmsg_20260520T004429339992000Z_3102a2004f`, and
  `mail_read(thread_id=...)` hydrated `thread_message_count=3`; an untargeted
  changed-subject reply still returned the legacy subject-derived
  `legacy_thread_id=thread_topics_dogfood-legacy-changed-subject-reply`.
  On-disk proof:
  `topics.talk/2026-05-20T00-44-29-355144000Z.reply.dogfood-changed-subject-reply.md`
  contains both `thread: "thread_topics_dogfood-parent-subject"` and
  `reply_to: "talkmsg_20260520T004429339992000Z_3102a2004f"`. Focused checks:
  `cargo test --workspace` and
  `uv run --with pytest --project memory-core pytest
  memory-core/tests/test_wiki_core_client.py` passed. Follow-up below closes the
  file-only target-resolution gap; renderer/menu behavior was intentionally left
  untouched in this pass.
- [x] Old file-only talk entries without delivery records can be explicit reply
  and thread targets. Evidence: `/tmp/onecontext-file-only-reply-dogfood.HWHEME`
  copied raw `runtime/1Context`, page-created `topics`, inserted two talk files
  with no mailbox rows, then proved `talk-append --reply-to
  talkmsg_file_only_parent_cli` returned
  `thread_id=thread_topics_file_only_parent_cli` and `reply_to` preserved the
  parent id; `talk-append --thread-id thread_topics_file_only_direct_cli` also
  resolved from talk frontmatter. Focused Rust checks:
  `cargo test -p onecontext-wiki-core
  file_only_talk_messages_can_be_reply_and_thread_targets` and
  `cargo test -p onecontext-wiki-core` passed. Later file-only hydration
  evidence below closes the remaining `mail-read --thread-id` parent-context
  gap.

## Notes

- Current baseline: the Rust core/CLI slice now works as the first-class
  consumer surface for file-backed runtime data. Swift and memory call it by
  subprocess JSON.
- The renderer now reads `wiki.toml` navigation during whole-site renders, so
  `wiki.page.create` placement metadata affects the actual brand menu.
- Publish freshness now tracks page source, tombstones, and `wiki.toml`.
  Talk-message, mailbox, and notification churn stays in the inbox system
  unless a caller explicitly chooses to force a render.
- Previously open dogfood surfaces now covered: list objects, role-related page
  status, page watching, direct page-mailbox watcher delivery, shorthand role
  assignment, subscribe
  backfill summaries, atomic mail claim, publish receipt link-health repair
  closure, reader-visible broken-link warnings, relative link diagnostics, and
  terminal mail pressure are now inspectable. `agent-identify` now acts as the
  revived-session wakeup command; the Python adapter covers the current
  high-use Rust collaboration commands; and `page-open.edit` now reports
  explicit edit policy fields. Evidence:
  `/tmp/onecontext-agent-identify-edit-policy-rKVg4d` showed
  `agent-identify.status=registered`, top-level `addresses`,
  `recommended_operation=wiki.page.patch_body`,
  `recommended_write_mode=hash_checked_patch`, and operation-shaped
  `allowed_actions`.
- Latest browser/reader lifecycle evidence: Raman's
  `/tmp/1ctx-pattern-c-20260519T175310Z` run created linked
  project/topic/custom pages, published `route_count=14` with no link issues,
  tombstoned the linked topic, observed
  `page-delete.link_impact.inbound_link_count=2`, republished with
  `next_action=repair_links` and reader-visible broken-link markers, then
  repaired via hash-checked `page-patch-body` to final `link_issues=0` and no
  reader warning markers.
- Latest generated site-page and reader evidence:
  `/tmp/onecontext-default-reader-proof-znG3hU` and
  `/tmp/onecontext-browser-reader-proof-cSXtJW` both published raw
  `runtime/1Context` with `site_input_count=3`, `next_action=none`, and
  `link_diagnostics.status=ok`. The in-app browser loaded the freshly served
  fixture at `http://127.0.0.1:51715/`, saw Home links for `For You`,
  `This Week`, and `Open Questions`, loaded `/open-questions/` with
  `h1=Open Questions`, loaded `/topics/talk/` with `h1=Talk - Topics`, and
  confirmed `/api/wiki/state` returns OK under `serve-site.mjs`. Sagan's
  `/tmp/onecontext-static-reader-probe-A-YTUKmL` also proved plain static
  serving keeps reader routes working while `enhance.js` backs off after the
  optional host-state API is absent, avoiding repeated `/api/wiki/state` noise.
- Latest custom placement evidence: Locke's `/tmp/onecontext-probe-a-pP4e6V`
  run created primary, utility, and hidden custom pages, then published
  `route_count=19`, `markdown_twin_count=19`, `link_issue_count=0`, and
  `next_action=none`. HTTP proof showed every custom/talk route returned `200`,
  the menu sorted primary and utility pages by `nav_order`, and the hidden page
  rendered at `/probe-a-hidden` without appearing in the menu.
- Latest custom site-growth evidence: Huygens'
  `/tmp/1ctx-probe-b-custom-growth-4T1Z8r/runtime` run created linked project,
  topic, and style pages, published `route_count=17` with `link_issues=0`,
  verified six custom page/talk routes over HTTP, and used Playwright to click
  brand-menu navigation, a cross-link, and the style talk round trip with
  `failures=[]`. It also fixed a reader polish bug where brand menu labels and
  summaries were glued together without a separating space.
- Latest mail/list evidence: Laplace's
  `/tmp/1context-probe-b-20260519T192003Z` run exercised two agents, role
  assignment, direct mail, reviewer/watch lists, `page-watch`, claim/mark/ack,
  `agent-inbox`, `page-status`, and `notify-poll`. It confirmed direct mail
  stays out of page-associated mailbox counts, page/list/role mail reaches the
  expected agents, and notification ack is only wakeup handling: actionable mail
  can remain after a notification disappears.
- Latest tombstone repair evidence: Nash's
  `/tmp/1context-probe-c-20260519T191455Z` run created Alpha/Beta/Gamma
  cross-links to both `/probe-c-beta` and `/probe-c-beta.md`, published cleanly,
  tombstoned Beta, observed `inbound_link_count=4` and reader-visible broken
  link markers, then repaired Alpha/Gamma with hash-checked `page-patch-body`
  and republished to `next_action=none`, `link_status=ok`, and `0` issues.
- Latest focused mail-read evidence: `/tmp/onecontext-mail-read-proof-RinZJE`
  showed an agent can move from a notification or inbox thread to
  `mail-read --message-id` / `mail-read --thread-id` and get the exact talk
  body, deliveries, and attachments for that one item. `mail-thread` and
  `wiki-talk-thread` are accepted aliases for the same `wiki.mail.read`
  operation.
- Latest attachment-edge evidence: Ampere's
  `/tmp/onecontext-probe-c-talk-attachments.RkKDpY` run found and fixed the
  attachment prevalidation/duplicate-name edge cases. The current core tests
  include `talk_attachments_copy_media_and_duplicate_names` and
  `invalid_talk_attachments_do_not_leave_orphan_directories`.
- Latest status/validate evidence: `/tmp/onecontext-status-validate-proof-2dJn5a`
  proves the top-level agent question now has one-call answers before and
  after publish. `wiki.status` is intentionally tiny, while `wiki.validate`
  carries typed issue codes, severities, paths, and next actions. The Python
  adapter exports `wiki_status` and `wiki_validate` so memory-side consumers
  do not need to shell out manually.
- Latest validation-repair evidence:
  `/tmp/onecontext-validate-diagnostics-proof-vxxcJ7` proves broken-link
  validation issues now carry the underlying link-diagnostic objects, removing
  the previous need to cross-read the publish receipt just to find `href`,
  rendered route, markdown twin, and source HTML path for a repair.
- Latest focused mail-state evidence: Plato's
  `/tmp/onecontext-probe-c-20260519T195240Z` run exercised multi-recipient
  delivery, focused `mail-read`, claim, `mail-mark`, `mail-mark-all`,
  snooze/include-snoozed, notifications, `agent-inbox`, and `page-status`.
  It passed 25 checks with `failed_checks=0`; the important consumer lesson is
  that closed history remains visible in default `agent-inbox`, while
  actionable fields are `actionable_count`, `pages_requiring_action`, and page
  `open_delivery_count`.
- Latest projects/topics reader evidence: Singer's
  `/tmp/onecontext-probe-b-projects-topics-aD9DAO/1Context` fixture rendered
  8 source pages and 8 talk pages into `route_count=19`, returned HTTP `200`
  for default, project, topic, talk, markdown twin, and `.1context` manifest
  routes, crawled 189 internal links with no failures, and browser-clicked
  menu navigation plus project/topic cross-links through to a topic talk route.
  It also confirmed `/projects/talk` and `/topics/talk` render curator prompt
  templates and role/list talk entries.
- Latest nested lifecycle evidence: Bohr's
  `/tmp/onecontext-probe-a-lifecycle-Wov0iO` run created linked nested pages
  at `/labs/probe-a` and `/labs/probe-a/note`, verified route, markdown twin,
  and talk routes over HTTP, tombstoned the child page and observed child page,
  markdown, and talk routes return `404`, then repaired the parent link and
  ended with `validate.status=ok`, `status.state=idle`.
- Latest attachment/list/mail evidence: Fermat's
  `/tmp/onecontext-probe-b-talk-mail-20260519T200626Z-94BixA` run passed 32
  checks covering invalid attachment preflight with no orphan dir, valid
  attachment copying and filename sanitization, role/list/page delivery,
  focused mail read, claim/read/snooze/done transitions, notification
  ack/poll, `page-status`, `list-status`, `list-members`, final
  `status.state=idle`, and `validate.status=ok`.
- Latest grown-reader evidence: Darwin's
  `/tmp/onecontext-probe-c-grown-reader-20260519T200545Z` run created a
  baseline plus nested project/topic/person mini wiki, published
  `route_count=21` and `markdown_twin_count=21`, verified 27/27 HTTP routes,
  crawled 201 internal links with zero broken links, and used Playwright to
  click brand-menu, nested route, talk route, page markdown twin, and talk
  markdown twin paths. It found and fixed a reader issue where short talk
  pages had valid `.talk.md` twins but no visible Markdown-source link because
  the agent note insertion required a long lead paragraph.
- Latest canonical repair evidence: Bacon's
  `/tmp/onecontext-probe-a-lifecycle-20260519T202403Z-STyJe9` and local
  `/tmp/onecontext-canonical-link-pageid-proof-wMIKB4` prove agents can repair
  nested-route broken links using canonical page ids, not ambiguous rendered
  slugs. After repairs, final `status=idle`, `validate=ok`, and
  `next_action=none`.
- Latest list/mail branchability evidence: Mill's
  `/tmp/onecontext-probe-b-list-mail-after-20260519T202416Z-6QjLXj` run
  verified branch-friendly operation tokens, next actions, liveness counts,
  inbox counts, notification counts, and final closed-work state across
  subscriptions, list status, mail inbox, agent inbox, and notification poll.
- Latest grown-reader polish evidence: Socrates'
  `/tmp/onecontext-probe-c-reader-20260519T202119Z-QDVFB3` run found and
  fixed nested talk-toggle routing and desktop brand-menu overflow for larger
  wikis. It also verified short and long talk pages, markdown source links,
  static-mode host state 404 behavior, reader broken-link warnings, and a
  final 659-link clean crawl.
- Latest page-recipient ergonomics evidence:
  `/tmp/onecontext-page-uri-recipient-proof-Cv3QIN` proves the natural
  `page://<page-id-or-route>` recipient shorthand now resolves to canonical
  page mailbox delivery for `wiki.talk.append`, avoiding a common
  `mailbox://page/<page-id>` memorization trap while keeping stored mail
  addresses canonical.
- Latest edit-safety dogfood evidence:
  `/tmp/onecontext-probe-a-edit-safety-20260519T211357Z-7GKAcj/evidence/summary.json`
  passed 25 checks across page creation, hash-captured `page-open`, stale
  `page-write-body`, stale `page-patch-body`, ambiguous patch refusal, missing
  patch refusal, empty-find refusal, talk-only append, publish status
  before/after source and talk changes, tombstone delete, inbound-link warning,
  successor creation, link repair, and final `wiki.validate=ok` /
  `wiki.status=idle`. It found and fixed a daemon error-code mismatch:
  missing and ambiguous body patches now return the stable API codes
  `body_patch_not_found` and `body_patch_ambiguous` instead of older or generic
  command-failure codes.
- Latest recipient-addressing dogfood evidence:
  `/tmp/onecontext-probe-b-addressing-20260519T2045XX-j3qiZg` passed 13 checks
  across `role://`, `list://`, `mailbox://page/<id>`, `page://<id>`, and
  `page:///<route>` recipients. It found and fixed the inconsistency where
  talk append accepted page aliases but mail recipient commands did not.
  `mail-subscribe`, `mail-subscriptions --address`, `mail-inbox`,
  `mail-claim`, and `mail-mark` now normalize page aliases to
  `mailbox://page/<page-id>` and bad page aliases return
  `invalid_page_recipient` with a concrete repair hint.
- Latest mail/inbox dogfood evidence:
  `/tmp/onecontext-probe-b-mail-inbox-20260519T205919Z-05c8c1` passed 29
  checks across four registered agents, list creation, page alias
  subscriptions, role/list/page fanout, notification poll/ack, focused
  `mail-read`, list claims, competing-claim refusal, timed snooze,
  `mail-mark-all`, page/list/agent status surfaces, final forced publish, and
  `wiki.validate=ok`. The final fixture closed all actionable mail:
  reviewer/curator/triage `agent-inbox.summary.actionable_count=0`,
  `page-status.mail.open_delivery_count=0`, and
  `list-status.mailbox.actionable_count=0`.
- Latest liveness-pressure dogfood evidence:
  `/tmp/onecontext-probe-b-liveness-pressure-20260519T211454Z/evidence/summary.json`
  passed 39 checks across five short-lease agents, heartbeat, one stale agent,
  one retired agent, page-linked lists, `page://topics` subscriptions,
  role/list/page talk delivery, `mail-read`, notification poll/ack, claim,
  idempotent claim, competing-claim refusal, timed snooze, `mail-mark-all`,
  and final pressure clear. It found and fixed a page-status blind spot:
  `mail.subscription_liveness_counts` now covers all page-related
  subscriptions, including explicit page-linked list members, while watcher
  counts stay watcher-only. Final evidence closed all actionable mail with
  `status.state=idle`, `validate.status=ok`,
  `page-status.mail.open_delivery_count=0`, and
  `list-status.mailbox.actionable_count=0`.
- Latest placement/delete dogfood evidence:
  `/tmp/onecontext-probe-a-placement-20260519T2052-rerun-zwuaAw/evidence/summary.json`
  passed 22 checks across primary, utility, and hidden page creation with
  nested routes and `nav_order`, body write/patch, cross-links, publish, HTTP
  route verification, tombstone delete, inbound-link warning detection, link
  repair, and final clean republish. It found and fixed a CLI-help mismatch:
  `--nav-section hidden` already worked, and the help now advertises
  `primary|utility|hidden`.
- Latest delete/restore dogfood evidence:
  `/tmp/onecontext-probe-a-delete-restore-20260519T204228Z-lTHXAc` passed 15
  checks across create, link, publish, tombstone, broken-link repair,
  same-id recreate refusal, same-route replacement refusal, successor-page
  creation, HTTP route checks, and final `wiki.validate=ok`. It found and
  fixed the unclear same-route replacement path: tombstoned routes now return
  `route_already_exists` with a hint to choose a new route until an explicit
  restore operation exists.
- Latest large-reader dogfood evidence:
  `/tmp/onecontext-probe-c-reader-scale-20260519T2035XX-2byHKL` built a
  53-route fixture with primary, utility, nested, project, topic, person, org,
  tool, and process pages; browser-tested menu scrolling, nested article/talk
  toggles, markdown twins, static-mode host-state fallback, warning markers,
  and screenshots; crawled 1,958 internal links with zero broken after repair.
  It found and fixed the talk-page heading issue by demoting headings inside
  collapsed curator prompts so the rendered talk page has one visible article
  `h1`.
- Latest local agent-workbench evidence:
  `/tmp/onecontext-main-dogfood-20260519-RWdl1I/evidence/summary.json` passed
  12 checks across page creation with explicit placement, body write and
  hash-checked patch, cross-links, agent identify, page watch, role assignment,
  list creation/subscription, talk delivery to role/list/page aliases, publish,
  `wiki.status`, `wiki.validate`, page/list/agent inboxes, notification poll,
  and `mail-mark-all`. It found and fixed a control-flow paper cut: clean
  rendered pages with no open mail now report `next_action=none` while keeping
  `wiki.page.open` in `allowed_actions`, so agents do not treat a normal page
  as pending work.
- Latest local recovery evidence:
  `/tmp/onecontext-main-recovery-20260519-ZAyNpR/evidence/summary.json` passed
  10 checks across clean publish, intentionally broken internal link publish,
  `page-status.next_action=repair_links`, `wiki.validate` warning, hash-checked
  repair patch, clean republish, page-recipient talk delivery, and talk-only
  `publish-status` remaining `render_required=false`. It found and fixed a
  branchability mismatch: publish receipts now expose
  `link_diagnostics.broken_internal_count` alongside `issue_count`, matching
  the page-status link summary field agents already use.
- Latest local restore evidence:
  `/tmp/onecontext-main-restore-20260519-q6SNdJ/evidence/summary.json` passed
  9 checks across create, publish, tombstone delete, route removal after
  publish, explicit `wiki.page.restore`, pre-publish `needs_publish`, clean
  republish, utility navigation restoration, final `wiki.validate=ok`, and
  normal page-recipient talk after restore. It added the V0 `page-restore`
  lifecycle operation so agents no longer have to guess between recreating a
  tombstoned page and choosing a successor route.
- Latest memory-adapter restore evidence: `memory-core/tests/test_wiki_core_client.py`
  now covers `wiki_page_restore` after tombstone delete, proving Python-side
  consumers can call the explicit restore operation and observe
  `operation=wiki.page.restore`, `next_action=publish`,
  `page-status.state=needs_publish`, `flags.enabled=true`, and
  `flags.tombstoned=false`. Verification:
  `uv run --project memory-core --with pytest pytest -q memory-core/tests/test_wiki_core_client.py`.
- Latest Probe A consumer API evidence:
  `/tmp/onecontext-probe-a-consumer-api-20260519T215501Z/evidence/summary.json`
  passed 65 checks with zero failures across the Rust CLI and Python
  memory-core adapter. The fixture covered `ensure`, status, validate, list,
  `page-create-all`, custom page create/open/status/write/patch/delete/restore,
  hash mismatch and ambiguous patch errors, unknown page/list errors, agent
  identify/whoami/status/list/inbox, list create/status/members, page watch,
  role assignment, mail subscriptions, talk append with `page://` and list/role
  delivery, attachment delivery, mail read/inbox/claim/mark/mark-all,
  notification poll/ack, publish-status, and publish through
  `wiki_publish(..., node="node")`. It tightened the adapter contract by
  passing the optional `node` argument through `wiki_publish` and documented
  that the node value is an executable token/path, not a shell command string.
- Latest reader-scale dogfood evidence:
  `/tmp/onecontext-probe-c-reader-scale-20260519T205620Z-4SGgDZ/evidence/summary.json`
  built a 47-route, 47-markdown-twin mini wiki covering topics, projects,
  people, organizations, tools, processes, and nested pages. It ran a clean
  publish, a broken-link warning publish, and a repaired publish; the final
  crawl checked 3,164 internal route links with zero broken links. Browser
  proof captured five screenshots and passed 20 checks covering brand-menu
  scrolling/clicking, nested article/talk toggles, markdown twin HTTP routes,
  and Agent view. It found and fixed a nested-page Agent-view bug where the
  HTML surface used `/<slug>` instead of the frontmatter route; Agent view now
  reports `/reader-lab/overview/deep-note` for that nested page.
- Latest route-edge reader dogfood evidence:
  `/tmp/onecontext-probe-c-route-edges-20260519T211222Z/evidence/summary.json`
  built a 21-route, 21-markdown-twin fixture with nested source routes,
  utility navigation, a hidden source page, page/talk markdown twins, a
  broken-link warning phase, and a clean repair. The final HTTP crawl fetched
  83 route/twin paths and checked 893 internal links with zero missing
  targets; in-app browser proof passed 12 checks for route-index relative
  links, menu behavior, nested Agent view, and nested talk source links.
  Screenshots were captured at
  `/tmp/onecontext-probe-c-route-edges-20260519T211222Z/evidence/browser-reader-hub.png`,
  `/tmp/onecontext-probe-c-route-edges-20260519T211222Z/evidence/browser-menu-open.png`,
  `/tmp/onecontext-probe-c-route-edges-20260519T211222Z/evidence/browser-agent-nested.png`,
  and
  `/tmp/onecontext-probe-c-route-edges-20260519T211222Z/evidence/browser-talk-nested.png`.
  It found and fixed two reader edge bugs: route-index duplicate pages now set
  a canonical `<base>` so relative links resolve like the canonical route, and
  Agent view now lists live talk HTML and talk markdown surfaces for article
  pages.
- Latest reader-polish dogfood evidence:
  `/var/folders/1b/r_tcp0rj2vd_7hn6j859tq1c0000gn/T/onecontext-probe-c-reader-polish-20260519T213235Z-79173e/evidence/summary.json`
  built a 17-page, 37-route, 37-markdown-twin fixture with nested pages,
  hidden source-backed pages, utility navigation, project/topic/person/org/tool
  and process pages, talk messages, a broken-link warning phase, and a clean
  repair. The final HTTP crawl fetched 74 served paths and checked 980
  internal links with zero missing targets; Playwright proof passed across menu
  navigation, hidden-page non-leakage, nested talk toggles, Agent view talk
  surfaces, short/long content, markdown twins, and trailing-slash route-index
  pages. Screenshots were captured at
  `/var/folders/1b/r_tcp0rj2vd_7hn6j859tq1c0000gn/T/onecontext-probe-c-reader-polish-20260519T213235Z-79173e/evidence/reader-lab.png`,
  `/var/folders/1b/r_tcp0rj2vd_7hn6j859tq1c0000gn/T/onecontext-probe-c-reader-polish-20260519T213235Z-79173e/evidence/brand-menu-open.png`,
  `/var/folders/1b/r_tcp0rj2vd_7hn6j859tq1c0000gn/T/onecontext-probe-c-reader-polish-20260519T213235Z-79173e/evidence/deep-note-talk.png`,
  and
  `/var/folders/1b/r_tcp0rj2vd_7hn6j859tq1c0000gn/T/onecontext-probe-c-reader-polish-20260519T213235Z-79173e/evidence/deep-note-agent.png`.
  It found and fixed a dev-server proof gap: `serve-site.mjs` now prefers the
  generated `route/index.html` file when a requested URL explicitly ends with
  `/`, so browser tests truly exercise route-index output instead of silently
  serving the canonical `.html` page. `scripts/test-wiki.sh` now asserts the
  trailing-slash route-index `<base>` contract so the harness can catch this
  regression.
- Latest API-consistency dogfood evidence:
  `/tmp/onecontext-probe-a-api-consistency-20260519T213509Z-3GKcN0/evidence/summary.json`
  passed 85 checks over 47 JSON receipts across `ensure`, `page-create-all`,
  `wiki.list`, `page-open`, `page-status`, hash-checked body write and patch,
  missing and ambiguous patch failures, bad page id, bad `page://` alias, bad
  list subscription, list create/status/members, page watch, role assignment,
  talk append, list/page/agent inbox, mail read by message and thread,
  notification poll/ack, mail claim/mark, publish-status, publish, validate,
  tombstone delete, tombstoned talk refusal, and archive-maintenance
  tombstoned talk. It found and fixed the remaining response-shape mismatch:
  `wiki.list`, `wiki.page.status`, `wiki.publish.status`, `wiki.agent.list`,
  `wiki.agent.status`, `wiki.agent.whoami`, `wiki.agent.register`, and
  `wiki.agent.heartbeat` now all include operation-shaped top-level receipts;
  missing configured pages now map to `error.code=unknown_page` with a repair
  hint; and page-status missing-source `next_action` now uses `page_create`
  like validation and publish-status.
- Latest multi-agent collaboration dogfood evidence:
  `/tmp/onecontext-probe-b-collab-20260519T213509Z-g3xyr1f0/evidence/summary.json`
  passed 70 checks across project/topic/brief page creation, explicit source
  writes, source-only publish, four agent registrations, curator roles,
  page/list watchers, `page://` subscription aliases, proposal/reply/decision
  talk messages, attachment copy, notification poll/ack, claim, competing-claim
  refusal, timed snooze, `mail-mark-all`, `mail-read --thread-id`,
  page-status/list-status/agent-inbox pressure, tombstone delete, successor
  repair, final clean publish, validation, and agent retire. It found and
  fixed a rendered attachment bug: talk attachments are now linked through the
  page talk route and copied into the published talk asset path so
  post-render link diagnostics report zero broken links.
- Latest restore/navigation reader dogfood evidence:
  `/tmp/onecontext-probe-c-restore-nav-20260519T2145XX-7l14kG/evidence/summary.json`
  created primary, utility, hidden, and nested pages, published them, tombstoned
  the utility/hidden/nested pages, published the tombstone state, restored each
  page, repaired intentional relative-link mistakes, and republished clean.
  Tombstone HTTP proof showed the hub still returned `200` while tools, hidden,
  nested, their markdown twins, and talk routes returned `404`. Final publish
  reported `link_diagnostics.broken_internal_count=0`, `wiki.validate=ok`, and
  `wiki.status=idle`. Playwright proof checked 19 routes, 19 markdown twins,
  295 internal links with zero broken targets, 20 explicit restored route/twin
  paths, brand-menu membership, hidden-page non-leakage, nested trailing-slash
  route-index `<base>`, nested talk HTML/markdown, and Agent view HTML,
  markdown, Talk HTML, and Talk Markdown surfaces.
- Latest attachment/mail UX dogfood evidence:
  `/var/folders/1b/r_tcp0rj2vd_7hn6j859tq1c0000gn/T/onecontext-probe-b-attachments-mail-20260519T220845-kGqhhF/evidence/summary.json`
  passed 36 checks across project/topic page creation, source writes, agent
  registration, page role/list/page-mail subscriptions, invalid subscription
  relation repair hints, multi-attachment proposal/reply talk messages,
  `mail-read --message-id`, `mail-read --thread-id`, page/list/role inboxes,
  agent inboxes, notification poll/ack, claim and competing-claim refusal,
  canonical `mail-mark` page-alias receipts, `mail-mark-all`, clean publish,
  HTTP-served talk pages, seven route-local attachment assets, tombstone 404s
  for page/talk/attachment routes, restore 200s, final `wiki.status=idle`, and
  `wiki.validate=ok`. Browser proof screenshot:
  `/var/folders/1b/r_tcp0rj2vd_7hn6j859tq1c0000gn/T/onecontext-probe-b-attachments-mail-20260519T220845-kGqhhF/evidence/browser-talk.png`.
  It found and fixed three attachment/mail UX bugs: same-subject burst talk
  messages now get distinct nanosecond-stamped IDs, safe attachment filenames
  collapse punctuation runs, and `wiki.mail.mark` receipts now expose the
  canonical recipient/state after alias normalization. It also added a repair
  hint for invalid mail subscription relations.
- Latest rendered-browser dogfood evidence:
  `/tmp/1ctx-rendered-dogfood-20260519T221738Z-SHeihr/evidence` created a
  topic page, project page, and hidden nested project phase, wrote cross-links,
  appended talk messages, published, tombstoned the nested phase, observed the
  expected `repair_links` post-delete diagnostics for stale links, restored the
  phase, and republished clean. Final publish reported
  `link_diagnostics.broken_internal_count=0`, `next_action=none`, and all six
  new source/talk routes were present in the manifest. Playwright proof against
  `http://127.0.0.1:57904` ran 109 browser checks over source routes, talk
  routes, markdown twins, internal links, brand-menu items, hidden nested
  non-leakage, trailing-slash route-index `<base>` tags, Agent view, Talk
  Agent view, and a missing nested route with zero failures. Screenshots were
  captured under the evidence directory. It found and fixed one serving polish
  issue: `serve-site.mjs` now aliases `/favicon.ico` to the rendered PNG
  favicon, so intentional missing-route diagnostics no longer create a second
  favicon 404 console error. Patched proof against `http://127.0.0.1:58508`
  returned `200 image/png` for `/favicon.ico` before and after the missing-route
  diagnostic.
- Latest consumer API/Python adapter ergonomics evidence:
  `/tmp/onecontext-consumer-api-adapter-20260519T152438/evidence/summary.json`
  passed 42 checks with zero failures. The fixture page-created/published
  default configured pages, then created a custom primary-menu page at
  `/agent-lab/consumer-api-lab-20260519t152438` with explicit family placement,
  opened it by route, verified `wiki.list` and `wiki.page.status` expose
  custom/template/edit/render/tombstone metadata, wrote the body through the
  Python adapter, patched it through the Rust CLI using an adapter-provided
  source hash, published, patched again through the adapter, tombstoned,
  published route removal, restored by route, and published back to a final
  clean rendered state. Earlier ergonomic complaints in this slice were
  receipt-shape polish, not missing adapter functions; later receipt work added
  fresh `page_status`/`hashes` to create/write/patch receipts, leaving only
  compact vocabulary and status-field polish if that becomes worth tightening.
- Latest main-thread agent dogfood evidence:
  `/tmp/onecontext-main-dogfood-20260519T152341/evidence/main-dogfood-summary.json`
  passed 24 checks with zero failures. The fixture created a primary
  `/dogfood/agent-garden` page plus nested `/dogfood/agent-garden/note`, wrote
  and patched article bodies through the Python adapter, assigned role/list/page
  inbox routes, appended an attachment-bearing talk proposal, claimed it from a
  unified `agent-inbox` through the new `agent-claim` helper, acked three
  notifications, published 15 routes/markdown twins, tombstoned the nested
  page, observed expected `repair_links` diagnostics on the parent, repaired
  the source link, restored the child, and ended with
  `publish_status.next_action=none`. Browser evidence at
  `/tmp/onecontext-main-dogfood-20260519T152341/evidence/browser-proof.json`
  verified the rendered parent, talk route, child route, and route-local text
  attachment. It found and fixed two consumer-polish bugs: agents can now call
  `wiki.agent.claim` / `onecontext-wiki agent-claim` without choosing the
  underlying role/list/page recipient, and `serve-site.mjs` serves `.txt`
  attachments as `text/plain` so the in-app browser can display them instead
  of treating them as aborted downloads.
- Latest receipt-ergonomics dogfood evidence:
  `/tmp/onecontext-receipt-ergonomics-20260519T153607/evidence/receipt-ergonomics-summary.json`
  passed 11 checks with zero failures. The fixture created
  `/dogfood/receipt-chain-proof`, verified `wiki.page.create` returned
  `page_status` plus source/talk/curator/conventions hashes, wrote the body
  with the create receipt's hash, patched the body with the write receipt's
  fresh hash without calling `page-open`, published, tombstoned, verified the
  delete receipt's `page_status.state=tombstoned`, restored, verified the
  restore receipt returned `page_status.state=needs_publish` plus a fresh
  source hash, republished clean, and ended with
  `publish_status.next_action=none`. The core now includes optional
  `page_status` and `hashes` on page lifecycle receipts, and the Python adapter
  regression test asserts those fields for create/write/patch.
- Latest receipt-edit-preconditions dogfood evidence:
  `/tmp/onecontext-receipt-ergonomics-final-Xp1dwF/evidence/summary.json`
  passed 14 checks with zero failures. The fixture page-created/published
  baseline configured pages, created `/agent-lab/receipt-lab-153944`, wrote the
  body with only the create receipt's `edit.expected_source_sha256`, patched
  with only the write receipt's fresh `edit.expected_source_sha256`, published
  clean, tombstoned the rendered route, published route removal, restored, and
  republished clean. The checks verified create/write/patch/delete/restore
  receipts all use `next_action=publish` when publication is required, include
  `page_status`, and expose `edit.expected_source_sha256` equal to
  `hashes.source_sha256`; delete also reports `edit.safe_to_edit=false` while
  restore reports `edit.safe_to_edit=true`.
- Latest agent-claim mail pressure dogfood evidence:
  `/var/folders/1b/r_tcp0rj2vd_7hn6j859tq1c0000gn/T/onecontext-agent-claim-dogfood-20260519T223920Z-4fvrzoli/evidence/summary.json`
  passed 47 checks with zero failures. The fixture registered curator,
  reviewer, competing reviewer, and watcher agents; created a page-scoped
  curator role, reviewers list, watcher list, and page mailbox subscriptions;
  delivered one multi-recipient talk proposal to role/list/page/watchers;
  claimed the reviewers-list delivery from `agent-inbox`; verified repeat
  claim idempotence and typed `mail_already_claimed` refusal for the competing
  reviewer; claimed the role delivery; snoozed the page mailbox; archived the
  role delivery; cleared all pressure with `mail-mark-all --state done`;
  verified notification queues and `page-status.mail.open_*` pressure at each
  step; tombstoned the page; confirmed normal talk append returns
  `error.code=tombstoned_page`; restored the page; and verified normal
  post-restore talk/notification delivery.
- Latest claimable-inbox split evidence:
  `/tmp/onecontext-claimable-inbox-20260519T154327/evidence/claimable-inbox-summary.json`
  passed 6 checks with zero failures. The fixture registered two reviewers,
  subscribed both to one shared list, delivered one proposal, confirmed both
  initially saw `actionable_count=1` and `claimable_count=1`, had agent A claim
  the shared delivery through `agent-claim`, then confirmed agent A stayed
  idempotently claimable while agent B saw `actionable_count=1`,
  `claimable_count=0`, and `next_action=mail_read_or_watch_claim`. A competing
  `agent-claim` failed with a claimable-inbox error, and `mail-mark-all done`
  cleared both actionable and claimable pressure. The core now exposes
  `claimable_count` / `claimable_thread_count` / `claimable_delivery_count` so
  agents can branch without trial-claiming another agent's work.
- Latest rendered media/menu browser dogfood evidence:
  `/tmp/1ctx-render-media-menu-20260519T223750Z-38970/evidence/browser-checks.json`
  passed 34 checks with zero failures against
  `http://127.0.0.1:60372`. The fixture created a visible hub, visible nested
  page, and hidden nested page, appended PNG/JPEG/text talk attachments,
  published, verified source routes, talk routes, markdown twins, trailing
  slash route indexes, brand-menu inclusion/exclusion, hidden-page non-leakage,
  attachment links, direct JPEG opening, initial/restored internal crawls,
  missing-route diagnostics, tombstone 404s for page/talk/markdown/attachment
  routes, and restore back to a clean crawl. It found and fixed one static
  server media bug: `serve-site.mjs` now serves `.jpg`, `.jpeg`, `.gif`,
  `.webp`, and `.avif` image attachments with their browser-native MIME types
  instead of `application/octet-stream`.
- Latest rich content graph dogfood evidence:
  `/var/folders/1b/r_tcp0rj2vd_7hn6j859tq1c0000gn/T/onecontext-rich-content-graph-20260519T225610Z-k45itj8y/evidence/rich-content-graph-summary.json`
  passed 37 checks with zero failures. The fixture created a topic page, project
  page, person page, tool page, and domain page, using a mix of Rust CLI and
  Python adapter calls. It wrote cross-linked bodies, published clean, verified
  `wiki.list` metadata for edited rendered custom pages and primary/hidden site
  placement, tombstoned the linked tool page, verified delete `link_impact`,
  post-publish `repair_links`, `publish-status.link_health`, `validate`
  warnings, and per-page `page-status.links` diagnostics. It then repaired all
  stale links with hash-checked patches across both API surfaces, republished
  clean, restored the tool page, re-linked one topic to the restored route, and
  ended with `validate.status=ok` and `publish_status.next_action=none`.
- Latest fresh-runtime publish/fallback evidence:
  `/tmp/onecontext-fresh-publish-fallback-20260519T225538Z/evidence/summary.json`
  passed 11 CLI checks with zero failures from a disposable copy of
  `runtime/1Context` without calling `page-create-all`. Before publication,
  `status`, `publish-status`, `validate`, `page-status topics`, and
  `page-open topics` now converge on `next_action=publish`; validation is a
  publishable warning instead of a blocker; and `page-open` reports missing
  source/talk hashes as `null`, `edit.safe_to_edit=false`, and
  `edit.recommended_operation=wiki.publish` instead of pretending the page is
  patchable. `publish` then safe-created the four configured pages during
  publish preflight, rendered 11 routes, and ended with
  `publish-status.next_action=none` and `validate.status=ok`.
- Latest Python-adapter direct-publish backfill evidence:
  `/var/folders/1b/r_tcp0rj2vd_7hn6j859tq1c0000gn/T/onecontext-publish-backfill-1779235099-w68rkjjk/evidence/publish-backfill-summary.json`
  passed from a fresh disposable copy of `runtime/1Context`. The adapter saw
  `publish-status.next_action=publish` with four missing configured sources,
  `page-open topics.edit.recommended_operation=wiki.publish`, called
  `wiki.publish` without `page-create-all`, observed a consumer-facing
  `operation=wiki.publish.preflight`, verified the nested result also avoided
  `wiki.page.create_all`, saw `render_input.next_action=publish` for the newly
  safe-created pages, rendered all four configured pages, and ended with
  `after.next_action=none`.
- Latest rendered/browser surface dogfood evidence:
  `/var/folders/1b/r_tcp0rj2vd_7hn6j859tq1c0000gn/T/onecontext-render-browser-surface-20260519t225743z-idfdfelu/evidence/summary.json`
  passed initial, tombstoned, and restored browser phases with zero failures
  against `http://127.0.0.1:63118` during the run. The fixture created a
  visible hub, visible nested utility page, and hidden page; wrote cross-linked
  page bodies; appended text, PNG, and JPEG talk attachments; verified source
  routes, talk routes, markdown twins, trailing-slash route indexes, favicon
  aliasing, brand-menu inclusion/exclusion, hidden-page non-leakage, Reader and
  Agent view transitions, missing-route diagnostics, attachment link MIME
  types, child tombstone 404s, stale-link diagnostics, restore, final
  `validate`, and final `publish-status`. The only harness adjustment was to
  explicitly switch back to Reader view after testing Agent view because the
  viewer intentionally preserves the selected mode across route loads.
- Latest publish-preflight receipt naming dogfood evidence:
  `/tmp/onecontext-publish-preflight-receipt-20260519T235601Z/evidence/summary.json`
  passed 8 checks with zero failures from a disposable copy of
  `runtime/1Context`. The run called `wiki.publish` directly from a fresh
  runtime with four missing configured sources, observed one `preflight` entry
  named `operation=wiki.publish.preflight`, verified the nested per-page receipts
  used canonical `wiki.page.create`, verified the preflight JSON did not
  contain `wiki.page.create_all`, rendered cleanly, and ended with
  `publish-status.next_action=none`. Consumer-agent judgment: exposing
  `wiki.page.create_all` inside the publish receipt was a leaky low-level
  concept; the direct `page-create-all` maintenance verb can still exist, but
  happy-path agent docs and receipts should keep it out of the publish story.
- Latest Reader/Agent Talk navigation dogfood evidence:
  `/tmp/1ctx-view-persistence-fixture/evidence/view-persistence-summary.json`
  passed with zero browser failures against `http://127.0.0.1:51805`. The run
  proved that entering Agent view on an article, then clicking the Talk button,
  now lands on the talk route in Reader mode instead of inheriting raw Agent
  mode. Explicit Agent mode on the talk route still works, and the Talk-back
  navigation returns to the article in Reader mode. This removes a real
  dogfood footgun while preserving ordinary article-to-article Agent view
  persistence.
- Latest mail/list/inbox notification dogfood evidence:
  `/var/folders/1b/r_tcp0rj2vd_7hn6j859tq1c0000gn/T/onecontext-mail-list-inbox-dogfood-fixed-v9bb1jl_/artifacts/evidence.json`
  passed seven disposable-fixture checks focused on multi-agent subscription and
  address ergonomics: `page-status topics` exposed `mailbox://page/topics`,
  `role://topics.curator`, and `list://topics.watchers`; `list://topics.triage`
  had two live members and now reports `next_action=mail_claim_or_mark` while
  shared work is open or claimed; both agents saw the claimed list delivery with
  `claimed_by`; snoozed list work was hidden from default inboxes but visible
  with `--include-snoozed`; and marking all deliveries `done` cleared page
  pressure, list actionable counts, agent inbox actionable counts, and pending
  notifications. The run found and fixed one core metadata bug:
  `wiki.list.status` previously returned `next_action=none` even when its
  mailbox had actionable work. Verification:
  `cargo test --workspace` and `uv run --with pytest --project memory-core pytest
  memory-core/tests/test_wiki_core_client.py`. Superseded follow-up:
  `list-status` now has `--include-snoozed` / `--include-archived` and exposes
  hidden-message audit flags, so agents no longer need to switch to
  `mail-inbox` just to discover that list work is hidden by audit filters.
- Latest configured-page preservation dogfood evidence:
  `/var/folders/1b/r_tcp0rj2vd_7hn6j859tq1c0000gn/T/onecontext-preserve-configured-1779235636-ynv78aem/evidence/preserve-configured-summary.json`
  passed from a fresh disposable copy of `runtime/1Context`. The run created
  the configured `topics` page, wrote a user-authored body, then called
  `wiki.publish` while `for-you`, `your-context`, and `projects` were still
  missing source. Publish preflight used `operation=wiki.publish.preflight` plus
  canonical per-page `wiki.page.create` receipts, backfilled the missing
  siblings, reported all `topics` file evidence as
  `skipped_existing`, preserved `topics.content_state=edited`, rendered the
  user-authored text into `topics.html`, and ended with
  `publish-status.next_action=none`.
- Latest nested talk attachment browser evidence:
  `/tmp/1context-rendered-surface-20260520000943/browser-checks.json` passed
  the rendered-surface browser dogfood after a route-index base fix for nested
  talk pages. Follow-up republish evidence from
  `/tmp/1context-rendered-surface-20260520000943/1Context/user-wiki/site/work/deep/nested-page.talk.html`
  confirmed canonical `*.talk.html` now also includes
  `<base href="/work/deep/nested-page/talk/">`, so relative talk attachment
  links resolve to
  `/work/deep/nested-page/talk/attachments/proof.txt` instead of escaping to
  the parent route.
- Latest backlink/link-impact ergonomics dogfood evidence:
  `/tmp/onecontext-backlink-link-impact-ergonomics-ec4pil/artifacts/summary.json`
  passed from a disposable copy of `runtime/1Context`. The run created a small
  topic/project/person/tool graph with route links and a markdown-twin link,
  published cleanly, tombstoned `erg-tool`, and confirmed the delete receipt
  returned `link_impact.status=warning`, `inbound_link_count=3`,
  `source_page_count=2`, and both `route` and `markdown_twin` target kinds.
  Post-delete publish returned `link_diagnostics.status=warning`,
  `issue_count=3`, and `next_action=repair_links`; `wiki.validate` returned
  warning; `page-status erg-topic` and `page-status erg-project` both exposed
  warning `links` with broken targets. Restoring `erg-tool` and publishing
  returned link diagnostics to `ok`. The run repeated the lifecycle for
  `erg-person`, saw two inbound route links become warnings, restored it, and
  ended with `validate.status=ok`, `publish-status.next_action=none`, and
  `link_health.status=ok`. No core patch was needed. Later branchable
  `link_repair_lifecycle` evidence below closes the remaining stitched-receipt
  repair-story complaint.
- Latest link-repair story ergonomics evidence:
  `/tmp/onecontext-link-repair-ergonomics-6RIc97` passed from a disposable copy
  of `runtime/1Context`. The run created `ergonomics-source` and
  `ergonomics-target`, published cleanly, tombstoned the target, and confirmed
  the delete receipt still previews `link_impact.inbound_link_count=2` and
  `post_publish_expected_next_action=repair_links`. The post-delete publish
  returned `link_diagnostics.status=warning`, `issue_count=2`, and
  `next_action=repair_links`; the same structured repair task now appears in
  the publish receipt at `after.link_health.repair_tasks`,
  `publish-status.link_health.repair_tasks`,
  `wiki.status.publish.link_health.repair_tasks`,
  `wiki.validate.issues[].repair_tasks`, and
  `page-status ergonomics-source.links.repair_tasks`. The repair task names the
  source page, route, rendered source paths, both broken targets, and suggested
  operations: `wiki.page.open`, `wiki.page.patch_body`, `wiki.publish`, and
  `wiki.validate`. The run then repaired the source with a hash-checked
  `page-patch-body`, republished, and confirmed `publish-status.next_action=none`,
  `validate.status=ok`, `page-status.links.status=ok`, and no remaining repair
  tasks. Verification: `cargo check --workspace`,
  `cargo test -p onecontext-wiki-daemon`, and the fixture assertions in
  `13-warning-checks.json` / `20-clean-checks.json`. Later branchable
  `link_repair_lifecycle` evidence below closes the remaining delete-preview
  through final-clean timeline complaint.
- Latest stale edit conflict dogfood evidence:
  `/var/folders/1b/r_tcp0rj2vd_7hn6j859tq1c0000gn/T/onecontext-stale-edit-1779236578-rp4locgi/evidence/stale-edit-summary.json`
  passed from a disposable copy of `runtime/1Context`. Two simulated agents
  opened `topics` at the same source hash; agent A wrote the page; agent B's
  stale `page-write-body` and `page-patch-body` both failed with structured
  `source_hash_mismatch` payloads, `operation=wiki.page.write_body` /
  `operation=wiki.page.patch_body`, and repair hints telling the agent to rerun
  `wiki.page.open` and use the fresh `edit.expected_source_sha256`. After
  reopening, the patch succeeded, publish completed, and
  `publish-status.next_action=none`. The Python adapter now preserves the
  parsed error JSON on `WikiCoreError.payload` so agents do not need to scrape
  JSON out of an exception string.
- Latest rendered graph navigation browser evidence:
  `/tmp/onecontext-rendered-graph-nav-172438/evidence/browser-checks.json`
  passed one Playwright browser test against
  `http://127.0.0.1:55832`. The disposable fixture rendered nested routes
  `/field/notes` and `/field/notes/deep-dive`, section routes
  `/field/notes/calibration` and `/field/notes/operator-loop`, the talk route
  `/field/notes/talk`, canonical `*.html` surfaces, markdown twins,
  `.1context/route-manifest.json`, `.1context/content-index.json`, a copied
  talk attachment at `/field/notes/talk/attachments/evidence.txt`, Reader view,
  Agent view, and a brand-menu click into the nested Deep Dive page. The run
  found and fixed one renderer manifest bug: section markdown twins were
  inheriting the parent `route`, so route manifests could report duplicate
  parent routes for section pages. Section frontmatter now derives
  `route`, `parent_route`, `md_url`, and `talk_url` from the parent route plus
  section slug, with regression coverage in
  `wiki-engine/src/renderer/index.test.mjs`. Follow-up evidence below covers
  rendered section talk routes.
- Latest rendered section-talk route browser evidence:
  `/var/folders/1b/r_tcp0rj2vd_7hn6j859tq1c0000gn/T/onecontext-section-talk-route-20260520T004149-IGZzs8/evidence/browser-checks.json`
  and `http-route-checks.json` passed against the disposable fixture served at
  `http://127.0.0.1:57748`. The fixture rendered nested configured route
  `/field/notes`, section route `/field/notes/calibration`, section talk route
  `/field/notes/calibration/talk`, page-level talk route `/field/notes/talk`,
  and markdown twins `/field/notes/calibration.talk.md` and
  `/field/notes.talk.md`. Browser automation clicked the section Talk button
  into the rendered section talk route, confirmed `h1=Talk · Calibration`,
  `base href=/field/notes/calibration/talk/`, alternate markdown
  `/field/notes/calibration.talk.md`, no console errors, then clicked back to
  `/field/notes/calibration`. The same pass confirmed page-level talk still
  renders `Talk · Field Notes` with alternate markdown `/field/notes.talk.md`.
  HTTP checks returned 200 for the page, page talk, section, section talk, and
  markdown twins, while `/field/notes/operator-loop/talk` stayed 404 because
  that section had `talk: false`. Regression coverage:
  `npm test` in `wiki-engine` passed 6 tests, including the new
  `render-to-dir emits rendered routes for section talk stubs` case. Remaining
  complaint: the local Codex in-app browser pane was unavailable and the Chrome
  extension browser was blocked by an open extension UI, so the visible proof
  used an isolated disposable Playwright Chromium install.
- Latest tombstone/restore mail-survival dogfood evidence:
  `/var/folders/1b/r_tcp0rj2vd_7hn6j859tq1c0000gn/T/onecontext-tombstone-mail-20260520004812-0H4iTt/evidence/tombstone-mail-summary.json`
  passed from a disposable copy of `runtime/1Context`. The run created a hidden
  dogfood page at `/dogfood/archive-mail-20260520004812`, assigned a curator,
  created a page watcher list, delivered one talk entry to both the curator
  role and list, tombstoned the page, verified normal `talk-append` was refused
  with structured `tombstoned_page` plus the `--allow-tombstoned` repair hint,
  appended an explicit archive-maintenance note with `--allow-tombstoned`,
  published the tombstone, restored the page, republished, and ended with
  `validate.status=ok`, `page-status.state=rendered`,
  `page-status.next_action=none`, and page mail still visible:
  `mail.actionable_count=3`, `mail.open_delivery_count=3`,
  `mail.open_thread_count=2`. The pass found and fixed one agent-ergonomic
  naming mismatch: page mail now exposes `mail.actionable_count` as an alias
  for `mail.open_delivery_count`, matching inbox surfaces while preserving the
  precise delivery/thread fields.
- Latest Python adapter unsubscribe/retire dogfood evidence:
  `/var/folders/1b/r_tcp0rj2vd_7hn6j859tq1c0000gn/T/onecontext-python-unsubscribe-62_654sc/evidence/python-unsubscribe-summary.json`
  passed from a disposable copy of `runtime/1Context`. The run used the
  memory-side Python adapter as an agent would: create a hidden dogfood page,
  create a watcher list, subscribe, unsubscribe, verify list membership dropped
  from `1` to `0`, append list mail and confirm the unsubscribed agent received
  `0` notifications, resubscribe and confirm the next list mail produced a live
  notification, then retire the agent. The pass found and fixed one adapter
  coverage gap: Python now exposes `WikiCoreClient.mail_unsubscribe` and
  `wiki_mail_unsubscribe`, with export/docs/test coverage. Follow-up below
  fixes the retired-agent notification error code that this run exposed.
- Latest retired-agent notification error proof:
  `/var/folders/1b/r_tcp0rj2vd_7hn6j859tq1c0000gn/T/onecontext-retired-error-krn3k79q/evidence/retired-error-summary.json`
  passed from a disposable copy of `runtime/1Context`. After an agent subscribed,
  received one notification, and explicitly retired, `wiki.notify.poll` now
  returns structured `retired_agent` for that identity instead of the vaguer
  `unknown_active_agent`, with the existing repair hint to start a new
  thread/session and call `agent-identify`.
- Latest Worker C rendered navigation/talk-twin evidence:
  `/var/folders/1b/r_tcp0rj2vd_7hn6j859tq1c0000gn/T/onecontext-worker-c-nav-RSDcJl/evidence/browser-checks.json`
  and `http-route-checks.json` passed against a disposable source tree served
  at `http://127.0.0.1:59821`. The fixture rendered primary, utility, hidden,
  and nested custom pages; page-level talk at `/dogfood/sitemap/talk`; section
  talk at `/dogfood/sitemap/calibration/talk`; route and markdown twins for
  pages, page talk, section pages, and section talk; and verified the hidden
  page remained out of the brand menu while direct hidden routes still served.
  The pass fixed the page-level route override path so configured routes emit
  `talk_route` for the human talk route and `talk_url` for the `.talk.md`
  markdown twin, matching the section frontmatter contract.
- Latest unsubscribe API/runbook parity evidence:
  `/var/folders/1b/r_tcp0rj2vd_7hn6j859tq1c0000gn/T/onecontext-doc-unsubscribe-20260520010912-LeE9pb/evidence/doc-unsubscribe-summary.json`
  passed from a disposable copy of `runtime/1Context`. The run followed the
  documented CLI loop: create hidden dogfood page, create watcher list,
  subscribe, unsubscribe by `agent_id`/`address`/`relation`/`kind`, verify
  `cancelled_count=1`, `remaining_count=0`, list `member_count=0`, then append
  durable list mail and confirm the unsubscribed agent received `0`
  notifications. The API doc, architecture table, and runbook now list
  `wiki.mail.unsubscribe` explicitly.
- Latest file-only talk thread hydration evidence:
  `/tmp/onecontext-wiki-core-dogfood.UELv6K/evidence/mail-read-thread.json`
  and `mail-read-file-message.json` passed against a disposable runtime. The
  run created legacy on-disk talk markdown entries without delivery records,
  appended a delivered reply with `--reply-to talkmsg_file_only_parent_cli`,
  and confirmed `mail-read --thread-id` now merges the file-only parent beside
  delivered replies. `mail-read --message-id talkmsg_file_only_parent_cli` also
  works and reports `delivery_count=0`, making the lack of mailbox delivery
  explicit rather than silently hiding the legacy entry.
- Latest file-only talk attachment hydration evidence:
  `/tmp/onecontext-file-only-attachment-hydration.BGkp6J/evidence/summary.json`
  passed against a disposable runtime. A legacy Markdown-only talk parent with
  structured attachment frontmatter and no mailbox rows now reads back through
  `mail-read --message-id` with `attachment_count=1`, and
  `mail-read --thread-id` preserves that hydrated parent beside a delivered
  reply in the same thread.
- Latest runtime defaults hygiene and browser route evidence:
  `test-results/wiki-core-main-runtime-hygiene-20260520T1136Z` published a
  disposable copy of `runtime/1Context` to 11 routes / 11 markdown twins,
  ended with `validate.status=ok`, `publish-status.render_required=false`,
  and found no unexpected public-runtime leak hits. In-app Browser proof in
  `browser-route-evidence.json` loaded `/for-you`, `/topics`, `/topics/talk`,
  `/projects`, and `/your-context` with no missing-route text, then clicked the
  Talk button from `/topics` to `/topics/talk` and saw `h1=Talk - Topics`.
- Latest runtime/defaults wording cleanup evidence:
  `test-results/worker-cm-runtime-hygiene-20260520T1136Z` ran package smoke and
  leak scans, cleaned `runtime/1Context/user-wiki/README.md` so the blessed
  runtime tree no longer mentions the private `runtime-test` lab, and updated
  stale materialize/defaults phrasing in the older wiki engine goal doc.
- Latest core concurrency/idempotence evidence:
  `test-results/worker-cl-concurrency-idempotence-20260520T1136Z` proved
  lifecycle hash checks and mail mutations run under the relevant advisory
  locks, repeated claim/mark/ack/tombstone paths avoid duplicate durable
  events, duplicate agent roles/capabilities are deduped, and
  `cargo test --workspace -- --nocapture` passed.
- Latest publish failure recovery evidence:
  `test-results/worker-ck-publish-recovery-20260520T1136Z` proved a broken
  `wiki.toml` returns `repair_wiki_toml` hints, blocks publish without
  overwriting the last good site, then republishes cleanly after repair. The
  Swift bridge now preserves failed `wiki.publish` receipts as structured JSON
  results instead of flattening them into an RPC error string; focused proof:
  `swift test --package-path macos --filter WikiCoreProcessClientTests` passed,
  including
  `testFailedPublishReceiptReturnsStructuredObjectOnNonZeroExit`.
- Latest subscription lifecycle dedupe evidence:
  `/tmp/onecontext-worker-b-subscription-dogfood-HjowtV/evidence/summary.json`
  passed against a disposable runtime. Duplicate `mail-subscribe` now returns
  `status=renewed`, keeps the same subscription id, dedupes/normalizes kind
  filters, and leaves `subscription_count_after_duplicate=1`; unsubscribe by
  relation/kind matches order-insensitively and returns
  `next_action=mail_subscriptions` when related subscriptions remain. The same
  run proved list liveness can show active/stale/retired subscription counts
  of `1/1/1`.
- Latest large rendered-site fixture evidence:
  `/var/folders/1b/r_tcp0rj2vd_7hn6j859tq1c0000gn/T/1ctx-worker-c-dogfood-q0gedv/evidence/playwright-checks.json`
  passed against a disposable rendered site at `http://127.0.0.1:61657`. The
  regression fixture now builds primary, utility, hidden, and nested routes,
  page talk, section talk, route/markdown twins, and checks both
  `.1context/route-manifest.json` and `.1context/content-index.json`.
- Latest static-mode state closure evidence:
  `/var/folders/1b/r_tcp0rj2vd_7hn6j859tq1c0000gn/T/1ctx-worker-c-static-state-ic7h5o/evidence`
  passed against a disposable rendered site at `http://127.0.0.1:63400`.
  `GET /api/wiki/state` now returns daemon-shaped read-only static state
  (`_storage.writable=false`), `PATCH /api/wiki/state` returns `405`, and
  Playwright saw zero console messages with only `GET /api/wiki/state => 200`
  network entries after a theme change.
- Latest agent-facing CLI strictness evidence:
  `/tmp/onecontext-cli-extra-args-W9iU2c/summary.json` passed. Dogfooding
  proved `page-status topics unexpected-extra` used to silently succeed, which
  is unsafe for model consumers. The daemon CLI now rejects stray arguments for
  page lifecycle, publish, agent lifecycle, talk append, and notification
  commands before mutating state. The same evidence directory includes
  `page-status-extra.json` with `error.code=unexpected_arguments`,
  `page-status-extra.exit` with exit code `1`, and
  `regression-test.txt` for
  `agent_facing_commands_reject_stray_arguments_before_mutating`.
- Latest custom page/site-tree dogfood evidence:
  `test-results/wiki-core-worker-a-dogfood-20260520T014243Z` passed without a
  code patch. The disposable runtime created
  `worker-a-site-tree-proof` at `/projects/worker-a-site-tree-proof` with
  explicit `nav_section=primary` and `nav_order=12`, wrote and patched the
  body, linked it from `projects`, published, tombstoned, republished,
  restored, and republished again. Status metadata moved from
  `template_unedited` to `edited_from_template`, the tombstone receipt reported
  one inbound `projects` link, final `wiki.validate` returned `ok`, final
  `publish-status.next_action=none`, and link diagnostics ended with
  `broken_internal_count=0`.
- Latest mail lifecycle dogfood evidence:
  `/tmp/onecontext-worker-b-dogfood-20260520T014118Z/evidence/summary.json`
  passed without a code patch. The disposable runtime covered
  `agent-register`, `agent-identify`, heartbeat, retire, list creation,
  kind-filtered subscribe/unsubscribe, attachment-bearing talk append,
  agent inbox, mail read, agent/mail claim, mark, mark-all, notify poll, and
  notify ack. Agent inbox started at `actionable=3`, notifications started at
  `3`, ack/marking drained the agent workbench to `0`, subscriptions drained
  to `0`, and retired-agent `notify-poll` returned structured
  `retired_agent`. The pass documented the intended distinction that raw
  durable list status may still show a list delivery while a particular
  agent's subscription-filtered workbench is clear.
- Latest rendered-site/browser dogfood evidence:
  `test-results/worker-c-rendered-site/browser-evidence.json` passed against a
  disposable served site. The run verified menu links, `/guide`, `/guide/talk`,
  `/guide/details`, `/guide/details/talk`, nested project/utility/hidden
  routes, markdown twins including `/hidden/lab.md`, Agent view, and static
  `/api/wiki/state` (`GET 200`, `PATCH 405`). It found and fixed two renderer
  gaps: direct `render-site.mjs` output now writes
  `.1context/link-diagnostics.json`, annotates canonical and route-index HTML
  with reader-visible broken-link warnings, and adds the diagnostics pointer to
  `route-manifest.json`; route-bearing pages now advertise the correct
  route-based markdown twin instead of the source slug fallback.
- Latest dogfood hygiene note:
  negative mutation probes must run against disposable copies, not
  `runtime/1Context`. A bad-input check briefly created throwaway
  `bad-nav-order-proof` page files and a `bad-ttl-proof` agent registration in
  the blessed runtime; those exact generated files were removed immediately and
  a follow-up `find runtime/1Context -path '*bad-nav-order-proof*' -o -path
  '*bad-ttl-proof*' -o -name '*24460a2bd521bc16*'` returned no matches. A
  later audit also removed the matching accidental `wiki.toml` page entry and
  temporary `user-wiki/.1context/page-ledger.jsonl` rows, then repeated the
  same `find` / `rg` cleanup checks with no matches. Future destructive or
  mutation-oriented dogfood evidence should name its disposable root
  explicitly.
- Latest numeric CLI flag ergonomics evidence:
  `/tmp/onecontext-numeric-cli-flags-P8hN0B/summary.json` passed against a
  disposable runtime copy. Bad `page-create --nav-order nope` and bad
  `agent-register --ttl-seconds nope` both exited `1` with
  `error.code=invalid_arguments` instead of silently dropping to defaults,
  while valid `page-create --nav-order 7` still succeeded. The daemon CLI now
  validates `--nav-order` as an integer and validates `--ttl-seconds` as a
  positive integer for agent register/identify/heartbeat, mail subscribe,
  page watch, and page role assignment, with focused regression coverage for
  malformed, missing, and valid numeric values.
- Latest list-status audit ergonomics evidence:
  `test-results/wiki-core-worker-b-list-status-mail-inbox-20260520T020036Z/evidence/summary.json`
  passed from a disposable runtime. Default `list-status` showed only visible
  mail while reporting `audit_flags=["archived_hidden","snoozed_hidden"]`,
  `hidden_archived_count=1`, and `hidden_snoozed_count=1`; include-snoozed,
  include-archived, and both flags surfaced the expected hidden rows. Matching
  `mail-inbox` checks on list, role, and page mailboxes proved the same audit
  semantics. The API example now maps actionable list work to
  `next_action="mail_claim_or_mark"` instead of stale `none`.
- Latest browser link-repair lifecycle evidence:
  `test-results/wiki-core-worker-c-route-link-20260520T015830Z` passed without
  a renderer patch. The broken fixture rendered reader-visible warnings and
  `.1context/link-diagnostics.json` with `status=warning`, `issue_count=3`,
  and targets `/topics/missing-signal` plus `/projects/deep-dive/appendix`.
  The repaired fixture rerendered to `status=ok`, `issue_count=0`, with browser
  screenshots proving the warning disappeared while menu links, Agent view, and
  nested routes stayed reachable. Disposable servers were stopped after proof.
- Latest CLI missing-value ergonomics evidence:
  `test-results/wiki-core-worker-a-cli-missing-values-20260520T021730Z`
  passed against a disposable runtime. Dangling or next-flag values for global
  `--root`, `page-create --title`, `publish --trigger`, `talk-append --body`,
  `--to`, `--attachment`, `mail-subscribe --kind`, `mail-mark --until`, and
  `notify-ack --state` all exited `1` with missing-value JSON before mutating
  the runtime. `mutation-diff.exit=0` proves the disposable copy's file
  checksums stayed unchanged.
- Latest talk attachment dogfood evidence:
  `test-results/worker-b-attachments-20260520T021319Z/evidence` passed from a
  disposable runtime. `talk-append` copied five attachments, disambiguated a
  duplicate filename to `duplicate-2.txt`, rejected missing and unsafe
  attachment paths with `invalid_attachment` and no orphan attachment
  directories, and `mail-read` returned attachment metadata without absolute
  path leaks. Publish rendered all talk attachment links, copied all attachment
  files into the static site, and browser fetch proof returned HTTP `200` for
  all five rendered attachment URLs.
- Latest disposable browser-navigation dogfood evidence:
  `test-results/wiki-core-worker-c-browser-nav-20260520T021342Z` passed without
  a renderer patch. The fixture created pages under `/topics`, `/projects`, and
  `/for-you`, plus nested `/projects/worker-c-browser-nav/deep-note`, ran a
  delete/restore cycle for the nested page, and ended with `19` routes,
  `19` markdown twins, `8` source inputs, `8` talk inputs,
  `link_diagnostics.status=ok`, and `wiki.validate.status=ok`. Browser checks
  passed for menu navigation, route manifest, markdown twins, Agent view, talk
  routes, and zero console/page/request errors. Operational lesson: concurrent
  `page-create` calls against the same `wiki.toml` exposed a real lifecycle
  race, now covered by the core page lifecycle lock evidence below.
- Latest core page lifecycle concurrency evidence:
  `test-results/wiki-core-worker-a-page-create-race-20260520T023432Z` proved
  the pre-fix race: 80 parallel `page-create` CLI calls all exited `0` and
  wrote 80 source files, but only 13 `race-page-*` registry records survived in
  `wiki.toml`. `test-results/wiki-core-worker-a-page-create-race-fixed-20260520T023932Z`
  then proved the fix: 80 successes, 0 errors, 80 page records, 80 source
  files, `wiki.validate` exit `0`, and `wiki.list` exit `0`. The Rust core now
  serializes page lifecycle registry writes with an advisory file lock at
  `user-wiki/.1context/page-lifecycle.lock` and rewrites `wiki.toml`
  atomically.
- Latest core mail concurrency evidence:
  `test-results/wiki-core-worker-b-concurrent-mail-notify-20260520T023618Z`
  proved the pre-fix race: concurrent `agent-claim` / `mail-claim` attempts
  could both return `claimed` for the same message and recipient, with 16
  duplicate claim races out of 48 attempts.
  `test-results/wiki-core-worker-b-concurrent-mail-notify-fixed-20260520T024202Z`
  then proved the fix: 48 claim races, 0 double-success cases, 0 duplicate
  claim audit keys, and concurrent `mail-mark-all --state done` plus
  `notify-ack` left notification counts at 0 with all deliveries `done`. The
  Rust core now serializes mail mark/claim mutations through
  `context-engine/mail/.mutation.lock`, refuses claims for terminal
  `done`/`archived` deliveries, and keeps the CLI thin instead of adding a
  broad command-wide mutation lock.
- Latest large-graph browser dogfood evidence:
  `test-results/wiki-core-worker-c-large-graph-20260520T023505Z` built a
  disposable 14-page grown-wiki graph across for-you, projects, topics,
  people, tools, and organizations, with talk folders, talk entries, markdown
  twins, and a delete/restore cycle for `/tools/repair-cycle`. Initial publish
  produced `39` routes and `39` markdown twins with link diagnostics `ok`;
  tombstone publish intentionally produced `6` broken inbound links; restore
  publish returned to `39` routes, `39` markdown twins, `0` broken internal
  links, and `wiki.validate.status=ok`. The browser crawl checked 28 custom
  page/talk routes, fetched 630 internal links, verified the menu/Agent/Talk
  views, and found 0 console errors, 0 page errors, and 0 non-aborted request
  failures.
- Latest integrated verification evidence, 2026-05-20T02:50Z:
  `cargo fmt --check && cargo test -q --workspace`, `uv run --project
  memory-core --with pytest pytest -q memory-core/tests/test_wiki_core_client.py`,
  `npm test --prefix wiki-engine`, the renderer `node --check` bundle plus
  `git diff --check`, and `./scripts/test-wiki.sh` all passed. Focused
  concurrency reruns also passed for
  `concurrent_page_create_commands_preserve_both_registry_entries` and
  `parallel_page_create_processes_preserve_registry_records`. `cargo run -q -p
  onecontext-wiki-daemon -- --help` also passed after integrating the lock
  simplification, resolving the temporary `cargo run` block seen during the
  large-graph worker run. A final
  `runtime/1Context` scan found no disposable bad-flag or race-test IDs, so the
  public blessed runtime stayed clean during this dogfood round.
- Latest main-agent workflow dogfood evidence:
  `test-results/wiki-core-main-agent-workflow-20260520T030452Z` created a
  hidden `/topics/main-agent-flow` page, performed a hash-checked
  `page-write-body` using `page-open.edit.expected_source_sha256`, published
  the content, registered two agents, watched and assigned the page curator
  role, appended talk mail with an agent-chosen thread id
  `main-agent-flow-thread`, claimed it through `agent-claim`, read it by
  thread, marked all deliveries `done`, and verified `wiki.validate.status=ok`
  with `issue_count=0`. The run exposed and fixed an agent ergonomics bug:
  `talk-append --thread-id <new-id>` used to require an existing thread target,
  which made user/session correlation ids awkward. The Rust core now allows a
  sender to start a page-local talk thread with an explicit id while still
  rejecting cross-page thread reuse. Focused unit proof:
  `cargo test -q -p onecontext-wiki-core talk_append_can_start_with_explicit_thread_id`.
  Mail-state operations did not require publishing: after `mail-mark-all
  --state done`, `publish-status` reported `render_required=false`,
  `site_needs_publish=false`, and `next_action=none`.
- Latest agent churn dogfood evidence:
  `test-results/wiki-core-worker-a-agent-churn-20260520T030134Z` ran 44 CLI
  commands against a disposable runtime with 4 agents under churn: 2 active,
  1 stale, and 1 retired at the end. It exercised short TTL registration,
  heartbeat renewal, explicit retirement, page/list/role subscriptions,
  unsubscribe paths, talk delivery to page/list/role/direct recipients, agent
  inbox, notification poll/ack, claim, mark-done, archive audit views, and
  inventory/status reporting. Final evidence showed 5 deliveries, 8
  notifications, active/stale/retired subscription context preserved, archived
  and done mail still inspectable in audit views, and all churn assertions
  passed. No core or daemon patch was needed for this slice.
- Latest delete/restore attachment dogfood evidence:
  `test-results/wiki-core-worker-c-delete-attachments-20260520T030343Z`
  created nested custom pages across primary, utility, and hidden nav sections,
  added talk messages with text and image attachments, published, tombstoned a
  linked nested leaf, and restored it. Tombstone publish produced
  `link_health.status=warning` with `broken_internal_count=2`; the browser UI
  showed the broken-link warning banner and marked the missing leaf link.
  Restore publish cleared diagnostics to `broken_internal_count=0`, validation
  returned `issue_count=0`, the restored leaf route returned HTTP 200, and
  rendered talk attachment links fetched as `text/plain` and `image/png` with
  HTTP 200. Hard delete remains intentionally unsupported in the current CLI:
  `page-delete --mode hard` exits 1 with `unsupported page-delete mode: hard;
  expected tombstone`.
- Latest edit/publish dogfood evidence:
  `test-results/wiki-core-worker-b-edit-publish-20260520T030221Z` created a
  disposable three-page graph (`/alpha-hub`, `/beta-plan`, `/gamma-notes`),
  edited bodies only through hash-checked `onecontext-wiki` operations,
  verified stale-hash rejection with `source_hash_mismatch`, verified
  intentionally ambiguous patch rejection with `body_patch_ambiguous`,
  published a deliberately broken `/missing-ghost` internal link and received
  `next_action="repair_links"`, repaired the body through `page-patch-body`,
  and proved final `validate.status=ok`, `publish-status.render_required=false`,
  clean route manifests, markdown twins, browser-visible routes, and idempotent
  repeated publish with `next_action=none`. No production code patch was needed
  for this slice.
- Latest integrated verification evidence, 2026-05-20T03:12Z:
  after the explicit talk-thread-id patch and the latest dogfood evidence,
  `cargo fmt --check && cargo test -q --workspace`, `uv run --project
  memory-core --with pytest pytest -q memory-core/tests/test_wiki_core_client.py`,
  and `git diff --check` passed. A final `runtime/1Context` scan found no
  disposable `main-agent-flow`, `explicit-new-talk-thread`,
  `worker-c-delete-attachments`, or `agent-churn` strings.
- Latest final verification evidence, 2026-05-20T03:12Z:
  after integrating the edit/publish worker evidence, `cargo fmt --check &&
  cargo test -q --workspace`, `npm test --prefix wiki-engine`, and
  `git diff --check` passed. A final `runtime/1Context` scan found no
  disposable `main-agent-flow`, `explicit-new-talk-thread`,
  `worker-c-delete-attachments`, `agent-churn`, `alpha-hub`, `missing-ghost`,
  or `gamma-notes` strings.
- Latest main concurrent-talk dogfood evidence:
  `test-results/wiki-core-main-talk-concurrency-20260520T031543Z` launched
  48 concurrent `talk-append` CLI processes against `topics`, mixing identical
  and varied subjects but sharing the explicit thread id
  `main-talk-concurrency-thread`. All 48 appends succeeded, produced 48 unique
  message ids, 48 talk files, 48 delivery receipts, 48 mailbox rows, 48
  notifications, and a thread read with 48 messages / 48 deliveries. The
  disposable runtime initially reported `page_needs_publish` warnings from
  safe-created defaults, then publish returned `status=published` with
  `route_count=11`, and final validation returned `status=ok` with
  `issue_count=0`. No code patch was needed for this slice.
- Latest Python-client consumer dogfood evidence:
  `test-results/wiki-core-worker-c-python-client-20260520T031629Z` exercised
  the memory-core Python `WikiCoreClient` against a disposable runtime for
  create/open/write/patch/publish/status/talk/mail operations, producing 23
  receipts plus stale-patch and invalid-mail-read error envelopes. It found a
  small consumer ergonomics issue: `WikiCoreError` preserved the Rust JSON
  envelope, but callers had to dig through nested payload keys to retry well.
  The Python adapter now exposes `operation`, `error_code`, `error_message`,
  and `repair_hints` convenience properties while preserving the original
  `payload`. Worker verification passed with the dogfood script, Python
  boundary/client tests, daemon tests, and `git diff --check`.
- Latest large concurrent-talk dogfood evidence:
  `test-results/wiki-core-worker-a-talk-concurrency-20260520T031609Z`
  launched 96 concurrent `talk-append` CLI processes against `topics`, mixing
  same-subject, lane-subject, and varied-subject messages, delivering each to
  `role://topics.curator`, `role://topics.reviewer`,
  `list://topics.watchers`, `mailbox://page/topics`, and a direct agent
  address, with attachment inputs on every seventh message. Verification
  proved 96 unique message ids, 96 talk markdown files, 480 mailbox deliveries,
  800 notifications, 11 JSONL files parsed cleanly, and same-subject thread
  readback grouped 48 messages. Final disposable publish succeeded and final
  validation returned `status=ok`, `warning_count=0`, and `blocking_count=0`.
  No core or daemon patch was needed.
- Latest nav/sitemap browser dogfood evidence:
  `test-results/wiki-core-worker-b-nav-sitemap-20260520T031632Z` created and
  published five disposable fixture pages: two primary pages, one utility page,
  one hidden page, and one nested `/projects/worker-b-map` page. Playwright
  browser verification passed 86 checks: primary menu order followed
  `nav_order`, the hidden page stayed absent from the menu while its direct
  route worked, utility menu click worked, nested routes and talk routes
  returned HTTP 200, route/link diagnostics stayed clean, and all 10 worker
  page/talk markdown twins fetched as `text/markdown`. No production patch was
  needed.
- Latest integrated verification evidence, 2026-05-20T03:25Z:
  after the concurrent-talk, Python-client, and nav/sitemap dogfood slices,
  `cargo fmt --check && cargo test -q --workspace`, `uv run --project
  memory-core --with pytest pytest -q memory-core/tests/test_wiki_interface_boundary.py
  memory-core/tests/test_wiki_core_client.py`, `npm test --prefix
  wiki-engine`, and `git diff --check` all passed. A final `runtime/1Context`
  scan found no disposable `main-talk-concurrency`,
  `worker-a-talk-concurrency`, `worker-b-nav-sitemap`,
  `worker-c-python-client`, `worker-b-priority`, `worker-b-hidden`, or
  `python-client-dogfood` strings.
- Latest main create-validation dogfood evidence:
  `test-results/wiki-core-main-create-validation-20260520T033238Z` exercised
  page-create failure/recovery against a disposable runtime. Duplicate routes
  failed with `route_already_exists`; missing and escaping template paths now
  fail as `invalid_page_template` with repair hints; invalid `--nav-section
  banana` now fails as `invalid_arguments` with a repair hint and no
  `wiki.toml` or source/talk leak. This fixed a real API-contract bug: the CLI
  advertised `--nav-section primary|utility|hidden`, but the core previously
  accepted arbitrary nav section strings and wrote them into `wiki.toml`.
  Regression proof: `cargo test -q -p onecontext-wiki-daemon
  page_create_rejects_invalid_nav_section_before_mutating`. After a valid page
  create and publish, final validation returned `status=ok` with
  `issue_count=0`.
- Latest mail/tombstone dogfood evidence:
  `test-results/wiki-core-worker-c-mail-tombstone-20260520T033058Z` created a
  disposable page with watcher/list/role agents, appended initial mail, claimed
  the role delivery, snoozed list delivery, archived page-mailbox delivery,
  tombstoned the page, and verified normal talk append is refused with
  `tombstoned_page` unless `--allow-tombstoned` is passed. Existing
  pre-tombstone mail stayed readable and inspectable; restore cleared the
  tombstone state, accepted new mail, and updated inbox/list-status/notification
  state. The run passed without a Rust patch.
- Latest integrated verification evidence, 2026-05-20T03:39Z:
  after the create-validation patch, `cargo fmt --check && cargo test -q
  --workspace`, `uv run --project memory-core --with pytest pytest -q
  memory-core/tests/test_wiki_interface_boundary.py
  memory-core/tests/test_wiki_core_client.py`, `npm test --prefix wiki-engine`,
  `git diff --check`, and the focused invalid-nav regression test passed. A
  final `runtime/1Context` scan found no disposable `main-create-validation`,
  `bad-nav-section`, `worker-c-mail-tombstone`, `worker-a-publish-recovery`, or
  `worker-b-route-conflicts` strings.
- Latest publish recovery dogfood evidence:
  `test-results/wiki-core-worker-a-publish-recovery-20260520T033155Z` proved
  and fixed a last-good publishing bug. Before the fix, a failed render wrote
  directly into `user-wiki/site`, and because the renderer clears its output
  directory first, a frontmatter failure deleted the previous last-good site.
  The daemon publisher now renders into `context-engine/runs/wiki-publish-staging`
  and promotes to `user-wiki/site` only after the renderer succeeds and reports
  `status="published"`. The post-fix proof shows the deliberate frontmatter
  failure returns `status="failed"` while `for-you.html`, `for-you.md`,
  `for-you.talk.html`, `for-you.talk.md`, and the route manifest remain
  present; after repair, forced publish returns `status="published"`,
  `publish-status.render_required=false`, `validate.status=ok`, and browser
  proof fetched 9 HTML routes plus 8 markdown twins with HTTP 200.
- Latest route-conflict dogfood evidence:
  `test-results/wiki-core-worker-b-route-conflicts-20260520T034415Z` exercised
  page id, route, talk-route, source-path, route format, slug, nav section, and
  nav order safety against a disposable runtime. Negative `page-create` runs
  compared before/after `wiki.toml` and `user-wiki/source` hashes. Invalid
  slug/route/nav inputs failed with useful error codes; duplicate id, duplicate
  route, talk-route collision, site-page route conflict, and source-path
  conflict were rejected before partial writes; invalid config validation
  blocked publish with `next_action="repair_wiki_toml"`; valid pages then
  published and browser verification passed.
- Latest main path-safety dogfood evidence:
  `test-results/wiki-core-main-path-safety-20260520T034926Z` extended the
  route-conflict hardening to filesystem path components. Before the patch,
  `page-create --family-group ../outside` and `--family-id ../outside` could
  escape the intended `user-wiki/source/families` path and leave files. The
  Rust core now validates `family_group` and `family_id` as safe lowercase
  filesystem tokens before writing `wiki.toml` or page source/talk files. The
  passing proof shows unsafe family tokens fail with `invalid_page_path` and
  repair hints; `.md` routes and routes with spaces fail with
  `invalid_page_route`; no outside directory or `wiki.toml` leak remains.
- Latest fallback/recovery dogfood evidence:
  `test-results/wiki-core-worker-d-fallback-recovery-20260520T034805Z`
  exercised missing source, missing talk, template-derived,
  edited-from-template, explicit `page-create-all` recovery, and direct
  publish-preflight recovery against a disposable runtime. It found a real
  consumer-shape gap: `wiki.page.status` and `wiki.list` exposed template/edit
  state and `next_action`, but `wiki.page.open` did not. The Rust core now
  returns `template_state`, `flags`, and `next_action` from `wiki.page.open`,
  so an agent opening a page gets the same handoff metadata before deciding
  whether to create from template, patch, publish, or wait. Browser proof
  fetched 11 routes and 11 markdown twins with no console/page errors.
- Latest main open-handoff dogfood evidence:
  `test-results/wiki-core-main-open-handoff-20260520T041057Z` exercised the
  exact agent decision point for `wiki.page.open`: missing configured source,
  template-unedited source, edited-from-template source, missing talk folder,
  tombstoned page, disabled page, and explicit recovery. Assertions passed that
  `safe_to_edit`, `recommended_write_mode`, `template_state`, `flags`, and
  `next_action` line up with the operation an agent should choose. The slice
  also clarified a useful boundary: `page-create-all` recovers missing
  source/talk but does not undo explicit tombstone or disabled config; clean
  recovery required `wiki.page.restore` plus re-enabling the page. Final
  `publish-status` returned `render_required=false`, and final validation
  returned `status=ok`.
- Latest integrated verification evidence, 2026-05-20T03:52Z:
  after the publish-recovery, route-conflict, and path-safety hardening,
  `cargo fmt --check && cargo test -q --workspace`, `uv run --project
  memory-core --with pytest pytest -q memory-core/tests/test_wiki_interface_boundary.py
  memory-core/tests/test_wiki_core_client.py`, `npm test --prefix wiki-engine`,
  and `git diff --check` all passed. The Python client test was updated to
  assert the newer `page_already_exists` error properties when recreating a
  tombstoned page with conflicting placement. A final `runtime/1Context` scan
  found no disposable `main-path-safety`, `path-safety-bad`,
  `worker-a-publish-recovery`, `worker-b-route-conflicts`, or
  `worker-d-fallback-recovery` strings.
- Latest integrated verification evidence, 2026-05-20T04:13Z:
  after the `wiki.page.open` metadata patch, Worker D fallback/recovery
  evidence, and main open-handoff dogfood slice, `cargo fmt --check && cargo
  test -q --workspace`, `uv run --project memory-core --with pytest pytest -q
  memory-core/tests/test_wiki_interface_boundary.py
  memory-core/tests/test_wiki_core_client.py`, `npm test --prefix wiki-engine`,
  `git diff --check`, and a focused diff whitespace check all passed. A final
  `runtime/1Context` scan found no disposable `main-open-handoff`,
  `worker-e-open-status`, `worker-f-browser`, or `worker-g-agent-mail` strings.
- Latest open/status matrix dogfood evidence:
  `test-results/wiki-core-worker-e-open-status-matrix-20260520T040834Z`
  compared `wiki.list`, `wiki.page.status`, `wiki.page.open`, and
  `wiki.validate` across fresh missing source, template-unedited,
  edited-from-template, missing talk, tombstoned, disabled, and invalid-config
  states. It found a real agent-handoff bug: closed pages returned
  `allowed_actions=[wiki.page.restore,wiki.validate]`, but `wiki.page.open`
  recommended `wiki.talk.append`. The main follow-up fix in
  `test-results/wiki-core-main-closed-page-policy-20260520T042207Z` now makes
  tombstoned and disabled pages recommend `wiki.page.restore`, exposes
  top-level `talk_state` on `wiki.page.open`, and maps disabled-page talk
  append failures to `disabled_page` with repair hints. CLI assertions passed
  for tombstoned and disabled open/error envelopes. A second follow-up proof at
  `test-results/wiki-core-main-invalid-config-envelope-20260520T042343Z` maps
  invalid `user-wiki/wiki.toml` TOML parse failures to `invalid_wiki_config`
  with repair hints for both `wiki.validate` and `wiki.page.open`.
- Latest browser relationship dogfood evidence:
  `test-results/wiki-core-worker-f-browser-relationship-crawl-20260520T041019Z`
  created a disposable five-page graph across For You, Projects, Topics,
  Utility, and Hidden routes, appended talk entries, published, served the
  static site, and ran a Playwright crawl. Browser proof passed 165 checks,
  21 routes, and 21 markdown twins. Primary/utility/hidden menu behavior,
  reader links, talk links, Agent view surfaces, direct hidden-route access,
  markdown content types, route manifest, content index, and link diagnostics
  were all coherent. The one broken link was intentional and rendered as a
  warning with a direct 404, not a wrong redirect.
- Latest agent mail/attachment dogfood evidence:
  `test-results/wiki-core-worker-g-agent-mail-attachments-20260520T040903Z`
  registered three agents, created a review list, delivered role/list/page
  mailbox/direct mail with attachments, copied attachments into talk using
  safe relative handles, exercised claim/read/done/archive/snooze paths,
  unsubscribed and retired an agent, refused normal talk append after
  tombstone, allowed explicit archive-maintenance append, and probed missing
  list/agent paths. Final alpha, beta, gamma, and page mailbox actionable mail
  counts were all zero; snoozed list mail stayed hidden by default and visible
  with `--include-snoozed`.
- Latest integrated verification evidence, 2026-05-20T04:25Z:
  after the closed-page policy and invalid-config error-envelope fixes, `cargo
  fmt --check && cargo test -q --workspace`, `uv run --project memory-core
  --with pytest pytest -q memory-core/tests/test_wiki_interface_boundary.py
  memory-core/tests/test_wiki_core_client.py`, `npm test --prefix wiki-engine`,
  `git diff --check`, and a focused diff whitespace check all passed. A final
  `runtime/1Context` scan found no disposable `worker-e-open-status`,
  `worker-f-browser`, `worker-g-agent-mail`, `main-closed-page-policy`,
  `main-invalid-config-envelope`, `worker-f-brief`, `relationship-crawl`, or
  `worker-g-mail-attachments` strings.
- Latest post-fix open-policy dogfood evidence:
  `test-results/wiki-core-worker-h-postfix-open-policy-20260520T043018Z`
  re-ran the open/status matrix after the closed-page and invalid-config fixes.
  Tombstoned and disabled `wiki.page.open` now return top-level
  `talk_state=ready`, recommend `wiki.page.restore`, include restore in
  `allowed_actions`, and omit normal `wiki.talk.append`. Invalid TOML returns
  `invalid_wiki_config` with repair hints across `wiki.validate`,
  `wiki.status`, `wiki.list`, `wiki.page.status`, and `wiki.page.open`. Worker H
  also surfaced a generated `[[site_pages]]` source-page boundary gap: publish
  and reserved routes worked, but source-page commands still reported generic
  `unknown_page` for generated site ids/routes. Main follow-up proof
  `test-results/wiki-core-main-generated-site-page-envelope-20260520T044058Z`
  now maps generated-site page open/status attempts to `generated_site_page`
  with repair hints to inspect publish status or the route manifest instead of
  treating them like editable source pages.
- Latest subscription-cleanup dogfood evidence:
  `test-results/wiki-core-worker-i-subscription-cleanup-20260520T042551Z`
  proved `page-watch` cleanup was too fussy for agents. A page watch creates
  both a default watchers-list subscription and a page-mailbox watcher
  subscription; cleaning only one leaves the page still watched. The Rust core
  now exposes `wiki.page.unwatch`, and `wiki.page.watch` receipts include an
  `unsubscribe_plan` with the exact paired cleanup addresses. Main proof
  `test-results/wiki-core-main-page-unwatch-20260520T043730Z` shows
  `page-unwatch --kind proposal` cancels the exact list and page-mailbox rows
  while leaving a broader page-mailbox watch visible with
  `next_action=mail_subscriptions`; broad `page-unwatch` then clears the
  remaining watch and leaves `mail-subscriptions` at zero.
- Latest delete/restore browser dogfood evidence:
  `test-results/wiki-core-worker-j-delete-restore-browser-20260520T042636Z`
  created linked custom pages, published them, tombstoned a page with inbound
  links, published again, and verified browser-visible behavior against a
  disposable static server. Tombstone publish removed the target page/talk
  routes from manifest and menu, returned 404 without wrong redirects, marked
  inbound links broken in rendered HTML, and reported five broken internal
  links. Restore publish brought the route, menu entry, markdown twins, and
  link diagnostics back to `ok`.
- Latest integrated verification evidence, 2026-05-20T04:45Z:
  after `wiki.page.unwatch`, generated-site-page error envelopes, Python
  adapter wiring, and API/runbook updates, `cargo fmt --check && cargo test -q
  --workspace`, `uv run --project memory-core --with pytest pytest -q
  memory-core/tests/test_wiki_interface_boundary.py
  memory-core/tests/test_wiki_core_client.py`, `npm test --prefix wiki-engine`,
  `git diff --check`, and a focused diff whitespace check all passed. A final
  `runtime/1Context` scan found no disposable `worker-h-postfix`,
  `worker-i-subscription`, `worker-j-delete`, `main-page-unwatch`,
  `main-generated-site-page`, `worker-i-sub-cleanup`, or
  `delete-restore-browser` strings.
- Latest Python page-unwatch dogfood evidence:
  `test-results/wiki-core-worker-k-python-page-unwatch-20260520T044348Z`
  exercised the memory-core Python adapter against a disposable runtime. It
  proved class and module helpers both expose `wiki.page.unwatch`; Python sees
  the `unsubscribe_plan` from `wiki.page.watch`; scoped unwatch with
  `kinds=["proposal"]` cancels the two exact list/page-mailbox watcher rows
  while leaving broader `proposal,question` watcher rows; broad unwatch clears
  the remaining rows; and `generated_site_page` reaches `WikiCoreError` for
  both class and module paths. A focused Python regression test now covers the
  same scoped/broad helper behavior.
- Latest site-page browser/API dogfood evidence:
  `test-results/wiki-core-worker-l-site-pages-browser-api-20260520T044313Z`
  published a disposable runtime with generated site pages and verified `/`,
  `/this-week`, and `/open-questions` in a browser harness. Publish reported
  `site_input_count=3`, `route_count=11`, `markdown_twin_count=11`, route
  manifest and link diagnostics were `ok`, generated page status/open returned
  typed `generated_site_page`, and page-create conflicts against generated
  routes returned `route_already_exists` naming the owning site page. Main
  follow-up `test-results/wiki-core-main-route-source-kind-20260520T045300Z`
  adds `source_kind` route metadata, proving generated site pages report
  `generated_site_page` while source-backed pages report `source_page`.
- Latest concurrent agent lifecycle dogfood evidence:
  `test-results/wiki-core-worker-m-concurrent-agent-lifecycle-20260520T044448Z`
  registered and identified multiple agents concurrently, refreshed a stale
  short-TTL agent, watched/unwatched `topics` concurrently, appended concurrent
  talk fanout, polled/acked notifications, contended claims, marked mail done,
  heartbeated/retired agents, and verified JSONL integrity across 11 files.
  Final checks found no JSONL failures, no bad `next_action`, no notification
  leak to retired agents, no final mail/subscription incoherence, and no
  duplicate active subscription keys after stabilization; contended claim had
  exactly one success and two `mail_already_claimed` errors.
- Latest integrated verification evidence, 2026-05-20T04:55Z:
  after route `source_kind` metadata and the focused Python page-unwatch
  regression landed, `cargo fmt --check && cargo test -q --workspace`,
  `uv run --project memory-core --with pytest pytest -q
  memory-core/tests/test_wiki_interface_boundary.py
  memory-core/tests/test_wiki_core_client.py`, `npm test --prefix wiki-engine`,
  `git diff --check`, and a public runtime leak scan for the latest disposable
  dogfood fixture strings all passed.
- Latest concurrent talk append dogfood evidence:
  `test-results/wiki-core-worker-n-concurrent-talk-20260520T045705Z`
  registered three agents, created and edited a disposable page, then ran 96
  concurrent same-subject/same-thread `talk-append` subprocesses. The pass
  hardened talk writes so append uses the shared mail mutation lock and talk
  source/attachment writes use exclusive create semantics. Post-fix evidence
  showed 96 successful appends, 96 distinct talk source files, 288 deliveries,
  clean JSONL integrity across nine files, `mail-read --thread-id` hydrating 96
  messages, a forced publish rendering all 96 talk markers, and
  `wiki.validate` returning ok.
- Latest retired-agent consistency dogfood evidence:
  `test-results/wiki-core-worker-o-retired-agent-20260520T045712Z` found
  `agent-heartbeat` after retirement returned the less helpful
  `unknown_active_agent` path. The current core gate now matches the rest of
  the active-agent surface: same-thread register, heartbeat, agent inbox,
  notification poll/ack, mail subscribe/unsubscribe, page watch/unwatch, page
  role assign, agent claim, and mail claim all return typed `retired_agent`.
  Post-fix evidence lives in
  `test-results/wiki-core-worker-o-retired-agent-postfix-20260520T050022Z`.
- Latest page-watch concurrency dogfood evidence:
  `test-results/wiki-core-worker-p-page-watch-concurrency-20260520T045647Z`
  ran 64-way and 256-way concurrent `page-watch` calls for the same
  page/agent/list and a raw 256-way `mail-subscribe` fanout. Each path deduped
  to one active list watcher and one active page-mailbox watcher, then
  `page-unwatch`/`mail-unsubscribe` cleaned the surfaces back to zero active
  subscriptions without a source patch.
- Latest integrated verification evidence, 2026-05-20T05:06Z:
  after the concurrent talk write hardening and retired-agent consistency
  proof, `cargo fmt --check && cargo test -q --workspace`,
  `uv run --project memory-core --with pytest pytest -q
  memory-core/tests/test_wiki_interface_boundary.py
  memory-core/tests/test_wiki_core_client.py`, `npm test --prefix wiki-engine`,
  `git diff --check`, and a public runtime leak scan for Worker N/O/P fixture
  strings all passed.
- Latest main-agent browser loop evidence:
  `test-results/wiki-core-main-agent-loop-20260520-051316` used the Rust CLI as
  an ordinary wiki-writing agent: create a custom page at a primary
  `/projects/...` route, open it, hash-check write and patch the body, register
  an agent, watch the page, append proposal mail to the page mailbox, inspect
  agent inbox and `mail-read`, publish, validate, and check publish status.
  The loop deliberately hit two useful repairable errors first:
  `invalid_page_route` for uppercase timestamp route segments and
  `invalid_agent_address` when using an agent id as a mail address. The
  corrected loop ended with source and talk routes in the manifest,
  `source_kind=source_page`, `talk_source_kind=talk_page`, two mail
  deliveries, `validate.status=ok`, `publish_status.next_action=none`, and an
  in-app browser proof that the rendered page and talk route load with the
  expected headings and talk message.
- Latest source-only talk hydration proof:
  `test-results/wiki-core-worker-q-file-only-thread-20260520T051205Z`
  confirmed the current core already hydrates legacy file-only talk parents in
  `mail-read --thread-id` beside delivered replies. Explicit-thread,
  derived-thread, and direct thread-target cases all returned
  `message_count=2`, `delivery_count=1`, and the source-only parent with
  `deliveries=[]`, so agents keep parent context without pretending old
  Markdown-only talk had mailbox delivery rows.
- Latest branchable link-repair lifecycle proof:
  `test-results/wiki-core-worker-r-repair-story-20260520T051253Z` dogfooded
  delete -> publish warning -> repair -> clean and tightened receipt shape.
  `link_repair_lifecycle` now includes `branch.state`, `branch.terminal`,
  `branch.next_command`, and `branch.followup_commands`, progressing
  `publish_then_repair_links -> repair_links -> clean`. Focused daemon
  lifecycle tests passed, and the disposable dogfood loop ended with final
  validate, page status, and publish status all clean.
- Latest grown-wiki browser proof:
  `test-results/wiki-core-worker-s-grown-wiki-20260520T051251Z` built a fuller
  disposable site with topics, projects, custom nested pages, generated site
  pages, page relationships, talk routes, menu sections, and markdown twins.
  Publish produced 35 routes and 35 markdown twins. Browser verification ran
  111 checks with zero console errors, page errors, or request failures,
  verified menu groups/dropdowns, generated routes, nested section routes, talk
  toggles/routes, markdown twins, and confirmed the expected broken
  `/topics/ghost-signal` link is flagged while direct navigation returns a
  real 404 instead of a wrong redirect.
- Latest integrated verification evidence, 2026-05-20T05:19Z:
  after the main-agent loop, Worker Q/R/S evidence, and branchable
  link-repair lifecycle update, `cargo fmt --check && cargo test -q
  --workspace`, `uv run --project memory-core --with pytest pytest -q
  memory-core/tests/test_wiki_interface_boundary.py
  memory-core/tests/test_wiki_core_client.py`, `npm test --prefix wiki-engine`,
  `git diff --check`, and a public runtime leak scan for this round's
  disposable fixture strings all passed.
- Latest stale-edit recovery evidence:
  `test-results/wiki-core-main-stale-recover-20260520-052917` dogfooded two
  agents opening the same page hash. Agent A wrote the page, Agent B's stale
  full write failed with typed `source_hash_mismatch`, and the daemon error
  envelope now carries structured `error.details.page`,
  `expected_source_sha256`, and `found_source_sha256` instead of requiring an
  agent to parse the message string. Agent B followed the repair hint, reopened
  the page, hash-checked a narrow patch, published, and ended with
  `validate.status=ok`, `page_state=rendered`, and the route present.
- Latest notification liveness evidence:
  `test-results/wiki-core-worker-t-liveness-20260520T052930Z` reproduced a
  mismatch where an agent with an expired mail subscription saw
  `mail-subscriptions=0` and empty `agent-inbox` but still received old
  `notify-poll` wakeups. The current core filters notification polling through
  the agent's active owned addresses and active subscriptions. Post-fix proof
  `test-results/wiki-core-worker-t-liveness-20260520T053236Z` shows the
  expired-subscription agent with `subscriptions_after=0`,
  `agent-inbox.message_count=0`, and `notify-poll.notification_count=0`, while
  stale-agent control surfaces still point to `agent-identify`.
- Latest tombstone/archive/restore evidence:
  `test-results/wiki-core-worker-u-tombstone-archive-20260520T052647Z`
  created a custom page with watcher/list/role state, appended talk, tombstoned
  it, verified normal tombstoned talk is refused with `tombstoned_page`,
  appended explicit archive-maintenance talk with `--allow-tombstoned`,
  published the tombstone, and verified browser-visible 404/no menu entry.
  Restore publish brought page and talk routes plus the menu entry back; final
  state was `rendered`, `tombstoned=false`, active watcher `1`, page/list/role
  subscriptions coherent, `wiki.validate=ok`, and `wiki.status=idle`.
- Latest graph repair browser evidence:
  `test-results/wiki-core-worker-v-graph-repair-20260520T052704Z` built a
  topic/project/tool/person-style graph with mixed route and markdown links,
  published clean, deleted `/tools/worker-v-link-lens`, and saw delete impact
  `inbound_link_count=3` across `route` and `markdown_twin` targets. Tombstone
  publish returned `next_action=repair_links`; the worker repaired all three
  source pages via `page-open` hashes and `page-patch-body`, then final
  publish/validate returned `next_action=none`, link diagnostics `ok`, and
  `broken_internal_count=0`, with browser screenshots for initial, warning,
  and final clean states plus direct 404/no-redirect checks.
- Latest integrated verification evidence, 2026-05-20T05:36Z:
  after the structured stale-edit details, notify-poll liveness fix, and Worker
  U/V browser evidence, `cargo fmt --check && cargo test -q --workspace`,
  `uv run --project memory-core --with pytest pytest -q
  memory-core/tests/test_wiki_interface_boundary.py
  memory-core/tests/test_wiki_core_client.py`, `npm test --prefix wiki-engine`,
  `git diff --check`, and a public runtime leak scan for this round's fixture
  strings all passed.
- Latest alias/API freeze-readiness evidence:
  `test-results/wiki-core-main-alias-api-20260520-054035` used only `wiki-*`
  alias commands for page create/open/write/patch, agent identify, page watch,
  talk append, mail read, agent inbox, publish, status, list, and validate.
  Every receipt kept the canonical API operation token, with zero operation
  mismatches; the page rendered at `/reference/...`, `wiki.validate` returned
  ok, and page status showed coherent page-mailbox/watch pressure. Worker W's
  broader `test-results/wiki-core-worker-w-api-cli-20260520T054119Z` pass ran
  42 commands across help, aliases, page lifecycle, mail, notifications,
  publish, and a Python adapter smoke test, finding only one stale API doc
  example: `wiki.page.open` now correctly documents `operation:
  "wiki.page.open"` instead of `wiki.page.status`.
- Latest mailbox delivery semantics evidence:
  `test-results/wiki-core-mailbox-delivery-semantics-20260520T054407Z`
  exercised page roles, page-associated lists, page mailbox subscriptions,
  kind filters, notify ack, claim, done, snooze, and audit reads. Before
  action, page mail had nine actionable deliveries from three talk messages;
  curator and reviewer views matched their role/list/page subscriptions and
  kind filters. Final default pressure was zero for page, curator, reviewer,
  and notify-poll, while `--include-snoozed` preserved review audit visibility
  and ledgers retained claims, deliveries, notifications, and ack attempts.
- Latest route-local talk attachment diagnostics evidence:
  `test-results/wiki-core-worker-y-link-edge-browser-20260520T054111Z`
  found and fixed a renderer diagnostics bug: talk pages render with
  route-local `<base href=".../talk/">`, but post-render link diagnostics were
  resolving relative `attachments/evidence.txt` links from the canonical
  `.talk.html` filepath and falsely flagging them broken. The renderer now
  uses the rendered base href when normalizing relative internal links. The
  restored fixture ended with eight routes, eight markdown twins, link
  diagnostics ok, and browser evidence of 40 checks, five screenshots, zero
  page errors, and zero request failures; the tombstoned fixture still reported
  expected broken links and 404/no wrong redirect behavior.
- Latest integrated verification evidence, 2026-05-20T05:53Z:
  after alias/API evidence, mailbox semantics evidence, the API doc correction,
  and the route-local talk attachment diagnostics fix, `cargo fmt --check &&
  cargo test -q --workspace`, `uv run --project memory-core --with pytest
  pytest -q memory-core/tests/test_wiki_interface_boundary.py
  memory-core/tests/test_wiki_core_client.py`, `npm test --prefix wiki-engine`,
  `git diff --check`, and a public runtime leak scan for this round's fixture
  strings all passed.
- Latest RuntimeDefaults/backfill production-shape evidence:
  `test-results/wiki-core-main-runtime-defaults-20260520-060041` and Worker
  AA's `test-results/wiki-runtime-worker-aa-20260520T060108Z` proved the
  packaged defaults path against disposable homes. Fresh install copied
  packaged defaults with no proposals, preserve-user-edit produced
  `installed_with_conflicts` and wrote the expected
  `context-engine/proposals/wiki/runtime-defaults/user-wiki__wiki.toml.proposal.json`,
  and custom page fallback rendered source/talk routes from user data. Worker
  AA also caught and fixed a package leak: generated
  `context-engine/runs/wiki-publish-result.json` was being bundled into
  RuntimeDefaults. `scripts/build-macos-app.sh` now scrubs generated run
  receipts before manifesting/bundling, package smoke passed after rebuild,
  and `dist/.../RuntimeDefaults/1Context/context-engine/runs` is empty.
- Latest generated site-page boundary evidence:
  `test-results/wiki-core-worker-z-site-page-boundaries-20260520T060230Z`
  published generated site pages and verified `/`, `/this-week`, and
  `/open-questions` render with markdown twins while route/content metadata
  marks them `source_kind=generated_site_page`. Source-backed pages remain the
  only `wiki.list` rows. Open/status/write/delete against generated page ids or
  routes return typed `generated_site_page`. The pass found and fixed one
  boundary leak: `page-create home` could create a source-backed `/home` page
  even though `home` was already a generated `[[site_pages]]` id. It now
  returns typed `generated_site_page` with the generated-page repair hint and
  leaves no source/talk artifacts.
- Latest Python-as-agent adapter evidence:
  `test-results/worker-ab-python-adapter-dogfood-20260520T000000Z` used the
  memory-core Python adapter as an actual agent client across page
  create/open/write/patch/delete/restore, agent register/identify/status,
  heartbeat/retire, watch/unwatch, list/subscription APIs, talk append/reply
  with attachment, inbox/read/claim/mark/mark-all/snooze, notification
  poll/ack, publish/status/validate, and structured error handling. It
  produced 52 successful receipts and four expected typed errors:
  stale write and stale patch as `source_hash_mismatch`, generated page status
  and open as `generated_site_page`.
- Latest integrated verification evidence, 2026-05-20T06:12Z:
  after RuntimeDefaults scrub, generated site-page create protection, Python
  adapter dogfood, and cleanup of stale goal notes, `cargo fmt --check &&
  cargo test -q --workspace`, `uv run --project memory-core --with pytest
  pytest -q memory-core/tests/test_wiki_interface_boundary.py
  memory-core/tests/test_wiki_core_client.py`, `npm test --prefix wiki-engine`,
  `git diff --check`, and leak scans over public runtime plus bundled
  RuntimeDefaults run receipts all passed.
- Latest package accountability dogfood evidence:
  `test-results/wiki-core-worker-ac-package-accountability-20260520T061718Z`
  re-ran package smoke and used the packaged Rust core to publish a disposable
  copied runtime. It found the useful gap: the RuntimeDefaults manifest was
  proving the source checkout/build-time core more clearly than the exact
  packaged payload. The current build now writes the manifest after bundled
  helper executables are signed and before the final app signature, using the
  stripped `Contents/Resources/WikiEngine` tree and signed
  `Contents/MacOS/onecontext-wiki` helper. `scripts/test-launch-agent-package.sh`
  now recomputes manifest tree/file hashes against the built app so this cannot
  silently drift again.
- Latest identity/mail lifecycle evidence:
  `test-results/wiki-core-worker-ad-identity-mail-20260520T061711Z` covered
  register/identify same thread, duplicate register refusal, retired thread
  refusal, new thread after retire, stale lease identify refresh, `agent_id`
  mail-address misuse refusal, direct primary-address mail, role mail,
  notification poll/ack, thread reads, and JSONL audit visibility. Focused
  `cargo test -p onecontext-wiki-daemon` passed.
- Latest docs/API freeze-readiness evidence:
  Worker AE's doc audit removed stale consumer-facing `materialize` language in
  favor of `page-create`, `wiki.publish`, and page lifecycle wording; refreshed
  generated-site-page, RuntimeDefaults run-receipt scrub, notify-poll liveness,
  and route-local talk attachment diagnostics docs; and left only the removed
  historical helper filename as a `materialize` audit hit. Targeted doc
  whitespace checks passed.
- Latest integrated verification evidence, 2026-05-20T06:26Z:
  after the packaged manifest coherence fix, identity/mail lifecycle dogfood,
  and doc freeze-readiness pass, a fresh dev app build completed in `17.95s`.
  `scripts/test-launch-agent-package.sh` passed with manifest recomputation
  evidence in `test-results/wiki-core-main-package-manifest-20260520T062514Z`;
  `cargo fmt --check`, `cargo test -q --workspace`, `uv run --project
  memory-core --with pytest pytest -q
  memory-core/tests/test_wiki_interface_boundary.py
  memory-core/tests/test_wiki_core_client.py`, `npm test --prefix wiki-engine`,
  `git diff --check`, and leak scans over public runtime plus bundled
  RuntimeDefaults run receipts all passed.
- Latest main-agent talk/publish boundary evidence:
  `test-results/wiki-core-main-talk-publish-20260520T063252Z` created a
  three-page project/topic/tool cluster with cross-links, published it, then
  appended talk/mail against the project page. `publish-status` correctly moved
  to `next_action=publish` after source writes, returned to `none` after the
  content publish, and stayed `none` after talk/mail-only work. Page status
  still exposed one actionable page-mail item, role inbox and agent inbox each
  saw the message, `wiki.validate` returned ok, and in-app browser verification
  loaded the rendered page plus talk shell from the disposable static site.
- Latest graph/menu/browser evidence:
  `test-results/wiki-core-worker-af-page-graph-menu-20260520T063156Z` created
  five linked pages across project, topic, custom, utility, and hidden routes,
  appended talk to each, published, and browser-checked menu/dropdown and
  article-link navigation. Publish produced 21 routes and 21 markdown twins;
  Worker AF contributed 10 page/talk routes; link diagnostics were clean; and
  the browser pass recorded 60 checks with zero console, page, or request
  errors. The only captured failure was useful agent ergonomics: using an
  `agent_id` as a mail address was rejected with a repair hint, and retrying
  with the primary address succeeded.
- Latest talk inbox/address fanout evidence:
  `test-results/wiki-core-worker-ag-talk-address-20260520T063558Z` registered
  four agents, created two pages and two lists, exercised subscriptions,
  `page-watch`, page roles, parent/reply talk with attachments, inbox reads,
  claim/mark/snooze, notification poll/ack, and JSONL integrity. The pass
  ended with six subscriptions, seven deliveries, ten notification outbox
  records, five claim/mark rows, two-message thread hydration with attachment
  counts `2` and `1`, and valid JSONL across agent directory, list,
  subscription, delivery, claim, notification, page-ledger, and mailbox files.
- Latest delete/restore link-impact evidence:
  `test-results/wiki-core-worker-ah-publish-delete-restore-20260520T063245Z`
  found and fixed a real delete lifecycle gap: `page-delete` counted inbound
  links in source page bodies but missed source-backed talk Markdown. The core
  now includes `.md` files under each live page's talk folder in pre-delete
  link impact and has targeted coverage in
  `page_delete_link_impact_includes_talk_markdown_links`. The fixture's delete
  impact now reports six inbound links, including the talk route issue with
  `phase=pre_delete_talk_link_scan`; after repair, tombstoned publish is clean,
  restore removes the tombstone, restored publish is clean, and browser proof
  in `browser-finish-evidence.json` confirms the tombstoned-repaired target
  route stays a true 404/no-redirect while restored page and talk routes load.
- Latest integrated verification evidence, 2026-05-20T06:46Z:
  after the talk/publish boundary proof, Worker AF/AG dogfood, and Worker AH's
  talk-link delete impact fix, `cargo fmt --check`, `cargo test -q
  --workspace`, `uv run --project memory-core --with pytest pytest -q
  memory-core/tests/test_wiki_interface_boundary.py
  memory-core/tests/test_wiki_core_client.py`, `npm test --prefix wiki-engine`,
  and `git diff --check` passed. A fresh dev app rebuild completed in `16.83s`
  and `scripts/test-launch-agent-package.sh` passed with package manifest
  evidence in `test-results/wiki-core-main-post-ah-package-20260520T064540Z`.
  Public runtime and bundled RuntimeDefaults leak scans for this round's
  disposable fixture strings returned no matches, and bundled
  `context-engine/runs` remained empty.
- Latest fallback/backfill dogfood evidence:
  `test-results/wiki-core-main-fallback-recovery-20260520065311` deleted a
  configured custom page source and talk folder from a disposable runtime, then
  proved `publish-status`, `wiki.validate`, `page-status`, and `page-open`
  all pointed to `next_action=publish`. `wiki.publish` recovered the page from
  the configured template, recreated talk/curator/conventions files, rendered
  13 routes and 13 markdown twins, and ended with `publish-status.next_action=none`
  and `validate.status=ok`. The pass found and fixed stale consumer wording:
  publish preflight receipts now say `action=backfill_configured_pages`, and
  validation/policy text says configured pages are backfilled rather than
  materialized.
- Latest sitemap/attachment/mail dogfood evidence:
  Worker AI's
  `test-results/wiki-core-worker-ai-sitemap-placement-20260520T065031Z` passed
  80 checks for primary/utility/hidden custom placement, visible menu ordering,
  hidden-page menu exclusion, route manifest entries, and negative duplicate
  route/id/nav probes without mutating the disposable runtime on failure.
  Worker AJ's `test-results/worker-aj-attachments-20260520T065046Z` proved
  rendered page/talk routes and markdown twins return attachment links with
  zero missing files and zero local path leaks, with browser route fetches
  returning 200 for all checked surfaces. Worker AK's
  `test-results/wiki-core-worker-ak-notify-mail-lifecycle-20260520T065005Z`
  ran 63 CLI operations across registration, leases, page/list roles,
  talk fanout, inbox reads, claim/mark/snooze/archive/done, notification
  poll/ack, stale expiry, and retirement; final page/list actionable pressure
  was zero while retired/stale agent liveness stayed inspectable.
- Latest RuntimeDefaults freshness evidence, 2026-05-20T07:04Z:
  the package smoke caught that bundling RuntimeDefaults through the renderer
  directly left `user-wiki/site` present but `wiki.publish.status` reported
  `render_required=true` because the Rust freshness markers were missing. The
  app build now creates bundled defaults through `onecontext-wiki publish`,
  copies the sanitized render result for the RuntimeDefaults manifest, then
  scrubs `context-engine/runs` before packaging. `scripts/test-launch-agent-package.sh`
  now asserts `source-fingerprint.txt`, `page-fingerprints.json`, and packaged
  `onecontext-wiki --root .../RuntimeDefaults/1Context publish-status` returning
  `render_required=false`, `site_needs_publish=false`, and `next_action=none`.
- Latest integrated verification evidence, 2026-05-20T07:05Z:
  after the fallback wording patch and RuntimeDefaults freshness fix, `cargo
  fmt --check`, `cargo test -q --workspace`, `uv run --project memory-core
  --with pytest pytest -q memory-core/tests/test_wiki_interface_boundary.py
  memory-core/tests/test_wiki_core_client.py`, `npm test --prefix wiki-engine`,
  `bash -n scripts/build-macos-app.sh scripts/test-launch-agent-package.sh`,
  `git diff --check`, and stale `materialize_configured_pages` wording scans
  passed. A fresh dev app rebuild completed in `8.55s`; the hardened package
  smoke passed with evidence in
  `test-results/wiki-core-main-post-fallback-20260520T070020Z`, packaged
  RuntimeDefaults publish status is idle/clean, bundled
  `context-engine/runs` is empty, and public runtime plus bundled defaults leak
  scans for this round's disposable fixture strings returned no matches.
- Latest packaged-agent route metadata evidence:
  `test-results/wiki-core-main-packaged-agent-20260520071407` used the
  packaged `dist/1Context.app/Contents/MacOS/onecontext-wiki` helper from
  outside the repo so it resolved the bundled `Resources/WikiEngine`. Starting
  from bundled RuntimeDefaults, it proved the packaged defaults are initially
  idle, rejected an uppercase route with `invalid_page_route`, created and
  edited `/projects/packaged-agent-20260520071407`, published content cleanly,
  appended page/role talk mail with an attachment, confirmed talk/mail did not
  require publish, hydrated the focused mail thread, force-refreshed rendered
  talk, and ended with `validate.status=ok` and `publish-status.next_action=none`.
  The pass exposed and fixed a consumer metadata gap: route manifest and
  content-index entries now carry canonical `page_id` plus `talk_for`, so
  agents do not have to infer page identity from rendered slugs.
- Latest RuntimeDefaults upgrade scenario evidence:
  `test-results/wiki-core-worker-al-runtime-defaults-20260520T071241Z`
  added and proved an app-update scenario covering v1 to v2 packaged defaults.
  It backfilled a missing default, generated conflict proposals for changed
  bundled defaults, preserved edited `user-wiki/wiki.toml`, preserved a
  user-authored custom source and talk curator file, rendered the upgraded
  home with trigger `runtime-test.app-upgrade`, and kept `runtime/1Context`
  checksums identical before/after. The Swift scenario harness passed all four
  cases: `fresh-user`, `preserve-user-edit`, `custom-page`, and
  `app-upgrade-user`.
- Latest inbox/notification pressure evidence:
  `test-results/wiki-core-worker-am-dogfood-20260520T071553Z` ran 86 checks
  with zero failures across four agents, short leases, one stale agent, one
  retired agent, topic/project pages, watcher and reviewer lists, page roles,
  parent/reply talk with attachments, focused thread reads, shared claims,
  competing-claim refusal, snooze/done/archive, notification poll/ack, and
  negative probes for bad list addresses, duplicate register, missing reply
  target, and snooze without `--until`. Final pressure remained inspectable:
  two active agents, one stale/expired, one retired, seven topic mail
  deliveries, one open thread, and archived/snoozed reviewer-list work visible
  in audit mode.
- Latest renderer/browser graph evidence:
  `test-results/wiki-renderer-worker-an-dogfood-20260520T071509Z` built a
  disposable grown graph with two generated site pages, 11 source pages, two
  talk folders, attachments/media, hidden pages, menu navigation, Reader/Agent
  and Talk routes, markdown twins, and a tombstone/restore cycle. It found and
  fixed a generated-site-page bug: nested `[[site_pages]]` routes like
  `/system/status` used to place `system/status` into frontmatter `slug`,
  violating slug validation. Generated site pages now keep a safe page slug
  while preserving the nested route output stem. Browser proof passed two
  Playwright tests; restored publish returned 21 routes / 21 markdown twins
  with link status `ok`, tombstone publish produced the expected warning/404
  state, and restore returned to clean link diagnostics.
- Worker AT renderer/browser graph evidence:
  `test-results/wiki-core-worker-at-20260520T073327Z` proved a disposable graph
  with generated root/nested site pages, configured source pages, nested
  topic/project/work routes, page and section talk routes, hidden direct-route
  pages, text/PNG talk attachments, route-manifest/content-index metadata,
  markdown twins, menu links, Agent view, tombstone warning/404 behavior, and
  restore back to clean link diagnostics. No renderer patch was needed; final
  fixture checks passed `41/41`, and browser checks passed `14/14`.
- Latest integrated verification evidence, 2026-05-20T07:24Z:
  after the route-manifest `page_id` fix, nested generated-site-page slug fix,
  RuntimeDefaults upgrade scenario, and stale wiki-side `materialize` wording
  cleanup, `cargo fmt --check`, `cargo test -q --workspace`, `npm test
  --prefix wiki-engine`, `uv run --project memory-core --with pytest pytest -q
  memory-core/tests/test_wiki_interface_boundary.py
  memory-core/tests/test_wiki_core_client.py`, `./scripts/test-wiki.sh`,
  `./scripts/test-wiki-runtime-defaults-scenarios.sh`, `bash -n` on packaging
  and wiki scripts, and `git diff --check` all passed. A fresh dev app rebuild
  completed in `8.28s`, package smoke passed, packaged RuntimeDefaults
  publish status is idle/clean, bundled `context-engine/runs` is empty, wiki
  surface `materialize` wording scan returned no matches, and public runtime
  plus bundled defaults leak scans for this round's disposable fixture strings
  returned no matches.
- Latest route metadata and bridge dogfood evidence, 2026-05-20T07:43Z:
  `test-results/wiki-core-main-route-metadata-20260520T073134Z` created
  disposable source/target pages whose `page_id`, route, and slug intentionally
  differed, then proved route-manifest/content-index entries preserve canonical
  `page_id`, talk entries carry `talk_for`, tombstoning the linked target
  produces a canonical `repair_links` diagnostic for the source page, restore
  returns link diagnostics to `ok`, and browser-visible source/target/talk
  routes load. This pass fixed the remaining `page-open` ergonomics gap:
  `wiki.page.open` now returns top-level `title`, `route`, `collection`, and
  `type`, plus nested `page_status`, so agents no longer infer placement from
  `handles.published`. Worker AR's
  `test-results/wiki-core-worker-ar-20260520T073305Z` proved the Swift process
  bridge preserves typed Rust-core JSON errors, exposes `wiki.publish.status`
  over daemon JSON-RPC, and keeps talk/mail pressure from forcing publish.
  Worker AS's `test-results/wiki-core-worker-as-20260520T073122Z` added a
  Python adapter lifecycle regression for register/create/open/write/patch,
  content publish, talk/mail/notify, tombstone, restore, and typed patch
  errors. Worker AT's
  `test-results/wiki-core-worker-at-20260520T073327Z` proved the renderer graph
  again with final fixture checks `41/41` and browser checks `14/14`.
- Latest integrated verification evidence, 2026-05-20T07:43Z:
  after the `page-open` receipt shape fix, Swift bridge JSON-RPC publish-status
  path, Python adapter lifecycle test, and renderer dogfood pass, `cargo fmt
  --check`, `cargo test -q --workspace`, `uv run --with pytest --project
  memory-core pytest memory-core/tests/test_wiki_core_client.py
  memory-core/tests/test_wiki_interface_boundary.py -q`, `npm test --prefix
  wiki-engine`, `swift test --package-path macos --filter
  WikiCoreProcessClientTests`, `swift test --package-path macos --filter
  OneContextWikiRuntimeTests`, `./scripts/test-wiki.sh`,
  `./scripts/test-wiki-runtime-defaults-scenarios.sh`, `bash -n` on packaging
  and wiki scripts, `git diff --check`, and stale `page-open` wording scans
  passed. The real browser contract passed with
  `browser_contract_base_url=http://127.0.0.1:58953`; RuntimeDefaults scenario
  summary was written to
  `/tmp/1ctx-runtime-defaults-scenarios/runtime-defaults-scenarios-summary.json`.
- Latest daemon lifecycle and rendered ergonomics evidence, 2026-05-20T08:02Z:
  `test-results/wiki-core-main-daemon-rpc-20260520T074856Z` and
  `test-results/wiki-core-main-daemon-rpc-after-au-20260520T080133Z` proved a
  live debug `1contextd` Unix-socket flow for `wiki.list`,
  `wiki.publish.status`, `wiki.page.create`, `wiki.page.open`,
  `wiki.page.write_body`, `wiki.page.patch_body`, `wiki.page.status`,
  `wiki.page.delete`, and `wiki.page.restore`, plus CLI wrappers for
  `1context wiki page-open` and `1context wiki publish-status`. The proof
  caught an operational wart: long disposable `test-results/...` runtime homes
  exceed macOS `sockaddr_un` path length, so daemon dogfood must use a short
  `/tmp` runtime home or a shorter socket path. Worker AU's
  `test-results/wiki-core-worker-au-20260520T075254Z` patched lifecycle
  receipts so create/write/patch/delete/restore expose top-level `route`,
  `type`, and `collection`, and so `page-delete` reports
  `next_action="repair_links"` when inbound links are known. Worker AV's
  `test-results/wiki-core-worker-av-20260520T075102Z` proved mail/list
  threads survive tombstone and restore while publish pressure stays
  content-scoped. Worker AW's
  `test-results/wiki-core-worker-aw-20260520T074758Z` fixed renderer/server
  ergonomics for root `/talk`, nested talk markdown twins, copied talk
  attachments, route-derived `route_index_path`, static `/api/wiki/search`,
  and root Talk button navigation.
- Latest integrated verification evidence, 2026-05-20T08:02Z:
  after the daemon lifecycle bridge, operation receipt identity fields,
  delete repair next-action, mail/list tombstone regression, and renderer
  root/nested talk fixes, `cargo fmt --check`, `cargo test -q --workspace`,
  `swift test --package-path macos --filter OneContextWikiRuntimeTests`,
  `swift build --package-path macos --product 1context`, `swift build
  --package-path macos --product 1contextd`, `uv run --with pytest --project
  memory-core pytest memory-core/tests/test_wiki_core_client.py
  memory-core/tests/test_wiki_interface_boundary.py -q`, `npm test --prefix
  wiki-engine`, `./scripts/test-wiki.sh`,
  `./scripts/test-wiki-runtime-defaults-scenarios.sh`, `bash -n` on packaging
  and wiki scripts, `git diff --check`, and stale daemon/page-open wording
  scans passed. The browser contract passed at
  `browser_contract_base_url=http://127.0.0.1:61289`; RuntimeDefaults scenario
  summary remained
  `/tmp/1ctx-runtime-defaults-scenarios/runtime-defaults-scenarios-summary.json`.
- Latest explicit daemon publish and app-mirror evidence, 2026-05-20T08:25Z:
  `test-results/wiki-core-main-daemon-publish-current-20260520T081853Z`
  proved a live debug `1contextd` can create a nested page, write its body,
  report `wiki.publish.status.next_action=publish`, run explicit synchronous
  `wiki.publish`, and return a receipt with `app_publish.status="published"`.
  The proof verified both `user-wiki/site` and
  `Application Support/1Context/wiki-site/current` contain the new route,
  markdown twin, talk route, and no duplicate `*.talk/index.html` route
  directory. The CLI wrapper `1context wiki publish --trigger
  cli-publish-current-proof` also succeeded through the daemon and returned an
  app-mirror receipt. Browser proof in the same evidence folder loaded the
  app-visible proof route, content links, Talk control, `/topics`, direct talk
  route, and static search result. Earlier failed expectation evidence in
  `test-results/wiki-core-main-daemon-publish-20260520T081120Z` is retained:
  it showed the markdown twin convention is `<route>.md`, not
  `<route>/index.md`.
- Latest worker dogfood evidence, 2026-05-20T08:25Z:
  Worker AX's `test-results/wiki-core-worker-ax-20260520T080934Z` dogfooded
  Rust create/open/hash-write/patch/publish/force-publish/delete/restore and
  found no Rust patch needed. Worker AY's
  `test-results/wiki-core-worker-ay-20260520T080937Z` dogfooded the Python
  adapter through 43 operations and recorded adapter ergonomics: publish is
  present, but Python still wants file-backed body helpers and typed request
  objects. Worker AZ's `test-results/wiki-core-worker-az-20260520T010803`
  patched renderer ergonomics so talk notes stay near the talk header and
  duplicate `/index/` / `*.talk/` route directories are no longer emitted, with
  browser/search/menu screenshots captured in that folder.
- Latest integrated verification evidence, 2026-05-20T08:25Z:
  after explicit daemon `wiki.publish`, app-mirror publication, daemon
  publication serialization, CLI publish wrapper, docs refresh, and Worker AZ's
  renderer cleanup, `cargo fmt --check`, `cargo test -q --workspace`,
  `npm test --prefix wiki-engine`, `uv run --with pytest --project
  memory-core pytest memory-core/tests/test_wiki_core_client.py
  memory-core/tests/test_wiki_interface_boundary.py
  memory-core/tests/test_wiki_authoring_facade.py -q`, `swift test
  --package-path macos --filter OneContextWikiRuntimeTests`, `swift build
  --package-path macos --product 1context`, `swift build --package-path macos
  --product 1contextd`, `./scripts/test-wiki.sh`,
  `./scripts/test-wiki-runtime-defaults-scenarios.sh`, `git diff --check`, and
  stale daemon-publish/materializer wording scans passed. The real browser
  contract passed at `browser_contract_base_url=http://127.0.0.1:63723`; the
  RuntimeDefaults scenario summary remained
  `/tmp/1ctx-runtime-defaults-scenarios/runtime-defaults-scenarios-summary.json`.
- Latest daemon collaboration-surface evidence, 2026-05-20T08:35Z:
  `test-results/wiki-core-main-daemon-collab-20260520T083045Z` proved a live
  debug `1contextd` can create and publish a page, identify sender and curator
  agents, list/status/inbox those agents, append a talk proposal through
  daemon JSON-RPC, deliver direct and role mail, read the message, claim it,
  mark one delivery done, mark all deliveries done, poll/ack notifications,
  and force-publish only for browser-visible talk proof. The proof captured an
  important consumer footgun in
  `test-results/wiki-core-main-daemon-collab-20260520T082756Z`: `agent_id` is
  not a mail address; callers must use the returned `primary_address` or
  `addresses[0]` for direct delivery. The final summary shows
  `talk_did_not_require_publish=true`, so talk/mail work creates inbox and
  notification pressure without making content publish dirty. Browser proof in
  the final evidence folder loaded the app-visible page route, content links,
  Talk control, talk message, and static search result.
- Latest worker collaboration evidence, 2026-05-20T08:35Z:
  Worker BA's `test-results/wiki-core-worker-ba-20260520T082958Z` added
  bridge coverage for API-shaped nested `page` / `message` talk params,
  attachment objects, snooze aliases, and `notify.ack` `id`. Worker BB's
  `test-results/wiki-core-worker-bb-20260520T082628Z` added Python
  `body_file`, `find_file`, and `replace_file` support for page write/patch
  helpers, with focused pytest coverage. Worker BC's
  `test-results/wiki-core-worker-bc-20260520T082616Z` rendered a fuller
  mail-like graph with talk attachments and proved `/mail/today/talk`,
  `/mail/today.talk.md`, the attachment route, menu, search, and Agent view in
  browser DOM snapshots without needing renderer patches.
- Latest integrated verification evidence, 2026-05-20T08:35Z:
  after daemon agent/talk/mail/notify JSON-RPC exposure, nested bridge params,
  Python file-backed edit helpers, and the collaboration dogfood pass, `swift
  test --package-path macos --filter WikiCoreRPCBridgeTests`, `swift test
  --package-path macos --filter OneContextWikiRuntimeTests`, `swift build
  --package-path macos --product 1context`, `swift build --package-path macos
  --product 1contextd`, `cargo fmt --check`, `cargo test -q --workspace`,
  `uv run --with pytest --project memory-core pytest
  memory-core/tests/test_wiki_core_client.py
  memory-core/tests/test_wiki_interface_boundary.py
  memory-core/tests/test_wiki_authoring_facade.py -q`, `npm test --prefix
  wiki-engine`, `./scripts/test-wiki.sh`,
  `./scripts/test-wiki-runtime-defaults-scenarios.sh`, `git diff --check`, and
  stale daemon-collaboration/materializer wording scans passed. The real
  browser contract passed at `browser_contract_base_url=http://127.0.0.1:65342`;
  the RuntimeDefaults scenario summary remained
  `/tmp/1ctx-runtime-defaults-scenarios/runtime-defaults-scenarios-summary.json`.
- Latest daemon page/list/talkfile dogfood evidence, 2026-05-20T09:00Z:
  `test-results/wiki-core-main-daemon-page-list-talkfile-20260520T085310Z/summary.json`
  proved the live debug `1contextd` JSON-RPC path for page creation, file-backed
  body write, list creation, page watch/unwatch, page role assignment,
  `wiki.publish`, file-backed `wiki.talk.append`, mail read, agent inbox,
  notification poll/ack, `agent-claim`, `mail-mark-all`, and tombstone
  publish cleanup. Browser proof in the same folder loaded the app-mirror page,
  talk route, and message-scoped `.eml` attachment from the in-app browser;
  the attachment served inline as `text/plain`. Delete proof then confirmed the
  route was removed from the manifest and the served URL returned the expected
  `missing route` response. The pass corrected an assertion assumption: rendered
  talk attachments live under `talk/attachments/<message-id>/`, not directly
  under `talk/attachments/`.
- Latest worker ergonomics evidence, 2026-05-20T09:00Z:
  Worker BD exposed `wiki.page.watch`, `wiki.page.unwatch`,
  `wiki.page.assign_role`, `wiki.list.create`, `wiki.lists`,
  `wiki.list.status`, and `wiki.list.members` through the Swift daemon bridge,
  with `swift test --package-path macos --filter WikiCoreRPCBridgeTests`
  passing. Worker BE added `--body-file` support for `talk-append` and the
  Python adapter's `talk_append(..., body_file=...)`, with focused Rust and
  pytest coverage. Worker BF's
  `test-results/wiki-core-worker-bf-browser-usability-20260520T084516Z`
  patched the rendered search modal with an explicit close button and served
  `.eml` attachments as `text/plain; charset=utf-8`; `npm test` and browser
  proof passed.
- Latest integrated verification evidence, 2026-05-20T09:00Z:
  after daemon page/list JSON-RPC exposure, file-backed talk bodies, rendered
  search close/MIME polish, docs parity, and the main daemon/browser dogfood,
  `swift test --package-path macos --filter WikiCoreRPCBridgeTests`, `swift
  test --package-path macos --filter OneContextWikiRuntimeTests`, `swift build
  --package-path macos --product 1context --product 1contextd`, `cargo fmt
  --check && cargo test -q --workspace`, focused `cargo test --package
  onecontext-wiki-daemon` body-file/missing-value tests, `uv run --with pytest
  --project memory-core pytest memory-core/tests/test_wiki_core_client.py
  memory-core/tests/test_wiki_interface_boundary.py
  memory-core/tests/test_wiki_authoring_facade.py -q`, `npm test --prefix
  wiki-engine`, and `git diff --check` passed.
- Latest daemon recovery dogfood evidence, 2026-05-20T09:14Z:
  `test-results/wiki-core-main-recovery-20260520T090755Z` proved a live debug
  `1contextd` can recover from ordinary agent mistakes and still complete a
  full wiki loop. The proof intentionally triggered missing patch-input,
  missing talk-body, invalid template, and stale source-hash errors, then
  recovered through `wiki.validate`, agent identification, page create,
  file-backed body write, hash-checked patch, list create/status/members,
  page role assignment, page watch, publish, file-backed talk append with a
  `.eml` attachment, curator/list inbox delivery, mail read, notification
  poll/ack, claim, mark-all, force-render for talk visibility, tombstone
  delete, and final publish. In-app browser proof in
  `browser-proof.json` loaded the published page, talk route, and
  message-scoped attachment; `browser-delete-proof.json` confirmed the deleted
  route served `missing route`. The pass also caught and corrected the dev
  runtime-home fixture shape: `ONECONTEXT_DEV_RUNTIME_HOME` is a fake home
  directory whose user data lives at `<home>/1Context`.
- Latest worker simplification evidence, 2026-05-20T09:14Z:
  Worker BG made the Swift bridge own wiki-core method support with
  `WikiCoreRPCBridge.supports(method:)`, removed the daemon's copied
  wiki-core allowlist, and added `wiki.validate -> validate`; focused
  `WikiCoreRPCBridgeTests` passed. Worker BH moved `talk-append` body-source
  validation ahead of page-status/role expansion and added a direct repair hint
  for missing or ambiguous body input; focused Rust tests and a CLI dogfood
  probe passed. Worker BI added browser evidence in
  `test-results/wiki-rendered-browser-dogfood-worker-bi-20260520T090324Z` for
  a rendered page, menu navigation, Agent view talk surfaces, talk route, and
  `.eml` attachment opening inline as `text/plain`; no renderer patch was
  needed.
- Latest focused verification evidence, 2026-05-20T09:14Z:
  after the bridge routing cleanup, `wiki.validate` daemon exposure,
  `talk-append` validation-order fix, and main recovery/browser dogfood,
  `swift test --package-path macos --filter WikiCoreRPCBridgeTests`,
  `cargo fmt --check -p onecontext-wiki-daemon`, `cargo test -p
  onecontext-wiki-daemon talk_append -- --nocapture`, `cargo test -p
  onecontext-wiki-daemon talk_body_source_errors_have_direct_repair_hints --
  --nocapture`, `cargo test -p onecontext-wiki-daemon
  value_cli_flags_reject_dangling_or_flag_values_before_mutating --
  --nocapture`, and `git diff --check` passed.
- Latest reusable dogfood harness evidence, 2026-05-20T09:28Z:
  `scripts/test-wiki-core-dogfood.mjs` now runs the repeatable live-daemon
  wiki dogfood loop against a disposable fake home: `wiki.validate`, expected
  error recovery, agent identify, page create/write/patch, list create/status,
  role assignment, watch, publish, talk append with `.eml` attachment,
  inbox/mail/notification claim and ack, force-render for talk visibility,
  HTTP proof for page/talk/attachment, tombstone, and route-disappears proof.
  Default delete proof passed at
  `test-results/wiki-core-dogfood-20260520T092752Z`; the runner rebuilt Rust
  and Swift with `--build` before proving the integrated state. The runbook now
  documents `node scripts/test-wiki-core-dogfood.mjs` and the
  `--keep-runtime --leave-published` browser-inspection mode.
- Latest in-app browser harness evidence, 2026-05-20T09:28Z:
  `test-results/wiki-core-dogfood-browser-20260520T092001Z` kept a rendered
  dogfood page alive for inspection. After restarting `serve-site.mjs` against
  the emitted app mirror, the in-app browser loaded
  `/dogfood-harness-092001`, `/dogfood-harness-092001/talk`, the
  message-scoped `.eml` attachment, and `/dogfood-harness-092001?view=agent`;
  `in-app-browser-proof.json` shows all checks passed. The pass exposed a small
  harness wording issue: the temporary server stops on script exit, so the
  runner now emits `served_during_run_url`, `server_stops_on_exit`, and
  `runtime_kept`.
- Latest worker dogfood evidence, 2026-05-20T09:28Z:
  Worker BJ found and fixed a Rust CLI bug where `list-create --page
  /route-style-reference` stored the route string instead of the canonical page
  id, making later `lists --page <id>` filters miss the list. Evidence under
  `test-results/worker-bj-rust-cli-dogfood` shows the before/after receipts;
  `cargo test -p onecontext-wiki-daemon` passed. Worker BL added command-style
  bridge aliases for page/talk/mail/notify calls, nested `message.attachments`,
  and local `body or bodyFile` missing-body errors; focused
  `WikiCoreRPCBridgeTests` passed with 12 tests. Worker BK rendered and
  browser-tested a fuller multi-page fixture under
  `test-results/worker-bk-rendered-wiki-20260520T091914Z`, covering brand menu
  navigation, body links, talk toggle/routes, attachments, Agent view,
  search-result navigation, and `npm test --prefix wiki-engine` with 12 tests.
- Latest integrated verification evidence, 2026-05-20T09:28Z:
  after the reusable dogfood runner, Rust list page-reference canonicalization,
  bridge alias/missing-body polish, and browser evidence,
  `swift test --package-path macos --filter WikiCoreRPCBridgeTests`,
  `cargo test -p onecontext-wiki-daemon`, `node --check
  scripts/test-wiki-core-dogfood.mjs`, `node scripts/test-wiki-core-dogfood.mjs
  --build`, and `git diff --check` passed.
- Latest alias/route dogfood evidence, 2026-05-20T09:45Z:
  `scripts/test-wiki-core-dogfood.mjs` now intentionally drives the
  consumer-shaped alias API instead of only the canonical core method names:
  `wiki.page-write-body`, `wiki.page-patch-body`, `wiki.list-create`,
  `wiki.list-status`, `wiki.list-members`, `wiki.page-assign-role`,
  `wiki.page-watch`, `wiki.talk-append`, `wiki.mail-inbox`,
  `wiki.mail-read`, `wiki.notify-poll`, `wiki.notify-ack`, and
  `wiki.mail-mark-all`. The runner also creates lists with a route-style page
  reference and proves lookup by both route and canonical page id, so the
  harness now matches how an agent would naturally address a page.
- Latest direct core fix evidence, 2026-05-20T09:45Z:
  Worker BM reproduced that `WikiCore::create_list(..., Some("/topics"))`
  stored the route string as `page_id`, which made `mail_lists(Some("topics"))`
  miss the list. `crates/onecontext-wiki-core/src/lib.rs` now canonicalizes
  page references in both `create_list` and `mail_lists`; the new
  `mail_lists_canonicalize_page_route_references_in_core` regression and the
  full `cargo test -p onecontext-wiki-core` suite passed.
- Latest browser/menu/search evidence, 2026-05-20T09:45Z:
  Worker BN's disposable proof under
  `test-results/worker-bn-dogfood-20260520T093315Z` verified menu navigation
  to `/topics`, body-link navigation to `/projects`, the talk toggle, inline
  attachment serving as `text/plain`, article and talk Agent views, search UI
  results for both page and talk routes, and no captured browser console
  errors. The remaining polish concern is that the rendered talk attachment
  label can still display `application/octet-stream` even when the static
  server serves the file correctly as text.
- Latest bridge alias evidence, 2026-05-20T09:45Z:
  Worker BO added Swift bridge regression coverage for the consumer-typed
  aliases, page file parameters, mail/notify aliases, and nested
  `attachments.source.path`; focused `WikiCoreRPCBridgeTests` passed with 13
  tests.
- Latest integrated proof evidence, 2026-05-20T09:45Z:
  the rebuilt default delete proof passed at
  `test-results/wiki-core-dogfood-20260520T094036Z`. The leave-published proof
  passed at `test-results/wiki-core-dogfood-browser-20260520T094058Z`, and
  in-app browser verification wrote `in-app-browser-proof.json` after loading
  the page route, talk route, message-scoped attachment, and page Agent view.
  Verification commands passed: `cargo fmt --check -p onecontext-wiki-core -p
  onecontext-wiki-daemon`, `cargo test -p onecontext-wiki-core`, `cargo test
  -p onecontext-wiki-daemon`, `swift test --package-path macos --filter
  WikiCoreRPCBridgeTests`, `node --check scripts/test-wiki-core-dogfood.mjs`,
  `node scripts/test-wiki-core-dogfood.mjs --build`, and `git diff --check`.
- Latest attachment metadata fix evidence, 2026-05-20T10:05Z:
  the previous browser pass exposed that `.eml` talk attachments rendered with
  the core metadata label `application/octet-stream` even though the static
  server served them inline as text. `crates/onecontext-wiki-core/src/lib.rs`
  now infers `.eml` as `text/plain`, and
  `scripts/test-wiki-core-dogfood.mjs` fails if a rendered talk page contains
  `application/octet-stream` for the dogfood attachment. Focused
  `talk_attachments_copy_media_and_duplicate_names`, full
  `cargo test -p onecontext-wiki-core`, `cargo test -p
  onecontext-wiki-daemon`, and `node --check
  scripts/test-wiki-core-dogfood.mjs` passed.
- Latest live dogfood and browser evidence, 2026-05-20T10:05Z:
  rebuilt live-daemon proof passed at
  `test-results/wiki-core-dogfood-20260520T095046Z`, including the new
  talk-page `must_not_contain: application/octet-stream` assertion and
  attachment `content_type: text/plain; charset=utf-8`. A leave-published
  proof passed at `test-results/wiki-core-dogfood-20260520T095302Z`; after
  restarting `serve-site.mjs` against its app mirror, the in-app browser wrote
  `in-app-browser-proof.json` showing page, talk, attachment, and Agent-view
  checks all passed. Screenshot capture timed out after the route checks, so
  this pass records DOM/text proof instead of an image.
- Latest worker dogfood evidence, 2026-05-20T10:05Z:
  Worker BP rendered and browser-tested a three-page link graph with topics,
  projects, and a custom navigation note under
  `test-results/worker-bp-link-graph-20260520T0948Z`; menu links, body links,
  talk toggles, search-result navigation, and link diagnostics passed. Worker
  BQ proved the agent directory/mail/subscription path under
  `test-results/worker-bq-mail-inbox-20260520T0948Z`: parent delivery to role,
  list, and page mailbox, reply delivery to the author, notification poll/ack,
  claim, and mark-all all passed. Worker BR proved create, edit, page-status,
  tombstone, republish, and no-redirect route disappearance under
  `test-results/worker-br-delete-fallback-20260520T0948Z`.
- Current remaining dogfood gap, 2026-05-20T10:05Z:
  Worker BR found that `page-status`/`wiki.list` distinguish
  `template_unedited`, `edited_from_template`, and `tombstoned`, but do not yet
  expose a crisp `shipped_default` versus custom-created page origin. Both
  bundled sample pages and disposable custom pages currently report
  `origin: created_from_template` with `flags.template_derived: true`; this is
  the next metadata simplification gap to close.
- Latest origin metadata fix evidence, 2026-05-20T10:18Z:
  the previous shipped-default/custom-page gap is now closed for source-backed
  pages. `runtime/1Context/user-wiki/wiki.toml` marks packaged source pages
  with `origin = "runtime_default"`, `WikiCore` falls back to the registry
  origin when the page ledger has no `page.created` event, and page/list status
  now exposes `flags.runtime_default` and `flags.custom_created` alongside the
  existing `origin` and `flags.template_derived`. New pages created through
  `wiki.page.create` still append `origin = "created_from_template"` to
  `wiki.toml` and ledger events. The transitional Swift inventory compiler has
  the same fields so old Swift-side inventory reads do not drift from the Rust
  core.
- Latest origin harness evidence, 2026-05-20T10:18Z:
  `scripts/test-wiki-core-dogfood.mjs` now fails unless `for-you`,
  `your-context`, `projects`, and `topics` report
  `origin: runtime_default`, `flags.runtime_default: true`, and
  `flags.custom_created: false`, and unless the disposable dogfood page reports
  `origin: created_from_template`, `flags.runtime_default: false`, and
  `flags.custom_created: true`. The rebuilt live-daemon dogfood proof passed at
  `test-results/wiki-core-dogfood-20260520T101320Z`; the faster no-build proof
  with the new assertions passed at
  `test-results/wiki-core-dogfood-20260520T101512Z`.
- Latest worker metadata/render/mail evidence, 2026-05-20T10:18Z:
  Worker BS reproduced the old-origin misclassification in pre-origin runtime
  copies and proved the current origin-aware behavior under
  `test-results/worker-bs-status-origin-20260520T1003Z`. Worker BT rendered and
  browser-tested a default-like page plus custom page fixture under
  `test-results/worker-bt-rendered-default-custom-20260520T1003Z`, covering
  routes, no missing-route fallback, menu, body links, talk toggles, search,
  and metadata source kind. Worker BU proved mail/list/watch/talk flows across
  a default existing page and a custom-created page under
  `test-results/worker-bu-mail-origin-20260520T1003Z`.
- Current remaining dogfood gaps, 2026-05-20T10:18Z:
  generated `[[site_pages]]` such as `home`, `this-week`, and
  `open-questions` are still outside `wiki.list`; page status/list does not
  expose the template path or template hash yet; tombstoned template-derived
  pages collapse `template_state` to `unknown`; list records store
  `created_at`, but list summaries/status omit it; and Worker BT saw the search
  modal close button fail to dismiss under headless Playwright, requiring a
  navigation reset in the browser harness.
- Latest verification evidence, 2026-05-20T10:18Z:
  `cargo fmt --check -p onecontext-wiki-core -p onecontext-wiki-daemon`,
  focused `cargo test -p onecontext-wiki-core
  page_status_distinguishes_runtime_defaults_from_custom_template_pages --
  --nocapture`, full `cargo test -p onecontext-wiki-core`, full `cargo test -p
  onecontext-wiki-daemon`, `swift test --package-path macos --filter
  WikiInventoryTests`, `swift test --package-path macos --filter
  WikiCoreRPCBridgeTests`, `node --check scripts/test-wiki-core-dogfood.mjs`,
  `node scripts/test-wiki-core-dogfood.mjs --build`, `node
  scripts/test-wiki-core-dogfood.mjs`, and `git diff --check` passed.
- Latest generated-page/list metadata evidence, 2026-05-20T10:35Z:
  Worker BV proved that configured generated `[[site_pages]]` were visible in
  `wiki.toml` and post-publish route manifests but missing from `wiki.list` and
  `wiki.status`; evidence is under
  `test-results/worker-bv-site-pages-list-20260520T1020Z`. `WikiCore` now
  exposes generated site pages as first-class, non-source-backed status rows in
  `wiki.list`, with `page_count`, `source_page_count`, and
  `generated_page_count` on both `wiki.list` and `wiki.status`.
  `wiki.page.status home` now returns a status card with
  `kind="generated_site_page"`, `origin="generated_site_page"`,
  `flags.source_backed=false`, and publish/validate-only allowed actions.
- Latest tombstone/list timestamp evidence, 2026-05-20T10:35Z:
  Worker BW proved and fixed the tombstoned-template metadata bug under
  `test-results/worker-bw-tombstone-template-20260520T1020Z`; tombstoned
  template-derived pages now keep `content_state="tombstoned"` while retaining
  `template_state="edited_from_template"` when the retained source and template
  baseline are available. Mail list summaries now also expose the persisted
  list `created_at` in `wiki.list.create`, `wiki.lists`,
  `wiki.list.status`, and `wiki.list.members`, so agents can distinguish newly
  created lists from longstanding mailboxes without reading raw JSONL.
- Latest integrated proof evidence, 2026-05-20T10:35Z:
  `cargo test -p onecontext-wiki-core` passed with 31 tests, including the new
  `inventory_and_status_include_generated_site_pages` and
  `mail_list_summaries_expose_created_at` regressions.
  `cargo test -p onecontext-wiki-daemon` passed with 16 unit tests and the
  page-create concurrency integration test. `node --check
  scripts/test-wiki-core-dogfood.mjs`, `git diff --check`, and the rebuilt
  dogfood harness passed at
  `test-results/wiki-core-dogfood-20260520T103252Z`; the summary proves
  `page_count=7`, `source_page_count=4`, `generated_page_count=3`,
  generated home status metadata, matching list `created_at` across create and
  status, talk rendering, inline `.eml` attachment serving, and tombstone route
  disappearance.
- Current remaining dogfood gaps, 2026-05-20T10:35Z:
  page status/list still does not expose the template path or template hash,
  and the search modal close button has a prior headless-Playwright failure
  report that still needs a focused reproduction/fix pass. Worker BX was
  closed before returning useful search-modal evidence in this lap, so that
  probe should be respawned fresh.
- Latest template/inbox metadata evidence, 2026-05-20T10:49Z:
  the template provenance gap is now closed for the Rust core inventory. Every
  `wiki.list` / `wiki.page.status` row exposes a `template` object with
  `relative_path`, user-data-relative `path`, current template `sha256`, and
  `baseline_sha256` when the page was created from a template baseline.
  Generated site pages expose their site template path/hash with no page
  baseline. The dogfood harness now fails unless runtime defaults, generated
  site pages, and custom-created pages all expose this template metadata.
  Worker CA also proved and fixed an inbox triage gap: `wiki.agent.inbox`
  thread summaries now expose `attachment_count`, so agents can see
  attachment-bearing threads before expanding raw delivery rows.
- Latest worker dogfood evidence, 2026-05-20T10:49Z:
  Worker BY reproduced the search-modal close path and proved the current
  dirty `enhance.js` fix works under headless Chromium; evidence under
  `test-results/worker-by-search-close-20260520T1035Z` includes screenshots,
  `browser-search-close.json`, `node --check wiki-engine/theme/js/enhance.js`,
  `npm test` for `wiki-engine`, and `git diff --check` for the theme files.
  Worker BZ dogfooded a topics/projects-style custom page graph under
  `test-results/worker-bz-topic-project-graph-20260520T1035Z`: created two
  linked custom pages, patched Projects to link them, appended talk, verified
  list mail, published, tombstoned one page, and proved the deleted route
  became a missing route while remaining pages marked the old links broken.
  Worker CA dogfooded agent directory, page watch, list mail, notification,
  claim/mark, default-page, and custom-page flows under
  `test-results/worker-ca-mail-agent-20260520T1035Z`; it also preserved a
  useful non-blocking finding that deeply nested fake daemon homes can exceed
  Unix socket path limits.
- Latest integrated proof evidence, 2026-05-20T10:49Z:
  `cargo fmt --package onecontext-wiki-core --package onecontext-wiki-daemon`,
  `cargo test -p onecontext-wiki-core`, `cargo test -p
  onecontext-wiki-daemon`, `node --check scripts/test-wiki-core-dogfood.mjs`,
  `node --check wiki-engine/theme/js/enhance.js`, `node
  scripts/test-wiki-core-dogfood.mjs --build`, and `git diff --check` passed.
  The rebuilt dogfood evidence at
  `test-results/wiki-core-dogfood-20260520T104557Z` proves runtime-default,
  generated, and custom page template metadata; `page_count=7`,
  `source_page_count=4`, `generated_page_count=3`; matching list
  `created_at`; `agent_inbox.threads[0].attachment_count=1`;
  notification `attachment_count=1`; talk rendering; inline `.eml`
  attachment serving; and tombstone route disappearance.
- Current remaining dogfood gaps, 2026-05-20T10:49Z:
  no blocking wiki-core dogfood gaps are open from this lap. The next useful
  slices are production-hardening rather than obvious API holes: make the
  Unix socket path less fragile for deeply nested disposable runtimes, keep
  repeating browser/menu/link/inbox rounds, and decide whether runtime-default
  configured pages should receive template baseline ledger events before first
  materializing publish or only when their source is actually created.
- Latest socket/list-status/generated/browser evidence, 2026-05-20T11:03Z:
  the deep-runtime Unix socket fragility is fixed for debug dogfood runs.
  `RuntimePaths.current()` now honors `ONECONTEXT_DEV_SOCKET_PATH`, and the
  dogfood harness defaults to a short `/tmp/1cw-dogfood-*.sock` while keeping
  user data under the requested fake home. A deliberately deep fake home that
  would have exceeded `sockaddr_un` completed the full live-daemon dogfood loop
  at `test-results/wiki-core-dogfood-20260520T105247Z-deep-socket`, with
  `socket.txt` proving `/tmp/1cw-dogfood-105243.sock`.
- Latest worker dogfood evidence, 2026-05-20T11:03Z:
  Worker CB verified generated site pages end to end under
  `test-results/worker-cb-generated-pages-20260520T1050Z`: `wiki.list` and
  `wiki.page.status` include `home`, `this-week`, and `open-questions`; source
  edit operations reject generated pages with the typed `generated_site_page`
  error; `/`, `/this-week`, `/open-questions`, and markdown twins serve with
  `source_kind=generated_site_page`; Utility browser navigation clicked through
  This Week and Open Questions. Worker CC rendered and browser-tested a
  multi-page fixture under
  `test-results/worker-cc-browser-regression-20260520T1050Z`, passing 32
  menu/search/talk/Agent-view/attachment/broken-link checks with no product
  patch. Worker CD proved and fixed `list-status` audit guidance under
  `test-results/worker-cd-mail-status-20260520T1050Z`: when archived or
  future-snoozed mail is hidden and no open work remains, `next_action` is now
  `include_hidden_mail` instead of `none`.
- Latest integrated proof evidence, 2026-05-20T11:03Z:
  `cargo fmt --package onecontext-wiki-core --package onecontext-wiki-daemon`,
  `cargo test -p onecontext-wiki-core`, `cargo test -p
  onecontext-wiki-daemon`, `swift test --package-path macos --filter
  PathAndPermissionTests`, `uv run --project memory-core --with pytest pytest
  memory-core/tests/test_wiki_core_client.py -k list_status -q`,
  `node --check scripts/test-wiki-core-dogfood.mjs`, `node
  scripts/test-wiki-core-dogfood.mjs --build`, and `git diff --check` passed.
  The rebuilt dogfood evidence at
  `test-results/wiki-core-dogfood-20260520T110240Z` proves the short socket
  path, generated/source page counts, template metadata, inbox thread
  attachment count, talk/mail/publish/delete flow, and tombstone route
  disappearance.
- Current remaining dogfood gaps, 2026-05-20T11:03Z:
  no blocking dogfood gap is open from this lap. Keep cycling through
  browser-generated routes, custom page graphs, mail/list audit states, and
  first-run/update RuntimeDefaults behavior; the remaining baseline-ledger
  policy question for runtime defaults is still a design choice, not a proven
  defect.
- Latest RuntimeDefaults summary-contract evidence, 2026-05-20T11:09Z:
  main-lane dogfood reran `./scripts/test-wiki-runtime-defaults-scenarios.sh`
  into `test-results/wiki-core-main-runtime-defaults-20260520T1106Z` and found
  an evidence gap: the Swift scenario already exercised `app-upgrade-user`,
  but the shell summary only reported fresh-user, preserve-user-edit, and
  custom-page. `scripts/test-wiki-runtime-defaults-scenarios.sh` now includes
  the app-upgrade scenario in its summary contract and asserts its copied
  backfill entry plus all three conflict proposal files. The proof rerun at
  `test-results/wiki-core-main-runtime-defaults-20260520T1106Z-after-summary-fix`
  reports `app-upgrade-user` with `ledger_status=installed_with_conflicts`,
  `copied_count=1`, `proposal_count=3`, `route_count=13`, and
  `published_route_count=13`.
- Latest worker dogfood evidence, 2026-05-20T11:16Z:
  Worker CE browser-tested rendered routes under
  `test-results/worker-ce-browser-routes-20260520T1106Z`, passing 48 checks
  across generated pages, custom/source pages, brand-menu navigation,
  Reader/Agent views, talk routes, markdown twins, inline talk attachment
  serving, broken-link diagnostics, and tombstone disappearance. Worker CF
  dogfooded page lifecycle/tree placement under
  `test-results/worker-cf-page-lifecycle-20260520T1106Z`: primary, utility,
  hidden, and nested project pages were created, hash-written, patched,
  listed/statused, published, HTTP-checked, tombstoned, republished, and
  verified as missing routes. Worker CG dogfooded mail/talk under
  `test-results/worker-cg-mail-talk-20260520T1106Z`: agent identity, reviewer
  list, page watch, role assignment, role/list/page-recipient talk delivery,
  four sanitized attachments, reply/thread behavior, inbox summaries,
  notification poll/ack, claim, mark done, snooze/archive, hidden list-status
  audits, and final clean publish pressure all passed.
- Latest live-daemon and consumer dogfood evidence, 2026-05-20T11:30Z:
  main lane reran the live Swift-daemon/Rust-core harness at
  `test-results/wiki-core-dogfood-20260520T1121Z-main`. The run created
  `dogfood-harness-112245`, proved `page_count=7`,
  `generated_page_count=3`, generated-home metadata, template-derived custom
  page creation, list `created_at`, inbox thread `attachment_count=1`,
  notification delivery, publish-after-talk, tombstone publish, and final
  `state=tombstoned` / `next_action=none`; its expected negative checks
  covered missing patch text, missing talk body, invalid template escape, and
  stale source hash repair hints. Worker CH tightened
  `scripts/test-wiki-runtime-defaults-scenarios.sh` again and proved it under
  `test-results/worker-ch-runtime-defaults-ledger-20260520T1121Z`: each
  scenario now asserts app-support route mirror equality, `current-render.json`
  status/trigger, and packaged-manifest freshness against the bundled manifest.
  Worker CI browser-tested UI affordances under
  `test-results/worker-ci-ui-affordances-20260520T1121Z`; the final
  `browser-affordance-check.json` passed 8 checks with 7 screenshots across
  Reader/Agent modes, brand menu links, search, generated pages, talk routes,
  markdown twins, inline `.eml` attachment serving, and broken-link display.
  Worker CJ dogfooded the Python `WikiCoreClient` as a memory consumer under
  `test-results/worker-cj-python-consumer-20260520T1121Z`, producing 43
  receipts across page lifecycle, publish, agent/list/mail/talk/notification,
  tombstone, and validation flows, with no defects.
- Latest agent graph and route-compatibility dogfood evidence, 2026-05-20T12:05Z:
  main lane ran `test-results/wiki-core-main-agent-graph-20260520T1155Z`,
  passing 25 checks across project/topic page creation, `page-open` hash
  writes, direct mail using returned `primary_address`, page/list/role talk
  delivery, `agent-inbox`, `mail-read --thread-id`, `agent-claim`,
  notification poll/ack, `mail-mark-all`, clean publish, static search,
  tombstone link warning, restore, final `wiki.validate=ok`, and
  `wiki.status=idle`. Browser-route proof loaded
  `/projects/main-agent-project-1155`, `/topics/main-agent-topic-1155`, and
  `/projects/main-agent-project-1155/talk`, then clicked the Talk button to
  the talk route. Worker CN found and fixed a static-server compatibility edge:
  legacy `.talk` route stems such as `/guide.talk`, `/guide.talk/`,
  `/index.talk`, and nested `/guide/reader-agent.talk` now resolve to the
  rendered `.talk.html` surface while canonical `/guide/talk` remains the
  manifest route. Evidence:
  `test-results/worker-cn-talk-route-20260520T1155Z/http-route-proof.json` and
  `browser-route-proof.json`, with focused regression coverage in
  `wiki-engine/src/renderer/index.test.mjs`.
- Latest structured publish failure and Python consumer evidence,
  2026-05-20T12:05Z: Worker CO proved the failed-publish path end to end under
  `test-results/worker-co-daemon-publish-failure-20260520T1155Z`: direct Rust
  `wiki.publish` exited 2 with structured JSON, and debug `1contextd` returned
  that failed publish as JSON-RPC `result`, not `error.message`, preserving
  `status=failed`, `next_action=repair_wiki_toml`, nested validation issues,
  and array `repair_hints`. Worker CP dogfooded the Python memory consumer
  under `test-results/worker-cp-python-consumer-20260520T1155Z`, covering page
  create/open/write/patch, publish, primary-address direct mail, page/list/role
  delivery, `agent-claim`, thread hydration, notification ack,
  tombstone/restore, and final clean validate/status with no product patch.
- Latest integrated verification evidence, 2026-05-20T12:05Z:
  `cargo fmt --check --package onecontext-wiki-core --package
  onecontext-wiki-daemon`, `cargo test -q -p onecontext-wiki-core`, `cargo test
  -q -p onecontext-wiki-daemon`, `uv run --project memory-core --with pytest
  pytest memory-core/tests/test_wiki_core_client.py -q`, `node --test
  wiki-engine/src/renderer/index.test.mjs`, `swift test --package-path macos
  --filter WikiCore`, `node --check scripts/test-wiki-core-dogfood.mjs`, `node
  --check wiki-engine/theme/js/enhance.js`, and `git diff --check` passed.
  Temporary Playwright `node_modules` from main and Worker CN evidence folders
  were removed after proof; no disposable servers or debug daemons were left
  running.
- Latest main mail/page audit evidence, 2026-05-20T12:22Z:
  main lane ran `test-results/wiki-core-main-mail-audit-20260520T1210Z`,
  passing 26 checks across hidden topic page creation, hash body writes,
  primary agent addresses, page-associated reviewer list creation, role/list/page
  recipients, reviewer inbox work, `mail-read` delivery hydration,
  `agent-claim`, archive/snooze hidden audit state, `list-status
  --include-hidden-*`, `mail-mark-all`, publish, static search, legacy
  `.talk` aliases, delete/tombstone publish, restore publish, and final clean
  `wiki.validate` / `wiki.status`. A key behavioral proof from this lap:
  talk/mail writes did not create fresh publish pressure; only content source
  edits, delete, and restore did.
- Latest worker dogfood evidence, 2026-05-20T12:22Z:
  Worker CQ hardened the RuntimeDefaults scenario proof in
  `test-results/worker-cq-runtime-defaults-20260520T1210Z`: fresh-user and
  custom-page installs are `installed`, preserve-user-edit and app-upgrade-user
  are `installed_with_conflicts`, app-upgrade backfilled one missing prompt,
  produced three conflict proposals, mirrored source routes into app-support
  `wiki-site/current`, recorded `current-render.json` triggers, and verified
  canonical `/slug/talk` plus legacy `/slug.talk` / `/slug.talk/` routes for
  packaged defaults and each scenario mirror. Worker CR built a grown rendered
  fixture under `test-results/worker-cr-reader-search-20260520T1210Z`, with
  browser proof for projects/topics/people/tools pages, a generated custom
  page, Reader/Agent views, talk route, search modal, tombstone
  delete/restore, broken-link banner, legacy talk alias, and inline `.eml`
  attachment serving. Worker CS stress-tested mail concurrency under
  `test-results/worker-cs-mail-concurrency-20260520T1210Z`, including direct,
  role, list, and page mailbox delivery; one-winner claim races; mark-all;
  archive/snooze hidden audit flags; unsubscribe; stale/retired agent pressure;
  and final page/list status pressure.
- Latest integrated verification evidence, 2026-05-20T12:22Z:
  `cargo fmt --check && cargo test -q -p onecontext-wiki-core -p
  onecontext-wiki-daemon`, `node --test wiki-engine/src/renderer/index.test.mjs`,
  `node --check scripts/test-wiki-core-dogfood.mjs`, `node --check
  wiki-engine/theme/js/enhance.js`, `bash -n
  scripts/test-wiki-runtime-defaults-scenarios.sh`, `uv run --with pytest
  --project memory-core pytest memory-core/tests/test_wiki_core_client.py -q`,
  `swift test --package-path macos --filter WikiRuntimeDefaultsScenarioTests`,
  `swift test --package-path macos --filter WikiCore`, `swift test
  --package-path macos --filter WikiRuntimeDefaultsInstallerTests`, and
  `git diff --check` passed. The scenario Swift test remains opt-in by design;
  the full scenario contract was exercised by Worker CQ with real disposable
  RuntimeDefaults homes. Bulky local Playwright install artifacts were removed
  from Worker CR evidence after proof, and the only matching long-running
  process left was the installed `/Applications/1Context.app` daemon.
- Current remaining dogfood gaps, 2026-05-20T12:22Z:
  no blocking wiki-core, publish, talk, mail, or RuntimeDefaults defect is open
  from this lap. Keep cycling through fuller grown page graphs and browser
  checks, but the system is now mostly finding edge-case proof gaps rather than
  basic API awkwardness. The next high-value pressure points are production
  polish: reduce fixture/evidence friction, keep RuntimeDefaults manifests easy
  to audit, and continue dogfooding large custom site maps with attachments and
  cross-page links.
- Latest main grown-map evidence and Swift bridge fix, 2026-05-20T12:47Z:
  main lane ran `test-results/wiki-core-main-grown-map-20260520T1225Z`,
  creating four custom pages across project/topic/tool route families, writing
  cross-linked bodies, registering author/curator agents, assigning page roles,
  creating a reviewer list, appending talk with an `.eml` attachment, publishing,
  serving the app-support mirror, searching static content, tombstoning and
  restoring the beta topic, and finishing with `render_required=false` plus
  `next_action=repair_links` for one intentional broken-link warning. The first
  pass exposed a real Swift bridge defect: large publish receipts could block
  when stdout was only drained after `waitUntilExit()`. `WikiCoreProcessClient`
  now drains stdout/stderr while the child runs, and
  `WikiCoreProcessClientTests.testCallDrainsLargeStdoutBeforeWaitingForProcessExit`
  proves a 200k JSON payload returns without deadlocking.
- Latest worker dogfood evidence, 2026-05-20T12:47Z:
  Worker CT audited RuntimeDefaults after a fresh dev build under
  `test-results/worker-ct-runtime-defaults-audit-20260520T1225Z`: packaged
  defaults freshness, first-run/update backfill, user-file preservation,
  conflict proposals, app-support current mirrors, package smoke, and legacy
  talk aliases all stayed green with no source patch. Worker CU browser-tested
  a grown rendered wiki under
  `test-results/worker-cu-browser-grown-wiki-20260520T1225Z`, then fixed two
  renderer defects: root-route section markdown/talk twins now emit at canonical
  `/reader-agent-proof.md` style paths instead of `index/...`, and object-array
  frontmatter no longer renders as `[object Object]` in markdown twins. Worker
  CV dogfooded Rust/Python mail lifecycle under
  `test-results/worker-cv-mail-lifecycle-20260520T1225Z`, passing 13 checks
  across explicit placement, stable agent addresses, subscriptions, threaded
  replies with attachments, inbox/read/claim/mark/notify, delete/restore, and
  template/user-edited/generated/tombstoned metadata with no product patch.
- Latest integrated verification evidence, 2026-05-20T12:47Z:
  `swift test --package-path macos --filter WikiCore`, `node --test
  wiki-engine/src/renderer/index.test.mjs`, `node --check
  test-results/wiki-core-main-grown-map-20260520T1225Z/run-grown-map.mjs`,
  `node --check scripts/test-wiki-core-dogfood.mjs`, `node --check
  wiki-engine/theme/js/enhance.js`, `bash -n
  scripts/test-wiki-runtime-defaults-scenarios.sh`, `cargo fmt --check &&
  cargo test -q -p onecontext-wiki-core -p onecontext-wiki-daemon`, `uv run
  --with pytest --project memory-core pytest
  memory-core/tests/test_wiki_core_client.py -q`, and `git diff --check`
  passed. No bulky evidence installs were left in the new evidence folders, and
  no disposable static server, debug daemon, grown-map process, or Rust publish
  child remained running; only the installed `/Applications/1Context.app`
  daemon matched the process check.
- Current remaining dogfood gaps, 2026-05-20T12:47Z:
  one real defect was fixed this lap in the Swift bridge, and CU fixed two
  renderer polish defects. No blocking wiki-core/mail/publish/RuntimeDefaults
  defect is open from the current evidence. The next rounds should keep
  attacking production polish: make fixture evidence less noisy, keep
  browser-route proofs cycling through larger site maps, and keep looking for
  lifecycle mismatches between app status, publish status, and user-facing
  repair guidance.
- Latest main large-publish evidence, 2026-05-20T13:05Z:
  main lane ran `test-results/wiki-core-main-large-publish-20260520T1248Z`,
  creating 24 custom pages across project/topic/tool route families, writing
  large ring-linked bodies, registering author and curator agents, assigning a
  page role, creating a reviewer list, watching a page, appending talk with an
  `.eml` attachment, and then publishing through the Swift daemon. The
  `wiki.publish` JSON-RPC response was a 180,506-byte structured receipt in
  16,549 ms, with 59 harness checks, final `wiki.validate=ok`,
  `publish.status.next_action=none`, `render_required=false`, all 24 generated
  pages visible to static search, and app-support mirror routes returning 200.
- Latest worker dogfood evidence, 2026-05-20T13:05Z:
  Worker CW hardened Swift process handling under
  `test-results/worker-cw-swift-bridge-status-20260520T1248Z`: core and
  renderer child stdout/stderr now drain while children run, large structured
  failed publish receipts survive as JSON results, and daemon `wiki.status`
  exposes distinct `app_status` and `publish_status` surfaces. Worker CX ran an
  evidence-only renderer/browser regression under
  `test-results/worker-cx-render-browser-regression-20260520T1248Z`, proving
  create/delete/restore route counts, canonical and legacy talk routes,
  attachments, brand menu, search, Reader/Agent toggle, Talk toggle, and
  markdown twins. Worker CY fixed a real mail bug under
  `test-results/worker-cy-core-mail-race-20260520T1248Z`: colliding mailbox
  directory keys such as `role://worker-cy.alpha` and
  `role://worker-cy/alpha` now stay recipient-exact, and invalid-recipient
  sends fail before staging attachment directories.
- Latest integrated verification evidence, 2026-05-20T13:05Z:
  `cargo fmt --check && cargo test -q -p onecontext-wiki-core -p
  onecontext-wiki-daemon`, `uv run --with pytest --project memory-core pytest
  memory-core/tests/test_wiki_core_client.py -q`, `node --test
  wiki-engine/src/renderer/index.test.mjs`, `node --check
  test-results/wiki-core-main-large-publish-20260520T1248Z/run-large-publish.mjs`,
  `node --check scripts/test-wiki-core-dogfood.mjs`, `node --check
  wiki-engine/theme/js/enhance.js`, `bash -n
  scripts/test-wiki-runtime-defaults-scenarios.sh`, `swift test
  --package-path macos --filter WikiCore`, and `git diff --check` passed. The
  evidence-folder hygiene check found no local Playwright/package installs, and
  the process check found no disposable dogfood server or debug daemon still
  running.
- Current remaining dogfood gaps, 2026-05-20T13:05Z:
  no blocking wiki-core, mail, publish, renderer, or RuntimeDefaults defect is
  open from this lap. The one nonblocking production-polish item is local-web
  API port allocation: CW observed a bind collision when an already-installed
  runtime owned the same debug API port. Keep the heartbeat cycling through
  larger site maps, browser-visible route checks, app/publish status separation,
  attachment-heavy talk flows, and any remaining places where evidence is noisy
  or lifecycle guidance is unclear.
- Latest main browser/status evidence, 2026-05-20T13:19Z:
  main lane ran `test-results/wiki-core-main-status-browser-20260520T1306Z`
  with `--keep-runtime --leave-published`, proving the reusable daemon consumer
  loop still creates a custom page, writes and hash-patches the body, appends
  talk with `.eml` attachment, delivers/claims/marks inbox mail, leaves
  `publish_status_after_talk.next_action=none`, and publishes an app-support
  mirror. Browser proof against the mirror passed 12 checks and captured five
  screenshots: article, brand menu, Agent view, Talk route, and search modal.
  It verified the brand menu links to `/for-you`, `/your-context`, `/projects`,
  and `/topics`; the Talk button navigates to `/dogfood-harness-130808/talk`;
  search finds the custom page; and both markdown twins are reachable.
- Latest main delete-path evidence, 2026-05-20T13:19Z:
  main lane also ran `test-results/wiki-core-main-delete-path-20260520T1306Z`
  without `--leave-published`, creating a separate page, publishing it,
  appending talk/mail, tombstoning it, publishing again, and proving the route
  returns `404 missing route` from the app-support mirror. Final page status was
  `tombstoned`, `publish_after_delete.status=published`, and
  `final_page_status.next_action=none`.
- Latest worker dogfood evidence, 2026-05-20T13:19Z:
  Worker CZ fixed the local-web API-port collision under
  `test-results/worker-cz-local-web-api-port-20260520T1306Z`: when the default
  loopback API port `39192` is already occupied, `WikiLocalAPIServer` now falls
  forward through a bounded local range, records a warning on the API snapshot,
  and `CaddyManager` resolves the same selected port for `/api/wiki/*`
  proxying. Worker DA rendered a 20-route grown wiki under
  `test-results/worker-da-browser-grown-map-20260520T1306Z`, proving 20
  markdown twins, cross-linked projects/topics/tools/people, talk routes,
  attachments, search, Reader/Agent toggle, and tombstone delete/restore with
  seven screenshots and no renderer patch. Worker DB fixed inbox lifecycle
  guardrails under `test-results/worker-db-inbox-directory-20260520T1306Z`:
  `mail-mark --state claimed` is now rejected in favor of `mail-claim` or
  `agent-claim`, TTL flags require positive integers before mutation, and stale
  known agents can be explicitly retired.
- Latest integrated verification evidence, 2026-05-20T13:19Z:
  `cargo fmt --check && cargo test -q -p onecontext-wiki-core -p
  onecontext-wiki-daemon`, `uv run --with pytest --project memory-core pytest
  memory-core/tests/test_wiki_core_client.py -q`, `swift test --package-path
  macos --filter OneContextLocalWebTests`, `swift test --package-path macos
  --filter WikiCore`, `node --test wiki-engine/src/renderer/index.test.mjs`,
  `node --check scripts/test-wiki-core-dogfood.mjs`, `node --check
  test-results/worker-da-browser-grown-map-20260520T1306Z/dogfood-grown-map.mjs`,
  `node --check wiki-engine/theme/js/enhance.js`, `bash -n
  scripts/test-wiki-runtime-defaults-scenarios.sh`, and `git diff --check`
  passed. Bulky Playwright/npm artifacts left by the Worker DA proof were
  removed, and the process check found no disposable dogfood server or debug
  daemon still running.
- Current remaining dogfood gaps, 2026-05-20T13:19Z:
  no blocking browser/render, publish, local-web API, mail, or Swift bridge
  defect is open from this lap. The next high-value proof gap is to rerun DB's
  stale-agent retirement as a clean end-to-end CLI artifact; the behavior is
  covered by Rust and Python regressions, but the worker's expanded evidence
  script malformed a JSONL fixture while trying to synthesize a stale agent.
  Keep cycling larger site maps and app/status/browser proofs, but the current
  system is mostly surfacing polish and evidence-harness gaps rather than core
  lifecycle failures.
- Latest main stale-agent retirement evidence, 2026-05-20T13:32Z:
  main lane ran
  `test-results/wiki-core-main-stale-agent-retire-20260520T1321Z`, closing the
  prior stale-agent evidence gap with a repeatable CLI harness. The proof builds
  `onecontext-wiki`, copies a disposable runtime, registers an agent with
  `role://main.stale-retire`, appends a stale heartbeat as one valid JSONL line,
  validates every line parses, verifies `agent-status` and `whoami` report
  `liveness=stale` with `next_action=agent_identify`, verifies stale agents are
  hidden from normal `agent-list` and visible with `--include-stale`, retires
  the stale agent, then verifies `agent-status`, `agent-list --include-retired`,
  and `whoami` report `liveness=retired` with
  `next_action=agent_register_new_thread`. The harness passed 16 checks and
  leaves `summary.json`, `03-agent-events-after-stale-append.json`, and
  `12-agent-events-final.json` as evidence.
- Latest worker dogfood evidence, 2026-05-20T13:32Z:
  Worker DC independently repeated the stale-agent retirement lifecycle under
  `test-results/worker-dc-stale-agent-retire-20260520T1321Z`, including
  `jq -c` validation before and after retire, with no product patch. Worker DD
  fixed daemon publish/status normalization under
  `test-results/worker-dd-daemon-status-publish-20260520T1321Z`: `wiki.publish`
  now returns structured failed result receipts for daemon/core exceptions,
  normalizes publish receipts, keeps app-mirror failures structured, and stops
  lifting render failures into top-level `wiki.status.lastError`. Worker DE
  fixed static search under
  `test-results/worker-de-talk-attachments-browser-20260520T1321Z`: `serve-site`
  no longer returns stale content-index rows when the rendered route or markdown
  twin is gone, and the proof covers canonical/legacy talk routes, markdown
  twins, `.eml` / `.md` / `.txt` attachment MIME behavior, search, and
  delete/restore reappearance.
- Latest integrated verification evidence, 2026-05-20T13:32Z:
  `cargo fmt --check && cargo test -q -p onecontext-wiki-core -p
  onecontext-wiki-daemon`, `uv run --with pytest --project memory-core pytest
  memory-core/tests/test_wiki_core_client.py -q`, `swift test --package-path
  macos --filter WikiCore`, `swift test --package-path macos --filter
  OneContextLocalWebTests`, `node --test wiki-engine/src/renderer/index.test.mjs`,
  `node --check wiki-engine/tools/serve-site.mjs`, `node --check
  test-results/wiki-core-main-stale-agent-retire-20260520T1321Z/run-stale-agent-retire.mjs`,
  `node --check
  test-results/worker-dd-daemon-status-publish-20260520T1321Z/daemon-status-publish-proof.mjs`,
  `bash -n scripts/test-wiki-runtime-defaults-scenarios.sh`, and `git diff
  --check` passed. Evidence hygiene found no local `node_modules`,
  `package-lock.json`, `.npm-cache`, or Playwright artifacts in this lap's
  evidence folders, and the process check found no disposable dogfood server or
  debug daemon still running.
- Current remaining dogfood gaps, 2026-05-20T13:32Z:
  the stale-agent retirement gap is now closed. No blocking defect is open from
  this lap. Worker DE could not capture browser screenshots because the
  available browser automation surface was busy/no active pane, so the next
  rounds should include another visual browser pass for talk attachments while
  continuing to press larger page graphs, status/publish failures, and
  search-after-delete behavior.
- Latest main browser dogfood evidence, 2026-05-20T13:47Z:
  main lane ran `test-results/main-browser-dogfood-20260520T1336Z` with
  `--build --keep-runtime --leave-published`, proving the live-daemon consumer
  loop still creates a custom page, hash-checks write/patch edits, creates a
  review list, assigns a curator role, watches the page, publishes to the
  app-support mirror, appends talk with `.eml` attachment, delivers inbox mail,
  polls/acks notifications, claims/marks mail, and keeps
  `publish_status_after_talk.next_action=none`. The visual gap from the prior
  lap is closed with Chrome screenshots for the page, talk route, and
  attachment plus `browser-navigation.spec.mjs`, which clicked the Talk toggle,
  switched to Agent view, opened the brand menu to Projects, and opened the
  attachment route. The Codex in-app browser API still reported no active pane,
  so the proof used disposable Playwright/Chrome for the browser layer.
- Latest worker dogfood evidence, 2026-05-20T13:47Z:
  Worker DF ran `test-results/worker-df-page-graph-20260520T1336Z`, creating
  `/dogfood/worker-df-page-graph` with explicit route/slug/family/nav
  placement, proving runtime-default versus custom/edited metadata, publishing,
  tombstoning, proving the route disappeared without stale HTML links, restoring
  it, and verifying the restored rendered route. Worker DG ran
  `test-results/worker-dg-mail-inbox-20260520T1336Z`, registering two agents,
  exercising direct/list/role talk delivery, parent/reply mail threads,
  `agent-inbox`, `mail-inbox`, `mail-read`, `agent-claim`, `mail-claim`,
  `mail-mark`, `mail-mark-all`, `notify-poll`, and `notify-ack`, and proving
  publish pressure stayed clean at five checkpoints. Worker DH fixed local-web
  health under `test-results/worker-dh-swift-status-20260520T1336Z`:
  `/api/wiki/health` now reads mirrored `.1context/current-render.json`
  before legacy `publish-manifest.json`, so the API reports `published_at`
  after a current successful publish.
- Latest integrated verification evidence, 2026-05-20T13:47Z:
  `cargo fmt --check && cargo test -q -p onecontext-wiki-core -p
  onecontext-wiki-daemon`, `uv run --with pytest --project memory-core pytest
  memory-core/tests/test_wiki_core_client.py -q`, `node --test
  wiki-engine/src/renderer/index.test.mjs`, `swift test --package-path macos
  --filter LocalWebTests`, `swift test --package-path macos --filter
  WikiCoreProcessClientTests`, `swift test --package-path macos --filter
  WikiCoreRPCBridgeTests`, `node --check` on the main/DF/DG/DH evidence
  harnesses, `node --check wiki-engine/tools/serve-site.mjs`, `bash -n
  scripts/test-wiki-runtime-defaults-scenarios.sh`, and `git diff --check`
  passed. Evidence hygiene found no local `node_modules`, package lockfiles,
  `.npm-cache`, or Playwright installs in this lap's evidence folders, and the
  process check found no disposable dogfood server or debug daemon still
  running.
- Current remaining dogfood gaps, 2026-05-20T13:47Z:
  no blocking page lifecycle, mail/inbox/notification, publish/status,
  renderer, local-web API, or browser-navigation defect is open from this lap.
  The biggest nonblocking product-feel note from the screenshots is that a
  custom article page can render as a very narrow desktop card; keep testing
  fuller custom pages and decide later whether that is a deliberate reading
  style or should widen for agent-authored operational pages.
- Latest main operational-width evidence, 2026-05-20T14:04Z:
  main lane ran `test-results/main-operational-width-20260520T1351Z` and
  turned the prior nonblocking product-feel note into a small renderer fix.
  `enhance.js` already treated medium article width as the settings default,
  but `renderShell()` emitted `data-article-width="s"` when page frontmatter did
  not specify a width. `wiki-engine/src/renderer/template.mjs` now defaults to
  `article_width = "m"`, and `template.test.mjs` verifies the default while
  preserving explicit `article_width: "s"`. The dogfood page was rewritten into
  a richer operational note with sections, links, a table, decision log, and
  open questions, then republished and screenshot as `operational-page.png`;
  `render-width-proof.json` proves the rendered HTML contains
  `data-article-width="m"`.
- Latest worker dogfood evidence, 2026-05-20T14:04Z:
  Worker DI ran `test-results/worker-di-grown-custom-map-20260520T1351Z`, a
  21-route custom site map with hub/topic/project/tool/people-like pages, talk
  route, markdown twins, static search, and an intentional tombstone-linked
  broken-link diagnostic with `next_action=repair_links`. Worker DJ ran
  `test-results/worker-dj-mail-edgecases-20260520T1351Z`, covering duplicate
  register/identify, retired-thread refusal, retired-agent inbox refusal with
  mailbox auditability, stale and expired subscription visibility, list
  membership after retire, reply-to thread lookup, `mail-read --thread-id`,
  idempotent notification ack, and publish-pressure stability. Worker DK ran
  `test-results/worker-dk-local-api-regression-20260520T1351Z`, proving
  current-render health/search/state, missing current-render, stale legacy
  manifest with missing app-support mirror index, and state write failure. No
  sidecar found a product bug this lap.
- Latest integrated verification evidence, 2026-05-20T14:04Z:
  `cargo fmt --check && cargo test -q -p onecontext-wiki-core -p
  onecontext-wiki-daemon`, `uv run --with pytest --project memory-core pytest
  memory-core/tests/test_wiki_core_client.py -q`, `node --test
  wiki-engine/src/renderer/template.test.mjs wiki-engine/src/renderer/index.test.mjs`,
  `node --check wiki-engine/src/renderer/template.mjs`, `node --check
  wiki-engine/tools/serve-site.mjs`, `swift test --package-path macos --filter
  OneContextLocalWebTests`, `swift test --package-path macos --filter
  WikiCoreRPCBridgeTests`, `node --check` on the DI/DJ evidence harnesses,
  `bash -n scripts/test-wiki-runtime-defaults-scenarios.sh`, and `git diff
  --check` passed. Evidence hygiene found no `node_modules`, package lockfiles,
  `.npm-cache`, Playwright installs, or DK probe executable in this lap's
  evidence folders. The process check found no disposable dogfood server or
  debug daemon still running.
- Current remaining dogfood gaps, 2026-05-20T14:04Z:
  no blocking page layout, render, mail, directory, notification, local-web API,
  publish/status, or static-route defect is open from this lap. The recurring
  environment-only gap is that the Codex in-app browser surface sometimes has
  no active pane and the Playwright MCP profile can be busy; disposable
  Playwright/Chrome screenshots remain the reliable fallback for visual proof.
- Latest main topic/project relation evidence, 2026-05-20T14:06Z:
  main lane ran `test-results/main-topic-project-relations-20260520T1406Z`,
  creating linked project, topic, and tool pages; rewriting the default
  Projects and Topics indexes to link the custom pages; sending a project talk
  review message with a markdown attachment to both the page mailbox and a
  reviewer list; publishing; verifying `/projects`, `/topics`, all custom
  routes, the talk route, attachment route, and search; tombstoning the tool
  page; proving the route 404s and link diagnostics mention the tombstoned
  target; restoring and republishing the tool; and ending with
  `wiki.publish.status.next_action=none`. This lap found a real renderer bug:
  custom pages with no generated TOC were being placed in the two-column grid's
  TOC column and rendered as a skinny desktop article. The renderer now emits
  `opctx-layout--no-toc` for pages with no TOC, CSS switches those pages to a
  centered one-column article layout, `template.test.mjs` covers the case, and
  `project-page.png` now shows the custom article at normal width.
- Latest worker dogfood evidence, 2026-05-20T14:06Z:
  Worker DL ran
  `test-results/worker-dl-template-fallback-20260520T1406Z`, found and fixed a
  Rust core fallback-metadata bug where a preexisting configured source without
  a baseline ledger reported as `unknown` instead of `edited`, and proved
  runtime-default backfill, preexisting source preservation, edited configured
  page preservation, and edited custom page metadata. Worker DM ran
  `test-results/worker-dm-attachments-20260520T1406Z`, found and fixed Rust mail
  metadata so raw delivery records carry `attachment_count`, then proved
  image/markdown/text attachment serving, role/page/agent inbox counts,
  notification counts, safe `user-wiki://` handles, and talk-only churn keeping
  `publish-status.next_action=none`. Worker DN ran
  `test-results/worker-dn-swift-bridge-20260520T1406Z`, proving Swift daemon
  bridge create/write/patch/publish/delete/restore, local-web health/search, app
  mirror publication, and final `publish-status.next_action=none`; its only
  failure was harness-only URL encoding, fixed in evidence code with no product
  patch.
- Latest integrated verification evidence, 2026-05-20T14:06Z:
  `node --test wiki-engine/src/renderer/template.test.mjs
  wiki-engine/src/renderer/index.test.mjs`, focused Rust regressions
  `cargo test -q -p onecontext-wiki-core
  create_all_preserves_preexisting_configured_source_as_user_edited` and
  `cargo test -q -p onecontext-wiki-core
  talk_attachments_copy_media_and_duplicate_names`, `cargo build -q -p
  onecontext-wiki-daemon`, and the rerun of
  `node test-results/main-topic-project-relations-20260520T1406Z/run-topic-project-relations.mjs`
  passed. A first combined `cargo test` command used invalid Cargo filter syntax
  and was immediately rerun as two focused test commands.
- Current remaining dogfood gaps, 2026-05-20T14:06Z:
  no blocking page lifecycle, fallback metadata, talk attachment, inbox,
  notification, publish/status, Swift bridge, local-web, search, delete/restore,
  or no-TOC renderer defect is open from this lap. The Codex in-app browser
  automation surface still reported no active pane, so visual proof used the
  disposable Playwright/Chrome screenshot path.
- Latest main composed-wiki evidence, 2026-05-20T14:22Z:
  main lane ran `test-results/main-composed-wiki-20260520T1422Z`, creating a
  composed mini-site with `/projects/main-compose-hub-1422`,
  `/topics/main-compose-brief-1422`, and `/tools/main-compose-runbook-1422`.
  It proved duplicate create refusal, explicit route/collection/nav placement,
  body writes, publish, talk append with markdown attachment, agent inbox
  delivery, talk-only churn keeping `publish-status.next_action=none`, forced
  talk render, `/projects` and `/topics` index links, custom route search,
  talk route and attachment route serving, no-TOC layout on the brief, normal
  TOC layout on the runbook, brand menu exposure for all custom pages,
  tombstone 404 plus broken-link diagnostics, restore, and final
  `wiki.publish.status.next_action=none`. Screenshots `brief-no-toc-page.png`
  and `runbook-toc-page.png` show the two layout paths side by side.
- Latest worker dogfood evidence, 2026-05-20T14:22Z:
  Worker DO ran `test-results/worker-do-nav-layout-20260520T1422Z` and found no
  renderer bug: the no-TOC page rendered with no `.opctx-toc`,
  `opctx-layout--no-toc`, and one computed grid column; the heading page
  rendered TOC anchors; brand menu links included both custom pages and were
  clicked both directions. Worker DP ran
  `test-results/worker-dp-mail-notify-20260520T1422Z`, proving direct, role,
  page-mailbox, list, and reply delivery; notification poll/ack; claim/read/
  snooze/done; retired-agent refusal; and stable publish status after talk-only
  and mail-state churn. Worker DQ ran
  `test-results/worker-dq-page-lifecycle-20260520T1422Z`, proving create/open/
  write/patch/list/status/validate/publish/tombstone/restore/final-publish
  lifecycle metadata. DQ found a real ergonomic issue: recreating over a
  tombstoned page could report misleading duplicate/nav placement information
  before the restore-oriented tombstone refusal. Rust core now refuses
  tombstoned/disabled existing pages before duplicate option validation and
  body-edit refusals explicitly point to `wiki.page.restore`.
- Latest integrated verification evidence, 2026-05-20T14:22Z:
  `node --check test-results/main-composed-wiki-20260520T1422Z/run-composed-wiki.mjs`,
  `cargo build -q -p onecontext-wiki-daemon`, the main composed harness,
  Worker DQ's lifecycle harness after updating the expected error envelope,
  `cargo test -q -p onecontext-wiki-daemon
  tombstoned_body_edit_errors_point_to_restore`, `cargo test -q -p
  onecontext-wiki-daemon page_create`, `cargo fmt --check`, `node --test
  wiki-engine/src/renderer/template.test.mjs wiki-engine/src/renderer/index.test.mjs`,
  `node --check` on the DO/DP/DQ evidence harnesses, and `git diff --check`
  over the touched files passed. Evidence hygiene removed Worker DO's temporary
  local Playwright install and found no `node_modules`, package lockfiles,
  `.npm-cache`, or Playwright artifacts in this lap's evidence folders. The
  process check found no disposable dogfood server or debug daemon still
  running.
- Current remaining dogfood gaps, 2026-05-20T14:22Z:
  no blocking composed-page, nav/menu, no-TOC/TOC layout, mail/inbox/
  notification, publish/status, tombstone/restore, or lifecycle error-envelope
  defect is open from this lap. Nonblocking product question: `done` messages
  still appear in `agent-inbox` as non-actionable history; current behavior is
  coherent but we may later choose to hide done mail by default if the inbox
  gets noisy. Environment-only gap remains the browser surface: Codex in-app
  browser automation sometimes reports no active pane and Chrome extension
  automation can be blocked by extension UI, so disposable Playwright/Chrome
  remains the reliable visual proof path.
- Latest main link-repair evidence, 2026-05-20T14:37Z:
  main lane ran `test-results/main-link-repair-20260520T1437Z`, creating a
  project hub, linked target, and replacement topic; publishing; proving all
  routes; tombstoning the target; verifying the deleted route 404s, link
  diagnostics report one broken internal link, and `wiki.publish.status`
  returns `next_action=repair_links`; patching the hub source to remove the
  stale link; appending a talk repair note; forcing a final publish; and ending
  with clean diagnostics plus `next_action=none`. Screenshots
  `hub-before-repair.png` and `hub-after-repair.png` show the visible repair.
- Latest worker dogfood evidence, 2026-05-20T14:37Z:
  Worker DR ran `test-results/worker-dr-link-repair-20260520T1437Z`, proving an
  independent link-repair flow where `page-delete`, publish diagnostics,
  `publish-status`, and `page-status` all point to `repair_links` until the
  stale link is removed, after which `wiki.validate` returns `status=ok` and
  `issue_count=0`. Worker DS ran
  `test-results/worker-ds-talk-render-mail-20260520T1437Z` and found a real
  talk-renderer bug: mail-backed talk entries rendered their prose but dropped
  message ids, thread ids, subjects, recipients, state, created timestamps, and
  attachments. `wiki-engine/src/renderer/talk-folder.mjs` now normalizes and
  renders that metadata into the HTML and markdown twin, resolves route-relative
  talk attachment links against the talk route, and `talk-folder.test.mjs`
  covers the behavior. Worker DT ran
  `test-results/worker-dt-swift-local-web-repair-20260520T1437Z`, proving the
  Swift local-web bridge can see Rust-core create/edit/publish/talk/mail/
  notify/static behavior, app-support mirror additions/removals/restores, and
  `/api/wiki/health`, `/api/wiki/search`, and `/api/wiki/state`.
- Latest integrated verification evidence, 2026-05-20T14:37Z:
  `node --check test-results/main-link-repair-20260520T1437Z/run-link-repair.mjs`,
  `node --check test-results/worker-dr-link-repair-20260520T1437Z/run-link-repair.mjs`,
  `node --check test-results/worker-dt-swift-local-web-repair-20260520T1437Z/worker-dt-followup.mjs`,
  `cargo fmt --check`, `cargo test -q -p onecontext-wiki-daemon
  tombstoned_body_edit_errors_point_to_restore`, `node --test
  wiki-engine/src/renderer/talk-folder.test.mjs
  wiki-engine/src/renderer/template.test.mjs
  wiki-engine/src/renderer/index.test.mjs`, `npm test --prefix wiki-engine`,
  and `git diff --check` over the touched renderer/CSS files passed. Evidence
  hygiene removed Worker DS's temporary package manifest files and found no
  remaining local `node_modules`, package lockfiles, package manifests,
  `.npm-cache`, or Playwright installs in this lap's evidence folders. The only
  matching long-running process was the installed `/Applications/1Context.app`
  daemon, which was left untouched.
- Current remaining dogfood gaps, 2026-05-20T14:37Z:
  no blocking page lifecycle, link-repair, talk rendering, attachment serving,
  inbox/notification, publish/status, Swift bridge, local-web API, search,
  tombstone/restore, or static-route defect is open from this lap. The
  environment-only gap remains browser automation reliability: in-app/Chrome
  automation can be blocked by active-pane or extension UI state, so disposable
  Playwright/Chrome remains the reliable visual proof path until the browser
  bridge is made steadier.
- Latest main agent-workflow evidence, 2026-05-20T15:00Z:
  main lane ran `test-results/main-agent-workflow-20260520T1500Z`, creating a
  daily workbench page, project page, topic page, and stale note; registering
  author/curator agents; creating a reviewer list; assigning the page curator;
  publishing the draft; sending curator review mail with a markdown attachment;
  proving `agent-inbox`, `notify-poll`, `mail-read`, and `mail-mark-all`;
  proving talk-only review churn keeps `publish-status.next_action=none`;
  tombstoning the stale note; proving stale route 404 plus
  `next_action=repair_links`; rewriting the daily page to remove the stale link;
  replying on the original talk thread; publishing; proving clean link
  diagnostics, search, menu exposure, talk route, attachment route, and final
  `next_action=none`. Screenshots `day-before-review.png` and
  `day-after-repair.png` capture the visible before/after.
- Latest main renderer cleanup, 2026-05-20T15:00Z:
  the main screenshot exposed a bad reader experience: `enhance.js` injected a
  small branded agent note inside the article body after the lead paragraph,
  which made the wiki content look polluted by app chrome. The note injector
  and unused CSS were removed; the Reader/Agent/Talk controls remain the
  explicit surface switch. The rerun screenshot shows the article body clean,
  and `rg opctx-agent-note|injectAgentNote|A wiki for humans` over the patched
  JS/CSS/rendered page returned no matches.
- Latest worker dogfood evidence, 2026-05-20T15:00Z:
  Worker DU ran
  `test-results/worker-du-inbox-visibility-20260520T1500Z`, proving
  direct/list/role/page-mailbox delivery, reply fanout, notification ack,
  read, snooze/default-hide plus `--include-snoozed`, role claim affecting peer
  claimability, `mail-mark-all` closing reply deliveries, retired-agent inbox
  refusal, and mail-only churn keeping publish status clean. Worker DV ran
  `test-results/worker-dv-talk-attachments-20260520T1500Z`, creating a rich
  talk/inbox page with text, markdown, uppercase `.PNG`, nested-path, and reply
  attachments; proving page/talk routes, markdown twins, attachment URLs,
  search, screenshot, and clean link diagnostics. DV found and fixed a real
  static-serving bug: uppercase image extensions such as `.PNG` now resolve to
  `image/png` instead of `application/octet-stream`. Worker DW ran
  `test-results/worker-dw-local-web-menu-20260520T1500Z`, proving Rust core to
  Swift daemon bridge to app-support mirror to local-web/static serving for
  custom project/topic/tool pages, menu links, talk route, health/search/state
  API, delete 404, restore 200, and final clean publish status.
- Latest integrated verification evidence, 2026-05-20T15:00Z:
  `node --check wiki-engine/theme/js/enhance.js`, `npm test --prefix
  wiki-engine`, `cargo test -q -p onecontext-wiki-core
  talk_attachments_copy_media_and_duplicate_names`, `cargo test -q -p
  onecontext-wiki-core page_restore_reopens_tombstoned_page_and_navigation`,
  `node --check` on the main/DV/DW evidence harnesses, and `git diff --check`
  over the touched renderer/static/evidence paths passed. Evidence hygiene
  found no local `node_modules`, package lockfiles, package manifests,
  `.npm-cache`, or Playwright installs in this lap's evidence folders. The
  process check found no disposable dogfood server or debug daemon still
  running; the installed `/Applications/1Context.app` daemon was left
  untouched.
- Current remaining dogfood gaps, 2026-05-20T15:00Z:
  no blocking page workflow, talk attachment, inbox visibility, notification,
  publish/status, link repair, static serving, Swift bridge, local-web API,
  menu, search, tombstone/restore, or rendered-reader defect is open from this
  lap. The repeated environment/tooling gap is still browser automation:
  in-app Browser had no active pane for workers, Chrome extension automation
  timed out, and MCP Playwright profile contention blocked DW screenshots; the
  reliable fallback remains isolated Playwright CLI screenshots.
- Latest main surface-switch evidence, 2026-05-20T15:16Z:
  main lane ran `test-results/main-surface-toggle-20260520T1516Z`, creating a
  custom nested project page and linked topic page, appending a talk review,
  publishing, serving the rendered wiki, and driving browser proof across
  Reader, Agent, and Talk controls. The proof checked the article route, talk
  route, markdown twin, talk markdown twin, search results for body and talk
  tokens, Agent-view surface URLs with plain slash routes instead of escaped
  `%2F`, Talk navigation to `/projects/main-surface-toggle-1516/talk`, and
  return to the article reader route. Screenshot `agent-view.png` shows the
  clean Agent surface table and raw markdown without the removed branded reader
  note.
- Latest worker dogfood evidence, 2026-05-20T15:16Z:
  Worker DX ran `test-results/worker-dx-mail-aging-20260520T1516Z`, proving
  stale agent leases, expired subscriptions, retired-agent liveness,
  active/stale/retired reviewer-list counts, role/list/page mailbox delivery,
  notification fanout, snooze hide and expiry behavior, `mail-mark-all`, and
  talk/mail-only churn keeping `publish-status.next_action=none`. Worker DY ran
  `test-results/worker-dy-graph-repair-20260520T1516Z`, creating a
  project/topic/tool/person graph, tombstoning the central topic, proving three
  inbound broken links, repairing them with hash-checked `page-patch-body`,
  publishing cleanly, and verifying static routes, search, menu behavior, and
  deleted-route 404s. Worker DZ ran
  `test-results/worker-dz-static-local-state-20260520T1516Z`, proving
  `serve-site` static state is read-only, static search works, uppercase media
  attachments keep correct content types, browser Reader/Agent/Talk controls
  do not write `/api/wiki/state`, and the Swift local-web state API accepts
  valid patches while rejecting oversized payloads.
- Latest product fix, 2026-05-20T15:16Z:
  DY found and fixed a real delete lifecycle bug in
  `crates/onecontext-wiki-daemon/src/main.rs`: when a page delete already
  reports `next_action=publish_then_repair_links`, the emitted
  `link_repair_lifecycle` now preserves that composed action and branch instead
  of downgrading the agent-facing state to plain `repair_links`. The focused
  daemon unit test now asserts the composed branch, next command, and follow-up
  command sequence.
- Latest integrated verification evidence, 2026-05-20T15:16Z:
  `npm test --prefix wiki-engine`, `cargo test -q -p onecontext-wiki-daemon`,
  `cargo test -q -p onecontext-wiki-core --lib mail`, `swift test
  --package-path macos --filter
  LocalWebTests/testWikiLocalAPIStatePersistsAndRejectsOversizedPayloads`,
  `cargo fmt --check --package onecontext-wiki-daemon`, and `node --check` on
  the main/DX/DY evidence harnesses passed. Evidence hygiene removed DZ's
  temporary local Playwright install and found no remaining `node_modules`,
  package manifests, package lockfiles, or `.npm-cache` in this lap's evidence
  folders. The process check found only the installed
  `/Applications/1Context.app` daemon, which was left untouched.
- Current remaining dogfood gaps, 2026-05-20T15:16Z:
  no blocking surface switch, static preview, local-web state, mail aging,
  notification, snooze, mark-all, link-repair, tombstone, menu, search,
  uppercase attachment serving, or publish-status defect is open from this lap.
  The recurring tooling gap remains browser automation reliability: isolated
  Playwright remains the dependable visual proof path when the in-app or Chrome
  bridge is unavailable, and disposable browser installs must be trimmed from
  evidence after use.
- Latest main consumer-ergonomics evidence, 2026-05-20T15:34Z:
  main lane ran `test-results/main-consumer-ergonomics-20260520T1534Z`,
  creating a custom project hub, linked topic, and hidden archive page. It
  proved the consumer path an agent should actually use: read
  `wiki.list`/`wiki.page.status` metadata after create, write with the create
  hash, intentionally fail a stale-hash `page-patch-body`, recover through
  `page-open` and the current `expected_source_sha256`, apply a hash-checked
  patch, append talk, publish, verify HTTP routes/search/browser Reader-Agent-
  Talk behavior, tombstone the hidden archive, republish, and confirm the
  hidden archive route returns 404. The main evidence recorded 23 passed
  checks plus screenshots `hub-agent.png` and `hub-talk.png`.
- Latest worker dogfood evidence, 2026-05-20T15:34Z:
  Worker EA ran `test-results/worker-ea-list-status-20260520T1534Z`, proving
  list/status metadata for runtime defaults, custom pages across groups,
  write/patch transitions, rendered/stale states, publish clearing stale,
  tombstone state before and after delete publish, handles, watcher liveness,
  and page/list/role delivery counts. Worker EB ran
  `test-results/worker-eb-threaded-mail-20260520T1534Z`, registering six
  agents, claiming a page curator role, creating reviewer/watcher lists,
  appending threaded talk with attachments, exercising direct/list/role/page
  mail, polling notifications, read/mark/snooze, retiring one watcher, and
  independently checking final `page.status.mail` counts
  (`message_count=7`, `unread_count=2`, `actionable_count=3`,
  `open_thread_count=2`). Worker EC ran
  `test-results/worker-ec-toc-menu-browser-20260520T1534Z`, rendering a dense
  page graph and proving 41 ToC anchors against 41 browser-visible targets,
  menu links across five nav groups, project/topic/tool navigation, article to
  talk and back toggles, Agent markdown view, heading/body/talk search tokens,
  and text/markdown twins.
- Latest integrated verification evidence, 2026-05-20T15:34Z:
  `npm test --prefix wiki-engine`, `cargo test -q -p
  onecontext-wiki-daemon`, `cargo test -q -p onecontext-wiki-core --lib mail`,
  and `node --check` on the main/EA/EB/EC evidence harnesses passed.
  `git diff --check` over the goal doc, daemon patch, and this lap's evidence
  folders passed. Evidence hygiene removed EC's temporary local Playwright
  install and found no remaining `node_modules`, package manifests, package
  lockfiles, `.npm-cache`, or `.playwright*` files in this lap's evidence
  folders.
- Current remaining dogfood gaps, 2026-05-20T15:34Z:
  no blocking list/status metadata, stale-hash recovery, edit/publish,
  tombstone, hidden-page, talk/inbox, notification, snooze, watcher liveness,
  ToC, menu, search, markdown twin, or browser route defect is open from this
  lap. The repeated gap remains transport and tooling, not core behavior:
  these laps mostly dogfood the direct Rust CLI/core rather than the Swift
  daemon JSON-RPC wrapper, and in-app browser availability remains less
  reliable than isolated Playwright for repeatable visual proof.
- Latest main guardrail/restore evidence, 2026-05-20T15:50Z:
  main lane ran `test-results/main-guardrail-restore-20260520T1550Z`,
  creating a project hub and linked topic target, writing duplicate patch
  tokens, proving ambiguous `page-patch-body` fails with
  `body_patch_ambiguous` plus actionable repair hints, recovering with
  `page-open` and a hash-checked unique patch, publishing cleanly, tombstoning
  the linked target, proving delete emits `publish_then_repair_links`, proving
  delete publish makes the target route 404 and `publish-status` asks for
  `repair_links`, restoring the target instead of editing the source,
  republishing, and proving the target route returns 200 with clean link
  health. Browser proof screenshots `hub-agent.png` and `target-restored.png`
  show the recovered graph.
- Latest worker dogfood evidence, 2026-05-20T15:50Z:
  Worker FA ran `test-results/worker-fa-restore-watchers-20260520T1550Z`,
  proving delete/restore lifecycle with watcher/reviewer lists, curator role,
  page mail, route 404 after delete publish, normal tombstoned talk append
  refusal with `tombstoned_page`, explicit `--allow-tombstoned` archive
  maintenance append, route 200 after restore publish, restored
  `page.status.state=rendered`, watcher/list metadata preserved, and
  quiet `publish-status.next_action=none`. Worker FB ran
  `test-results/worker-fb-link-diagnostics-20260520T1550Z`, intentionally
  creating broken route, markdown-twin, talk-route, and relative links across
  project/topic/tool pages; initial publish produced
  `broken_internal_count=10` and actionable repair tasks, hash-checked
  repairs landed on all three pages, and final validate/publish-status/link
  diagnostics were clean. Worker FC ran
  `test-results/worker-fc-static-api-20260520T1550Z`, proving rendered static
  API and browser behavior for custom pages, talk, attachments, state/search
  HTTP methods, content types, Reader/Agent/Talk toggles, search modal, menu
  navigation, and zero browser console/response errors.
- Latest product fix, 2026-05-20T15:50Z:
  FC found and fixed a static API contract bug in
  `wiki-engine/tools/serve-site.mjs`: `OPTIONS /api/wiki/search` now returns
  `204` with `Allow: GET, HEAD, OPTIONS`, matching `/api/wiki/state`; write
  rejections on both static APIs also include the same `Allow` header. The
  permanent `serve-site` test in `wiki-engine/src/renderer/index.test.mjs`
  now pins state/search `OPTIONS`, search `HEAD`, search write rejection, and
  `Allow` headers.
- Latest integrated verification evidence, 2026-05-20T15:50Z:
  `node --check wiki-engine/tools/serve-site.mjs`, `npm test --prefix
  wiki-engine`, `cargo test -q -p onecontext-wiki-daemon`, `cargo test -q -p
  onecontext-wiki-core --lib mail`, `node --check` on the main/FB/FC
  JavaScript harnesses, `bash -n` on the FA shell harness, and `git diff
  --check` over the goal doc, static API patch, renderer test patch, and this
  lap's evidence folders passed. Evidence hygiene found no `node_modules`,
  package manifests, package lockfiles, `.npm-cache`, or `.playwright*` files
  in this lap's evidence folders.
- Current remaining dogfood gaps, 2026-05-20T15:50Z:
  no blocking ambiguous-edit guardrail, hash recovery, delete/restore,
  tombstoned talk policy, watcher/list preservation, link diagnostics,
  repair hints, static API, browser menu/search/toggle, attachment serving, or
  markdown twin defect is open from this lap. The main remaining product
  confidence gap is still transport-level: most dogfood runs exercise the Rust
  CLI/core and static renderer directly; a future lap should spend more time
  through the Swift daemon JSON-RPC/local-web wrapper as the consumer surface.
- Latest main Swift bridge evidence, 2026-05-20T16:07Z:
  main lane ran `test-results/main-swift-bridge-20260520T1607Z`, building the
  Rust core, opting the Swift `WikiCoreProcessClient` into the real core binary,
  and running focused Swift proof for the process client, RPC bridge, LocalWeb
  search API, LocalWeb state API, and LocalWeb render-state health API. The
  disposable runtime created and rendered `swift-bridge-proof`, kept
  `publish-status.next_action=none`, and proved talk-only churn on `topics`
  updated page mail metadata (`message_count=1`, active watcher/list metadata)
  without forcing a content publish. The lap summary recorded eight passed
  checks and zero failures.
- Latest worker dogfood evidence, 2026-05-20T16:07Z:
  Worker FD ran `test-results/worker-fd-swift-rpc-20260520T1607Z`, proving the
  live Swift daemon JSON-RPC bridge through health, validate, list,
  page.status/open/create/write/patch, list/watch/role, publish/status,
  talk/mail/notify, app-mirror publish, and HTTP route/talk/attachment serving;
  it also replayed the LocalWeb API for health/search/state/bookmarks and built
  both `1contextd` and `1context`. Worker FE ran
  `test-results/worker-fe-defaults-upgrade-20260520T1607Z`, proving
  RuntimeDefaults fresh install, preserve-user-edit, app-upgrade, custom-page,
  backfill, conflict-proposal, legacy talk alias, and publish-from-user-data
  scenarios; fresh/custom installs copied 202 defaults, preserve/app-upgrade
  preserved user files and wrote proposals, and route manifests matched the
  app-support published mirror. Worker FF ran
  `test-results/worker-ff-browser-regression-20260520T1607Z`, rendering 10
  routes with 10 markdown twins, passing 27 HTTP checks and 41 isolated
  Playwright browser checks across Reader/Agent/Talk, search modal, menu links,
  attachment serving, and state/search method contracts with zero console
  errors, request failures, or local browser 4xx/5xx responses.
- Latest integrated verification evidence, 2026-05-20T16:07Z:
  `npm test --prefix wiki-engine`, `cargo test -q -p onecontext-wiki-daemon`,
  `swift test --package-path macos --filter
  'WikiCoreRPCBridgeTests|WikiCoreProcessClientTests|LocalWebTests/testWikiLocalAPI'`,
  and `node --check` on FD/FF evidence harnesses passed. Evidence hygiene found
  no `node_modules`, package manifests, package lockfiles, `.npm-cache`, or
  `.playwright*` files in the 16:07 evidence folders.
- Current remaining dogfood gaps, 2026-05-20T16:07Z:
  no blocking Swift process-client, JSON-RPC bridge, LocalWeb search/state/
  health, RuntimeDefaults backfill/conflict, publish-from-user-data, static
  search/state, browser menu/search/toggle, attachment, talk/mail, or
  publish-status defect is open from this lap. The remaining confidence gaps
  are now narrower and concrete: the privileged Caddy HTTPS/Open Wiki branded
  path was not exercised in disposable fixtures, the in-app browser pane was not
  available to Worker FF so isolated Playwright remained the visual proof path,
  and FE's packaged-defaults freshness check used the existing dirty dev
  `dist/1Context.app` manifest rather than a clean release bundle.
- Latest main Open Wiki/local-web evidence, 2026-05-20T16:24Z:
  main lane ran `test-results/main-localweb-openwiki-20260520T1624Z`. The
  installed 0.1.86 app reports ready setup and serves
  `https://wiki.1context.localhost/your-context` through Caddy with rendered
  `Your Context` content and healthy `/__1context/health`. The current debug
  0.1.87 CLI reports the new target URL as `https://localhost/your-context`
  and refuses `wiki local-url` because it looks for a debug-build local HTTPS
  helper; direct curl confirms `https://localhost/your-context` fails TLS while
  the installed branded route succeeds. Current source still passes the full
  `LocalWebTests` suite, including the multi-address Caddy config for
  localhost, 127.0.0.1, and the branded host. This narrows the Open Wiki gap to
  dev/install skew and upgrade proof, not renderer output.
- Latest worker dogfood evidence, 2026-05-20T16:24Z:
  Worker FG ran `test-results/worker-fg-clean-defaults-20260520T1624Z`,
  rebuilding a dev-channel app bundle in 37 seconds, validating the DMG,
  package smoke, RuntimeDefaults manifest freshness, and RuntimeDefaults
  install/backfill/preserve scenarios. The fresh manifest is version 0.1.87,
  matches `HEAD`, records `git_dirty=true`, has render status published, and
  has clean `publish_status.next_action=none`. Worker FH ran
  `test-results/worker-fh-graph-agent-20260520T1624Z`, creating project/topic/
  tool pages, linking them, hash-writing and patching bodies, registering
  agents, appending talk with an attachment, publishing, tombstoning the tool,
  proving two broken inbound links, repairing project/topic links, and
  finishing with `wiki.status.state=idle`, `next_action=none`, and zero final
  link issues. Worker FI ran
  `test-results/worker-fi-mail-ergonomics-20260520T1624Z`, exercising direct,
  list, role, and page mail; watcher inboxes; notification ack; read,
  mark-all, snooze, and retire behavior; and proving mail-only churn leaves
  `render_required=false`.
- Latest product fix, 2026-05-20T16:24Z:
  FI fixed the reusable dogfood harness
  `scripts/test-wiki-core-dogfood.mjs` so notification ack uses the actual
  `notification.notification_id`, passes it as `notificationId` to
  `wiki.notify-ack`, and immediately proves the follow-up poll is empty. This
  was a consumer-harness bug, not a Rust mail-core bug, but it matters because
  it makes future dogfood runs prove the inbox wakeup lifecycle instead of only
  assuming it.
- Latest integrated verification evidence, 2026-05-20T16:24Z:
  main lane passed `swift test --package-path macos --filter LocalWebTests`.
  Worker FG passed `release-train.sh build --channel dev`,
  `test-launch-agent-package.sh`, and `test-wiki-runtime-defaults-scenarios.sh`.
  Worker FH passed `cargo build --package onecontext-wiki-daemon` and
  `cargo test --package onecontext-wiki-daemon`. Worker FI passed
  `node --check scripts/test-wiki-core-dogfood.mjs`, `node --check` on its
  mail ergonomics harness, and `cargo test --package onecontext-wiki-core
  repeated_mail_claim_mark_and_notification_ack_are_idempotent`. The final
  integration sweep also passed `npm test --prefix wiki-engine`,
  `cargo test -q -p onecontext-wiki-daemon`, `git diff --check`, and evidence
  hygiene for the 16:24 evidence folders; no dogfood harness process was left
  running.
- Current remaining dogfood gaps, 2026-05-20T16:24Z:
  no blocking RuntimeDefaults freshness, dev-build timing, page graph,
  tombstone/repair, direct/list/role/page mail, notification ack, snooze,
  retire, or local-web source-test defect is open from this lap. The concrete
  remaining Open Wiki gap is upgrade/runtime skew: the installed 0.1.86 app
  serves the branded route, while current 0.1.87 source wants localhost and the
  debug CLI sees the local HTTPS helper as missing. A future lap should install
  or launch the freshly rebuilt dev app and prove the new localhost Open Wiki
  route end to end after setup/state migration.
- Latest Open Wiki/dev-build evidence, 2026-05-20T16:39Z:
  main lane ran `test-results/main-openwiki-upgrade-20260520T1639Z` and found
  an important false-confidence edge: `wiki local-url` could print the desired
  URL from cached diagnostics without first repairing/starting the local web
  edge. The CLI now calls `CaddyManager.start()` before printing the URL, and a
  dev-channel rebuild passed in
  `test-results/main-openwiki-upgrade-20260520T1639Z/dev-build-after-local-url-fix`.
  Worker FJ then isolated `dist/1Context.app` 0.1.87 without overwriting the
  installed app and proved both `https://localhost/your-context` and
  `https://wiki.1context.localhost/your-context` return HTTP 200 while the dist
  Caddy edge is active. Evidence:
  `test-results/worker-fj-openwiki-upgrade-20260520T1639Z/isolated-dist-localhost-proof.txt`
  and `summary.md`.
- Latest richer rendered-graph evidence, 2026-05-20T16:39Z:
  Worker FK built and rendered a fuller disposable wiki graph inspired by the
  public demo shape, with project, topic, people, organization, article, talk,
  markdown twin, menu, and search surfaces. Evidence:
  `test-results/worker-fk-fuller-graph-browser-20260520T1639Z/render-result.json`
  reports 37 routes, 37 markdown twins, 17 source pages, 17 talk pages, and 0
  broken internal links; `curl-proof.json` passed 9/9 route-class checks; and
  `playwright-proof.json` passed 7 browser steps with 0 console messages and 0
  page errors.
- Latest template/status metadata fix, 2026-05-20T16:39Z:
  Worker FL found and fixed a metadata truth bug in the Rust core: edited pages
  that were later tombstoned could lose `flags.user_edited` even though their
  template diff still showed real user authorship. The post-fix dogfood run at
  `test-results/worker-fl-template-status-20260520T1639Z/post-fix-summary.json`
  proves `wiki.page.status` and `wiki.list` now keep `user_edited: true` across
  create, write, publish, tombstone, tombstone-publish, restore, and
  restore-publish. The daemon dogfood evidence at
  `test-results/worker-fl-template-status-20260520T1639Z/daemon-dogfood/summary.json`
  also completed the reusable live-daemon loop.
- Latest cleanup/state evidence, 2026-05-20T16:39Z:
  after the isolated dist proof, the local machine was restored to the installed
  `/Applications/1Context.app` 0.1.86 menu LaunchAgent. Installed diagnose again
  reports `Local Wiki: reachable` at
  `https://wiki.1context.localhost/your-context`, with Caddy owned by the
  installed bundle. The remaining Open Wiki gap is therefore not "can 0.1.87
  serve localhost" but "does the signed/update install path migrate the user
  from the branded 0.1.86 local-web mode to the localhost 0.1.87 mode without
  leaving old menu/Caddy state in control."
- Latest upgrade-seam evidence, 2026-05-20T16:56Z:
  main lane captured `test-results/main-upgrade-seam-20260520T1656Z` and
  Worker FM captured
  `test-results/worker-fm-upgrade-seam-20260520T1656Z/SUMMARY.md`. The installed
  `/Applications/1Context.app` is signed/notarized 0.1.86 and owns the live
  menu, runtime, proxy, and Caddy. The live setup marker still records
  `public_host = wiki.1context.localhost`; the Caddy state records
  `/Applications/1Context.app/Contents/Resources/local-web/caddy/caddy`; and
  the live Caddyfile serves only `https://wiki.1context.localhost:39191`.
  Installed diagnose is healthy on the branded route. Dist 0.1.87 diagnose sees
  the same live runtime as `url mode changed`, with localhost and direct
  loopback probes failing TLS while the branded probe stays OK. We stopped
  before overwriting `/Applications/1Context.app` or mutating shared local-web
  state. The next proof is a reversible state snapshot plus an actual 0.1.87
  install/start path, then verify the Caddyfile expands to `127.0.0.1`,
  `localhost`, and `wiki.1context.localhost`.
- Latest LocalWeb diagnostics fixes, 2026-05-20T16:56Z:
  main lane tightened local-web diagnostics for exactly this mixed-version seam.
  `LocalWebDiagnostics` now includes `runningCaddyExecutable` so diagnose can
  print both the configured Caddy candidate for the current bundle and the
  running Caddy executable recorded in state. This made the installed 0.1.86
  Caddy ownership visible from a debug/dist probe. The same pass fixed Caddy and
  proxy candidate selection so executable directories are not treated as
  binaries; debug diagnose now reports missing bundled Caddy instead of
  shortening an executable directory to the repo root. Evidence:
  `test-results/main-upgrade-seam-20260520T1656Z/debug-diagnostics-after-caddy-missing-fix.txt`.
- Latest relationship/rendered-browser evidence, 2026-05-20T16:56Z:
  Worker FN created a disposable richer relationship fixture at
  `test-results/worker-fn-rich-relationships-20260520T1656Z`. The final render
  has 20 routes, 20 markdown twins, 10 page surfaces, 10 talk surfaces, and 0
  broken internal links after repair. The lifecycle dogfood created pages,
  wrote bodies, appended talk, published, tombstoned
  `/projects/deprecated-widget`, observed one broken inbound link from
  `/projects`, repaired the link, republished, and validated clean. Curl passed
  11 checks; Playwright passed 10 browser steps with no console or page errors.
- Latest mail/talk publish-boundary evidence, 2026-05-20T16:56Z:
  Worker FO ran `test-results/worker-fo-mail-talk-20260520T1656Z`, registering
  agents, subscribing/unsubscribing lists, sending direct/list/role/page
  talk-mail, copying an attachment, reading/claiming/acking/marking/snoozing/
  retiring, and checking page/list metadata. The evidence proves source content
  edits require publish, while talk/mail delivery, mail-state churn, and retire
  stay content-publish clean. Final publish status is clean.
- Latest integrated verification evidence, 2026-05-20T16:56Z:
  main lane passed `swift test --package-path macos --filter LocalWebTests`
  and `swift build --package-path macos --product 1context` after the
  diagnostics patches. Worker FM passed the focused LocalWeb test suite, Worker
  FN passed its node harness with Rust CLI build, curl, and Playwright proof,
  and Worker FO passed `cargo build --package onecontext-wiki-daemon`,
  `cargo test --package onecontext-wiki-core mail`,
  `cargo test --package onecontext-wiki-daemon
  talk_append_accepts_body_file_and_rejects_ambiguous_body_sources`, and
  `node --check` on its harness.
- Current remaining dogfood gaps, 2026-05-20T16:56Z:
  no blocking Rust-core page lifecycle, rendered relationship graph, talk/mail
  publish-boundary, or LocalWeb diagnostics defect is open from this lap. The
  remaining Open Wiki proof is now specifically the signed/update install path:
  snapshot live local-web state, run or install 0.1.87 as the owner of the menu
  and local-web Caddy, prove localhost and branded routes both serve, and prove
  the old 0.1.86 branded-only state cannot keep supervising the new app.
- Latest reversible local-web migration proof, 2026-05-20T17:11Z:
  main lane ran
  `test-results/main-reversible-dist-localhost-20260520T1711Z`, and Worker FP
  independently ran
  `test-results/worker-fp-upgrade-restore-plan-20260520T1711Z`. Both proved the
  non-install `dist/1Context.app` 0.1.87 start path can take over shared
  local-web state without overwriting `/Applications/1Context.app`: `wiki
  local-url` prints `https://localhost/your-context`, rewrites the Caddyfile to
  `https://127.0.0.1:39191, https://localhost:39191,
  https://wiki.1context.localhost:39191`, and serves HTTP 200 for both
  `https://localhost/your-context` and
  `https://wiki.1context.localhost/your-context`. Main evidence:
  `test-results/main-reversible-dist-localhost-20260520T1711Z/rerun/14-caddyfile-after-dist-start.txt`,
  `rerun/16-localhost-after-dist-start.headers`,
  `rerun/17-branded-after-dist-start.headers`, and
  `rerun/18-localhost-after-dist-start.body.txt` containing `Your Context`.
- Latest restore/harness-safety evidence, 2026-05-20T17:11Z:
  the main proof restored the machine to installed 0.1.86 afterward:
  `/Applications/1Context.app` owns the menu LaunchAgent, installed Caddy is the
  only local-web Caddy process, installed diagnose is healthy on the branded
  route, and localhost fails again as expected for the old branded-only
  Caddyfile. Evidence:
  `test-results/main-reversible-dist-localhost-20260520T1711Z/99-restore3-installed-state.log`
  and `99-installed-diagnose-after-restore3.txt`. This lap also exposed a
  harness-safety hazard: a fake-home Swift CLI/RPC dogfood run temporarily
  wrote the real user LaunchAgent plist because `launchctl` is user-session
  global. The plist was restored from backup, and future fake-home CLI dogfood
  should not bootstrap the real `com.haptica.1context.menu` label unless that is
  the explicit proof target.
- Latest Swift CLI/RPC consumer evidence, 2026-05-20T17:11Z:
  Worker FQ ran `test-results/worker-fq-swift-cli-rpc-20260520T1711Z`. The
  shipped Swift CLI read/publish commands (`wiki list`, `wiki page-status`,
  `wiki page-open`, `wiki publish-status`, `wiki publish`) matched direct Rust
  core receipts with `all_passed=true`, and richer create/edit/talk/mail
  behavior worked through the Swift daemon JSON-RPC bridge. The remaining API
  gap is ergonomic, not functional: create/edit/talk/mail exist through daemon
  JSON-RPC and direct Rust core, but are not yet first-class `1context wiki ...`
  subcommands.
- Latest sitemap/fallback browser evidence, 2026-05-20T17:11Z:
  Worker FR ran `test-results/worker-fr-sitemap-fallback-20260520T1711Z`,
  creating a disposable custom runtime with configured pages, generated
  site-template fallback, nested pages, hidden/private page, people/org/topic/
  project pages, talk entries, attachment rendering, and delete -> broken-link
  warning -> repair -> restore. Final render: 40 routes, 22 pages, 18 talk
  routes, zero missing required routes, and final link diagnostics `ok`.
  `npm test` in `wiki-engine` passed 16/16, the harness passed `node --check`,
  and browser proof/screenshots covered menu, search, and Agent view.
- Current remaining dogfood gaps, 2026-05-20T17:11Z:
  no blocking dist-start local-web migration, Swift CLI read/publish, daemon
  JSON-RPC, sitemap fallback, browser render, attachment, or repair/restore gap
  is open from this lap. The next proof should be quieter and more production
  shaped: with no concurrent fake-home launchctl workers, install or update to
  signed 0.1.87, prove the signed app becomes owner of menu/runtime/Caddy,
  prove localhost and branded routes both serve, and prove app restart preserves
  the migrated multi-host Caddy state.
- Latest signed-update readiness evidence, 2026-05-20T17:27Z:
  main lane captured
  `test-results/main-signed-update-readiness-20260520T1727Z`, and Worker FS
  captured
  `test-results/worker-fs-signed-update-readiness-20260520T1727Z`. The release
  manifest validates as `0.1.87` over `0.1.86` with mandatory update policy.
  Installed `/Applications/1Context.app` was inspected read-only and remains
  signed/notarized/stapled `0.1.86` with running menu/runtime LaunchAgents. The
  local `dist/` surface is not release proof material: `dist/1Context.app` is
  ad-hoc signed and Gatekeeper-rejected, `dist/1Context-0.1.87-macos-arm64.dmg`
  is not signed/stapled, local `dist/appcast.xml` is stale at `0.1.63`, and
  GitHub has no public `v0.1.87` release yet.
- Latest release-proof guard evidence, 2026-05-20T17:27Z:
  the direct remote Sparkle proof script now refuses to mutate the installed app
  unless `ONECONTEXT_UPDATE_RUNNER_I_UNDERSTAND_DESTRUCTIVE=1` is set. Evidence:
  `release/tools/proof/prove-remote-sparkle-update.sh` and
  `scripts/test-release-train.sh`; Worker FS passed
  `./scripts/release-train.sh manifest validate`,
  `./scripts/release-train.sh validate --channel dev`,
  `./scripts/release-train.sh prove --dry-run --ref main`,
  `npm --prefix release/runner test`, and `./scripts/test-release-train.sh`
  without installing or overwriting anything.
- Latest fake-home LaunchAgent safety fix, 2026-05-20T17:27Z:
  Worker FT patched `LaunchAgentManager` so user LaunchAgent lifecycle methods
  only run when runtime paths exactly match the standard installed-user paths
  for the same home directory. This closes the earlier dogfood hazard where a
  fake/disposable runtime could repair or rewrite the real
  `gui/$uid/com.haptica.1context.menu` label. Evidence:
  `test-results/worker-ft-launchctl-safety-20260520T1727Z/SUMMARY.md` and
  focused `LaunchAgentManagerTests`.
- Latest agent API gap evidence, 2026-05-20T17:27Z:
  Worker FU ran
  `test-results/worker-fu-agent-api-gap-20260520T1727Z` through both direct
  Rust core and Swift daemon JSON-RPC using only disposable runtimes. It created
  and edited pages, appended talk with one attachment in each path, read mail,
  tombstoned and restored pages, and ended with validation blockers `0`.
  `1context wiki validate` is now a first-class Swift CLI command. Remaining
  CLI ergonomics are still open: page create/write/patch/tombstone/restore,
  talk append, agent inbox, mail read, standalone mail send, and attachment
  metadata are not yet named `1context wiki ...` subcommands.
- Latest integrated verification evidence, 2026-05-20T17:27Z:
  after the worker patches were present in the shared workspace, main lane
  passed `swift test --package-path macos --filter
  'LaunchAgentManagerTests|WikiCoreRPCBridgeTests'` with 18 tests, `cargo test
  --package onecontext-wiki-daemon` with 18 tests, `./scripts/test-release-train.sh`,
  and `swift build --package-path macos --product 1context`.
- Current remaining dogfood gaps, 2026-05-20T17:27Z:
  the wiki core and disposable agent dogfood are healthy enough to keep using.
  The production release proof is blocked until the notary profile
  `1context-notary` is restored, a clean/tagged protected lane creates fresh
  signed/notarized `v0.1.87` artifacts and appcast assets, and the destructive
  self-hosted update proof is explicitly opted in. The remaining agent-facing
  product gap is CLI ergonomics for the mutating page/talk/mail verbs already
  available through Rust core and daemon JSON-RPC.
- Latest Swift CLI page-mutation evidence, 2026-05-20T17:42Z:
  main lane added first-class `1context wiki page-create`, `page-write-body`,
  `page-patch-body`, `page-delete`, and `page-restore` commands over the
  daemon JSON-RPC bridge. Evidence:
  `test-results/main-swift-cli-page-mutation-20260520T1742Z` created
  `cli-page-proof-1742`, opened it for a source hash, wrote and patched the
  body through files, published, observed `state=rendered`, tombstoned,
  restored, and ended with `validation_after_restore=ok`.
- Latest Swift CLI talk/mail evidence, 2026-05-20T17:42Z:
  main lane added first-class `1context wiki agent-register`, `agent-inbox`,
  `talk-append`, `mail-inbox`, `mail-read`, `mail-mark`, `mail-claim`,
  `notify-poll`, and `notify-ack` commands. Evidence:
  `test-results/main-swift-cli-talk-mail-20260520T1742Z` created and published
  `cli-mail-page-1742`, registered an agent as
  `role://cli-mail-page-1742.curator`, appended talk with one attachment, read
  the delivered mail, polled and acked one notification (`1 -> 0`), claimed and
  marked the message done, reduced actionable inbox pressure (`1 -> 0`), and
  proved `next_action_after_talk=none`, `next_action_after_mail_done=none`, and
  final `validation_status=ok`.
- Latest talk/mail consumer gap evidence, 2026-05-20T17:42Z:
  Worker FV ran
  `test-results/worker-fv-talk-mail-cli-gap-20260520T1742Z` through direct Rust
  CLI and daemon JSON-RPC. Core talk/mail/notification behavior stayed green,
  but it found a real consumer gap: attachment filename/caption/alt metadata is
  still not first-class for agents. Metadata requested through daemon JSON-RPC
  came back as the raw source filename and dropped caption/alt, because the
  current Rust CLI and Swift bridge path are effectively `--attachment <path>`.
- Latest rendered relationship/browser evidence, 2026-05-20T17:42Z:
  Worker FW ran
  `test-results/worker-fw-rendered-links-browser-20260520T1742Z`, building 9
  custom project/topic/person/organization pages with nested routes, internal
  links, agents, reviewer list, page watch/role assignment, talk messages, and
  an attachment. It tombstoned/restored `/organizations/fw-haptica-lab/wiki-team`;
  delete publish reported 3 broken inbound links, restore publish returned link
  diagnostics to `ok`, final site had 29 routes and 0 broken internal links,
  and isolated Playwright passed 8/8 browser-visible checks.
- Latest non-destructive release status evidence, 2026-05-20T17:42Z:
  Worker FX ran
  `test-results/worker-fx-release-safety-and-status-20260520T1742Z`. The
  destructive Sparkle proof guard works against fake paths, installed
  `/Applications/1Context.app` remains signed/notarized/stapled `0.1.86`, local
  `dist/` remains ad-hoc/rejected and not release proof material, public latest
  remains `v0.1.86`, no `v0.1.87` release exists yet, and the notary profile
  `1context-notary` is still missing.
- Latest integrated verification evidence, 2026-05-20T17:42Z:
  after the Swift CLI command expansion, main lane passed
  `swift build --package-path macos --product 1context` and
  `swift test --package-path macos --filter WikiCoreRPCBridgeTests`. The
  disposable CLI page and talk/mail proofs both used fake runtime homes and
  short sockets; no `/Applications`, real LaunchAgents, or live `~/1Context`
  paths were mutated.
- Current remaining dogfood gaps, 2026-05-20T17:42Z:
  the agent-facing Swift CLI now covers page mutation, talk append, mail inbox,
  mail read/claim/mark, agent inbox, and notification poll/ack. Remaining wiki
  consumer gaps are narrower: attachment metadata needs a first-class
  end-to-end path; standalone mail-send remains intentionally absent or
  undecided because talk append is the durable send path; notification ack is
  wakeup-only and still requires mail claim/mark to close work. The signed
  0.1.87 production proof remains blocked on missing notary credentials, fresh
  signed/notarized artifacts, public appcast/release assets, and explicit
  destructive update-proof opt-in.
- Latest attachment metadata fix, 2026-05-20T18:00Z:
  main lane made attachment metadata first-class through the agent adapter path.
  The Rust CLI now accepts `--attachment-filename`, `--attachment-caption`, and
  `--attachment-alt` aligned with repeated `--attachment` flags, and the Swift
  RPC bridge/CLI passes object or aligned metadata through instead of flattening
  everything to paths. Evidence:
  `test-results/main-attachment-metadata-cli-20260520T1800Z` sent
  `raw-source-name.txt` as `agent-facing-name.txt` with caption and alt text;
  `wiki.talk.append`, `wiki.mail.read`, and the persisted talk source all
  preserved `agent-facing-name.txt`, `Agent-facing caption`, and
  `Agent-facing alt text`.
- Latest rendered attachment evidence, 2026-05-20T18:00Z:
  Worker FY ran `test-results/worker-fy-attachment-render-20260520T1800Z`,
  creating one disposable page and a talk message with three attachments, then
  publishing and serving the rendered site. Browser/fetch proof returned HTTP
  200 for the talk page and text, PNG, and markdown attachment routes; filenames
  and captions rendered correctly. The remaining polish gap is render
  duplication: attachments appear once in the generated markdown body and again
  in the renderer attachment aside. Stored `alt_text` is preserved in
  metadata/mail but is not yet surfaced in rendered HTML or markdown.
- Latest Swift CLI regression evidence, 2026-05-20T18:00Z:
  Worker FZ ran
  `test-results/worker-fz-swift-cli-regression-20260520T1800Z` against a fake
  runtime/socket through the expanded Swift CLI. It covered page create/write/
  patch/delete/restore, agent register, talk append, mail inbox/read/claim/
  mark, notify poll/ack, publish, and validate. Final validation was `ok` with
  0 blockers, final publish was `published` with link health `ok`, actionable
  mail dropped `1 -> 0`, notifications dropped `1 -> 0`, four failure-envelope
  checks passed, and publish boundaries behaved correctly.
- Latest sitemap/delete/link evidence, 2026-05-20T18:00Z:
  Worker GA ran
  `test-results/worker-ga-link-delete-sitemap-20260520T1800Z`, creating 9
  custom project/topic/person/organization pages, publishing, tombstoning
  `ga-topic-link-health`, confirming 6 inbound-link warnings and 6 broken links
  after delete publish, restoring the page, and republishing back to 0 broken
  links. Final proof: 29 routes, 12/12 required routes present, validation
  blockers 0, broken internal links 0, and HTTP route/menu/talk/attachment
  checks 7/7.
- Latest integrated verification evidence, 2026-05-20T18:00Z:
  main lane passed `cargo test --package onecontext-wiki-core
  talk_attachments_copy_media_and_duplicate_names`, `cargo test --package
  onecontext-wiki-daemon talk_attachment_metadata_flags_align_with_attachments`,
  `cargo test --package onecontext-wiki-daemon
  value_cli_flags_reject_dangling_or_flag_values_before_mutating`, full
  `cargo test --package onecontext-wiki-daemon`, `swift test --package-path
  macos --filter WikiCoreRPCBridgeTests`, and `swift build --package-path macos
  --product 1context`.
- Current remaining dogfood gaps, 2026-05-20T18:00Z:
  the main agent-facing API gap from 17:42 is closed: attachment
  filename/caption/alt metadata now survives the Swift CLI and daemon/Rust path.
  Remaining wiki polish is in rendered talk presentation: avoid duplicate
  attachment listings and decide how/where `alt_text` should surface. Product
  semantics remain unchanged: talk append is still the durable mail-send path,
  and notification ack still only clears wakeup pressure while mail claim/mark
  closes the work. The signed 0.1.87 production proof remains blocked on the
  missing `1context-notary` profile, fresh signed/notarized artifacts, public
  appcast/release assets, and explicit destructive update-proof opt-in.
- Latest rendered attachment polish fix, 2026-05-20T18:15Z:
  main lane updated `wiki-engine/src/renderer/talk-folder.mjs` so generated
  core talk entries keep their portable `## Attachments` markdown in source,
  while rendered talk pages de-duplicate that generated trailing section and
  use the structured attachment metadata for presentation. Captions and
  `alt_text` now surface in both rendered HTML and markdown twins. Evidence:
  `test-results/main-rendered-attachment-polish-20260520T1815Z` produced a
  Swift/Rust-generated talk entry with `agent-render-proof.txt`, `Rendered
  caption`, and `Rendered alt text`; the rendered HTML had one attachment href,
  the markdown twin had one attachment link, both contained caption/alt text,
  and the copied attachment payload was present.
- Latest rendered attachment regression evidence, 2026-05-20T18:15Z:
  Worker GB ran
  `test-results/worker-gb-rendered-attachment-regression-20260520T1815Z`,
  creating/rendering/serving `/topics/gb-rendered-attachment-regression` with
  text, PNG, and markdown attachments. Captions and alt text were represented
  in both rendered HTML and markdown, each attachment had `html_links=1` and
  `markdown_links=1`, page/talk/markdown/attachment routes returned 200, and
  `gap-list.json` was empty.
- Latest Swift CLI metadata regression evidence, 2026-05-20T18:15Z:
  Worker GC ran
  `test-results/worker-gc-swift-cli-metadata-regression-20260520T1815Z`,
  executing 21 Swift CLI commands against a disposable daemon socket. It
  created/published a page, registered author and curator agents, appended talk
  with three attachments, read mail, checked agent inbox, acked notification,
  claimed/marked mail done, and validated. All 3/3 filenames, captions, and
  alt text stayed aligned and raw source names did not leak. Publish boundaries
  remained correct and final validation was `ok` with 0 blockers.
- Latest sitemap/delete/mail evidence, 2026-05-20T18:15Z:
  Worker GD ran `test-results/worker-gd-sitemap-delete-mail-20260520T1815Z`,
  creating 10 project/topic/people/org pages, publishing 31 routes with 13/13
  required routes present, tombstoning `gd-topic-link-integrity`, surfacing 8
  broken internal links after delete publish, restoring it, and returning final
  link diagnostics to 0 broken links. Talk/mail proof also passed with role
  mailbox, direct agent inbox, list subscription, attachments, claim/mark done,
  and notification ack (`1 -> 0`). Curl passed 6/6 and Playwright passed 7/7.
- Latest integrated verification evidence, 2026-05-20T18:15Z:
  main lane passed `npm test --prefix wiki-engine` with 16/16 tests,
  `swift test --package-path macos --filter WikiCoreRPCBridgeTests`, and
  `cargo test --package onecontext-wiki-daemon
  talk_attachment_metadata_flags_align_with_attachments`. The integrated
  rendered-attachment proof and all worker dogfood runs used disposable
  runtimes/sockets; no `/Applications`, real LaunchAgents, or live `~/1Context`
  paths were mutated.
- Current remaining dogfood gaps, 2026-05-20T18:15Z:
  the attachment API and rendered attachment polish gaps are closed for the
  current design: metadata survives through the agent path and the rendered
  talk page has one attachment link per attachment with caption/alt text
  visible. Product semantics remain intentionally explicit: talk append is the
  durable mail-send path, notification ack clears wakeup pressure, and
  mail claim/mark closes the work. The signed 0.1.87 production proof is still
  outside this disposable dogfood lane until `1context-notary`, fresh signed/
  notarized artifacts, public appcast/release assets, and destructive
  update-proof opt-in are available.
- Latest docs/API alignment evidence, 2026-05-20T18:30Z:
  main lane and Worker GE updated the consumer-facing wiki docs to match the
  current surface: `onecontext-wiki` is the full local workbench, the installed
  `1context wiki` CLI is the app-facing subset, and daemon JSON-RPC forwards
  the broader collaboration surface through `WikiCoreRPCBridge`. The docs now
  call out page lifecycle commands, explicit publish vs transitional
  `wiki.refresh`, talk/mail/notify semantics, fallback templates, generated
  site pages, attachment filename/caption/alt metadata, and rendered
  attachment de-duplication. Evidence:
  `test-results/worker-ge-docs-api-audit-20260520T1830Z`; GE also passed
  `swift test --package-path macos --filter WikiCoreRPCBridgeTests` and
  `git diff --check` on the touched docs.
- Latest installed Swift CLI ergonomics evidence, 2026-05-20T18:30Z:
  Worker GF ran
  `test-results/worker-gf-cli-help-and-failure-20260520T1830Z` through 28
  installed-CLI commands against a disposable runtime/socket. Failure-envelope
  checks passed 15/15, the happy path created/wrote a page, registered an
  agent, appended talk with filename/caption/alt attachment metadata, read
  mail, polled/acked one notification, and validated with 0 blockers. Main lane
  then fixed two remaining rough edges: `1context wiki <subcommand> --help`
  now prints focused wiki help without touching the socket, and missing
  `notify-poll` now reports `wiki notify-poll requires an agent id`.
- Latest medium graph/render dogfood evidence, 2026-05-20T18:30Z:
  Worker GG ran `test-results/worker-gg-render-smoke-20260520T1830Z`,
  creating 8 custom pages across projects, topics, people, organizations, and
  nested routes. It published 27 final routes with 10/10 required routes
  present, preserved 3/3 talk/mail attachment filenames/captions/alt text,
  cleared notification pressure `1 -> 0`, cleared direct mail actionable
  pressure `1 -> 0`, tombstoned a page to surface 7 broken links, restored it
  back to 0 broken links, and passed 8/8 curl checks across route pages,
  search API, menu contents, talk page, and attachment routes.
- Latest integrated verification evidence, 2026-05-20T18:30Z:
  after integrating worker findings, main lane passed
  `swift build --package-path macos --product 1context`, verified
  `macos/.build/debug/1context wiki --help`, verified
  `macos/.build/debug/1context wiki page-create --help` is non-mutating help,
  verified missing `notify-poll` exits with the agent-id-specific error, and
  passed `git diff --check` across the touched docs, CLI file, goal ledger, and
  worker evidence directories.
- Remaining dogfood gaps observed at 2026-05-20T18:30Z:
  the wiki core is now feeling usable for agent work in the disposable lanes:
  create/write/patch/delete/restore, publish, list/status, talk/mail,
  attachment metadata, notification pressure, link diagnostics, and rendered
  route proof are all exercised repeatedly. Remaining polish was narrower:
  Swift CLI structured error output and installed wrapper coverage for
  watch/list/subscription helpers were not yet closed. Signed 0.1.87 release
  proof remained blocked on credentials/artifacts outside this wiki dogfood loop.
- Latest subscription/list/workbench evidence, 2026-05-20T18:45Z:
  Worker GH ran
  `test-results/worker-gh-subscription-workbench-20260520T1845Z` against a
  disposable runtime home and fake daemon socket. The installed-facing
  `1context wiki` CLI now exercises the broader workbench through
  `WikiCoreRPCBridge`: `agent-identify`, `whoami`, `agent-list`,
  `agent-status`, `agent-inbox`, `agent-claim`, `agent-heartbeat`,
  `list-create`, `lists`, `list-status`, `list-members`, `page-watch`,
  `page-unwatch`, `page-assign-role`, `mail-subscribe`, `mail-unsubscribe`,
  `mail-subscriptions`, `mail-read`, `mail-mark-all`, `notify-poll`, and
  `notify-ack`. The run executed 32 commands and passed 25/25 checks:
  subscriptions went from 4 active entries before scoped unsubscribe/unwatch
  to 1 remaining role subscription after cleanup, notification pressure went
  `2 -> 0`, and final validation had 0 blockers. The evidence records
  `mutated_real_surfaces: false` and verifies no command args touched
  `/Applications`, real `launchctl`, or live `~/1Context`.
- Latest integrated verification evidence, 2026-05-20T18:45Z:
  main/Worker GH passed `swift build --package-path macos --product 1context`,
  `swift build --package-path macos --product 1contextd`, `node --check` on
  the GH workbench harness, the GH harness itself, and `git diff --check`
  across the touched Swift CLI file plus GH evidence files.
- Remaining dogfood gaps observed at 2026-05-20T18:45Z:
  the installed CLI wrapper gap for watch/list/subscription workbench commands
  is now closed for the current design. Remaining polish was mostly ergonomic:
  structured Swift CLI error output, command-specific help, and a fresh copied
  runtime that validates with non-blocking configured-page warnings until
  publish/backfill is run. Signed 0.1.87 production proof remains outside this
  disposable dogfood lane until credentials/artifacts are available.
- Latest main installed-workbench smoke evidence, 2026-05-20T18:45Z:
  main lane added installed `1context wiki` wrappers for the common workbench:
  page watch/unwatch/assign-role, list create/list/status/members, agent
  identify/heartbeat/retire/whoami/list/status/claim, mail
  subscribe/unsubscribe/subscriptions/mark-all, and the existing page/talk/mail
  operations. Evidence:
  `test-results/main-installed-cli-workbench-wrappers-20260520T1845Z`
  exercised 24 installed-CLI commands against a disposable runtime and daemon
  socket. It created a list, subscribed/watched/assigned an agent, appended
  talk, read mail, cleared actionable inbox pressure `2 -> 0` with
  `mail-mark-all`, cleared notification pressure `2 -> 0`, cleaned up with
  `page-unwatch` and `mail-unsubscribe`, and validated with 0 blockers.
- Latest error-envelope dogfood evidence, 2026-05-20T18:45Z:
  Worker GJ ran `test-results/worker-gj-error-envelope-20260520T1845Z`,
  exercising 31 Rust and Swift CLI success/failure cases against an isolated
  runtime and disposable daemon. Rust CLI errors were structured JSON with
  typed codes including `source_hash_mismatch`, `body_patch_ambiguous`,
  `invalid_address`, and `tombstoned_page`. Swift CLI RPC failures preserve
  those Rust JSON envelopes only inside stderr prose, and local argument errors
  are still prose-only. GJ patched the safe local footgun: Swift
  `page-write-body`, `page-patch-body`, and `talk-append` now reject mutually
  exclusive `--body/--body-file`, `--find/--find-file`, and
  `--replace/--replace-file` inputs instead of silently choosing one.
- Latest browser graph evidence, 2026-05-20T18:45Z:
  Worker GI ran `test-results/worker-gi-browser-graph-20260520T1845Z`,
  creating 5 project/topic/person/org/nested pages, publishing 21 final routes
  with 8/8 required routes present, preserving 2/2 talk attachment filenames,
  captions, and alt text, clearing notification pressure `1 -> 0`, and
  clearing inbox actionable pressure `1 -> 0`. It verified project, topic,
  talk, attachment, and search routes with curl, then used headless Chrome for
  project/talk/attachment DOM checks plus a screenshot. Tombstone/delete
  surfaced 2 broken internal links and removed the nested route; restore
  returned broken internal links to 0.
- Latest integrated verification evidence, 2026-05-20T18:45Z:
  after integrating worker findings, main lane passed
  `swift build --package-path macos --product 1context`, `swift test
  --package-path macos --filter WikiCoreRPCBridgeTests`, installed-CLI help
  proof for the new wrapper commands, the disposable installed-workbench smoke,
  local mutually-exclusive flag checks, and `git diff --check` across the
  touched Swift CLI, docs, goal ledger, and evidence directories. All
  disposable proofs used fake runtime homes/sockets; hygiene checks found no
  leftover worker server/processes and no `/Applications`, real LaunchAgent, or
  live `~/1Context` mutation by the dogfood harnesses.
- Current remaining dogfood gaps, 2026-05-20T18:45Z:
  the common agent workbench is now reachable through both the Rust CLI and
  installed Swift CLI. Command-specific help is still broad rather than
  per-command. The installed `/Applications` app remains version `0.1.86`, so
  current debug CLI behavior will not be user-visible until the next signed
  release path is unblocked.
- Latest structured Swift CLI failure evidence, 2026-05-20T19:01Z:
  main lane patched `macos/Sources/OneContextCLI/main.swift` so `1context wiki`
  local argument failures return stdout JSON instead of stderr prose, and daemon
  RPC failures preserve the Rust-core JSON envelope directly on stdout. Evidence:
  `test-results/main-swift-json-errors-20260520T1901Z` proves five nonzero
  cases all have `stdout_json=true`, `stderr_empty=true`, `status=error`,
  operation names, typed error codes, and repair hints: missing `notify-poll`
  agent (`invalid_arguments`), mutually exclusive write-body inputs
  (`invalid_arguments`), extra `page-status` argument
  (`unexpected_arguments`), unknown subcommand (`unknown_command`), and live
  daemon/RPC unknown page (`unknown_page`). Build proof:
  `swift build --package-path macos --product 1context` and
  `swift build --package-path macos --product 1contextd`.
- Latest browser/harness hygiene evidence, 2026-05-20T19:01Z:
  Worker GL recovered local evidence in
  `test-results/worker-gl-browser-render-20260520T1901Z`: two custom pages,
  list/watch/role wiring, two talk messages, successful publish, clean route
  manifest entries for page and talk routes, and final validation. The useful
  defect was harness-level: an ad hoc Playwright run used Playwright's default
  `test-results` output directory and cleaned earlier untracked evidence before
  GL recovered its fixture. Main lane added `playwright.config.cjs` so ad hoc
  Playwright output goes to `.playwright-artifacts/test-output`, ignored
  `.playwright-artifacts/`, and proved the guard with
  `test-results/playwright-outputdir-guard-20260520T1901Z`: a marker inside
  `test-results` and the GL evidence directory survived a failing Playwright
  run. The same GL proof records a real remaining browser/tooling gap:
  local Playwright cannot yet resolve `@playwright/test`, and raw rendered HTML
  did not expose the expected `data-talk-toggle` or `data-view-set="agent"`
  controls on the custom pages.
- Sidecar reports to rerun as durable local evidence, 2026-05-20T19:01Z:
  GK reported successful project/topic/tool cross-link dogfood with clean
  publish and page-status metadata, and GM reported agent inbox/list/mail
  ergonomics including the awkward split between list/role-delivered mail and
  primary-address `mail-claim`/`mail-mark`. Because the Playwright cleanup
  removed untracked `test-results` directories during this lap, those reports
  are treated as leads, not durable evidence, until recreated under the guarded
  evidence setup.
- Latest local browser harness proof, 2026-05-20T19:18Z:
  main lane installed root dev-only `@playwright/test`, added
  `playwright.config.cjs` with guarded output under `.playwright-artifacts/`
  and a named `chromium` project, and ignored root `node_modules/`,
  `.playwright-cli/`, and `.playwright-artifacts/`. Evidence:
  `test-results/main-browser-controls-proof-20260520T1918Z` reran the recovered
  GL browser spec with `npx playwright test ... --project=chromium` against a
  served disposable site. It passed with `playwright_exit_status=0`, produced
  five screenshots, and confirmed `talkToggleCount=1`, `agentToggleCount=1`,
  `brandMenuToggleCount=1`, HTTP 200 status, and markdown alternates for
  `/topics/gl-navigation-hub`, `/projects/gl-project-trail`, and both talk
  routes. This closes the false-negative raw-HTML concern: the controls are
  injected by `enhance.js` and must be verified in a browser runtime, not by
  grepping static HTML.
- Latest relationship-page evidence rerun, 2026-05-20T19:18Z:
  Worker GN ran
  `test-results/worker-gn-relationship-dogfood-20260520T1918Z` evidence-only,
  creating a custom project/topic/person graph, writing cross-linked edited
  bodies, publishing, serving the static site, and verifying routes. It proved
  custom pages report `created_from_template`, `edited`,
  `custom_created=true`, and `user_edited=true`; publish returned
  `route_count=17`, `broken_internal=0`, `next_action=none`; route summary
  included page and talk routes for all 3 custom pages; curl route checks
  returned 200; and final validation was clean.
- Latest mail ergonomics evidence rerun, 2026-05-20T19:18Z:
  Worker GO ran
  `test-results/worker-go-mail-ergonomics-20260520T1918Z` evidence-only through
  64 debug Swift CLI commands against a disposable runtime/socket. It exercised
  agent identify/heartbeat/status/list/retire, list/status/members, page
  watch/role assignment, role/list/direct talk delivery, `agent-inbox` vs
  `mail-inbox`, `agent-claim` vs `mail-claim`, `mail-mark` vs
  `mail-mark-all`, and `notify-poll`/`notify-ack`. The durable ergonomic gap:
  list/role mail appears in `agent-inbox`, but `mail-claim`/`mail-mark` against
  the agent primary address fail because the mailbox row belongs to the
  list/role recipient. `agent-claim`, canonical role/list recipients, and
  `mail-mark-all` work; direct mail to the agent primary address works with
  `mail-claim` and `mail-mark`.
- Latest delete/restore/browser evidence rerun, 2026-05-20T19:18Z:
  Worker GP ran
  `test-results/worker-gp-delete-restore-browser-20260520T1918Z` evidence-only,
  creating linked hub/target/probe pages, appending target talk, publishing,
  tombstoning the target, then restoring it. Delete publish reported two broken
  internal links with repair tasks; the tombstoned target page route, talk
  route, and markdown twin returned 404 while hub/probe stayed 200; normal talk
  append to the tombstoned page refused with typed `tombstoned_page`; explicit
  archive-maintenance talk with `--allow-tombstoned` succeeded; restore brought
  page/talk/markdown routes back to 200 and final `wiki.validate` returned
  `status=ok`. The caveat is semantic, not blocking: `wiki list` keeps
  tombstoned pages as historical metadata even when the browser route is gone.
- Latest integrated verification evidence, 2026-05-20T19:18Z:
  after integrating main and sidecar findings, main lane passed
  `swift build --package-path macos --product 1context`, `npm ls
  @playwright/test --depth=0`, `node --check playwright.config.cjs`, the local
  Playwright browser proof with `--project=chromium`, `git diff --check` across
  touched docs/tooling/evidence, and bulky-artifact checks across current
  evidence directories. Process hygiene found no leftover disposable
  `serve-site`, worker daemon, or Playwright processes.
- Latest primary-address mail fix, 2026-05-20T19:33Z:
  main lane fixed the durable ergonomics gap from Worker GO. When an active
  agent sees a list/role/page delivery through `agent-inbox`, `mail-claim` and
  `mail-mark` can now be called with that agent's `primary_address`; the core
  resolves the visible delivery and updates the canonical recipient mailbox.
  Regression coverage passed:
  `cargo test -q -p onecontext-wiki-core mail_claim_and_mark_accept_agent_primary_address_for_visible_subscription_delivery`,
  plus nearby `mail_mark_cannot_create_unowned_claims` and
  `list_status_points_at_mail_actions_for_open_list_work`. Daemon-backed
  evidence:
  `test-results/main-agent-inbox-primary-mail-20260520T1933Z-rerun` used the
  debug Swift CLI through a disposable daemon socket, proved the exact primary
  mailbox had no row, proved `agent-inbox` showed the canonical list delivery,
  claimed one list message with `--recipient <primary_address>`, marked a
  second list message done with `--recipient <primary_address>`, and verified
  the final list mailbox recorded the canonical `claimed` and `done` states.
  The first run also exposed a harness detail worth remembering: Unix-domain
  daemon sockets need short paths, so disposable sockets should live under
  `/tmp` rather than deep evidence folders.
- Latest sidecar dogfood evidence, 2026-05-20T19:33Z:
  Worker GQ passed browser-visible menu/search/talk/Reader-Agent navigation
  under `test-results/worker-gq-browser-menu-20260520T1933Z`: three custom
  pages, clean publish/validate, HTTP 200 route checks, Playwright screenshots,
  brand-menu navigation, search modal/static API, talk toggle, and Agent view.
  Worker GR passed runtime-default/fallback dogfood under
  `test-results/worker-gr-runtime-defaults-fallback-20260520T1933Z`: generated
  `site_pages` correctly refuse source edits, explicit custom-page placement
  publishes cleanly, and final validation is clean. GR found one real metadata
  gap: `page-create --nav-order 305` writes `wiki.toml`, but `wiki.list` and
  `page-status` report `nav_order: null`. Worker GS passed notification
  lifecycle dogfood under
  `test-results/worker-gs-notify-lifecycle-20260520T1933Z`: identify,
  heartbeat, watches, role/list talk delivery, notify poll/ack, stale lease,
  retire, cleanup, and validation. GS found the stale-agent error surface is
  semantically awkward (`unknown_active_agent` instead of an expired-lease
  code), and confirmed the correct revival path is `agent-identify` plus
  renewing expired watches, not `agent-heartbeat`.
- Latest nav-order metadata fix, 2026-05-20T19:48Z:
  main lane fixed the Worker GR placement gap. `nav_order` is now parsed into
  page records, generated site-page records, `wiki.page.status`, `wiki.list`,
  and `wiki.page.open`. Regression coverage passed:
  `cargo test -q -p onecontext-wiki-core page_status_distinguishes_runtime_defaults_from_custom_template_pages`
  and the nearby primary-address mail regression. Closed-loop CLI evidence:
  `test-results/main-nav-order-roundtrip-20260520T1948Z` copied a fresh
  `runtime/1Context`, created `main-nav-order-1948` with
  `--nav-section primary --nav-order 305`, and proved the create receipt,
  `page-status`, `wiki.list`, `page-open`, and persisted `wiki.toml` all agree
  on `nav_order=305`.
- Latest browser menu ordering fix, 2026-05-20T19:48Z:
  Worker GU found that visual brand-menu order still violated page-level
  `nav_order`: shared family groups bucketed `GU Primary Early` and
  `GU Primary Late` together before default pages. Main lane patched
  `wiki-engine/tools/render-site.mjs` so menu grouping preserves ordered
  contiguous runs instead of globally bucketing by group label. Regression
  coverage passed:
  `npm test --prefix wiki-engine -- --test-name-pattern "brand menu preserves page nav_order"`.
  Closed-loop browser evidence:
  `test-results/main-gu-nav-order-browser-rerun-20260520T1948Z` reran the GU
  placement harness after the renderer patch; publish, HTTP proof, browser
  proof, search, talk routes, hidden-page menu exclusion, and menu order all
  passed. `nav-order-analysis.json` now reports `mismatch_count=0`, and
  `browser-proof.json` reports `pass=true`.
- Latest sidecar dogfood evidence, 2026-05-20T19:48Z:
  Worker GV passed talk attachments/delete/restore under
  `test-results/worker-gv-talk-attachments-delete-20260520T1948Z`: proposal
  with three attachments, duplicate filename suffixing, replies via
  `--reply-to` and explicit `--thread-id`, `mail-read` message/thread
  expansion, tombstone refusal for normal talk, archive note with
  `--allow-tombstoned`, delete publish 404s, restore publish 200s, and source
  attachment survival after restore. Worker GT prepared but did not run a
  stale-agent harness before wrap; main-lane hygiene discarded its large
  copied build/source sandbox and preserved only the small setup note under
  `test-results/worker-gt-expired-lease-20260520T1948Z`. Treat that folder as
  setup-only, not stale-lease behavior proof.
- Latest stale-agent lifecycle fix, 2026-05-20T20:04Z:
  main lane fixed the Worker GS expired-lease ergonomics gap. Stale agents now
  fail protected control commands with `agent lease expired`, and the daemon
  maps that to structured `stale_agent` errors with a direct repair hint to
  call `agent-identify` on the same thread id. Regression coverage passed:
  `cargo test -q -p onecontext-wiki-core stale_agent_control_commands_require_identify_refresh`
  and `cargo test -q -p onecontext-wiki-daemon stale_agent_errors_are_actionable`.
  Closed-loop CLI evidence:
  `test-results/main-stale-agent-expired-lease-20260520T2004Z` identified an
  agent with a one-second lease, observed `agent-status` liveness `stale` and
  `next_action=agent_identify`, proved `agent-inbox`, `notify-poll`, and
  `agent-heartbeat` all return `stale_agent`, then refreshed the same thread
  with `agent-identify` and used `agent-inbox` successfully.
- Latest sidecar dogfood evidence, 2026-05-20T20:04Z:
  Worker GX passed full inbox triage through the disposable Swift CLI/daemon
  under `test-results/worker-gx-inbox-triage-20260520T2004Z`: 65 commands
  across agent register, page create/write, list create, mail subscribe, page
  watch, role assignment, talk delivery, inbox/read/claim/mark, notify
  poll/ack, and final status checks; all ended with zero actionable work for
  curator, reviewer, and page-watcher. Worker GY passed render-failure
  last-good preservation under
  `test-results/worker-gy-render-failure-last-good-20260520T2004Z`: a known
  good route stayed HTTP 200 with the same HTML hash after a controlled
  frontmatter render failure, restore republished cleanly, and final
  validation was clean. GY found a real validation nuance: `wiki.validate`
  does not pre-detect that bad renderer frontmatter; the actionable error
  appears during `wiki.publish`, which does preserve last-good output. Worker
  GW did not execute its route/browser loop before wrap, so
  `test-results/worker-gw-project-topic-links-20260520T2004Z` is inconclusive
  setup-only evidence, not product proof.
- Latest integrated verification, 2026-05-20T20:04Z:
  after the stale-agent patch, main lane passed `cargo fmt --check`, focused
  core and daemon cargo tests, `cargo build -q -p onecontext-wiki-daemon`,
  `swift build --package-path macos --product 1context`, `git diff --check`
  across touched source/docs/evidence, bulky-artifact checks for the current
  evidence folders, and process hygiene with no leftover disposable server,
  daemon, or `onecontext-wiki` process.
- Latest validate/frontmatter dogfood, 2026-05-20T20:19Z:
  main lane fixed the Worker GY validation nuance. `wiki.validate` now checks
  authored page source frontmatter against renderer-required fields and enum
  values, and invalid source frontmatter becomes a blocking
  `invalid_page_frontmatter` issue with `next_action=repair_source`.
  `wiki.publish` now reflects validation `next_action` and repair hints instead
  of always pointing blocked preflight at `repair_wiki_toml`. Regression
  coverage passed:
  `cargo test -q -p onecontext-wiki-core validate_blocks_renderer_incompatible_page_frontmatter`
  and
  `cargo test -q -p onecontext-wiki-daemon publish_uses_validation_repair_hints_for_source_frontmatter_blocks`.
  Closed-loop CLI evidence:
  `test-results/main-validate-render-frontmatter-20260520T2019Z` created a
  disposable page, corrupted `access`, proved `wiki.validate` returns
  `status=error`, `can_publish=false`, and `next_action=repair_source`, proved
  `wiki.publish` stops before rendering with `render_required=false`, then
  restored the source and proved validation is publishable again.
- Latest sidecar dogfood evidence, 2026-05-20T20:19Z:
  Worker GZ produced a useful relationship/browser HTTP proof under
  `test-results/worker-gz-relationship-browser-20260520T2019Z`: three custom
  pages across `/projects`, `/topics`, and `/tools`, cross-linked bodies,
  publish `status=published`, `route_count=17`, `broken_internal=0`, clean
  validation, HTTP 200 checks for page/talk/markdown routes, route manifest,
  content index, and search results for all three pages. Worker HB generated a
  browser-control fixture but the Playwright run failed with
  `ERR_CONNECTION_REFUSED` before assertions because the disposable server was
  not reachable; treat
  `test-results/worker-hb-browser-controls-20260520T2019Z` as harness failure
  evidence, not product failure. Worker HA produced only a disposable Cargo
  build cache and no useful report; the cache was removed and
  `test-results/worker-ha-tombstone-mail-20260520T2019Z` is marked
  inconclusive setup-only evidence.
