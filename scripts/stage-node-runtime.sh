#!/usr/bin/env bash
set -euo pipefail

# Pinned Node.js LTS runtime for the bundled wiki renderer. The app must never
# depend on a host-installed node for post-install wiki re-renders, so release
# builds stage this runtime and scripts/build-macos-app.sh copies it into
# Contents/Resources/node-runtime when it is present.
NODE_VERSION="24.16.0"
NODE_ARCH="darwin-arm64"
NODE_ARCHIVE_NAME="node-v$NODE_VERSION-$NODE_ARCH.tar.gz"
NODE_ARCHIVE_SHA256="39189dab4eeb15706c424af0ac08a3044c9e48f7db12a7d77f6b7aafc7dd5df6"
NODE_ARCHIVE_URL="https://nodejs.org/dist/v$NODE_VERSION/$NODE_ARCHIVE_NAME"

usage() {
  cat <<USAGE
Usage: scripts/stage-node-runtime.sh [options]

Stages a repo-local pinned Node.js runtime (v$NODE_VERSION, $NODE_ARCH) for the
macOS app wiki renderer. Downloads the official tarball, verifies its pinned
sha256, and extracts only bin/node plus the Node.js LICENSE into the staging
directory. npm, corepack, headers, and docs are intentionally not staged.

Options:
  --dest DIR   Destination runtime directory
               (default: release/node-runtime/macos-arm64)
  -h, --help   Show this help.
USAGE
}

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEST="$ROOT/release/node-runtime/macos-arm64"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --dest)
      DEST="${2:?missing value for --dest}"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

fail() {
  echo "$*" >&2
  exit 1
}

ARCHIVE_DIR="$ROOT/dist/release-tools/node/downloads/$NODE_ARCH"
ARCHIVE="$ARCHIVE_DIR/$NODE_ARCHIVE_NAME"
WORK_DIR="$ROOT/dist/release-tools/node/$NODE_ARCH"

if [[ ! -f "$ARCHIVE" ]]; then
  mkdir -p "$ARCHIVE_DIR"
  curl -fsSL "$NODE_ARCHIVE_URL" -o "$ARCHIVE"
fi

ACTUAL_SHA256="$(shasum -a 256 "$ARCHIVE" | awk '{print $1}')"
if [[ "$ACTUAL_SHA256" != "$NODE_ARCHIVE_SHA256" ]]; then
  rm -f "$ARCHIVE"
  {
    echo "Pinned Node.js artifact checksum mismatch for $NODE_ARCHIVE_NAME."
    echo "Expected: $NODE_ARCHIVE_SHA256"
    echo "Actual:   $ACTUAL_SHA256"
  } >&2
  exit 1
fi

rm -rf "$WORK_DIR"
mkdir -p "$WORK_DIR"
tar -xzf "$ARCHIVE" -C "$WORK_DIR" \
  "node-v$NODE_VERSION-$NODE_ARCH/bin/node" \
  "node-v$NODE_VERSION-$NODE_ARCH/LICENSE"

EXTRACTED="$WORK_DIR/node-v$NODE_VERSION-$NODE_ARCH"
[[ -f "$EXTRACTED/bin/node" ]] || fail "node binary missing from extracted archive: $EXTRACTED/bin/node"

rm -rf "$DEST"
mkdir -p "$DEST/bin"
cp "$EXTRACTED/bin/node" "$DEST/bin/node"
cp "$EXTRACTED/LICENSE" "$DEST/LICENSE"
chmod 755 "$DEST/bin/node"

STAGED_VERSION="$("$DEST/bin/node" --version)"
[[ "$STAGED_VERSION" == "v$NODE_VERSION" ]] \
  || fail "staged node reports $STAGED_VERSION, expected v$NODE_VERSION"
printf '%s\n' "$STAGED_VERSION" > "$DEST/node.version"

echo "node runtime staged at $DEST"
echo "node:    $STAGED_VERSION ($NODE_ARCH)"
echo "sha256:  $NODE_ARCHIVE_SHA256 ($NODE_ARCHIVE_NAME)"
