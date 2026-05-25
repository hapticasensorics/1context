import Foundation

public enum AgentHarnessRPCBridgeError: Error, LocalizedError, Equatable {
  case unsupportedMethod(String)
  case invalidJSONParameters(String)

  public var errorDescription: String? {
    switch self {
    case .unsupportedMethod(let method):
      return "Unsupported agent harness RPC method: \(method)"
    case .invalidJSONParameters(let method):
      return "Agent harness RPC params for \(method) are not valid JSON"
    }
  }
}

public final class AgentHarnessRPCBridge: @unchecked Sendable {
  private let callHarness: ([String]) throws -> [String: Any]

  public init(call: @escaping ([String]) throws -> [String: Any]) {
    self.callHarness = call
  }

  public convenience init(client: AgentHarnessProcessClient) {
    self.init { arguments in
      try client.call(arguments)
    }
  }

  public static func supports(method: String) -> Bool {
    AgentHarnessRPCMethod(method) != nil
  }

  public func supports(method: String) -> Bool {
    Self.supports(method: method)
  }

  public func status() throws -> [String: Any] {
    try call(method: "agent.harness.status")
  }

  public func describe() throws -> [String: Any] {
    try call(method: "agent.harness.describe")
  }

  public func ensure() throws -> [String: Any] {
    try call(method: "agent.harness.ensure")
  }

  public func agents(request: [String: Any] = [:]) throws -> [String: Any] {
    try call(method: "agent.harness.agents", params: request)
  }

  public func agentStatus(unitID: String) throws -> [String: Any] {
    try agentStatus(request: ["unit_id": unitID])
  }

  public func agentStatus(request: [String: Any]) throws -> [String: Any] {
    try call(method: "agent.harness.agent-status", params: request)
  }

  public func call(request: [String: Any]) throws -> [String: Any] {
    try call(method: "agent.harness.call", params: request)
  }

  public func startTurn(unitID: String, request: [String: Any] = [:]) throws -> [String: Any] {
    try startTurn(request: Self.requestWithUnitID(unitID, request: request))
  }

  public func startTurn(request: [String: Any]) throws -> [String: Any] {
    try call(method: "agent.harness.start-turn", params: request)
  }

  public func completeTurn(unitID: String, request: [String: Any] = [:]) throws -> [String: Any] {
    try completeTurn(request: Self.requestWithUnitID(unitID, request: request))
  }

  public func completeTurn(request: [String: Any]) throws -> [String: Any] {
    try call(method: "agent.harness.complete-turn", params: request)
  }

  public func observeProof(
    unitID: String,
    proof: String? = nil,
    proofKey: String? = nil,
    kind: String? = nil,
    request: [String: Any] = [:]
  ) throws -> [String: Any] {
    var request = Self.requestWithUnitID(unitID, request: request)
    if let proof {
      request["proof"] = proof
    }
    if let proofKey {
      request["proof_key"] = proofKey
    }
    if let kind {
      request["kind"] = kind
    }
    return try observeProof(request: request)
  }

  public func observeProof(request: [String: Any]) throws -> [String: Any] {
    try call(method: "agent.harness.observe-proof", params: request)
  }

  public func recordAdapterEvent(
    unitID: String,
    kind: String,
    status: String,
    request: [String: Any] = [:]
  ) throws -> [String: Any] {
    var request = Self.requestWithUnitID(unitID, request: request)
    request["kind"] = kind
    request["status"] = status
    return try recordAdapterEvent(request: request)
  }

  public func recordAdapterEvent(request: [String: Any]) throws -> [String: Any] {
    try call(method: "agent.harness.record-adapter-event", params: request)
  }

  public func transportPlan(request: [String: Any] = [:]) throws -> [String: Any] {
    try call(method: "agent.harness.transport-plan", params: request)
  }

  public func retire(unitID: String, reason: String? = nil) throws -> [String: Any] {
    var request: [String: Any] = ["unit_id": unitID]
    if let reason {
      request["reason"] = reason
    }
    return try retire(request: request)
  }

  public func retire(request: [String: Any]) throws -> [String: Any] {
    try call(method: "agent.harness.retire", params: request)
  }

  public func call(method: String, params: [String: Any] = [:]) throws -> [String: Any] {
    guard let harnessMethod = AgentHarnessRPCMethod(method) else {
      throw AgentHarnessRPCBridgeError.unsupportedMethod(method)
    }

    switch harnessMethod {
    case .status:
      return try callHarness(["status"])
    case .describe:
      return try callHarness(["describe"])
    case .ensure:
      return try callHarness(["ensure"])
    case .call,
         .birth,
         .startTurn,
         .completeTurn,
         .observeProof,
         .recordAdapterEvent,
         .transportPlan,
         .agents,
         .agentStatus,
         .retire:
      return try callHarness(try requestArguments(command: harnessMethod.command, method: method, params: params))
    }
  }

  private func requestArguments(command: String, method: String, params: [String: Any]) throws -> [String] {
    guard JSONSerialization.isValidJSONObject(params) else {
      throw AgentHarnessRPCBridgeError.invalidJSONParameters(method)
    }
    let data: Data
    do {
      data = try JSONSerialization.data(withJSONObject: params, options: [.sortedKeys])
    } catch {
      throw AgentHarnessRPCBridgeError.invalidJSONParameters(method)
    }
    guard let json = String(data: data, encoding: .utf8) else {
      throw AgentHarnessRPCBridgeError.invalidJSONParameters(method)
    }
    return [command, "--request-json", json]
  }

  private static func requestWithUnitID(_ unitID: String, request: [String: Any]) -> [String: Any] {
    var request = request
    request["unit_id"] = unitID
    return request
  }
}

private enum AgentHarnessRPCMethod {
  case status
  case describe
  case ensure
  case call
  case birth
  case startTurn
  case completeTurn
  case observeProof
  case recordAdapterEvent
  case transportPlan
  case agents
  case agentStatus
  case retire

  init?(_ method: String) {
    let normalized = method
      .replacingOccurrences(of: "_", with: "-")
      .lowercased()

    switch normalized {
    case "agent.harness.status":
      self = .status
    case "agent.harness.describe":
      self = .describe
    case "agent.harness.ensure":
      self = .ensure
    case "agent.harness.call":
      self = .call
    case "agent.harness.birth":
      self = .birth
    case "agent.harness.start-turn":
      self = .startTurn
    case "agent.harness.complete-turn":
      self = .completeTurn
    case "agent.harness.observe-proof":
      self = .observeProof
    case "agent.harness.record-adapter-event":
      self = .recordAdapterEvent
    case "agent.harness.transport-plan":
      self = .transportPlan
    case "agent.harness.agents", "agent.harness.list":
      self = .agents
    case "agent.harness.agent-status":
      self = .agentStatus
    case "agent.harness.retire":
      self = .retire
    default:
      return nil
    }
  }

  var command: String {
    switch self {
    case .status:
      return "status"
    case .describe:
      return "describe"
    case .ensure:
      return "ensure"
    case .call:
      return "call"
    case .birth:
      return "birth"
    case .startTurn:
      return "start-turn"
    case .completeTurn:
      return "complete-turn"
    case .observeProof:
      return "observe-proof"
    case .recordAdapterEvent:
      return "record-adapter-event"
    case .transportPlan:
      return "transport-plan"
    case .agents:
      return "agents"
    case .agentStatus:
      return "agent-status"
    case .retire:
      return "retire"
    }
  }
}
