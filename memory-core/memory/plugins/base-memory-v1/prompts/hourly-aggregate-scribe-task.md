# Task: Aggregate Shards Into One Hourly Talk Entry

Write the canonical hourly talk entry for:

```text
date: {date}
hour: {hour}
output_path: {output_path}
shard_count: {shard_count}
```

The shard witness notes are loaded above in the prompt. Use them as your source
of truth.

Required frontmatter:

```markdown
---
kind: conversation
author: claude-opus-hourly-aggregate-scribe
ts: {date}T{hour}:00:00Z
---
```

Rules:

- Create exactly the target file unless the shards collectively warrant
  `<no-talk>`.
- Do not edit shard files.
- Do not read sibling talk entries or article files.
- Preserve concrete evidence from shards: timestamps, paths, commands,
  session ids, errors, decisions, and exact operator phrases.
- Include a final "What I'd flag" section for unresolved issues, cross-shard
  uncertainty, or work that clearly continued outside the hour.
