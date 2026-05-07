---
title: Goal
slug: goal
section: project
access: private
summary: "The professional app bar for 1Context: permission flows, update discipline, and the behaviors that must feel reliable before broad release."
status: published
last_updated: 2026-05-07
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
- surface mandatory updates prominently in the menu
- prefer automatic download, install, and relaunch for mandatory blessed builds
- stop passive remembering on versions below the minimum blessed version when
  update is available but not yet applied
- report update status in diagnostics so support can tell whether a machine is
  current, stale, blocked, or unable to reach the appcast

Daily checks are not enough if they leave users running old builds for days.
The product standard is: after a blessed mandatory release, there should be no
long-lived previous-version 1Context process continuing to monitor work.

## Product Standard

Professional 1Context behavior is direct:

- Open Wiki opens the wiki or opens the setup flow that makes opening possible.
- Refresh Wiki refreshes the wiki or opens the setup flow that makes refresh
  possible.
- Start starts remembering or opens the setup and permission flow required to
  start remembering.
- Please Update means Sparkle can take the user into the signed update flow.
- Mandatory Update means old passive monitoring should not continue quietly.

The app can be privacy-respecting and still be assertive. The professional
shape is explicit consent, immediate repair, visible state, and no silent
failure.

## Verification Targets

- Open Wiki with missing Local Wiki Access opens setup instead of failing
  silently.
- After setup is granted, the original Open Wiki action completes without the
  user needing to rediscover it.
- A mandatory Sparkle appcast item moves an older installed app to the blessed
  version through a deterministic update smoke.
- Menu, CLI, and diagnostics agree on update availability, mandatory status,
  installed version, and required setup state.
- Passive remembering pauses or refuses to start when the installed version is
  below a declared mandatory minimum.
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
- Mandatory updates use Sparkle's automatic background install path when
  possible; UI is reserved for blocked cases where macOS or Sparkle needs the
  user.
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
  recovery path.
- The release docs, `/goal`, GitHub workflow, release notes, and live local wiki
  all agree on what the current blessed version is.

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
- [ ] Publish `v0.1.56` with `1Context.dmg`, versioned DMG, checksum, and
  `appcast.xml`.
- [ ] Prove remote Sparkle update from installed `0.1.55` to `0.1.56`.
- [ ] Prove `0.1.56` steady state after update with the reusable verifier.

##### 0.1.57 Release Automation Proof

- [ ] Run the updated GitHub release workflow or equivalent scripted release
  path end to end for a real release.
- [ ] Prove workflow-provided mandatory metadata appears correctly in the
  generated appcast.
- [ ] Prove `0.1.56 -> 0.1.57` auto-update through the remote Sparkle feed.
- [ ] Add a release artifact audit script if the workflow still requires manual
  asset inspection.

##### 0.1.58 Uninstall and Residue Proof

- [ ] Add full uninstall smoke that can inspect helper, LaunchAgents, local CA,
  managed hooks, logs/cache, and optional data deletion.
- [ ] Run uninstall without `--delete-data` and prove user wiki content remains.
- [ ] Run uninstall with delete-data in a controlled fixture account or fixture
  path and prove only approved paths are removed.
- [ ] Reinstall from DMG and prove setup/update still works after residue cleanup.
- [ ] Prove remote Sparkle update into `0.1.58`.

##### 0.1.59 Failed Update and Recovery Proof

- [ ] Add failed-update rollback smoke for broken appcast, missing asset, bad
  signature, and interrupted download cases where feasible.
- [ ] Prove failed update leaves old app launchable and diagnostics clear.
- [ ] Prove stale local HTTPS helper is detected and repaired after app
  replacement.
- [ ] Prove remote Sparkle update into `0.1.59`.

##### 0.1.60 Permission Flywheel Expansion

- [ ] Define the permission readiness model for future Screen Recording and
  Accessibility surfaces without making the CLI the accidental permission owner.
- [ ] Add native setup rows for permission-dependent shipped surfaces only.
- [ ] Prove blocked action opens the relevant permission/setup flow.
- [ ] Prove granted state is detected automatically without manual Check Again.
- [ ] Prove remote Sparkle update into `0.1.60`.

##### 0.1.61 Restart and Login Resilience

- [ ] Prove menu LaunchAgent and runtime LaunchAgent recover after app relaunch.
- [ ] Prove machine-restart or login-style recovery on this Mac with screenshots
  and CLI status artifacts.
- [ ] Prove desired state `running` survives update/restart unless the user
  explicitly stops remembering.
- [ ] Prove remote Sparkle update into `0.1.61`.

##### 0.1.62 Diagnostics and Supportability

- [ ] Add one command or evidence bundle that collects version, appcast, Sparkle
  defaults, LaunchAgent state, helper state, setup state, runtime health, local
  wiki health, and recent logs.
- [ ] Redact sensitive paths/content by default while preserving operator-useful
  state.
- [ ] Prove diagnostics can distinguish healthy, needs setup, needs update,
  failed update, and stopped-by-user states.
- [ ] Prove remote Sparkle update into `0.1.62`.

##### 0.1.63 Clean-Machine Acceptance

- [ ] Run clean-machine checklist from DMG install through setup, wiki open,
  update, relaunch, and uninstall cleanup.
- [ ] Capture screenshots and command artifacts for every consent or native UI
  step.
- [ ] Prove Homebrew remains an install channel and Sparkle remains the update
  engine.
- [ ] Prove remote Sparkle update into `0.1.63`.

##### 0.1.64 Release Train Rehearsal

- [ ] Run a full release rehearsal where the only accepted proof is installed
  app behavior after remote update.
- [ ] Validate `/goal`, release docs, release notes, appcast, GitHub release
  assets, and installed app version all agree.
- [ ] Prove remote Sparkle update into `0.1.64`.

##### 0.1.65 Blessed Professional App

- [ ] Publish `0.1.65` as the final blessed version of this train.
- [ ] Mark it mandatory if any previous train version should stop running.
- [ ] Prove the installed app reaches `0.1.65` through remote Sparkle update.
- [ ] Prove the final installed app is runtime-steady, setup-ready, wiki-healthy,
  menu-visible, update-current, and uninstall-verifiable.
- [ ] Close this `/goal` section with evidence paths and any intentionally
  deferred future permissions work.

## See Also

- [For You](/for-you)
- [Your Context](/your-context)
- [Projects](/projects)
- [Topics](/topics)
