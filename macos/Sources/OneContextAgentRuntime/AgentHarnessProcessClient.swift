import Foundation
import OneContextPlatform

public struct AgentHarnessProcessError: Error, LocalizedError, @unchecked Sendable {
  public let message: String
  public let terminationStatus: Int32?
  public let structuredPayload: [String: Any]?
  public let stdout: String
  public let stderr: String

  public init(
    message: String,
    terminationStatus: Int32? = nil,
    structuredPayload: [String: Any]? = nil,
    stdout: String = "",
    stderr: String = ""
  ) {
    self.message = message
    self.terminationStatus = terminationStatus
    self.structuredPayload = structuredPayload
    self.stdout = stdout
    self.stderr = stderr
  }

  public var errorDescription: String? {
    message
  }

  public var errorCode: String? {
    if let error = structuredPayload?["error"] as? [String: Any] {
      return error["code"] as? String
    }
    return structuredPayload?["code"] as? String
  }

  public var repairHints: [String] {
    structuredPayload?["repair_hints"] as? [String] ?? []
  }
}

public final class AgentHarnessProcessClient: @unchecked Sendable {
  private let runtimePaths: RuntimePaths
  private let fileManager: FileManager
  private let environment: [String: String]

  public init(
    runtimePaths: RuntimePaths,
    fileManager: FileManager = .default,
    environment: [String: String] = ProcessInfo.processInfo.environment
  ) {
    self.runtimePaths = runtimePaths
    self.fileManager = fileManager
    self.environment = environment
  }

  public func status() throws -> [String: Any] {
    try call(["status"])
  }

  public func describe() throws -> [String: Any] {
    try call(["describe"])
  }

  public func ensure() throws -> [String: Any] {
    try call(["ensure"])
  }

  public func agents(request: [String: Any] = [:]) throws -> [String: Any] {
    try call(command: "agents", request: request)
  }

  public func agentStatus(unitID: String) throws -> [String: Any] {
    try agentStatus(request: ["unit_id": unitID])
  }

  public func agentStatus(request: [String: Any]) throws -> [String: Any] {
    try call(command: "agent-status", request: request)
  }

  public func call(request: [String: Any]) throws -> [String: Any] {
    try call(command: "call", request: request)
  }

  public func startTurn(unitID: String, request: [String: Any] = [:]) throws -> [String: Any] {
    try startTurn(request: Self.requestWithUnitID(unitID, request: request))
  }

  public func startTurn(request: [String: Any]) throws -> [String: Any] {
    try call(command: "start-turn", request: request)
  }

  public func completeTurn(unitID: String, request: [String: Any] = [:]) throws -> [String: Any] {
    try completeTurn(request: Self.requestWithUnitID(unitID, request: request))
  }

  public func completeTurn(request: [String: Any]) throws -> [String: Any] {
    try call(command: "complete-turn", request: request)
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
    try call(command: "observe-proof", request: request)
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
    try call(command: "record-adapter-event", request: request)
  }

  public func transportPlan(request: [String: Any] = [:]) throws -> [String: Any] {
    try call(command: "transport-plan", request: request)
  }

  public func retire(unitID: String, reason: String? = nil) throws -> [String: Any] {
    var request: [String: Any] = ["unit_id": unitID]
    if let reason {
      request["reason"] = reason
    }
    return try retire(request: request)
  }

  public func retire(request: [String: Any]) throws -> [String: Any] {
    try call(command: "retire", request: request)
  }

  public func call(_ arguments: [String]) throws -> [String: Any] {
    let executable = try discoverExecutable()
    let process = Process()
    let stdout = Pipe()
    let stderr = Pipe()
    process.executableURL = executable
    process.arguments = ["--root", runtimePaths.userContentDirectory.path] + arguments
    process.standardOutput = stdout
    process.standardError = stderr

    let outputBuffer = ProcessPipeBuffer()
    let errorBuffer = ProcessPipeBuffer()
    try process.run()
    let pipeDrainGroup = DispatchGroup()
    drain(stdout, into: outputBuffer, group: pipeDrainGroup)
    drain(stderr, into: errorBuffer, group: pipeDrainGroup)
    process.waitUntilExit()
    pipeDrainGroup.wait()

    let output = outputBuffer.data()
    let errorOutput = errorBuffer.data()
    let outputText = String(decoding: output, as: UTF8.self)
      .trimmingCharacters(in: .whitespacesAndNewlines)
    let errorText = String(decoding: errorOutput, as: UTF8.self)
      .trimmingCharacters(in: .whitespacesAndNewlines)
    let outputObject = Self.jsonObject(from: output)
    let errorObject = Self.jsonObject(from: errorOutput)

    guard process.terminationStatus == 0 else {
      let structuredPayload = outputObject ?? errorObject
      let message = Self.structuredErrorMessage(from: structuredPayload)
        ?? (!errorText.isEmpty
        ? errorText
        : (!outputText.isEmpty ? outputText : "agent harness exited with status \(process.terminationStatus)"))
      throw AgentHarnessProcessError(
        message: message,
        terminationStatus: process.terminationStatus,
        structuredPayload: structuredPayload,
        stdout: outputText,
        stderr: errorText
      )
    }

    guard let object = outputObject else {
      throw AgentHarnessProcessError(
        message: "agent harness returned non-object JSON",
        terminationStatus: process.terminationStatus,
        stdout: outputText,
        stderr: errorText
      )
    }
    return object
  }

  public func discoverExecutable() throws -> URL {
    let candidates = executableCandidates()
    if let executable = candidates.first(where: { fileManager.isExecutableFile(atPath: $0.path) }) {
      return executable
    }
    throw AgentHarnessProcessError(
      message: "onecontext-agent-harness executable missing. Checked: \(candidates.map(\.path).joined(separator: ", "))"
    )
  }

  private func executableCandidates() -> [URL] {
    var candidates: [URL] = []
    if let override = environment["ONECONTEXT_AGENT_HARNESS_BIN"], !override.isEmpty {
      candidates.append(URL(fileURLWithPath: override))
    }

    if let executableDirectory = Bundle.main.executableURL?.deletingLastPathComponent() {
      candidates.append(executableDirectory.appendingPathComponent("onecontext-agent-harness"))
    }

    let cwd = URL(fileURLWithPath: fileManager.currentDirectoryPath, isDirectory: true)
    candidates.append(cwd.appendingPathComponent("target/debug/onecontext-agent-harness"))
    candidates.append(cwd.appendingPathComponent("target/release/onecontext-agent-harness"))
    candidates.append(cwd.appendingPathComponent("../target/debug/onecontext-agent-harness"))
    candidates.append(cwd.appendingPathComponent("../target/release/onecontext-agent-harness"))
    return candidates
  }

  private func call(command: String, request: [String: Any]) throws -> [String: Any] {
    try call([command] + Self.requestJSONArguments(command: command, request: request))
  }

  private static func requestJSONArguments(command: String, request: [String: Any]) throws -> [String] {
    guard JSONSerialization.isValidJSONObject(request) else {
      throw AgentHarnessProcessError(message: "Agent harness request for \(command) is not valid JSON")
    }
    let data: Data
    do {
      data = try JSONSerialization.data(withJSONObject: request, options: [.sortedKeys])
    } catch {
      throw AgentHarnessProcessError(message: "Agent harness request for \(command) is not valid JSON")
    }
    guard let json = String(data: data, encoding: .utf8) else {
      throw AgentHarnessProcessError(message: "Agent harness request for \(command) could not be encoded as UTF-8")
    }
    return ["--request-json", json]
  }

  private static func requestWithUnitID(_ unitID: String, request: [String: Any]) -> [String: Any] {
    var request = request
    request["unit_id"] = unitID
    return request
  }

  private static func jsonObject(from data: Data) -> [String: Any]? {
    guard !data.isEmpty else {
      return nil
    }
    return try? JSONSerialization.jsonObject(with: data) as? [String: Any]
  }

  private static func structuredErrorMessage(from payload: [String: Any]?) -> String? {
    guard let payload else {
      return nil
    }
    if let error = payload["error"] as? [String: Any],
       let message = error["message"] as? String,
       !message.isEmpty {
      return message
    }
    if let message = payload["message"] as? String, !message.isEmpty {
      return message
    }
    return nil
  }

  private func drain(_ pipe: Pipe, into buffer: ProcessPipeBuffer, group: DispatchGroup) {
    group.enter()
    DispatchQueue.global(qos: .utility).async {
      buffer.append(pipe.fileHandleForReading.readDataToEndOfFile())
      group.leave()
    }
  }
}

private final class ProcessPipeBuffer: @unchecked Sendable {
  private let lock = NSLock()
  private var chunks = Data()

  func append(_ data: Data) {
    guard !data.isEmpty else {
      return
    }
    lock.lock()
    chunks.append(data)
    lock.unlock()
  }

  func data() -> Data {
    lock.lock()
    let snapshot = chunks
    lock.unlock()
    return snapshot
  }
}
