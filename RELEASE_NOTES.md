# 1Context v0.1.62 Public Preview

This release includes:

- `0.1.62` is a mandatory failed-update supportability release.
- Failed updates now show simple support copy instead of technical updater
  details.
- The failed-update window includes `Try Again` while keeping normal
  `Check for Updates` behavior separate.
- Mandatory failed-update smokes now cover missing assets, bad signatures,
  broken appcasts, interrupted downloads, and runtime survival.
- Release-lockdown diagnostics now classify healthy, needs setup, needs update,
  failed update, and stopped-by-user states.
- The release evidence bundle captures version, appcast, Sparkle state,
  LaunchAgents, helper readiness, runtime health, wiki health, and recent logs.

Install:

Download `1Context.dmg`, open it, and launch `1Context.app`. The app moves
itself to Applications, opens setup, and uses Sparkle for app-owned updates.

Known preview limits:

- macOS 13 Ventura or newer required
- Apple Silicon only
- Claude Code and Codex are the first supported agent surfaces
- memory collection and page creation are currently manual
- chat/librarian execution is an API shell in this release
- cloud wiki sharing is not enabled yet; the local wiki is private
