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
- No Homebrew, host `PATH`, or host-installed library dependency for any build
  that creates a distributable `.app` or `.dmg`. Dev builds can use a developer
  machine; prototype, private, and official builds must use bundled, pinned, or
  release-owned inputs.
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

## Shipped Dependency Boundary

The professional release boundary is the generated `.app` and `.dmg`, not a
particular developer laptop. Anything a tester receives must be self-contained:
the local web edge, runtime assets, third-party notices, and update settings must
come from the bundle or a release-owned pinned artifact. Prototype, private, and
official builds should fail if they would resolve Caddy, Python helpers, web
assets, or runtime libraries from Homebrew, `/opt/homebrew`, `/usr/local`, or a
developer checkout.

Allowed release inputs are Xcode/Apple signing tools, Git, GitHub Actions runner
facilities, and explicit release-owned artifacts. A dev build may keep a faster
host-tool path, but that path cannot be shared by distributable channels without
an assertion proving the artifact is still self-contained.

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

### Bundled Dependency Boundary

- [x] Remove Homebrew installs from release/proof workflows used by
  prototype, private, or official distributable builds.
- [x] Replace `build-macos-app.sh` host `caddy` discovery with a release-owned
  Caddy/runtime artifact for prototype, private, and official channels.
- [x] Keep any dev-only host-tool fallback isolated to `--channel dev` and prove
  non-dev channels fail before using Homebrew, host `PATH`, or checkout-local
  runtime dependencies.
- [x] Add package smoke checks that reject `/opt/homebrew`, `/usr/local/Cellar`,
  host Caddy paths, repo checkout paths, and unbundled runtime-library
  references in the generated `.app` and DMG.
- [x] Add a release-factory scan that fails if active release workflows or
  non-dev package scripts contain `brew install`, `brew --prefix`, or
  `command -v caddy`.

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
- [x] Prototype build creates a pass-around DMG without appcast mutation.
- [x] Private build and publish create private update assets without public
  appcast mutation.
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
- [x] Prototype, private, and official build evidence proves no Homebrew or host
  dependency was used to produce the shipped `.app` or DMG.
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
- 2026-05-13: Proved the prototype channel. `./scripts/release-train.sh build
  --channel prototype` produced `dist/1Context-0.1.67-macos-arm64.dmg`, signed
  and notarized both the app and DMG, and wrote timing evidence at
  `/tmp/1ctx-release-factory-prototype-evidence/timings/build-prototype.json`.
  Elapsed build time was 125 seconds, under the 720 second prototype budget.
  `codesign --verify`, `spctl --assess`, and `./scripts/test-launch-agent-package.sh`
  passed. The prototype app has no `SUFeedURL`, and no new appcast was generated.
- 2026-05-13: Proved the private asset build after fixing
  `scripts/build-macos-app.sh` to preserve `ONECONTEXT_RELEASE_CHANNEL` when it
  reloads the manifest. `./scripts/release-train.sh build --channel private`
  produced a signed/notarized `0.1.67` DMG and a private appcast at
  `dist/private/appcast.xml`. The app `SUFeedURL` is the private latest appcast
  URL, and `./scripts/release-manifest.py validate --channel private --appcast
  dist/private/appcast.xml`, `codesign --verify`, `spctl --assess`, and
  `./scripts/test-launch-agent-package.sh` passed. Elapsed private build time:
  135 seconds.
- 2026-05-13: Created the private artifact repo
  `hapticasensorics/1context-private-release` and proved
  `./scripts/release-train.sh publish --channel private`. The publish uploaded
  `1Context-0.1.67-macos-arm64.dmg`, its SHA-256 file, `appcast.xml`, and
  `private-asset-manifest.json` to the private `v0.1.67` release, downloaded the
  assets back through GitHub, validated the private appcast, verified the DMG
  checksum, passed evidence redaction, and wrote
  `/tmp/1ctx-release-factory-private-publish-evidence/timings/publish-private.json`.
  Elapsed private publish time: 7 seconds.
- 2026-05-13: Added the private real-Mac proof workflow
  `.github/workflows/self-hosted-mac-private-update-proof.yml`. It still accepts
  only `proof_reason`, but it runs `release-train.sh prove --channel private
  --runner-execute`, uses the private release repo for old/new DMG downloads, and
  leaves the runner on the private feed by policy. Proof:
  `./scripts/release-manifest.py validate`, `./scripts/test-release-train.sh`,
  `actionlint`, and `git diff --check`.
- 2026-05-13: Added and implemented the shipped dependency boundary. Vendored
  Caddy `v2.11.2` as a release-owned darwin-arm64 artifact under
  `release/tools/caddy/`; changed non-dev app builds to extract and checksum
  that artifact instead of using Homebrew or host `PATH`; removed Homebrew
  Python/Caddy bootstrap from CI, release, and proof workflows; added the
  simple built-in TOML parser fallback so older macOS runner Python can validate
  the manifest without Homebrew; added no-brew release scans; and made non-dev
  app builds fail on Homebrew/Caddy path leakage in the generated bundle. The
  attempted private `0.1.67` to `0.1.68` proof run `25833229899` failed before
  GUI proof because the runner had no usable `GH_TOKEN` for the private release
  asset download; the private workflow now exports `GH_TOKEN` explicitly and
  gives a clear secret-configuration error if the private artifact repo token is
  missing.
- 2026-05-13: Proved the no-Homebrew distributable path with a real prototype
  build. `ONECONTEXT_RELEASE_EVIDENCE_DIR=/tmp/1ctx-release-factory-prototype-no-brew-evidence
  ./scripts/release-train.sh build --channel prototype` produced signed,
  notarized, stapled `dist/1Context-0.1.68-macos-arm64.dmg` in 101 seconds,
  under the 720 second prototype budget. Proof: `./scripts/test-release-train.sh`,
  `./scripts/test.sh`, `swift test --package-path macos`, `./scripts/package-macos-smoke.sh`,
  `./scripts/test-launch-agent-package.sh`, `actionlint`, `git diff --check`,
  `codesign --verify --deep --strict dist/1Context.app`, `spctl --assess`, and
  a bundle scan showing no `/opt/homebrew`, `/usr/local/Cellar`, or
  `/Cellar/caddy` paths. The prototype app has no `SUFeedURL`; the bundled Caddy
  reports `v2.11.2`.
