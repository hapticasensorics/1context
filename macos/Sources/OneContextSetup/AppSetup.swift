import Foundation
import OneContextLocalWeb
import OneContextPermissions
import OneContextUpdate

public struct OneContextAppSetupSnapshot: Codable, Equatable, Sendable {
  public let localWikiAccess: LocalWebSetupSnapshot
  public let sensitivePermissions: [PermissionSnapshot]

  public var requiredReady: Bool {
    localWikiAccess.ready
  }

  public var localWikiStatus: String {
    localWikiAccess.ready ? "Granted" : "Required"
  }

  public var requiredSummary: String {
    localWikiAccess.ready ? "1Context setup is complete." : localWikiAccess.blockingSummary
  }

  public init(
    localWikiAccess: LocalWebSetupSnapshot,
    sensitivePermissions: [PermissionSnapshot]
  ) {
    self.localWikiAccess = localWikiAccess
    self.sensitivePermissions = sensitivePermissions
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
  public let nativeUpdate: NativeUpdateSnapshot?

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
    localWeb: LocalWebSnapshot,
    nativeUpdate: NativeUpdateSnapshot?
  ) {
    self.state = state
    self.setup = setup
    self.localWeb = localWeb
    self.nativeUpdate = nativeUpdate
  }
}

public enum OneContextAppSetup {
  public static func current(checkSensitivePermissionsInCurrentProcess: Bool = false) -> OneContextAppSetupSnapshot {
    snapshot(
      localWikiAccess: LocalWebSetupInstaller().status(),
      checkSensitivePermissionsInCurrentProcess: checkSensitivePermissionsInCurrentProcess
    )
  }

  public static func snapshot(
    localWikiAccess: LocalWebSetupSnapshot,
    checkSensitivePermissionsInCurrentProcess: Bool = false
  ) -> OneContextAppSetupSnapshot {
    let permissions = PermissionReporter(
      checker: MacOSPermissionChecker(checkCurrentProcess: checkSensitivePermissionsInCurrentProcess)
    ).snapshots()
    return OneContextAppSetupSnapshot(
      localWikiAccess: localWikiAccess,
      sensitivePermissions: permissions
    )
  }
}

public enum OneContextAppReadiness {
  public static func current(
    localWeb: CaddyManager = CaddyManager(),
    checkSensitivePermissionsInCurrentProcess: Bool = false,
    nativeUpdate: NativeUpdateSnapshot? = nil
  ) -> OneContextAppReadinessSnapshot {
    snapshot(
      localWebDiagnostics: localWeb.diagnostics(),
      checkSensitivePermissionsInCurrentProcess: checkSensitivePermissionsInCurrentProcess,
      nativeUpdate: nativeUpdate
    )
  }

  public static func snapshot(
    localWebDiagnostics: LocalWebDiagnostics,
    checkSensitivePermissionsInCurrentProcess: Bool = false,
    nativeUpdate: NativeUpdateSnapshot? = nil
  ) -> OneContextAppReadinessSnapshot {
    let setup = OneContextAppSetup.snapshot(
      localWikiAccess: localWebDiagnostics.setup,
      checkSensitivePermissionsInCurrentProcess: checkSensitivePermissionsInCurrentProcess
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
      localWeb: localWebDiagnostics.snapshot,
      nativeUpdate: nativeUpdate
    )
  }
}

public enum OneContextAppSetupDiagnostics {
  public static func render(_ snapshot: OneContextAppSetupSnapshot) -> [String] {
    var lines = [
      "Local Wiki Access: \(snapshot.localWikiStatus)",
      "Local Wiki URL: \(snapshot.localWikiAccess.targetURL)"
    ]
    lines.append(contentsOf: LocalWebSetupDiagnostics.render(snapshot.localWikiAccess).map {
      $0.trimmingCharacters(in: .whitespaces)
    })
    lines.append("")
    lines.append("Sensitive Permissions:")
    lines.append(contentsOf: PermissionDiagnostics.render(snapshot.sensitivePermissions).map {
      $0.trimmingCharacters(in: .whitespaces)
    })
    return lines
  }
}

public enum OneContextAppReadinessDiagnostics {
  public static func render(_ snapshot: OneContextAppReadinessSnapshot) -> [String] {
    var lines = [
      "App Readiness: \(display(snapshot.state))",
      "Required Setup: \(snapshot.requiredSetupReady ? "ready" : "needs setup")",
      "Setup Summary: \(snapshot.requiredSetupSummary)",
      "Local Wiki: \(snapshot.localWeb.running ? "reachable" : snapshot.localWeb.health)",
      "Local Wiki URL: \(snapshot.localWeb.url)"
    ]
    if let nativeUpdate = snapshot.nativeUpdate {
      lines.append("Native Update: \(nativeUpdate.availability.rawValue)")
    }
    return lines
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
