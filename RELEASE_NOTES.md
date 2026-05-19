# 1Context v0.1.87 Public Preview

This release includes:

- `0.1.87` moves release orchestration into a typed release runner with explicit
  dev, prototype, private, and official channels.
- The signed release workflow keeps self-hosted proof optional by default while
  still auditing published assets after a tagged build.
- Release tooling is slimmer and easier to reason about: retired compatibility
  scripts were removed, and the remaining shell entrypoints delegate to the
  release runner.
- Wiki runtime tooling now lives with the wiki engine instead of being scattered
  through the release and memory-runtime surfaces.
- Official release permissions are narrower by default; artifact attestations
  remain disabled until that path is intentionally designed.
- Appcasts, release assets, version metadata, and Sparkle update policy continue
  to validate against `release/release.toml`.
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
