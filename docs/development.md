# 1Context Development And Release Notes

This document holds the maintainer details that used to make the README feel
like an engineering manual. The README should stay product-first.

## Local Files

1Context keeps user-owned content and app machinery separate:

```text
~/1Context/
  human-readable wiki files and user-owned content

~/Library/Application Support/1Context/
  app/runtime state, config, indexes, sockets, queues, and local web state

~/Library/Logs/1Context/
  logs and debug/support information

~/Library/Caches/1Context/
  disposable cache, safe to delete
```

See [../PERMISSIONS.md](../PERMISSIONS.md) for the ownership, consent, and
privacy contract used by the runtime and installer.

## Privacy

The public preview makes no product telemetry calls and does not upload project
data. Native update diagnostics live behind `OneContextUpdate`; Sparkle can land
behind that boundary without changing setup, runtime, or CLI callers.

## Agent Integrations

1Context includes first-party Claude Code and Codex hook bridges. They install
only managed command hooks and can remove them again without touching unrelated
agent settings:

```bash
1context agent integrations status
1context agent integrations install
1context agent integrations repair
1context agent integrations uninstall
```

Hook messages should read the canonical local wiki URL from 1Context-managed
state. They should not hardcode ports, start web servers, or depend on a
developer checkout.

## Memory Core Adapter

The public macOS app can be configured to call an external memory-core
executable through a narrow JSON subprocess boundary. This is the future bridge
for the private memory engine; the public app does not bundle or copy that
business logic.

```bash
1context memory-core status
1context memory-core doctor
1context memory-core configure --executable /path/to/memory-core
1context memory-core run -- status --json
1context memory-core configure --clear
```

The adapter is explicit and bounded: lifecycle commands do not depend on memory
core, hooks do not run heavy memory work, and `run` only accepts documented JSON
command shapes such as `status --json`, `wiki list --json`, and
`memory tick --wiki-only --json`.

See [memory-core-contract.md](memory-core-contract.md) for the subprocess
contract and compatibility fixture.

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
The CLI remains a support path:

```bash
1context setup local-web status
1context setup local-web install
1context setup local-web uninstall
```

The menu bar owns Caddy lifetime. The daemon owns the local `/api/wiki/*`
adapter. Browser code should call relative `/api/wiki/*` routes so the same
static site can run behind local Caddy today and cloud hosting later.

See [local-web-contract.md](local-web-contract.md) for the local-first web
contract.

See [macos-app-architecture.md](macos-app-architecture.md) for the app-owned
setup, permissions, update, and local-web source boundaries.

## Test Commands

```bash
swift test --package-path macos
./scripts/test.sh
```

For memory-core tests:

```bash
cd memory-core
uv run --with pytest pytest
```

For RPC lifecycle stress:

```bash
ONECONTEXT_STRESS_COUNT=1000 ./scripts/stress-runtime-rpc.sh
```

For menu responsiveness work, launch the app with perf timing enabled and
inspect `~/Library/Logs/1Context/menu.log`:

```bash
ONECONTEXT_MENU_PERF_LOG=1 open /Applications/1Context.app
```

For updater work, keep tests on the native updater adapter and release feed.
`1context update`, menu update, and diagnostics should all report the same
app-owned update state.

## Release Packaging

Local ad-hoc packaging:

```bash
ALLOW_UNNOTARIZED=1 NOTARIZE=0 ./scripts/package-macos-release.sh
```

Maintainer release packaging uses Developer ID signing and notarization:

```bash
CODESIGN_IDENTITY="Developer ID Application: Example, Inc. (TEAMID)" NOTARIZE=1 ./scripts/package-macos-release.sh
```

Release packaging validates that archives do not contain local owner/group
metadata, AppleDouble files, local build paths, or SwiftPM resource-bundle
fallback paths.

When `NOTARIZE=1`, packaging signs and notarizes both layers:

1. `dist/1Context.app` is Developer ID signed, submitted as a ZIP, stapled, and
   assessed with Gatekeeper.
2. `dist/1Context-<version>-macos-arm64.dmg` is signed, submitted, stapled, and
   assessed with Gatekeeper.

`ALLOW_UNNOTARIZED=1 NOTARIZE=0` is the only supported local-dev bypass for
stapler and Gatekeeper checks.

To clear local release outputs before packaging:

```bash
./scripts/clean-release-artifacts.sh
```

To notarize a built release artifact directly, first configure a `notarytool`
keychain profile. Direct DMG notarization expects the DMG to already be signed:

```bash
NOTARYTOOL_PROFILE=1context-notary ./scripts/notarize-macos-artifact.sh dist/1Context.app
NOTARYTOOL_PROFILE=1context-notary ./scripts/notarize-macos-artifact.sh dist/1Context-<version>-macos-arm64.dmg
```
