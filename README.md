# 1Context

1Context passively captures what you work on, builds a living wiki for every
project you touch, updates as you work, and opens in any browser with a
shareable link. Claude Code and Codex connect via MCP. No configuration, no
prompts, no workflow changes.

[haptica.ai](https://haptica.ai)

## Install

```bash
brew install --cask hapticasensorics/tap/1context
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
1context restart
1context stop
```

The menu bar app can be packaged locally with:

```bash
NOTARIZE=1 ./scripts/package-macos-release.sh
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
