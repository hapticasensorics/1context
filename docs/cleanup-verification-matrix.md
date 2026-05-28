# Cleanup Verification Matrix

Use this matrix for cleanup slices. A slice can narrow the test set to affected
areas while it is in progress, but the final cleanup must pass the full matrix.

## Always Run For Cleanup Slices

| Gate | Command | Pass Condition |
| --- | --- | --- |
| Cleanup report | `npm run cleanup:guard` | Prints current script size, stale script references, and stale-term samples. |
| Shell syntax | `bash -n devtools/cleanup-guard.sh` | No syntax errors. |
| Deleted reference check | `rg --fixed-strings '<deleted-path-or-name>' .` | No active references outside archive or recycle-bin notes. |

## Final Gates

| Surface | Command | Pass Condition |
| --- | --- | --- |
| Scripts size | `find scripts -type f -print0 \| xargs -0 wc -l` | `scripts/` is at or below 2,860 LOC. |
| Strict cleanup guard | `npm run cleanup:guard:strict` | No retired dogfood script references; scripts size target met. |
| Rust | `cargo test` | Workspace tests pass, or affected crates pass with a documented full-run follow-up. |
| Swift | `swift test --package-path macos` | macOS package tests pass. |
| Wiki engine | `npm test --prefix wiki-engine` | Wiki engine tests and schema checks pass. |
| Release runner | `npm --prefix release/runner run build && npm --prefix release/runner test` | Release runner builds and tests pass. |
| Release train | `./scripts/release-train.sh validate --channel dev` | Dev release validation passes. |
| Browser/GUI | `npx playwright test` | Checked-in Playwright specs pass. |
| macOS app | `./scripts/release-train.sh build --channel dev` plus installed `1context-cli diagnose` | Stable dev app builds, installs, and diagnoses cleanly. |

## Dev Permission Probe

Permission/TCC validation is not part of normal cleanup unless the slice changes
permissions, bundle identity, capture permissions, or install behavior. When it
is required, use exactly one timestamped dev probe path from `AGENTS.md` and
report:

- `BUILD_TIME`
- installed app path under `/Applications`
- `/usr/bin/time -p` `real/user/sys`
- live permission probe evidence path

## What Fails A Cleanup Slice

- A deleted subsystem is still referenced by CI, build scripts, package
  manifests, active docs, or tests.
- A legacy behavior is recreated as an adapter, bridge, normalizer, alias
  resolver, repair mode, upgrade path, or fallback mode.
- Generated proof, screenshot, video, `latest.json`, or evidence output is
  checked in as product contract.
- A test only proves migration, repair, scaffold, alias, fallback, or synthetic
  proof behavior.
