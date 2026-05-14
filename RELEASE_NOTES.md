# 1Context v0.1.73 Public Preview

This release includes:

- `0.1.73` keeps the release factory moving after the public `0.1.72` train:
  the packaged app now carries a small allowlisted memory-runtime artifact
  instead of a source checkout, and official proof is wired to exercise real
  uninstall, reinstall, delete-data, and setup restoration.
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
