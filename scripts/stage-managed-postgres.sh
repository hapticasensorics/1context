#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage: scripts/stage-managed-postgres.sh [options]

Stages a repo-local managed Postgres bundle for the macOS app. By default it
tries to assemble the bundle from locally installed PostgreSQL 17, pgvector,
and TimescaleDB prefixes. If --source is passed, it copies an already assembled
bundle directory instead.

Options:
  --source DIR             Copy an already assembled bundle directory.
  --dest DIR               Destination bundle directory.
  --postgres-prefix DIR    PostgreSQL 17 prefix to stage from.
  --pgvector-prefix DIR    pgvector prefix to stage from.
  --timescaledb-prefix DIR TimescaleDB prefix to stage from.
  -h, --help               Show this help.
USAGE
}

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PATH="/opt/homebrew/bin:/usr/local/bin:$PATH"

SOURCE=""
DEST="runtime/managed-postgres/macos-arm64"
POSTGRES_PREFIX="${ONECONTEXT_MANAGED_POSTGRESQL_PREFIX:-}"
PGVECTOR_PREFIX="${ONECONTEXT_MANAGED_PGVECTOR_PREFIX:-}"
TIMESCALEDB_PREFIX="${ONECONTEXT_MANAGED_TIMESCALEDB_PREFIX:-}"
EXTENSION_REL_DIR="share/postgresql/extension"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --source)
      SOURCE="${2:?missing value for --source}"
      shift 2
      ;;
    --dest)
      DEST="${2:?missing value for --dest}"
      shift 2
      ;;
    --postgres-prefix)
      POSTGRES_PREFIX="${2:?missing value for --postgres-prefix}"
      shift 2
      ;;
    --pgvector-prefix)
      PGVECTOR_PREFIX="${2:?missing value for --pgvector-prefix}"
      shift 2
      ;;
    --timescaledb-prefix)
      TIMESCALEDB_PREFIX="${2:?missing value for --timescaledb-prefix}"
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

trim() {
  printf '%s' "$1" | awk '{$1=$1};1'
}

discover_brew_prefix() {
  local formula="$1"
  if ! command -v brew >/dev/null 2>&1; then
    return 1
  fi
  if ! brew list --formula "$formula" >/dev/null 2>&1; then
    return 1
  fi
  local prefix
  prefix="$(brew --prefix "$formula" 2>/dev/null || true)"
  [[ -n "$prefix" && -d "$prefix" ]] || return 1
  printf '%s\n' "$prefix"
}

require_prefix() {
  local label="$1"
  local value="$2"
  [[ -n "$value" ]] || fail "missing ${label} prefix"
  [[ -d "$value" ]] || fail "${label} prefix does not exist: $value"
}

find_matching_files() {
  local prefix="$1"
  local pattern="$2"
  [[ -d "$prefix" ]] || return 0
  local matches
  matches="$(find -L "$prefix" -type f -name "$pattern" -print | LC_ALL=C sort)"
  if [[ -n "${POSTGRES_MAJOR:-}" ]]; then
    local major_matches
    major_matches="$(printf '%s\n' "$matches" | grep "/postgresql@${POSTGRES_MAJOR}/" || true)"
    if [[ -n "$major_matches" ]]; then
      printf '%s\n' "$major_matches"
      return 0
    fi
  fi
  if [[ -n "$matches" ]]; then
    printf '%s\n' "$matches"
  fi
}

find_first_matching_file() {
  local prefix="$1"
  local pattern="$2"
  find_matching_files "$prefix" "$pattern" | sed -n '1p'
}

copy_unique_match() {
  local source_prefix="$1"
  local pattern="$2"
  local dest_dir="$3"
  local label="$4"
  local match
  match="$(find_first_matching_file "$source_prefix" "$pattern")"
  [[ -n "$match" ]] || fail "could not find ${label} (${pattern}) under $source_prefix"
  mkdir -p "$dest_dir"
  rm -f "$dest_dir/$(basename "$match")"
  cp "$match" "$dest_dir/$(basename "$match")"
  chmod u+w "$dest_dir/$(basename "$match")" 2>/dev/null || true
}

copy_matching_files() {
  local source_prefix="$1"
  local pattern="$2"
  local dest_dir="$3"
  local label="$4"
  mapfile -t matches < <(find_matching_files "$source_prefix" "$pattern")
  [[ ${#matches[@]} -gt 0 ]] || fail "could not find ${label} (${pattern}) under $source_prefix"
  mkdir -p "$dest_dir"
  local match
  for match in "${matches[@]}"; do
    rm -f "$dest_dir/$(basename "$match")"
    cp "$match" "$dest_dir/$(basename "$match")"
    chmod u+w "$dest_dir/$(basename "$match")" 2>/dev/null || true
  done
}

copy_extension_assets() {
  local source_prefix="$1"
  local extension_name="$2"
  local require_library="$3"
  local extension_dir="$DEST/$EXTENSION_REL_DIR"
  local library_dir="$DEST/lib/postgresql"
  copy_unique_match "$source_prefix" "${extension_name}.control" "$extension_dir" "${extension_name} control file"
  copy_matching_files "$source_prefix" "${extension_name}--*.sql" "$extension_dir" "${extension_name} SQL files"
  if [[ "$require_library" == "1" ]]; then
    copy_matching_files "$source_prefix" "${extension_name}*.dylib" "$library_dir" "${extension_name} dylibs"
  fi
}

prune_release_forbidden_bundle_artifacts() {
  find "$DEST" -type f \( \
    -name '*.a' \
    -o -name '*.pc' \
    -o -path '*/pgxs/*' \
    -o -path '*/include/*' \
    -o -path '*/doc/*' \
    -o -path '*/man/*' \
  \) -delete 2>/dev/null || true
  find "$DEST" -type d -empty -delete 2>/dev/null || true
}

detect_timescaledb_prefix() {
  local postgres_prefix="$1"
  local explicit_prefix="$2"
  if [[ -n "$explicit_prefix" ]]; then
    printf '%s\n' "$explicit_prefix"
    return 0
  fi
  if [[ -n "$(find_first_matching_file "$postgres_prefix" 'timescaledb.control')" ]]; then
    printf '%s\n' "$postgres_prefix"
    return 0
  fi
  local candidate
  for candidate in \
    "$(discover_brew_prefix timescaledb 2>/dev/null || true)" \
    "$(discover_brew_prefix timescaledb-ha 2>/dev/null || true)"; do
    if [[ -n "$candidate" && -n "$(find_first_matching_file "$candidate" 'timescaledb.control')" ]]; then
      printf '%s\n' "$candidate"
      return 0
    fi
  done
  if [[ -d /opt/homebrew/opt || -d /usr/local/opt ]]; then
    candidate="$(find /opt/homebrew/opt /usr/local/opt -maxdepth 2 -type f -name 'timescaledb.control' -print 2>/dev/null | sed -n '1p' || true)"
    if [[ -n "$candidate" ]]; then
      dirname "$(dirname "$(dirname "$(dirname "$candidate")")")"
      return 0
    fi
  fi
  return 1
}

resolve_loader_path() {
  /usr/bin/python3 - "$1" "$2" <<'PY'
import os
import sys

from_path = os.path.abspath(sys.argv[1])
to_path = os.path.abspath(sys.argv[2])
print("@loader_path/" + os.path.relpath(to_path, os.path.dirname(from_path)))
PY
}

is_host_managed_dylib() {
  case "$1" in
    /opt/homebrew/opt/*|/opt/homebrew/Cellar/*|/usr/local/opt/*|/usr/local/Cellar/*)
      [[ "$1" == *.dylib ]]
      ;;
    *)
      return 1
      ;;
  esac
}

find_host_dylib_by_basename() {
  local basename_dep="$1"
  local roots=()
  [[ -d /opt/homebrew/opt ]] && roots+=( /opt/homebrew/opt )
  [[ -d /usr/local/opt ]] && roots+=( /usr/local/opt )
  [[ ${#roots[@]} -gt 0 ]] || return 0
  find -L "${roots[@]}" -type f -name "$basename_dep" -print 2>/dev/null | sed -n '1p' || true
}

is_mach_o() {
  file -b "$1" 2>/dev/null | grep -q 'Mach-O'
}

set_install_id() {
  local dylib="$1"
  local new_id
  new_id="$(resolve_loader_path "$dylib" "$dylib")"
  install_name_tool -id "$new_id" "$dylib"
}

sign_staged_mach_o_files() {
  command -v codesign >/dev/null 2>&1 || return 0
  while IFS= read -r mach_o; do
    is_mach_o "$mach_o" || continue
    codesign --force --sign - "$mach_o" >/dev/null
  done < <(find "$DEST/bin" "$DEST/lib" -type f -print)
}

rewrite_dependency_paths() {
  local mach_o="$1"
  local dep
  while IFS= read -r dep; do
    [[ -n "$dep" ]] || continue
    if [[ "$dep" == @loader_path/* ]]; then
      local relative_dep="${dep#@loader_path/}"
      local staged_target
      staged_target="$(dirname "$mach_o")/$relative_dep"
      if [[ ! -f "$staged_target" ]]; then
        local source_dep
        source_dep="$(find_host_dylib_by_basename "$(basename "$relative_dep")")"
        if [[ -n "$source_dep" ]]; then
          mkdir -p "$(dirname "$staged_target")"
          cp "$source_dep" "$staged_target"
          chmod u+w "$staged_target" 2>/dev/null || true
          STAGED_HOST_DEP_CHANGED=1
        fi
      fi
      continue
    fi
    case "$dep" in
      @*|/System/*|/usr/lib/*)
        continue
        ;;
    esac
    local staged_target=""
    if [[ -n "$POSTGRES_PREFIX" && "$dep" == "$POSTGRES_PREFIX/"* ]]; then
      staged_target="$DEST/${dep#$POSTGRES_PREFIX/}"
    elif [[ -n "$PGVECTOR_PREFIX" && "$dep" == "$PGVECTOR_PREFIX/"* ]]; then
      staged_target="$DEST/${dep#$PGVECTOR_PREFIX/}"
    elif [[ -n "$TIMESCALEDB_PREFIX" && "$dep" == "$TIMESCALEDB_PREFIX/"* ]]; then
      staged_target="$DEST/${dep#$TIMESCALEDB_PREFIX/}"
    elif is_host_managed_dylib "$dep"; then
      mkdir -p "$DEST/lib/deps"
      staged_target="$DEST/lib/deps/$(basename "$dep")"
      if [[ ! -f "$staged_target" ]]; then
        cp "$dep" "$staged_target"
        chmod u+w "$staged_target" 2>/dev/null || true
        STAGED_HOST_DEP_CHANGED=1
      fi
    else
      local basename_dep
      basename_dep="$(basename "$dep")"
      for staged_target in \
        "$DEST/lib/$basename_dep" \
        "$DEST/lib/postgresql/$basename_dep" \
        "$DEST/lib/deps/$basename_dep"; do
        [[ -f "$staged_target" ]] && break
      done
      [[ -f "$staged_target" ]] || staged_target=""
    fi
    [[ -n "$staged_target" && -f "$staged_target" ]] || continue
    install_name_tool -change "$dep" "$(resolve_loader_path "$mach_o" "$staged_target")" "$mach_o"
  done < <(otool -L "$mach_o" | tail -n +2 | awk '{print $1}')
}

write_manifest() {
  local postgres_version="$1"
  local postgres_major="$2"
  local timescale_version="$3"
  local build_id="$4"
  local extension_rel_dir="$5"
  cat > "$DEST/manifest.json" <<JSON
{
  "bundle_schema": 1,
  "arch": "arm64",
  "postgres_major": $postgres_major,
  "postgres_version": "$postgres_version",
  "timescale_version": "$timescale_version",
  "build_id": "$build_id",
  "bin": {
    "postgres": "bin/postgres",
    "initdb": "bin/initdb",
    "pg_ctl": "bin/pg_ctl",
    "pg_isready": "bin/pg_isready",
    "psql": "bin/psql",
    "createdb": "bin/createdb"
  },
  "extension": {
    "timescaledb_control": "$extension_rel_dir/timescaledb.control",
    "timescaledb_library_glob": "lib/postgresql/timescaledb*.dylib"
  },
  "required_extensions": [
    "timescaledb",
    "btree_gist",
    "pgcrypto",
    "pg_trgm",
    "vector",
    "pg_stat_statements"
  ],
  "required_preload_libraries": [
    "timescaledb",
    "pg_stat_statements"
  ]
}
JSON
}

if [[ -n "$SOURCE" ]]; then
  [[ -d "$SOURCE" ]] || fail "source bundle does not exist: $SOURCE"
  rm -rf "$DEST"
  mkdir -p "$DEST"
  rsync -a --delete "$SOURCE"/ "$DEST"/
  "$ROOT/scripts/verify-managed-postgres-bundle.sh" "$DEST"
  echo "managed Postgres bundle staged from existing source at $DEST"
  exit 0
fi

if [[ -z "$POSTGRES_PREFIX" ]]; then
  POSTGRES_PREFIX="$(discover_brew_prefix postgresql@17 2>/dev/null || true)"
fi
if [[ -z "$PGVECTOR_PREFIX" ]]; then
  PGVECTOR_PREFIX="$(discover_brew_prefix pgvector 2>/dev/null || true)"
fi
if ! TIMESCALEDB_PREFIX="$(detect_timescaledb_prefix "$POSTGRES_PREFIX" "$TIMESCALEDB_PREFIX" 2>/dev/null)"; then
  TIMESCALEDB_PREFIX=""
fi

missing=()
[[ -n "$POSTGRES_PREFIX" && -d "$POSTGRES_PREFIX" ]] || missing+=("PostgreSQL 17 prefix")
[[ -n "$PGVECTOR_PREFIX" && -d "$PGVECTOR_PREFIX" ]] || missing+=("pgvector prefix")
[[ -n "$TIMESCALEDB_PREFIX" && -d "$TIMESCALEDB_PREFIX" ]] || missing+=("TimescaleDB prefix")
if [[ ${#missing[@]} -gt 0 ]]; then
  {
    echo "could not stage managed Postgres bundle automatically."
    echo
    echo "Missing components:"
    printf '  - %s\n' "${missing[@]}"
    echo
    echo "Expected either:"
    echo "  - a real assembled bundle via --source DIR"
    echo "  - or local prefixes via --postgres-prefix, --pgvector-prefix, and --timescaledb-prefix"
    echo
    echo "Helpful commands:"
    echo "  brew install postgresql@17 pgvector timescale/tap/timescaledb"
    echo "  scripts/stage-managed-postgres.sh --postgres-prefix <pg17-prefix> --pgvector-prefix <pgvector-prefix> --timescaledb-prefix <timescaledb-prefix>"
  } >&2
  exit 1
fi

require_prefix "PostgreSQL 17" "$POSTGRES_PREFIX"
require_prefix "pgvector" "$PGVECTOR_PREFIX"
require_prefix "TimescaleDB" "$TIMESCALEDB_PREFIX"
[[ -x "$POSTGRES_PREFIX/bin/postgres" ]] || fail "postgres binary missing from PostgreSQL prefix: $POSTGRES_PREFIX/bin/postgres"

POSTGRES_VERSION_RAW="$("$POSTGRES_PREFIX/bin/postgres" --version)"
POSTGRES_VERSION="$(trim "${POSTGRES_VERSION_RAW#postgres (PostgreSQL) }")"
POSTGRES_MAJOR="${POSTGRES_VERSION%%.*}"
[[ "$POSTGRES_MAJOR" == "17" ]] || fail "managed bundle requires PostgreSQL 17, found: $POSTGRES_VERSION"

TIMESCALE_CONTROL_SOURCE="$(find_first_matching_file "$TIMESCALEDB_PREFIX" 'timescaledb.control')"
[[ -n "$TIMESCALE_CONTROL_SOURCE" ]] || fail "could not find timescaledb.control under $TIMESCALEDB_PREFIX"
TIMESCALE_VERSION="$(sed -nE "s/^[[:space:]]*default_version[[:space:]]*=[[:space:]]*'([^']+)'.*/\\1/p" "$TIMESCALE_CONTROL_SOURCE" | sed -n '1p')"
[[ -n "$TIMESCALE_VERSION" ]] || TIMESCALE_VERSION="unknown"
BUILD_ID="managed-pg${POSTGRES_MAJOR}-ts-$(printf '%s' "$TIMESCALE_VERSION" | tr -cs 'A-Za-z0-9._-' '-')-arm64"

POSTGRES_SHARE_SOURCE=""
POSTGRES_SHARE_REL_DIR=""
if [[ -f "$POSTGRES_PREFIX/share/postgres.bki" ]]; then
  POSTGRES_SHARE_SOURCE="$POSTGRES_PREFIX/share"
  POSTGRES_SHARE_REL_DIR="share"
elif [[ -f "$POSTGRES_PREFIX/share/postgresql/postgres.bki" ]]; then
  POSTGRES_SHARE_SOURCE="$POSTGRES_PREFIX/share/postgresql"
  POSTGRES_SHARE_REL_DIR="share/postgresql"
else
  fail "could not find postgres.bki under $POSTGRES_PREFIX/share or $POSTGRES_PREFIX/share/postgresql"
fi
EXTENSION_REL_DIR="$POSTGRES_SHARE_REL_DIR/extension"

rm -rf "$DEST"
mkdir -p "$DEST/bin" "$DEST/lib" "$DEST/lib/postgresql" "$DEST/$EXTENSION_REL_DIR"

for bin in postgres initdb pg_ctl pg_isready psql createdb; do
  [[ -x "$POSTGRES_PREFIX/bin/$bin" ]] || fail "required PostgreSQL binary missing: $POSTGRES_PREFIX/bin/$bin"
  cp "$POSTGRES_PREFIX/bin/$bin" "$DEST/bin/$bin"
done

if [[ -d "$POSTGRES_PREFIX/lib" ]]; then
  rsync -aL "$POSTGRES_PREFIX/lib/" "$DEST/lib/"
fi
rsync -a "$POSTGRES_SHARE_SOURCE/" "$DEST/$POSTGRES_SHARE_REL_DIR/"
chmod -R u+w "$DEST" 2>/dev/null || true

copy_extension_assets "$POSTGRES_PREFIX" "btree_gist" "1"
copy_extension_assets "$POSTGRES_PREFIX" "pgcrypto" "1"
copy_extension_assets "$POSTGRES_PREFIX" "pg_trgm" "1"
copy_extension_assets "$POSTGRES_PREFIX" "pg_stat_statements" "1"
copy_extension_assets "$PGVECTOR_PREFIX" "vector" "1"
copy_extension_assets "$TIMESCALEDB_PREFIX" "timescaledb" "1"
prune_release_forbidden_bundle_artifacts

write_manifest "$POSTGRES_VERSION" "$POSTGRES_MAJOR" "$TIMESCALE_VERSION" "$BUILD_ID" "$EXTENSION_REL_DIR"

if command -v install_name_tool >/dev/null 2>&1 && command -v otool >/dev/null 2>&1; then
  for _round in 1 2 3 4 5; do
    STAGED_HOST_DEP_CHANGED=0
    while IFS= read -r mach_o; do
      is_mach_o "$mach_o" || continue
      rewrite_dependency_paths "$mach_o"
      if [[ "$mach_o" == *.dylib ]]; then
        set_install_id "$mach_o"
      fi
    done < <(find "$DEST/bin" "$DEST/lib" -type f -print)
    [[ "$STAGED_HOST_DEP_CHANGED" == "0" ]] && break
  done
fi
sign_staged_mach_o_files

"$ROOT/scripts/verify-managed-postgres-bundle.sh" "$DEST"

echo "managed Postgres bundle staged at $DEST"
echo "postgres:   $POSTGRES_VERSION ($POSTGRES_PREFIX)"
echo "pgvector:   $PGVECTOR_PREFIX"
echo "timescale:  $TIMESCALE_VERSION ($TIMESCALEDB_PREFIX)"
