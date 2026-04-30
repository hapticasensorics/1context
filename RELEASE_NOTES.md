# 1Context v0.1.48 Public Preview

This release includes:

- professional local wiki web edge with bundled Caddy
- Swift-owned dynamic `/api/wiki/*` adapter for health, search, state, bookmarks, and chat capability shell
- last-good static wiki publishing with bundled polished wiki templates
- first-party Claude and Codex hook install/repair using the live canonical wiki URL
- removal of the old Python wiki server from the public release package
- local web recovery when Caddy is stale or stopped

Install:

```bash
brew install --cask hapticasensorics/tap/1context
```

Known preview limits:

- macOS 13 Ventura or newer required
- Apple Silicon only
- chat/librarian execution is an API shell in this release
- cloud wiki sharing is not enabled yet; the local wiki is private
