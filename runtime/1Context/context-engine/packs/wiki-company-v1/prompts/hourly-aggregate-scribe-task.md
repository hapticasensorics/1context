# Task: Aggregate Scribe Reports Into One Hourly Talk Entry

Write the canonical hourly talk entry for:

```text
date: {date}
hour: {hour}
output_path: {output_path}
scribe_report_count: {scribe_report_count}
```

The scribe reports are loaded above in the prompt. Use them as your source of
truth.

Required frontmatter:

```markdown
---
kind: conversation
author: codex-hourly-aggregate-scribe
ts: {date}T{hour}:00:00Z
---
```

Rules:

- Create exactly the target file unless the scribe reports collectively warrant
  `<no-talk>`.
- Do not edit scribe report files.
- Do not read sibling talk entries or article files.
- Preserve concrete evidence from reports: timestamps, paths, commands,
  session ids, errors, decisions, and exact operator phrases.
- Include a final "What I'd flag" section for unresolved issues, cross-report
  uncertainty, or work that clearly continued outside the hour.
