import Foundation

public enum OneContextBlockedSetupAction: String, Codable, Equatable, Sendable {
  case openWiki = "open_wiki"
  case refreshWiki = "refresh_wiki"

  public var setupMessage: String {
    switch self {
    case .openWiki:
      return "Finish setup to open your wiki."
    case .refreshWiki:
      return "Finish setup to refresh your wiki."
    }
  }

  public var resumesAfterSetup: Bool {
    switch self {
    case .openWiki, .refreshWiki:
      return true
    }
  }
}

public struct OneContextSetupContinuation: Equatable, Sendable {
  public private(set) var pendingAction: OneContextBlockedSetupAction?

  public init(pendingAction: OneContextBlockedSetupAction? = nil) {
    self.pendingAction = pendingAction
  }

  @discardableResult
  public mutating func block(_ action: OneContextBlockedSetupAction) -> String {
    pendingAction = action
    return action.setupMessage
  }

  public mutating func consumeResumableActionAfterSetup() -> OneContextBlockedSetupAction? {
    defer {
      pendingAction = nil
    }
    guard pendingAction?.resumesAfterSetup == true else {
      return nil
    }
    return pendingAction
  }

  public mutating func clear() {
    pendingAction = nil
  }
}
