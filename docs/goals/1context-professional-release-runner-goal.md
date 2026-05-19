# Milestone: Professional Release Runner

## Goal

Greenfield the release control plane into a small typed runner that an agent can operate end to end with minimal human intervention. The public command stays `scripts/release-train.sh`, while release policy, manifest validation, phase orchestration, evidence writing, GitHub dispatch/audit, and bless rules move into `release/runner`.

The shell layer remains only where it is the right tool: macOS packaging, signing/notarization helpers, redaction helpers, and real GUI proof scripts.

## Done When

- `scripts/release-train.sh <validate|build|publish|prove|audit|bless>` is backed by the TypeScript release runner.
- `release/release.toml` has exactly one active parser/validator in the release path.
- Existing release behavior is preserved for dev dry-runs, appcast validation, proof dispatch dry-runs, asset manifests, timing evidence, redaction, and bless gates.
- CI can run the release runner tests and the existing release-train harness without hidden local dependencies.
- GitHub workflows are thin: permissions, runners, credentials, and command invocation only.
- Release evidence is machine-readable enough for an agent to audit or bless without human memory.
- Old manifest/control-plane code is deleted once the runner owns the behavior.

## Checklist

### 1. Baseline

- [x] Current operator surface identified: `scripts/release-train.sh validate|build|publish|prove|audit|bless`.
- [x] Current release truth identified: `release/release.toml`.
- [x] Current bloat/problem identified: release orchestration is split across a 1,064-line Bash control plane and a 1,027-line Python manifest helper.
- [x] Two xhigh read-only agents reviewed the architecture and proof/autonomy risks.

### 2. Runner Skeleton

- [x] Add isolated `release/runner` Node package with TypeScript, tests, and typed dependencies.
- [x] Add `.gitignore` coverage so runner dependencies/build output do not pollute git.
- [x] Implement typed manifest loader, validator, environment export, appcast checks, asset manifest writing, clean-tree checks, and fixture proof generation.
  Proof: `npm --prefix release/runner run build`, `npm --prefix release/runner test`, `./scripts/test-release-train.sh`.
- [x] Add runner unit tests for manifest policy, appcast policy, asset manifests, fixture proof results, and clean-tree failure.
  Proof: `npm --prefix release/runner test`.

### 3. Phase Orchestration

- [x] Implement shared release context, command execution, timing, and evidence writing.
- [x] Port `validate` phase.
  Proof: `./scripts/release-train.sh validate --channel dev`.
- [x] Port `build` phase while keeping shell helpers as platform adapters.
  Proof: `./scripts/release-train.sh build --channel dev --dry-run`, `./scripts/release-train.sh build --channel dev`.
- [x] Port `publish` phase without rebuilds and with private/official channel support.
  Proof: `./scripts/test-release-train.sh` command-order, asset-manifest, and secret-boundary checks.
- [x] Port `prove` phase with dry-run, dispatch, runner-execute, trusted-ref gating, and proof-reason-only workflow input.
  Proof: `./scripts/release-train.sh prove --dry-run --ref main --proof-reason 'goal proof'`, `./scripts/test-release-train.sh`.
- [x] Port `audit` phase and preserve `asset-manifest.json` staging.
  Proof: `./scripts/test-release-train.sh`.
- [x] Port `bless` phase as strict evidence inspection only.
  Proof: `./scripts/test-release-train.sh`.

### 4. Compatibility Cutover

- [x] Replace `scripts/release-train.sh` with a thin shim into the runner.
- [x] Move remaining old Python manifest callsites to runner manifest subcommands.
- [x] Delete `release/tools/release-manifest.py`.
- [x] Update docs and tests that name the old Python helper.

### 5. Workflow Hardening

- [x] Add CI runner install/build/test steps for `release/runner`.
- [x] Add workflow guard tests for self-hosted runner use, protected environments, and trusted refs.
- [x] Make the protected self-hosted signing/notarization/publish lane opt-in with `run_signed_publish`.
  Proof: `.github/workflows/release.yml` defaults `run_signed_publish=false`, emits an explicit validation-only job when disabled, and `./scripts/test-release-train.sh` guards the switch.
- [x] Add secret-boundary tests so publish/prove/audit do not expose signing/notary/Sparkle private credentials unnecessarily.
- [x] Make self-hosted Mac proof opt-in for the release workflow, with bless tied to that proof.
- [x] Disable artifact attestation/OIDC permissions until that path is intentionally designed.
  Proof: `.github/workflows/release.yml` uses read-only default permissions, grants `contents: write` only to publish, grants `actions: write` only to optional proof dispatch, and `./scripts/test-release-train.sh` rejects `id-token: write` / `attestations: write`.
- [x] Upload release evidence in release workflows with `if: always()`.
  Proof: `actionlint`, `./scripts/test-release-train.sh`.

### 6. Verification

- [x] `npm --prefix release/runner ci`
- [x] `npm --prefix release/runner run build`
- [x] `npm --prefix release/runner test`
- [x] `./scripts/test-release-train.sh`
- [x] `./scripts/test.sh`
- [x] `ONECONTEXT_ALLOW_LAUNCH_AGENT_SMOKE=1 ./scripts/test-launch-agent-package.sh`
- [x] `swift test --package-path macos`
- [x] `npm --prefix wiki-engine test`
- [x] `./scripts/test-wiki.sh`
- [x] `./scripts/release-train.sh validate --channel dev`
- [x] `ONECONTEXT_RELEASE_EVIDENCE_DIR=/tmp/... ./scripts/release-train.sh build --channel dev --dry-run`
- [x] `ONECONTEXT_RELEASE_EVIDENCE_DIR=/tmp/... ./scripts/release-train.sh prove --dry-run --ref main --proof-reason "..."`
- [x] `./scripts/release-train.sh build --channel dev`
  Result: actual `0.1.87` dev package build completed with DMG validation and redaction audit.
- [x] `actionlint`
- [x] `git diff --check`

## Notes

- Current status: local professional release runner migration is complete and `0.1.87` has passed the local dev release proof layer, but the signed publish run is queued until the protected self-hosted Mac comes online.
- Future workflow dispatches default to validation-only mode; use `run_signed_publish=true` for the real signed public release lane and add `run_self_hosted_proof=true` only when the real-Mac update bench should run too.
