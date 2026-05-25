#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CAPTURE_ROOT="${ONECONTEXT_BUNDLE_BENCH_CAPTURE_ROOT:-$(mktemp -d "${TMPDIR:-/tmp}/onecontext-large-windows.XXXXXX")}"
RECORD_COUNT="${ONECONTEXT_BUNDLE_BENCH_RECORDS:-10000}"
PAYLOAD_BYTES="${ONECONTEXT_BUNDLE_BENCH_PAYLOAD_BYTES:-8192}"
TARGET_INDEX="${ONECONTEXT_BUNDLE_BENCH_TARGET_INDEX:-$((RECORD_COUNT - 3))}"
PRIME_INDEX="${ONECONTEXT_BUNDLE_BENCH_PRIME_INDEX:-1}"
MAX_INDEXED_LINES_SCANNED="${ONECONTEXT_BUNDLE_BENCH_MAX_INDEXED_LINES_SCANNED:-256}"
WINDOWS_FILE_NAME="${ONECONTEXT_BUNDLE_BENCH_WINDOWS_FILE_NAME:-2026-05-25.windows.jsonl}"
WINDOWS_PATH="$CAPTURE_ROOT/windows/$WINDOWS_FILE_NAME"
RESPONSE_PATH="$CAPTURE_ROOT/export-response.json"
PRIME_RESPONSE_PATH="$CAPTURE_ROOT/export-prime-response.json"
TIME_PATH="$CAPTURE_ROOT/export-time.txt"
PRIME_TIME_PATH="$CAPTURE_ROOT/export-prime-time.txt"
RANGE_PATH="$CAPTURE_ROOT/export-range.env"

mkdir -p "$CAPTURE_ROOT/windows" "$CAPTURE_ROOT/media/frames-2fps"
printf 'frame 1\n' >"$CAPTURE_ROOT/media/frames-2fps/frame-000001.jpg"
printf 'frame 2\n' >"$CAPTURE_ROOT/media/frames-2fps/frame-000002.jpg"

BENCH_WINDOWS_PATH="$WINDOWS_PATH" \
BENCH_RECORD_COUNT="$RECORD_COUNT" \
BENCH_PAYLOAD_BYTES="$PAYLOAD_BYTES" \
BENCH_TARGET_INDEX="$TARGET_INDEX" \
BENCH_RANGE_PATH="$RANGE_PATH" \
node <<'NODE'
const fs = require("fs");

const path = process.env.BENCH_WINDOWS_PATH;
const recordCount = Number(process.env.BENCH_RECORD_COUNT);
const payloadBytes = Number(process.env.BENCH_PAYLOAD_BYTES);
const targetIndex = Math.max(0, Math.min(recordCount - 2, Number(process.env.BENCH_TARGET_INDEX)));
const base = Date.parse("2026-05-25T12:00:00.000Z");
const padding = "x".repeat(payloadBytes);
const stream = fs.createWriteStream(path);

for (let index = 0; index < recordCount; index += 1) {
  const time = new Date(base + index * 1000).toISOString();
  stream.write(JSON.stringify({
    schemaVersion: 1,
    eventType: "capture.window_snapshot",
    recordedAt: time,
    payload: {
      windows: [{
        windowID: index,
        ownerName: "Synthetic",
        bundleID: "com.haptica.synthetic",
        title: padding
      }],
      displays: [{
        displayID: 1,
        bounds: { x: 0, y: 0, width: 1512, height: 982 }
      }]
    }
  }) + "\n");
}

stream.end();
const start = new Date(base + targetIndex * 1000 + 250).toISOString();
const end = new Date(base + targetIndex * 1000 + 260).toISOString();
fs.writeFileSync(process.env.BENCH_RANGE_PATH, `START_TIME=${start}\nEND_TIME=${end}\n`);
NODE

source "$RANGE_PATH"

cd "$REPO_ROOT"
cargo build --quiet -p onecontext-capture-bundler
if [[ "$PRIME_INDEX" != "0" ]]; then
  /usr/bin/time -p target/debug/onecontext-capture-bundler \
    export \
    --capture-root "$CAPTURE_ROOT" \
    --start "$START_TIME" \
    --end "$END_TIME" \
    >"$PRIME_RESPONSE_PATH" 2>"$PRIME_TIME_PATH"
fi
/usr/bin/time -p target/debug/onecontext-capture-bundler \
  export \
  --capture-root "$CAPTURE_ROOT" \
  --start "$START_TIME" \
  --end "$END_TIME" \
  >"$RESPONSE_PATH" 2>"$TIME_PATH"

BENCH_RESPONSE_PATH="$RESPONSE_PATH" \
BENCH_PRIME_RESPONSE_PATH="$PRIME_RESPONSE_PATH" \
BENCH_TARGET_INDEX="$TARGET_INDEX" \
BENCH_PAYLOAD_BYTES="$PAYLOAD_BYTES" \
BENCH_MAX_INDEXED_LINES_SCANNED="$MAX_INDEXED_LINES_SCANNED" \
ONECONTEXT_BUNDLE_BENCH_PRIME_INDEX="$PRIME_INDEX" \
START_TIME="$START_TIME" \
END_TIME="$END_TIME" \
node <<'NODE'
const fs = require("fs");
const response = JSON.parse(fs.readFileSync(process.env.BENCH_RESPONSE_PATH, "utf8"));
const bundlePath = response.bundle.bundle_path;
const spoolReport = JSON.parse(fs.readFileSync(`${bundlePath}/quality/spool_read_report.json`, "utf8"));
const lookup = JSON.parse(fs.readFileSync(`${bundlePath}/quality/bracketing_window_snapshot_lookup.json`, "utf8"));
const largeFile = spoolReport.files.find((file) => String(file.path).endsWith(".windows.jsonl"));
const maxIndexedLinesScanned = Number(process.env.BENCH_MAX_INDEXED_LINES_SCANNED || 256);
const indexPrimed = process.env.ONECONTEXT_BUNDLE_BENCH_PRIME_INDEX !== "0";
let primeLargeFile = null;
if (indexPrimed) {
  const primeResponse = JSON.parse(fs.readFileSync(process.env.BENCH_PRIME_RESPONSE_PATH, "utf8"));
  const primeBundlePath = primeResponse.bundle.bundle_path;
  const primeSpoolReport = JSON.parse(fs.readFileSync(`${primeBundlePath}/quality/spool_read_report.json`, "utf8"));
  primeLargeFile = primeSpoolReport.files.find((file) => String(file.path).endsWith(".windows.jsonl"));
  if (!primeLargeFile) {
    throw new Error("prime export did not report .windows.jsonl in spool_read_report.json");
  }
}

if (!largeFile) {
  throw new Error(".windows.jsonl was not reported in spool_read_report.json");
}
if (largeFile.full_record_parse_count !== 0) {
  throw new Error(`expected zero full spool payload parses, got ${largeFile.full_record_parse_count}`);
}
if (largeFile.index_used !== true) {
  throw new Error("expected indexed windows spool reader to be used");
}
if (indexPrimed && largeFile.index_built === true) {
  throw new Error("expected measured export to reuse the primed windows spool index, but it rebuilt it");
}
if (largeFile.indexed_lines_scanned > maxIndexedLinesScanned) {
  throw new Error(`expected indexed reader to scan <= ${maxIndexedLinesScanned} lines, scanned ${largeFile.indexed_lines_scanned}`);
}
if (largeFile.parsed_lines >= largeFile.total_lines) {
  throw new Error(`expected indexed reader not to parse every spool line, parsed ${largeFile.parsed_lines}/${largeFile.total_lines}`);
}
if (lookup.full_payload_parse_count !== 0) {
  throw new Error(`expected zero full bracketing payload parses, got ${lookup.full_payload_parse_count}`);
}
if (lookup.selected_record_count !== 2) {
  throw new Error(`expected two bracketing records, got ${lookup.selected_record_count}`);
}

console.log(JSON.stringify({
  status: "ok",
  capture_root: response.capture_root,
  bundle_path: bundlePath,
  timing_path: `${response.capture_root}/export-time.txt`,
  record_count: largeFile.total_lines,
  payload_bytes_per_record: Number(process.env.BENCH_PAYLOAD_BYTES || 8192),
  target_index: Number(process.env.BENCH_TARGET_INDEX),
  start_time: process.env.START_TIME,
  end_time: process.env.END_TIME,
  index_primed: indexPrimed,
  prime_timing_path: indexPrimed ? `${response.capture_root}/export-prime-time.txt` : null,
  prime_spool_index_used: primeLargeFile ? primeLargeFile.index_used : null,
  prime_spool_index_built: primeLargeFile ? primeLargeFile.index_built : null,
  prime_spool_indexed_lines_scanned: primeLargeFile ? primeLargeFile.indexed_lines_scanned : null,
  spool_scan_strategy: largeFile.scan_strategy,
  spool_parsed_lines: largeFile.parsed_lines,
  spool_index_used: largeFile.index_used,
  spool_index_built: largeFile.index_built,
  spool_index_refreshed: largeFile.index_refreshed,
  spool_indexed_lines_scanned: largeFile.indexed_lines_scanned,
  spool_index_checkpoint_count: largeFile.index_checkpoint_count,
  spool_full_record_parse_count: largeFile.full_record_parse_count,
  bracketing_files_scanned: lookup.files_scanned,
  bracketing_lines_scanned: lookup.lines_scanned,
  bracketing_full_payload_parse_count: lookup.full_payload_parse_count,
  bracketing_minimal_envelope_parse_count: lookup.minimal_envelope_parse_count,
  bracketing_selected_record_count: lookup.selected_record_count
}, null, 2));
NODE

if [[ -f "$PRIME_TIME_PATH" ]]; then
  echo "prime:"
  cat "$PRIME_TIME_PATH"
fi
echo "measured:"
cat "$TIME_PATH"
