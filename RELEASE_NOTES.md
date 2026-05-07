# 1Context v0.1.54 Public Preview

This release includes:

- Mandatory Sparkle updates now prefer the automatic background install path
  from launch and from the menu's mandatory update action, avoiding the manual
  "Install and Relaunch" flow when Sparkle can install silently.
- Sparkle update defaults are reasserted from the release bundle, including the
  aggressive scheduled check interval, so blessed builds keep checking often.
- DMG validation now fails if Sparkle's `Updater.app`, `Autoupdate`, or update
  XPC services are missing from the embedded framework.
- Appcast generation now starts from a clean updates directory by default, so a
  latest-release feed cannot accidentally inherit stale DMGs from a previous
  release build.
- The remote update proof now records the repaired `0.1.51 -> 0.1.53` path and
  uses this release for the mandatory automatic `0.1.53 -> 0.1.54` trial.
- `/goal` continues to track the permissions and update flywheel as a live
  checklist with app-state evidence.

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
