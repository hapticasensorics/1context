# 1Context v0.1.43 Public Preview

This release includes:

- memory-core adapter boundary hardening
- exact memory-core command-shape validation
- memory-core configure/doctor contract checks via `status --json`
- memory-core JSON contract validation before successful runs
- redacted memory-core process errors
- private temp capture files and timeout escalation for memory-core subprocesses
- GUI updater now launches a short temp zsh script instead of a fragile quoted command
- cleaner updater Terminal copy and completion text
- version-only release to validate the fixed `0.1.40` GUI updater path
- version-only release to validate the fixed `0.1.39` GUI updater path
- Terminal updater now forces zsh instead of relying on the user's default shell
- menu relaunch waits for the old menu process/lock to clear before bootstrapping
- launchctl helper timeout handling avoids false timeouts on fast commands
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
