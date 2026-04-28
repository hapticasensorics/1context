# 1Context

The cross-agent memory layer for your work.

Own your context. An engine for agentic work.

1Context passively captures what you work on, builds a living wiki for every
project you touch, updates as you work, and opens in any browser with a
shareable link. Claude Code, Cursor, and Codex connect via MCP. No
configuration, no prompts, no workflow changes.

This repository currently contains the public bootstrap CLI and installation
plumbing for 1Context. The product runtime is under active development.

## Install

```bash
brew tap hapticasensorics/tap
brew install hapticasensorics/tap/1context
```

## Check

```bash
1context --version
1context doctor
1context paths
```

## Status

Bootstrap preview. The CLI currently validates installation, versioning, and
future runtime paths.

The bootstrap CLI makes no network calls and collects no telemetry.

## License

Apache-2.0. Copyright Haptica, Inc.
