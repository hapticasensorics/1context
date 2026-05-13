# Permissions

1Context is local-first software. Permission and storage behavior should be boring, inspectable, and owned by the logged-in user.

## Ownership Model

### User Owns

User content lives under:

```text
~/1Context/
```

This root is for user-created wiki files and user-owned content. The user should
be able to read, edit, delete, back up, sync, or inspect it with normal user
tools. 1Context must not make this directory root-owned or opaque.

A fresh install may show polished default wiki pages even while `~/1Context/` is
empty. Those shipped system-shell pages are app-owned templates and generated
runtime state, not user-authored content.

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

The installed local wiki publication, including shipped template pages and the
current static site served by Caddy, belongs here. Runtime-generated wiki state
must remain separate from durable user-authored content unless the product
explicitly promotes it into `~/1Context/`.

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
- Does not upload project data.
- Does not request Screen Recording, Accessibility, Microphone, Calendar, Contacts, or broad file permissions.

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

## Diagnostics

`1context diagnose` is always redacted. Raw paths and private support details
belong in internal release evidence, not the public CLI.

Logs live under:

```text
~/Library/Logs/1Context/
```

Logs are support/debug information, not user content.
