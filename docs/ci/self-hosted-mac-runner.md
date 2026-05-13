# Self-hosted Mac Update Runner

This runner exists for one job only: proving that a signed 1Context DMG update
can move a real installed Mac app from version N to N+1 through Sparkle, then
stay healthy afterward.

Do not use this runner for pull requests. GitHub explicitly warns that
self-hosted runners can be persistently compromised by untrusted workflow code,
especially from fork pull requests. The workflow in this repo is dispatch-only,
targets a custom runner label, references the protected
`onecontext-update-runner` environment, and rejects refs outside `main`,
`release/*`, `rc/*`, or `v*` tags.

## Optional Release Gate

The self-hosted Mac proof is optional by policy. It is not part of every update
release and should not become a reflexive box to check. Normal releases can ship
with hosted CI, release asset audit, appcast validation, and any local
maintainer smoke that matches the change.

Use the real-Mac runner when the update hop itself is high signal:

- Sparkle feed/signature/mandatory-update metadata changed.
- The install, move-to-Applications, setup readiness, LaunchAgent, local web, or
  post-update repair path changed.
- The release is mandatory for privacy, data integrity, updater continuity, or
  runtime correctness.
- A previous update had a regression or residue issue and the next release is
  proving the repair.
- The release is a milestone, public launch candidate, or first update after a
  meaningful macOS/runtime change.

Skip it for routine copy, docs, cosmetic UI, release-note-only, or small
non-updater patches unless the release owner explicitly wants the extra proof.
Every workflow dispatch requires a `proof_reason`; use that as the approval
receipt when deciding whether to spend the Mac cycle.

## What It Runs

`.github/workflows/self-hosted-mac-update-proof.yml` runs on:

```yaml
runs-on: [self-hosted, macOS, ARM64, onecontext-update-runner]
```

The release train dispatches the self-hosted proof job, which:

1. Stops any existing 1Context processes.
2. Clears disposable Sparkle/WebKit update caches while preserving
   `~/1Context` and `~/Library/Application Support/1Context`.
3. Installs version N from a signed DMG into `/Applications/1Context.app`.
4. Points the app at the staging appcast for N+1.
5. Runs `scripts/prove-remote-sparkle-update.sh`.
6. Runs `scripts/verify-macos-steady-state.sh`.
7. Uploads evidence from `dist/self-hosted-update-proof/`.

The self-hosted workflow validates the staged appcast against its dispatch
inputs and the release manifest when it is running as part of the protected
release train.

The installed version N app must have `SUFeedURL` set to the same appcast URL
used for the proof. Public release DMGs normally point at the public latest
GitHub appcast. For a pre-public staging proof, pass `old_dmg_url` or
`ONECONTEXT_OLD_DMG_PATH` for a version-N baseline DMG built with
`ONECONTEXT_SPARKLE_FEED_URL=<staging_appcast_url>`. The script fails if the
installed app feed does not match the requested proof feed, because otherwise it
could accidentally prove production latest instead of the staging candidate.

That preservation is intentional. This is update CI, not clean-account setup CI:
destroying app support would turn the proof into a Local Wiki Access permission
exercise and make update failures harder to read.

## Machine Setup

Use a dedicated, logged-in macOS user on a dedicated Mac. The first practical
mode should be a foreground runner inside that GUI session, not a headless
`svc.sh` service, because the update harness uses AppleScript, Hammerspoon
desktop capture, menu-bar automation, and Sparkle windows.

For the existing `hapticainfra` MacBook Air convention, reuse the same operating
shape rather than adding a second automation model:

- keep the Mac awake and reachable over Tailscale;
- keep surprise macOS updates disabled during release windows;
- use the visible GUI session for proof screenshots and prompts;
- treat the Hammerspoon GUI bridge as the preferred screenshot path;
- use direct `screencapture` only as a local fallback. It can fail from SSH even
  when Hammerspoon has valid Screen Recording permission in the GUI session.

Before first use, open System Settings and grant Hammerspoon Screen Recording
access. Grant the logged-in runner terminal Accessibility or Automation access
if AppleScript prompts. Then run one local sanity check:

```bash
osascript -e 'tell application "System Events" to get name of first process'
curl --fail --silent http://127.0.0.1:8742/status
curl --fail --silent \
  -X POST http://127.0.0.1:8742/capture \
  -H 'Content-Type: application/json' \
  -d '{"target":"desktop","path":"/tmp/1context-runner-screen.png"}'
test -s /tmp/1context-runner-screen.png
```

Also complete 1Context Local Wiki setup once as the runner user before the first
update proof. This is a bench prerequisite, not part of the update test:

1. Open `/Applications/1Context.app` in the runner user's logged-in GUI session.
2. Choose `Settings` -> `Setup` and grant `Local Wiki Access`.
3. Approve the certificate trust prompt.
4. In System Settings -> General -> Login Items & Extensions, allow `1Context`
   under App Background Activity if macOS asks.
5. Verify the bench is ready:

```bash
/Applications/1Context.app/Contents/MacOS/1context-cli setup local-web status
/Applications/1Context.app/Contents/MacOS/1context-cli start
/Applications/1Context.app/Contents/MacOS/1context-cli diagnose
```

The self-hosted proof preflights this condition and fails before replacing
`/Applications/1Context.app` if `Setup Ready: yes` is missing. Set
`ONECONTEXT_RUNNER_SETUP_PREFLIGHT=0` only for an intentional first-run setup
experiment.

## Enroll The Runner

The helper downloads the current GitHub runner package and registers it with
the required custom label. It obtains a registration token through `gh`; if the
account lacks runner-admin permission, create the token in GitHub Settings and
pass it as `ACTIONS_RUNNER_INPUT_TOKEN`. The target Mac does not need `gh` when
that token is supplied.

```bash
cd /Users/paulhan/dev/1context-public-launch
gh auth status

ONECONTEXT_RUNNER_REPO=hapticasensorics/1context \
ONECONTEXT_RUNNER_NAME="$(hostname -s)-1context-update" \
ONECONTEXT_RUNNER_LABELS=onecontext-update-runner \
./scripts/self-hosted-mac-runner-enroll.sh
```

Run it from the logged-in GUI session:

```bash
cd ~/actions-runners/1context-update
./run.sh
```

If this becomes a permanent bench, prefer a user LaunchAgent only after the
foreground runner has proven that Sparkle prompts, AppleScript, and screenshots
still work in that session.

## Release Publishing Credentials

The update-proof workflow consumes already-built public DMGs and appcasts. The
production `Release` workflow is narrower and more sensitive: it signs,
notarizes, creates the GitHub release, uploads `1Context.dmg`, the versioned
DMG, checksums, and `appcast.xml`, then audits the public `latest/download`
assets.

For that publishing job, the `haptica_release` account has a dedicated release
keychain:

```text
/Users/haptica_release/Library/Keychains/1context-release.keychain-db
```

The protected GitHub environment `onecontext-update-runner` provides:

- `ONECONTEXT_RELEASE_KEYCHAIN`: path to the release keychain.
- `ONECONTEXT_RELEASE_KEYCHAIN_PASSWORD`: secret used only to unlock that
  keychain during the job.
- `ONECONTEXT_CODESIGN_IDENTITY`: the Developer ID Application identity.
- `ONECONTEXT_SPARKLE_PUBLIC_ED_KEY`: the public Sparkle EdDSA key embedded in
  the app bundle.
- `ONECONTEXT_SPARKLE_PRIVATE_ED_KEY`: secret Sparkle EdDSA key used by
  `generate_appcast`.
- `ONECONTEXT_NOTARYTOOL_PROFILE`: the notarytool profile stored in the release
  keychain.

The workflow runs:

```bash
./scripts/prepare-macos-release-keychain.sh
./scripts/check-macos-release-credentials.sh
```

before checking the tag or building artifacts. `prepare` unlocks only the
explicit release keychain and adds it to the user keychain search list for the
job. `check` fails early unless the runner can see Developer ID signing, Sparkle
signing, and a working notary profile. Do not put these credentials in hosted
GitHub runners or PR-triggered workflows.

## Required GitHub Guardrails

Configure these before treating a green run as release proof:

- Repository or org runner group is restricted to `hapticasensorics/1context`.
- Runner has custom label `onecontext-update-runner`.
- Environment `onecontext-update-runner` exists and requires reviewer approval.
- No workflow with `pull_request` or `pull_request_target` uses the
  `onecontext-update-runner` label.
- Signing, notarization, and Sparkle private-key material is limited to the
  protected release workflow and dedicated release keychain above. The update
  proof job still consumes already-built DMGs and appcasts.

## Trigger A Proof Run

Prefer the release train so release proof becomes muscle memory instead of a
hand-filled form. It reads `release/release.toml`, infers the mandatory
old-version baseline from `minimum_autoupdate_version`, validates the trusted
ref shape, and owns dispatch, watch, artifact download, redaction, and audit:

```bash
./scripts/release-train.sh prove --dry-run
./scripts/release-train.sh prove
```

After the release assets and appcast are uploaded, dispatch and watch the proof:

```bash
./scripts/release-train.sh prove
```

For optional releases or staging feeds, update `release/release.toml` first so
the release train owns the version, update class, appcast URL, and old-version
baseline.

From GitHub UI:

1. Actions -> Self-hosted Mac Update Proof.
2. Run workflow from `main`, `release/*`, `rc/*`, or a `v*` tag.
3. Enter a short `proof_reason`, old version N, staging appcast URL, expected
   N+1 version if it differs from `VERSION`, and update class.
4. Approve the `onecontext-update-runner` environment only if the reason is
   release-worthy, the staging appcast URL matches the intended candidate, and
   the version-N DMG being installed is built to use that appcast URL.

The wrapper is equivalent to this lower-level `gh` call:

```bash
gh workflow run self-hosted-mac-update-proof.yml \
  --repo hapticasensorics/1context \
  --ref main \
  -f proof_reason='mandatory updater policy change' \
  -f old_version=0.1.60 \
  -f new_version=0.1.61 \
  -f staging_appcast_url=https://example.invalid/staging/appcast.xml \
  -f update_class=mandatory
```

If the old DMG is not a normal GitHub release asset, pass it explicitly:

```bash
gh workflow run self-hosted-mac-update-proof.yml \
  --repo hapticasensorics/1context \
  --ref rc/0.1.61 \
  -f proof_reason='staging candidate after Sparkle feed change' \
  -f old_version=0.1.60 \
  -f new_version=0.1.61 \
  -f old_dmg_url=https://example.invalid/releases/1Context-0.1.60-macos-arm64.dmg \
  -f staging_appcast_url=https://example.invalid/staging/appcast.xml \
  -f update_class=mandatory
```

For a true pre-public staging proof, that explicit old DMG path/URL is usually
required unless the public version-N app already points at the same staging
appcast.

The proof leaves `/Applications/1Context.app` at N+1 and healthy. By default it
also requires the final installed app to point back at the public Sparkle feed
(`https://github.com/hapticasensorics/1context/releases/latest/download/appcast.xml`).
If a staging proof installs an app whose final `SUFeedURL` is a local or staging
feed, the script tries to restore the public GitHub release for the same version
before leaving the runner. This prevents a shared human-visible runner from
later showing `Update failed.` because it is still pointed at a dead local
appcast.

For an intentionally staging-only runner, set
`ONECONTEXT_UPDATE_RUNNER_ALLOW_NON_PUBLIC_FINAL_FEED=1` in the runner command
or workflow environment and capture that decision in the proof reason.

The next run will stop the app, reinstall N from DMG, clear disposable updater
caches, and repeat.

## References

- [GitHub: adding self-hosted runners](https://docs.github.com/en/actions/hosting-your-own-runners/managing-self-hosted-runners/adding-self-hosted-runners)
- [GitHub: secure use reference](https://docs.github.com/en/actions/security-for-github-actions/security-guides/security-hardening-for-github-actions)
- [GitHub: using labels with self-hosted runners](https://docs.github.com/en/actions/hosting-your-own-runners/managing-self-hosted-runners/using-labels-with-self-hosted-runners)
- [GitHub: deployments and environments](https://docs.github.com/en/actions/reference/deployments-and-environments)
