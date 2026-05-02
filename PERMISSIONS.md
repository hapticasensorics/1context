# Permissions

1Context is local-first software. Permission and storage behavior should be boring, inspectable, and owned by the logged-in user.

## Ownership Model

### User Owns

User content lives under:

```text
~/1Context/
```

This root is for human-readable wiki files and user-owned content. The user should be able to read, edit, delete, back up, sync, or inspect it with normal user tools. 1Context must not make this directory root-owned or opaque.

### Runtime Owns

Runtime and app machinery lives under:

```text
~/Library/Application Support/1Context/
~/Library/Logs/1Context/
~/Library/Caches/1Context/
```

These paths are current-user owned and private by default:

```text
directories: 0700
files:       0600
sockets:     0600
```

This includes config, sockets, pid files, queues, indexes, caches, and logs. The runtime should repair these permissions on startup.

Memory-core adapter state lives under:

```text
~/Library/Application Support/1Context/memory-core/
~/Library/Logs/1Context/memory-core.log
```

The adapter stores only configuration, state, and support logs. It does not
bundle private memory logic, run implicitly during install, or scan files on its
own. Configuring a memory core requires an explicit executable path.

### Installer Owns

The installer owns placement and registration only:

```text
/Applications/1Context.app
/Applications/1Context.app/Contents/MacOS/1context-cli
~/Library/LaunchAgents/com.haptica.1context*.plist
1Context.app/Contents/Library/LaunchDaemons/com.haptica.1context.local-web-proxy.plist
~/Library/Application Support/1Context/local-web/setup/
```

The local-web helper is bundled inside the signed app and registered with
macOS ServiceManagement. Local CA trust metadata lives in user-owned app
support. Neither location may contain user wiki content or memory data.

The installer must not silently widen permissions, create root-owned user state,
persist development overrides, or hide runtime startup failures.

## Consent Model

Product owns when users are asked for consent. Runtime and platform code enforce the policy.

Current public preview:

- Starts a user LaunchAgent for the menu bar app and local runtime.
- Uses native app UI, ServiceManagement background-item approval, and user
  keychain trust for `https://wiki.1context.localhost`.
- Keeps native update checks behind app-owned signed release infrastructure.
- Can optionally install managed Claude Code settings in `~/.claude/settings.json`.
- Can optionally configure an external memory-core executable under the public app support directory.
- Does not upload project data.
- Does not request Screen Recording, Accessibility, Microphone, Calendar, Contacts, or broad file permissions.

### Agent Hooks

`1context agent integrations install` currently modifies Claude Code user
settings only. It adds:

```text
~/.claude/settings.json
  hooks.SessionStart[] -> 1context agent hook --provider claude --event SessionStart
  statusLine          -> 1context agent statusline --provider claude
```

The public preview does not install prompt-submit, tool-use, pre-compact, or
session-end hooks by default. Those hook commands are reserved no-op entry
points, and require explicit product consent before installation.

`1context agent integrations uninstall` removes only 1Context-managed hook and
status-line commands. It preserves unrelated Claude settings and user hooks.
The uninstall path should call this cleanup before the app bundle is removed, so
Claude should not keep calling a deleted binary.

If Claude settings contain `disableAllHooks`, 1Context reports manual review and
does not install or repair hooks.

Hook commands receive Claude's event JSON on stdin. The public preview uses only
minimal fields such as `cwd` to return a small local wiki/repo pointer. It does
not upload hook payloads. Release hooks ignore `ONECONTEXT_*` environment
overrides unless `ONECONTEXT_AGENT_ALLOW_ENV_OVERRIDES=1` is explicitly set for
local development/testing.

Future capture/orchestration features must ask only when needed and explain why the permission is needed before relying on it.

Expected future permission owners:

```text
Screen Recording       capture/vision feature, product-approved prompt
Accessibility          automation/control feature, product-approved prompt
Microphone             meeting/audio feature, product-approved prompt
Browser/MCP surfaces   explicit connector setup
Diagnostics            user-initiated support flow
```

## Security Invariants

- Run as the logged-in user.
- Avoid root unless there is a clear, reviewed need. The local HTTPS proxy is
  the reviewed exception: it binds only `127.0.0.1:443` and forwards encrypted
  TCP to the user-owned Caddy backend.
- Never make local memory world-readable.
- Keep user content separate from runtime state.
- Keep destructive cleanup paths narrow and allowlisted.
- Do not execute install/update commands supplied by remote metadata.
- Do not persist dev environment overrides into release LaunchAgents.
- Make install/start failures visible.
- Redact user home paths in default diagnostic output.
- Never run memory-core commands implicitly from install, diagnose, or lifecycle commands.
- Keep memory-core subprocess calls explicit, allowlisted, timeout-bounded, and JSON-validated.

## Diagnostics

`1context diagnose` and `1context debug` redact the user home directory by default. Use:

```bash
1context diagnose --no-redact
```

only for local debugging when exact paths are needed.

Logs live under:

```text
~/Library/Logs/1Context/
```

Logs are support/debug information, not user content.
