# Task: Write One Hourly Talk Entry

Write one talk entry for the inherited hour.

Target file:

```text
{output_path}
```

Required frontmatter:

```markdown
---
kind: conversation
author: claude-opus-hourly-scribe
ts: {date}T{hour}:00:00Z
---
```

Rules:

- Create the target file if and only if the inherited hour contains meaningful
  activity worth remembering.
- If the inherited hour is empty, write no talk file and return only
  `<no-talk>` in your final response.
- Do not read sibling talk entries.
- Do not edit any file except the target file.
- Ground claims in the inherited lived experience. Preserve timestamps, session
  ids, file paths, commands, errors, and decisions when they matter.
- For a meaty or multi-stream hour, prefer the e08 talk-page shape: opening
  thesis, short specific sections for major streams, and a final "What I'd
  flag" section for unresolved issues, risks, or operator-working-style signals.
- If the inherited hour references context that clearly began outside the
  window and you cannot write safely, write a NEEDS wider-window request instead
  of guessing.

The target workspace is temporary. The important artifact is the markdown file.
