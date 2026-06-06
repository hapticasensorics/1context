import Foundation

public enum AgentOfficeLifecycle: String, Codable, Equatable, Sendable {
  case idle
  case planned
  case born
  case working
  case postedMail
  case completed
  case waiting
  case failed
  case retired

  public var isActive: Bool {
    switch self {
    case .born, .working, .postedMail:
      return true
    case .idle, .planned, .completed, .waiting, .failed, .retired:
      return false
    }
  }
}

public struct AgentOfficeAgentSnapshot: Equatable, Sendable {
  public var unitID: String
  public var jobID: String
  public var role: String
  public var phase: String
  public var lifecycle: AgentOfficeLifecycle
  public var hasTalkReceipt: Bool
  public var summary: String
  public var error: String

  public init(
    unitID: String,
    jobID: String,
    role: String,
    phase: String,
    lifecycle: AgentOfficeLifecycle,
    hasTalkReceipt: Bool = false,
    summary: String = "",
    error: String = ""
  ) {
    self.unitID = unitID
    self.jobID = jobID
    self.role = role
    self.phase = phase
    self.lifecycle = lifecycle
    self.hasTalkReceipt = hasTalkReceipt
    self.summary = summary
    self.error = error
  }
}

public struct AgentOfficeRunSnapshot: Equatable, Sendable {
  public var runID: String
  public var status: String
  public var sourcePath: String
  public var updatedAt: Date?
  public var monthlyTokenUsage: Int
  public var monthlyTokenUsageIsApproximate: Bool
  public var tokensPerSecond: Double?
  public var tokensPerSecondIsApproximate: Bool
  public var activeTurnCount: Int
  public var plannedCount: Int
  public var completedCount: Int
  public var failedCount: Int
  public var agents: [AgentOfficeAgentSnapshot]

  public init(
    runID: String = "",
    status: String = "idle",
    sourcePath: String = "",
    updatedAt: Date? = nil,
    monthlyTokenUsage: Int = 0,
    monthlyTokenUsageIsApproximate: Bool = false,
    tokensPerSecond: Double? = nil,
    tokensPerSecondIsApproximate: Bool = false,
    activeTurnCount: Int = 0,
    plannedCount: Int = 0,
    completedCount: Int = 0,
    failedCount: Int = 0,
    agents: [AgentOfficeAgentSnapshot] = []
  ) {
    self.runID = runID
    self.status = status
    self.sourcePath = sourcePath
    self.updatedAt = updatedAt
    self.monthlyTokenUsage = monthlyTokenUsage
    self.monthlyTokenUsageIsApproximate = monthlyTokenUsageIsApproximate
    self.tokensPerSecond = tokensPerSecond
    self.tokensPerSecondIsApproximate = tokensPerSecondIsApproximate
    self.activeTurnCount = activeTurnCount
    self.plannedCount = plannedCount
    self.completedCount = completedCount
    self.failedCount = failedCount
    self.agents = agents
  }

  public static let idle = AgentOfficeRunSnapshot()
}

public struct AgentOfficeDesk: Equatable, Sendable {
  public var role: String
  public var label: String

  public init(role: String, label: String) {
    self.role = role
    self.label = label
  }

  public static let defaultDesks: [AgentOfficeDesk] = [
    AgentOfficeDesk(role: "scribe", label: "Scribes"),
    AgentOfficeDesk(role: "hourly_aggregate", label: "Hour Desk"),
    AgentOfficeDesk(role: "daily_editor", label: "Editor"),
    AgentOfficeDesk(role: "biographer", label: "Bio"),
    AgentOfficeDesk(role: "context_curator", label: "Context"),
    AgentOfficeDesk(role: "curator", label: "Curator"),
    AgentOfficeDesk(role: "librarian", label: "Library"),
    AgentOfficeDesk(role: "historian", label: "History"),
    AgentOfficeDesk(role: "contradiction", label: "Check"),
    AgentOfficeDesk(role: "publisher", label: "Publish")
  ]
}
