# 1Context v0.1.55 Public Preview

This release includes:

- Setup now keeps observing readiness while the setup window is open, so grants
  made outside the app are detected automatically instead of waiting for a
  manual Check Again click.
- When Local Wiki Access is already ready, setup shows the useful Open Wiki
  action and removes the stale Check Again button.
- Completing or rediscovering ready setup immediately marks remembering as the
  desired state, starts the runtime, starts the local web edge, and resumes the
  blocked action when safe.
- The menu app now loads its LaunchAgent during launch instead of only writing
  the plist for a future login, so an installed app can self-heal a missing menu
  auto-start registration.
- `0.1.55` is intended as the mandatory follow-up to `0.1.54` for the setup and
  permissions flywheel.

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
