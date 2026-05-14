# 1Context v0.1.84 Public Preview

This release includes:

- `0.1.84` finishes the release-factory setup authorization repair. The
  protected runner now targets the visible macOS Certificate Trust Settings
  sheet, focuses the secure password field, sets the field value directly
  through Accessibility, and clicks `Update Settings`.
- A focused password-entry harness proves the exact setup prompt in isolation
  before the slower release proof runs.
- The proof contract now explicitly preserves the product boundary: an already
  setup user should not need sudo or a fresh Local Wiki Access grant during an
  app update.
- The protected runner can use an explicit admin-authorization secret only for
  setup and reinstall proof prompts, so the release lane can automate the same
  first-setup macOS prompt a tester sees without making updates depend on sudo.
- Signed app setup ignores stale runner `SUDO_USER` environment variables
  when the app is actually running as the logged-in macOS user, so Local Wiki
  Access can finish from the real setup window.
- The official proof runner treats setup restoration as a closed-loop GUI
  workflow, captures each `Grant` / `Check Again` action, and keeps checking
  diagnose until Local Wiki Access and runtime health are actually restored.
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
