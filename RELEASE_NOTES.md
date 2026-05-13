# 1Context v0.1.65 Public Preview

This release includes:

- `0.1.65` is the blessed professional-app release for this update train.
- Releases now run through a manifest-driven protected release path with
  signing, notarization, asset audit, runner attestation, and redacted evidence.
- Mandatory Sparkle updates remain quiet and automatic.
- Manual `Check for Updates` remains a normal user action.
- The app keeps the existing memory runtime usable while update work happens.

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
