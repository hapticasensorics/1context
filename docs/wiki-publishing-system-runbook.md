# 1Context Wiki Publishing System Runbook

- Status: operating guide for the canonical V0 publishing API
- Last updated: 2026-05-20
- Latest evidence: 2026-05-20 installed Swift CLI and daemon bridge expose the
  shared Rust-core page lifecycle, publish, talk, mail, notification, and
  attachment metadata operations; the current repo Swift CLI also returns
  structured stdout JSON for wiki consumer failures

Use this with [Wiki System Architecture](wiki-system-architecture.md) and
[Wiki Publishing System API](wiki-publishing-system-api.md). The architecture
doc defines the internal spine; the API doc defines the consumer contract; this
file shows how to use the system from each side.

## End-To-End Publishing Loop

The normal closed-loop path is:

```text
compile WikiInventory
  -> register/heartbeat agents that will consume or produce wiki work
  -> read inbox headers before scanning talk folders
  -> create/delete pages when structure changes
  -> edit user source/talk/wiki.toml or append talk messages
  -> deliver mail and enqueue notifications for recipients
  -> validate inventory
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

Current caveat: the Swift host still owns local-web supervision and its
transitional render queue, but the agent-facing wiki workbench is now the
portable Rust core. Use `onecontext-wiki` for the full local workbench: page
lifecycle, validation, publish, agent directory, mail, talk, lists, and
notifications. Use `1context wiki` when you want to exercise the installed
daemon and app mirror path; it exposes the common app-facing workbench directly
and the daemon bridge accepts the same broader JSON-RPC method surface.

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

Agent dogfood proof:

```bash
node scripts/test-wiki-core-dogfood.mjs
```

This drives the live debug daemon over JSON-RPC against a disposable fake home:
validate, expected error recovery, page create/edit, list/watch/role, publish,
talk/mail/notify, HTTP route checks, tombstone, and route-disappears proof.
Use this when changing the wiki API surface or agent ergonomics.

For in-app browser inspection, leave the rendered files behind:

```bash
node scripts/test-wiki-core-dogfood.mjs --keep-runtime --leave-published
```

Then serve the emitted `app_mirror` path with `wiki-engine/tools/serve-site.mjs`
and open the reported page, talk route, and attachment route in the browser.

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

Normal CLI shape:

```bash
onecontext-wiki --root <runtime-home>/1Context page-create dummy-custom \
  --title "Dummy Custom" \
  --route /dummy-custom \
  --family-group custom \
  --family-group-title Custom \
  --family-id dummy-custom \
  --family-title "Dummy Custom" \
  --type context-page \
  --template pages/context-page.md \
  --talk-conventions-template talk/conventions.md \
  --summary "Fixture custom page generated from the fallback template." \
  --nav-order 900
```

The API derives conservative defaults where it can, and accepts explicit route
and storage placement when the caller needs a precise sitemap position.
Generated `[[site_pages]]` are not source-backed pages: `page-create` refuses
ids or routes that belong to generated site pages with `generated_site_page`.
Use `publish-status`, `status`, or the route manifest when checking generated
routes such as `/`, `/this-week`, or `/open-questions`.

## Leave A Page Watch Or Watcher List

Use `page-unwatch` when an agent is done watching a page through
`page-watch`. It cleans up both subscriptions created by `page-watch`: the
default watchers list and the durable page mailbox watcher.

```bash
onecontext-wiki --root <runtime-home>/1Context page-unwatch topics \
  --agent-id agent_codex_019e3f72 \
  --kind proposal
```

If the receipt reports `next_action=mail_subscriptions`, broader watcher rows
still exist for that page. Run a broad page unwatch to clear all page watcher
subscriptions for that agent:

```bash
onecontext-wiki --root <runtime-home>/1Context page-unwatch topics \
  --agent-id agent_codex_019e3f72
```

Use `mail-unsubscribe` when an agent is done watching a role, list, or page
mailbox. This stops future live wakeups for the matching filter; it does not
delete historical mail.

```bash
onecontext-wiki --root <runtime-home>/1Context mail-unsubscribe \
  --agent-id agent_codex_019e3f72 \
  --address list://topics.watchers \
  --relation watcher \
  --kind proposal
```

Check the result with:

```bash
onecontext-wiki --root <runtime-home>/1Context mail-subscriptions \
  --agent-id agent_codex_019e3f72

onecontext-wiki --root <runtime-home>/1Context list-members \
  list://topics.watchers
```

File escape hatch:

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

After page creation, verify the backing source and talk files:

```bash
test -f <runtime-home>/1Context/user-wiki/source/families/custom/dummy-custom/source/dummy-custom.md
test -f <runtime-home>/1Context/user-wiki/source/families/custom/dummy-custom/talk/dummy-custom.talk/_meta.yaml
```

This is the template fallback path. The page did not exist as source yet; page
creation uses `templates/pages/context-page.md` and
`templates/talk/conventions.md` to create missing user-owned files. If any
destination already exists, page creation must leave it alone and record
`skipped_existing` lifecycle evidence.

If a page has been intentionally removed, add a tombstone:

```text
<runtime-home>/1Context/user-wiki/source/families/custom/dummy-custom/source/dummy-custom.tombstone.toml
```

Page creation must then report the page as tombstoned and must not recreate
`dummy-custom.md`.

Deletion:

```bash
onecontext-wiki --root <runtime-home>/1Context page-delete dummy-custom --mode tombstone
```

## Render A Runtime Fixture

```bash
node wiki-engine/tools/render-site.mjs \
  --source-root <runtime-home>/1Context/user-wiki/source \
  --output /tmp/1context-wiki-site \
  --result-json /tmp/1context-wiki-render.json
```

Or ask the portable core to run the publish contract:

```bash
onecontext-wiki --root <runtime-home>/1Context publish \
  --wiki-engine wiki-engine \
  --node node \
  --trigger agent
```

`--node` is an executable name/path, not a shell command string.

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

`render-site.mjs` renders enabled `[[site_pages]]` from `wiki.toml` before
source-backed pages. The bundled home page emits `index.html`, `index.md`, and
a route-manifest entry for `/`, so a plain static server should not expose a
directory listing at the wiki root. Enabled site pages can also participate in
`primary_navigation` or `utility_navigation`; the default runtime ships Home,
This Week, and Open Questions as generated site-page routes. Talk markdown
manifests use the page's `talk_route` such as `/topics/talk`, not the parent
page route.

## Trigger The Current Daemon

The daemon speaks newline-delimited JSON-RPC over the daemon Unix socket.
Modern consumers should prefer `wiki.publish` for an explicit, synchronous
publish request. The older `wiki.refresh` method still exists as a
support/startup queue trigger: it is asynchronous and whole-site scoped.

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

Inspect the current installed CLI surface:

```bash
1context wiki list
1context wiki validate
1context wiki page-status topics
1context wiki page-open topics
1context wiki page-create demo-page --title "Demo Page" --route /demo-page
1context wiki page-write-body demo-page --body-file /tmp/demo-page.md
1context wiki page-patch-body topics --find "old" --replace "new" --expected-source-sha256 "$HASH"
1context wiki page-delete demo-page --mode tombstone
1context wiki page-restore demo-page
1context wiki list-create --address list://topics.reviewers --title "Topics Reviewers" --page topics --owner "$AGENT_ID"
1context wiki agent-register --thread-id "$CODEX_THREAD_ID" --capability wiki.mail
1context wiki agent-identify --thread-id "$CODEX_THREAD_ID" --capability wiki.mail
1context wiki whoami --thread-id "$CODEX_THREAD_ID"
1context wiki page-watch topics --agent-id "$AGENT_ID" --list list://topics.reviewers --kind proposal
1context wiki page-assign-role topics --agent-id "$AGENT_ID" --role curator --kind proposal
1context wiki mail-subscribe --agent-id "$AGENT_ID" --address list://topics.reviewers --relation member --kind proposal
1context wiki talk-append topics --kind proposal --subject "Proposal" --from "$AGENT_ADDRESS" --to-role curator --body "..." --attachment screenshot.png --attachment-caption "Browser proof" --attachment-alt "Screenshot of the rendered route"
1context wiki agent-inbox "$AGENT_ID"
1context wiki list-status list://topics.reviewers
1context wiki mail-inbox role://topics.curator
1context wiki mail-read --message-id "$MESSAGE_ID"
1context wiki mail-claim "$MESSAGE_ID" --recipient role://topics.curator --agent-id "$AGENT_ID"
1context wiki mail-claim "$MESSAGE_ID" --recipient "$AGENT_ADDRESS" --agent-id "$AGENT_ID" # resolves visible list/role/page deliveries
1context wiki mail-mark "$MESSAGE_ID" --recipient role://topics.curator --state done
1context wiki mail-mark "$MESSAGE_ID" --recipient "$AGENT_ADDRESS" --state done # resolves visible list/role/page deliveries
1context wiki mail-mark-all "$MESSAGE_ID" --state done
1context wiki notify-poll "$AGENT_ID"
1context wiki notify-ack "$NOTIFICATION_ID" --agent-id "$AGENT_ID"
1context wiki page-unwatch topics --agent-id "$AGENT_ID" --list list://topics.reviewers --kind proposal
1context wiki mail-unsubscribe --agent-id "$AGENT_ID" --address list://topics.reviewers --relation member --kind proposal
1context wiki publish-status
1context wiki publish --trigger agent-edit
```

Those wrappers call daemon JSON-RPC methods such as `wiki.list`,
`wiki.page.status`, and `wiki.page.open`, which delegate to the Rust wiki core
for inventory, page lifecycle, list/subscription, and mail semantics. Use
`onecontext-wiki` directly for repo-local debugging or when testing a Rust CLI
flag before it is promoted to the installed app contract.
Receipts include top-level `schema_version`, `status`, and `operation`;
`wiki.page.open` also returns top-level `title`, `route`, `collection`,
`type`, and nested `page_status` so consumers do not infer placement from URI
handles.

Daemon JSON-RPC examples for the collaboration surface:

```json
{"method":"wiki.agent.identify","params":{"threadId":"thread-123","roles":["role://topics.curator"],"capabilities":["wiki.mail"]}}
{"method":"wiki.list.create","params":{"listAddress":"list://topics.reviewers","page":{"id":"topics"},"agentId":"agent_codex_..."}}
{"method":"wiki.page.watch","params":{"page":"topics","agentId":"agent_codex_...","listAddress":"list://topics.reviewers","kinds":["proposal","reply"]}}
{"method":"wiki.page.assign_role","params":{"page":"topics","agentId":"agent_codex_...","role":"curator","kinds":["proposal"]}}
{"method":"wiki.talk.append","params":{"page":"topics","message":{"kind":"proposal","subject":"Review","fromAddress":"agent://codex/thread-123","toRoles":["curator"],"bodyFile":"/tmp/review.md"}}}
{"method":"wiki.agent.inbox","params":{"agentId":"agent_codex_..."}}
{"method":"wiki.mail.mark","params":{"messageId":"talkmsg_...","recipient":"agent://codex/thread-123","state":"done"}}
{"method":"wiki.notify.poll","params":{"agentId":"agent_codex_..."}}
```

Use the returned `agent_id` for control calls such as `wiki.agent.status`,
`wiki.agent.inbox`, `wiki.agent.claim`, and `wiki.notify.poll`. Use the
returned `primary_address` or `addresses[0]`, not `agent://<agent_id>`, when
sending direct mail through `wiki.talk.append` or reading a mailbox with
`wiki.mail.inbox`.

Use `wiki.publish` when source, tombstones, or `wiki.toml` changed and you want
the app-visible static site updated now. It is synchronous, whole-site scoped,
and backed by the Rust publisher. The CLI wrapper uses a longer socket timeout
than status calls because rendering can take more than the normal health-check
budget. A successful daemon receipt includes `app_publish.status="published"`,
which proves the Application Support mirror used by Local Web was updated.
Talk/mail/notification-only changes should not call publish for page-content
freshness. If a caller intentionally wants the static rendered talk pages to
show newly appended talk entries immediately, it may call
`wiki.publish --force`, but that is a reader-surface refresh rather than a
page-content dirty-state requirement.

Automatic source publishing is a Settings-controlled daemon behavior. The
shared preference key is `WikiAutomaticPublishCadence` in
`~/Library/Preferences/com.haptica.1context.plist`. Supported values are
`no_limit`, `1_minute`, and `30_minute`; they limit automatic publish starts
after source, tombstone, asset, or `wiki.toml` changes. Explicit `wiki.publish`
bypasses that cadence because the caller is asking for immediate proof.
`wiki.status` reports the active cadence and earliest next automatic publish
time.

Interpretation:

- `render.state = refreshing` means a manual refresh is queued or running.
- `render.state = starting` means automatic startup preparation is queued or
  running.
- `render.last.status = skipped` means the current coordinator accepted
  unchanged source inputs and republished the validated existing site.
- `render.last.status = failed` means Local Web should still serve the previous
  last-good site.

The public CLI no longer treats `1context wiki refresh` as the main consumer
interface. Prefer `1context wiki publish` for explicit publication and reserve
`wiki.refresh` for startup/support queue behavior.

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
- custom configured pages are created from fallback templates
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
- `hashes.wiki_core`
- `hashes.renderer`
- `hashes.manifest_writer`
- `render_summary.status`
- `render_summary.route_count`
- `render_summary.markdown_twin_count`

For a built app, `hashes.wiki_engine` is computed from the stripped bundled
`Contents/Resources/WikiEngine` tree, and `hashes.wiki_core` is computed from
the signed `Contents/MacOS/onecontext-wiki` helper. Package smoke recomputes
the manifest hashes against the app bundle.

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

Normal consumer shape:

```text
wiki.agent.identify(thread_id, roles, capabilities)
wiki.agent.whoami(thread_id | agent_id)
wiki.agent.status(agent_id)
wiki.mail.inbox(recipient)
wiki.list()
wiki.page.open(page)
wiki.page.patch_body(page, find, replace, expected_source_sha256)
wiki.talk.append(page, message, attachments)
wiki.mail.mark(message, recipient, state)
wiki.mail.mark_all(message, state)
wiki.validate(scope)
wiki.publish(scope, wait: "completed")
wiki.page.status(page)
wiki.agent.retire(agent_id)
```

For structural changes:

```text
wiki.page.create(page)
wiki.page.write_body(page, body_markdown, expected_source_sha256)
wiki.publish(page, wait: "completed")
```

For site-tree placement, set the route, family group, navigation section, and
`nav_order` at create time. The renderer sorts menu entries by `nav_order`
inside the configured navigation arrays, then falls back to array order for
ties or missing values. Primary and utility navigation are sorted separately;
utility groups render after primary groups even when a utility page has a lower
`nav_order`. `page-create` persists `nav_section` on the `[[pages]]` record so
later agents do not have to infer placement only from navigation arrays.
`wiki.list` and `wiki.page.status` both return `nav_section` so callers can
tell whether a page is primary, utility, hidden, or default-positioned without
re-reading `wiki.toml`.

For embedded page files and images, use the page asset API target instead of
guessing source-relative URLs:

```text
wiki.asset.add(page, file, purpose, caption, alt_text)
wiki.page.patch_body(page, find, replace_with_returned_markdown)
wiki.publish(page, wait: "completed")
```

Current V0 already supports talk attachments through `wiki.talk.append`. Page
assets are the next wiki-content surface: they should copy into
`source/<page-slug>.assets/`, publish as route-sibling assets such as
`/topics.assets/diagram.png`, and appear in the content index. Talk attachments
remain workflow/evidence files; page assets are reader content.

For talk and curator work:

```text
wiki.agent.identify(thread_id, roles, capabilities)
wiki.agent.heartbeat(agent_id)
wiki.agent.list(include_stale, include_retired)
wiki.agent.status(agent_id)
wiki.mail.subscribe(agent_id, address, relation, kinds)
wiki.page.watch(page, agent_id, kinds)
wiki.page.assign_role(page, agent_id, role, kinds)
wiki.agent.inbox(agent_id)
wiki.agent.claim(agent_id, message)
wiki.mail.inbox(recipient)
wiki.talk.append(page, kind: "proposal" | "concern" | "question" | "reply", thread_id?, reply_to?, attachments)
wiki.mail.read(message_or_thread)
wiki.mail.claim(message, recipient, agent_id)
wiki.mail.mark(message, state)
wiki.mail.mark_all(message, state)
wiki.notify.poll(agent_id)
wiki.notify.ack(notification)
wiki.curator.apply(decision)
wiki.publish(page, wait: "completed") only when accepted work changed source, tombstones, or wiki.toml
```

Current CLI equivalents:

```bash
onecontext-wiki --root ~/1Context status
onecontext-wiki --root ~/1Context validate
onecontext-wiki --root ~/1Context page-create-all # rare fixture/debug utility; publish also backfills missing configured pages safely
onecontext-wiki --root ~/1Context page-open topics
onecontext-wiki --root ~/1Context page-status topics
onecontext-wiki --root ~/1Context page-create scratch --title "Scratch" --route /scratch --nav-section primary
onecontext-wiki --root ~/1Context page-write-body scratch --body-file body.md
onecontext-wiki --root ~/1Context page-patch-body scratch --find-file find.md --replace-file replace.md
onecontext-wiki --root ~/1Context page-delete scratch --mode tombstone
onecontext-wiki --root ~/1Context page-restore scratch
onecontext-wiki --root ~/1Context publish-status
onecontext-wiki --root ~/1Context publish --trigger agent-edit
onecontext-wiki --root ~/1Context agent-identify --thread-id "$CODEX_THREAD_ID" --capability wiki.mail
onecontext-wiki --root ~/1Context agent-register --thread-id "$CODEX_THREAD_ID" --capability wiki.mail # low-level create only; refuses known thread ids
onecontext-wiki --root ~/1Context whoami --thread-id "$CODEX_THREAD_ID"
onecontext-wiki --root ~/1Context agent-list --include-stale --include-retired
onecontext-wiki --root ~/1Context agent-status "$AGENT_ID"
onecontext-wiki --root ~/1Context list-create --address list://wiki.reviewers --title "Wiki Reviewers" --description "Agents reviewing proposed wiki changes." --owner "$AGENT_ID"
onecontext-wiki --root ~/1Context lists --address list://wiki.reviewers
onecontext-wiki --root ~/1Context mail-subscribe --agent-id "$AGENT_ID" --address list://wiki.reviewers --relation member --kind review
onecontext-wiki --root ~/1Context page-watch topics --agent-id "$AGENT_ID" --kind review
onecontext-wiki --root ~/1Context page-assign-role topics --agent-id "$AGENT_ID" --role curator --kind proposal
onecontext-wiki --root ~/1Context talk-append --page topics --kind proposal --subject "Proposal" --from "$AGENT_ADDRESS" --to-role curator --body "..." --attachment screenshot.png --attachment-caption "Browser proof" --attachment-alt "Screenshot of the rendered route"
onecontext-wiki --root ~/1Context talk-append --page topics --kind reply --subject "Follow-up" --from "$AGENT_ADDRESS" --to-role curator --reply-to "$MESSAGE_ID" --body "..." # resolves the parent message's thread even if the subject changed
onecontext-wiki --root ~/1Context talk-append --page topics --kind reply --subject "Thread follow-up" --from "$AGENT_ADDRESS" --to-role curator --thread-id "$THREAD_ID" --body "..." # explicit existing thread target
onecontext-wiki --root ~/1Context mail-read --message-id "$MESSAGE_ID"
onecontext-wiki --root ~/1Context mail-read --thread-id "$THREAD_ID" # same as wiki-talk-thread
onecontext-wiki --root ~/1Context mail-subscriptions --agent-id "$AGENT_ID"
onecontext-wiki --root ~/1Context mail-subscriptions --address list://wiki.reviewers
onecontext-wiki --root ~/1Context list-status list://wiki.reviewers
onecontext-wiki --root ~/1Context list-members list://wiki.reviewers
onecontext-wiki --root ~/1Context agent-inbox "$AGENT_ID"
onecontext-wiki --root ~/1Context agent-inbox "$AGENT_ID" --include-snoozed
onecontext-wiki --root ~/1Context agent-claim "$AGENT_ID" "$MESSAGE_ID" # preferred after agent-inbox
onecontext-wiki --root ~/1Context mail-claim "$MESSAGE_ID" --recipient role://topics.curator --agent-id "$AGENT_ID"
onecontext-wiki --root ~/1Context mail-claim "$MESSAGE_ID" --recipient "$AGENT_ADDRESS" --agent-id "$AGENT_ID" # works for visible role/list/page deliveries
onecontext-wiki --root ~/1Context mail-mark "$MESSAGE_ID" --recipient role://topics.curator --state snoozed --until "<future RFC3339>"
onecontext-wiki --root ~/1Context mail-mark "$MESSAGE_ID" --recipient "$AGENT_ADDRESS" --state done # resolves through agent-inbox when exact primary mailbox has no row
onecontext-wiki --root ~/1Context mail-mark-all "$MESSAGE_ID" --state done
onecontext-wiki --root ~/1Context notify-poll "$AGENT_ID"
onecontext-wiki --root ~/1Context notify-ack "$NOTIFICATION_ID" --agent-id "$AGENT_ID"
```

Talk attachments stay in the user-owned talk folder as
`attachments/<message-id>/<safe-filename>`, while rendered talk pages link to
the route-local published copy under `/<page-route>/talk/attachments/...`.
Post-render diagnostics resolve relative talk attachment links against the
rendered talk page base href, so a source link like `attachments/evidence.txt`
is checked at `/<page-route>/talk/attachments/evidence.txt`.
Safe filenames collapse punctuation runs, reject path separators, and add
numeric suffixes for duplicate basenames. Use repeated
`--attachment-filename`, `--attachment-caption`, and `--attachment-alt` values
in the same order as repeated `--attachment` values. The CLI rejects dangling
metadata with no matching attachment. Rendered talk HTML and markdown twins
show caption and alt text once; the renderer strips the generated fallback
attachment list from the talk body when structured attachment frontmatter is
present, so agents do not see duplicated links.

The CLI accepts API-shaped command aliases with a `wiki-` prefix. For example,
`wiki-list`, `wiki-page-status`, `wiki-agent-list`, `wiki-agent-status`, and
`wiki-whoami` resolve to the same operations as `list`, `page-status`,
`agent-list`, `agent-status`, and `whoami`. `agent-whoami` and
`wiki-agent-whoami` are accepted aliases for `whoami`. `identify` is accepted
as a short alias for `agent-identify`.

The current Rust core exposes the normal agent actions through
`onecontext-wiki`. The Python `wiki_interface` adapter is a thin subprocess
client over the same JSON surface for page lifecycle, publish status, agent
identify/list/status/inbox, mail claim/mark/mark-all/snooze, page watch/role
assignment, lists, notifications, `wiki.status`, and `wiki.validate`. Use the
CLI JSON surface first for any new Rust command that has not yet been wrapped.
Use direct file edits only when the structured operation cannot express the
change.
Create `list://` routing surfaces before subscribing to them; V0 rejects
phantom list subscriptions with `unknown_list`. When in doubt, call
`list-status` for the current list metadata, active members, mailbox counts,
and recent messages. Missing lists return `status=missing`, `exists=false`,
and `next_action=list_create` so agents can branch without parsing nulls.
If default `list-status` hides archived or future-snoozed mail and no open
work remains, `next_action=include_hidden_mail`; rerun with
`--include-archived` and/or `--include-snoozed` for an audit read.
`list-create --owner` accepts either a concrete mail address or an active
agent id; agent ids are resolved to that agent's primary `agent://...` address
in the saved list metadata.
Roster totals are not the same thing as live coverage:
`member_count` / `mail.watcher_count` are total unique subscribed agents, while
`active_member_count`, `inactive_member_count`, `active_watcher_count`,
`inactive_watcher_count`, and `subscription_liveness_counts` show whether those
agents are active, stale, retired, or unknown. Check active counts before
assuming a page or list has someone currently awake.
On `page-status`, `mail.subscription_liveness_counts` covers all page-related
subscriptions, including explicit page-linked lists; the watcher counts remain
the narrower watcher-only roster.
Use `agent-identify` when a stale session wakes back up. `agent-heartbeat`
extends only active leases, and retired thread/session pointers are not revived;
start a new session instead.
Use the returned `primary_address` or `addresses[]` values for mail delivery.
Use `agent_id` for control commands such as `agent-status`, `agent-inbox`,
`notify-poll`, and `mail-claim`. `agent://agent_codex_...` is rejected because
it is almost always a confused agent id, not a registered mailbox address.
After `agent-inbox`, the lowest-friction claim path is `agent-claim "$AGENT_ID"
"$MESSAGE_ID"`. If you need an explicit mail command, `mail-claim` and
`mail-mark` also accept the agent's returned `primary_address` for visible
role/list/page deliveries. The command resolves through the active agent inbox
and updates the canonical delivery mailbox; the receipt's `recipient` field is
the canonical mailbox that actually changed.
For page-scoped work, prefer `page-status` to discover
`mail.page_mailbox`, `mail.curator_address`, and
`mail.default_watchers_list`. It also reports `mail.associated_lists` for
explicit list objects whose `page_id` matches the page, and counts those list
deliveries in page mail pressure. Then use `page-watch`, `page-assign-role`,
and `talk-append --to-role` before dropping down to raw addresses.
`talk-append --to page://<page-id-or-route>` is accepted as page-recipient
shorthand and is saved/delivered as the canonical `mailbox://page/<page-id>`.
The same page alias is accepted by recipient-oriented mail commands such as
`mail-subscribe`, `mail-subscriptions --address`, `mail-inbox`, `mail-claim`,
and `mail-mark`; their receipts still report the canonical
`mailbox://page/<page-id>` address so subsequent automation has a stable
mailbox key.
`page-watch` subscribes the agent to both the default watchers list and the
page mailbox so direct page talk reaches normal watchers.
The receipt includes `unsubscribe_plan`; use `page-unwatch` for normal cleanup
instead of hand-writing two `mail-unsubscribe` calls.
`mail-subscribe` reports a `backfill` summary for historical messages surfaced
by the new subscription; retrospective notifications are intentionally not
created.
`agent-inbox` returns raw per-delivery `messages` plus grouped `threads`; use
`threads` for the normal agent workbench when one talk entry fans out through a
role, page mailbox, and list. Thread rows include `attachment_count` so agents
can triage attachment-bearing work before expanding raw deliveries.

Use `wiki.list` for compact sitemap rows with canonical `state`, scan-friendly
`flags`, `content_state`, `origin`, `template_state`, `dirty_since_publish`,
`talk_state`, validation counts, template path/hash metadata, and
`next_action`. `origin` is intentionally consumer-facing: packaged pages should
report `runtime_default`, while pages born through `wiki.page.create` report
`created_from_template`; scan `flags.runtime_default` and
`flags.custom_created` when the distinction matters more than the exact origin
string. Generated `[[site_pages]]` also appear here with
`kind="generated_site_page"`, `origin="generated_site_page"`,
`flags.source_backed=false`, and `talk_state="not_applicable"`; they are
publishable navigation/render targets, not editable source pages. Use
`wiki.page.status` for the full page card: template baseline hash, current
source hash, last published hash, source/talk/render freshness, allowed
actions, and whether the page is still template-unedited or has diverged.
Page create, body write, body patch, delete, and restore receipts also return a
fresh `page_status` when the page can be inspected. Create, body write, body
patch, and restore receipts return `hashes`, so a normal agent can chain
guarded body edits with `hashes.source_sha256` instead of reopening the page
between each patch.
For missing configured source rows, `next_action` is `publish`, matching
`wiki.validate` and `wiki.publish.status`; publish can safely backfill those
already-configured pages before rendering. New custom pages still use
`wiki.page.create`.

For a normal memory agent today:

1. Register in the agent directory with the current thread/session pointer.
2. Read `wiki.mail.inbox` and `wiki.list` before walking files.
3. Use `wiki.page.open` to get source/talk/curator/conventions handles and the
   current source hash.
4. Use `wiki.page.write_body` for full body replacement or
   `wiki.page.patch_body` for exact body patches. Pass
   `expected_source_sha256` when preserving an observed edit precondition.
5. Use returned `hashes.source_sha256` for follow-up body patches in the same
   work session.
6. Append talk entries through `wiki.talk.append`; use `wiki.agent.claim`
   after `wiki.agent.inbox`, or `wiki.mail.claim` only when claiming a known
   recipient mailbox.
7. Mark non-claim delivery state with `wiki.mail.mark`. Use `done` to close
   one recipient's delivery while keeping the message visible; use `snoozed
   --until <RFC3339>` to hide it from normal inbox/notification/page-pressure
   views until the due time; use `archived` to hide it from the default inbox.
   Notification polling is also subscription-filtered: expired, cancelled, or
   kind-mismatched live subscriptions do not keep old list/role/page-mailbox
   wakeups visible. If one talk message was fanned out to a
   role, list, and page mailbox and the work is resolved everywhere, use
   `wiki.mail.mark_all` so page open-thread pressure clears without chasing
   each mailbox manually. Terminal mail states do not keep notification
   pressure in `agent-inbox`. Prefer `summary.actionable_count` and
   `mail.open_delivery_count` for remaining open work; prefer
   `summary.claimable_count` when deciding whether this specific agent should
   attempt `wiki.agent.claim`. A shared delivery claimed by another agent
   remains actionable but is not claimable for competitors. `message_count` is
   durable history and can stay non-zero after the work is done. Use
   `summary.pages_with_open_mail_count` for mail triage and
   `summary.pages_requiring_action` for page lifecycle work such as publish or
   link repair. Raw `notify-poll` also filters out wakeups whose delivery is
   already terminal, so mark-all can quiet agents even when no explicit
   notification ack was sent. Notification ack is only transport handling: it
   can remove a wakeup from `notify-poll` while the underlying mail remains
   actionable. Use mail claim/mark calls, not notify ack, to change work state.
7. If the change adds a page, call `wiki.page.create` with explicit placement
   metadata and then write the page body. `wiki.page.create` preflights the
   rendered template frontmatter before touching `wiki.toml`; if it returns
   `invalid_page_template`, fix the template fields first. It refuses
   tombstoned or disabled page ids; use `wiki.page.restore` when the same page
   should intentionally return. Tombstoned routes also remain reserved;
   replacement pages should use a new route unless an explicit restore owns the
   old route.
8. If the change removes a page, call `wiki.page.delete` so deletion is
   tombstone-first and reviewable. Read `link_impact` in the delete receipt:
   it previews source links that will break when the route is removed, before
   you publish and force readers to see the warnings.
9. If a tombstoned page should return, call `wiki.page.restore` and publish.
   Restore removes the tombstone, re-enables the page, and restores navigation
   according to the saved `nav_section`; it does not recreate deleted source.
10. Request `wiki.publish` when source, tombstones, or `wiki.toml` changed and
   you need immediate reader-visible proof. Otherwise, rely on the configured
   automatic publish cadence. Talk/mail/notification-only changes should not
   require a page-content publish.
11. Read operation receipts, publish evidence, and `wiki.page.status` instead
   of assuming publication succeeded.
12. If publish returns `link_diagnostics.status = "warning"`, repair the
    listed source page links and publish again. Broken internal links are
    warnings, not render failures, because the site can still be served while
    agents clean them up.
13. After publish, `wiki.list` and `wiki.page.status` expose the same page-level
    link warnings through `links` and `next_action = "repair_links"` so agents
    do not need to retain the original publish stdout.
14. Reader-visible output also marks broken internal links with
    `.opctx-link-warning` and `.opctx-broken-link`, and writes
    `.1context/link-diagnostics.json` in the rendered site. The route manifest
    includes a compact `link_diagnostics` pointer/summary. This is generated
    evidence, not source mutation.

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

## macOS Host Recipe

On daemon startup:

1. `RuntimePaths.current()` resolves production paths.
2. The host locates bundled RuntimeDefaults, WikiEngine, local-web support, and
   the target Rust wiki core.
3. `WikiRuntimeDefaultsInstaller.installMissingDefaults()` copies missing
   packaged defaults and writes conflict proposals for changed user files.
4. Setup ledger is written to
   `Library/Application Support/1Context/setup/runtime-defaults-install.json`.
5. The daemon queues `wiki.prepare`.
6. The current transitional coordinator renders from actual user data.
7. The coordinator validates and promotes last-good output.

Important startup rule: configured pages become render inputs through the Rust
core page lifecycle. Hosts and agents should call `wiki.page.create` for new
custom pages rather than writing source/talk paths by hand. For packaged or
configured defaults that are already listed in `wiki.toml`, hosts may call
`wiki.publish` directly; publish will safe-backfill missing source/talk files
with the same lifecycle logic before rendering.
Generated `[[site_pages]]` are render inputs owned by the renderer; ordinary
source-backed pages are still born through `wiki.page.create`.
The render coordinator renders; it does not edit `[[pages]]`, register agents,
deliver mail, or enqueue notifications.
Those belong in the portable wiki core API.

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

This creates a fixture runtime, adds a dummy custom page, creates its backing
source/talk files, renders, serves it, then uses Playwright to verify:

- page routes
- talk routes
- markdown twins
- brand menu links
- route-index duplicate pages with relative links, including trailing-slash URLs
- table-of-contents anchors
- Agent view markdown loading, including talk surface links
- missing-route diagnostics
- no local path leakage

The timeout is intentionally larger than a unit test because it is an
exhaustive browser sweep, not a smoke check.

Browser proof hygiene:

- keep stable dogfood/release evidence under `test-results/`
- keep volatile Playwright run output under `.playwright-artifacts/`
- do not point Playwright `outputDir` at `test-results/`; Playwright cleans that
  directory at run start
- the repo root `playwright.config.cjs` exists as a guardrail for ad hoc
  browser runs and defines a named `chromium` project
- install root browser harness dependencies with `npm install`; use
  `npm run test:wiki:browser -- <spec> --project=chromium` for local browser
  proof

## Troubleshooting

### `render-site.mjs` says no source pages

Create or repair the backing source/talk files for the configured page, then
check for:

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

Check whether it is registered and source-backed:

```bash
rg -n 'id = "dummy-custom"|route = "/dummy-custom"' ~/1Context/user-wiki/wiki.toml
test -f ~/1Context/user-wiki/source/families/custom/dummy-custom/source/dummy-custom.md
test -f ~/1Context/user-wiki/source/families/custom/dummy-custom/talk/dummy-custom.talk/_meta.yaml
```

If only `wiki.toml` changed, the page has not become a render input yet. Call
`wiki.page.create` for the page, then confirm with
`1context wiki page-status dummy-custom` or `onecontext-wiki page-status`.

### Render fails

Expected behavior:

- failed staging output is discarded
- the CLI exits nonzero when JSON `status` is `failed`
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
The packaged `RuntimeDefaults/1Context/context-engine/runs` directory should be
empty apart from any intentional placeholder; generated publish receipts belong
to disposable run evidence, not the shipped defaults tree.

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
