# Wiki Agent Use Story

- Status: narrative model for the current wiki workbench
- Last updated: 2026-05-24

This is how the wiki should feel to an agent using it for real work.

```text
inspect -> open page -> edit or create -> attach evidence -> talk when useful
-> publish only when reader content changed -> verify -> leave a receipt
```

The wiki editing loop is not an inbox system. Agent mail is now a separate
protocol and toolset; page talk remains durable collaboration context, and
explicit mail delivery turns selected talk entries into claimable work.

## The Agent Starts With State

The agent begins by asking the core what exists:

```text
wiki.list()
wiki.publish.status()
```

It should see configured pages, custom pages, generated reader routes, missing
source, tombstones, template-derived pages, edited pages, validation warnings,
and publish needs. It should not have to guess from folder names.

For one page:

```text
wiki.page.status("topics")
wiki.page.open("topics")
```

The response should feel like opening a workbench:

- page id, title, route, slug, type, collection, and navigation placement
- whether this is a template-derived page, edited page, custom page, generated
  route, missing source, or tombstone
- source body and `source_sha256`
- source, talk, curator, conventions, asset, and published handles
- validation and next action

## Creating A Page Is One Operation

If a page does not exist, the agent does not assemble paths by hand. It calls:

```text
wiki.page.create(...)
```

The call owns the sitemap entry, source file, talk folder, conventions,
curator prompt, and page-ledger evidence. The agent may choose the route,
family group, family id, nav section, nav order, page type, and template.

This is the right level of control: configurable, but not fussy.

## Editing Is Hash-Backed

For page edits, the agent opens the page, reads the current hash, and writes
with a precondition:

```text
wiki.page.patch_body(page, find, replace, expected_source_sha256)
wiki.page.write_body(page, body, expected_source_sha256)
```

If another actor changed the file, the write fails. The agent reopens the page
and tries again with the current source.

Receipts should tell the agent:

- the new source hash
- whether publish is required
- what validation changed
- what to do next

## Assets Are Page Inputs

Images, screenshots, PDFs, logs, CSVs, and other evidence should be added
through:

```text
wiki.asset.add(page, file, purpose, caption, alt_text)
```

The core copies the file into the page-local asset folder and returns the
markdown snippet to insert into the page. The agent then patches the body and
publishes.

This keeps rendered assets tied to source truth and makes embedded files easy
to audit. After publish, images, files, external links, internal links,
Wikipedia-style footnote citations, and fenced code blocks are listed in
`.1context/reference-index.json` with stable `user-wiki://` citation URIs so an
agent can cite resources without path-walking.

Short code examples should stay as normal fenced Markdown blocks. Code files
that should be downloaded, hashed, or cited as files should be attached through
`wiki.asset.add` and linked from the body.

## Talk Is For Durable Context

When an agent wants to leave a note, review, question, decision, or dogfood
evidence without changing reader content, it uses:

```text
wiki.talk.append(page, kind, subject, from, to, cc, body, attachments)
```

In current V0, `to` and `cc` are metadata labels unless the caller explicitly
sets `delivery_mode = "mail"`. Delivery, notification, and claim state belong
to `toolset-mail`, not to implicit wiki page editing.

Talk append should normally leave publish status unchanged. Publishing is for
reader content, route changes, assets, tombstones, templates, and site config.

## Publishing Is Proof

When source truth changes, the agent publishes:

```text
wiki.publish(trigger = "agent")
```

A successful publish means:

- the inventory validated
- missing configured source pages were safely backfilled
- the static renderer completed
- the staged site validated
- `user-wiki/site` was promoted
- the Application Support mirror used by Local Web was updated
- route manifests and receipts explain what happened

If publish fails, the last-good site remains served. The agent fixes the
source, config, template, asset, route, or link issue and publishes again.

## Deleting Is Managed

Deleting a page is:

```text
wiki.page.delete(page, mode = "tombstone")
```

The route disappears after publish, but the tombstone preserves intent and
prevents accidental recreation. Restoring is:

```text
wiki.page.restore(page)
```

This gives agents a full lifecycle without raw file surgery.

## What Good Feels Like

The wiki workbench is good when an agent can say:

- "show me the site"
- "open this page"
- "make this small edit"
- "add this image"
- "leave a talk note"
- "publish and prove it"
- "delete and prove it disappeared"

without learning the folder layout or waking up an unrelated mail subsystem.
