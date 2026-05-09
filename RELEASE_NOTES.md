# 1Context v0.1.60 Public Preview

This release includes:

- `0.1.60` is an optional update proof release.
- Background optional update discovery should stay quiet: no release notes, no
  modal, no automatic relaunch.
- The menu should keep a pending update action visible until the optional update
  is installed.
- Manual update confirmation uses the concise policy copy: `A 1Context update is
  ready.`
- The update installs only after the user chooses Update.
- Settings continues to show the currently running app version before and after
  the optional update.
- The release manifest marks this release optional so the appcast must not
  include Sparkle critical-update metadata.

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
