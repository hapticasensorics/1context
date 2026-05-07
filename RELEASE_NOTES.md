# 1Context v0.1.56 Public Preview

This release includes:

- The GitHub release workflow now publishes the product Sparkle artifact shape:
  the versioned DMG, `1Context.dmg`, checksum, and signed `appcast.xml`.
- Release dispatch now accepts mandatory Sparkle metadata, including critical
  and minimum autoupdate versions.
- A reusable installed-app steady-state verifier records CLI status, update
  state, permissions, LaunchAgent state, runtime log deltas, and screenshot
  evidence.
- `/goal` now carries the full finish-it release train from `0.1.56` through
  `0.1.65`, with every version tied to remote Sparkle update proof.
- Professional-app docs now reflect the `0.1.55` shipped baseline and the active
  `0.1.56` cleanup release instead of stale `0.1.51` current-state language.
- `0.1.56` is intended as the mandatory follow-up to `0.1.55` for release
  workflow truth, steady-state proof, and the update flywheel.

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
