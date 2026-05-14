# macOS Release Runbook

This is the current operating doc for shipping the 1Context macOS app.

## Current Truth

- Current release candidate: `0.1.68`.
- Latest public release before this cut: `0.1.67`.
- Release truth: `release/release.toml`; do not copy version, update class, or
  appcast facts into docs or workflow inputs by hand.
- Normal install channel: signed, notarized, stapled `1Context.dmg` from GitHub
  Releases.
- Homebrew is not a release dependency or supported install surface for this
  train. Prototype, private, and official `.app`/DMG builds must bundle their
  runtime inputs instead of resolving tools from Homebrew or host `PATH`.
- Update engine: Sparkle from the installed `/Applications/1Context.app`.
- Canonical wiki URL: `https://wiki.1context.localhost/your-context`.
- Required first setup: Local Wiki Access through native app setup.
- User content root: `~/1Context`.
- App machinery root: `~/Library/Application Support/1Context`.
- Development/release goals live under `docs/goals/`, not in the installed user
  wiki.

## Product Boundaries

The app is the user-facing product surface. The CLI is support and automation.

- `OneContextInstall` owns app placement and relaunch into `/Applications`.
- `OneContextSetup` owns required setup readiness.
- `OneContextLocalWeb` owns Caddy, local HTTPS, helper registration, and wiki
  health.
- `OneContextUpdate` owns update policy, appcast configuration, and diagnostics;
  `OneContextSparkleUpdate` owns the actual Sparkle framework controller.

See [macos-app-architecture.md](macos-app-architecture.md) and
[local-web-contract.md](local-web-contract.md) for the detailed contracts.

## Release Factory

Local ad-hoc package:

```bash
./scripts/package-macos-smoke.sh
```

Release factory builds:

```bash
./scripts/release-train.sh build --channel dev
./scripts/release-train.sh build --channel prototype
./scripts/release-train.sh build --channel private
./scripts/release-train.sh build --channel official
```

The official path signs and notarizes the app and DMG, staples both, validates
Gatekeeper acceptance, and generates public Sparkle appcast artifacts for the
release DMG. Dev, prototype, and private channels must not mutate the public
appcast.

## Required Local Proof

Run these before treating a release candidate as locally proved:

```bash
./scripts/check-version-consistency.sh
./scripts/release-train.sh validate
swift test --package-path macos
./scripts/test.sh
./scripts/package-macos-smoke.sh
./scripts/test-launch-agent-package.sh
```

For release/update policy changes, also validate the generated appcast and
release assets:

```bash
./scripts/release-train.sh audit
```

## Update Policy

Founder-controlled policy lives in `release/release.toml`. The runbook explains
the product rule; the manifest is the machine-enforced release truth.

Mandatory releases may interrupt active use and should auto-install/relaunch as
soon as Sparkle can safely do so. Optional releases should stay quiet until the
user chooses the menu update action. Update windows should not expose builder
journals, raw Sparkle errors, installer details, or long release notes by
default.

Failed updates should keep the old app usable when possible and show only:

```text
Update failed. Please contact support at paul@haptica.ai.
```

## Self-hosted Mac Proof

The manifest decides when real-Mac proof is a release gate. If
`release/release.toml` lists `self_hosted_gui_update` in `required_proofs`, the
release is not blessed until the protected runner proves the hop and downloads
its normalized proof JSON. Use the runner for mandatory releases, Sparkle
metadata changes, install/setup/LaunchAgent/local-web changes, update residue
repairs, or launch candidates.

See [ci/self-hosted-mac-runner.md](ci/self-hosted-mac-runner.md).

Routine request:

```bash
./scripts/release-train.sh prove --dry-run
./scripts/release-train.sh prove
```

The factory resolves the old version, new version, update class, appcast URL,
timeout budget, artifact retention, and required proof matrix from
`release/release.toml`. The workflow dispatch form should ask only for a short
human reason.

## Clean-machine Acceptance

The remaining high-value acceptance pass is still real installed-app behavior,
not more planning docs:

- DMG open and move/install into `/Applications`.
- First-launch setup and Local Wiki Access grant.
- Wiki open and refresh at the canonical local HTTPS URL.
- App relaunch and login-style LaunchAgent recovery.
- Sparkle update from previous public version.
- Non-destructive uninstall preserving `~/1Context`.
- Controlled delete-data uninstall on the real proof runner.

The normalized release proof should own this evidence. Keep any exploratory
local checks temporary until they become `release-train.sh prove` proof JSON.

Track current release-train cleanup in
[goals/1context-delete-bloat-goal.md](goals/1context-delete-bloat-goal.md).
Older Sparkle transition evidence is archived at
[goals/archive/1context-release-lockdown-goal.md](goals/archive/1context-release-lockdown-goal.md).
