# Roadmap

## Update Checks

1Context is moving to app-native updates. The shipped product should update from
the signed app without launching Terminal or depending on a package manager.

Current path:

- Native updater diagnostics live in `OneContextUpdate`.
- The menu bar update command is app-owned and ready for a Sparkle adapter.
- Release packaging produces a signed `.app`, DMG, bundled CLI, daemon, Caddy,
  memory-core resources, and local HTTPS helper.

This avoids tying a core product feature to the user's shell or package-manager
state.

Future path:

- Add Sparkle for signed appcast updates.
- Keep `1context update` as a CLI support path that invokes the same app-owned
  updater domain where possible.
- Support stable/beta channels, minimum supported versions, security notices,
  and release notes.

## Packaging

- macOS public preview ships as a signed DMG containing `1Context.app`.
- Apple Silicon and macOS 13 Ventura or newer are required.
- The app owns setup, local web, runtime startup, and update checks.
- `1contextd` remains internal implementation plumbing.

## Uninstall

Uninstall should be understandable from the app era: quit the app, remove the
bundle, and optionally remove user-owned data and app-owned helper state.

Future path:

- Add `1context uninstall` for friendly app/runtime removal.
- Add `1context uninstall --delete-data` for full local data cleanup.
- Add an app-facing cleanup path for Local Wiki Access, LaunchAgents, and future
  ServiceManagement helper state.

## Product Runtime

The public repo currently validates installation, menu-bar control, local runtime lifecycle, and update plumbing.

The deeper context engine, project memory, wiki generation, MCP surfaces, and capture flows are still in active development.
