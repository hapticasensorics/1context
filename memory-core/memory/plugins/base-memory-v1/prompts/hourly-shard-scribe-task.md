# Task: Write One Hourly Shard Note

Write one shard witness note for this oversized hour.

```text
date: {date}
hour: {hour}
shard_id: {shard_id}
shard_label: {shard_label}
output_path: {output_path}
```

Required frontmatter:

```markdown
---
kind: synthesis
author: claude-opus-hourly-shard-scribe
ts: {date}T{hour}:00:00Z
shard-id: {shard_id}
shard-label: {shard_label}
---
```

Rules:

- Create exactly the target file.
- Do not write the final hourly conversation entry.
- Do not read sibling shard files, talk entries, or article files.
- Ground the shard note only in the inherited shard-lived experience.
- Prefer dense evidence over polished prose.
- Include a final `## Aggregator cautions` section when there is uncertainty,
  missing surrounding context, or a cross-shard dependency.
