# onecontext-capture-bundler

Rust CLI for the capture-bundle handoff surface.

The Swift capture runtime remains the owner of macOS sensors and the live
capture spool. This binary owns agent/operator-friendly bundle export,
inspection, validation, and retention commands around the shared Rust
`onecontext-capture-core` contract:

```bash
onecontext-capture-bundler export --start <rfc3339> --end <rfc3339> [--capture-root <path>] [--capture-id <id>] [--target active-window|all-windows|custom:<value>] [--frames-2fps-dir <path>] [--debug-video <path>] [--debug-pin] [--status-json <path>] [--ux-status-json <path>] [--sampler-json <path>] [--browser-proof-json <path>] [--dry-run]
onecontext-capture-bundler list [--capture-root <path>] [--class live|processing|failed|pinned|all]
onecontext-capture-bundler validate --bundle <path> [--strict]
onecontext-capture-bundler validate --capture-id <id> [--capture-root <path>]
onecontext-capture-bundler sweep [--capture-root <path>] [--processing-max-age-seconds <n>] [--live-max-age-seconds <n>] [--failed-max-age-seconds <n>] [--keep-live <n>] [--apply]
onecontext-capture-bundler status [--capture-root <path>]
onecontext-capture-bundler describe
```

`export` now calls `onecontext_capture_core::export_ready_bundle` to build a
READY bundle from the capture spool. `--dry-run` is non-mutating: it reads the
selected spool window and reports the planned export inputs plus the minimum
READY validation signal before any bundle directory is written.

READY bundles require the Swift screen-recording decoder's 2fps frame cache.
By default export reads it from `<capture-root>/media/frames-2fps`; pass
`--frames-2fps-dir` when the decoder wrote frames elsewhere. `--debug-video`
copies the source recording into `media/debug/` for investigation only; the 2fps
screenshots are normal capture evidence.
