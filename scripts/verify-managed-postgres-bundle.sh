#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage: scripts/verify-managed-postgres-bundle.sh [options] <bundle-prefix-or-app>

Validates a staged managed Postgres bundle manifest, required files, extension
control/SQL assets, required extension dylibs, and Mach-O dependency paths.
Pass either the bundle prefix (release/managed-postgres/runtime/macos-arm64) or a built
.app.

Options:
  --allow-host-fingerprints  Allow Homebrew/MacPorts/build-host strings for
                             dev-only inspection of non-release bundles.
  --require-sbom             Require and validate managed-postgres-sbom.json.
USAGE
}

ALLOW_HOST_FINGERPRINTS=0
REQUIRE_SBOM=0
ARGS=()
while [[ $# -gt 0 ]]; do
  case "$1" in
    --allow-host-fingerprints)
      ALLOW_HOST_FINGERPRINTS=1
      shift
      ;;
    --require-sbom)
      REQUIRE_SBOM=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      ARGS+=( "$1" )
      shift
      ;;
  esac
done

if [[ ${#ARGS[@]} -ne 1 ]]; then
  usage
  exit 2
fi

TARGET="${ARGS[0]}"
if [[ "$TARGET" == *.app ]]; then
  PREFIX="$TARGET/Contents/Resources/managed-postgres/macos-arm64"
else
  PREFIX="$TARGET"
fi
MANIFEST="$PREFIX/manifest.json"
SBOM="$PREFIX/managed-postgres-sbom.json"

[[ -d "$PREFIX" ]] || { echo "managed Postgres bundle directory missing: $PREFIX" >&2; exit 1; }
[[ -f "$MANIFEST" ]] || { echo "managed Postgres manifest missing: $MANIFEST" >&2; exit 1; }

REQUIRED_ENTRIES_RAW="$(
/usr/bin/python3 - "$MANIFEST" <<'PY'
import json
import posixpath
import sys

path = sys.argv[1]
with open(path, "r", encoding="utf-8") as handle:
    manifest = json.load(handle)

if manifest.get("bundle_schema") != 1:
    raise SystemExit("bundle_schema must be 1")
if manifest.get("arch") != "arm64":
    raise SystemExit("arch must be arm64")
if manifest.get("postgres_major") != 17:
    raise SystemExit("postgres_major must be 17")

postgres_version = manifest.get("postgres_version", "")
timescale_version = manifest.get("timescale_version", "")
build_id = manifest.get("build_id", "")
for label, value in [
    ("postgres_version", postgres_version),
    ("timescale_version", timescale_version),
    ("build_id", build_id),
]:
    if not value:
        raise SystemExit(f"{label} is required")
if postgres_version == "17.x":
    raise SystemExit("postgres_version placeholder was not replaced")
if timescale_version == "x.y.z":
    raise SystemExit("timescale_version placeholder was not replaced")
if "x.y.z" in build_id:
    raise SystemExit("build_id placeholder was not replaced")

bins = manifest.get("bin") or {}
extension = manifest.get("extension") or {}
required_extensions = manifest.get("required_extensions") or []
required_preload_libraries = manifest.get("required_preload_libraries") or []
expected_extensions = [
    "timescaledb",
    "btree_gist",
    "pgcrypto",
    "pg_trgm",
    "vector",
    "pg_stat_statements",
]
expected_preload = ["timescaledb", "pg_stat_statements"]
library_globs = {
    "timescaledb": extension.get("timescaledb_library_glob"),
    "btree_gist": "lib/postgresql/btree_gist*.dylib",
    "pgcrypto": "lib/postgresql/pgcrypto*.dylib",
    "pg_trgm": "lib/postgresql/pg_trgm*.dylib",
    "vector": "lib/postgresql/vector*.dylib",
    "pg_stat_statements": "lib/postgresql/pg_stat_statements*.dylib",
}

missing_extensions = [name for name in expected_extensions if name not in required_extensions]
if missing_extensions:
    raise SystemExit(f"required_extensions missing: {', '.join(missing_extensions)}")
missing_preload = [name for name in expected_preload if name not in required_preload_libraries]
if missing_preload:
    raise SystemExit(f"required_preload_libraries missing: {', '.join(missing_preload)}")

for key in ["postgres", "initdb", "pg_ctl", "pg_isready", "psql"]:
    value = bins.get(key)
    if not value:
        raise SystemExit(f"bin.{key} is required")
    print(f"exec:{value}")
if bins.get("createdb"):
    print(f"exec:{bins['createdb']}")

control = extension.get("timescaledb_control")
if not control:
    raise SystemExit("extension.timescaledb_control is required")
print(f"file:{control}")
extension_dir = posixpath.dirname(control) or "share/postgresql/extension"

for extension_name in expected_extensions:
    print(f"file:{extension_dir}/{extension_name}.control")
    print(f"glob:{extension_dir}/{extension_name}--*.sql")
    library_glob = library_globs.get(extension_name)
    if not library_glob:
        raise SystemExit(f"missing library glob for {extension_name}")
    print(f"glob:{library_glob}")
PY
)"
mapfile -t REQUIRED_ENTRIES <<<"$REQUIRED_ENTRIES_RAW"

if [[ "$REQUIRE_SBOM" == "1" ]]; then
  [[ -f "$SBOM" ]] || { echo "managed Postgres SBOM missing: $SBOM" >&2; exit 1; }
fi
if [[ -f "$SBOM" ]]; then
  /usr/bin/python3 - "$MANIFEST" "$SBOM" <<'PY'
import json
import re
import sys

manifest_path, sbom_path = sys.argv[1:]
with open(manifest_path, "r", encoding="utf-8") as handle:
    manifest = json.load(handle)
with open(sbom_path, "r", encoding="utf-8") as handle:
    sbom = json.load(handle)

if sbom.get("schema") != "1context.managed-postgres.sbom.v1":
    raise SystemExit("managed Postgres SBOM schema is invalid")
if sbom.get("arch") != "macos-arm64":
    raise SystemExit("managed Postgres SBOM arch must be macos-arm64")

bundle = sbom.get("bundle") or {}
if str(bundle.get("postgres_version", "")) != str(manifest.get("postgres_version", "")):
    raise SystemExit("SBOM postgres_version does not match manifest")
if str(bundle.get("timescale_version", "")) != str(manifest.get("timescale_version", "")):
    raise SystemExit("SBOM timescale_version does not match manifest")
if str(bundle.get("build_id", "")) != str(manifest.get("build_id", "")):
    raise SystemExit("SBOM build_id does not match manifest")

components = sbom.get("components")
if not isinstance(components, list):
    raise SystemExit("SBOM components must be a list")
by_name = {component.get("name"): component for component in components if isinstance(component, dict)}
for name in ["postgresql", "openssl", "timescaledb", "pgvector"]:
    component = by_name.get(name)
    if not component:
        raise SystemExit(f"SBOM missing component: {name}")
    for field in ["version", "url", "sha256", "archive", "license"]:
        value = component.get(field)
        if not isinstance(value, str) or not value:
            raise SystemExit(f"SBOM {name}.{field} is missing")
    if not component["url"].startswith("https://"):
        raise SystemExit(f"SBOM {name}.url must be HTTPS")
    if not re.fullmatch(r"[0-9a-f]{64}", component["sha256"]):
        raise SystemExit(f"SBOM {name}.sha256 must be a lowercase SHA-256 hex digest")

if by_name["postgresql"]["version"] != manifest.get("postgres_version"):
    raise SystemExit("SBOM PostgreSQL component version does not match manifest")
if by_name["timescaledb"]["version"] != manifest.get("timescale_version"):
    raise SystemExit("SBOM TimescaleDB component version does not match manifest")
PY
fi

for entry in "${REQUIRED_ENTRIES[@]}"; do
  kind="${entry%%:*}"
  rel="${entry#*:}"
  case "$rel" in
    /*|*../*)
      echo "manifest path must stay inside bundle: $rel" >&2
      exit 1
      ;;
  esac
  path="$PREFIX/$rel"
  case "$kind" in
    exec)
      [[ -x "$path" ]] || { echo "required executable missing or not executable: $path" >&2; exit 1; }
      ;;
    file)
      [[ -f "$path" ]] || { echo "required file missing: $path" >&2; exit 1; }
      ;;
    glob)
      shopt -s nullglob
      matches=( "$PREFIX"/$rel )
      shopt -u nullglob
      [[ ${#matches[@]} -gt 0 ]] || { echo "required glob matched no files: $PREFIX/$rel" >&2; exit 1; }
      ;;
  esac
done

if [[ "$TARGET" == *.app ]] && command -v codesign >/dev/null 2>&1; then
  codesign --verify --strict --deep --verbose=2 "$TARGET" >/dev/null
fi

if command -v otool >/dev/null 2>&1; then
  while IFS= read -r mach_o; do
    file -b "$mach_o" 2>/dev/null | grep -q 'Mach-O' || continue
    deps="$(otool -L "$mach_o" 2>/dev/null || true)"
    if grep -E '(/opt/homebrew|/usr/local/(Cellar|opt)|/opt/local)' <<<"$deps" >/dev/null; then
      echo "host-managed dependency leaked into $mach_o" >&2
      echo "$deps" >&2
      exit 1
    fi
    while IFS= read -r dep; do
      [[ -n "$dep" ]] || continue
      if [[ "$dep" == @loader_path/* ]]; then
        resolved="$(/usr/bin/python3 - "$mach_o" "$dep" <<'PY'
import os
import sys
mach_o, dep = sys.argv[1], sys.argv[2]
print(os.path.normpath(os.path.join(os.path.dirname(mach_o), dep.removeprefix("@loader_path/"))))
PY
)"
        [[ -f "$resolved" ]] || {
          echo "loader_path dependency missing for $mach_o: $dep -> $resolved" >&2
          exit 1
        }
      fi
    done < <(printf '%s\n' "$deps" | tail -n +2 | awk '{print $1}')
  done < <(find "$PREFIX" -type f \( -perm -0100 -o -perm -0010 -o -perm -0001 -o -name '*.dylib' \) -print)
fi

if command -v codesign >/dev/null 2>&1; then
  while IFS= read -r mach_o; do
    file -b "$mach_o" 2>/dev/null | grep -q 'Mach-O' || continue
    codesign --verify "$mach_o" >/dev/null
  done < <(find "$PREFIX" -type f \( -perm -0100 -o -perm -0010 -o -perm -0001 -o -name '*.dylib' \) -print)
fi

if [[ "$ALLOW_HOST_FINGERPRINTS" != "1" ]]; then
  forbidden_artifacts="$(find "$PREFIX" -type f \( \
    -name '*.a' \
    -o -name '*.pc' \
    -o -path '*/pgxs/*' \
    -o -path '*/include/*' \
    -o -path '*/doc/*' \
    -o -path '*/man/*' \
  \) -print)"
  if [[ -n "$forbidden_artifacts" ]]; then
    echo "managed Postgres bundle contains release-forbidden build artifacts:" >&2
    printf '%s\n' "$forbidden_artifacts" | sed -n '1,120p' >&2
    exit 1
  fi

  host_fingerprint_report="$(mktemp /tmp/onecontext-managed-pg-host-fingerprints.XXXXXX)"
  host_fingerprint_pattern='(/opt/homebrew|/usr/local/(Cellar|opt)|/opt/local|/Users/|Homebrew|PG_CONFIG_PATH=|PKG_CONFIG_PATH=)'
  while IFS= read -r file; do
    matches="$(strings -a "$file" 2>/dev/null | LC_ALL=C grep -E "$host_fingerprint_pattern" | sed -n '1,20p' || true)"
    if [[ -n "$matches" ]]; then
      {
        echo "$file"
        printf '%s\n' "$matches" | sed 's/^/  /'
      } >>"$host_fingerprint_report"
    fi
  done < <(find "$PREFIX" -type f -print)
  if [[ -s "$host_fingerprint_report" ]]; then
    echo "managed Postgres bundle contains host/build-system fingerprints:" >&2
    sed -n '1,160p' "$host_fingerprint_report" >&2
    rm -f "$host_fingerprint_report"
    exit 1
  fi
  rm -f "$host_fingerprint_report"
fi

echo "managed Postgres bundle verified: $PREFIX"
