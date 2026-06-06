#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage: scripts/build-managed-postgres-source.sh [options]

Builds a relocatable macOS arm64 managed Postgres runtime from source, stages it
into runtime/managed-postgres/macos-arm64, and runs the strict verifier + smoke.

Options:
  --work-dir DIR   Build/download/install work directory.
  --dest DIR       Destination bundle directory.
  --skip-smoke     Skip the live bundle smoke after staging.
  -h, --help       Show this help.

Version/env pins:
  ONECONTEXT_MANAGED_POSTGRES_VERSION     default: 17.10
  ONECONTEXT_MANAGED_TIMESCALEDB_VERSION  default: 2.27.2
  ONECONTEXT_MANAGED_PGVECTOR_VERSION     default: 0.8.2
  ONECONTEXT_MANAGED_OPENSSL_VERSION      default: 3.6.2

Optional checksum envs:
  ONECONTEXT_MANAGED_POSTGRES_SHA256
  ONECONTEXT_MANAGED_TIMESCALEDB_SHA256
  ONECONTEXT_MANAGED_PGVECTOR_SHA256
  ONECONTEXT_MANAGED_OPENSSL_SHA256
USAGE
}

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ARCH="$(uname -m)"
[[ "$ARCH" == "arm64" ]] || {
  echo "managed Postgres source build currently supports macOS arm64 only; found $ARCH" >&2
  exit 1
}

POSTGRES_VERSION="${ONECONTEXT_MANAGED_POSTGRES_VERSION:-17.10}"
TIMESCALEDB_VERSION="${ONECONTEXT_MANAGED_TIMESCALEDB_VERSION:-2.27.2}"
PGVECTOR_VERSION="${ONECONTEXT_MANAGED_PGVECTOR_VERSION:-0.8.2}"
OPENSSL_VERSION="${ONECONTEXT_MANAGED_OPENSSL_VERSION:-3.6.2}"
WORK_DIR="${ONECONTEXT_MANAGED_PG_BUILD_DIR:-/tmp/onecontext-managed-postgres-source/macos-arm64}"
DEST="runtime/managed-postgres/macos-arm64"
SKIP_SMOKE=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --work-dir)
      WORK_DIR="${2:?missing value for --work-dir}"
      shift 2
      ;;
    --dest)
      DEST="${2:?missing value for --dest}"
      shift 2
      ;;
    --skip-smoke)
      SKIP_SMOKE=1
      shift
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

DEST_ABS="$DEST"
case "$DEST_ABS" in
  /*) ;;
  *) DEST_ABS="$ROOT/$DEST_ABS" ;;
esac

DOWNLOADS="$WORK_DIR/downloads"
BUILD="$WORK_DIR/build"
PREFIX="$WORK_DIR/prefix"
OPENSSL_PREFIX="$WORK_DIR/openssl"
JOBS="${ONECONTEXT_MANAGED_PG_JOBS:-$(sysctl -n hw.ncpu 2>/dev/null || echo 4)}"

require_tool() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "required build tool missing: $1" >&2
    exit 1
  }
}

verify_sha256() {
  local file="$1"
  local expected="$2"
  [[ -n "$expected" ]] || {
    echo "warning: no sha256 pin provided for $(basename "$file"); source fetch is not release-approved yet" >&2
    return 0
  }
  local actual
  actual="$(shasum -a 256 "$file" | awk '{print $1}')"
  [[ "$actual" == "$expected" ]] || {
    echo "sha256 mismatch for $file" >&2
    echo "  expected: $expected" >&2
    echo "  actual:   $actual" >&2
    exit 1
  }
}

fetch() {
  local url="$1"
  local output="$2"
  local sha="$3"
  mkdir -p "$DOWNLOADS"
  if [[ ! -f "$output" ]]; then
    curl --fail --location --retry 3 --output "$output" "$url"
  fi
  verify_sha256 "$output" "$sha"
}

extract_once() {
  local archive="$1"
  local marker="$2"
  mkdir -p "$BUILD"
  if [[ ! -e "$marker" ]]; then
    tar -xf "$archive" -C "$BUILD"
  fi
}

require_tool curl
require_tool make
require_tool clang
require_tool cmake

POSTGRES_ARCHIVE="$DOWNLOADS/postgresql-$POSTGRES_VERSION.tar.bz2"
OPENSSL_ARCHIVE="$DOWNLOADS/openssl-$OPENSSL_VERSION.tar.gz"
TIMESCALE_ARCHIVE="$DOWNLOADS/timescaledb-$TIMESCALEDB_VERSION.tar.gz"
PGVECTOR_ARCHIVE="$DOWNLOADS/pgvector-$PGVECTOR_VERSION.tar.gz"

fetch \
  "https://ftp.postgresql.org/pub/source/v$POSTGRES_VERSION/postgresql-$POSTGRES_VERSION.tar.bz2" \
  "$POSTGRES_ARCHIVE" \
  "${ONECONTEXT_MANAGED_POSTGRES_SHA256:-}"
fetch \
  "https://www.openssl.org/source/openssl-$OPENSSL_VERSION.tar.gz" \
  "$OPENSSL_ARCHIVE" \
  "${ONECONTEXT_MANAGED_OPENSSL_SHA256:-}"
fetch \
  "https://github.com/timescale/timescaledb/archive/refs/tags/$TIMESCALEDB_VERSION.tar.gz" \
  "$TIMESCALE_ARCHIVE" \
  "${ONECONTEXT_MANAGED_TIMESCALEDB_SHA256:-}"
fetch \
  "https://github.com/pgvector/pgvector/archive/refs/tags/v$PGVECTOR_VERSION.tar.gz" \
  "$PGVECTOR_ARCHIVE" \
  "${ONECONTEXT_MANAGED_PGVECTOR_SHA256:-}"

extract_once "$OPENSSL_ARCHIVE" "$BUILD/openssl-$OPENSSL_VERSION"
extract_once "$POSTGRES_ARCHIVE" "$BUILD/postgresql-$POSTGRES_VERSION"
extract_once "$TIMESCALE_ARCHIVE" "$BUILD/timescaledb-$TIMESCALEDB_VERSION"
extract_once "$PGVECTOR_ARCHIVE" "$BUILD/pgvector-$PGVECTOR_VERSION"

if [[ ! -f "$OPENSSL_PREFIX/lib/libssl.3.dylib" ]]; then
  rm -rf "$OPENSSL_PREFIX"
  (
    cd "$BUILD/openssl-$OPENSSL_VERSION"
    make clean >/dev/null 2>&1 || true
    ./Configure darwin64-arm64-cc no-tests \
      --prefix="$OPENSSL_PREFIX" \
      --openssldir="$OPENSSL_PREFIX/ssl"
    make -j"$JOBS"
    make install_sw
  )
fi

if [[ ! -x "$PREFIX/bin/postgres" ]]; then
  (
    cd "$BUILD/postgresql-$POSTGRES_VERSION"
    env \
      CPPFLAGS="-I$OPENSSL_PREFIX/include" \
      LDFLAGS="-L$OPENSSL_PREFIX/lib" \
      ./configure \
        --prefix="$PREFIX" \
        --with-openssl \
        --without-readline \
        --without-zlib \
        --without-icu \
        --without-libxml \
        --without-libxslt \
        --without-ldap \
        --without-pam \
        --without-gssapi \
        --disable-nls \
        --enable-thread-safety
    make -j"$JOBS"
    make install
    cp "$OPENSSL_PREFIX"/lib/libcrypto*.dylib "$PREFIX/lib/"
    cp "$OPENSSL_PREFIX"/lib/libssl*.dylib "$PREFIX/lib/"
    for contrib in btree_gist pgcrypto pg_trgm pg_stat_statements; do
      make -C "contrib/$contrib" -j"$JOBS"
      make -C "contrib/$contrib" install
    done
  )
fi

if [[ ! -f "$PREFIX/share/postgresql/extension/timescaledb.control" ]]; then
  (
    cd "$BUILD/timescaledb-$TIMESCALEDB_VERSION"
    rm -rf build
    export PG_CONFIG="$PREFIX/bin/pg_config"
    if [[ -x ./bootstrap ]]; then
      ./bootstrap -DAPACHE_ONLY=ON -DREGRESS_CHECKS=OFF -DWARNINGS_AS_ERRORS=OFF
    else
      ./bootstrap.sh -DAPACHE_ONLY=ON -DREGRESS_CHECKS=OFF -DWARNINGS_AS_ERRORS=OFF
    fi
    make -C build -j"$JOBS"
    make -C build install
  )
fi

if [[ ! -f "$PREFIX/share/postgresql/extension/vector.control" ]]; then
  (
    cd "$BUILD/pgvector-$PGVECTOR_VERSION"
    make PG_CONFIG="$PREFIX/bin/pg_config" -j"$JOBS"
    make PG_CONFIG="$PREFIX/bin/pg_config" install
  )
fi

"$ROOT/scripts/stage-managed-postgres.sh" \
  --postgres-prefix "$PREFIX" \
  --pgvector-prefix "$PREFIX" \
  --timescaledb-prefix "$PREFIX" \
  --dest "$DEST_ABS"

if [[ "$SKIP_SMOKE" != "1" ]]; then
  "$ROOT/scripts/smoke-managed-postgres-bundle.sh" --run "$DEST_ABS"
fi

echo "source-built managed Postgres bundle staged at $DEST_ABS"
