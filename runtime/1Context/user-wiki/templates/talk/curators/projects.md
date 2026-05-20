# Projects Curator - Job Prompt Template

The system prompt is `context-engine/prompts/e08-for-you/agent-profile.md`.
Your job is to read proposals on a Projects talk folder and apply accepted
changes to the Projects index article.

## Inputs

- Article: `{{ article_path }}`
- Talk folder: `{{ talk_folder }}`
- Proposal glob: `{{ talk_folder }}/*.proposal.proj-*.md`
- Existing decision glob: `{{ talk_folder }}/*.decision.*.md`

## Scope

You may read the article, the talk folder, prior decisions, and evidence files
explicitly cited by proposals when verification is needed.

Do not create detailed project pages. This role maintains only the index.

## Processing Order

Process proposals oldest first by filename. Project state changes are
time-ordered; do not flatten them into one rewrite.

For each undecided proposal:

1. Read the proposed project entry, move, or cross-project pattern.
2. Check evidence and target section.
3. Decide: apply, refine and apply, defer, reject, or supersede.
4. Edit the Projects article only for accepted changes.
5. Add a decision entry to the talk folder.

## Article Sections

The default Projects schema is:

- Active
- Paused Or Blocked
- Recently Completed
- Archived
- Cross-Project Patterns

Replace an `<!-- empty: ... -->` placeholder only when a section receives its
first accepted entry.

Project entries should be brief: name, link if a page exists, one-line
description, current state, and dated state context when available.

Cross-project patterns need evidence from more than one project unless the
operator explicitly says the pattern should be preserved.

## Decision Entry Shape

Create a new markdown file:

```text
{{ now_utc_slug }}.decision.{{ proposal_slug }}.md
```

with frontmatter:

```yaml
---
kind: decision
author: "{{ agent_id }}"
created: "{{ now_utc }}"
parent: "{{ proposal_stem }}"
decision: "{{ apply|refine_apply|defer|reject|supersede }}"
state: resolved
---
```

## Output Summary

When finished, report edited sections, decision files created, and any proposals
left open.
