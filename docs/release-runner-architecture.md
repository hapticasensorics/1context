# Release Runner Architecture

This is the target shape for professionalizing the 1Context release train without losing the good parts of the current system.

The current release model is conceptually strong: `release/release.toml` is the source of truth, the runner builds signed/notarized artifacts, update proof runs on a self-hosted Mac, and release blessing requires evidence. The weak part is implementation shape: too much orchestration still lives in a large Bash script. Bash should be the adapter at the edge, not the release control plane.

## Design Goals

- Keep one operator UX: `scripts/release-train.sh <validate|build|publish|prove|audit|bless> --channel <name>`.
- Move orchestration into a typed release runner.
- Keep GitHub Actions YAML thin and boring.
- Keep `release/release.toml` as the release schematic.
- Make every generated release fact machine-readable.
- Keep local and CI behavior as close as possible: the same release command should run in both places.
- Make failure messages operator-actionable.
- Preserve proof as evidence, not prose.

## Recommended Stack

Use TypeScript for the release runner.

TypeScript is the right center of gravity here because this repo already ships a Node-based wiki engine, release work is mostly structured data plus subprocess orchestration, and runtime validation matters. The runner should use:

- `commander` for CLI parsing.
- `execa` for subprocess execution and captured logs.
- `zod` for manifest, environment, GitHub response, and evidence validation.
- A TOML parser for `release/release.toml`.
- Node filesystem/path APIs for artifact and evidence management.

Avoid `zx` for the release runner. It is good for small scripts, but a release system benefits from explicit subprocess calls, explicit typed results, and small testable modules.

Do not do a reckless Bash-to-TypeScript rewrite. The old Python manifest helper was the closest thing to a release brain, so the migration must avoid split-brain parsers. TypeScript now owns the active manifest schema and exposes compatibility-style `manifest` subcommands for shell helpers. There should never be two independent interpretations of `release/release.toml`.

## Ownership Boundaries

```text
scripts/release-train.sh
  Stable compatibility shim and human entrypoint.
  No release policy. No JSON generation. No GitHub API logic.

release/runner/
  TypeScript release runner package.
  Owns phase orchestration, typed phase results, evidence, and errors.

release/release.toml
  Declarative release truth: versions, channels, update policy, proof requirements,
  signing mode, artifact repository, budget policy, and appcast URLs.

release/tools/
  Low-level platform helpers that are awkward or intentionally shell-native:
  DMG creation, notarization, redaction/audit helpers, self-hosted GUI proof.

.github/workflows/
  Thin execution harnesses. They choose trigger, runner, permissions, environment,
  and credentials, then call the release runner.

macos/
  Product build and platform behavior.

wiki-engine/
  Wiki rendering and runtime-defaults materialization tooling.
```

## Runner Module Shape

```text
release/runner/src/cli.ts
release/runner/src/manifest.ts
release/runner/src/context.ts
release/runner/src/phases/validate.ts
release/runner/src/phases/build.ts
release/runner/src/phases/publish.ts
release/runner/src/phases/prove.ts
release/runner/src/phases/audit.ts
release/runner/src/phases/bless.ts
release/runner/src/exec.ts
release/runner/src/evidence.ts
release/runner/src/github.ts
release/runner/src/appcast.ts
release/runner/src/errors.ts
release/runner/test/*.test.ts
```

The main type should be a `ReleaseContext` built once from the manifest, environment, git state, and selected channel. Phases receive that context and return typed `PhaseResult` objects. Evidence writing should be centralized so every phase records the same schema fields: phase, channel, version, started_at, ended_at, status, inputs, outputs, and proof file paths.

## Phase Contracts

### validate

- Parse and validate `release/release.toml`.
- Validate version agreement with `VERSION`, `Core.swift`, and release notes.
- Validate workflow references and proof requirements.
- Validate clean-tree and exact-tag rules only for channels that require them.
- Do not build or mutate artifacts.

### build

- Run validate first.
- Build the app bundle.
- Create DMG for every packaging channel, including `dev`.
- Sign/notarize only when the channel requires it.
- Generate appcast only when the channel requires it.
- Validate artifact contents and absence of private/local paths.
- Write asset manifest and build evidence.

### publish

- Run validate first.
- Upload only already-built artifacts.
- Never rebuild during publish.
- Record exact GitHub release URL, asset names, sizes, and hashes.
- For private channels, publish only to the configured private artifact repo.
- Emit artifact provenance where the host supports it, especially for DMGs, appcasts, checksums, and `asset-manifest.json`.

### prove

- Dispatch or execute proof using manifest-derived release facts.
- Reject caller-provided release facts that can drift from the manifest.
- On self-hosted runners, prove the GUI/update path with real installed apps.
- Download or collect proof artifacts into the evidence directory.
- Include updater matrix results as JSON.

### audit

- Verify public release assets after publication.
- Compare appcast, stable DMG, versioned DMG, checksums, and asset manifest.
- Re-probe latest/download URLs enough times to catch propagation issues.

### bless

- No building, publishing, or proof execution.
- Verify that required evidence exists and passed.
- Fail closed if any required proof result is missing or non-passing.
- Write final bless evidence.

## GitHub Actions Shape

GitHub Actions should provide execution context, not release logic.

Good workflow responsibilities:

- `checkout`
- install or select Node/Swift/Xcode toolchains
- unlock signing keychain
- expose secrets and vars
- select runner labels
- call `scripts/release-train.sh ...`
- upload evidence artifacts

Bad workflow responsibilities:

- computing versions
- constructing appcast URLs
- deciding update class
- branching release policy in YAML
- hand-building JSON
- duplicating shell command sequences already owned by the runner

Reusable workflows are useful when the same job shape is shared across repos. Composite actions are useful for bundling step sequences inside one job. For this repo, the first move should be simpler: keep workflows local and thin, then extract reusable workflows only after two or three repos share the same release shape.

## Self-Hosted macOS Runner Rules

- Treat self-hosted macOS proof as production infrastructure.
- Use restrictive runner labels and protected GitHub environments.
- Never route pull request or fork code to the signed/notarized/Sparkle proof runner.
- Keep signing/notary credentials scoped to the release environment.
- Never let untrusted pull-request code run on privileged self-hosted release runners.
- Record runner identity, OS version, Xcode version, app path, and proof artifact paths.
- Prefer foreground GUI sessions for Sparkle/update proof where prompts and screenshots matter.
- Keep proof scripts hermetic about release facts: old version, new version, update class, appcast URL, and repository must come from the manifest or workflow inputs generated from the manifest.

## Migration Plan

### 1. Stabilize the Current Surface

- Keep `scripts/release-train.sh` as the only operator command.
- Keep the current script cleanup: general scripts stay small; helper tools live under their owning subsystem.
- Add CI checks that reject new one-off release scripts unless they are explicitly owned by `release/tools`.

### 2. Introduce TypeScript Runner Skeleton

- Add `release/runner/package.json`, `tsconfig.json`, and tests.
- Implement `validate` first.
- Have `scripts/release-train.sh validate` call the TypeScript runner.
- Keep the old Bash implementation for the other phases while `validate` proves the pattern.
- Keep `scripts/release-train.sh manifest ...` as a low-level TypeScript runner subcommand for shell helpers; do not restore a second manifest validator.

### 3. Move Manifest and Evidence Logic

- Port manifest parsing, channel policy, appcast validation, artifact manifest writing, timing, and evidence schemas into TypeScript.
- Keep shell helpers for notarization, DMG creation, and GUI proof.
- Add unit tests for manifest edge cases and evidence schema output.

### 4. Move Build/Publish/Proof/Audit/Bless One Phase At A Time

- Port one phase.
- Run old and new outputs against fixtures where feasible.
- Keep CLI behavior stable.
- Delete old Bash branch only after the replacement phase passes local and CI validation.

### 5. Thin the Bash Shim

Final `scripts/release-train.sh` should be about 20 lines:

```bash
#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
exec node "$ROOT/release/runner/dist/cli.js" "$@"
```

During development, it can fall back to `tsx` or `npm run release --`.

## Anti-Patterns To Avoid

- A second release command with slightly different behavior.
- GitHub Actions YAML that reconstructs release policy.
- Bash functions that hand-write complex JSON.
- Release facts accepted from arbitrary environment variables when they belong in the manifest.
- Publishing steps that rebuild artifacts.
- Bless steps that run new proof instead of checking existing proof.
- Hidden local machine state such as Homebrew paths, source-tree fallbacks, or developer usernames inside shipped artifacts.
- Proof that only checks logs instead of installing, launching, updating, and inspecting the real app.

## Success Criteria

- `scripts/release-train.sh validate --channel dev` is backed by the TypeScript runner.
- Unit tests cover manifest parsing, channel policy, appcast validation, evidence writing, and command planning.
- GitHub Actions workflows contain no release policy beyond trigger, permissions, runner, environment, and command invocation.
- `scripts/release-train.sh build --channel dev` remains the local package proof.
- Official release still produces a notarized DMG, public appcast, asset manifest, redacted evidence, self-hosted update proof, audit, and bless evidence.
- Public release artifacts have provenance or attestations where GitHub supports them.
- The script folder stays under the current simplicity budget: few entrypoints, no release-helper sprawl.

## References

- GitHub Actions reusable workflows and permissions: https://docs.github.com/actions/reference/workflows-and-actions/reusable-workflows
- GitHub Actions Toolkit: https://github.com/actions/toolkit
- Zod runtime schema validation: https://zod.dev/api
- Execa subprocess execution: https://github.com/sindresorhus/execa
- Commander CLI framework: https://www.npmjs.com/package/commander
- zx scripting tool, useful for small scripts but not preferred for this release runner: https://google.github.io/zx/
