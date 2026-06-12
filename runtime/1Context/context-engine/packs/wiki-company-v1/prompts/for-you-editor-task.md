# Task: For You Editor Review

This turn is a For You page edit. It runs after the historian/questioner has
read the scribe output and the hourly answerers have replied or recorded typed
skips. Read the current For You page, loaded scribe artifacts, historian
question artifacts, hourly answer replies, librarian artifacts, and all routed
proposals. Decide what is editorially approved, what needs refinement, what
should be deferred, and what should be rejected.

You are not merely writing the first draft. You are doing the editor desk pass
for the For You page before the curator rewrites/applies material in the page's
final style.

Target file:

```text
{output_path}
```

Required frontmatter:

```markdown
---
kind: editorial_review
author: codex-wiki-editor
ts: {date}T23:59:00Z
target-page: for-you
target-section: daily-memory
target-date: {date}
---
```

Rules:

- Write exactly one markdown file at the target path.
- Do not edit hourly conversation files.
- Do not edit an article body.
- Read the current For You page before approving or rejecting proposals.
- Review every loaded proposal or routed artifact that targets For You.
- Base claims on the scribe, historian-question, hourly-answerer, editor, and
  librarian artifacts already loaded into context.
- Decide proposal state: **approved**, **refine**, **defer**, or **reject**.
- For approved/refined material, write the exact prose the curator should
  consider applying.
- For rejected/deferred material, state the reason clearly enough that a future
  editor or curator understands what would unlock it.
- Write second-person For You prose for page-body material, not a scratchpad
  synthesis.
- Bracket recurring concepts with `[[Subject]]` when the day makes them feel
  durable enough to notice.
- After the prose, include a short `link_and_page_intent` note naming any
  proposed new pages, merges, removals, or uncertain links.
- Include a `removals_or_replacements` section for stale For You claims that
  should disappear or be rewritten.
- Include open threads or unresolved risks when they matter.
- If there is no meaningful source material and no current-page cleanup to
  recommend, write no file and return `<no-editorial-change>`.

Output shape:

```markdown
---
kind: editorial_review
author: codex-wiki-editor
ts: {date}T23:59:00Z
target-page: for-you
target-section: daily-memory
target-date: {date}
---

## Editorial Decisions

- approved: <proposal/artifact> - <reason>
- refine: <proposal/artifact> - <change needed>
- defer: <proposal/artifact> - <missing evidence or timing reason>
- reject: <proposal/artifact> - <reason>

## Approved For You Prose

<second-person page-body prose for the curator to apply or rewrite>

## Removals Or Replacements

- <stale claim or link> -> <remove | replace with current wording> - <reason>

## Link And Page Intent

- [[Subject]] - <create | keep | merge | remove | unsure> - <reason>

## Proposed Wiki Talk

<short note suitable for the For You talk page / Agent Mail>

## Next Agent Requests

- <request for curator, librarian, biographer, hourly answerer, or scribe>
```
