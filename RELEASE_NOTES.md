# 1Context v0.1.38 Public Preview

This release includes:

- CLI updater now forwards Terminal stdin to Homebrew so password prompts behave correctly
- cask postflight fix so the menu bar relaunches after upgrade even when the runtime was stopped
- safer GUI updater Terminal launch
- immediate menu reflection when `1context stop` stops the runtime
- version-only release to validate the fixed GUI updater path
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
- AppIcon.icns for Finder and Spotlight
- menu responsiveness and lifecycle hardening
- safer menu updater, release workflow guards, and stale runtime detection

Install:

```bash
brew install --cask hapticasensorics/tap/1context
```

Known preview limits:

- macOS 13 Ventura or newer required
- Apple Silicon only
- project wiki, MCP, and agent memory surfaces are still in active development
