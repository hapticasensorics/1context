# 1Context v0.1.58 Public Preview

This release includes:

- A real remote-update proof target for the app-owned no-click Sparkle updater:
  installed `0.1.57` should move to this release without Install Update or
  Install and Relaunch prompts.
- Mandatory Sparkle updates now use an app-owned no-click driver instead of the
  standard interactive Install Update and Install and Relaunch prompts.
- The updater grants Sparkle automatic-check permission in-app, auto-answers
  mandatory update decisions with install, and auto-continues through the final
  relaunch gate.
- Manual non-mandatory update checks still present a concise Update/Later
  confirmation.
- Focused tests lock the mandatory policy as install-without-prompt while
  preserving information-only and non-mandatory safeguards.
- A reusable installed-app steady-state verifier records CLI status, update
  state, permissions, LaunchAgent state, runtime log deltas, and screenshot
  evidence.
- `/goal` now carries the full finish-it release train from `0.1.56` through
  `0.1.65`, with every version tied to remote Sparkle update proof.
- Professional-app docs now reflect the `0.1.55` shipped baseline and the active
  release train instead of stale `0.1.51` current-state language.
- `0.1.58` is intended as the mandatory follow-up to `0.1.57` for the first
  fixed-driver no-click remote update proof.

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
