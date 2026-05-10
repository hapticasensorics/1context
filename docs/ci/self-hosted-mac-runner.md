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

The job calls `scripts/self-hosted-update-proof.sh`, which:

1. Stops any existing 1Context processes.
2. Clears disposable Sparkle/WebKit update caches while preserving
   `~/1Context` and `~/Library/Application Support/1Context`.
3. Installs version N from a signed DMG into `/Applications/1Context.app`.
4. Points the app at the staging appcast for N+1.
5. Runs `scripts/prove-remote-sparkle-update.sh`.
6. Runs `scripts/verify-macos-steady-state.sh`.
7. Uploads evidence from `dist/self-hosted-update-proof/`.

The self-hosted workflow validates the staged appcast against its dispatch
inputs. It does not require the checked-out repo's `release/update-policy.toml`
to already describe the staged N+1 appcast, because the runner can be used
against pre-release assets before the release commit lands.

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
/Applications/1Context.app/Contents/MacOS/1context-cli status --debug
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

## Required GitHub Guardrails

Configure these before treating a green run as release proof:

- Repository or org runner group is restricted to `hapticasensorics/1context`.
- Runner has custom label `onecontext-update-runner`.
- Environment `onecontext-update-runner` exists and requires reviewer approval.
- No workflow with `pull_request` or `pull_request_target` uses the
  `onecontext-update-runner` label.
- Do not store signing, notarization, or Sparkle private-key secrets on this
  runner. This job consumes already-built DMGs and appcasts.

## Trigger A Proof Run

From GitHub UI:

1. Actions -> Self-hosted Mac Update Proof.
2. Run workflow from `main`, `release/*`, `rc/*`, or a `v*` tag.
3. Enter a short `proof_reason`, old version N, staging appcast URL, expected
   N+1 version if it differs from `VERSION`, and update class.
4. Approve the `onecontext-update-runner` environment only if the reason is
   release-worthy, the staging appcast URL matches the intended candidate, and
   the version-N DMG being installed is built to use that appcast URL.

From `gh`:

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

The proof leaves `/Applications/1Context.app` at N+1 and healthy. The next run
will stop it, reinstall N from DMG, clear disposable updater caches, and repeat.

## References

- [GitHub: adding self-hosted runners](https://docs.github.com/en/actions/hosting-your-own-runners/managing-self-hosted-runners/adding-self-hosted-runners)
- [GitHub: secure use reference](https://docs.github.com/en/actions/security-for-github-actions/security-guides/security-hardening-for-github-actions)
- [GitHub: using labels with self-hosted runners](https://docs.github.com/en/actions/hosting-your-own-runners/managing-self-hosted-runners/using-labels-with-self-hosted-runners)
- [GitHub: deployments and environments](https://docs.github.com/en/actions/reference/deployments-and-environments)
