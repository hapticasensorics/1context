import Foundation

public let oneContextVersion = "0.1.51"
public let oneContextGitHubURL = URL(string: "https://github.com/hapticasensorics/1context")!


public struct RuntimeHealth: Codable, Sendable {
  public let status: String
  public let version: String
  public let uptimeSeconds: Int
  public let pid: Int32
  public let currentTime: String?
  public let requiredSetupReady: Bool?
  public let requiredSetupSummary: String?

  public init(
    status: String,
    version: String,
    uptimeSeconds: Int,
    pid: Int32,
    currentTime: String? = nil,
    requiredSetupReady: Bool? = nil,
    requiredSetupSummary: String? = nil
  ) {
    self.status = status
    self.version = version
    self.uptimeSeconds = uptimeSeconds
    self.pid = pid
    self.currentTime = currentTime
    self.requiredSetupReady = requiredSetupReady
    self.requiredSetupSummary = requiredSetupSummary
  }

  public init(status: String, version: String, uptimeSeconds: Int, pid: Int32, currentTime: String? = nil) {
    self.init(
      status: status,
      version: version,
      uptimeSeconds: uptimeSeconds,
      pid: pid,
      currentTime: currentTime,
      requiredSetupReady: nil,
      requiredSetupSummary: nil
    )
  }

  private enum CodingKeys: String, CodingKey {
    case status
    case version
    case uptimeSeconds
    case pid
    case currentTime
    case requiredSetupReady
    case requiredSetupSummary
  }

  public init(from decoder: Decoder) throws {
    let container = try decoder.container(keyedBy: CodingKeys.self)
    self.status = try container.decode(String.self, forKey: .status)
    self.version = try container.decode(String.self, forKey: .version)
    self.uptimeSeconds = try container.decode(Int.self, forKey: .uptimeSeconds)
    self.pid = try container.decode(Int32.self, forKey: .pid)
    self.currentTime = try container.decodeIfPresent(String.self, forKey: .currentTime)
    self.requiredSetupReady = try container.decodeIfPresent(Bool.self, forKey: .requiredSetupReady)
    self.requiredSetupSummary = try container.decodeIfPresent(String.self, forKey: .requiredSetupSummary)
  }

  public func encode(to encoder: Encoder) throws {
    var container = encoder.container(keyedBy: CodingKeys.self)
    try container.encode(status, forKey: .status)
    try container.encode(version, forKey: .version)
    try container.encode(uptimeSeconds, forKey: .uptimeSeconds)
    try container.encode(pid, forKey: .pid)
    try container.encodeIfPresent(currentTime, forKey: .currentTime)
    try container.encodeIfPresent(requiredSetupReady, forKey: .requiredSetupReady)
    try container.encodeIfPresent(requiredSetupSummary, forKey: .requiredSetupSummary)
  }
}

public enum RuntimeState: String, Codable, Sendable {
  case running
  case stopped
  case needsSetup
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
