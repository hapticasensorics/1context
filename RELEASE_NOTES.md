# 1Context v0.1.64 Public Preview

This release includes:

- `0.1.64` is a mandatory release-workflow rehearsal.
- The protected self-hosted release runner now has an explicit release keychain
  preflight for Developer ID signing, Sparkle signing, and notarization.
- The release workflow can build the signed/notarized app and publish the
  Sparkle appcast assets without relying on a local manual upload fallback.
- Mandatory updates remain quiet and policy-controlled.
- Manual `Check for Updates` remains a normal user action.

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
