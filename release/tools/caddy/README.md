# Bundled Caddy Release Tool

1Context release builds vendor Caddy as a pinned release-owned artifact so
prototype, private, and official `.app`/DMG builds do not depend on Homebrew or
the runner's `PATH`.

The current artifact is:

- `darwin-arm64/caddy-v2.11.2-darwin-arm64.tar.gz`
- Source binary: Caddy `v2.11.2`, Apache-2.0
- Contents copied into the app bundle: `caddy`, `LICENSE`, and `AUTHORS`

Dev builds may still use a host Caddy for local convenience. Non-dev release
channels must extract this artifact, verify its SHA-256 file, and bundle that
binary.
