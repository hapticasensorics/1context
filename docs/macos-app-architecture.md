# macOS App Architecture

1Context is a signed macOS app first. The CLI is a support surface, not the product's setup path. Setup, permissions, update, and the local wiki should therefore be modeled as app-owned capabilities with small infrastructure helpers underneath them.

## Current Direction

```mermaid
flowchart TD
  AppLaunch["App launch"] --> Install["OneContextInstall\n/Applications placement"]
  Install --> App["1Context.app\nmenu bar UI"]
  App["1Context.app\nmenu bar UI"] --> Setup["OneContextSetup\nreadiness + required setup model"]
  CLI["1context CLI\nsupport + automation"] --> Setup
  Setup --> LocalWeb["OneContextLocalWeb\nwiki server + diagnostics"]
  Setup --> Permissions["OneContextPermissions\nTCC permission snapshots"]
  App --> Update["OneContextUpdate\nnative app updater adapter"]
  LocalWeb --> Caddy["bundled Caddy\nuser-owned HTTPS backend"]
  LocalWeb --> Proxy["OneContextLocalWebProxy\nSMAppService privileged helper"]
  Proxy --> Port443["127.0.0.1 + ::1 :443"]
  Port443 --> Caddy
```

## Source Boundaries

- `OneContextMenuBar`: owns user-facing setup prompts, permissions UI, update UI, and opening the wiki.
- `OneContextInstall`: owns app placement decisions and moving/relaunching into `/Applications` before setup, update, or runtime chores run.
- `OneContextSetup`: owns the app-level readiness and setup model. It answers “can the required app experience work?” without knowing about AppKit.
- `OneContextPermissions`: owns macOS privacy permission snapshots such as Screen Recording and Accessibility.
- `OneContextLocalWeb`: owns Caddy configuration, local HTTPS diagnostics, certificate trust installation, and ServiceManagement registration.
- `OneContextLocalWebProxy`: stays intentionally tiny. It only binds the privileged local HTTPS port and forwards bytes to the user-owned Caddy backend.
- `OneContextUpdate`: owns native app update state. Sparkle can land behind this boundary without changing menu or CLI callers.
- `OneContextCLI`: supports diagnostics, automation, and repair. It should route users back to the app-owned permissions/setup surface when required setup is missing.

## Setup Policy

```mermaid
stateDiagram-v2
  [*] --> AppLaunch
  AppLaunch --> MovePrompt: launched outside /Applications
  MovePrompt --> AppRelaunch: user chooses Move
  AppRelaunch --> AppLaunch: /Applications/1Context.app
  MovePrompt --> SetupReady: user chooses Not Now and setup is ready
  MovePrompt --> PermissionsUI: user chooses Not Now and setup is missing
  AppLaunch --> SetupReady: required setup granted
  AppLaunch --> PermissionsUI: required setup missing
  PermissionsUI --> CertificateTrust: grant local certificate trust
  CertificateTrust --> BackgroundHelper: approve local HTTPS helper
  BackgroundHelper --> SetupReady: helper enabled and reachable
  BackgroundHelper --> PermissionsUI: user chooses Later
  SetupReady --> WikiOpen
  WikiOpen --> PermissionsUI: setup later becomes stale or missing
```

The required launch gate is Local Wiki Access because the app's primary wiki URL is `https://wiki.1context.localhost/your-context`. Future sensitive permissions, such as Screen Recording and Accessibility, should be added to `OneContextPermissions` and surfaced through `OneContextSetup` before feature code depends on them.

## Smoke Policy

The deterministic release smoke test keeps using high-port HTTP because it must run in CI without modifying the machine. Product HTTPS gets a separate opt-in smoke test because it intentionally touches macOS user trust and background item approval.
