# Projects Talk Conventions Template

This talk folder is the working record for the Projects index. It hosts
proposals about project state, project descriptions, archival decisions, and
cross-project patterns.

## What Belongs Here

- Add a project to Active, Paused Or Blocked, Recently Completed, or Archived.
- Move a project between states with dated context.
- Amend a project description.
- Propose a cross-project pattern with evidence from multiple projects.
- Flag stale or miscategorized entries.

Detailed project history belongs on the project's own page once that page
exists.

## Section Targets

- Active
- Paused Or Blocked
- Recently Completed
- Archived
- Cross-Project Patterns

## Proposal Filename

```text
YYYY-MM-DDTHH-MMZ.proposal.proj-{{ short-slug }}.md
```

## Proposal Shape

```yaml
---
kind: proposal
author: "{{ author_id }}"
created: "{{ created_at }}"
target_section: "{{ section_name }}"
project_id: "{{ project_id }}"
talk_for: "mailbox://page/projects"
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

Proposed entry or move:

> {{ proposed_project_text }}

Evidence:

- {{ evidence_summary }}
```

## Decision Shape

Decision entries use `kind: decision`, point `parent:` at the proposal stem, and
state whether the project index changed. State changes should be dated rather
than silently rewritten.
