# Your Context Talk Conventions Template

This talk folder is the working record for the Your Context article. It hosts
proposals about the operator's durable collaboration context: working style,
preferences, taste, recurring ideas, environment, standing requests, and
AI-specific instructions.

## What Belongs Here

- Proposals to add or revise durable operator context.
- Concerns that a section is stale, overfit, unfair, or misplaced.
- Replies that clarify evidence or argue for a better section.
- Decisions that record what was applied, deferred, rejected, or superseded.

Hourly narrative belongs on For You talk folders. This folder receives distilled
patterns and explicit standing instructions.

## Section Targets

Every proposal should name one target section:

- Working Style
- Coding Style
- Engineering Philosophy
- Preferences
- Taste
- Desires
- Recurring Ideas
- Habits
- Coworkers
- Infra And Tooling
- Standing Requests
- Notes For AI Agents
- Life Story

If no section fits, write a concern proposing a schema change instead of
inventing a section in the article.

## Proposal Filename

```text
YYYY-MM-DDTHH-MMZ.proposal.ycx-{{ short-slug }}.md
```

## Proposal Shape

```yaml
---
kind: proposal
author: "{{ author_id }}"
created: "2026-06-06T02:21:45Z"
target_section: "{{ section_name }}"
talk_for: "mailbox://page/your-context"
state: open
evidence:
  - "{{ evidence_uri }}"
attachments:
  - "{{ attachment_uri }}"
---
```

Body:

```markdown
## [PROPOSAL] {{ short_title }}

Proposed addition or revision:

> {{ proposed_article_text }}

Evidence:

- {{ evidence_summary }}
```

## Decision Shape

Decision entries use `kind: decision`, point `parent:` at the proposal stem, and
state whether the change was applied, refined and applied, deferred, rejected,
or superseded.
