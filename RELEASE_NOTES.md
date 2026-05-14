# 1Context v0.1.69 Public Preview

This release includes:

- `0.1.69` repairs post-update menu startup by letting the runtime restore the
  menu LaunchAgent after a mandatory Sparkle update.
- Release policy now defines build channels, timing budgets, and private
  appcast facts in `release/release.toml`.
- The old `release-train.sh package` path is deleted; official release builds
  use `release-train.sh build --channel official`.
- The public CLI is now support-only: version, redacted diagnostics, uninstall,
  and local wiki URL.
- Setup, app lifecycle, and updates are owned by the app UI and menu bar instead
  of old command-line control paths.
- The shipped app bundle remains small after deleting obsolete compatibility
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
