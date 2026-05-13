# macOS Release Runbook

This is the current operating doc for shipping the 1Context macOS app.

## Current Truth

- Current release candidate: `0.1.63`.
- Latest public release before this cut: `0.1.62`.
- Normal install channel: signed, notarized, stapled `1Context.dmg` from GitHub
  Releases.
- Secondary install channel: Homebrew cask in `hapticasensorics/homebrew-tap`.
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
- `OneContextPermissions` owns future sensitive permission snapshots without
  making the CLI the TCC permission owner.
- `OneContextLocalWeb` owns Caddy, local HTTPS, helper registration, and wiki
  health.
- `OneContextUpdate` owns update policy, appcast configuration, and diagnostics;
  `OneContextSparkleUpdate` owns the actual Sparkle framework controller.
- `OneContextMemoryCore` is reached through a bounded subprocess contract.

See [macos-app-architecture.md](macos-app-architecture.md),
[local-web-contract.md](local-web-contract.md), and
[memory-core-contract.md](memory-core-contract.md) for the detailed contracts.

## Release Package

Local ad-hoc package:

```bash
ALLOW_UNNOTARIZED=1 NOTARIZE=0 ./scripts/package-macos-release.sh
```

Production package:

```bash
ONECONTEXT_SPARKLE_FEED_URL="https://github.com/hapticasensorics/1context/releases/latest/download/appcast.xml" \
SPARKLE_DOWNLOAD_URL_PREFIX="https://github.com/hapticasensorics/1context/releases/download/v$(cat VERSION)/" \
./scripts/package-macos-production-release.sh
```

The production path signs and notarizes the app and DMG, staples both, validates
Gatekeeper acceptance, and generates Sparkle appcast artifacts for the release
DMG.

## Required Local Proof

Run these before treating a release candidate as locally proved:

```bash
./scripts/check-version-consistency.sh
./scripts/check-update-policy.sh
swift test --package-path macos
./scripts/test.sh
./scripts/test-upgrade-paths.sh
ALLOW_UNNOTARIZED=1 NOTARIZE=0 ./scripts/package-macos-release.sh
./scripts/test-launch-agent-package.sh
```

For memory-core changes:

```bash
cd memory-core
uv run --with pytest pytest
```

For release/update policy changes, also validate the generated appcast and
release assets:

```bash
./scripts/audit-github-release-assets.sh v$(cat VERSION)
```

## Update Policy

Founder-controlled policy lives in [update_policy.html](update_policy.html) and
`release/update-policy.toml`.

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

The real-Mac update runner is an escalation gate, not a routine release step.
Use it when the update hop itself matters: mandatory releases, Sparkle metadata
changes, install/setup/LaunchAgent/local-web changes, update residue repairs, or
launch candidates.

See [ci/self-hosted-mac-runner.md](ci/self-hosted-mac-runner.md).

Routine request:

```bash
./scripts/request-release-proof.sh --dry-run
./scripts/request-release-proof.sh \
  --dispatch \
  --watch \
  --download-artifacts \
  --ref v$(cat VERSION) \
  --proof-reason "routine mandatory release proof for $(cat VERSION)"
```

The wrapper resolves the new version, update class, public appcast URL, and
mandatory old-version baseline from `VERSION` and `release/update-policy.toml`.
Pass `--old-version` for optional releases.

## Clean-machine Acceptance

The remaining high-value acceptance pass is still real installed-app behavior,
not more planning docs:

- DMG open and move/install into `/Applications`.
- First-launch setup and Local Wiki Access grant.
- Wiki open and refresh at the canonical local HTTPS URL.
- App relaunch and login-style LaunchAgent recovery.
- Sparkle update from previous public version.
- Non-destructive uninstall preserving `~/1Context`.
- Controlled delete-data uninstall in a fixture account or fixture path.

Useful local harnesses:

```bash
./scripts/harness-macos-clean-machine-checklist.sh
./scripts/harness-macos-install-prompt.sh
./scripts/harness-macos-setup-flow.sh
./scripts/test-macos-install-move-local.sh
./scripts/test-macos-uninstall-command.sh
```

Track detailed release-train work in
[goals/1context-release-lockdown-goal.md](goals/1context-release-lockdown-goal.md).
