import Foundation

public let oneContextVersion = "0.1.44"
public let oneContextGitHubURL = URL(string: "https://github.com/hapticasensorics/1context")!
public let oneContextLatestReleaseURL = URL(string: "https://github.com/hapticasensorics/1context/releases/latest")!
public let oneContextHomebrewUpdateCommand = "brew upgrade --cask hapticasensorics/tap/1context"
public let oneContextUpdateCheckInterval: TimeInterval = 24 * 60 * 60


public struct RuntimeHealth: Codable, Sendable {
  public let status: String
  public let version: String
  public let uptimeSeconds: Int
  public let pid: Int32

  public init(status: String, version: String, uptimeSeconds: Int, pid: Int32) {
    self.status = status
    self.version = version
    self.uptimeSeconds = uptimeSeconds
    self.pid = pid
  }
}

public enum RuntimeState: String, Codable, Sendable {
  case running
  case stopped
  case needsAttention
}

public struct RuntimeSnapshot: Codable, Sendable {
  public let state: RuntimeState
  public let health: RuntimeHealth?
  public let lastErrorDescription: String?
  public let recommendedAction: String?

  public init(
    state: RuntimeState,
    health: RuntimeHealth? = nil,
    lastErrorDescription: String? = nil,
    recommendedAction: String? = nil
  ) {
    self.state = state
    self.health = health
    self.lastErrorDescription = lastErrorDescription
    self.recommendedAction = recommendedAction
  }
}


public enum RuntimeControlError: Error, LocalizedError {
  case daemonNotFound
  case missingPID
  case launchAgentFailed(String)
  case timedOut(String)
  case unsafeDeletionPath(String)
  case rootUserUnsupported

  public var errorDescription: String? {
    switch self {
    case .daemonNotFound:
      return "1Context runtime is not installed"
    case .missingPID:
      return "1Context is running, but did not report a process id"
    case .launchAgentFailed(let message):
      return message.isEmpty ? "Could not start 1Context" : message
    case .timedOut(let message):
      return message
    case .unsafeDeletionPath(let path):
      return "Refusing to delete unsafe path: \(path)"
    case .rootUserUnsupported:
      return "Run 1Context as your normal macOS user, not with sudo or as root"
    }
  }
}


public func compareVersions(_ lhs: String, _ rhs: String) -> Int {
  let left = versionComponents(lhs)
  let right = versionComponents(rhs)
  for index in 0..<max(left.count, right.count) {
    let difference = (index < left.count ? left[index] : 0) - (index < right.count ? right[index] : 0)
    if difference != 0 { return difference }
  }
  return 0
}

private func versionComponents(_ version: String) -> [Int] {
  version
    .replacingOccurrences(of: "^v", with: "", options: .regularExpression)
    .split(separator: ".")
    .map { Int($0) ?? 0 }
}
