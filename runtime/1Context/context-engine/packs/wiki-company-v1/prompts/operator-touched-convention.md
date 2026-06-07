# Operator-touched marker — protecting hand-edits from agent overwrites

This document defines the **operator-touched** marker: a
convention by which the operator signals to all agents that a
section of a wiki page has been hand-edited and should be
treated as authoritative — agents may APPEND content beneath
it, but must NOT modify or rewrite it.

## Why this exists

In multi-week operation, the operator hand-edits wiki pages
regularly: typo fixes, voice tightening, adding context the
agents missed, correcting mis-framings. Without an explicit
marker, the next curator/librarian/biographer pass might
overwrite the hand-edit on the assumption that the agent's
output is more recent. **Lost operator edits are the worst
class of failure** — they're silent, they erase deliberate
human authorship, and the operator only notices long after
the loss.

Wikipedia analog: `{{don't edit this comment}}` and the
related convention of treating final-state user-talk-page
contributions as off-limits to bot edits.

## The marker

Place the marker as an HTML comment on the line **immediately
above** the section heading:

```markdown
<!-- operator-touched: 2026-04-29 -->
### Working style — Scope-finding through conversation

Paul uses conversation as a scope-finding tool...
```

The date is the date of the most recent operator hand-edit.
The marker stays in place across subsequent agent passes;
it ages but doesn't expire automatically.

For paragraph-level edits within a section that's mostly
agent-authored, mark the paragraph itself:

```markdown
Some agent-authored prose. Some more agent-authored prose.

<!-- operator-touched: 2026-04-29 -->
A paragraph the operator hand-wrote or substantively rewrote.
The agent must not modify this paragraph but may add new
paragraphs above or below it.

More agent-authored prose.
```

## What agents do with it

All article-mutating agents (your-context-curator,
for-you-curator, librarian-on-expand, biographer) read these
markers before modifying any section.

**For sections marked operator-touched:**

- **DO NOT** rewrite, refine, soften, generalize, or
  consolidate the prose.
- **DO NOT** delete or move paragraphs within the section.
- **MAY** append new paragraphs at the END of the section
  (after the operator-touched content) when new evidence
  warrants. The new paragraphs should not be marked
  operator-touched unless the operator subsequently edits
  them.
- **MAY** post a `[CONCERN]` on the page's talk folder if
  the agent believes the operator-touched content has become
  stale or contradicts new evidence. The agent does NOT edit
  the marked content; it surfaces the concern, and the
  operator decides.

**For paragraph-level markers:**

- The marked paragraph is sacrosanct.
- Surrounding paragraphs in the same section may be edited
  per the agent's normal rules.

## What the operator does

When you hand-edit a section, add the marker:

```markdown
<!-- operator-touched: 2026-04-29 -->
```

If you're editing a paragraph rather than a whole section,
place the marker directly above that paragraph.

Updating the date is optional but useful for the
contradiction flagger and for sweep-mode aging logic.

## Future automation

Possible future shapes (not implemented yet):

- A git pre-commit hook that detects "human-shaped" diffs
  (small, inside an existing paragraph, no agent stream-jsonl
  in the same window) and adds the marker automatically.
- An editor wrapper (Cursor / VS Code extension) that adds
  the marker on save when the user has been the most recent
  editor.
- A renderer-level affordance: visually highlight
  operator-touched sections with a small operator-pen icon, so
  the reader knows what's machine-authored vs. hand-touched.

For now, the convention is manual but explicit.
