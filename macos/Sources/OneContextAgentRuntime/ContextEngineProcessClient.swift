import Foundation
import OneContextPlatform

public enum ContextEngineWikiUpdateMode: String, Sendable {
  case recentFirst = "recent-first"
  case incremental
  case backfill
  case dryRun = "dry-run"
}

public struct ContextEngineProcessError: Error, LocalizedError, @unchecked Sendable {
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
}

public final class ContextEngineProcessClient: @unchecked Sendable {
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

  public func describe(timeoutSeconds: Int = 30) throws -> [String: Any] {
    try call(["describe"], timeoutSeconds: timeoutSeconds)
  }

  public func updateWiki(
    runID: String? = nil,
    trigger: String,
    executeAgents: Bool = true,
    maxConcurrent: Int? = nil,
    sourceWindowDays: Int = 3,
    mode: ContextEngineWikiUpdateMode = .recentFirst,
    timeoutSeconds: Int = 1_800
  ) throws -> [String: Any] {
    var arguments = [
      "update-wiki",
      "--root",
      runtimePaths.userContentDirectory.path,
      "--trigger",
      trigger
    ]
    if let runID, !runID.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
      arguments += ["--run-id", runID]
    }
    arguments.append(executeAgents ? "--execute-agents" : "--no-agents")
    if let maxConcurrent {
      arguments += ["--max-concurrent", "\(max(1, maxConcurrent))"]
    }
    arguments += [
      "--source-window-days",
      "\(max(1, sourceWindowDays))",
      "--mode",
      mode.rawValue,
      "--json"
    ]
    return try call(arguments, timeoutSeconds: timeoutSeconds)
  }

  public func call(_ arguments: [String], timeoutSeconds: Int = 60) throws -> [String: Any] {
    let result = try ProcessRunner.run(
      executable: try discoverExecutable(),
      arguments: arguments,
      environment: processEnvironment(),
      timeoutSeconds: TimeInterval(max(5, timeoutSeconds))
    )
    let stdoutText = result.stdoutText.trimmingCharacters(in: .whitespacesAndNewlines)
    let stderrText = result.stderrText.trimmingCharacters(in: .whitespacesAndNewlines)
    let stdoutObject = Self.jsonObject(from: result.stdout)
    let stderrObject = Self.jsonObject(from: result.stderr)

    if result.timedOut {
      throw ContextEngineProcessError(
        message: "context-engine command timed out",
        terminationStatus: result.status,
        structuredPayload: stdoutObject ?? stderrObject,
        stdout: stdoutText,
        stderr: stderrText
      )
    }

    guard result.status == 0 else {
      if let stdoutObject {
        return stdoutObject
      }
      let structuredPayload = stderrObject
      let message = Self.structuredErrorMessage(from: structuredPayload)
        ?? (!stderrText.isEmpty
        ? stderrText
        : (!stdoutText.isEmpty ? stdoutText : "context-engine exited with status \(result.status)"))
      throw ContextEngineProcessError(
        message: message,
        terminationStatus: result.status,
        structuredPayload: structuredPayload,
        stdout: stdoutText,
        stderr: stderrText
      )
    }

    guard let stdoutObject else {
      throw ContextEngineProcessError(
        message: "context-engine returned non-object JSON",
        terminationStatus: result.status,
        stdout: stdoutText,
        stderr: stderrText
      )
    }
    return stdoutObject
  }

  public func discoverExecutable() throws -> URL {
    let candidates = executableCandidates()
    if let executable = candidates.first(where: { fileManager.isExecutableFile(atPath: $0.path) }) {
      return executable
    }
    throw ContextEngineProcessError(
      message: "onecontext-context-engine executable missing. Checked: \(candidates.map(\.path).joined(separator: ", "))"
    )
  }

  private func executableCandidates() -> [URL] {
    var candidates: [URL] = []
    if let override = environment["ONECONTEXT_CONTEXT_ENGINE_BIN"], !override.isEmpty {
      candidates.append(URL(fileURLWithPath: override))
    }

    if let executableDirectory = Bundle.main.executableURL?.deletingLastPathComponent() {
      candidates.append(executableDirectory.appendingPathComponent("onecontext-context-engine"))
    }

    let cwd = URL(fileURLWithPath: fileManager.currentDirectoryPath, isDirectory: true)
    candidates.append(cwd.appendingPathComponent("target/debug/onecontext-context-engine"))
    candidates.append(cwd.appendingPathComponent("target/release/onecontext-context-engine"))
    candidates.append(cwd.appendingPathComponent("../target/debug/onecontext-context-engine"))
    candidates.append(cwd.appendingPathComponent("../target/release/onecontext-context-engine"))
    return candidates
  }

  private func processEnvironment() -> [String: String] {
    var env = Self.environmentWithStorageDefaults(
      environment,
      identity: runtimePaths.identity
    )
    env = CodexRuntimeCapability.applySelectedRuntime(
      from: CodexRuntimeCapability.load(runtimePaths: runtimePaths, fileManager: fileManager),
      to: env
    )
    env[OneContextAppIdentity.environmentKey] = runtimePaths.identity.environmentValue
    env["PATH"] = developerToolPath(existing: env["PATH"])
    return env
  }

  private static func environmentWithStorageDefaults(
    _ environment: [String: String],
    identity: OneContextAppIdentity
  ) -> [String: String] {
    var env = environment
    let explicitDatabaseURL = normalizedExplicitDatabaseURL(from: env)
    if let explicitDatabaseURL {
      env["ONECONTEXT_MEMORY_DB_URL"] = explicitDatabaseURL
    }
    if env["ONECONTEXT_STORAGE_BACKEND"]?.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty != false {
      env["ONECONTEXT_STORAGE_BACKEND"] = explicitDatabaseURL == nil ? "managed_postgres" : "external_postgres"
    }
    if identity.kind == .dev, explicitDatabaseURL == nil {
      env.removeValue(forKey: "ONECONTEXT_MEMORY_DB_URL")
    }
    return env
  }

  private static func normalizedExplicitDatabaseURL(from environment: [String: String]) -> String? {
    for key in ["ONECONTEXT_MEMORY_DB_URL", "ONECONTEXT_MEMORY_DATABASE_URL", "DATABASE_URL"] {
      if let value = environment[key]?.trimmingCharacters(in: .whitespacesAndNewlines),
        !value.isEmpty
      {
        return value
      }
    }
    return nil
  }

  private func developerToolPath(existing: String?) -> String {
    var parts: [String] = []
    let home = fileManager.homeDirectoryForCurrentUser.path
    for item in [
      existing ?? "",
      "\(home)/.local/bin",
      "\(home)/.cargo/bin",
      "/opt/homebrew/bin",
      "/opt/homebrew/sbin",
      "/usr/local/bin",
      "/usr/bin",
      "/bin",
      "/usr/sbin",
      "/sbin"
    ] where !item.isEmpty {
      for segment in item.split(separator: ":").map(String.init) where !segment.isEmpty && !parts.contains(segment) {
        parts.append(segment)
      }
    }
    return parts.joined(separator: ":")
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
}
