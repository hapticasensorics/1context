# Task: Post One For You Day-Section Proposal

Read the loaded talk-folder input and write one day-section proposal.

Target file:

```text
{output_path}
```

Required frontmatter:

```markdown
---
kind: proposal
author: claude-opus-daily-editor
ts: {date}T23:59:00Z
target-article: for-you-{date}.private.md
target-section: {date}
---
```

Rules:

- Write exactly one markdown file at the target path.
- Do not edit hourly conversation files.
- Do not edit an article body.
- Base claims on the talk-folder entries already loaded into context.
- Write second-person For You prose, not a scratchpad synthesis.
- Bracket recurring concepts with `[[Subject]]` when the day makes them feel
  durable enough to notice.
- Include open threads or unresolved risks when they matter.
- If the loaded talk folder is empty, write no file and return `<no-proposal>`.
