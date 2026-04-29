# 1Context

Own your context. 1Context is a memory engine for agentic work.

This public preview installs the macOS menu bar app, local runtime, and
Homebrew Cask plumbing for 1Context. The full project wiki and agent memory
surfaces are in active development.

The Cask installs user LaunchAgents for the menu bar app and local runtime so
1Context can stay available after install and login. The runtime stores state
locally and does not upload project data in this preview.

[haptica.ai](https://haptica.ai)

## Install

```bash
brew install --cask hapticasensorics/tap/1context
```

Requires Apple Silicon and macOS 13 Ventura or newer.

Verify:

```bash
1context status
```

Support report:

```bash
1context diagnose
```

Uninstall:

```bash
brew uninstall --cask hapticasensorics/tap/1context
```

Remove user content and local preview data too:

```bash
brew uninstall --cask --zap hapticasensorics/tap/1context
```

## Local Files

1Context keeps user-owned content and app machinery separate:

```text
~/1Context/
  human-readable wiki files and user-owned content

~/Library/Application Support/1Context/
  app/runtime state, config, indexes, sockets, queues, and update metadata

~/Library/Logs/1Context/
  logs and debug/support information

~/Library/Caches/1Context/
  disposable cache, safe to delete
```

See [PERMISSIONS.md](PERMISSIONS.md) for the ownership, consent, and privacy contract used by the runtime and installer.

## Privacy

The public preview makes no product telemetry calls and does not upload project
data. It checks GitHub Releases at most once per day to show whether an update
is available. The update check uses a non-cookie, nonpersistent network session.
Disable update checks for a command invocation with:

```bash
ONECONTEXT_NO_UPDATE_CHECK=1 1context
```

## Agent Integrations

1Context includes a first-party Claude Code hook bridge. It installs only
managed command hooks and can remove them again without touching your other
Claude settings:

```bash
1context agent integrations status
1context agent integrations install
1context agent integrations repair
1context agent integrations uninstall
```

The current hook behavior is intentionally small: install adds only a Claude
`SessionStart` hook plus the status line. Claude receives a local wiki pointer
and repo-aware pointer when available. Prompt, tool, compact, and session-end
hooks are implemented as safe no-ops but are not installed by default. Codex
integration is status-only until its hook configuration is verified.

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
core, hooks do not run heavy memory work, and `run` only allows top-level
`status`, `storage`, `wiki`, and `memory` commands.

See [docs/memory-core-contract.md](docs/memory-core-contract.md) for the
subprocess contract and compatibility fixture.

## Development

This repository includes the public macOS runtime and menu bar app:

```bash
swift test --package-path macos
./scripts/test.sh
```

Runtime commands use product language:

```bash
1context start
1context status
1context diagnose
1context restart
1context stop
```

The menu bar app can be packaged locally with:

```bash
ALLOW_UNNOTARIZED=1 NOTARIZE=0 ./scripts/package-macos-release.sh
```

That produces an ad-hoc signed local build under `dist/` and does not require a
Developer ID certificate.

Maintainer release packaging uses Developer ID signing and notarization:

```bash
ONECONTEXT_SIGNING_MODE=developer-id NOTARIZE=1 ./scripts/package-macos-release.sh
```

Release packaging validates that archives do not contain local owner/group
metadata, AppleDouble files, local build paths, or SwiftPM resource-bundle
fallback paths. To clear local release outputs before packaging:

```bash
./scripts/clean-release-artifacts.sh
```

For RPC lifecycle stress, run:

```bash
ONECONTEXT_STRESS_COUNT=1000 ./scripts/stress-runtime-rpc.sh
```

For menu responsiveness work, launch the app with perf timing enabled and inspect
`~/Library/Logs/1Context/menu.log`:

```bash
ONECONTEXT_MENU_PERF_LOG=1 open /Applications/1Context.app
```

To notarize the built app, first configure a `notarytool` keychain profile, then run:

```bash
NOTARYTOOL_PROFILE=1context-notary ./scripts/notarize-macos-app.sh
```

## Thanks

Thanks to [Karpathy](https://gist.github.com/karpathy/442a6bf555914893e9891c11519de94f)
for llm-wiki.

## License

Apache-2.0. Copyright Aurem, Inc.
