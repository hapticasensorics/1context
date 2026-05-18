# Operator-Touched Marker

This policy protects operator hand-edits from later agent overwrites.

## Marker

Place this HTML comment directly above a section heading, subsection, or
paragraph:

```markdown
<!-- operator-touched: {{ touched_date }} -->
### {{ section_title }}

{{ operator_written_text }}
```

The date is the most recent known operator edit date. The marker stays in
place across later agent passes and does not expire automatically.

For paragraph-level edits inside an otherwise agent-authored section, place
the marker directly above the protected paragraph.

## Agent Rules

When an agent sees an `operator-touched` marker:

- Do not rewrite, refine, soften, generalize, move, or delete the marked
  content.
- Do not merge the marked content into nearby generated prose.
- You may append new content after the marked content when new evidence
  warrants.
- You may edit unmarked surrounding content according to the page's normal
  update policy.
- If the marked content appears stale, incomplete, or contradictory, post a
  concern in the page's talk folder instead of changing the marked text.

## Operator Use

When hand-editing a section or paragraph, add or update the marker directly
above the content you want agents to preserve:

```markdown
<!-- operator-touched: {{ touched_date }} -->
```

Use a concrete date when known. If the date is unknown during migration, use
`unknown` and replace it on the next confirmed operator edit.

## Future Automation

Possible future support:

- A pre-commit hook that detects operator edits and suggests a marker.
- An editor wrapper that adds or refreshes the marker when the operator saves a
  protected section.
- A renderer affordance that distinguishes operator-authored text from
  generated text.

For now, the convention is manual and explicit.
