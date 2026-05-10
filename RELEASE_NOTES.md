# 1Context v0.1.61 Public Preview

This release includes:

- `0.1.61` is a mandatory update proof release.
- Mandatory update verification now fails if updater prompts, installer
  explanations, relaunch copy, or release notes appear during the automatic
  update window.
- CI now runs upgrade-path checks that cover the destructive proof guard,
  update-class validation, mandatory no-UI assertions, and staging-feed safety.
- Packaged release smoke checks now verify the app bundle shape, local web
  helper plist, required executables, and bundled generated `/goal` assets.
- Fresh local web placeholders now seed every bundled wiki family, including
  `/goal`.
- Setup callbacks are isolated on the main actor so the Swift test suite passes
  under strict concurrency checking.

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
