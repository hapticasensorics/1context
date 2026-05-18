# Topics Talk Conventions Template

This talk folder is the working record for the Topics index. It hosts proposals
about named subjects, concept page candidates, recategorization, merges, splits,
and moves.

## What Belongs Here

- Add a topic to the index.
- Recategorize an existing topic.
- Flag a duplicate, stale, missing, or badly named topic.
- Propose a merge, split, or move.
- Mark a topic as pending concept-page work.

Concept page bodies belong on their own pages. This folder maintains the index
and taxonomy.

## Section Targets

- Engineering
- Infrastructure
- Process
- Tools
- Domain
- People
- Organizations

## Proposal Filename

```text
YYYY-MM-DDTHH-MMZ.proposal.topic-{{ short-slug }}.md
```

For restructuring:

```text
YYYY-MM-DDTHH-MMZ.merge.topic-{{ short-slug }}.md
YYYY-MM-DDTHH-MMZ.split.topic-{{ short-slug }}.md
YYYY-MM-DDTHH-MMZ.move.topic-{{ short-slug }}.md
```

## Proposal Shape

```yaml
---
kind: proposal
author: "{{ author_id }}"
created: "{{ created_at }}"
target_section: "{{ section_name }}"
topic_slug: "{{ topic_slug }}"
talk_for: "page://topics"
status: proposed
evidence:
  - "{{ evidence_uri }}"
---
```

Body:

```markdown
## [PROPOSAL] {{ topic_name }}

Proposed index entry:

> {{ topic_name }} - {{ one_line_description }}

Evidence:

- {{ evidence_summary }}
```

## Decision Shape

Decision entries use `kind: decided`, point `parent:` at the proposal stem, and
state whether the index changed. If the concept page does not exist yet, note
that it remains pending librarian work.
