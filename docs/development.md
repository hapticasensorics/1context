# 1Context Development And Release Notes

This document holds the maintainer details that used to make the README feel
like an engineering manual. The README should stay product-first.

Start with [README.md](README.md) for the docs map. For release operations, use
[macOS Release Runbook](macos-release-runbook.md) as the current source of
truth; this file keeps the supporting engineering details.

## Local Files

1Context keeps user-owned content and app machinery separate:

```text
~/1Context/
  user-wiki/        readable wiki source, talk, templates, assets, and export
  context-engine/   user-owned agent work, prompts, runs, ledgers, and manifests

~/Library/Application Support/1Context/
  app/runtime state, config, sockets, queues, local web mirrors, and derived indexes

~/Library/Logs/1Context/
  logs and debug/support information

~/Library/Caches/1Context/
  disposable cache, safe to delete
```

The production user-data contract lives in [user-data-spec.md](user-data-spec.md).
The repo-local development mirror is described in
[../runtime/README.md](../runtime/README.md).

See [../PERMISSIONS.md](../PERMISSIONS.md) for the ownership, consent, and
privacy contract used by the runtime and installer.

## Privacy

The public preview makes no product telemetry calls and does not upload project
data. Update policy, appcast diagnostics, and CLI-readable update snapshots live
in `OneContextUpdate`; the actual Sparkle framework driver lives in
`OneContextSparkleUpdate`. Setup and runtime readiness do not carry update state.

## Local Web

The release app bundles Caddy and serves the local wiki at the canonical local
URL reported by:

```bash
1context wiki local-url
```

Product mode requires local wiki access setup. The app opens the setup window
on launch, repair, and from `Settings > Setup...`. Setup registers the bundled
ServiceManagement helper for `127.0.0.1:443` and trusts the local Caddy CA in
the user's login keychain so browsers can open the canonical URL without a port.
The CLI is intentionally not a setup or lifecycle control plane; setup stays in
the app UI and redacted setup state is visible through `1context diagnose`.

The menu bar owns Caddy lifetime. The daemon owns the local `/api/wiki/*`
adapter and the wiki refresh entrypoint. Browser code should call relative
`/api/wiki/*` routes so the same static site can run behind local Caddy today
and cloud hosting later.

See [local-web-contract.md](local-web-contract.md) for the local-first web
contract.

See [macos-app-architecture.md](macos-app-architecture.md) for the app-owned
setup, permissions, update, and local-web source boundaries.

## Test Commands

```bash
swift test --package-path macos
./scripts/test.sh
```

For updater work, keep tests on update policy, appcast configuration, the
Sparkle controller, the menu update path, redacted diagnostics, and the release
feed. Public update controls live in the app, not in the CLI.

## Release Packaging

Local ad-hoc packaging:

```bash
./scripts/release-train.sh build --channel dev
```

The dev channel builds a side-by-side app identity instead of another copy of
the official app:

```text
dist/1Context Dev.app
/Applications/1Context Dev.app
bundle id: com.haptica.1context.dev
user data: ~/1Context-Dev
app support: ~/Library/Application Support/1Context Dev
logs: ~/Library/Logs/1Context Dev
preferences: ~/Library/Preferences/com.haptica.1context.dev.plist
local wiki: http://localhost:39291/your-context
```

To install the dev app without colliding with the official release:

```bash
./scripts/release-train.sh build --channel dev
ditto --norsrc --noqtn "dist/1Context Dev.app" "/Applications/1Context Dev.app"
open -na "/Applications/1Context Dev.app"
"/Applications/1Context Dev.app/Contents/MacOS/1context-cli" diagnose
```

The official app remains `/Applications/1Context.app` with bundle id
`com.haptica.1context`, `~/1Context`, and the portless local HTTPS helper. The
dev app uses an unprivileged HTTP localhost port by default, so it does not take
over the official `443` helper or Sparkle feed.

Maintainer release packaging uses Developer ID signing, notarization, and the
production Sparkle feed configuration:

```bash
./scripts/release-train.sh build --channel official
```

The release train reads `release/release.toml`, auto-detects the Developer ID
Application identity, reads the Sparkle public EdDSA key from the release
keychain account or `ONECONTEXT_SPARKLE_PUBLIC_ED_KEY`, signs and notarizes the
app and DMG, then generates `dist/sparkle-updates/appcast.xml`. For the
protected self-hosted release workflow,
`scripts/prepare-macos-release-keychain.sh` unlocks the dedicated release
keychain and `scripts/check-macos-release-credentials.sh` preflights Developer ID
signing, Sparkle signing, and the notary profile before any artifact is built.
The GitHub `Release` workflow defaults to validation-only mode so it does not
queue the protected self-hosted Mac by accident. Dispatch it with
`run_signed_publish=true` to build, sign, notarize, publish, and audit public
assets. Keep `run_self_hosted_proof=false` unless you also want the real-Mac
Sparkle proof and bless pass for that release.

Release packaging validates that archives do not contain local owner/group
metadata, AppleDouble files, Homebrew paths, local build paths, SwiftPM
resource-bundle fallback paths, stale wiki manifests, or generated markdown
source files in the bundled user-wiki surface.

Production packaging signs and notarizes both layers:

1. `dist/1Context.app` is Developer ID signed, submitted as a ZIP, stapled, and
   assessed with Gatekeeper.
2. `dist/1Context-<version>-macos-arm64.dmg` is signed, submitted, stapled, and
   assessed with Gatekeeper.

`scripts/release-train.sh build --channel dev` is the local unsigned package
path. It is not a release command. Prototype, private, and official channels
must bundle their release-owned runtime inputs and must not install or resolve
Homebrew packages while creating the shipped `.app` or DMG.

The self-hosted Mac update proof is required for official releases that list it
in `release/release.toml`. Private and official updater hops should be proved by
the factory instead of by hand-filled workflow inputs.

When the release owner decides the Mac proof is warranted, use
`scripts/release-train.sh prove` so `release/release.toml` owns the update
class, old-version baseline, appcast URL, and protected workflow dispatch. The
version-N app installed by the proof must already have `SUFeedURL` set to the
manifest appcast URL; rebuild the release artifact from the manifest instead of
passing a one-off staging feed through workflow inputs.

```bash
./scripts/release-train.sh prove
```

To notarize a built release artifact directly, first configure a `notarytool`
keychain profile. Direct DMG notarization expects the DMG to already be signed:

```bash
NOTARYTOOL_PROFILE=1context-notary ./release/tools/notarize-macos-artifact.sh dist/1Context.app
NOTARYTOOL_PROFILE=1context-notary ./release/tools/notarize-macos-artifact.sh dist/1Context-<version>-macos-arm64.dmg
```
