# For You Talk Conventions Template

This talk folder is the working record for a For You article. It is where
hourly entries, synthesis notes, proposals, concerns, replies, decisions, and
closures accumulate before accepted changes appear in the article.

## Folder Shape

```text
{{ page_slug }}.talk/
  _meta.yaml
  _conventions.md
  _curator.md
  {{ timestamp }}.conversation.md
  {{ timestamp }}.proposal.editor-day-{{ date }}.md
  {{ timestamp }}.reply.{{ thread_slug }}.md
  {{ timestamp }}.decision.{{ thread_slug }}.md
  archive/
```

Each contribution is one markdown file. Entries are append-only. Do not rewrite
old bodies to change meaning; add replies, corrections, or closures instead.

## Filename Convention

```text
YYYY-MM-DDTHH-MMZ.<kind>[.<short-slug>].md
```

Common kinds:

- `conversation` - hourly or session-scoped memory entry
- `question` - question that needs an answer before article work proceeds
- `proposal` - proposed article or process change
- `reply` - response to an existing entry
- `concern` - factual, framing, freshness, or sourcing problem
- `synthesis` - cross-entry interpretation
- `decided` - closure entry
- `verify` - citation or evidence request

## Entry Frontmatter

```yaml
---
id: "{{ entry_id }}"
kind: "{{ kind }}"
author: "{{ author_id }}"
created: "{{ created_at }}"
parent: "{{ parent_entry_id_or_stem }}"
talk_for: "page://{{ page_slug }}"
state: "{{ open|accepted|rejected|resolved|withdrawn|superseded|blocked|archived }}"
evidence:
  - "{{ evidence_uri }}"
attachments:
  - "{{ attachment_uri }}"
---
```

## Conversation Entries

Conversation entries should be factual, compact, and useful to downstream
writers. They should say what happened, what changed, what remains uncertain,
and which artifacts or source events support the claims.

## Proposals And Decisions

Proposal entries suggest changes. Decision entries record whether the proposal
was accepted, refined and accepted, deferred, rejected, withdrawn, or superseded.
Accepted changes cross the boundary by editing `user-wiki/source`; previews and
drafts remain review artifacts until then.

## Closure Box

```markdown
<details class="opctx-talk-closure" open>
<summary><strong>Closed - {{ decision_label }} by {{ author_id }}.</strong> {{ verdict }}</summary>

{{ reasoning }}

</details>
```
