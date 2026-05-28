# 1Context Wiki Publishing System Runbook

- Status: operating guide for the current wiki API
- Last updated: 2026-05-20

Use this with [Wiki Publishing System API](wiki-publishing-system-api.md) and
[Wiki System Architecture](wiki-system-architecture.md). This file shows the
practical loops.

## Fast Proof Loop

Rust core and daemon:

```bash
cargo check -p onecontext-wiki-core -p onecontext-wiki-daemon --tests
```

Swift app bridge:

```bash
swift test --package-path macos --filter OneContextWikiRuntimeTests
```

Python adapter:

```bash
cd memory-core
uv run --with pytest pytest tests/test_wiki_core_client.py
```

Current contract coverage should live in Rust, Swift, or checked-in Playwright
tests. The old wiki and mail dogfood runners were retired during cleanup.

## Create A Runtime Fixture

Use `runtime-test/` for private local scenarios. Do not wipe the whole folder;
create a named fixture.

```bash
./scripts/init-dev-wiki-runtime.sh runtime-test/my-scenario
```

The fixture mirrors the installed user-data shape:

```text
runtime-test/my-scenario/1Context/user-wiki/
runtime-test/my-scenario/1Context/context-engine/
runtime-test/my-scenario/Library/Application Support/1Context/
runtime-test/my-scenario/Library/Logs/1Context/
runtime-test/my-scenario/Library/Caches/1Context/
```

## Inspect The Wiki

Low-level Rust CLI:

```bash
onecontext-wiki --root <runtime-home>/1Context ensure
onecontext-wiki --root <runtime-home>/1Context status
onecontext-wiki --root <runtime-home>/1Context list
onecontext-wiki --root <runtime-home>/1Context validate
onecontext-wiki --root <runtime-home>/1Context publish-status
```

Installed app CLI:

```bash
1context wiki list
1context wiki validate
1context wiki publish-status
```

Use `list` for whole-site shape and `page-status` for one page.

```bash
onecontext-wiki --root <runtime-home>/1Context page-status topics
onecontext-wiki --root <runtime-home>/1Context page-open topics
```

`page-open` returns the editable body, source hash, route metadata, talk
handles, and next actions.

## Create A Page

Prefer the API over hand-building folders.

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

Verify:

```bash
onecontext-wiki --root <runtime-home>/1Context page-status dummy-custom
test -f <runtime-home>/1Context/user-wiki/source/families/custom/dummy-custom/source/dummy-custom.md
test -f <runtime-home>/1Context/user-wiki/source/families/custom/dummy-custom/talk/dummy-custom.talk/_meta.yaml
```

Publish when reader content should change:

```bash
onecontext-wiki --root <runtime-home>/1Context publish --trigger agent
```

## Edit A Page

Open first and use the returned source hash as the write precondition.

```bash
onecontext-wiki --root <runtime-home>/1Context page-open dummy-custom
```

Small patch:

```bash
onecontext-wiki --root <runtime-home>/1Context page-patch-body dummy-custom \
  --find "Old sentence." \
  --replace "New sentence." \
  --expected-source-sha256 <sha>
```

Prepared body file:

```bash
onecontext-wiki --root <runtime-home>/1Context page-write-body dummy-custom \
  --body-file /tmp/dummy-custom.md \
  --expected-source-sha256 <sha>
```

If the source changed, the command fails with a stale-source error. Reopen the
page and retry against the current hash.

## Add An Asset Or Image

```bash
onecontext-wiki --root <runtime-home>/1Context asset-add dummy-custom \
  --file /tmp/screenshot.png \
  --purpose evidence \
  --caption "Browser proof" \
  --alt "Screenshot of the dummy custom page"
```

Insert the returned markdown snippet into the page body, then publish.

```bash
onecontext-wiki --root <runtime-home>/1Context asset-list dummy-custom
onecontext-wiki --root <runtime-home>/1Context publish --trigger agent
```

## Append Talk

Talk is durable page discussion. By default, `to` and `cc` are metadata labels
and do not create inbox rows or notifications. Use explicit mail delivery only
when the talk entry should become claimable agent work.

```bash
onecontext-wiki --root <runtime-home>/1Context talk-append dummy-custom \
  --kind note \
  --subject "Dogfood note" \
  --from agent://dogfood \
  --to role://dummy-custom.curator \
  --body "Created and checked this page during dogfood."
```

```bash
onecontext-wiki --root <runtime-home>/1Context talk-append dummy-custom \
  --kind proposal \
  --subject "Review this page" \
  --from agent://dogfood \
  --to role://dummy-custom.curator \
  --delivery-mode mail \
  --body "Please review this page when you next check your inbox."
```

Reply to a known message:

```bash
onecontext-wiki --root <runtime-home>/1Context talk-append dummy-custom \
  --kind reply \
  --subject "Follow-up" \
  --from agent://dogfood \
  --reply-to <message-id> \
  --body "Confirmed."
```

Use `--thread-id` only when intentionally targeting an existing thread without
a specific parent message.

## Delete And Restore

Deletion is a tombstone, not raw file removal.

```bash
onecontext-wiki --root <runtime-home>/1Context page-delete dummy-custom --mode tombstone
onecontext-wiki --root <runtime-home>/1Context publish --trigger agent
```

The route should disappear from the published site while the tombstone reserves
the id/route. To restore:

```bash
onecontext-wiki --root <runtime-home>/1Context page-restore dummy-custom
onecontext-wiki --root <runtime-home>/1Context publish --trigger agent
```

## Browser Check

Serve the published mirror or use the app's Local Web URL.

```bash
1context wiki local-url
```

Then verify:

- the route renders
- navigation links work
- markdown twins and assets are present
- tombstoned routes return missing/not-found behavior
- generated routes do not redirect to unrelated pages

Use the in-app browser or Playwright when a route behavior changes.

## RuntimeDefaults Upgrade Check

RuntimeDefaults are backfill material:

1. Build app defaults from `runtime/1Context`.
2. Install missing defaults into the user tree only when files are absent.
3. Preserve existing user files.
4. Write conflict/proposal evidence for changed packaged defaults.
5. Render from actual user data.

The user tree is always the live truth.

## Mail Boundary

Do not use the former scattered mail prototype surface. Current agent mail goes
through the Rust-backed Agent Mail Protocol: identity, inbox, claim, mark,
notification poll, and notification ack live in `toolset-mail`; subscriptions,
page watches, explicit role assignment, and governance remain later layers.
