# 1Context Wiki Workspace

This directory is the default human-readable workspace for the memory core.

The wiki is intentionally plain files first:

```text
wiki/
  wiki.toml
  menu/
    <group>/
      group.toml
      <family>/
        family.toml
        source/
        talk/
        generated/
```

The public import does not include personal wiki content. A fresh install starts
empty, and `1context-memory-core wiki ensure --json` can create scaffolding once
families are configured.

Generated files and rendered output should be treated as rebuildable. Durable
user-authored wiki source belongs under user-owned content such as `~/1Context/`
once the macOS shell wires the memory core into the product runtime.

## Agent Usage Contract

Agents use the wiki through a small contract:

- read the current wiki URL, content index, source pages, and relevant talk
  conventions
- propose or apply source edits only when their role allows it
- leave reasoning, disagreements, and curation notes in talk folders
- cite evidence for durable claims
- stop after producing source edits or repair output

Agents do not publish the wiki, write `wiki-site/current`, repair Caddy state, or
decide whether a static bundle is safe to serve. Memory core owns semantic wiki
validation. The macOS app owns mechanical bundle safety and keeping the last good
published wiki online.

Agent startup context should summarize publication state in one or two lines,
for example "current site is healthy", "source has unpublished edits", or
"repair needed: broken link in `topics.md`". It should not ask ordinary agents
to reason from renderer logs or Swift bundle-gate internals.

## Release Publication Boundary

Release builds should only ship user-facing template and system-shell families
under `wiki/menu/`. Development goals, operator checklists, and control surfaces
belong under `docs/` or another explicit operator workflow, not in the installed
user wiki.

If a family must remain in `wiki/menu/` for a temporary operator workflow, mark
it out of normal user publication:

```toml
[policies]
publish_to_user_wiki = false
audience = "operator"
```

The normal site manifest, content index, macOS seed publisher, and packaged-site
copy path treat that policy as non-user-facing. A release package should never
include the former development `/goal` family or generated `/goal` assets.
