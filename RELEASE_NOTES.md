# 1Context v0.1.75 Public Preview

This release includes:

- `0.1.75` keeps the release factory moving after the public `0.1.74` train:
  the release budgets now match measured factory speed plus a small guard band,
  and the official proof runner can exercise real uninstall, reinstall,
  delete-data, and setup restoration without inheriting sudo-only environment.
- Distributable `.app` and DMG builds reject Homebrew, host Caddy, local library
  links, and executable script interpreters that depend on developer-machine
  package managers or language runtimes.
- Private and official appcasts are validated against `release/release.toml`
  instead of using compatibility skips.
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
