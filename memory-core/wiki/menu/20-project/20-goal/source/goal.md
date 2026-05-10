---
title: Goal
slug: goal
section: project
access: private
summary: "The professional app bar for 1Context: permission flows, update discipline, and the behaviors that must feel reliable before broad release."
status: published
last_updated: 2026-05-10
toc_enabled: true
talk_enabled: true
agent_view_enabled: true
copy_buttons_enabled: true
footer_enabled: true
---

# Goal

1Context should behave like a professional macOS app whose job is to remember
user-directed work. The app is not a shy utility hiding its needs. When a
feature needs setup or permission to do the thing the user asked for, 1Context
should say so, open the right flow, and finish the job.

## Permission Doctrine

Use permissions directly and honestly. 1Context creates memory from user action,
local work, and passive monitoring surfaces, so permissions are part of the
product contract. People installing this app are signing up for a system that
can see enough of their work to help them later.

This does not mean surprise access or vague permission prompts. It means:

- ask at the moment a blocked action needs the permission
- explain what 1Context will use the permission for
- retry or continue the original action after the permission is granted
- show a repair path when macOS says the grant is missing, stale, or attached
  to the wrong app copy
- never fail silently

## Blocked Action Rule

Any menu, CLI, hook, or local web action blocked by missing setup or permission
must launch the relevant setup or permissions flow.

Concrete example: the menu bar's Open Wiki action must not silently do nothing
when Local Wiki Access is missing. It should open setup, name the missing
grant, and then open the wiki once the setup check passes.

The same rule applies to future capture surfaces: screen recording,
accessibility, microphone, calendar, contacts, browser/MCP connectors, and
folder access. If a user asks 1Context to remember or act from that surface,
the app should request the permission needed for that surface at that moment.

## Sparkle Doctrine

Sparkle is the app update engine. Homebrew and the DMG are install channels;
Sparkle is how an installed app becomes the blessed current app.

Once a release is marked blessed, old code should stop surviving in the field.
The update path should be aggressive because 1Context is a passive monitoring
memory system: stale app code can keep observing, missing fixes, or producing
bad local state long after a release has fixed the issue.

## Mandatory Update Policy

Not every release needs to be mandatory. But when a release is marked
mandatory, 1Context should use Sparkle's automatic update path as strongly as
the platform allows.

The desired behavior:

- check for updates on launch and on a recurring background cadence
- surface pending updates in the menu until they are installed
- prefer automatic download, install, and relaunch for mandatory blessed builds
- allow mandatory updates to interrupt active use because 1Context is a passive
  memory system and cannot reliably infer when it is being used
- never show release notes, preflight explanation, or confirmation prompts for
  mandatory updates
- if an update fails, keep the old app useful whenever possible and show only:
  `Update failed. Please contact support at paul@haptica.ai.`
- report detailed update status in diagnostics and logs for support, not in
  user-facing copy

Daily checks are not enough if they leave users running old builds for days.
The product standard is: after a blessed mandatory release, there should be no
long-lived previous-version 1Context process continuing to monitor work.

Optional releases have the opposite attention policy. They may be discovered in
the background, but they should not interrupt active use, auto-open release
notes, or relaunch unless the user explicitly chooses the update from the menu.

## Product Standard

Professional 1Context behavior is direct:

- Open Wiki opens the wiki or opens the setup flow that makes opening possible.
- Refresh Wiki refreshes the wiki or opens the setup flow that makes refresh
  possible.
- Start starts remembering or opens the setup and permission flow required to
  start remembering.
- Check for Updates means the app can manually ask the signed Sparkle feed for
  current update state.
- Please Update or the policy-controlled update menu item means a pending update
  exists and the user can open the concise update flow.
- Mandatory updates install immediately when detected; the menu still exposes the
  update action until the installed app has actually moved.
- Settings always shows the currently running app version so support can confirm
  what code is alive without asking for terminal commands.

The app can be privacy-respecting and still be assertive. The professional
shape is explicit consent, immediate repair, visible state, and no silent
failure.

## Verification Targets

- Open Wiki with missing Local Wiki Access opens setup instead of failing
  silently.
- After setup is granted, the original Open Wiki action completes without the
  user needing to rediscover it.
- A mandatory Sparkle appcast item moves an older installed app to the blessed
  version through a deterministic update smoke, without release notes or a user
  confirmation prompt.
- An optional Sparkle appcast item stays silent in the background, keeps a menu
  update action visible until installed, and shows only concise policy copy when
  the user manually opens the update flow.
- Menu, CLI, and diagnostics agree on update availability, mandatory status,
  installed version, and required setup state.
- Settings shows the currently running version, and screenshot/AX evidence proves
  it matches the installed bundle version.
- Failed update attempts keep the old app usable whenever possible and show the
  controlled support message instead of raw Sparkle or installer details.
- A founder-controlled release policy manifest decides mandatory vs optional
  state and every user-facing update string before the release workflow runs.
- A repaired healthy `0.1.51` baseline can update through the real remote
  GitHub Sparkle feed to `0.1.53`.
- A remote `0.1.54` release marked mandatory can move the installed `0.1.53`
  app forward automatically, without a local appcast, hand-installed bundle, or
  user click on an Install and Relaunch button.
- A remote `0.1.55` release marked mandatory can repair the installed `0.1.54`
  setup flywheel: ready permissions are recognized without Check Again, setup
  starts the app/runtime instead of leaving the user to hunt, and the menu
  LaunchAgent is loaded during launch.

## Done When

- Blocked menu actions open the setup or permission flow that unblocks the
  original user intent.
- Setup completion resumes the original action when replaying that action is
  safe.
- Sparkle release artifacts can be marked mandatory in the release pipeline.
- The installed app checks aggressively, reports mandatory update state, and
  lets Sparkle automatically download and install eligible updates.
- Mandatory updates use Sparkle's automatic background install path immediately,
  even if that interrupts active use.
- Optional updates stay quiet in the background and require an explicit user
  click before install/relaunch.
- User-facing update text is controlled by policy, not generated release notes,
  Sparkle defaults, or raw error messages.
- The menu bar shows pending update state until installed, and Settings shows the
  currently running app version.
- Every claim above has a deterministic local proof: Swift tests, release-script
  checks, wiki render checks, and live local wiki verification.

## Checklist

### Baseline

- [x] `/goal` exists as a first-class wiki family with source, generated page,
  talk page, and route-table coverage.
- [x] `/goal` is linked from For You, Your Context, Projects, and Topics.
- [x] The live local wiki can serve `/goal` and `/goal.talk`.

### Permission Flow

- [x] Blocked Open Wiki records the original action, opens setup, and resumes
  Open Wiki after Local Wiki Access becomes ready.
- [x] Blocked Refresh Wiki records the original action, opens setup, and resumes
  Refresh Wiki after Local Wiki Access becomes ready.
- [x] Blocked Start opens setup and preserves the desired running intent without
  replaying a second button press.
- [x] Setup continuation has focused Swift tests for messages, replacement, and
  one-shot resume behavior.

### Sparkle Updates

- [x] Release app bundles enable automatic Sparkle checks, automatic eligible
  downloads/installs, and an aggressive scheduled check interval.
- [x] Appcast generation can mark a release mandatory through a release-time
  flag.
- [x] Menu and diagnostics can distinguish normal updates from mandatory
  updates.
- [x] Mandatory update state blocks new passive remembering starts and pauses a
  running passive monitor until Sparkle can install the update.
- [x] A local appcast smoke proves an older installed app moves to the blessed
  version and relaunches.

### Closed Loop

- [x] Swift setup and updater tests pass.
- [x] Wiki render and wiki regression tests pass.
- [x] Release scripts pass syntax or fixture validation for the new Sparkle
  flags.
- [x] `scripts/smoke-sparkle-local-appcast.sh` passed on this Mac, updating a
  fixture app from `0.1.51.900` to `0.1.51.901` and verifying the embedded CLI
  reports the new version.
- [x] The live local wiki route shows the current checklist.

### Remote Release Train

- [x] Baseline installed app is `0.1.51`.
- [x] Baseline remote Sparkle feed was GitHub Releases
  `latest/download/appcast.xml` advertising `0.1.51` before this trial.
- [x] Release workflow gap is identified: the current GitHub workflow still
  uploads an old tarball shape, while the real Sparkle path needs `1Context.dmg`,
  the versioned DMG, checksum, and `appcast.xml`.
- [x] The first `0.1.51 -> 0.1.53` attempt failed with a visible Sparkle error,
  captured as `dist/remote-update-evidence/0.1.51-to-0.1.53-error/`; logs
  showed the installed copy was missing `Sparkle.framework/Versions/B/Updater.app`.
- [x] The original `0.1.51` DMG was checked and contains the Sparkle helper
  layout, so the machine baseline was repaired by reinstalling that same signed
  `0.1.51` app from the cached DMG.
- [x] Publish a signed/notarized `0.1.53` GitHub release with the production
  Sparkle key and appcast assets.
- [x] Prove a healthy installed app moves from `0.1.51` to `0.1.53` through the
  remote Sparkle feed; this non-mandatory release downloaded, verified, and
  installed after the Sparkle `Install and Relaunch` confirmation.
- [x] Installed app is now `0.1.53` and setup status is healthy.
- [x] Code path updated so mandatory update actions use Sparkle's automatic
  background check/install flow instead of opening the manual update UI.
- [x] Publish a signed/notarized `0.1.54` GitHub release marked mandatory for
  automatic update from `0.1.53`.
- [x] Prove the installed `0.1.53` app moves to `0.1.54` through the remote
  Sparkle feed without local appcast fixtures and without clicking Install and
  Relaunch; `dist/remote-update-evidence/0.1.53-to-0.1.54-auto/` shows the
  version crossing from `0.1.53` to `0.1.54` with `windows=0`.
- [x] Capture release URLs, appcast snippets, installed app versions, CLI
  versions, setup status, and live wiki health as the final evidence bundle.

### 0.1.55 Setup Flywheel Repair

- [x] Reproduce the stale setup shape after restart/update: `0.1.54` can have
  Local Wiki Access and a working wiki while setup still invites manual
  Check Again.
- [x] Update setup so a visible setup window continuously rechecks readiness
  and completes setup automatically once required setup is ready.
- [x] Remove the footer Check Again control entirely when Local Wiki Access is
  ready, leaving Open Wiki as the relevant action.
- [x] Load the menu LaunchAgent during app launch, not just by writing the plist
  for a future login.
- [x] Prove the ready setup window with Accessibility and screenshot evidence;
  `dist/remote-update-evidence/setup-window-ready-no-check-again-after-patch/`
  shows Open Wiki plus granted permissions and no Check Again button.
- [x] Publish signed/notarized `0.1.55` as the mandatory remote update from
  `0.1.54`.
- [x] Prove the installed `0.1.54` app moves to `0.1.55` through the remote
  Sparkle feed without clicking Install and Relaunch.
- [x] Prove installed `0.1.55` is setup-ready, menu-loaded, and wiki healthy
  immediately after the update.
- [x] Prove installed `0.1.55` remains runtime-steady after restart and repeated
  status probes, with no fresh SIGTERM loop and no Sick/menu/CLI disagreement.
  Evidence: `dist/steady-state-evidence/20260507T071802Z/` crossed multiple
  menu refresh intervals with no new SIGTERM count and post-screenshot status
  still reported runtime, menu, setup, and wiki healthy.

### 0.1.56 Steady State and Design Cleanup

This goal repairs the gap exposed by `0.1.55`: update/setup can succeed while
the product still feels unprofessional if old release docs, stale workflow
artifacts, or a flapping passive runtime survive.

#### Done When

- `1context-cli status --debug`, menu state, LaunchAgent state, local wiki
  health, and desired runtime intent agree over a repeated steady-state probe.
- No new runtime SIGTERM loop appears during the steady-state probe.
- The GitHub release workflow publishes the same Sparkle/DMG assets used by the
  real release train: versioned DMG, `1Context.dmg`, checksum, and `appcast.xml`.
- Stale docs no longer present `v0.1.51` or tarball releases as the current
  release truth.
- `/goal` itself is rendered and visible in the live local wiki with this
  checklist.

#### Checklist

##### Baseline Truth

- [x] Installed app reports `0.1.55`.
  Evidence: `/Applications/1Context.app/Contents/MacOS/1context-cli version`.
- [x] Local Wiki Access is granted and the wiki is reachable.
  Evidence: `1context-cli permissions` reports Required Setup ready and Local
  Wiki reachable.
- [x] Live appcast advertises `0.1.55` as the current Sparkle release.
  Evidence: GitHub Releases `latest/download/appcast.xml`.
- [x] Release workflow drift is identified: `.github/workflows/release.yml`
  still uploads the old tarball artifact shape instead of Sparkle DMG/appcast
  assets.
- [x] Documentation drift is identified: professional-app docs still describe
  `v0.1.51` as the current baseline.
- [x] Runtime-health ambiguity is identified: a May 7 probe saw Local Web OK
  while runtime status returned socket no response.

##### Runtime Steady State

- [x] Add a reusable steady-state verifier that records CLI status, update
  state, permissions, LaunchAgent state, and runtime log deltas.
- [x] Run the verifier against the installed app long enough to cross multiple
  menu refresh intervals.
- [x] Confirm the current installed app does not send fresh SIGTERM during the
  verifier window while desired state is `running` and no mandatory update is
  pending.
- [x] Prove the menu is visible and agrees with CLI/runtime state after the
  verifier passes.

##### Release Workflow Cleanup

- [x] Update the GitHub release workflow to build notarized DMG plus signed
  Sparkle appcast assets.
- [x] Add workflow inputs for mandatory release metadata, including critical and
  minimum autoupdate versions.
- [x] Stop publishing tarball-only assets from the release workflow.
- [x] Validate the workflow syntax and artifact collection path locally.

##### Documentation Cleanup

- [x] Refresh professional-app docs so `0.1.55` is historical current truth and
  `0.1.56` is the active steady-state cleanup goal.
- [x] Keep older `0.1.50` to `0.1.55` evidence as history, not current baseline
  language.
- [x] Confirm stale `Install and Relaunch` and manual `Check Again` language is
  used only when describing old evidence or fixed bugs.

##### Live Wiki Proof

- [x] Render the updated `goal` wiki family from source.
- [x] Publish or patch the live local wiki so `/goal` shows this checklist.
- [x] Verify `/goal#0.1.56_Steady_State_and_Design_Cleanup` in the local
  browser or with a saved HTML/status artifact.

### 0.1.56 to 0.1.65 Finish-It Release Flywheel

This is the full professional-app finish goal. The point is not to worship
version numbers; the point is to keep moving the installed app through real
signed releases until setup, update, permissions, lifecycle, uninstall, and
diagnostics are boringly reliable. It is acceptable to use every version from
`0.1.56` through `0.1.65` as a proof step if that is what the work needs.

Each release in this train must answer four questions:

- What user-visible or operator-visible behavior improved?
- What local or CI harness proves it?
- Did the installed app update through the remote Sparkle feed?
- Did the app remain healthy after relaunch, restart, and repeated probes?

#### Done When

- The currently installed app reaches the final blessed version in this train
  through the remote Sparkle feed, starting from the installed `0.1.55` baseline.
- At least one mandatory remote update is proven after `0.1.55`, with no
  Install and Relaunch click and no local appcast fixture.
- Every release artifact shape is the product shape: signed/notarized app,
  signed/notarized DMG, `1Context.dmg`, checksum, and signed `appcast.xml`.
- Runtime/menu/CLI/local wiki/setup state agree after every update and after at
  least one machine restart or login-style LaunchAgent recovery.
- Uninstall cleanup is proven against the app-owned helper, launch agents,
  trusted local CA, managed hooks, logs/cache, and optional data deletion.
- Permission flywheel behavior is proven for each shipped permission-dependent
  surface: blocked action opens setup/permission flow, granted state is noticed
  automatically, and the original action continues when safe.
- Failed update and stale helper cases leave the old app usable with a clear
  user-facing support message and detailed operator logs.
- The release docs, `/goal`, GitHub workflow, release policy manifest, public
  release notes, appcast, and live local wiki all agree on what the current
  blessed version is and whether it is mandatory or optional.
- Release notes never appear in the updater window unless the founder-controlled
  policy explicitly allows it.
- Menu bar evidence proves `Check for Updates` when current, a pending update
  action when an update is available, and Settings shows the currently running
  version number.
- New findings discovered during closed-loop testing are immediately folded
  back into `/goal` as either checked evidence, an unchecked repair item, or an
  explicit deferred release-train item.

#### Version Train

##### 0.1.56 Release Workflow and Steady State

- [x] Add `/goal` checklist for steady-state cleanup and release workflow drift.
- [x] Add reusable installed-app steady-state verifier.
- [x] Prove installed `0.1.55` remains healthy over repeated probes.
- [x] Update GitHub release workflow to publish DMG/appcast assets instead of
  tarball-only artifacts.
- [x] Bump source version and release notes to `0.1.56`.
- [x] Build/sign/notarize `0.1.56`.
  Evidence: `dist/1Context-0.1.56-macos-arm64.dmg` and `dist/1Context.app`
  passed Developer ID signing, Apple notarization, stapling, and DMG validation.
- [x] Publish `v0.1.56` with `1Context.dmg`, versioned DMG, checksum, and
  `appcast.xml`.
- [x] Prove remote Sparkle update from installed `0.1.55` to `0.1.56`.
  Evidence: `dist/remote-update-evidence/0.1.55-to-0.1.56-auto/` shows the
  installed app crossing to `0.1.56` through the GitHub Sparkle feed.
  Note: this was not a no-click automatic update; `0.1.55` still presented
  Sparkle's Install Update and Install and Relaunch prompts.
- [x] Prove `0.1.56` steady state after update with the reusable verifier.
  Evidence: `dist/steady-state-evidence/0.1.56-after-update/`.

##### 0.1.57 Release Automation Proof

- [x] Replace the remaining Sparkle standard interactive prompt path for
  mandatory updates with a no-click app-owned automatic path.
  Evidence: `SparkleUpdateController` now owns `SPUUpdater` directly with an
  app-managed user driver; `swift test --package-path macos` passed including
  the new mandatory no-click policy tests.
- [x] Prove a mandatory update can download, install, and relaunch without
  Install Update or Install and Relaunch prompts.
  Closed by later evidence: the fully fixed path was proved in `0.1.58`, where
  `dist/remote-update-evidence/0.1.57-to-0.1.58-no-click/` shows automatic
  install/relaunch and version convergence with no captured Install Update or
  Install and Relaunch prompt.
- [x] Bump source version and release notes to `0.1.57`.
- [x] Build/sign/notarize `0.1.57`.
  Evidence: `dist/1Context-0.1.57-macos-arm64.dmg` and `dist/1Context.app`
  passed Developer ID signing, Apple notarization, stapling, DMG validation,
  and Gatekeeper assessment.
- [x] Make `/goal` updates durable while an older installed app is still
  republishing its bundled wiki. Repo render alone is not enough if live
  `wiki refresh` can overwrite the served page with older bundled content.
  Evidence: `WikiSitePublisher` now refresh-renders every wiki family instead
  of only `for-you`; `scripts/publish-goal-to-installed-wiki.sh` renders the
  repo goal, patches the installed app-support memory core, refreshes the
  installed app from that patched core, and verifies the live
  `https://wiki.1context.localhost/goal` marker. The focused
  `OneContextMemoryCoreTests` suite covers the all-family refresh behavior, and
  the script passed against this installed `0.1.60` app.
- [x] Run the updated GitHub release workflow or equivalent scripted release
  path end to end for a real release.
  Evidence: equivalent scripted production releases have been run for
  `0.1.58`, `0.1.59`, and `0.1.60`, each producing signed/notarized DMGs,
  Sparkle appcasts, checksums, GitHub release assets, and installed-app remote
  update proof. The actual GitHub Actions workflow can still be rehearsed later
  under the `0.1.64` release-train rehearsal item.
- [x] Prove workflow-provided mandatory metadata appears correctly in the
  generated appcast.
  Evidence: `dist/appcast.xml` advertises `0.1.57`, includes
  `sparkle:minimumAutoupdateVersion` of `0.1.56`, marks
  `sparkle:criticalUpdate` for `0.1.57`, points at the GitHub `v0.1.57` DMG,
  and includes an EdDSA signature.
- [x] Prove `0.1.56 -> 0.1.57` bridge update through the remote Sparkle feed.
  Note: this hop is initiated by already-installed `0.1.56`, so it may still
  show the old Sparkle click-through. That is acceptable only as the bridge to
  get the fixed `0.1.57` updater onto the machine.
  Evidence: `dist/remote-update-evidence/0.1.56-to-0.1.57-bridge/` shows
  installed version convergence to `0.1.57`, no captured Install/Relaunch prompt
  in accessibility window text, and healthy signed app status after the update.
  Nuance: the old updater had a stale `SULastCheckTime` and an orphaned menu
  process, so the bridge required resetting that timestamp and relaunching the
  old menu app before Sparkle moved.
- [x] Make local production packaging collect the same release asset shape as
  GitHub: versioned DMG, `1Context.dmg`, checksums, and `appcast.xml`.
  Evidence: `scripts/package-macos-release.sh` now copies the generated
  Sparkle appcast and writes both checksum files after packaging.
- [x] Add a release artifact audit script if upload still requires manual asset
  inspection.
  Evidence: `scripts/audit-github-release-assets.sh` checks a GitHub release is
  not draft/prerelease, requires `1Context.dmg`, the versioned DMG, checksum
  files, and `appcast.xml`, then validates the appcast against release policy;
  it passed against `v0.1.60`.

##### 0.1.58 Uninstall and Residue Proof

- [x] Prove `0.1.57 -> 0.1.58` mandatory update is truly no-click with the
  fixed app-owned Sparkle driver: no Install Update prompt, no Install and
  Relaunch prompt, automatic install, relaunch, and version convergence.
  Evidence: `dist/remote-update-evidence/0.1.57-to-0.1.58-no-click/` shows
  `plist=0.1.57 cli=0.1.57` moving to `plist=0.1.58 cli=0.1.58` at watcher
  iteration 2 with no captured Install/Relaunch prompt file, followed by
  healthy CLI status, local web health, codesign verification, and Gatekeeper
  acceptance.
- [x] Bump source version and release notes to `0.1.58`.
- [x] Build/sign/notarize `0.1.58`.
  Evidence: `dist/1Context-0.1.58-macos-arm64.dmg` and `dist/1Context.app`
  passed Developer ID signing, Apple notarization, stapling, DMG validation,
  Gatekeeper assessment, and full Swift tests.
- [x] Publish `v0.1.58` with `1Context.dmg`, versioned DMG, checksum, and
  `appcast.xml`.
  Evidence: GitHub release `v0.1.58` contains `1Context-0.1.58-macos-arm64.dmg`,
  checksum, `1Context.dmg`, and `appcast.xml`; the live latest appcast advertises
  `0.1.58` as mandatory from installed `0.1.57`.
- [x] Prove `0.1.58` steady state after no-click update with the reusable
  verifier.
  Evidence: `dist/steady-state-evidence/0.1.58-after-no-click-update-settled/`
  passed 75 seconds, 11 probes, and no new runtime SIGTERMs.
- [x] Harden the immediate post-update settling window. The first steady-state
  run after the no-click update observed a short runtime socket gap at
  `dist/steady-state-evidence/0.1.58-after-no-click-update/`, even though the
  app recovered and the settled verifier passed.
  Closed by later evidence: the settled `0.1.58` verifier passed, then
  `0.1.59` and `0.1.60` both passed post-update steady-state verification. If a
  settling gap reappears, it belongs in the restart/login rehearsal work rather
  than as a stale `0.1.58` blocker.
- [x] Move full uninstall/reinstall residue proof out of the historical
  `0.1.58` bucket and into the active acceptance work.
  Evidence: the still-required checks are now tracked under
  `0.1.63 Permission Flywheel and Clean-Machine Acceptance` and the final
  `0.1.65` blessed-app acceptance, where they can be tested against the current
  app instead of an old release.
- [x] Prove remote Sparkle update into `0.1.58`.
  Evidence: `dist/remote-update-evidence/0.1.57-to-0.1.58-no-click/` recorded
  installed `0.1.57` moving to installed `0.1.58` without clicking an update
  confirmation button.

##### 0.1.59 Update Policy Control Plane

- [x] Draft `docs/update_policy.html` with the founder-controlled update policy,
  current prompt screenshot, mandatory/optional behavior, menu bar behavior,
  failure copy, and default post-install message.
  Evidence: `docs/update_policy.html` and
  `docs/assets/update-policy/current-update-prompt.png`.
- [x] Add a release policy manifest schema that declares version, update class,
  approved-by, reason, minimum autoupdate version, release-notes visibility,
  optional prompt copy, failure copy, and post-install message copy.
  Evidence: `release/update-policy.toml`, `release/update-policy.schema.json`,
  and `scripts/update-policy.py`.
- [x] Move release-class control from ad hoc workflow inputs to the committed
  policy manifest; the workflow may execute policy but must not invent policy.
  Evidence: `.github/workflows/release.yml` no longer exposes mandatory/optional
  workflow inputs; `scripts/package-macos-release.sh` exports policy from the
  committed manifest.
- [x] Add validation that `VERSION`, tag, manifest, appcast, GitHub release, and
  generated public release notes agree.
  Evidence: `scripts/check-version-consistency.sh`, `scripts/check-update-policy.sh`,
  workflow tag checks, workflow GitHub release asset checks, and appcast policy
  validation.
- [x] Remove embedded builder-journal release notes from Sparkle updater UI by
  default; prove the appcast/update path cannot show release notes unless policy
  explicitly allows it.
  Evidence: signed production packaging generated
  `dist/sparkle-updates/appcast.xml` with no `<description>` and
  `scripts/test-update-policy.sh` rejects appcast descriptions when release
  notes are hidden by policy.
- [x] Replace raw user-facing Sparkle failure text with the controlled message:
  `Update failed. Please contact support at paul@haptica.ai.`
  Evidence: `SparkleUpdateController` now uses `UpdateUserFacingPolicy`; the
  signed bundle `Info.plist` contains `Update failed.` and
  `Please contact support at paul@haptica.ai.`
- [x] Move real failed-update proof out of the `0.1.59` policy-plumbing bucket
  and into `0.1.62 Failed Update and Supportability Proof`.
  Partial evidence: `MandatoryUpdateRuntimePolicy` no longer pauses passive
  remembering for mandatory updates and Swift tests cover that behavior. The
  remaining broken-appcast, missing-asset, bad-signature, and interrupted
  download checks stay open under `0.1.62`.
- [x] Add policy-controlled post-install message plumbing with default title
  `1Context Improved!`, disabled by default unless the manifest enables it.
  Evidence: `UpdateUserFacingPolicy` parses post-install copy from `Info.plist`;
  signed production packaging proves `OneContextUpdatePostInstallMessageEnabled`
  is `false` by default.
- [x] Produce signed/notarized local release artifacts for the policy-control
  build.
  Evidence: `scripts/package-macos-production-release.sh` produced
  `dist/1Context-0.1.59-macos-arm64.dmg`; Apple notarization accepted and
  stapled both `dist/1Context.app` and the DMG; `codesign --verify`,
  `xcrun stapler validate`, DMG validation, `scripts/test.sh`, full Swift tests,
  and `scripts/check-update-policy.sh --appcast dist/sparkle-updates/appcast.xml`
  passed.
- [x] Add reusable remote Sparkle update proof harness.
  Evidence: `scripts/prove-remote-sparkle-update.sh` fetches the live appcast,
  validates policy/appcast agreement, records versions, status, osascript window
  text, screenshots, and watches the installed bundle and embedded CLI cross to
  the expected version.
- [x] Prove remote Sparkle update into `0.1.59`.
  Evidence: GitHub release `v0.1.59` contains the signed/notarized DMG,
  versioned DMG, checksum files, and `appcast.xml`; the live latest appcast
  advertises `0.1.59` as a mandatory update from `0.1.58` with no release notes;
  `dist/remote-update-evidence/0.1.58-to-0.1.59-remote/` shows installed
  `plist=0.1.58 cli=0.1.58` moving to `plist=0.1.59 cli=0.1.59`.
- [x] Prove `0.1.59` steady state after remote update.
  Evidence: `dist/steady-state-evidence/0.1.59-after-remote-update/` passed 35
  seconds, 6 probes, runtime health OK, menu bar running, setup ready, and no
  new runtime SIGTERMs.

##### 0.1.60 Optional Update UX Proof

- [x] Publish `0.1.60` as an optional release through the policy manifest.
  Evidence: `release/update-policy.toml` marks `0.1.60` as `optional`, the
  signed/notarized GitHub release `v0.1.60` contains the versioned DMG,
  `1Context.dmg`, checksums, and `appcast.xml`, and the live appcast contains no
  `sparkle:criticalUpdate`, no `sparkle:minimumAutoupdateVersion`, and no
  `<description>` release notes.
- [x] Prove background optional update discovery stays silent: no modal, no
  release notes, no automatic relaunch.
  Evidence:
  `dist/remote-update-evidence/0.1.59-to-0.1.60-optional-remote-rerun/`
  records installed `plist=0.1.59 cli=0.1.59` through the quiet background
  window before any user confirmation.
- [x] Prove the menu bar keeps a pending update action visible until the optional
  update is installed.
  Evidence:
  `dist/remote-update-evidence/0.1.59-to-0.1.60-optional-remote-rerun/menu-after-background-discovery.txt`
  shows Settings `Version 0.1.59` and the pending `Please Update` action.
- [x] Prove clicking the menu update action shows only concise policy copy:
  `A 1Context update is ready.`
  Evidence:
  `dist/remote-update-evidence/0.1.59-to-0.1.60-optional-remote-rerun/accessibility-optional-prompt.txt`
  contains `Update 1Context?`, `A 1Context update is ready.`, `Later`, and
  `Update`, with no release notes or installer explanation.
- [x] Prove the optional update installs and relaunches only after the user
  explicitly clicks Update.
  Evidence:
  `dist/remote-update-evidence/0.1.59-to-0.1.60-optional-remote-rerun/watch.log`
  shows the installed bundle and embedded CLI crossing from `0.1.59` to
  `0.1.60` after the proof harness clicked Update.
- [x] Prove Settings shows the currently running version before and after the
  optional update.
  Evidence: the optional proof captures Settings `Version 0.1.59` before
  install; after install, osascript menu capture shows Settings `Version 0.1.60`
  and the menu returned to `Check for Updates`.
- [x] Prove remote Sparkle update into `0.1.60`.
  Evidence:
  `dist/remote-update-evidence/0.1.59-to-0.1.60-optional-remote-rerun/result.txt`
  is `result=passed`, with `old_version=0.1.59` and `new_version=0.1.60`.
- [x] Prove `0.1.60` steady state after the optional update.
  Evidence: `dist/steady-state-evidence/0.1.60-after-optional-update/` passed
  35 seconds, 6 probes, installed `version=0.1.60`, runtime health, setup
  readiness, menu running state, and local wiki health.

##### 0.1.61 Mandatory Immediate Update Proof

- [x] Add an optional self-hosted Mac update CI gate for release-worthy update
  hops.
  Evidence: `.github/workflows/self-hosted-mac-update-proof.yml` is
  dispatch-only, runs only on the `onecontext-update-runner` self-hosted Mac
  label, requires the protected `onecontext-update-runner` environment, rejects
  untrusted refs, installs version N, proves the staged Sparkle update to N+1,
  runs steady-state verification, and uploads evidence. The runner docs and
  scripts require the installed N app's `SUFeedURL` to match the proof appcast
  so staging proofs cannot accidentally exercise the public latest feed.
- [x] Prove the self-hosted runner can execute a staged mandatory update hop.
  Evidence: GitHub Actions run
  `https://github.com/hapticasensorics/1context/actions/runs/25617081477`
  passed at `1c985c9`, artifact
  `self-hosted-mac-update-proof-0.1.60-to-0.1.61` shows `0.1.60 -> 0.1.61`
  through a mandatory staged appcast, `sparkle:criticalUpdate`, matching
  `minimumAutoupdateVersion=0.1.60`, and 120 seconds of steady-state probes
  after install.
- [x] Harden the remote update proof harness so mandatory proofs fail if a
  user-facing update prompt, installer explanation, or release-note text appears
  during the automatic update window.
  Evidence: `scripts/prove-remote-sparkle-update.sh` now captures
  Accessibility text during mandatory watch iterations and optional background
  discovery, then fails on `Update 1Context?`, `Install Update`,
  `Install and Relaunch`, release notes, installer explanation, or relaunch
  explanation text before the version hop is accepted.
- [x] Prove the hardened harness locally on this Mac against the staged
  mandatory `0.1.60 -> 0.1.61` assets.
  Evidence:
  `dist/self-hosted-update-proof/local-0.1.60-to-0.1.61-strict-20260510T021556Z/result.txt`
  is `result=passed`; `update-proof/watch.log` crosses from `0.1.60` to
  `0.1.61`; `update-proof/accessibility-1.txt` and
  `update-proof/accessibility-2.txt` passed the no-prompt/no-release-notes
  assertion; `steady-state/summary.txt` passed 60 seconds and 10 probes after
  install.
- [x] Promote the update-flow lockdown into CI so regressions fail before a
  release cut.
  Evidence: CI run
  `https://github.com/hapticasensorics/1context/actions/runs/25617804524`
  passed at `bdf470f`; it runs Swift tests, wiki/app tests,
  `scripts/test-upgrade-paths.sh`, package creation, and
  `scripts/test-launch-agent-package.sh`. The new upgrade-path test checks the
  destructive proof guard, update-class validation, mandatory no-UI assertions,
  and the staging-feed match guard. The package smoke checks the signed app
  shape, LaunchDaemon plist, required executables, and bundled generated
  `/goal` assets.
- [x] Restore this Mac to the current public release after the staged proof.
  Evidence: `/Applications/1Context.app` was reinstalled from GitHub release
  `v0.1.60`, reports version `0.1.60`, points back at
  `https://github.com/hapticasensorics/1context/releases/latest/download/appcast.xml`,
  and reports Health OK, menu running, and setup ready.
- [x] Publish `0.1.61` as a mandatory release through the policy manifest.
  Evidence: `release/update-policy.toml` marks `0.1.61` as `mandatory`, sets
  `minimum_autoupdate_version = "0.1.60"`, disables updater release notes, and
  the public GitHub release `v0.1.61` contains the notarized versioned DMG,
  `1Context.dmg`, both checksum files, and `appcast.xml`. The release asset
  audit passed against `https://github.com/hapticasensorics/1context/releases/tag/v0.1.61`.
- [x] Prove mandatory update detection can interrupt use and immediately
  download, install, and relaunch.
  Evidence:
  `dist/remote-update-evidence/0.1.60-to-0.1.61-mandatory-public/watch.log`
  shows the installed app crossing from `plist=0.1.60 cli=0.1.60` at
  `2026-05-10T03:03:07Z` to `plist=0.1.61 cli=0.1.61` at
  `2026-05-10T03:03:18Z` after the harness relaunched the running installed
  app to trigger the launch update check.
- [x] Prove the public `0.1.61` mandatory release repeats the no-release-notes,
  no-preflight, no-confirmation behavior through GitHub release assets.
  Evidence: the live GitHub appcast advertises `sparkle:criticalUpdate`,
  `sparkle:minimumAutoupdateVersion` of `0.1.60`, and `sparkle:version` of
  `0.1.61`, with no `<description>` release notes. The mandatory public proof's
  Accessibility captures passed the harness assertion against
  `Update 1Context?`, `Install Update`, `Install and Relaunch`, release-note
  text, installer explanation, and relaunch explanation text.
- [x] Prove the mandatory menu state does not get stuck after the installed app
  moves.
  Evidence: because the public mandatory hop installed immediately, there was no
  durable pre-install pending menu interval to inspect. After install,
  `dist/remote-update-evidence/0.1.60-to-0.1.61-mandatory-public/menu-after-install.txt`
  shows Settings `Version 0.1.61` and the top-level menu back at
  `Check for Updates`.
- [x] Prove the default post-install message remains hidden when disabled by
  policy.
  Evidence: `release/update-policy.toml` keeps
  `ui.post_install_message.enabled = false`, and the public mandatory proof's
  Accessibility and window captures contain no `1Context Improved!` or update
  completion message after the app reaches `0.1.61`.
- [x] Prove a policy-enabled post-install message can show exactly
  `1Context Improved!` with founder-provided body copy in a fixture or local
  appcast proof.
  Evidence: `PostInstallUpdateMessageGate` now drives a launch-based one-shot
  post-install message when policy enables it and the app detects a version
  change. `NativeUpdaterTests` cover default-hidden, first-launch-hidden, and
  show-once behavior. The local Sparkle fixture
  `dist/sparkle-local-smoke/post-install-message-0.1.61/evidence/` passed with
  `result=passed`; `post-install-message.txt` records
  `title=1Context Improved!` and
  `body=Founder-controlled fixture body for 0.1.61.901.`, while
  `post-install-accessibility.txt` and `post-install-desktop.png` prove the
  real macOS alert showed that exact controlled copy.
- [x] Prove remote Sparkle update into `0.1.61`.
  Evidence:
  `dist/remote-update-evidence/0.1.60-to-0.1.61-mandatory-public/result.txt`
  is `result=passed`, with `old_version=0.1.60` and `new_version=0.1.61`.
- [x] Prove `0.1.61` steady state after the mandatory update.
  Evidence: `dist/steady-state-evidence/0.1.61-after-mandatory-public/`
  passed 60 seconds, 10 probes, installed `version=0.1.61`, runtime health,
  setup readiness, menu running state, and local wiki health.

##### 0.1.62 Failed Update and Supportability Proof

- [x] Add a missing-asset failed-update smoke for a mandatory Sparkle update.
  Evidence: `scripts/smoke-sparkle-local-appcast.sh` now supports
  `ONECONTEXT_SPARKLE_SMOKE_FAILURE_CASE=missing_asset`, removes the advertised
  mandatory DMG after appcast generation, and verifies the failed update leaves
  the old fixture app installed. The proof at
  `dist/sparkle-local-smoke/missing-asset-failure-0.1.61/evidence/` passed with
  `failure_case=missing_asset`, `attempted_new_version=0.1.61.901`, and
  `installed_cli_version=0.1.61.900`.
- [x] Prove the missing-asset user-facing failed-update path shows only:
  `Update failed. Please contact support at paul@haptica.ai.`
  Evidence: `failure-message.txt` records only the controlled support title and
  body, `failure-accessibility.txt` contains those strings plus `OK`, and the
  harness fails if the alert exposes technical details such as 404, download,
  signature, Sparkle, installer, or relaunch text.
- [x] Prove the missing-asset failed update retains an internal failure reason
  for support.
  Evidence:
  `dist/sparkle-local-smoke/missing-asset-failure-0.1.61/evidence/http-server.log`
  preserves the real `404` for
  `1Context-0.1.61.901-macos-arm64.dmg`, while the user-facing alert remains
  non-technical.
- [x] Prove the missing-asset failed update leaves the old app launchable.
  Evidence:
  `dist/sparkle-local-smoke/missing-asset-failure-0.1.61/evidence/result.txt`
  records `installed_cli_version=0.1.61.900` after the failed mandatory update
  attempt.
- [x] Add a bad-signature failed-update smoke for a mandatory Sparkle update.
  Evidence: `scripts/smoke-sparkle-local-appcast.sh` supports
  `ONECONTEXT_SPARKLE_SMOKE_FAILURE_CASE=bad_signature`, corrupts the
  downloaded DMG after appcast signing, and verifies
  `SUVerifyUpdateBeforeExtraction` is enabled so the bad archive is rejected
  before the host app disappears. The proof at
  `dist/sparkle-local-smoke/bad-signature-failure-0.1.61/evidence/` passed with
  `failure_case=bad_signature`, `attempted_new_version=0.1.61.901`, and
  `installed_cli_version=0.1.61.900`.
- [x] Prove the bad-signature user-facing failed-update path shows only:
  `Update failed. Please contact support at paul@haptica.ai.`
  Evidence: `failure-message.txt` records only the controlled support title and
  body, `failure-accessibility.txt` contains those strings plus `OK`, and the
  harness fails if the alert exposes technical details such as 404, download,
  signature, Sparkle, installer, or relaunch text.
- [x] Prove the bad-signature failed update retains operator evidence without
  teaching the user update internals.
  Evidence: `signature-corruption.txt` records the deliberate post-signing DMG
  SHA-256 change, and `http-server.log` records a successful appcast and DMG
  fetch while the user-facing alert remains non-technical.
- [x] Prove release app bundles require Sparkle to verify downloads before
  extraction.
  Evidence: `scripts/build-macos-app.sh` writes
  `SUVerifyUpdateBeforeExtraction = true`; the local Sparkle smoke and package
  smoke assert the key is present in the built app.
- [x] Add a broken-appcast failed-update retry smoke for a mandatory Sparkle
  update.
  Evidence: `scripts/smoke-sparkle-local-appcast.sh` supports
  `ONECONTEXT_SPARKLE_SMOKE_FAILURE_CASE=broken_appcast`, corrupts the served
  appcast XML after first generating and validating a mandatory appcast, then
  can repair the feed while the controlled failure alert is open and click
  `Try Again`.
- [x] Prove the broken-appcast user-facing failed-update path stays simple and
  includes a manual retry button.
  Evidence:
  `dist/sparkle-local-smoke/broken-appcast-retry-0.1.61/evidence/failure-message.txt`
  records `title=Update failed.`, the support body, `buttons=Try Again, OK`,
  and `action=try_again`; `failure-accessibility.txt` contains only the
  controlled title/body and those two buttons.
- [x] Prove clicking `Try Again` after a failed appcast check re-runs the update
  check after the failed Sparkle cycle and can complete the update.
  Evidence:
  `dist/sparkle-local-smoke/broken-appcast-retry-0.1.61/evidence/result.txt`
  records `retry_after_failure=1` and `retried_cli_version=0.1.61.901` after
  starting from `old_version=0.1.61.900`.
- [x] Prove the broken-appcast failed update retains operator evidence without
  exposing it to the user.
  Evidence: `appcast-corruption.txt` records the deliberate XML corruption,
  `retry-repair.txt` records the feed repair before `Try Again`, and
  `http-server.log` records the failed-cycle appcast fetches followed by the
  successful retry appcast and DMG fetch.
- [x] Keep the normal menu `Check for Updates` path separate from failed-update
  retry behavior.
  Evidence: `SparkleUpdateController` retries the same check mode only after
  consuming a failure-window `Try Again` request, while
  `SparkleUpdateControllerTests.testManualChecksCanAskForNonMandatoryUpdates`
  still covers the manual update path and `scripts/test.sh` passes.
- [x] Extend failed-update smoke to an interrupted download case where feasible.
  Evidence: `scripts/smoke-sparkle-local-appcast.sh` supports
  `ONECONTEXT_SPARKLE_SMOKE_FAILURE_CASE=interrupted_download` with a local
  HTTP server that advertises the signed DMG's real byte length, sends only a
  small prefix, and closes the connection. The proof at
  `dist/sparkle-local-smoke/interrupted-download-failure-0.1.61/evidence/result.txt`
  passed with `failure_case=interrupted_download`,
  `attempted_new_version=0.1.61.901`, and
  `installed_cli_version=0.1.61.900`.
- [x] Prove every remaining user-facing failed-update path shows only:
  `Update failed. Please contact support at paul@haptica.ai.`
  Evidence:
  `dist/sparkle-local-smoke/missing-asset-failure-0.1.61/evidence/failure-message.txt`,
  `dist/sparkle-local-smoke/bad-signature-failure-0.1.61/evidence/failure-message.txt`,
  `dist/sparkle-local-smoke/broken-appcast-retry-0.1.61/evidence/failure-message.txt`,
  and
  `dist/sparkle-local-smoke/interrupted-download-failure-0.1.61/evidence/failure-message.txt`
  all record the controlled title/body and do not expose Sparkle/download/signature
  details in the user-facing accessibility dump.
- [x] Prove internal diagnostics/logs retain the real failure reason for support.
  Evidence: missing-asset evidence keeps the corrupted appcast and HTTP 404
  server log, bad-signature evidence keeps `signature-corruption.txt`, and
  broken-appcast evidence keeps `appcast-corruption.txt`, `retry-repair.txt`,
  and `http-server.log`; interrupted-download evidence keeps
  `download-interruption.txt` and `http-server.log` with declared/sent byte
  counts.
- [x] Prove failed update leaves the old app launchable and remembering continues
  unless the app is already in the short install/relaunch phase.
  Evidence:
  `dist/sparkle-local-smoke/runtime-survives-interrupted-download-0.1.61/evidence/result.txt`
  records `installed_cli_version=0.1.61.900`, `runtime_survived=1`,
  `runtime_pid_before=64775`, `runtime_pid_after=64775`,
  `desired_state=running`, and the short smoke socket path after the interrupted
  mandatory update failed. `runtime-status-before.txt` and
  `runtime-status-after.txt` both show `1Context is running.`, `Health: OK`,
  and `Socket: responding` for version `0.1.61.900`.
- [x] Prove stale local HTTPS helper is detected and repaired after app
  replacement.
  Evidence:
  `LocalWebTests.testLocalHTTPSSetupAppReplacementDetectsStaleProxyAndRepairRestoresReadiness`
  models an app replacement where the bundled proxy SHA is
  `NEW_PROXY_FROM_REPLACED_APP` but the installed helper is still
  `OLD_PROXY_FROM_PREVIOUS_APP`; readiness blocks with `Proxy current: no` and
  the Setup next action. The same test then models setup repair installing the
  new proxy SHA and proves readiness returns to complete with
  `Proxy current: yes`.
- [ ] Add one command or evidence bundle that collects version, appcast, Sparkle
  defaults, LaunchAgent state, helper state, setup state, runtime health, local
  wiki health, and recent logs.
- [ ] Add a reusable GUI evidence harness for app/menu/Sparkle windows using
  osascript, accessibility window text, and screenshots; include a fallback path
  when the in-app browser automation control surface is unavailable.
  Partial evidence: `scripts/prove-remote-sparkle-update.sh` now captures
  osascript window text plus desktop screenshots for remote update proofs; this
  item stays open until failed-update and optional-update windows use the same
  harness.
- [ ] Redact sensitive paths/content by default while preserving operator-useful
  state.
- [ ] Prove diagnostics can distinguish healthy, needs setup, needs update,
  failed update, and stopped-by-user states.
- [ ] Prove remote Sparkle update into `0.1.62`.

##### 0.1.63 Permission Flywheel and Clean-Machine Acceptance

- [ ] Define the permission readiness model for future Screen Recording and
  Accessibility surfaces without making the CLI the accidental permission owner.
- [ ] Add native setup rows for permission-dependent shipped surfaces only.
- [ ] Prove blocked action opens the relevant permission/setup flow.
- [ ] Prove granted state is detected automatically without manual Check Again.
- [ ] Run clean-machine checklist from DMG install through setup, wiki open,
  update, relaunch, and uninstall cleanup.
- [ ] Add full uninstall smoke that can inspect helper, LaunchAgents, local CA,
  managed hooks, logs/cache, and optional data deletion.
- [ ] Run uninstall without `--delete-data` and prove user wiki content remains.
- [ ] Run uninstall with delete-data in a controlled fixture account or fixture
  path and prove only approved paths are removed.
- [ ] Reinstall from DMG and prove setup/update still works after residue
  cleanup.
- [ ] Capture screenshots and command artifacts for every consent or native UI
  step.
- [ ] Prove Homebrew remains an install channel and Sparkle remains the update
  engine.
- [ ] Prove remote Sparkle update into `0.1.63`.

##### 0.1.64 Restart, Login, and Release Train Rehearsal

- [ ] Prove menu LaunchAgent and runtime LaunchAgent recover after app relaunch.
- [ ] Prove machine-restart or login-style recovery on this Mac with screenshots
  and CLI status artifacts.
- [ ] Prove desired state `running` survives update/restart unless the user
  explicitly stops remembering.
- [ ] Run a full release rehearsal where the only accepted proof is installed
  app behavior after remote update.
- [ ] Configure the release workflow with signing/notarization secrets or move
  production signing to the self-hosted Mac runner, so future blessed releases
  do not require a local manual upload fallback.
- [ ] Validate `/goal`, release docs, release policy manifest, public release
  notes, appcast, GitHub release assets, and installed app version all agree.
- [ ] Prove menu bar and Settings screenshots match the policy after rehearsal.
- [ ] Prove remote Sparkle update into `0.1.64`.

##### 0.1.65 Blessed Professional App

- [ ] Publish `0.1.65` as the final blessed version of this train.
- [ ] Mark it mandatory if any previous train version should stop running.
- [ ] Prove the installed app reaches `0.1.65` through remote Sparkle update.
- [ ] Prove the final installed app is runtime-steady, setup-ready, wiki-healthy,
  menu-visible, update-current, policy-current, and uninstall-verifiable.
- [ ] Prove the final app has founder-controlled update policy end to end:
  mandatory vs optional, no updater release notes by default, concise optional
  prompt, simple failed-update support message, optional `1Context Improved!`
  post-install message, menu update action, and Settings version number.
- [ ] Close this `/goal` section with evidence paths and any intentionally
  deferred future permissions work.

## See Also

- [For You](/for-you)
- [Your Context](/your-context)
- [Projects](/projects)
- [Topics](/topics)
