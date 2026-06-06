import Foundation
import OneContextAgentRuntime
import OneContextLocalWeb

public struct OneContextAppSetupSnapshot: Codable, Equatable, Sendable {
  public let localWikiAccess: LocalWebSetupSnapshot
  public let rememberingPermissions: OneContextRememberingPermissionsSnapshot
  public let codexRuntime: CodexRuntimeCapabilitySnapshot

  public var requiredReady: Bool {
    localWikiAccess.ready && rememberingPermissions.requiredReady && codexRuntime.requiredReady
  }

  public var localWikiStatus: String {
    localWikiAccess.ready ? "Granted" : "Required"
  }

  public var requiredSummary: String {
    var blockers: [String] = []
    if !localWikiAccess.ready {
      blockers.append(localWikiAccess.blockingSummary)
    }
    if !rememberingPermissions.requiredReady {
      blockers.append(rememberingPermissions.requiredSummary)
    }
    if !codexRuntime.requiredReady {
      blockers.append(codexRuntime.blockingSummary)
    }
    return blockers.isEmpty ? "1Context setup is complete." : blockers.joined(separator: " ")
  }

  public init(
    localWikiAccess: LocalWebSetupSnapshot,
    rememberingPermissions: OneContextRememberingPermissionsSnapshot,
    codexRuntime: CodexRuntimeCapabilitySnapshot = .assumedInstalledReady()
  ) {
    self.localWikiAccess = localWikiAccess
    self.rememberingPermissions = rememberingPermissions
    self.codexRuntime = codexRuntime
  }
}

public enum OneContextAppReadinessState: String, Codable, Sendable {
  case ready
  case needsSetup
  case needsAttention
}

public struct OneContextAppReadinessSnapshot: Codable, Equatable, Sendable {
  public let state: OneContextAppReadinessState
  public let setup: OneContextAppSetupSnapshot
  public let localWeb: LocalWebSnapshot

  public var requiredSetupReady: Bool {
    setup.requiredReady
  }

  public var requiredSetupSummary: String {
    setup.requiredSummary
  }

  public var menuTitle: String {
    switch state {
    case .ready:
      return "1Context Ready"
    case .needsSetup:
      return "1Context Needs Setup"
    case .needsAttention:
      return "1Context Needs Attention"
    }
  }

  public init(
    state: OneContextAppReadinessState,
    setup: OneContextAppSetupSnapshot,
    localWeb: LocalWebSnapshot
  ) {
    self.state = state
    self.setup = setup
    self.localWeb = localWeb
  }
}

public enum OneContextAppSetup {
  public static func current() -> OneContextAppSetupSnapshot {
    snapshot(
      localWikiAccess: LocalWebSetupInstaller().status(),
      codexRuntime: CodexRuntimeCapability.loadOrProbeAndPersist()
    )
  }

  public static func snapshot(
    localWikiAccess: LocalWebSetupSnapshot,
    rememberingPermissions: OneContextRememberingPermissionsSnapshot = OneContextSystemPermissions.current(),
    codexRuntime: CodexRuntimeCapabilitySnapshot = .assumedInstalledReady()
  ) -> OneContextAppSetupSnapshot {
    OneContextAppSetupSnapshot(
      localWikiAccess: localWikiAccess,
      rememberingPermissions: rememberingPermissions,
      codexRuntime: codexRuntime
    )
  }
}

public enum OneContextAppReadiness {
  public static func current(
    localWeb: CaddyManager = CaddyManager()
  ) -> OneContextAppReadinessSnapshot {
    snapshot(
      localWebDiagnostics: localWeb.diagnostics(),
      codexRuntime: CodexRuntimeCapability.loadOrProbeAndPersist()
    )
  }

  public static func snapshot(
    localWebDiagnostics: LocalWebDiagnostics,
    rememberingPermissions: OneContextRememberingPermissionsSnapshot = OneContextSystemPermissions.current(),
    codexRuntime: CodexRuntimeCapabilitySnapshot = .assumedInstalledReady()
  ) -> OneContextAppReadinessSnapshot {
    let setup = OneContextAppSetup.snapshot(
      localWikiAccess: localWebDiagnostics.setup,
      rememberingPermissions: rememberingPermissions,
      codexRuntime: codexRuntime
    )
    let state: OneContextAppReadinessState
    if !setup.requiredReady {
      state = .needsSetup
    } else if !localWebDiagnostics.caddyExecutableExists || !localWebDiagnostics.caddyExecutableIsExecutable {
      state = .needsAttention
    } else {
      state = .ready
    }
    return OneContextAppReadinessSnapshot(
      state: state,
      setup: setup,
      localWeb: localWebDiagnostics.snapshot
    )
  }
}

public enum OneContextAppSetupDiagnostics {
  public static func render(
    _ snapshot: OneContextAppSetupSnapshot,
    redact: (String) -> String = { $0 }
  ) -> [String] {
    var lines = [
      "Local Wiki Access: \(snapshot.localWikiStatus)",
      "Local Wiki URL: \(snapshot.localWikiAccess.targetURL)",
      "Codex Runtime: \(snapshot.codexRuntime.displayStatus)",
      "Codex Runtime Detail: \(snapshot.codexRuntime.userDetail)"
    ]
    lines.append(contentsOf: snapshot.rememberingPermissions.diagnosticPermissions.map {
      "\($0.title): \($0.displayStatus)"
    })
    lines.append(contentsOf: snapshot.rememberingPermissions.diagnosticPermissions.compactMap {
      $0.ready ? nil : "\($0.title) Detail: \($0.detail)"
    })
    lines.append(contentsOf: OneContextSystemPermissions.fullDiskAccessDiagnostics(redact: redact))
    lines.append(contentsOf: LocalWebSetupDiagnostics.render(snapshot.localWikiAccess, redact: redact).map {
      $0.trimmingCharacters(in: .whitespaces)
    })
    return lines
  }
}

public enum OneContextAppReadinessDiagnostics {
  public static func render(_ snapshot: OneContextAppReadinessSnapshot) -> [String] {
    [
      "App Readiness: \(display(snapshot.state))",
      "Required Setup: \(snapshot.requiredSetupReady ? "ready" : "needs setup")",
      "Setup Summary: \(snapshot.requiredSetupSummary)",
      "Codex Runtime: \(snapshot.setup.codexRuntime.userDetail)",
      "Local Wiki: \(snapshot.localWeb.running ? "reachable" : snapshot.localWeb.health)",
      "Local Wiki URL: \(snapshot.localWeb.url)"
    ]
  }

  private static func display(_ state: OneContextAppReadinessState) -> String {
    switch state {
    case .ready:
      return "ready"
    case .needsSetup:
      return "needs setup"
    case .needsAttention:
      return "needs attention"
    }
  }
}
