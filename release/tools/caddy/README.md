# Bundled Caddy Release Tool

1Context release builds use a checksum-pinned Caddy archive downloaded into
`dist/release-tools/` so prototype, private, and official `.app`/DMG builds do
not depend on Homebrew, the runner's `PATH`, or a checked-in binary tarball.

The current artifact is:

- URL: `https://github.com/caddyserver/caddy/releases/download/v2.11.2/caddy-v2.11.2-darwin-arm64.tar.gz`
- checksum pin: `darwin-arm64/caddy-v2.11.2-darwin-arm64.tar.gz.sha256`
- Source binary: Caddy `v2.11.2`, Apache-2.0
- Contents copied into the app bundle: `caddy`, `LICENSE`, and `AUTHORS`

Dev builds may still use a host Caddy for local convenience. Non-dev release
channels must download/cache this artifact, verify its SHA-256 file, and bundle
that binary.
