#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage: scripts/build-managed-postgres-runtime.sh [options]

Builds a relocatable managed Postgres runtime from source, then stages it into
runtime/managed-postgres/macos-arm64. This is the user-shippable path: the
resulting bundle must not depend on Homebrew, MacPorts, or machine-local
Postgres install paths at runtime.

Options:
  --dest DIR                  Destination bundle directory.
  --work-dir DIR              Build workspace directory.
  --postgres-version VERSION  PostgreSQL version to build (default: 17.10).
  --timescaledb-version VER   TimescaleDB version to build (default: 2.27.2).
  --pgvector-version VERSION  pgvector version to build (default: 0.8.2).
  --openssl-version VERSION   OpenSSL version to build (default: 3.6.2).
  --openssl-prefix DIR        Existing static OpenSSL prefix for pgcrypto.
  --jobs N                    Parallel build jobs (default: hw.ncpu).
  --keep-work                 Keep the build workspace after success.
  --no-smoke                  Skip the runtime smoke test.
  -h, --help                  Show this help.
USAGE
}

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEST="runtime/managed-postgres/macos-arm64"
WORK_DIR="$ROOT/.tmp-managed-postgres-build"
POSTGRES_VERSION="17.10"
TIMESCALEDB_VERSION="2.27.2"
PGVECTOR_VERSION="0.8.2"
OPENSSL_VERSION="3.6.2"
OPENSSL_PREFIX="${ONECONTEXT_MANAGED_OPENSSL_PREFIX:-}"
JOBS="$(sysctl -n hw.ncpu 2>/dev/null || echo 4)"
KEEP_WORK=0
RUN_SMOKE=1

while [[ $# -gt 0 ]]; do
  case "$1" in
    --dest)
      DEST="${2:?missing value for --dest}"
      shift 2
      ;;
    --work-dir)
      WORK_DIR="${2:?missing value for --work-dir}"
      shift 2
      ;;
    --postgres-version)
      POSTGRES_VERSION="${2:?missing value for --postgres-version}"
      shift 2
      ;;
    --timescaledb-version)
      TIMESCALEDB_VERSION="${2:?missing value for --timescaledb-version}"
      shift 2
      ;;
    --pgvector-version)
      PGVECTOR_VERSION="${2:?missing value for --pgvector-version}"
      shift 2
      ;;
    --openssl-version)
      OPENSSL_VERSION="${2:?missing value for --openssl-version}"
      shift 2
      ;;
    --openssl-prefix)
      OPENSSL_PREFIX="${2:?missing value for --openssl-prefix}"
      shift 2
      ;;
    --jobs)
      JOBS="${2:?missing value for --jobs}"
      shift 2
      ;;
    --keep-work)
      KEEP_WORK=1
      shift
      ;;
    --no-smoke)
      RUN_SMOKE=0
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

fail() {
  echo "$*" >&2
  exit 1
}

require_tool() {
  command -v "$1" >/dev/null 2>&1 || fail "required build tool missing: $1"
}

require_tool clang
require_tool cmake
require_tool curl
require_tool git
require_tool make
require_tool tar

case "$(uname -m)" in
  arm64|aarch64) ;;
  *) fail "managed Postgres source build currently targets macOS arm64 only" ;;
esac

case "$(uname -s)" in
  Darwin) ;;
  *) fail "managed Postgres source build currently targets macOS only" ;;
esac

POSTGRES_MAJOR="${POSTGRES_VERSION%%.*}"
[[ "$POSTGRES_MAJOR" == "17" ]] || fail "managed runtime requires PostgreSQL 17, got $POSTGRES_VERSION"

WORK_DIR="$(cd "$(dirname "$WORK_DIR")" && pwd)/$(basename "$WORK_DIR")"
DOWNLOAD_DIR="$WORK_DIR/downloads"
SRC_DIR="$WORK_DIR/src"
PREFIX_DIR="$WORK_DIR/prefix"
POSTGRES_PREFIX="$PREFIX_DIR/runtime"
POSTGRES_TARBALL="$DOWNLOAD_DIR/postgresql-$POSTGRES_VERSION.tar.bz2"
POSTGRES_URL="https://ftp.postgresql.org/pub/source/v$POSTGRES_VERSION/postgresql-$POSTGRES_VERSION.tar.bz2"
TIMESCALE_SRC="$SRC_DIR/timescaledb-$TIMESCALEDB_VERSION"
PGVECTOR_SRC="$SRC_DIR/pgvector-$PGVECTOR_VERSION"
OPENSSL_TARBALL="$DOWNLOAD_DIR/openssl-$OPENSSL_VERSION.tar.gz"
OPENSSL_URL="https://www.openssl.org/source/openssl-$OPENSSL_VERSION.tar.gz"
OPENSSL_SRC="$SRC_DIR/openssl-$OPENSSL_VERSION"
BUILD_OPENSSL_FROM_SOURCE=0
if [[ -z "$OPENSSL_PREFIX" ]]; then
  OPENSSL_PREFIX="$PREFIX_DIR/openssl"
  BUILD_OPENSSL_FROM_SOURCE=1
fi

export CC="${CC:-clang}"
export MACOSX_DEPLOYMENT_TARGET="${MACOSX_DEPLOYMENT_TARGET:-14.0}"

rm -rf "$WORK_DIR"
mkdir -p "$DOWNLOAD_DIR" "$SRC_DIR" "$PREFIX_DIR"

cleanup() {
  local status="$?"
  if [[ "$status" != "0" ]]; then
    echo "managed Postgres source build failed; workdir preserved at $WORK_DIR" >&2
    return
  fi
  if [[ "$KEEP_WORK" != "1" ]]; then
    rm -rf "$WORK_DIR"
  else
    echo "managed Postgres source build workdir preserved at $WORK_DIR"
  fi
}
trap cleanup EXIT

echo "managed Postgres source build"
echo "  postgres:  $POSTGRES_VERSION"
echo "  timescale: $TIMESCALEDB_VERSION"
echo "  pgvector:  $PGVECTOR_VERSION"
if [[ "$BUILD_OPENSSL_FROM_SOURCE" == "1" ]]; then
  echo "  openssl:   $OPENSSL_VERSION (source build)"
else
  echo "  openssl:   $OPENSSL_PREFIX"
fi
echo "  work_dir:  $WORK_DIR"
echo "  dest:      $DEST"

if [[ "$BUILD_OPENSSL_FROM_SOURCE" == "1" ]]; then
  curl -fL "$OPENSSL_URL" -o "$OPENSSL_TARBALL"
  tar -C "$SRC_DIR" -xzf "$OPENSSL_TARBALL"
  pushd "$OPENSSL_SRC" >/dev/null
  ./Configure darwin64-arm64-cc no-shared no-tests no-apps \
    --prefix="$OPENSSL_PREFIX" \
    --openssldir="$OPENSSL_PREFIX/ssl"
  make -j "$JOBS"
  make install_sw
  popd >/dev/null
fi
[[ -f "$OPENSSL_PREFIX/include/openssl/evp.h" ]] || fail "OpenSSL headers missing: $OPENSSL_PREFIX/include/openssl/evp.h"
[[ -f "$OPENSSL_PREFIX/lib/libcrypto.a" ]] || fail "static libcrypto missing: $OPENSSL_PREFIX/lib/libcrypto.a"

curl -fL "$POSTGRES_URL" -o "$POSTGRES_TARBALL"
tar -C "$SRC_DIR" -xjf "$POSTGRES_TARBALL"

git clone --depth 1 --branch "$TIMESCALEDB_VERSION" \
  https://github.com/timescale/timescaledb.git "$TIMESCALE_SRC"
git clone --depth 1 --branch "v$PGVECTOR_VERSION" \
  https://github.com/pgvector/pgvector.git "$PGVECTOR_SRC"

pushd "$SRC_DIR/postgresql-$POSTGRES_VERSION" >/dev/null
./configure \
  --prefix="$POSTGRES_PREFIX" \
  --disable-rpath \
  --without-bonjour \
  --without-gssapi \
  --without-icu \
  --without-ldap \
  --without-libxml \
  --without-libxslt \
  --without-llvm \
  --without-pam \
  --without-perl \
  --without-readline \
  --without-tcl
make -j "$JOBS"
make install
for contrib in btree_gist pg_trgm pg_stat_statements; do
  make -C "contrib/$contrib" install
done
make -C contrib/pgcrypto \
  PG_CPPFLAGS="-I$OPENSSL_PREFIX/include" \
  LDFLAGS="-L$OPENSSL_PREFIX/lib" \
  LIBS="-lcrypto -lz" \
  install
popd >/dev/null

PG_CONFIG="$POSTGRES_PREFIX/bin/pg_config"
[[ -x "$PG_CONFIG" ]] || fail "pg_config missing after PostgreSQL install: $PG_CONFIG"

pushd "$PGVECTOR_SRC" >/dev/null
make clean >/dev/null 2>&1 || true
make -j "$JOBS" PG_CONFIG="$PG_CONFIG" OPTFLAGS=""
make install PG_CONFIG="$PG_CONFIG" OPTFLAGS=""
popd >/dev/null

BUILD_FORCE_REMOVE=true BUILD_DIR="$TIMESCALE_SRC/build" \
  "$TIMESCALE_SRC/bootstrap" \
    -DCMAKE_BUILD_TYPE=Release \
    -DPG_CONFIG="$PG_CONFIG" \
    -DREGRESS_CHECKS=OFF \
    -DUSE_OPENSSL=0 \
    -DWARNINGS_AS_ERRORS=OFF
cmake --build "$TIMESCALE_SRC/build" --parallel "$JOBS" --target install

"$ROOT/scripts/stage-managed-postgres.sh" \
  --dest "$DEST" \
  --postgres-prefix "$POSTGRES_PREFIX" \
  --pgvector-prefix "$POSTGRES_PREFIX" \
  --timescaledb-prefix "$POSTGRES_PREFIX"

"$ROOT/scripts/verify-managed-postgres-bundle.sh" "$DEST"
if [[ "$RUN_SMOKE" == "1" ]]; then
  "$ROOT/scripts/smoke-managed-postgres-bundle.sh" --run "$DEST"
fi

echo "managed Postgres source-built runtime staged at $DEST"
