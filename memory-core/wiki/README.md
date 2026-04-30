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
