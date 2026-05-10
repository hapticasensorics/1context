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

That preservation is intentional. This is update CI, not clean-account setup CI:
destroying app support would turn the proof into a Local Wiki Access permission
exercise and make update failures harder to read.

## Machine Setup

Use a dedicated, logged-in macOS user on a dedicated Mac. The first practical
mode should be a foreground runner inside that GUI session, not a headless
`svc.sh` service, because the update harness uses AppleScript, `screencapture`,
menu-bar automation, and Sparkle windows.

For the existing `hapticainfra` MacBook Air convention, reuse the same operating
shape rather than adding a second automation model:

- keep the Mac awake and reachable over Tailscale;
- keep surprise macOS updates disabled during release windows;
- use the visible GUI session for proof screenshots and prompts;
- use the Hammerspoon/remote GUI bridge as an optional extra capture layer when
  direct `screencapture` evidence is not enough.

Before first use, open System Settings and grant the logged-in runner terminal
or shell host Accessibility and Screen Recording access. Then run one local
sanity check:

```bash
osascript -e 'tell application "System Events" to get name of first process'
screencapture -x /tmp/1context-runner-screen.png
```

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
   release-worthy and the staging appcast URL matches the intended candidate.

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

The proof leaves `/Applications/1Context.app` at N+1 and healthy. The next run
will stop it, reinstall N from DMG, clear disposable updater caches, and repeat.

## References

- [GitHub: adding self-hosted runners](https://docs.github.com/en/actions/hosting-your-own-runners/managing-self-hosted-runners/adding-self-hosted-runners)
- [GitHub: secure use reference](https://docs.github.com/en/actions/security-for-github-actions/security-guides/security-hardening-for-github-actions)
- [GitHub: using labels with self-hosted runners](https://docs.github.com/en/actions/hosting-your-own-runners/managing-self-hosted-runners/using-labels-with-self-hosted-runners)
- [GitHub: deployments and environments](https://docs.github.com/en/actions/reference/deployments-and-environments)
