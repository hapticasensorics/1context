# Public Export Workflow

The private repository owns the working product source. The public repository is
the published downstream tree.

Current local roles:

- `/Users/paulhan/dev/1Context`: private repository, remote
  `hapticasensorics/1context-private.git`.
- `/Users/paulhan/dev/1Context-private-2`: private wiki-origin branch,
  `private-2`.
- `/Users/paulhan/dev/1Context-private-4`: private architecture/artifact branch,
  `private-4`.
- `/Users/paulhan/dev/1context-public-launch`: public repository, remote
  `hapticasensorics/1context.git`.

The private branch `codex/public-product-source` is seeded from the current
public product tree. Future public product work should land there first, then
the public repository should pull from it through the export tool.

## Export

From `/Users/paulhan/dev/1context-public-launch`:

```bash
uv run python devtools/export-public-from-private.py
```

The default run is a dry run. It compares tracked files in the private source
ref against allowlisted public paths and refuses private-only path families such
as `experiments/`, `sessions/`, `content/`, `agent/`, and `.1context/`.

Apply an export:

```bash
uv run python devtools/export-public-from-private.py --apply
```

Then inspect, test, commit, and push the public branch:

```bash
git status --short
bash devtools/cleanup-guard.sh --strict
```

The manifest is `public-export.toml`. Override the local source repo or ref when
needed:

```bash
ONECONTEXT_PRIVATE_REPO=/Users/paulhan/dev/1Context \
ONECONTEXT_PUBLIC_SOURCE_REF=codex/public-product-source \
uv run python devtools/export-public-from-private.py
```

## Branch Contract

- Private source branch: `hapticasensorics/1context-private.git`,
  `codex/public-product-source`.
- Public publish branch: `hapticasensorics/1context.git`, normally `main` or a
  release PR branch.
- Public export changes should cite the private source commit in the commit
  message.
- Do not point the export tool at `private-2` or `private-4`; those are retained
  as private history branches.
