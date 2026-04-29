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
- Does not upload project data.
- Does not request Screen Recording, Accessibility, Microphone, Calendar, Contacts, or broad file permissions.

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
