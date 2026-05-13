# 1Context Delete Bloat Audit - 2026-05-13

Generated at: `2026-05-13T21:52:00Z`

Comparison base: `dd5ccd2` (`Close 0.1.65 release lockdown goal`)

## Release Surface Deleted Or Internalized

- Deleted old update-policy source: `release/update-policy.toml`,
  `release/update-policy.schema.json`, `scripts/update-policy.py`,
  `scripts/check-update-policy.sh`, and `scripts/test-update-policy.sh`.
- Deleted release wrapper scripts:
  `scripts/package-macos-production-release.sh` and
  `scripts/package-macos-release.sh`.
- Deleted old proof request/evidence scripts:
  `scripts/request-release-proof.sh`,
  `scripts/collect-release-lockdown-evidence.sh`,
  `scripts/classify-release-lockdown-diagnostics.py`,
  `scripts/test-release-proof-request.sh`,
  `scripts/test-release-lockdown-diagnostics.sh`, and
  `scripts/test-upgrade-paths.sh`.
- Deleted `scripts/audit-github-release-assets.sh`; public asset checks now run
  inside `scripts/release-train.sh audit` and `publish`.
- Moved `scripts/self-hosted-update-proof.sh` to
  `scripts/release/internal/self-hosted-update-proof.sh`; the self-hosted
  workflow now executes it through
  `scripts/release-train.sh prove --runner-execute`.

## Shipped App Bloat Deleted

- Deleted the app-bundled `memory-core` copy path and macOS memory-core bridge:
  `OneContextMemoryCore`, `MemoryCoreAdapter`, `MemoryCoreSetup`, and
  `WikiSitePublisher`.
- Deleted first-party agent hook installation from the shipped app:
  `OneContextAgent`, its tests, CLI surface, and docs references.
- Deleted future permission scaffolding from the shipped setup flow:
  `OneContextPermissions`, tests, and public `permissions` CLI.
- Deleted `OneContextRuntimeSupport` re-export glue.
- Deleted app/test env backdoors:
  `ONECONTEXT_VERSION_OVERRIDE`, install prompt overrides, app trash overrides,
  `ONECONTEXT_SMOKE_FIXTURE`, `ONECONTEXT_SMOKE_STATE_DIR`,
  `ONECONTEXT_SHOW_SETUP_ON_LAUNCH`, daemon path overrides, and Sparkle retry
  delay overrides.
- Deleted runtime path/lifecycle fixture backdoors:
  `RuntimePaths.current(environment:)`, LaunchAgent `EnvironmentVariables`
  path persistence, `ONECONTEXT_LAUNCH_AGENT_DISABLED`, uninstall home
  overrides, `scripts/stress-runtime-rpc.sh`, and
  `scripts/test-macos-uninstall-command.sh`.
- Deleted local-web setup path env overrides from shipped setup code:
  `ONECONTEXT_LOCAL_WEB_SYSTEM_*`, proxy executable/plist/cert path overrides,
  and `ONECONTEXT_APP_BUNDLE_PATH`. Tests now pass typed
  `LocalWebSetupSystemPaths` fixtures instead.
- Deleted menu performance logging env scaffolding:
  `ONECONTEXT_MENU_PERF_LOG`, `perfStart`, `perfLog`, and the stderr
  `[1context-menu-perf]` hook.
- Deleted the remaining shipped `ONECONTEXT_*` product env hooks from
  `macos/Sources`; release/build scripts keep explicit operator env vars, but
  product code no longer uses them as hidden behavior switches.
- Deleted obsolete public CLI control-plane commands: `start`, `stop`, `quit`,
  `restart`, `status`, `logs`, `update`, `setup local-web`, and `wiki refresh`.
  The shipped CLI now exposes only version/help, redacted `diagnose`,
  `uninstall`, and `wiki local-url`.
- Deleted unshipped local wiki chat/provider API routes and placeholder JSON.
- Deleted old ad hoc GUI harnesses that preserved product test hooks:
  `scripts/smoke-sparkle-local-appcast.sh`,
  `scripts/harness-macos-clean-machine-checklist.sh`,
  `scripts/harness-macos-launchagent-recovery.sh`,
  `scripts/harness-macos-setup-flow.sh`,
  `scripts/harness-macos-setup-gui.sh`, and
  `scripts/harness-macos-update-current-gui.sh`.

## Tracked State And Docs Deleted

- Deleted tracked runtime wiki state under `memory-core/memory/runtime/wiki`.
- Deleted tracked generated wiki outputs under `memory-core/wiki/generated` and
  `memory-core/wiki/menu/**/generated`.
- Deleted stale update-policy doc/screenshot:
  `docs/update_policy.html` and
  `docs/assets/update-policy/current-update-prompt.png`.
- Deleted unused wiki theme icon variants:
  `memory-core/wiki-engine/theme/assets/onecontext-icon.png` and
  `memory-core/wiki-engine/theme/assets/onecontext-icon-32.png`.
- Deleted `docs/memory-core-contract.md` and memory-core contract tests.

## Negative Checks

- `rg` over active docs, scripts, workflows, and macOS sources finds no public
  references to:
  `scripts/audit-github-release-assets.sh`,
  `run: ./scripts/self-hosted-update-proof.sh`,
  `ONECONTEXT_SMOKE_FIXTURE`,
  `ONECONTEXT_SMOKE_STATE_DIR`,
  `ONECONTEXT_SHOW_SETUP_ON_LAUNCH`,
  `ONECONTEXT_ALLOW_DAEMON_OVERRIDE`,
  `ONECONTEXT_DAEMON_PATH`,
  `ONECONTEXT_SPARKLE_AUTOMATIC_RETRY_DELAYS_SECONDS`,
  `ONECONTEXT_SPARKLE_MANUAL_RETRY_DELAYS_SECONDS`,
  `ONECONTEXT_USER_CONTENT_DIR`,
  `ONECONTEXT_APP_SUPPORT_DIR`,
  `ONECONTEXT_CACHE_DIR`,
  `ONECONTEXT_LOG_DIR`,
  `ONECONTEXT_SOCKET_PATH`,
  `ONECONTEXT_LOG_PATH`,
  `ONECONTEXT_PREFERENCES_PATH`,
  `ONECONTEXT_PERSIST_ENV_PATH_OVERRIDES`,
  `ONECONTEXT_LAUNCH_AGENT_DISABLED`,
  `ONECONTEXT_UNINSTALL_HOME_DIR`,
  `ONECONTEXT_ALLOW_UNINSTALL_HOME_OVERRIDE`,
  `scripts/stress-runtime-rpc.sh`,
  `scripts/test-macos-uninstall-command.sh`,
  `ONECONTEXT_LOCAL_WEB_SYSTEM_SUPPORT_DIR`,
  `ONECONTEXT_LOCAL_WEB_SYSTEM_LOG_DIR`,
  `ONECONTEXT_LOCAL_WEB_PROXY_EXECUTABLE_PATH`,
  `ONECONTEXT_LOCAL_WEB_LAUNCH_DAEMON_PATH`,
  `ONECONTEXT_LOCAL_WEB_TRUSTED_ROOT_CERT_PATH`,
  `ONECONTEXT_LOCAL_WEB_TRUSTED_ROOT_SHA1_PATH`,
  `ONECONTEXT_LOCAL_WEB_TRUSTED_ROOT_SHA256_PATH`,
  `ONECONTEXT_LOCAL_WEB_SETUP_MARKER_PATH`,
  `ONECONTEXT_LOCAL_WEB_PROXY_LOG_PATH`,
  `ONECONTEXT_APP_BUNDLE_PATH`,
  `ONECONTEXT_MENU_PERF_LOG`,
  `perfStart`,
  `perfLog`,
  public CLI lifecycle/setup/update functions,
  `1context-menu-perf`,
  `docs/update_policy.html`,
  `current-update-prompt.png`, or
  `assets/update-policy`.
- `scripts/test-release-train.sh` asserts the old public package/proof/audit
  scripts do not exist and the self-hosted workflow enters through
  `scripts/release-train.sh prove --runner-execute`.
- `scripts/test-launch-agent-package.sh` asserts the packaged app has no
  bundled `memory-core`, generated local goal/wiki source, or dead chat/provider
  API strings.
- `rg -n "ONECONTEXT_[A-Z0-9_]+" macos/Sources` returns no matches.
- `rg` for the deleted CLI switch cases and functions in
  `macos/Sources/OneContextCLI/main.swift` returns no matches.

## Proof Commands

Passed on 2026-05-13:

- `swift test --package-path macos`
- `./scripts/test.sh`
- `./scripts/test-release-train.sh`
- `./scripts/package-macos-smoke.sh`
- `./scripts/test-launch-agent-package.sh`
- `cd memory-core && uv run --with pytest pytest`
- `actionlint .github/workflows/ci.yml .github/workflows/release.yml .github/workflows/self-hosted-mac-update-proof.yml`
- `bash -n scripts/release-train.sh scripts/release/internal/self-hosted-update-proof.sh scripts/test-release-train.sh scripts/build-macos-app.sh scripts/test.sh scripts/package-macos-smoke.sh scripts/test-release-app-product-https.sh`
- `git diff --check`
- `./scripts/measure-shipped-app-lines.sh`

## Size Result

`scripts/measure-shipped-app-lines.sh` reports:

- Baseline: 61,309 nonblank shipped-app lines.
- Current: 7,724 nonblank shipped-app lines.
- Removed: 53,585 lines.
- Reduction: 87.4%.
- Target: at most 24,523 lines for the 60% reduction goal.
