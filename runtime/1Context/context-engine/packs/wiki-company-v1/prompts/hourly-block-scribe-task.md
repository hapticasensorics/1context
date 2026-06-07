# Task: Write Hourly Entries For One Fixed Block

You are writing the fixed UTC block:

```text
date: {date}
block_start: {block_start}
block_end: {block_end}
hours: {hours}
talk_folder: {talk_folder}
manifest_path: {manifest_path}
```

For each listed hour, choose exactly one status:

- `written`: write the hourly talk entry file.
- `no-talk`: no talk entry is warranted.
- `needs-retry`: the hour should be re-run as a single-hour Opus job.

Hourly talk entry path format:

```text
{talk_folder}/{date}THH-00Z.conversation.md
```

Required frontmatter for each written entry:

```markdown
---
kind: conversation
author: claude-opus-hourly-block-scribe
ts: {date}THH:00:00Z
---
```

Also write the block result manifest at:

```text
{manifest_path}
```

Manifest schema:

```json
{{
  "date": "{date}",
  "block_start": "{block_start}",
  "block_end": "{block_end}",
  "hours": [
    {{
      "hour": "00",
      "status": "written",
      "path": "{talk_folder}/{date}T00-00Z.conversation.md",
      "reason": "short reason"
    }},
    {{
      "hour": "01",
      "status": "no-talk",
      "reason": "checked; no journal-shaped memory"
    }},
    {{
      "hour": "02",
      "status": "needs-retry",
      "reason": "too dense or ambiguous for block pass"
    }}
  ]
}}
```

Rules:

- Write exactly one result object for every listed hour.
- Do not include unlisted hours.
- Do not edit existing talk entries.
- Do not write article prose.
- Do not write filler. Silence is part of the system.
- For `no-talk`, do not create a `.conversation.md` file.
- For `needs-retry`, do not create a `.conversation.md` file.
