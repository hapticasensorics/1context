# 1Context Release Factory Goal

Status: active
Owner: Codex
Target train: 0.1.67 through 0.1.81 unless the manifest says otherwise

## Purpose

Build a fast, boring, closed-loop release factory for 1Context.

The factory must support four jobs without reviving old release bloat:

- Dev build: local iteration that proves the app compiles and basic contracts hold.
- Prototype copy: a DMG we can hand to a tester without mutating any public update feed.
- Private release: signed, notarized assets and a controlled tester appcast for
  real-machine testing. The channel is private from the official-public-feed
  perspective, but the installed app must be able to fetch the appcast and DMG
  over unauthenticated HTTPS.
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
- No official public appcast mutation for dev, prototype, or private builds.
- No authenticated update feed for tester Sparkle updates. A private GitHub repo
  can hold operator-only evidence, but it cannot be the appcast origin unless
  the shipped app also owns a real authentication design. For this release
  factory, private-channel appcasts must be app-reachable over ordinary HTTPS.
- No Homebrew, host `PATH`, or host-installed library dependency for any build
  that creates a distributable `.app` or `.dmg`. Dev builds can use a developer
  machine; prototype, private, and official builds must use bundled, pinned, or
  release-owned inputs.
- The distributable boundary is the artifact, not the script name. A dev-only
  local app may be convenient, but any `.app` or DMG intended for testers,
  private release, or official release must be self-contained and pass the
  dependency audit.
- No hidden dynamic-library or script-interpreter dependency on host package
  managers or language runtimes in distributable app bundles. The package audit
  must inspect Mach-O linkages and executable shebangs, not just grep readable
  resource files.
- No human-interpreted success for official releases. Official bless needs
  machine-readable proof JSON, timing JSON, asset manifests, runner attestations,
  redaction reports, and real-Mac GUI evidence.
- Updates must not require manual sudo or a fresh Local Wiki Access grant for an
  already setup user. Admin authorization belongs to first setup, setup repair,
  or reinstall/delete-data proof, and any such prompt must be captured as setup
  evidence instead of counted as update success.

## Target Times

These targets measure elapsed wall time on an ordinary warmed runner unless noted.
Human attention after kickoff should stay under five minutes. Targets should stay
close to observed factory speed plus about one minute of guard band; if a lane
gets slower, the timing artifact must explain the cost.

- Dev warm build plus smoke: under 90 seconds.
- Dev cold build plus smoke: under 2 minutes.
- Prototype signed/notarized DMG: under 3 minutes warm, under 5 minutes cold.
- Private signed/notarized release assets: under 4 minutes.
- Private publish: under 30 seconds.
- Private proof on real Macs: under 5 minutes elapsed, parallel and unattended.
- Official signed/notarized assets plus public audit: under 4 minutes.
- Official standard proof and bless: under 6 minutes.
- Official destructive uninstall/reinstall/delete-data proof and bless: under 10
  minutes until the first full 0.1.81 measurement replaces this provisional cap.

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
- `private`: signed, notarized DMG plus controlled tester appcast and private
  evidence. This can update tester machines, but it must not change the
  official public latest feed. The tester appcast may be unlisted, but it must
  not require GitHub authentication from Sparkle.
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
- [x] Delete or internalize stale release scripts that are not factory commands.
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
- [x] Add Mach-O dynamic-library and executable-shebang auditing for
  distributable `.app` bundles so hidden Homebrew, Python, Node, npm, uv, or
  local-library dependencies fail before a `.dmg` is accepted.
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
- [x] Fail when a stage exceeds its manifest budget unless the channel marks the
  budget as advisory.
- [x] Add a timing summary to release evidence so slow notarization, SwiftPM
  fetch/build, DMG creation, upload, and runner proof are visible.
- [ ] Keep expensive real-Mac proof parallel and unattended.

### Channel Proof

- [x] Dev build passes local smoke without public asset mutation.
- [x] Prototype build creates a pass-around DMG without appcast mutation.
- [x] Private build and publish create private update assets without public
  appcast mutation.
- [x] Private-channel appcast and enclosure URLs are app-reachable over
  unauthenticated HTTPS; a private GitHub repository is not accepted as the
  Sparkle feed origin.
- [x] Private-channel release facts come from the manifest-owned
  `hapticasensorics/1context-preview-release` artifact repo, with no
  `ONECONTEXT_PRIVATE_GITHUB_REPO` or token side channel.
- [x] Private release creates private update assets and proves update on at
  least one non-primary Mac.
- [x] Official release proves public Sparkle update on the protected runner.
- [ ] Official release proves install, update, uninstall without data deletion,
  reinstall, and controlled delete-data in a throwaway account.

### Memory Runtime Reintroduction

- [x] Define the smallest memory runtime artifact contract.
- [x] Build that artifact from source at package time or CI time.
- [x] Package only allowlisted runtime files.
- [x] Add package smoke for runtime artifact contents, size, and absence of
  source checkout paths.

### Exit Criteria

- [x] `scripts/release-train.sh validate` passes.
- [x] `scripts/release-train.sh build --channel dev` passes within target time.
- [x] `scripts/release-train.sh build --channel prototype` passes within target time.
- [x] `scripts/release-train.sh build --channel private` passes within target time.
- [x] `scripts/release-train.sh build --channel official` passes within target time.
- [x] Private proof passes on the self-hosted Mac runner.
- [x] Official proof, audit, and bless pass from a clean tag.
- [x] Prototype, private, and official build evidence proves no Homebrew or host
  dependency was used to produce the shipped `.app` or DMG.
- [x] The active docs, workflows, tests, and scripts have no old release command
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
  Elapsed build time was 125 seconds, under the 3 minute warmed prototype
  budget.
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
  under the 3 minute warmed prototype budget. Proof: `./scripts/test-release-train.sh`,
  `./scripts/test.sh`, `swift test --package-path macos`, `./scripts/package-macos-smoke.sh`,
  `./scripts/test-launch-agent-package.sh`, `actionlint`, `git diff --check`,
  `codesign --verify --deep --strict dist/1Context.app`, `spctl --assess`, and
  a bundle scan showing no `/opt/homebrew`, `/usr/local/Cellar`, or
  `/Cellar/caddy` paths. The prototype app has no `SUFeedURL`; the bundled Caddy
  reports `v2.11.2`.
- 2026-05-13: Rebuilt and republished the private `0.1.68` assets from a clean
  detached worktree at `38665b8` so the private channel also uses the
  release-owned Caddy artifact. Private build evidence:
  `/tmp/1ctx-release-factory-private-no-brew-build-evidence/timings/build-private.json`;
  elapsed 152 seconds. Private publish evidence:
  `dist/release-evidence/0.1.68/timings/publish-private.json`; elapsed 8
  seconds. The private appcast downloaded back from
  `hapticasensorics/1context-private-release` and validated. The first private
  proof rerun reached old-DMG install, then failed because the remote update
  harness fetched the private appcast with unauthenticated `curl`; fixed the
  private runner path so it passes `ONECONTEXT_REMOTE_APPCAST_GITHUB_REPO` and
  `prove-remote-sparkle-update.sh` downloads private appcasts through `gh`.
- 2026-05-13: The next private proof run `25834251490` proved the remaining
  design flaw: the harness can download the private appcast through `gh`, but
  the installed app cannot use a GitHub-private appcast or enclosure URL. The
  downloaded `live-appcast.xml` correctly advertised `0.1.68` with critical
  update metadata, but the installed `0.1.67` app stayed at `plist=0.1.67
  cli=0.1.67` until the 420 second proof timeout. This is now a release-factory
  requirement, not a runner problem: the private channel must publish the
  Sparkle appcast and DMG to an unauthenticated HTTPS origin while still avoiding
  mutation of the official public latest feed.
- 2026-05-13: Repointed the private-channel policy toward an app-reachable
  preview artifact repo. `release/release.toml` now owns the artifact repo,
  requires public visibility plus `appcast_authentication = "none"`, and removes
  the `ONECONTEXT_PRIVATE_GITHUB_REPO` side channel from `release-train.sh`.
  Created `hapticasensorics/1context-preview-release` as a public release-asset
  repo for the private channel, and proved the manifest/test wiring with
  `./scripts/release-manifest.py validate`, private-channel `export-env`,
  `bash -n`, `actionlint`, and `./scripts/test-release-train.sh`.
- 2026-05-13: Built and published app-reachable preview assets. The clean
  worktree private `0.1.68` build wrote
  `/tmp/1ctx-release-factory-preview-build-evidence/timings/build-private.json`
  with elapsed 81 seconds, bundled Caddy `v2.11.2`, no Homebrew path scan hits,
  and `SUFeedURL=https://github.com/hapticasensorics/1context-preview-release/releases/latest/download/appcast.xml`.
  Published `v0.1.68` to `hapticasensorics/1context-preview-release` in 7
  seconds, then created a temporary clean `0.1.67` baseline with the same
  preview feed and published it in 8 seconds. Unauthenticated `curl` proved
  `latest/download/appcast.xml` resolves to `0.1.68`, and both `0.1.67` and
  `0.1.68` DMG URLs return public release assets.
- 2026-05-13: Private proof run `25835070446` proved the actual Sparkle hop:
  the runner installed preview `0.1.67`, launched the app, fetched the live
  preview appcast, and observed `plist=0.1.68 cli=0.1.68` on the second watch
  iteration. The run still failed the post-update steady-state gate because the
  runtime LaunchAgent recovered but the menu LaunchAgent was loaded with no PID.
  The fix is product-owned: the daemon now repairs/kickstarts the menu
  LaunchAgent on runtime startup, so the KeepAlive runtime can restore the menu
  after mandatory updates, login, or restart.
- 2026-05-13: The follow-up `0.1.68` to `0.1.69` proof showed the daemon repair
  did execute: runtime logs include `menu launch agent ready
  path=/Applications/1Context.app/Contents/MacOS/1Context`, and the update
  reached `plist=0.1.69 cli=0.1.69`. The remaining failure was proof narrowness:
  Sparkle can leave the menu running as a normal app process while the menu
  LaunchAgent remains loaded without owning that PID. The steady-state proof now
  records the menu process list and accepts either a launchd-owned menu PID or a
  live `/Applications/1Context.app/Contents/MacOS/1Context` process plus a
  loaded menu LaunchAgent. Harness proof: `bash -n
  scripts/release/internal/verify-macos-steady-state.sh`, `./scripts/test-release-train.sh`,
  `./scripts/test.sh`, and `git diff --check`.
- 2026-05-13: Private proof run `25835624162` passed on the self-hosted Mac
  runner after the steady-state harness accepted Sparkle-relaunched menu
  ownership. `release-train.sh prove --channel private` dispatched the
  manifest-only workflow, approved the protected runner environment, downloaded
  the proof artifacts, copied normalized proof JSON into
  `/tmp/1ctx-release-factory-preview-069-proof-dispatch-2/proof-results`, and
  wrote timing evidence at
  `/tmp/1ctx-release-factory-preview-069-proof-dispatch-2/timings/prove-private.json`.
  Evidence: `0.1.68 -> 0.1.69` update succeeded, elapsed proof dispatch was 214
  seconds, the GitHub job completed in 2 minutes 50 seconds, redaction audit
  passed, and proof cases `mandatory_automatic_success`,
  `already_current_manual_check`, `app_relaunch_recovery`,
  `old_app_with_new_appcast`, and `stale_sparkle_defaults` all passed.
- 2026-05-13: Made speed evidence enforceable. Private and official channels
  now have non-advisory manifest budgets, while dev and prototype remain
  advisory for local iteration. `release-train.sh` writes step-level timing
  records plus `timing-summary.json`, and `release-evidence.json` lists that
  summary as required evidence. The summary exposes build, notarization, DMG,
  appcast, publish/upload, proof dispatch/watch/download, redaction, audit, and
  bless timing without requiring a human to read a shell transcript. Proof:
  `./scripts/release-manifest.py validate`, private-channel `export-env`
  showing `ONECONTEXT_RELEASE_BUDGET_ADVISORY=0`, `./scripts/test-release-train.sh`,
  and `git diff --check`.
- 2026-05-13: Internalized the remaining top-level proof engines. Moved
  `lib-gui-evidence.sh`, `prove-remote-sparkle-update.sh`, and
  `verify-macos-steady-state.sh` under `scripts/release/internal/`, updated the
  self-hosted proof runner and tests to call those internal paths, and deleted
  the stale `scripts/clean-release-artifacts.sh` helper instead of preserving
  another release-shaped public command. Proof: no runnable top-level proof
  helper remains, active docs point at internal proof engines only, `bash -n`
  passes over the moved scripts, `./scripts/test-release-train.sh` passes, and
  `git diff --check` passes.
- 2026-05-13: Prepared the official public hop as `0.1.65 -> 0.1.70` instead
  of reusing the preview-channel `0.1.68 -> 0.1.69` previous version. The first
  local official build attempt exposed a factory bug: missing local
  `1context-notary` credentials made notarization fail, but the timing wrapper
  returned success after recording failed substeps. Fixed `time_release_step`
  so any failed subprocess fails the release stage immediately. Proof:
  `./scripts/test-release-train.sh`, `./scripts/test.sh`, `bash -n
  scripts/release-train.sh`, and `git diff --check`. The official build remains
  unproven until it runs from `v0.1.70` on a runner with notarization
  credentials.
- 2026-05-13: The first protected `v0.1.70` release workflow run
  `25836351389` failed before credentials unlocked because the macOS runner
  executed `release-train.sh` with system Bash 3.2, which treats an empty array
  expansion under `set -u` as an unbound variable. Fixed the argument parser and
  then fixed the same empty-array pattern in default manifest loading after
  retry run `25836445237` exposed it. Added `/bin/bash` regression checks for
  both default and explicit-channel validation. Proof:
  `/bin/bash scripts/release-train.sh validate --channel dev`,
  `./scripts/test-release-train.sh`, `./scripts/test.sh`, and `git diff --check`.
- 2026-05-13: Protected release retry run `25836538883` passed manifest
  validation and release-keychain preparation, then correctly failed credential
  preflight because the workflow did not pass `SPARKLE_PRIVATE_ED_KEY` into
  `scripts/check-macos-release-credentials.sh`. The build step already had the
  secret, but preflight must check the same credential set before expensive
  work starts. Fixed `.github/workflows/release.yml` so credential preflight
  receives `secrets.ONECONTEXT_SPARKLE_PRIVATE_ED_KEY`.
- 2026-05-13: Protected official release run `25836611212` passed from tag
  `v0.1.70` in 2 minutes 43 seconds. The workflow validated the manifest,
  prepared the release keychain, passed credential preflight, built signed and
  notarized official artifacts, uploaded GitHub release assets, and passed the
  public asset audit. Independent audit proof:
  `ONECONTEXT_RELEASE_AUDIT_PROBES=1 ./scripts/release-train.sh audit --channel
  official` passed; public latest appcast now advertises mandatory `0.1.70`
  with `sparkle:minimumAutoupdateVersion` `0.1.65`, and both versioned and
  stable DMG downloads passed checksum/size validation.
- 2026-05-13: Official real-Mac update proof run `25836730311` passed through
  `release-train.sh prove --channel official`, proving the public `0.1.65 ->
  0.1.70` mandatory Sparkle hop on the protected runner in 2 minutes 49
  seconds. Downloaded proof evidence lives at
  `/tmp/1ctx-release-factory-official-070-proof-dispatch`; normalized proof
  JSON passed for `mandatory_automatic_success`, `already_current_manual_check`,
  `app_relaunch_recovery`, `old_app_with_new_appcast`, and
  `stale_sparkle_defaults`, with no release-notes prompt, installer
  click-through, support alert, or runtime pause.
- 2026-05-13: Added release-factory support for fixture-owned updater matrix
  proof results. `release-train.sh prove` now runs the focused
  `OneContextSparkleUpdateTests` fixture suite after real-Mac artifacts
  download, then asks `release-manifest.py write-fixture-proof-results` to emit
  normalized JSON for the seven `sparkle_fixture` cases. This leaves only the
  `login_restart_recovery` / real uninstall-reinstall proof missing before
  `release-train.sh bless` can pass the full matrix.
- 2026-05-13: Tightened the shipped dependency boundary beyond path greps.
  Added `scripts/audit-macos-app-dependencies.sh`, wired it into non-dev app
  builds and package smoke, and added a regression test that rejects an
  executable script whose shebang points at Homebrew Python. Local proof:
  `./scripts/audit-macos-app-dependencies.sh dist/1Context.app` passed.
- 2026-05-13: Added the missing real-Mac `login_restart_recovery` proof step to
  the self-hosted update runner. After the update and already-current check,
  the runner now stops 1Context, reopens the installed app, and runs the
  steady-state verifier against menu, runtime, setup, and local web before it
  emits `proof-results/login_restart_recovery.json`. This is wired but still
  needs the next official tagged proof run before the exit box can be checked.
- 2026-05-13: Archived the completed delete-bloat goal so active docs point to
  the release-factory goal and manifest-driven runbook only. The old cleanup
  file remains under `docs/goals/archive/` as historical evidence, not an
  active release surface.
- 2026-05-13: Official `v0.1.72` passed the full public release-factory close.
  The protected release workflow `25838276795` built, signed, notarized,
  published, and audited official assets in 2 minutes 46 seconds. The protected
  real-Mac proof workflow `25838404159` passed in 3 minutes 36 seconds, proving
  the public `0.1.71 -> 0.1.72` Sparkle hop plus already-current manual check,
  app relaunch recovery, old-app/new-appcast behavior, stale Sparkle defaults,
  and login/restart recovery. `release-train.sh prove --channel official`
  downloaded the runner artifacts, emitted the remaining fixture-owned matrix
  proof JSON, and passed redaction audit. Reconstructed the official
  `asset-manifest.json` from public GitHub release assets, redacted/audited the
  evidence bundle, and `release-train.sh bless --channel official` passed for
  `v0.1.72`.
- 2026-05-13: Closed a release-factory failure-propagation bug found while
  updating this goal. A dirty-tree official `bless` printed the clean-tree
  preflight error but continued because Bash suppresses `set -e` inside
  functions invoked through `if "$@"`. `time_release_step` now executes each
  step in a strict subshell and records failed timing evidence before returning
  the original exit code. Regression proof: `scripts/test-release-train.sh`
  creates a tracked-file dirty fixture and asserts official dry-run build stops
  at `build-official-validate-preflight` with status `failed`.
- 2026-05-13: Reintroduced memory runtime as a small release artifact instead
  of a source checkout. `release/memory-runtime/CONTRACT.md` defines the
  allowlist; `scripts/build-memory-runtime-artifact.sh` builds
  `dist/release-tools/memory-runtime` from static HTML/JSON source, rejects
  Python, Node, shell, markdown, package-manager files, generated directories,
  local paths, dev goal routes, `memory-core`, `uv run`, and `npm ci`, and
  writes `manifest.json` with file hashes and total size. `build-macos-app.sh`
  copies that artifact into `Contents/Resources/memory-runtime`; the local web
  publisher consumes `memory-runtime/wiki-site` when seeding the app-owned wiki
  shell. Package smoke now requires the artifact, validates schema, required
  files, and size, and still rejects `Contents/Resources/memory-core`. Proof:
  `./scripts/build-memory-runtime-artifact.sh`, `swift test --package-path
  macos`, `./scripts/test-release-train.sh`, `./scripts/package-macos-smoke.sh`,
  `./scripts/test-launch-agent-package.sh`, `./scripts/test.sh`, and
  `git diff --check`. The packaged artifact is 40 KB on disk and contains only
  the allowlisted static wiki seed plus manifest.
- 2026-05-13: Wired the official real-Mac proof to cover the remaining
  uninstall/reinstall lane for the next tagged release. Official
  `release-train.sh prove --runner-execute` now enables
  `ONECONTEXT_RUN_UNINSTALL_REINSTALL_PROOF=1` and
  `ONECONTEXT_UPDATE_RUNNER_ALLOW_DELETE_DATA=1`, while private proof skips the
  destructive lane. The self-hosted runner proof now runs normal uninstall,
  verifies a `~/1Context` sentinel survives, reinstalls the current release DMG,
  restores Local Wiki Access through the real menu setup UI, verifies steady
  state, then runs controlled `uninstall --delete-data --keep-app`, proves an
  approved 1Context sentinel is removed while an adjacent non-1Context sentinel
  survives, restores setup again, and emits
  `proof-results/real_uninstall_reinstall.json`. This is implemented and
  locally syntax/test verified, but the Channel Proof checkbox remains open
  until a protected official tagged proof run downloads and blesses that JSON.
- 2026-05-13: Tightened release-factory speed targets to match measured
  factory behavior plus about one minute of guard band instead of carrying old
  12 to 45 minute budgets. The goal now targets prototype DMGs under 3 minutes,
  private release assets under 4 minutes, private real-Mac proof under 5
  minutes, official signed/notarized assets plus public audit under 4 minutes,
  official standard proof plus bless under 6 minutes, and the first destructive
  uninstall/reinstall proof under a provisional 10 minute cap. The same tighter
  budgets are now encoded in `release/release.toml`, with private and official
  channels still non-advisory.
- 2026-05-13: Official `v0.1.75` proved that the public update path, manual
  already-current check, login/restart recovery, and public asset audit are
  fast and healthy, but the destructive uninstall/reinstall lane exposed a
  setup automation gap: after reinstall, the runner could leave the app waiting
  on the Local Wiki Access setup window. `v0.1.76` now hardens that lane by
  foregrounding the setup window, clicking nested AppKit buttons, capturing
  after each `Grant` action, looping through `Grant` / `Check Again`, and
  failing with setup-specific evidence instead of a generic recovery timeout.
- 2026-05-13: Official `v0.1.76` proved the stronger setup GUI evidence path:
  the runner clicked `Grant` and captured each retry, but the shipped app kept
  reporting `Run 1Context setup as your macOS user, not with sudo` because the
  runner GUI process inherited a stale `SUDO_USER` environment variable. The
  `v0.1.77` product fix makes app-owned lifecycle/setup guards reject only
  true root/euid 0 execution, with a regression test proving stale `SUDO_USER`
  does not block a normal logged-in user process.
- 2026-05-13: Official `v0.1.77` then exposed that the proof still had a
  human-clean-runner precondition: it failed immediately if the currently
  installed runner app was not already setup-ready. That contradicts the
  release-factory goal. `v0.1.78` records pre-run setup readiness as evidence
  only; the proof owns installing the baseline app and restoring setup through
  the same GUI lane it is validating.
- 2026-05-13: Official `v0.1.78` proved the public Sparkle hop itself worked
  quickly, but the self-hosted proof then failed steady-state because it asked
  runtime/local-web to be healthy before establishing a setup-ready baseline.
  `v0.1.80` restores setup before the Sparkle update, then proves the update
  preserves setup without another grant. The protected runner can use an
  explicit optional admin-authorization secret only for first setup and
  reinstall/delete-data proof prompts, not to hide a sudo requirement during
  normal updates.
- 2026-05-13: Official `v0.1.80` then exposed a harness ordering bug, not an
  update sudo bug: the runner was setup-ready, but the setup helper opened the
  old app, so the mandatory update completed before the update harness started
  and the harness found `0.1.80` where it expected the old version. `v0.1.81`
  checks setup readiness with the CLI before launching the old app. A
  setup-ready baseline must enter the update proof still on the old version.
