import Foundation

public let oneContextVersion = "0.1.29"
public let oneContextGitHubURL = URL(string: "https://github.com/hapticasensorics/1context")!
public let oneContextLatestReleaseURL = URL(string: "https://github.com/hapticasensorics/1context/releases/latest")!
public let oneContextHomebrewUpdateCommand = """
set -e
echo "Checking Homebrew..."
if ! command -v brew >/dev/null 2>&1; then
  echo "Homebrew is required to update 1Context." >&2
  exit 1
fi
echo "Checking 1Context tap..."
if ! brew --repo hapticasensorics/tap >/dev/null 2>&1; then
  brew tap hapticasensorics/tap
fi
tap_repo="$(brew --repo hapticasensorics/tap)"
echo "Refreshing 1Context cask metadata..."
git -C "$tap_repo" fetch --quiet --no-tags origin main:refs/remotes/origin/main
git -C "$tap_repo" merge --quiet --ff-only refs/remotes/origin/main
echo "Installing 1Context..."
HOMEBREW_NO_AUTO_UPDATE=1 HOMEBREW_NO_INSTALL_CLEANUP=1 brew upgrade --cask hapticasensorics/tap/1context
echo "Checking 1Context..."
1context restart >/dev/null 2>&1 || true
1context --version
"""
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
