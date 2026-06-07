#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage: scripts/build-managed-postgres-runtime.sh [options]

Builds a relocatable managed Postgres runtime from source, then stages it into
release/managed-postgres/runtime/macos-arm64. This is the user-shippable path: the
resulting bundle must not depend on Homebrew, MacPorts, or machine-local
Postgres install paths at runtime.

Options:
  --dest DIR                  Destination bundle directory.
  --work-dir DIR              Build workspace directory.
  --postgres-version VERSION  PostgreSQL version to build (default: 17.10).
  --timescaledb-version VER   TimescaleDB version to build (default: 2.27.2).
  --pgvector-version VERSION  pgvector version to build (default: 0.8.2).
  --openssl-version VERSION   OpenSSL version to build (default: 3.6.2).
  --source-lock FILE          Pinned source URL/checksum lock file.
  --openssl-prefix DIR        Existing static OpenSSL prefix for pgcrypto.
  --jobs N                    Parallel build jobs (default: min(hw.ncpu, 6)).
  --keep-work                 Keep the build workspace after success.
  --no-smoke                  Skip the runtime smoke test.
  -h, --help                  Show this help.
USAGE
}

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEST="release/managed-postgres/runtime/macos-arm64"
WORK_DIR="$ROOT/.tmp-managed-postgres-build"
POSTGRES_VERSION="17.10"
TIMESCALEDB_VERSION="2.27.2"
PGVECTOR_VERSION="0.8.2"
OPENSSL_VERSION="3.6.2"
SOURCE_LOCK="$ROOT/release/managed-postgres/sources.macos-arm64.json"
OPENSSL_PREFIX="${ONECONTEXT_MANAGED_OPENSSL_PREFIX:-}"
DEFAULT_JOBS="$(sysctl -n hw.ncpu 2>/dev/null || echo 4)"
if [[ "$DEFAULT_JOBS" -gt 6 ]]; then
  DEFAULT_JOBS=6
fi
JOBS="${ONECONTEXT_MANAGED_POSTGRES_BUILD_JOBS:-$DEFAULT_JOBS}"
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
    --source-lock)
      SOURCE_LOCK="${2:?missing value for --source-lock}"
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
require_tool make
require_tool python3
require_tool shasum
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
POSTGRES_URL=""
POSTGRES_SHA256=""
TIMESCALE_SRC="$SRC_DIR/timescaledb-$TIMESCALEDB_VERSION"
TIMESCALE_TARBALL="$DOWNLOAD_DIR/timescaledb-$TIMESCALEDB_VERSION.tar.gz"
TIMESCALE_URL=""
TIMESCALE_SHA256=""
PGVECTOR_SRC="$SRC_DIR/pgvector-$PGVECTOR_VERSION"
PGVECTOR_TARBALL="$DOWNLOAD_DIR/pgvector-$PGVECTOR_VERSION.tar.gz"
PGVECTOR_URL=""
PGVECTOR_SHA256=""
OPENSSL_TARBALL="$DOWNLOAD_DIR/openssl-$OPENSSL_VERSION.tar.gz"
OPENSSL_URL=""
OPENSSL_SHA256=""
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

lock_value() {
  local component="$1"
  local field="$2"
  [[ -f "$SOURCE_LOCK" ]] || fail "managed Postgres source lock is missing: $SOURCE_LOCK"
  /usr/bin/python3 - "$SOURCE_LOCK" "$component" "$field" <<'PY'
import json
import sys

path, component_name, field = sys.argv[1:]
with open(path, "r", encoding="utf-8") as handle:
    data = json.load(handle)
if data.get("schema") != "1context.managed-postgres.sources.v1":
    raise SystemExit(f"unsupported source lock schema: {data.get('schema')!r}")
if data.get("arch") != "macos-arm64":
    raise SystemExit(f"source lock arch must be macos-arm64, got {data.get('arch')!r}")
for component in data.get("components", []):
    if component.get("name") == component_name:
        value = component.get(field)
        if not isinstance(value, str) or not value:
            raise SystemExit(f"{component_name}.{field} is missing in {path}")
        print(value)
        break
else:
    raise SystemExit(f"{component_name} is missing in {path}")
PY
}

load_locked_component() {
  local component="$1"
  local expected_version="$2"
  local version
  version="$(lock_value "$component" version)"
  [[ "$version" == "$expected_version" ]] || fail "$component version $expected_version does not match source lock version $version"
}

load_locked_component postgresql "$POSTGRES_VERSION"
POSTGRES_URL="$(lock_value postgresql url)"
POSTGRES_SHA256="$(lock_value postgresql sha256)"
load_locked_component openssl "$OPENSSL_VERSION"
OPENSSL_URL="$(lock_value openssl url)"
OPENSSL_SHA256="$(lock_value openssl sha256)"
load_locked_component timescaledb "$TIMESCALEDB_VERSION"
TIMESCALE_URL="$(lock_value timescaledb url)"
TIMESCALE_SHA256="$(lock_value timescaledb sha256)"
load_locked_component pgvector "$PGVECTOR_VERSION"
PGVECTOR_URL="$(lock_value pgvector url)"
PGVECTOR_SHA256="$(lock_value pgvector sha256)"

download_verified() {
  local url="$1"
  local dest="$2"
  local expected_sha="$3"
  local label="$4"
  curl -fL "$url" -o "$dest"
  local actual_sha
  actual_sha="$(shasum -a 256 "$dest" | awk '{print $1}')"
  if [[ -z "$expected_sha" || "$actual_sha" != "$expected_sha" ]]; then
    echo "$label checksum mismatch." >&2
    echo "Expected: $expected_sha" >&2
    echo "Actual:   $actual_sha" >&2
    exit 1
  fi
}

write_bundle_sbom() {
  local dest="$1"
  /usr/bin/python3 - "$SOURCE_LOCK" "$dest/manifest.json" "$dest/managed-postgres-sbom.json" <<'PY'
import json
import sys
from datetime import datetime, timezone

source_lock_path, manifest_path, output_path = sys.argv[1:]
with open(source_lock_path, "r", encoding="utf-8") as handle:
    source_lock = json.load(handle)
with open(manifest_path, "r", encoding="utf-8") as handle:
    manifest = json.load(handle)

payload = {
    "schema": "1context.managed-postgres.sbom.v1",
    "arch": source_lock.get("arch"),
    "generated_at": datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
    "bundle": {
        "postgres_major": manifest.get("postgres_major"),
        "postgres_version": manifest.get("postgres_version"),
        "timescale_version": manifest.get("timescale_version"),
        "build_id": manifest.get("build_id"),
    },
    "source_lock": {
        "schema": source_lock.get("schema"),
        "path": "release/managed-postgres/sources.macos-arm64.json",
    },
    "components": source_lock.get("components", []),
}
with open(output_path, "w", encoding="utf-8") as handle:
    json.dump(payload, handle, indent=2, sort_keys=True)
    handle.write("\n")
PY
}

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
echo "  lock:      $SOURCE_LOCK"

if [[ "$BUILD_OPENSSL_FROM_SOURCE" == "1" ]]; then
  download_verified "$OPENSSL_URL" "$OPENSSL_TARBALL" "$OPENSSL_SHA256" "OpenSSL"
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

download_verified "$POSTGRES_URL" "$POSTGRES_TARBALL" "$POSTGRES_SHA256" "PostgreSQL"
tar -C "$SRC_DIR" -xjf "$POSTGRES_TARBALL"

download_verified "$TIMESCALE_URL" "$TIMESCALE_TARBALL" "$TIMESCALE_SHA256" "TimescaleDB"
tar -C "$SRC_DIR" -xzf "$TIMESCALE_TARBALL"
download_verified "$PGVECTOR_URL" "$PGVECTOR_TARBALL" "$PGVECTOR_SHA256" "pgvector"
tar -C "$SRC_DIR" -xzf "$PGVECTOR_TARBALL"

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

write_bundle_sbom "$DEST"
"$ROOT/scripts/verify-managed-postgres-bundle.sh" --require-sbom "$DEST"
if [[ "$RUN_SMOKE" == "1" ]]; then
  "$ROOT/scripts/smoke-managed-postgres-bundle.sh" --run "$DEST"
fi

echo "managed Postgres source-built runtime staged at $DEST"
