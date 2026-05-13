# 1Context v0.1.66 Public Preview

This release includes:

- `0.1.66` is the destructive cleanup release for the Sparkle-era app.
- The public CLI is now support-only: version, redacted diagnostics, uninstall,
  and local wiki URL.
- Setup, app lifecycle, and updates are owned by the app UI and menu bar instead
  of old command-line control paths.
- The shipped app bundle is much smaller after deleting obsolete compatibility
  code, test hooks, generated state, and source-checkout packaging assumptions.
- Mandatory Sparkle updates remain quiet and automatic; manual `Check for
  Updates` remains a normal user action.

Install:

Download `1Context.dmg`, open it, and launch `1Context.app`. The app moves
itself to Applications, opens setup, and uses Sparkle for app-owned updates.

Known preview limits:

- macOS 13 Ventura or newer required
- Apple Silicon only
- memory collection and page creation are currently manual
- cloud wiki sharing is not enabled yet; the local wiki is private
