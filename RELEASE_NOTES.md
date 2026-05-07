# 1Context v0.1.53 Public Preview

This release includes:

- Open Wiki, Refresh Wiki, and Start now open the app-owned setup flow when
  Local Wiki Access blocks the requested action instead of failing silently.
- Setup can resume the original Open Wiki or Refresh Wiki action once Local Wiki
  Access is ready.
- Sparkle update handling is more aggressive: release builds enable automatic
  checks/downloads where supported, mandatory appcast items are surfaced in the
  menu, and critical updates can install immediately when Sparkle is ready.
- Passive remembering pauses or refuses to start while a mandatory update is
  pending, so stale monitoring code does not keep running quietly.
- App and bundled CLI version reporting now resolves from the installed app
  bundle, which keeps Sparkle-replaced apps, diagnostics, and support commands
  aligned.
- A local Sparkle appcast smoke now proves an older installed fixture app can
  update to a newer fixture app through Sparkle before remote releases are
  promoted.
- `/goal` is now a first-class local wiki page tracking the professional app
  checklist, including the `0.1.53 -> 0.1.54` remote update trial.

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
