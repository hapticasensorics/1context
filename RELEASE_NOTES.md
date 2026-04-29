# 1Context v0.1.46 Public Preview

This release includes:

- managed Claude `SessionStart` hook installs with an explicit matcher
- SessionStart hook context now includes a runtime timestamp from the 1Context daemon
- runtime health RPC now reports the daemon's current time
- tests for hook matcher installation and daemon-backed hook context

Install:

```bash
brew install --cask hapticasensorics/tap/1context
```

Known preview limits:

- macOS 13 Ventura or newer required
- Apple Silicon only
- project wiki, MCP, and agent memory surfaces are still in active development
