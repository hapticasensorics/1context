# 1Context v0.1.25 Public Preview

This release includes:

- Homebrew Cask installation for Apple Silicon Macs
- native macOS menu bar app
- local runtime start/status/stop
- `1context --version`
- `1context --help`
- faster CLI start/restart after stopping the runtime
- lifecycle debug output with `1context start --debug`, `stop --debug`, and `restart --debug`
- update checks that avoid GitHub API rate limits
- menu lifecycle fixes for CLI start/stop and Spotlight launch
- release-artifact hygiene checks and `1context diagnose`
- IPC and installer hardening from red-team review

Install:

```bash
brew install --cask hapticasensorics/tap/1context
```

Known preview limits:

- macOS 13 Ventura or newer required
- Apple Silicon only
- project wiki, MCP, and agent memory surfaces are still in active development
