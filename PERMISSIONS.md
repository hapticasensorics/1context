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

This includes config, update state, sockets, pid files, queues, indexes, caches, and logs. The runtime should repair these permissions on startup.

### Installer Owns

The installer owns placement and registration only:

```text
/Applications/1Context.app
/opt/homebrew/bin/1context
~/Library/LaunchAgents/com.haptica.1context*.plist
```

The installer must not silently widen permissions, create root-owned user state, persist development overrides, or hide runtime startup failures.

## Consent Model

Product owns when users are asked for consent. Runtime and platform code enforce the policy.

Current public preview:

- Starts a user LaunchAgent for the menu bar app and local runtime.
- Checks GitHub Releases for updates using a non-cookie, nonpersistent session.
- Can optionally install managed Claude Code settings in `~/.claude/settings.json`.
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
session-end hooks by default. Those hook commands exist as safe no-op/fallback
entry points for future compatibility, but they require explicit future
product consent before installation.

`1context agent integrations uninstall` removes only 1Context-managed hook and
status-line commands. It preserves unrelated Claude settings and user hooks.
Homebrew uninstall also makes a best-effort call to this cleanup path before the
app bundle is removed, so Claude should not keep calling a deleted binary.

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
- Avoid root unless there is a clear, reviewed need.
- Never make local memory world-readable.
- Keep user content separate from runtime state.
- Keep destructive cleanup paths narrow and allowlisted.
- Do not execute install/update commands supplied by remote metadata.
- Do not persist dev environment overrides into release LaunchAgents.
- Make install/start failures visible.
- Redact user home paths in default diagnostic output.

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
