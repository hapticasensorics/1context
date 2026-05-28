# onecontext-capture-core

Rust implementation of the 1Context capture-bundle file contract.

This crate owns reusable bundle mechanics only. It does not run macOS sensors,
score attention, decide keep/drop, build summaries, or write Timescale rows.

## Boundaries

```text
Swift daemon / OneContextCapture:
  production sensors, permissions, live status/proof, and the eventual
  capture.bundle.export RPC

onecontext-capture-core:
  paths, required V0 files, manifest/source schemas, atomic writer helpers,
  JSONL spool window reads, READY validation, and retention planning/audit

onecontext-capture-bundler:
  operator/debug CLI around bundle folders and READY export validation

onecontext-capture-dashboard:
  GUI/debug reader of daemon status, live spool, and bundle lifecycle tests
```

## Capture Root Layout

Use the app-owned Application Support capture root:

```text
~/Library/Application Support/1Context/capture/
~/Library/Application Support/1Context Dev/capture/
```

`CaptureRootPaths` expects:

```text
capture/
  events/
  windows/
  displays/
  media/
  bundles/
    processing/
    live/
    failed/
    pinned/
  retention/
    sweeps.jsonl
```

Bundle-internal paths must be relative. `BundleRelativePath` rejects absolute
paths and parent-directory escapes.

## V0 READY Bundle Rules

A READY bundle lives at:

```text
capture/bundles/live/<capture_id>/
```

It must include `manifest.json`, `READY`, every path returned by
`required_bundle_files()`, and source inventory entries for every
`mandatory_lane_ids()` lane.

Mandatory V0 lanes may be degraded, but they must not be omitted. Degraded
lanes should have empty JSONL files plus `sources.json` and
`quality/known_gaps.jsonl` records that explain the missing direct source.

The bundle must not contain attention-filter output, keep/drop decisions,
semantic summaries, Timescale write receipts, claim state, or memory DB write
results.

## Lifecycle

```text
1. AtomicBundleWriter creates bundles/processing/<capture_id>.partial/.
2. The writer emits manifest.json with state="partial".
3. The writer emits required V0 files, direct source records, and explicit
   degraded quality records for missing current lanes.
4. The writer emits READY and rewrites manifest.json with state="ready".
5. The writer atomically promotes the directory to bundles/live/<capture_id>/.
6. Retention later deletes expired live/failed bundles and appends
   retention/sweeps.jsonl.
```

Consumers should ignore `processing/*.partial` and only read bundles where
`READY` exists and `manifest.state == "ready"`.

## Windows JSONL Export Fallback Contract

Window spool indexes are accelerators, not source-of-truth files. Capture bundle
export must still produce a correct bundle when an index is missing, stale,
newer than the log, or only covers a prefix of an append-only JSONL file.

The integration contract is:

- A trusted sidecar index may narrow file and byte ranges, but the exporter must
  verify it against the source log metadata before relying on it.
- If the source log has bytes appended after the indexed EOF, export must scan
  the append tail and include matching records. This prevents freshly captured
  windows from disappearing until the next index refresh.
- If the index is absent, stale, malformed, or references a different source
  log identity, tolerant export must fall back to source JSONL reads and emit
  degraded quality metadata rather than fail READY promotion solely because the
  accelerator is unusable.
- Undated legacy `*.windows.jsonl` files are ignored by bracketing lookup; new
  product exports require dated window logs or direct current lane records.
- Index builders and fast readers must not assume timestamp fields appear before
  large payload fields. Fixtures should cover JSON object orderings where
  `payload` precedes `recordedAt` or `eventTimeStart`, otherwise the remaining
  full-file scan can reappear for large dated window logs.
- Ordinary export uses tolerant spool reads: malformed JSONL lines are skipped,
  recorded in `quality/spool_read_report.json`, and surfaced as
  `malformed_spool_line_skipped` known gaps. Strict reads are reserved for
  explicit diagnostics and fail on malformed lines.
- Raw provenance for selected source records must preserve the original JSONL
  line number, byte offset, and hash. Bracketing records should carry equivalent
  source provenance when the indexed/tail lookup owns those offsets; until then,
  treat missing bracketing offsets as an integration risk.

The `window_jsonl_export_fallbacks` integration tests cover tolerant vs strict
malformed-line behavior, appended records after index creation, rejection of
undated legacy bracketing input, and raw provenance offsets for selected
in-window records.

## Useful Commands

```bash
cargo test -p onecontext-capture-core
cargo test -p onecontext-capture-dashboard --test capture_bundle_contract
cargo run -p onecontext-capture-bundler -- describe
```

Operator inspection:

```bash
CAPTURE_ROOT="$HOME/Library/Application Support/1Context Dev/capture"
cargo run -p onecontext-capture-bundler -- list --capture-root "$CAPTURE_ROOT" --class all
cargo run -p onecontext-capture-bundler -- validate --capture-root "$CAPTURE_ROOT" --capture-id <capture_id> --strict
cargo run -p onecontext-capture-bundler -- sweep --capture-root "$CAPTURE_ROOT"
```

Only use `sweep --apply` after reviewing the dry-run output.

## Fixture Status

There is no standalone fixture-generator binary yet. The current fixture path is
the crate test helper that seeds a window snapshot into a temp capture root,
calls `export_ready_bundle`, and validates the READY bundle.

If a future agent adds a fixture generator, it should create a disposable
capture-root tree, seed at least one `capture.window_snapshot`, export a READY
bundle, validate it, and print the bundle path plus validation report.
