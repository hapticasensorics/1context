#!/usr/bin/env bash
set -u

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT" || exit 1

STRICT=0
if [[ "${1:-}" == "--strict" ]]; then
  STRICT=1
fi

SCRIPT_LIMIT=2860
FORBIDDEN_TERMS='legacy|compat|compatibility|migration|fallback|scaffold|repair|alias|backfill|upgrade'

RG_EXCLUDES=(
  --glob '!.git/**'
  --glob '!target/**'
  --glob '!dist/**'
  --glob '!node_modules/**'
  --glob '!release/runner/node_modules/**'
  --glob '!release/runner/dist/**'
  --glob '!recycle-bin/**'
  --glob '!runtime/**'
  --glob '!runtime-test/**'
  --glob '!test-results/**'
  --glob '!docs/archive/**'
  --glob '!docs/goals/archive/**'
  --glob '!docs/retired.md'
  --glob '!docs/cleanup-policy.md'
  --glob '!docs/coding-agent-cleanup-questions.md'
  --glob '!docs/cleanup-deletion-program.md'
  --glob '!docs/cleanup-verification-matrix.md'
  --glob '!docs/handrolled-legacy-cleanup-plan.md'
  --glob '!docs/low-end-loc-cleanup-plan.md'
  --glob '!devtools/cleanup-guard.sh'
)

SCRIPT_RETIRE_CANDIDATES=(
  'generate-agent-mail-triad-demo.mjs'
  'test-agent-harness-boundary-dogfood.mjs'
  'test-agent-mail-dogfood.mjs'
  'test-codex-adapter-live-server-dogfood.mjs'
  'test-codex-adapter-live-mail-flow.mjs'
  'test-codex-adapter-harness-dogfood.mjs'
  'test-wiki-core-dogfood.mjs'
  'verify-agent-mail-triad-mcp-realism.mjs'
  'onecontext-wiki-mcp-server.mjs'
  'test-capture-audit-regenerated-bundle.sh'
  'benchmark-capture-bundle-large-windows.sh'
  'benchmark-capture-bundle-media-export.sh'
  'test-capture-dashboard-metadata.sh'
  'launch-attention-dashboard.sh'
  'benchmark-memory-backfill.sh'
  'summarize-memory-benchmarks.sh'
  'test-memory-local-web-e2e.sh'
  'test-installed-app-permission-capabilities.sh'
  'test-browser-extension-native-host.sh'
  'test-wiki-runtime-defaults-scenarios.sh'
  'test-release-train.sh'
  'test-launch-agent-package.sh'
)

GENERATED_TRACKED_PATHS=(
  'docs/assets/attention-capture-mockup/**'
  'demos/agent-mail-triad/static/fixtures/latest.json'
  'release/tools/caddy/**/*.tar.gz'
)

RETIRED_REFERENCE_PATTERNS=(
  'docs/assets/attention-capture-mockup'
  'attention-debug-20260524'
  'docs/attention-capture-mockup.html'
  'demos/peekaboo-evidence-wall'
)

failures=0

section() {
  printf '\n== %s ==\n' "$1"
}

line_count() {
  if [[ -d scripts ]]; then
    find scripts -type f -print0 | xargs -0 awk 'END { print NR + 0 }'
  else
    printf '0\n'
  fi
}

count_matches() {
  local pattern="$1"
  shift
  rg -n -i "$pattern" "${RG_EXCLUDES[@]}" "$@" 2>/dev/null | wc -l | tr -d ' '
}

sample_matches() {
  local pattern="$1"
  shift
  rg -n -i "$pattern" "${RG_EXCLUDES[@]}" "$@" 2>/dev/null | sed -n '1,25p'
}

section "Scripts Size Gate"
scripts_loc="$(line_count)"
printf 'scripts LOC: %s\n' "$scripts_loc"
printf 'target LOC:  <= %s\n' "$SCRIPT_LIMIT"
if (( scripts_loc > SCRIPT_LIMIT )); then
  printf 'status:      over target\n'
  if (( STRICT )); then
    failures=$((failures + 1))
  fi
else
  printf 'status:      within target\n'
fi

section "Generated Artifact Re-Entry"
tracked_generated="$(
  git ls-files "${GENERATED_TRACKED_PATHS[@]}" 2>/dev/null \
    | while IFS= read -r path; do
        [[ -e "$path" ]] && printf '%s\n' "$path"
      done
)"
if [[ -n "$tracked_generated" ]]; then
  printf '%s\n' "$tracked_generated"
  if (( STRICT )); then
    failures=$((failures + 1))
  fi
else
  printf 'no active tracked generated attention assets, latest.json fixtures, or Caddy tarballs\n'
fi

section "Retired Artifact Active References"
for pattern in "${RETIRED_REFERENCE_PATTERNS[@]}"; do
  refs="$(rg -n --fixed-strings "$pattern" "${RG_EXCLUDES[@]}" . 2>/dev/null || true)"
  count="$(printf '%s\n' "$refs" | sed '/^$/d' | wc -l | tr -d ' ')"
  printf '%4s  %s\n' "$count" "$pattern"
  if (( count > 0 )); then
    printf '%s\n' "$refs" | sed -n '1,8p'
    if (( STRICT )); then
      failures=$((failures + 1))
    fi
  fi
done

section "Retire-Candidate Script References"
for name in "${SCRIPT_RETIRE_CANDIDATES[@]}"; do
  refs="$(rg -n --fixed-strings "$name" "${RG_EXCLUDES[@]}" . 2>/dev/null || true)"
  count="$(printf '%s\n' "$refs" | sed '/^$/d' | wc -l | tr -d ' ')"
  printf '%4s  %s\n' "$count" "$name"
  if (( count > 0 )); then
    printf '%s\n' "$refs" | sed -n '1,8p'
    if (( STRICT )); then
      failures=$((failures + 1))
    fi
  fi
done

section "Forbidden Term Counts"
declare -a CATEGORIES=(
  'product:crates macos/Sources wiki-engine/src browser-extension release/runner/src'
  'tests:macos/Tests wiki-engine/tests crates'
  'docs:docs'
  'scripts:scripts package.json .github'
)

for category in "${CATEGORIES[@]}"; do
  label="${category%%:*}"
  paths="${category#*:}"
  # shellcheck disable=SC2086
  count="$(count_matches "$FORBIDDEN_TERMS" $paths)"
  printf '%8s  %s\n' "$label" "$count"
done

section "Forbidden Term Samples"
for category in "${CATEGORIES[@]}"; do
  label="${category%%:*}"
  paths="${category#*:}"
  printf '\n-- %s --\n' "$label"
  # shellcheck disable=SC2086
  sample_matches "$FORBIDDEN_TERMS" $paths
done

section "Result"
if (( failures > 0 )); then
  printf 'cleanup guard failed with %s strict violation(s)\n' "$failures"
  exit 1
fi

if (( STRICT )); then
  printf 'cleanup guard passed\n'
else
  printf 'report complete; pass --strict to fail on current guard violations\n'
fi
