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
