# Task: Post One Concept Candidate

Read the loaded talk-folder input and write one concept-scout entry.

Target file:

```text
{output_path}
```

Required frontmatter for a proposal:

```markdown
---
kind: proposal
author: claude-opus-concept-scout
ts: {date}T23:59:00Z
---
```

Rules:

- Write exactly one markdown file at the target path.
- Prefer `kind: proposal` when there is a real candidate concept.
- Use `kind: question` or `kind: concern` if the day only reveals uncertainty.
- Ground the proposal in specific talk-folder entries.
- Include suggested slug, what the concept is, why it deserves a page or index
  entry, and adjacent concepts if obvious.
- Do not edit topics.md or any concept page.
