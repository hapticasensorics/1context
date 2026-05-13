# 1Context Release Factory Goal

Status: active
Owner: Codex
Target train: 0.1.67 through 0.1.70 unless the manifest says otherwise

## Purpose

Build a fast, boring, closed-loop release factory for 1Context.

The factory must support four jobs without reviving old release bloat:

- Dev build: local iteration that proves the app compiles and basic contracts hold.
- Prototype copy: a DMG we can hand to a tester without mutating any public update feed.
- Private release: signed, notarized assets and a private appcast for real-machine testing.
- Official release: signed, notarized, public Sparkle assets with multi-machine proof and bless evidence.

The goal is not to preserve compatibility with deleted release paths. The goal is to
delete bloat, make the current path explicit, and keep every release fact in the
manifest and evidence bundle.

## Non-Negotiables

- No backwards-compatible shims.
- No old wrapper scripts that call the new path while pretending to be supported.
- No environment-variable side channels for release facts, channels, user copy, or
  appcast policy when those facts belong in `release/release.toml`.
- No shipped source checkout, generated wiki source, dev goal, local path, or test
  harness artifact in the app bundle.
- No repo-local runtime bypasses in product code.
- No public appcast mutation for dev, prototype, or private builds.
- No human-interpreted success for official releases. Official bless needs
  machine-readable proof JSON, timing JSON, asset manifests, runner attestations,
  redaction reports, and real-Mac GUI evidence.

## Target Times

These targets measure elapsed wall time on an ordinary warmed runner unless noted.
Human attention after kickoff should stay under five minutes.

- Dev warm build plus smoke: 30 to 90 seconds.
- Dev cold build plus smoke: under 3 minutes.
- Prototype signed DMG: 3 to 8 minutes warm, under 12 minutes cold.
- Private signed/notarized release assets: 5 to 12 minutes.
- Private proof on real Macs: 10 to 20 minutes elapsed, parallel and unattended.
- Official signed/notarized assets: 8 to 15 minutes.
- Official full proof and bless: 15 to 30 minutes P50, 45 minutes P95.

If a target is unrealistic, the release evidence should say why with stage timings
instead of hiding the cost in a giant shell log.

## Design

`release/release.toml` remains the release source of truth. The release factory
adds channel data, timing budgets, proof budgets, private appcast facts, and
artifact policy to that same manifest. Docs explain policy, but tooling never
reads docs as release truth.

`scripts/release-train.sh` remains the only release UX, but the command language
should become factory-shaped:

```bash
scripts/release-train.sh validate
scripts/release-train.sh build --channel dev
scripts/release-train.sh build --channel prototype
scripts/release-train.sh build --channel private
scripts/release-train.sh build --channel official
scripts/release-train.sh publish --channel private
scripts/release-train.sh publish --channel official
scripts/release-train.sh prove --channel private
scripts/release-train.sh prove --channel official
scripts/release-train.sh audit --channel official
scripts/release-train.sh bless --channel official
```

`package` is not a release-factory command. It should disappear from workflows,
docs, tests, and the command parser.

## Channel Contract

- `dev`: local app bundle and deterministic smoke. No notarization, no appcast,
  no GitHub release, no public artifact mutation.
- `prototype`: signed DMG suitable for passing around. No Sparkle appcast and no
  GitHub release mutation.
- `private`: signed, notarized DMG plus private appcast and private evidence.
  This can update tester machines, but it must not change the public latest feed.
- `official`: signed, notarized DMG, public appcast, stable DMG, GitHub release
  assets, multi-machine proof, audit, and bless.

Optional and mandatory release policy stays manifest-owned. Optional releases can
show the approved prompt. Mandatory releases can interrupt active use and install
immediately. Failed attempted updates show the simple support message only after
the retry budget is exhausted.

## Memory Core Boundary

Memory core can come back only as an allowlisted runtime artifact. It must not
come back as a copied source checkout, host `uv run`, runtime `npm ci`, or broad
plugin tree. The factory must keep package-smoke checks that fail on
`Contents/Resources/memory-core`.

## Checklist

### Delete Bloat First

- [x] Delete the repo-local runtime bypass and `ONECONTEXT_RUNTIME_ROOT` product
  path override when it reappears.
- [x] Add a no-shim scan that fails on runtime root bypasses, old release
  wrappers, `package` release commands, and deleted update-policy names.
- [ ] Delete or internalize stale release scripts that are not factory commands.
- [x] Rewrite stale docs so active docs mention factory commands only.

### Manifest And Commands

- [x] Extend `release/release.toml` with channel policies, timing budgets,
  artifact policy, and private appcast facts.
- [x] Validate those fields in `scripts/release-manifest.py`.
- [x] Replace `release-train.sh package` with `release-train.sh build`.
- [x] Route GitHub release workflow through `build --channel official`.
- [x] Keep the self-hosted proof workflow manifest-only, with humans supplying
  only `proof_reason`.

### Speed And Evidence

- [x] Emit stage timing JSON for validate, build, publish, prove, audit, and bless.
- [ ] Fail when a stage exceeds its manifest budget unless the channel marks the
  budget as advisory.
- [ ] Add a timing summary to release evidence so slow notarization, SwiftPM
  fetch/build, DMG creation, upload, and runner proof are visible.
- [ ] Keep expensive real-Mac proof parallel and unattended.

### Channel Proof

- [x] Dev build passes local smoke without public asset mutation.
- [ ] Prototype build creates a pass-around DMG without appcast mutation.
- [ ] Private release creates private update assets and proves update on at
  least one non-primary Mac.
- [ ] Official release proves public Sparkle update on the protected runner.
- [ ] Official release proves install, update, uninstall without data deletion,
  reinstall, and controlled delete-data in a throwaway account.

### Memory Runtime Reintroduction

- [ ] Define the smallest memory runtime artifact contract.
- [ ] Build that artifact from source at package time or CI time.
- [ ] Package only allowlisted runtime files.
- [ ] Add package smoke for runtime artifact contents, size, and absence of
  source checkout paths.

### Exit Criteria

- [ ] `scripts/release-train.sh validate` passes.
- [ ] `scripts/release-train.sh build --channel dev` passes within target time.
- [ ] `scripts/release-train.sh build --channel prototype` passes within target time.
- [ ] `scripts/release-train.sh build --channel private` passes within target time.
- [ ] `scripts/release-train.sh build --channel official` passes within target time.
- [ ] Private proof passes on the self-hosted Mac runner.
- [ ] Official proof, audit, and bless pass from a clean tag.
- [ ] The active docs, workflows, tests, and scripts have no old release command
  or compatibility-shim references.

## Evidence Log

- 2026-05-13: Removed the reintroduced repo-local runtime bypass before starting
  the factory. Deleted `runtime/`, `scripts/dev-runtime-env.sh`, and
  `scripts/with-dev-runtime.sh`; removed the `ONECONTEXT_RUNTIME_ROOT` product
  path override diff from Swift, tests, docs, and `scripts/test.sh`.
- 2026-05-13: First factory tranche landed. Added channel policy and timing
  budgets to `release/release.toml`; validated them in `scripts/release-manifest.py`;
  replaced `release-train.sh package` with `release-train.sh build`; routed the
  protected release workflow through `build --channel official`; deleted
  `scripts/check-release-manifest.sh`; and added the no-shim scan in
  `scripts/test-release-train.sh`. Proof: `./scripts/test-release-train.sh`,
  `./scripts/test.sh`, `./scripts/release-train.sh validate --channel dev`,
  `./scripts/release-train.sh build --channel dev`, `./scripts/test-launch-agent-package.sh`,
  and `git diff --check`. The real dev factory build completed in 16 seconds.
- 2026-05-13: Bumped the next honest train to `0.1.67` so current `main` no
  longer pretends to be the already-cut `0.1.66` tag. Updated `VERSION`,
  `Core.swift`, `RELEASE_NOTES.md`, the runbook, and `release/release.toml`.
  Proof: `./scripts/release-manifest.py validate`, `./scripts/test-release-train.sh`,
  `./scripts/test.sh`, `swift test --package-path macos`, and `git diff --check`.
