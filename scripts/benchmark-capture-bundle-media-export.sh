#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DEFAULT_AUDIT_ROOT="/Users/paulhan/Library/Application Support/1Context Dev/capture-audits/capture-audit-20260525T093158Z-60s"

usage() {
  cat <<'USAGE'
Usage:
  scripts/benchmark-capture-bundle-media-export.sh

Environment:
  ONECONTEXT_CAPTURE_AUDIT_ROOT          Saved capture audit root.
  ONECONTEXT_CAPTURE_BUNDLER             Optional path to onecontext-capture-bundler.
  ONECONTEXT_BUNDLE_BENCHMARK_DIR        Output directory. Defaults to dist/capture-bundle-media-benchmark/<timestamp>.
  ONECONTEXT_BUNDLE_BENCH_ITERATIONS     Iterations to run. Defaults to 3.
  ONECONTEXT_BUNDLE_BENCH_FRAME_COUNT    Synthetic 2fps frame count. Defaults to 120.
  ONECONTEXT_BUNDLE_BENCH_FRAME_BYTES    Bytes per synthetic frame. Defaults to 1048576.
  ONECONTEXT_BUNDLE_BENCH_DEBUG_BYTES    Optional synthetic debug video bytes. Defaults to 0.

Rehydrates saved audit spool/capability data into fresh capture roots, exports a
READY bundle with deterministic synthetic frame media, and records /usr/bin/time
measurements for the Rust bundler export path.
USAGE
}

if [[ "${1:-}" == "--help" || "${1:-}" == "-h" ]]; then
  usage
  exit 0
fi

fail() {
  echo "capture bundle media benchmark failed: $*" >&2
  exit 1
}

require_tool() {
  command -v "$1" >/dev/null 2>&1 || fail "required tool not found: $1"
}

require_tool awk
require_tool date
require_tool dd
require_tool ditto
require_tool jq

if [[ -z "${ONECONTEXT_CAPTURE_BUNDLER:-}" && ! -x "$ROOT/target/release/onecontext-capture-bundler" ]]; then
  require_tool cargo
  cargo build -p onecontext-capture-bundler --release >/dev/null
fi
BUNDLER_BIN="${ONECONTEXT_CAPTURE_BUNDLER:-$ROOT/target/release/onecontext-capture-bundler}"
[[ -x "$BUNDLER_BIN" ]] || fail "bundler is not executable: $BUNDLER_BIN"

AUDIT_ROOT="${ONECONTEXT_CAPTURE_AUDIT_ROOT:-$DEFAULT_AUDIT_ROOT}"
AUDIT_ROOT="${AUDIT_ROOT%/}"
RUN_JSON="$AUDIT_ROOT/run.json"
RAW_ROOT="$AUDIT_ROOT/raw/window"

[[ -d "$AUDIT_ROOT" ]] || fail "audit root not found: $AUDIT_ROOT"
[[ -f "$RUN_JSON" ]] || fail "run.json not found: $RUN_JSON"
[[ -d "$RAW_ROOT" ]] || fail "raw/window spool copy not found: $RAW_ROOT"

ITERATIONS="${ONECONTEXT_BUNDLE_BENCH_ITERATIONS:-3}"
FRAME_COUNT="${ONECONTEXT_BUNDLE_BENCH_FRAME_COUNT:-120}"
FRAME_BYTES="${ONECONTEXT_BUNDLE_BENCH_FRAME_BYTES:-1048576}"
DEBUG_BYTES="${ONECONTEXT_BUNDLE_BENCH_DEBUG_BYTES:-0}"
STAMP="$(date -u +%Y%m%dT%H%M%SZ)"
BENCH_DIR="${ONECONTEXT_BUNDLE_BENCHMARK_DIR:-$ROOT/dist/capture-bundle-media-benchmark/$STAMP}"
FRAMES_DIR="$BENCH_DIR/source-frames"
DEBUG_VIDEO="$BENCH_DIR/source-debug-video.mov"
SUMMARY_JSON="$BENCH_DIR/benchmark-summary.json"

TIME_START="$(jq -r '.time_start' "$RUN_JSON")"
TIME_END="$(jq -r '.time_end' "$RUN_JSON")"
[[ "$TIME_START" != "null" && -n "$TIME_START" ]] || fail "run.json lacks time_start"
[[ "$TIME_END" != "null" && -n "$TIME_END" ]] || fail "run.json lacks time_end"

rm -rf "$BENCH_DIR"
mkdir -p "$FRAMES_DIR"
BASE_FRAME="$BENCH_DIR/frame-base.jpg"
dd if=/dev/zero of="$BASE_FRAME" bs="$FRAME_BYTES" count=1 status=none

frame_index=1
while [[ "$frame_index" -le "$FRAME_COUNT" ]]; do
  printf -v frame_name 'frame-%06d.jpg' "$frame_index"
  cp "$BASE_FRAME" "$FRAMES_DIR/$frame_name"
  frame_index=$((frame_index + 1))
done

if [[ "$DEBUG_BYTES" -gt 0 ]]; then
  dd if=/dev/zero of="$DEBUG_VIDEO" bs="$DEBUG_BYTES" count=1 status=none
fi

printf 'iteration,real,user,sys,bundle_path,byte_count,file_count\n' >"$BENCH_DIR/timings.csv"

for iteration in $(awk -v count="$ITERATIONS" 'BEGIN { for (i = 1; i <= count; i++) print i }'); do
  RUN_DIR="$BENCH_DIR/run-$iteration"
  CAPTURE_ROOT="$RUN_DIR/capture-root"
  mkdir -p "$CAPTURE_ROOT/events" "$CAPTURE_ROOT/windows" "$CAPTURE_ROOT/displays"
  for spool_dir in events windows displays; do
    if [[ -d "$RAW_ROOT/$spool_dir" ]]; then
      ditto "$RAW_ROOT/$spool_dir" "$CAPTURE_ROOT/$spool_dir"
    fi
  done

  export_args=(
    export
    --capture-root "$CAPTURE_ROOT"
    --start "$TIME_START"
    --end "$TIME_END"
    --debug-pin
    --frames-2fps-dir "$FRAMES_DIR"
  )
  [[ -f "$AUDIT_ROOT/outputs/status-after.json" ]] && export_args+=(--status-json "$AUDIT_ROOT/outputs/status-after.json")
  [[ -f "$AUDIT_ROOT/outputs/ux-status.json" ]] && export_args+=(--ux-status-json "$AUDIT_ROOT/outputs/ux-status.json")
  [[ -f "$AUDIT_ROOT/outputs/sampler-status.json" ]] && export_args+=(--sampler-json "$AUDIT_ROOT/outputs/sampler-status.json")
  [[ -f "$AUDIT_ROOT/outputs/browser-proof.json" ]] && export_args+=(--browser-proof-json "$AUDIT_ROOT/outputs/browser-proof.json")
  [[ "$DEBUG_BYTES" -gt 0 ]] && export_args+=(--debug-video "$DEBUG_VIDEO")

  /usr/bin/time -p "$BUNDLER_BIN" "${export_args[@]}" \
    >"$RUN_DIR/export-response.json" \
    2>"$RUN_DIR/export.time"

  real_time="$(awk '$1 == "real" { print $2 }' "$RUN_DIR/export.time")"
  user_time="$(awk '$1 == "user" { print $2 }' "$RUN_DIR/export.time")"
  sys_time="$(awk '$1 == "sys" { print $2 }' "$RUN_DIR/export.time")"
  bundle_path="$(jq -r '.bundle.bundle_path // ""' "$RUN_DIR/export-response.json")"
  byte_count="$(jq -r '.bundle.byte_count // 0' "$RUN_DIR/export-response.json")"
  file_count="$(jq -r '.bundle.file_count // 0' "$RUN_DIR/export-response.json")"
  printf '%s,%s,%s,%s,%s,%s,%s\n' \
    "$iteration" "$real_time" "$user_time" "$sys_time" "$bundle_path" "$byte_count" "$file_count" \
    >>"$BENCH_DIR/timings.csv"
done

jq -n \
  --arg audit_root "$AUDIT_ROOT" \
  --arg bench_dir "$BENCH_DIR" \
  --arg time_start "$TIME_START" \
  --arg time_end "$TIME_END" \
  --argjson iterations "$ITERATIONS" \
  --argjson frame_count "$FRAME_COUNT" \
  --argjson frame_bytes "$FRAME_BYTES" \
  --argjson debug_bytes "$DEBUG_BYTES" \
  --rawfile timings "$BENCH_DIR/timings.csv" \
  '{
    schema_version: 1,
    audit_root: $audit_root,
    benchmark_dir: $bench_dir,
    time_window: {start: $time_start, end: $time_end},
    media_fixture: {
      frame_count: $frame_count,
      frame_bytes: $frame_bytes,
      debug_video_bytes: $debug_bytes
    },
    iterations: $iterations,
    timings_csv: $timings
  }' >"$SUMMARY_JSON"

echo "Capture bundle media benchmark complete."
echo "Summary: $SUMMARY_JSON"
echo "Timings: $BENCH_DIR/timings.csv"
