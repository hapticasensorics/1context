import Foundation
import OneContextPlatform

public enum CodexRuntimeCapabilityStatus: String, Codable, Sendable {
  case ready
  case limited
  case checking
  case unavailable
}

public enum CodexRuntimeMode: String, Codable, Sendable {
  case installedCodex = "installed_codex"
  case installedPlaintextMultiAgentV2 = "installed_plaintext_multi_agent_v2"
  case bundledCompatibility = "bundled_compatibility"
  case harnessOnly = "harness_only"
  case explicitOverride = "explicit_override"
  case checking
  case unavailable
}

public struct CodexRuntimeCapabilitySnapshot: Codable, Equatable, Sendable {
  public let schemaVersion: String
  public let generatedAt: String
  public let status: CodexRuntimeCapabilityStatus
  public let mode: CodexRuntimeMode
  public let selectedBinaryPath: String?
  public let userTitle: String
  public let userDetail: String
  public let nativeMultiAgentV2: Bool
  public let plaintextMultiAgentV2: Bool
  public let harnessOnlyAgents: Bool
  public let probeSummary: String

  public var requiredReady: Bool {
    status == .ready || status == .limited
  }

  public var displayStatus: String {
    switch status {
    case .ready, .limited:
      return "Granted"
    case .checking:
      return "Checking"
    case .unavailable:
      return "Required"
    }
  }

  public var blockingSummary: String {
    switch status {
    case .ready, .limited:
      return ""
    case .checking:
      return "Codex Runtime is being checked."
    case .unavailable:
      return "Codex Runtime is required for wiki agents."
    }
  }

  public init(
    schemaVersion: String = "1context.codex-runtime-capability.v1",
    generatedAt: String = ISO8601DateFormatter().string(from: Date()),
    status: CodexRuntimeCapabilityStatus,
    mode: CodexRuntimeMode,
    selectedBinaryPath: String?,
    userTitle: String,
    userDetail: String,
    nativeMultiAgentV2: Bool,
    plaintextMultiAgentV2: Bool,
    harnessOnlyAgents: Bool,
    probeSummary: String
  ) {
    self.schemaVersion = schemaVersion
    self.generatedAt = generatedAt
    self.status = status
    self.mode = mode
    self.selectedBinaryPath = selectedBinaryPath
    self.userTitle = userTitle
    self.userDetail = userDetail
    self.nativeMultiAgentV2 = nativeMultiAgentV2
    self.plaintextMultiAgentV2 = plaintextMultiAgentV2
    self.harnessOnlyAgents = harnessOnlyAgents
    self.probeSummary = probeSummary
  }

  public static func assumedInstalledReady() -> Self {
    Self(
      status: .ready,
      mode: .installedCodex,
      selectedBinaryPath: nil,
      userTitle: "Codex Runtime",
      userDetail: "Using installed Codex.",
      nativeMultiAgentV2: false,
      plaintextMultiAgentV2: true,
      harnessOnlyAgents: true,
      probeSummary: "assumed_ready"
    )
  }

  public static func checking() -> Self {
    Self(
      status: .checking,
      mode: .checking,
      selectedBinaryPath: nil,
      userTitle: "Codex Runtime",
      userDetail: "Checking Codex agent tools.",
      nativeMultiAgentV2: false,
      plaintextMultiAgentV2: false,
      harnessOnlyAgents: false,
      probeSummary: "not_checked"
    )
  }

  enum CodingKeys: String, CodingKey {
    case schemaVersion = "schema_version"
    case generatedAt = "generated_at"
    case status
    case mode
    case selectedBinaryPath = "selected_binary_path"
    case userTitle = "user_title"
    case userDetail = "user_detail"
    case nativeMultiAgentV2 = "native_multi_agent_v2"
    case plaintextMultiAgentV2 = "plaintext_multi_agent_v2"
    case harnessOnlyAgents = "harness_only_agents"
    case probeSummary = "probe_summary"
  }
}

public enum CodexRuntimeCapability {
  public typealias Runner = @Sendable (
    _ executable: URL,
    _ arguments: [String],
    _ environment: [String: String]?,
    _ timeoutSeconds: TimeInterval
  ) throws -> ProcessRunResult

  public static func capabilityURL(runtimePaths: RuntimePaths) -> URL {
    runtimePaths.contextEngineDirectory
      .appendingPathComponent("capabilities", isDirectory: true)
      .appendingPathComponent("codex.json")
  }

  public static func load(
    runtimePaths: RuntimePaths,
    fileManager: FileManager = .default
  ) -> CodexRuntimeCapabilitySnapshot? {
    let url = capabilityURL(runtimePaths: runtimePaths)
    guard fileManager.fileExists(atPath: url.path),
      let data = try? Data(contentsOf: url)
    else {
      return nil
    }
    return try? JSONDecoder().decode(CodexRuntimeCapabilitySnapshot.self, from: data)
  }

  @discardableResult
  public static func persist(
    _ snapshot: CodexRuntimeCapabilitySnapshot,
    runtimePaths: RuntimePaths,
    fileManager: FileManager = .default
  ) throws -> URL {
    let url = capabilityURL(runtimePaths: runtimePaths)
    try RuntimePermissions.ensurePrivateDirectory(url.deletingLastPathComponent())
    let encoder = JSONEncoder()
    encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
    try RuntimePermissions.writePrivateData(try encoder.encode(snapshot), to: url)
    return url
  }

  public static func loadOrProbeAndPersist(
    runtimePaths: RuntimePaths = .current(),
    environment: [String: String] = ProcessInfo.processInfo.environment,
    fileManager: FileManager = .default,
    timeoutSeconds: TimeInterval = 2,
    runner: @escaping Runner = defaultRunner
  ) -> CodexRuntimeCapabilitySnapshot {
    if let snapshot = load(runtimePaths: runtimePaths, fileManager: fileManager) {
      return snapshot
    }
    let snapshot = probe(
      runtimePaths: runtimePaths,
      environment: environment,
      fileManager: fileManager,
      timeoutSeconds: timeoutSeconds,
      runner: runner
    )
    _ = try? persist(snapshot, runtimePaths: runtimePaths, fileManager: fileManager)
    return snapshot
  }

  public static func probe(
    runtimePaths: RuntimePaths = .current(),
    environment: [String: String] = ProcessInfo.processInfo.environment,
    fileManager: FileManager = .default,
    timeoutSeconds: TimeInterval = 2,
    runner: @escaping Runner = defaultRunner
  ) -> CodexRuntimeCapabilitySnapshot {
    let explicit = explicitOverrideCandidate(environment: environment, fileManager: fileManager)
    let installed = installedCodexCandidates(environment: environment, fileManager: fileManager)
    let bundled = bundledCodexCandidates(environment: environment, fileManager: fileManager)

    if let explicit, basicProbe(explicit, environment: environment, timeoutSeconds: timeoutSeconds, runner: runner) {
      let plaintext = plaintextMultiAgentV2Probe(explicit, environment: environment, timeoutSeconds: timeoutSeconds, runner: runner)
      return snapshot(
        status: plaintext ? .ready : .limited,
        mode: .explicitOverride,
        binary: explicit,
        detail: plaintext ? "Using configured Codex runtime." : "Using configured Codex runtime with native multi-agent tools limited.",
        plaintextMultiAgentV2: plaintext,
        probeSummary: plaintext ? "explicit_plaintext_multi_agent_v2" : "explicit_harness_only"
      )
    }

    let installedProbe = installed.compactMap { candidate -> (URL, Bool)? in
      guard basicProbe(candidate, environment: environment, timeoutSeconds: timeoutSeconds, runner: runner) else {
        return nil
      }
      return (
        candidate,
        plaintextMultiAgentV2Probe(candidate, environment: environment, timeoutSeconds: timeoutSeconds, runner: runner)
      )
    }

    if let nativeReady = installedProbe.first(where: { _ in nativeMultiAgentV2Ready(environment: environment) }) {
      return snapshot(
        status: .ready,
        mode: .installedCodex,
        binary: nativeReady.0,
        detail: "Using installed Codex.",
        nativeMultiAgentV2: true,
        plaintextMultiAgentV2: nativeReady.1,
        probeSummary: "installed_native_multi_agent_v2"
      )
    }
    if let plaintextReady = installedProbe.first(where: { $0.1 }) {
      return snapshot(
        status: .ready,
        mode: .installedPlaintextMultiAgentV2,
        binary: plaintextReady.0,
        detail: "Using installed Codex with compatible agent tools.",
        plaintextMultiAgentV2: true,
        probeSummary: "installed_plaintext_multi_agent_v2"
      )
    }

    let bundledProbe = bundled.compactMap { candidate -> (URL, Bool)? in
      guard basicProbe(candidate, environment: environment, timeoutSeconds: timeoutSeconds, runner: runner) else {
        return nil
      }
      return (
        candidate,
        plaintextMultiAgentV2Probe(candidate, environment: environment, timeoutSeconds: timeoutSeconds, runner: runner)
      )
    }
    if let bundledReady = bundledProbe.first(where: { $0.1 }) {
      return snapshot(
        status: .ready,
        mode: .bundledCompatibility,
        binary: bundledReady.0,
        detail: "Using bundled 1Context Codex runtime.",
        plaintextMultiAgentV2: true,
        probeSummary: "bundled_plaintext_multi_agent_v2"
      )
    }

    if let installedHarness = installedProbe.first {
      return snapshot(
        status: .limited,
        mode: .harnessOnly,
        binary: installedHarness.0,
        detail: "Using installed Codex with native multi-agent tools limited.",
        plaintextMultiAgentV2: false,
        probeSummary: "installed_harness_only"
      )
    }
    if let bundledHarness = bundledProbe.first {
      return snapshot(
        status: .limited,
        mode: .harnessOnly,
        binary: bundledHarness.0,
        detail: "Using bundled 1Context Codex runtime with native multi-agent tools limited.",
        plaintextMultiAgentV2: false,
        probeSummary: "bundled_harness_only"
      )
    }

    return CodexRuntimeCapabilitySnapshot(
      status: .unavailable,
      mode: .unavailable,
      selectedBinaryPath: nil,
      userTitle: "Codex Runtime",
      userDetail: "Install or sign in to Codex so 1Context can wake wiki agents.",
      nativeMultiAgentV2: false,
      plaintextMultiAgentV2: false,
      harnessOnlyAgents: false,
      probeSummary: "no_codex_runtime"
    )
  }

  public static func applySelectedRuntime(
    from snapshot: CodexRuntimeCapabilitySnapshot?,
    to environment: [String: String]
  ) -> [String: String] {
    var env = environment
    guard let snapshot else {
      return env
    }
    if let selectedBinaryPath = snapshot.selectedBinaryPath,
      !selectedBinaryPath.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    {
      env["ONECONTEXT_CODEX_BIN"] = selectedBinaryPath
    }
    env["ONECONTEXT_CODEX_RUNTIME_MODE"] = snapshot.mode.rawValue
    env["ONECONTEXT_CODEX_PLAINTEXT_MAV2"] = snapshot.plaintextMultiAgentV2 ? "1" : "0"
    return env
  }

  private static func snapshot(
    status: CodexRuntimeCapabilityStatus,
    mode: CodexRuntimeMode,
    binary: URL,
    detail: String,
    nativeMultiAgentV2: Bool = false,
    plaintextMultiAgentV2: Bool,
    probeSummary: String
  ) -> CodexRuntimeCapabilitySnapshot {
    CodexRuntimeCapabilitySnapshot(
      status: status,
      mode: mode,
      selectedBinaryPath: binary.path,
      userTitle: "Codex Runtime",
      userDetail: detail,
      nativeMultiAgentV2: nativeMultiAgentV2,
      plaintextMultiAgentV2: plaintextMultiAgentV2,
      harnessOnlyAgents: true,
      probeSummary: probeSummary
    )
  }

  public static func defaultRunner(
    executable: URL,
    arguments: [String],
    environment: [String: String]?,
    timeoutSeconds: TimeInterval
  ) throws -> ProcessRunResult {
    try ProcessRunner.run(
      executable: executable,
      arguments: arguments,
      environment: environment,
      timeoutSeconds: timeoutSeconds
    )
  }

  private static func basicProbe(
    _ candidate: URL,
    environment: [String: String],
    timeoutSeconds: TimeInterval,
    runner: Runner
  ) -> Bool {
    guard let result = try? runner(candidate, ["--version"], environment, timeoutSeconds) else {
      return false
    }
    return result.status == 0 && !result.timedOut
  }

  private static func plaintextMultiAgentV2Probe(
    _ candidate: URL,
    environment: [String: String],
    timeoutSeconds: TimeInterval,
    runner: Runner
  ) -> Bool {
    let arguments = [
      "app-server",
      "-c",
      "features.multi_agent_v2.enabled=true",
      "-c",
      "features.multi_agent_v2.encrypted_messages=false",
      "--help"
    ]
    guard let result = try? runner(candidate, arguments, environment, timeoutSeconds) else {
      return false
    }
    return result.status == 0 && !result.timedOut
  }

  private static func nativeMultiAgentV2Ready(environment: [String: String]) -> Bool {
    environment["ONECONTEXT_ASSUME_CODEX_NATIVE_MAV2"] == "1"
  }

  private static func explicitOverrideCandidate(
    environment: [String: String],
    fileManager: FileManager
  ) -> URL? {
    executableURL(environment["ONECONTEXT_CODEX_BIN"], fileManager: fileManager)
  }

  private static func installedCodexCandidates(
    environment: [String: String],
    fileManager: FileManager
  ) -> [URL] {
    var candidates: [URL] = []
    if let explicitInstalled = executableURL(environment["ONECONTEXT_INSTALLED_CODEX_BIN"], fileManager: fileManager) {
      candidates.append(explicitInstalled)
    }
    let home = environment["HOME"].flatMap { $0.isEmpty ? nil : $0 }
      ?? fileManager.homeDirectoryForCurrentUser.path
    let pathSegments = (environment["PATH"] ?? "")
      .split(separator: ":")
      .map(String.init)
      .filter { !$0.isEmpty }
    for segment in pathSegments {
      candidates.append(URL(fileURLWithPath: segment).appendingPathComponent("codex"))
    }
    candidates += [
      URL(fileURLWithPath: home).appendingPathComponent(".local/bin/codex"),
      URL(fileURLWithPath: home).appendingPathComponent(".cargo/bin/codex"),
      URL(fileURLWithPath: "/opt/homebrew/bin/codex"),
      URL(fileURLWithPath: "/usr/local/bin/codex")
    ]
    return dedupe(candidates).filter { fileManager.isExecutableFile(atPath: $0.path) }
  }

  private static func bundledCodexCandidates(
    environment: [String: String],
    fileManager: FileManager
  ) -> [URL] {
    var candidates: [URL] = []
    if let explicitBundled = executableURL(environment["ONECONTEXT_BUNDLED_CODEX_BIN"], fileManager: fileManager) {
      candidates.append(explicitBundled)
    }
    if let executableDirectory = Bundle.main.executableURL?.deletingLastPathComponent() {
      candidates.append(executableDirectory.appendingPathComponent("onecontext-codex"))
      candidates.append(executableDirectory.appendingPathComponent("codex"))
    }
    return dedupe(candidates).filter { fileManager.isExecutableFile(atPath: $0.path) }
  }

  private static func executableURL(_ path: String?, fileManager: FileManager) -> URL? {
    guard let path = path?.trimmingCharacters(in: .whitespacesAndNewlines),
      !path.isEmpty,
      fileManager.isExecutableFile(atPath: path)
    else {
      return nil
    }
    return URL(fileURLWithPath: path)
  }

  private static func dedupe(_ urls: [URL]) -> [URL] {
    var seen = Set<String>()
    var result: [URL] = []
    for url in urls {
      let path = url.resolvingSymlinksInPath().path
      guard seen.insert(path).inserted else { continue }
      result.append(URL(fileURLWithPath: path))
    }
    return result
  }
}
