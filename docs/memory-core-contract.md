# Memory Core Contract

The public 1Context app is the macOS shell: install, update, lifecycle,
diagnostics, permissions, hooks, and user-visible controls.

The memory core is the engine behind that shell. It may be implemented in
Python, Rust, or another runtime, but it must be reached through this explicit
subprocess contract. Public Swift code must not import private memory-engine
modules or depend on a developer checkout.

## Ownership

Public 1Context owns:

- CLI and menu bar UX.
- Runtime lifecycle and LaunchAgents.
- App paths, file permissions, diagnostics, and redaction.
- Hook entrypoints and hook installation.
- Memory-core configuration, health checks, timeout handling, and process
  supervision.

Memory core owns:

- Storage and lake semantics.
- Wiki routing and rendering.
- Memory ticks, replay dry-runs, and cycle validation.
- Future business logic for context, indexing, and agent orchestration.

## Configuration

Public stores adapter state under:

```text
~/Library/Application Support/1Context/memory-core/config.json
~/Library/Application Support/1Context/memory-core/state.json
~/Library/Logs/1Context/memory-core.log
```

Directories must be `0700`; files must be `0600`.

Default config:

```json
{
  "schema_version": 1,
  "enabled": false,
  "executable": null,
  "default_timeout_seconds": 10,
  "allowed_commands": ["status", "storage", "wiki", "memory"]
}
```

## Process Rules

Public invokes the configured executable directly as a subprocess. It does not
go through a shell.

The memory core must:

- Write machine-readable JSON to stdout.
- Write debug/error text to stderr.
- Exit nonzero on failure.
- Avoid printing prompts, transcripts, secrets, or user content in default error
  output.
- Complete within the configured timeout.

Public must:

- Enforce the top-level command allowlist.
- Validate JSON stdout before returning success.
- Capture stderr with redaction for diagnostics.
- Never run memory-core commands implicitly during install, update, diagnose, or
  lifecycle commands.

## Initial Commands

The adapter is intentionally narrow. Initial accepted command families:

```bash
status --json
storage init --json
wiki list --json
wiki ensure --json
wiki render --json
wiki routes --json
memory tick --wiki-only --json
memory replay-dry-run --start <iso8601> --end <iso8601> [--sources <ids>] [--replay-run-id <id>] --json
memory cycles list --json
memory cycles show <cycle-id> --json
memory cycles validate <cycle-id> --json
```

Hired-agent live execution, transcript import, broad filesystem scanning, and
web research are not part of this contract yet.

## Memory Core Target Surface

The memory core exposes a larger Python CLI. The public adapter targets only
the stable-looking front edge of that surface:

```text
storage init
wiki list
wiki ensure
wiki render
wiki routes
memory tick --wiki-only
memory replay-dry-run with bounded time range
memory cycles list/show/validate with explicit cycle ids
```

The memory core may grow broader commands for apps, ports, daemon loops,
hired-agent jobs, scheduler work, migrations, quality probes, route dry-runs,
and wiki apply/promotion. Those stay outside the public adapter until they have
explicit product consent, timeout budgets, and diagnostics semantics.

When a memory core is packaged into the app, it should provide a small
executable that speaks this contract. The public Swift app should continue to
treat it as a subprocess boundary, not a linked library.

## JSON Shape

The exact business payload can evolve, but every successful response should
include:

```json
{
  "status": "ok",
  "schema_version": 1
}
```

Failures should prefer a nonzero exit code plus a compact JSON error when the
process reaches application-level handling:

```json
{
  "status": "error",
  "schema_version": 1,
  "error": {
    "code": "not_ready",
    "message": "Memory core is not initialized."
  }
}
```

Transport-level failures may return non-JSON stderr; public 1Context will treat
those as degraded and redact diagnostics.

## Hook Integration

Hooks must stay safe when memory core is absent. The public hook bridge may
report whether a memory core is configured, but it must not synchronously run
heavy memory work by default.

Future prompt-aware retrieval should be behind an explicit flag, short timeout,
and small output budget.

## Compatibility Test

Run the public contract harness:

```bash
./scripts/test-memory-core-contract.sh
```

The included fixture lives at:

```text
scripts/fixtures/memory-core-fixture.sh
```

Private memory-core entrypoints should aim to pass the same command shape before
being wired into the public adapter.
