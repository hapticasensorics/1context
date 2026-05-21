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

Required 1Context Remembering inventory:

- Screen & System Audio Recording: pixels, display truth, window evidence, and
  optional system-audio context.
- Accessibility: focused app/window, UI semantics, titles, controls, and window
  change events.
- Input Monitoring: coarse UX anchors such as focus, click, scroll, and command
  timing. It must not persist raw typed text by default.
- Browser extension permissions: URL, tab, DOM, selected text, scroll position,
  and page metadata for browser-heavy work.
- Microphone: spoken context when audio remembering is enabled.
- Automation: app-specific Apple Events when a source lane needs direct app
  context or control.
- Full Disk Access: session logs, Messages, browser history, app databases, and
  durable local memory locations across the Mac.

Setup enforcement:

- Starts a user LaunchAgent for the menu bar app and local runtime.
- Uses native app UI, ServiceManagement background-item approval, and user
  keychain trust for `https://wiki.1context.localhost`.
- Keeps native update checks behind app-owned signed release infrastructure.
- Hard-gates Local Wiki Access, Screen & System Audio Recording, Accessibility,
  Input Monitoring, and Microphone before 1Context Remembering starts.
- Owns Browser extension permissions, Automation, and Full Disk Access from the
  same 1Context Setup page. These are not separate products or optional
  afterthoughts; their setup actions may launch browser, app-specific Automation,
  or Full Disk Access prompts under the hood, but the user-facing flow remains
  one 1Context setup gate.
- Shows `Granted` only after the signed app has enough proof for that lane.
  Native preflight APIs are not treated as the whole truth when they can be
  satisfied without a runtime proof. Input Monitoring must follow the IOHID
  listen-event gate that populates macOS Input Monitoring, then record proof
  only after the signed app can preflight that grant. Browser extension and
  Automation setup proofs are scoped to the current app identity so a
  timestamped permission-test app cannot inherit stale green state from the
  stable dev app. Full Disk Access is probed against the protected data paths
  that actually exist on the Mac, and all existing probes must be readable.
- Does not upload project data.
- Does not request Calendar or Contacts.

Development and TCC identity policy:

- Stable dev builds must keep the same bundle identifier,
  `com.haptica.1context.dev`, and must be signed with the local
  `Apple Development` identity. macOS TCC grants are tied to the app's
  designated code-signing requirement, not just the visible app name or bundle
  id. Ad-hoc rebuilds can therefore look granted in System Settings while the
  live app's preflight APIs still report Required.
- Permission-flow testing may intentionally use a fresh dev bundle identifier
  with `ONECONTEXT_PERMISSION_TEST_ID=<suffix>`. That mode also gets a
  suffix-scoped dev runtime identity, preferences domain, LaunchAgent labels,
  and localhost ports so multiple permission-test apps can run beside the stable
  dev app. It is for exercising first-run prompts and TCC reset behavior; it
  should not be used to judge whether normal dev rebuilds preserve
  already-granted access.
- Local Wiki Access is product setup state, not a macOS TCC grant. It may be
  re-granted for fresh permission-test runtime identities; that is setup state,
  not evidence that macOS privacy permissions were reset.

## Permission Implementation Fix Checklist

This checklist tracks the work needed before the setup page can be treated as a
true proof of the full 1Context Remembering permission surface. `Granted` should
mean the signed app, or the exact signed helper that owns the lane, has proved
the relevant runtime capability.

### Cross-Cutting Proof Model

- [x] Store and compare the current designated code requirement, or a stable
  digest of `codesign -dr`, in every local setup proof record. Bundle identifier
  alone is not enough for TCC identity.
- [x] Clear stale setup proof records when the bundle identifier, designated
  requirement, app version, helper identity, or proof schema no longer matches.
- [x] Decide which signed process owns each permission lane. If a helper,
  daemon, XPC service, or CLI performs the protected action, request and prove
  the permission from that subject or move the protected action back into the
  app process.
- [x] Add a permission evidence command or release-train phase that captures the
  installed app path, bundle identifier, `codesign -dr`, entitlements,
  Info.plist usage strings, app name, and setup snapshot for every dev
  permission build.
- [x] Make permission-test builds fail fast unless they are signed with Apple
  Development or Developer ID and the expected hardened-runtime entitlements are
  present.
- [x] Keep timestamped bundle identifiers for fresh prompt testing, but treat
  stable dev bundle identifiers as the source of truth for permission
  persistence testing.
- [x] Give timestamped permission-test builds a matching runtime identity,
  preferences domain, LaunchAgent labels, and localhost ports so they do not
  collide with the stable dev app while testing prompts.
- [x] Preserve the full timestamped permission-test identity in LaunchAgent
  environment variables. A `dev-permission:<suffix>` app must not relaunch as
  the generic `dev` identity after login, crash recovery, or runtime restart.

### Setup UI And State

- [x] Keep one 1Context setup page for all Remembering permissions.
- [x] Show `Granted` only after the lane-specific proof passes for the current
  TCC subject.
- [x] Add an explicit `Needs Relaunch` or equivalent state when macOS requires
  a restart before a newly granted permission is usable.
- [x] Make `Check Again` re-run live platform probes and discard invalid local
  proof instead of only rereading persisted setup state.
- [x] On setup polling, app launch, and user refresh, automatically promote
  already-authorized OS grants into current signed proof records instead of
  requiring the user to click `Grant` again after relaunch.
- [x] Keep Local Wiki Access separate from TCC-backed permissions because it is
  product setup state, not a macOS privacy grant.

### Screen And System Audio Recording

- [x] Stop treating `CGPreflightScreenCaptureAccess()` as proof of the combined
  Screen & System Audio Recording lane.
- [x] Either split the UI into Screen Recording and System Audio Recording, or
  keep one row and require both proofs before it turns green.
- [x] Add a real ScreenCaptureKit pixel proof from the app-owned capture process,
  not only a preflight check.
- [x] Add a real system-audio proof using ScreenCaptureKit audio output or Core
  Audio process taps before marking system audio ready.
- [x] Model the Screen Recording relaunch requirement after first grant.
- [ ] Gate audio capture implementation by OS version: ScreenCaptureKit system
  audio on macOS 13+, Core Audio taps on macOS 14.2+, and ScreenCaptureKit
  microphone capture on macOS 15+ where used.

### Accessibility

- [x] Keep using `AXIsProcessTrusted()` for status and
  `AXIsProcessTrustedWithOptions(...)` for prompting.
- [x] Replace the string literal prompt key with `kAXTrustedCheckOptionPrompt`.
- [x] Treat Accessibility prompting as asynchronous and keep polling after the
  user returns from System Settings.
- [ ] Add a focused-window or AX observer smoke proof if Accessibility becomes a
  hard runtime gate beyond consent.

### Input Monitoring

- [x] Use `IOHIDCheckAccess(kIOHIDRequestTypeListenEvent)` for status because
  it is the gate that corresponds to the macOS Input Monitoring list.
- [x] Use `IOHIDRequestAccess(kIOHIDRequestTypeListenEvent)` for prompting so
  1Context appears in Input Monitoring instead of getting a misleading green
  state from broader event APIs.
- [x] Remove the mixed keyboard/mouse/scroll event-tap proof because macOS can
  clear unpermitted event bits and still create a tap.
- [x] If an event-tap proof remains, make it key-only or otherwise prove the
  exact event class needed by the runtime.
- [x] Record Input Monitoring proof only after the IOHID grant/probe is
  true; never persist "requested once" as proof.
- [x] Open the Input Monitoring System Settings pane as a helper action, but do
  not rely on the user visually spotting the row as the permission source of
  truth.

### Browser Extension Permissions

- [x] Replace the current self-recorded browser-extension proof with an actual
  extension-to-app handshake.
- [x] Add or identify the production extension manifest, extension id, required
  permissions, host permissions, and optional host permissions.
- [x] Use the browser extension permissions API to prove required permissions
  are present.
- [x] Use native messaging or another signed local bridge so the app can verify
  the installed extension id, version, and granted browser permissions.
- [x] Scope browser-extension setup proof to bundle identifier, designated
  requirement, extension id, extension version, and granted permission set.
- [x] Keep browser pixels/OCR as fallback; browser extension data is the
  preferred proof lane for URL, tab, DOM, selected text, and scroll state.

### Microphone

- [x] Keep `NSMicrophoneUsageDescription` and the audio-input entitlement in
  signed builds.
- [x] Keep `AVCaptureDevice.authorizationStatus(for: .audio)` as the consent
  check.
- [x] Add a minimal runtime microphone capture proof from the process that will
  record spoken context.
- [x] Represent `.denied`, `.restricted`, and `.notDetermined` distinctly enough
  in diagnostics to explain why the setup row is not green.

### Automation

- [x] Stop treating Finder Automation as proof of all Automation.
- [x] Define the app targets that 1Context needs to automate, starting with the
  high-signal apps in the Remembering plan.
- [x] Use `AEDeterminePermissionToAutomateTarget` per target bundle identifier
  and per meaningful event class/event id when needed.
- [x] Store Automation proof per target bundle identifier, event scope,
  designated requirement, and timestamp.
- [x] Let the one setup page orchestrate per-app Automation prompts while making
  the UI clear about which target is currently being granted.
- [x] Let users configure the Automation app target set instead of requiring
  hidden installed apps that were never requested in System Settings.

### Full Disk Access

- [x] Keep Full Disk Access instead of Files & Folders for whole-Mac session log
  and app database access.
- [x] Open the current Full Disk Access System Settings URL for macOS 13+.
- [x] Treat Full Disk Access detection as a protected-path runtime probe because
  Apple does not provide a general preflight API.
- [x] Run the probes from the same signed process that will read protected data,
  or add separate helper proof for any helper that reads those paths.
- [x] Keep the probe list grounded in real product needs: Messages, Safari,
  Mail, browser profiles/history, and app databases that exist on the current
  Mac.
- [x] Require all existing protected probes in the selected probe set to be
  readable before showing `Granted`.
- [x] Make diagnostics show which protected probes failed, with redacted paths.

### Tests And Verification

- [x] Add unit tests for proof invalidation on bundle identifier, designated
  requirement, schema, extension id, target app, and app version mismatches.
- [x] Add Swift tests for Input Monitoring using injectable preflight/request
  functions so the wrong permission path cannot regress.
- [x] Add tests that Browser Extension and Automation cannot turn green from
  local self-recording alone.
- [x] Add FDA probe tests for no existing probes, partial readability, all
  readable, and diagnostics redaction.
- [x] Add a permission-test runbook or script that builds a timestamped app,
  installs it, launches setup, records identity evidence, and captures the
  before/after setup snapshot.

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
