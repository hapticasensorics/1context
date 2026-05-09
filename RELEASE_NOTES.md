# 1Context v0.1.59 Public Preview

This release includes:

- Founder-controlled update policy is now committed as release metadata instead
  of being invented by workflow inputs.
- Mandatory and optional release behavior, update failure copy, release-notes
  visibility, optional prompt copy, and rare post-install messaging are all
  validated from the policy manifest.
- Sparkle appcasts hide release notes by default unless policy explicitly allows
  them.
- Update failures use the simple support message: `Update failed. Please contact
  support at paul@haptica.ai.`
- Mandatory update detection no longer stops passive remembering just because an
  update attempt exists or fails.
- The menu keeps the normal runtime Start/Stop control separate from the pending
  update action.
- The release workflow validates policy, appcast metadata, tag/version agreement,
  release-note version agreement, and GitHub release assets.
- `0.1.59` is intended as the mandatory policy-control follow-up to `0.1.58`.

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
