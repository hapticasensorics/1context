# 1Context v0.1.63 Public Preview

This release includes:

- `0.1.63` is a mandatory updater-health release.
- Automatic update checks no longer show the support popup when no update was
  found and no install started.
- Automatic check-only failures retry silently and then stay quiet if the app is
  already current.
- Real failed update or install attempts still keep the old app usable and show
  simple support copy after retries.
- Manual `Check for Updates` remains a normal user action and can say
  `1Context is up to date.`
- Release proof now includes a broken-appcast check-only smoke that verifies no
  support alert is shown.

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
