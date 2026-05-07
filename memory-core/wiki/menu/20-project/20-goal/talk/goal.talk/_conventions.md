# Talk Conventions - Goal

The Goal talk page is the product-quality workbench for professional 1Context
behavior. It should turn strong product instincts into concrete acceptance
criteria, implementation notes, and release checks.

Use one timestamped markdown file per contribution. Each top-level file is one
thread. Replies use `parent:` frontmatter pointing to the parent filename or
stem.

## What Belongs Here

- Proposals about permission timing, setup repair, and blocked actions.
- Sparkle update policy, mandatory release rules, and old-version behavior.
- Evidence from local installs, update smokes, menu interactions, or support
  diagnostics.
- Decisions that explain why the Goal article changed.
- Corrections when a product standard is too broad, too timid, or not feasible
  on macOS.

## Promotion Rule

Do not turn a vague wish into a requirement. A goal entry should name the user
experience, the required system behavior, and how we will prove it.

## Archive Policy

Keep active talk entries in this folder for 90 days. After that, move settled
conversation files into `archive/` while preserving filenames, frontmatter, and
parent/thread references.
