# Roadmap

1Context is a signed macOS app with a private local wiki, app-owned setup,
Sparkle updates, uninstall support, and a small bundled runtime. The next work
is less about adding basic app plumbing and more about proving the release
flywheel and turning the memory system on safely.

## Release Flywheel

- Keep shipping through the signed, notarized DMG and Homebrew cask channels.
- Keep Sparkle as the in-app update engine and `1context-cli` as the support and
  automation surface.
- Finish real installed-app proof for update hops, restart/login recovery,
  setup repair, non-destructive uninstall, and controlled delete-data uninstall.
- Use the self-hosted Mac runner for mandatory updates, Sparkle metadata
  changes, LaunchAgent/local-web changes, repair releases, and launch-candidate
  rehearsals.
- Keep release class, updater copy, failure copy, post-install messaging, and
  menu behavior under the founder-controlled update policy.

## Permissions And Setup

- Treat Local Wiki Access as required setup for the current product.
- Request future sensitive permissions only from app-owned setup surfaces, at
  the moment a shipped feature needs them.
- Make blocked actions open the relevant permission or setup flow instead of
  failing silently.
- Keep setup state live: if permissions or local HTTPS are already granted, the
  app should recognize that without requiring a manual recheck.

## Wiki And Memory

- Keep the installed wiki focused on user-facing system-shell pages and
  user-owned content. Development goals and operator checklists live in `docs/`.
- Preserve the clean boundary between `~/1Context` user content and app-owned
  generated or shipped wiki state under Application Support.
- Turn on passive capture and memory writing only behind explicit product
  permission decisions and deterministic proof.
- Reintroduce memory-writing engines only as explicit runtime artifacts with
  release proof. Do not ship a source checkout as the app runtime.

## Cloud And Sharing

- Keep browser code on relative `/api/wiki/*` routes so local and future cloud
  hosting share the same contract.
- Add cloud wiki sharing only after the local ownership and consent model stays
  clear.
- Avoid coupling browser pages to developer ports, source checkouts, or
  machine-specific paths.

## Current Sources

- Release operations: [docs/macos-release-runbook.md](docs/macos-release-runbook.md)
- Update policy: [docs/update_policy.html](docs/update_policy.html)
- App boundaries: [docs/macos-app-architecture.md](docs/macos-app-architecture.md)
- Local web contract: [docs/local-web-contract.md](docs/local-web-contract.md)
- Permissions contract: [PERMISSIONS.md](PERMISSIONS.md)
