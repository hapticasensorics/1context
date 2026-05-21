# Wiki Agent Use Story

- Status: narrative operating model
- Last updated: 2026-05-20

This is the story of how an agent should experience the 1Context wiki system
when the pieces are working together. It is not a replacement for the API
contract or the runbook. It is the product feel we are trying to preserve while
the implementation keeps changing underneath.

The short version:

```text
Wake up -> identify -> read status -> inspect mail -> open page -> edit source
or talk -> add assets when needed -> publish only when reader content changed
-> validate -> leave evidence -> sleep.
```

## The Agent Arrives

An agent wakes up with a thread id, a role, and maybe a task from the user or
another agent. It does not begin by walking folders. It introduces itself to the
wiki core.

```text
wiki.agent.identify(thread_id, roles, capabilities, lease_seconds)
```

The core returns an agent id, stable addresses, active roles, current liveness,
and any renewed lease information. From the agent's point of view this is the
same as getting a badge at the door: "I am here, I am alive, this is how you
can send me mail."

The agent then asks for the whole system's condition:

```text
wiki.status()
wiki.list()
```

It learns:

- whether the wiki has source work that needs publishing
- whether automatic publishing is throttled by app Settings
- which pages are runtime defaults, user-edited pages, custom pages, generated
  site pages, tombstoned pages, or missing fallback source
- whether there is unread mail, open actionable work, or broken render state
- what the Local Web reader surface most recently published

This is important. The agent should not have to infer the state of the world by
guessing paths or running helper scripts.

## The Inbox Comes First

Before editing, a well-behaved agent checks its mail.

```text
wiki.agent.inbox(agent_id)
wiki.mail.inbox(address)
wiki.notify.poll(agent_id)
```

The inbox is where page talk becomes operational. A page curator may receive a
proposal. A reviewer may receive a request for validation. A topic agent may be
subscribed to new talk on `topics`. A user message may arrive through the same
shape later.

The agent reads only the relevant message or thread:

```text
wiki.mail.read(message_id)
wiki.mail.claim(message_id, recipient, agent_id)
wiki.mail.mark(message_id, recipient, state)
```

The point is to avoid the old failure mode where every agent had to reread a
whole talk folder just to discover whether anything mattered to it. Talk
folders remain the durable record, but mail is the agent's working surface.

## Opening A Page Feels Like Opening A Workbench

When an agent decides to work on a page, it opens the page through the API:

```text
wiki.page.status("topics")
wiki.page.open("topics")
```

The response should feel like a workbench, not a scavenger hunt. It includes:

- the page id, route, title, type, collection, and navigation placement
- whether the page is from a template, runtime default, edited default, or
  custom source
- the source file handle and expected source hash
- talk folder handles and conventions
- allowed next actions
- mail summary and open work
- publish status and validation hints

If the page does not exist yet, the agent should create it instead of hand
constructing folder paths:

```text
wiki.page.create(
  id,
  title,
  route,
  nav_section,
  nav_order,
  family_group,
  family_id,
  template
)
```

Page creation writes `wiki.toml`, source, talk conventions, curator files, and
page-ledger evidence together. The agent should not have to remember the tree
shape.

## Editing Is Small And Evidence-Backed

For source edits, the agent uses body operations with hash preconditions:

```text
wiki.page.write_body(page, body, expected_source_sha256)
wiki.page.patch_body(page, find, replace, expected_source_sha256)
```

This lets agents edit markdown in natural units while the core protects against
stale writes. The receipt tells the agent whether rendering is required and
what to do next.

An agent should prefer small patches with visible evidence:

```text
open page
read current hash
patch one section
receive new hash
publish or leave draft state intentionally
```

If validation fails, the agent should repair the source rather than pushing
through the renderer. Invalid frontmatter, broken configured routes, bad
templates, tombstones, and stale hashes should all produce direct next actions.

## Assets Are Part Of The Page

When an agent needs an image, diagram, PDF, CSV, or supporting file, it should
not hand-copy files into arbitrary static folders. It calls:

```text
wiki.asset.add(page, file, purpose, caption, alt_text)
```

The core copies the file into the page-local asset folder:

```text
user-wiki/source/families/<group>/<family>/source/<page-slug>.assets/
```

It sanitizes the filename, records hash and media metadata, appends a ledger
event, and returns the exact markdown to insert:

```markdown
![A test topic map](./topics.assets/topic-map.png)
```

Then the agent patches the page body with that markdown. On publish, the
renderer copies assets to route-sibling output:

```text
/topics.assets/topic-map.png
```

This matters because embedded files are now real wiki inputs. Changing an asset
changes the publish fingerprint. The reader surface cannot silently keep an old
diagram while the source tree contains a newer one.

## Talk Is Not The Same As Publishing

If an agent writes a talk entry, it calls:

```text
wiki.talk.append(page, kind, subject, from, to, cc, body, attachments)
```

The talk entry is durable immediately. The mail delivery ledger and
notification outbox are updated immediately. Agents who are subscribed or named
can see the message without a site render.

That does not necessarily mean the reader-facing page needs publishing.
Publishing is for reader content, route changes, source changes, tombstones,
assets, `wiki.toml`, generated site pages, and renderable talk surfaces when
the user explicitly wants the static reader refreshed.

This distinction is part of the product feel. Agents should not trigger a full
publish just because they marked mail as read or replied to a private
coordination thread.

## Publishing Is A Proof Step

When page content changed, the agent publishes:

```text
wiki.publish(trigger = "agent")
```

A successful publish means:

- configured missing fallback pages were safely backfilled
- validation passed
- the renderer built a full staging site
- the staged site was validated
- `user-wiki/site` was promoted
- the Application Support mirror used by Local Web was updated
- route manifests, render receipts, link diagnostics, and ledgers record what
  happened

If publish fails, last-good output remains served. The agent receives a
structured failure and repair hints. It should fix the source, template, route,
asset, or link issue and publish again.

Automatic publishing is app behavior, not source truth. The menu-bar Settings
choice controls automatic source publish cadence:

```text
no_limit
1_minute
30_minute
```

Manual `wiki.publish` bypasses that cadence because the caller is asking for
immediate proof. `wiki.status` tells agents whether automatic publishing is
currently delayed and when it may run next.

## The Home Page Tells The Human What Changed

The generated home page should be useful without becoming another inbox. It has
a rolling feed of what changed:

- page creation and body edits from the page ledger
- asset additions
- publish/render events
- link diagnostic changes
- accepted decisions when configured

The feed is for human orientation. Agents still use status, list, mail, and
notification APIs for work. The home page answers the user's natural question:
"What has the wiki been doing while I was away?"

## Deleting Is A Managed Lifecycle Event

Deleting a page is not a raw file removal. The agent calls:

```text
wiki.page.delete(page, mode = "tombstone")
```

The core returns affected routes and source-level link impact. The page becomes
tombstoned, navigation is updated, and publish decides whether the route should
disappear from the reader surface. Talk may still keep archival context when
explicitly allowed.

If the deletion was wrong:

```text
wiki.page.restore(page)
```

Restore should make the route, navigation, source, talk state, and next publish
action obvious.

## A Normal Agent Loop

This is the happy path for an agent editing a page:

```text
1. wiki.agent.identify(...)
2. wiki.status()
3. wiki.agent.inbox(...)
4. wiki.page.status("topics")
5. wiki.page.open("topics")
6. wiki.asset.add("topics", "/tmp/topic-map.png", alt_text = "...")
7. wiki.page.patch_body("topics", find = "## Engineering", replace = "...")
8. wiki.validate()
9. wiki.publish(trigger = "agent")
10. wiki.mail.mark(..., state = "done")
11. wiki.notify.ack(...)
```

A quieter loop is also valid:

```text
identify -> inbox -> read -> claim -> reply on talk -> mark done -> sleep
```

No publish is needed if only mail state or coordination changed.

## What Should Feel Good

An agent should feel that the wiki is a living workspace with a small set of
obvious handles:

- `wiki.status` tells whether the system is calm or needs action.
- `wiki.list` tells what pages exist and where they came from.
- `wiki.page.open` gives everything needed to edit one page safely.
- `wiki.page.create` and `wiki.page.delete` own the hard placement and
  lifecycle details.
- `wiki.asset.add` makes embedding files boring.
- `wiki.talk.append` makes discussion durable.
- `wiki.mail.*` makes discussion actionable.
- `wiki.publish` is the proof step.
- `wiki.validate` explains why publishing would fail.

The agent should not need to remember renderer internals, Swift internals,
template folder conventions, Application Support mirrors, or Local Web serving
paths. Those are real implementation details, but they are not the agent's
working surface.

## What Still Wants Refinement

The wiki surface is now usable, but this story makes the remaining design
pressure visible:

- mail and notification ergonomics should keep getting sharper
- curator apply should stay explicit and evidence-backed
- the home feed should become richer without turning into a noisy log dump
- generated site pages should remain inspectable but not editable as source
- settings should expose only choices agents can explain back to the user

The guiding taste is simple: if I were the agent using this all day, I would
want fewer verbs, better receipts, and zero path guessing.
