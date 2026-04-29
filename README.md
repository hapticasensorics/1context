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

## Privacy

The public preview makes no telemetry calls and does not upload project data.
It checks GitHub Releases at most once per day to show whether an update is
available. Disable update checks with:

```bash
ONECONTEXT_NO_UPDATE_CHECK=1 1context
```

## Development

This repository includes the public macOS runtime and menu bar app:

```bash
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
NOTARIZE=1 ./scripts/package-macos-release.sh
```

Release packaging validates that archives do not contain local owner/group
metadata, AppleDouble files, local build paths, or SwiftPM resource-bundle
fallback paths. To clear local release outputs before packaging:

```bash
./scripts/clean-release-artifacts.sh
```

Developer ID signing is enabled when the signing identity is present. To notarize
the built app, first configure a `notarytool` keychain profile, then run:

```bash
NOTARYTOOL_PROFILE=1context-notary ./scripts/notarize-macos-app.sh
```

## Thanks

Thanks to [Karpathy](https://gist.github.com/karpathy/442a6bf555914893e9891c11519de94f)
for llm-wiki.

## License

Apache-2.0. Copyright Aurem, Inc.
