import Foundation
import Darwin
import OneContextPlatform

public enum AgentProvider: String, Codable, CaseIterable {
  case claude
  case codex
}

public enum AgentHookEvent: String, Codable, CaseIterable {
  case sessionStart = "SessionStart"
  case userPromptSubmit = "UserPromptSubmit"
  case postToolUse = "PostToolUse"
  case preCompact = "PreCompact"
  case sessionEnd = "SessionEnd"

  public var requiresMatcher: Bool {
    switch self {
    case .postToolUse:
      return true
    case .sessionStart, .userPromptSubmit, .preCompact, .sessionEnd:
      return false
    }
  }
}

public struct AgentPaths: Equatable {
  public let directory: URL
  public let configFile: URL
  public let stateFile: URL
  public let hookLogFile: URL

  public init(directory: URL) {
    self.directory = directory
    self.configFile = directory.appendingPathComponent("config.json")
    self.stateFile = directory.appendingPathComponent("integrations.json")
    self.hookLogFile = directory.appendingPathComponent("hook.log")
  }

  public static func current(environment: [String: String] = ProcessInfo.processInfo.environment) -> AgentPaths {
    let runtime = RuntimePaths.current(environment: environment)
    let directory = URL(
      fileURLWithPath: environment["ONECONTEXT_AGENT_DIR"]
        ?? runtime.appSupportDirectory.appendingPathComponent("agent", isDirectory: true).path,
      isDirectory: true
    )
    return AgentPaths(directory: directory)
  }
}

public struct AgentHookInput: Codable, Equatable {
  public let sessionID: String?
  public let transcriptPath: String?
  public let cwd: String?
  public let hookEventName: String?
  public let toolName: String?
  public let trigger: String?
  public let reason: String?
  public let prompt: String?

  enum CodingKeys: String, CodingKey {
    case sessionID = "session_id"
    case transcriptPath = "transcript_path"
    case cwd
    case hookEventName = "hook_event_name"
    case toolName = "tool_name"
    case trigger
    case reason
    case prompt
  }
}

public struct AgentHookOutput: Codable, Equatable {
  public var systemMessage: String?
  public var hookSpecificOutput: AgentHookSpecificOutput?

  public init(systemMessage: String? = nil, hookSpecificOutput: AgentHookSpecificOutput? = nil) {
    self.systemMessage = systemMessage
    self.hookSpecificOutput = hookSpecificOutput
  }
}

public struct AgentHookSpecificOutput: Codable, Equatable {
  public var hookEventName: String
  public var additionalContext: String?

  public init(hookEventName: String, additionalContext: String? = nil) {
    self.hookEventName = hookEventName
    self.additionalContext = additionalContext
  }
}

public struct AgentConfig: Codable, Equatable {
  public var wikiURL: String
  public var statusLineLabel: String

  enum CodingKeys: String, CodingKey {
    case statusLineLabel = "status_line_label"
    case wikiURL = "wiki_url"
  }

  public init(
    wikiURL: String = AgentHookExecutor.defaultWikiURL,
    statusLineLabel: String = "1Context wiki"
  ) {
    self.wikiURL = wikiURL
    self.statusLineLabel = statusLineLabel
  }

  public init(from decoder: Decoder) throws {
    let container = try decoder.container(keyedBy: CodingKeys.self)
    self.wikiURL = try container.decodeIfPresent(String.self, forKey: .wikiURL)
      ?? AgentHookExecutor.defaultWikiURL
    self.statusLineLabel = try container.decodeIfPresent(String.self, forKey: .statusLineLabel)
      ?? "1Context wiki"
  }
}

public struct AgentIntegrationState: Codable, Equatable {
  public var schemaVersion: Int
  public var updatedAt: Date
  public var claude: ProviderState
  public var codex: ProviderState

  public init(
    schemaVersion: Int = 1,
    updatedAt: Date,
    claude: ProviderState,
    codex: ProviderState
  ) {
    self.schemaVersion = schemaVersion
    self.updatedAt = updatedAt
    self.claude = claude
    self.codex = codex
  }
}

public struct ProviderState: Codable, Equatable {
  public var status: String
  public var detail: String
  public var managedEvents: [String]

  public init(status: String, detail: String, managedEvents: [String] = []) {
    self.status = status
    self.detail = detail
    self.managedEvents = managedEvents
  }
}

public struct AgentIntegrationsReport: Equatable {
  public var claudeStatus: ProviderIntegrationStatus
  public var codexStatus: ProviderIntegrationStatus
  public var paths: AgentPaths
  public var todos: [String]

  public init(
    claudeStatus: ProviderIntegrationStatus,
    codexStatus: ProviderIntegrationStatus,
    paths: AgentPaths,
    todos: [String]
  ) {
    self.claudeStatus = claudeStatus
    self.codexStatus = codexStatus
    self.paths = paths
    self.todos = todos
  }
}

public struct ProviderIntegrationStatus: Equatable {
  public enum State: String, Equatable {
    case installed
    case notInstalled = "not installed"
    case partiallyInstalled = "partially installed"
    case planOnly = "plan only"
    case manualReview = "manual review"
  }

  public var provider: AgentProvider
  public var state: State
  public var detail: String
  public var settingsPath: String?
  public var statusLineDetail: String?
  public var installedEvents: [AgentHookEvent]
  public var missingEvents: [AgentHookEvent]

  public init(
    provider: AgentProvider,
    state: State,
    detail: String,
    settingsPath: String?,
    statusLineDetail: String? = nil,
    installedEvents: [AgentHookEvent],
    missingEvents: [AgentHookEvent]
  ) {
    self.provider = provider
    self.state = state
    self.detail = detail
    self.settingsPath = settingsPath
    self.statusLineDetail = statusLineDetail
    self.installedEvents = installedEvents
    self.missingEvents = missingEvents
  }
}

public enum AgentIntegrationAction: String {
  case install
  case repair
  case uninstall
}

public final class AgentIntegrationManager {
  private let paths: AgentPaths
  private let claudeSettingsPath: URL
  private let executablePath: String
  private let fileManager: FileManager

  public init(
    paths: AgentPaths = .current(),
    claudeSettingsPath: URL? = nil,
    executablePath: String,
    fileManager: FileManager = .default
  ) {
    self.paths = paths
    self.claudeSettingsPath = claudeSettingsPath ?? Self.defaultClaudeSettingsPath()
    self.executablePath = executablePath
    self.fileManager = fileManager
  }

  public static func defaultClaudeSettingsPath(
    environment: [String: String] = ProcessInfo.processInfo.environment
  ) -> URL {
    if let override = environment["ONECONTEXT_CLAUDE_SETTINGS_PATH"] {
      return URL(fileURLWithPath: override)
    }
    return FileManager.default.homeDirectoryForCurrentUser
      .appendingPathComponent(".claude/settings.json")
  }

  public func status() -> AgentIntegrationsReport {
    AgentIntegrationsReport(
      claudeStatus: claudeStatus(),
      codexStatus: codexStatus(),
      paths: paths,
      todos: codexTodos()
    )
  }

  public func install() throws -> AgentIntegrationsReport {
    try mergeClaudeHooks(action: .install)
  }

  public func repair() throws -> AgentIntegrationsReport {
    try mergeClaudeHooks(action: .repair)
  }

  public func uninstall() throws -> AgentIntegrationsReport {
    try mergeClaudeHooks(action: .uninstall)
  }

  public func render(_ report: AgentIntegrationsReport, title: String = "1Context Agent Integrations") -> String {
    var lines: [String] = [
      title,
      "",
      "State: \(report.paths.stateFile.path)",
      "Config: \(report.paths.configFile.path)",
      "Hook Log: \(report.paths.hookLogFile.path)",
      "Wiki URL: \(readAgentConfig().wikiURL)",
      "",
      renderProvider(report.claudeStatus),
      "",
      renderProvider(report.codexStatus)
    ]

    if !report.todos.isEmpty {
      lines.append("")
      lines.append("Next:")
      lines.append(contentsOf: report.todos.map { "  - \($0)" })
    }
    return lines.joined(separator: "\n")
  }

  private func renderProvider(_ status: ProviderIntegrationStatus) -> String {
    var lines = [
      "\(displayName(status.provider)): \(status.state.rawValue)",
      "  \(status.detail)"
    ]
    if let settingsPath = status.settingsPath {
      lines.append("  Settings: \(settingsPath)")
    }
    if let statusLineDetail = status.statusLineDetail {
      lines.append("  Status Line: \(statusLineDetail)")
    }
    if !status.installedEvents.isEmpty {
      lines.append("  Installed Events: \(status.installedEvents.map(\.rawValue).joined(separator: ", "))")
    }
    if !status.missingEvents.isEmpty {
      lines.append("  Missing Events: \(status.missingEvents.map(\.rawValue).joined(separator: ", "))")
    }
    return lines.joined(separator: "\n")
  }

  private func displayName(_ provider: AgentProvider) -> String {
    switch provider {
    case .claude:
      return "Claude"
    case .codex:
      return "Codex"
    }
  }

  private func claudeStatus() -> ProviderIntegrationStatus {
    switch readClaudeSettings() {
    case .missing:
      return ProviderIntegrationStatus(
        provider: .claude,
        state: .notInstalled,
        detail: "Claude settings are missing; install can create a user settings file with 1Context hooks.",
        settingsPath: claudeSettingsPath.path,
        statusLineDetail: "install can create a 1Context status line",
        installedEvents: [],
        missingEvents: AgentHookEvent.allCases
      )
    case .unsafe(let detail):
      return ProviderIntegrationStatus(
        provider: .claude,
        state: .manualReview,
        detail: detail,
        settingsPath: claudeSettingsPath.path,
        statusLineDetail: "not inspected",
        installedEvents: [],
        missingEvents: AgentHookEvent.allCases
      )
    case .loaded(let root):
      let installed = AgentHookEvent.allCases.filter { hasManagedClaudeHook(root: root, event: $0) }
      let missing = AgentHookEvent.allCases.filter { !installed.contains($0) }
      let state: ProviderIntegrationStatus.State
      if installed.count == AgentHookEvent.allCases.count {
        state = .installed
      } else if installed.isEmpty {
        state = .notInstalled
      } else {
        state = .partiallyInstalled
      }
      let detail: String
      switch state {
      case .installed:
        detail = "Uses Claude Code command hooks in the user settings file."
      case .notInstalled:
        detail = "Claude settings exist, but no 1Context-managed hooks are installed."
      case .partiallyInstalled:
        detail = "Some 1Context-managed Claude Code hooks are missing; repair can restore them."
      case .planOnly, .manualReview:
        detail = "Claude Code hook settings need manual review."
      }
      return ProviderIntegrationStatus(
        provider: .claude,
        state: state,
        detail: detail,
        settingsPath: claudeSettingsPath.path,
        statusLineDetail: claudeStatusLineDetail(root: root),
        installedEvents: installed,
        missingEvents: missing
      )
    }
  }

  private func codexStatus() -> ProviderIntegrationStatus {
    ProviderIntegrationStatus(
        provider: .codex,
        state: .planOnly,
        detail: "No public Codex hook configuration is modified by this build.",
        settingsPath: nil,
        statusLineDetail: nil,
        installedEvents: [],
        missingEvents: AgentHookEvent.allCases
    )
  }

  private func codexTodos() -> [String] {
    [
      "Verify Codex hook configuration from local repo docs or first-party files before enabling install/repair.",
      "Keep Codex integration plan/status-only until the config schema and lifecycle semantics are confirmed."
    ]
  }

  private func mergeClaudeHooks(action: AgentIntegrationAction) throws -> AgentIntegrationsReport {
    switch readClaudeSettings() {
    case .unsafe:
      let report = status()
      try writeState(report: report, action: action)
      return report
    case .missing:
      guard action != .uninstall else {
        let report = status()
        try writeState(report: report, action: action)
        return report
      }
      var root: [String: Any] = [:]
      try applyClaudeAction(&root, action: action)
      try writeClaudeSettings(root)
    case .loaded(var root):
      try applyClaudeAction(&root, action: action)
      try writeClaudeSettings(root)
    }

    let report = status()
    try writeState(report: report, action: action)
    return report
  }

  private func applyClaudeAction(_ root: inout [String: Any], action: AgentIntegrationAction) throws {
    if root["hooks"] == nil {
      root["hooks"] = [String: Any]()
    }
    guard var hooks = root["hooks"] as? [String: Any] else {
      throw AgentIntegrationError.unsafeSettings("Claude settings has a non-object hooks value; leaving it unchanged.")
    }

    for event in AgentHookEvent.allCases {
      var groups = (hooks[event.rawValue] as? [[String: Any]]) ?? []
      groups = removeManagedHandlers(from: groups, event: event)

      if action == .install || action == .repair {
        groups.append(matcherGroup(event: event))
      }

      if groups.isEmpty {
        hooks.removeValue(forKey: event.rawValue)
      } else {
        hooks[event.rawValue] = groups
      }
    }

    if action == .install || action == .repair {
      try applyClaudeStatusLine(&root)
    } else {
      removeManagedClaudeStatusLine(from: &root)
    }

    root["hooks"] = hooks
  }

  private func applyClaudeStatusLine(_ root: inout [String: Any]) throws {
    if let statusLine = root["statusLine"] as? [String: Any] {
      if isManagedStatusLine(statusLine) {
        root["statusLine"] = managedStatusLine()
      }
      return
    }

    if root["statusLine"] == nil {
      root["statusLine"] = managedStatusLine()
      return
    }

    throw AgentIntegrationError.unsafeSettings("Claude settings has a non-object statusLine value; leaving it unchanged.")
  }

  private func removeManagedClaudeStatusLine(from root: inout [String: Any]) {
    guard let statusLine = root["statusLine"] as? [String: Any],
      isManagedStatusLine(statusLine)
    else {
      return
    }
    root.removeValue(forKey: "statusLine")
  }

  private func managedStatusLine() -> [String: Any] {
    [
      "type": "command",
      "command": "\(shellQuote(executablePath)) agent statusline --provider claude",
      "padding": 1,
      "refreshInterval": 30
    ]
  }

  private func claudeStatusLineDetail(root: [String: Any]) -> String {
    guard let statusLine = root["statusLine"] else {
      return "not installed"
    }
    guard let statusLineObject = statusLine as? [String: Any] else {
      return "manual review: non-object statusLine"
    }
    if isManagedStatusLine(statusLineObject) {
      return "installed"
    }
    return "existing non-1Context status line; not modified"
  }

  private func isManagedStatusLine(_ statusLine: [String: Any]) -> Bool {
    guard
      let type = statusLine["type"] as? String,
      type == "command",
      let command = statusLine["command"] as? String
    else {
      return false
    }
    return command.contains(" agent statusline --provider claude")
  }

  private func matcherGroup(event: AgentHookEvent) -> [String: Any] {
    var group: [String: Any] = [
      "hooks": [
        [
          "type": "command",
          "command": command(for: event),
          "timeout": 5
        ] as [String: Any]
      ]
    ]
    if event.requiresMatcher {
      group["matcher"] = "*"
    }
    return group
  }

  private func command(for event: AgentHookEvent) -> String {
    "\(shellQuote(executablePath)) agent hook --provider claude --event \(event.rawValue)"
  }

  private func hasManagedClaudeHook(root: [String: Any], event: AgentHookEvent) -> Bool {
    guard
      let hooks = root["hooks"] as? [String: Any],
      let groups = hooks[event.rawValue] as? [[String: Any]]
    else {
      return false
    }

    return groups.contains { group in
      guard let handlers = group["hooks"] as? [[String: Any]] else { return false }
      return handlers.contains { isManagedHandler($0, event: event) }
    }
  }

  private func removeManagedHandlers(from groups: [[String: Any]], event: AgentHookEvent) -> [[String: Any]] {
    groups.compactMap { group in
      guard let handlers = group["hooks"] as? [[String: Any]] else {
        return group
      }

      let remaining = handlers.filter { !isManagedHandler($0, event: event) }
      guard !remaining.isEmpty else { return nil }

      var copy = group
      copy["hooks"] = remaining
      return copy
    }
  }

  private func isManagedHandler(_ handler: [String: Any], event: AgentHookEvent) -> Bool {
    guard
      let type = handler["type"] as? String,
      type == "command",
      let command = handler["command"] as? String
    else {
      return false
    }
    return command.contains(" agent hook --provider claude --event \(event.rawValue)")
  }

  private func readClaudeSettings() -> ClaudeSettingsReadResult {
    guard fileManager.fileExists(atPath: claudeSettingsPath.path) else {
      return .missing
    }
    do {
      let data = try Data(contentsOf: claudeSettingsPath)
      guard !data.isEmpty else { return .loaded([:]) }
      let object = try JSONSerialization.jsonObject(with: data)
      guard let root = object as? [String: Any] else {
        return .unsafe("Claude settings root is not a JSON object; leaving it unchanged.")
      }
      if let hooks = root["hooks"], !(hooks is [String: Any]) {
        return .unsafe("Claude settings has a non-object hooks value; leaving it unchanged.")
      }
      if let hooks = root["hooks"] as? [String: Any] {
        for event in AgentHookEvent.allCases {
          if let value = hooks[event.rawValue], !(value is [[String: Any]]) {
            return .unsafe("Claude hooks.\(event.rawValue) is not an array of matcher groups; leaving it unchanged.")
          }
        }
      }
      return .loaded(root)
    } catch {
      return .unsafe("Claude settings could not be parsed as JSON; leaving it unchanged.")
    }
  }

  private func writeClaudeSettings(_ root: [String: Any]) throws {
    try fileManager.createDirectory(
      at: claudeSettingsPath.deletingLastPathComponent(),
      withIntermediateDirectories: true
    )
    let data = try JSONSerialization.data(withJSONObject: root, options: [.prettyPrinted, .sortedKeys])
    try data.write(to: claudeSettingsPath, options: .atomic)
    chmod(claudeSettingsPath.path, RuntimePermissions.privateFileMode)
  }

  private func writeState(report: AgentIntegrationsReport, action: AgentIntegrationAction) throws {
    try RuntimePermissions.ensurePrivateDirectory(paths.directory)
    try ensureAgentConfig()
    let state = AgentIntegrationState(
      updatedAt: Date(),
      claude: ProviderState(
        status: report.claudeStatus.state.rawValue,
        detail: "\(action.rawValue): \(report.claudeStatus.detail)",
        managedEvents: report.claudeStatus.installedEvents.map(\.rawValue)
      ),
      codex: ProviderState(
        status: report.codexStatus.state.rawValue,
        detail: report.codexStatus.detail
      )
    )
    let encoder = JSONEncoder()
    encoder.dateEncodingStrategy = .iso8601
    encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
    try RuntimePermissions.writePrivateData(try encoder.encode(state), to: paths.stateFile)
  }

  private func shellQuote(_ value: String) -> String {
    "'\(value.replacingOccurrences(of: "'", with: "'\\''"))'"
  }

  private func readAgentConfig() -> AgentConfig {
    guard let data = try? Data(contentsOf: paths.configFile),
      let config = try? JSONDecoder().decode(AgentConfig.self, from: data)
    else {
      return AgentConfig()
    }
    return config
  }

  private func ensureAgentConfig() throws {
    guard !fileManager.fileExists(atPath: paths.configFile.path) else { return }
    let encoder = JSONEncoder()
    encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
    try RuntimePermissions.writePrivateData(try encoder.encode(AgentConfig()), to: paths.configFile)
  }

  private enum ClaudeSettingsReadResult {
    case missing
    case unsafe(String)
    case loaded([String: Any])
  }
}

public enum AgentIntegrationError: Error, LocalizedError {
  case unsafeSettings(String)

  public var errorDescription: String? {
    switch self {
    case .unsafeSettings(let detail):
      return detail
    }
  }
}

public struct AgentHookExecutor {
  public static let defaultWikiURL = "http://localhost:3210"

  private let paths: AgentPaths
  private let userContentDirectory: URL
  private let wikiURL: String
  private let environment: [String: String]
  private let fileManager: FileManager

  public init(
    paths: AgentPaths = .current(),
    userContentDirectory: URL = RuntimePaths.current().userContentDirectory,
    wikiURL: String? = nil,
    environment: [String: String] = ProcessInfo.processInfo.environment,
    fileManager: FileManager = .default
  ) {
    self.paths = paths
    self.userContentDirectory = userContentDirectory
    self.wikiURL = wikiURL ?? environment["ONECONTEXT_WIKI_URL"] ?? Self.configuredWikiURL(paths: paths)
    self.environment = environment
    self.fileManager = fileManager
  }

  public static func configuredWikiURL(paths: AgentPaths = .current()) -> String {
    guard let data = try? Data(contentsOf: paths.configFile),
      let config = try? JSONDecoder().decode(AgentConfig.self, from: data),
      !config.wikiURL.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    else {
      return defaultWikiURL
    }
    return config.wikiURL
  }

  public func execute(provider: AgentProvider, event: AgentHookEvent, inputData: Data) -> AgentHookOutput {
    guard provider == .claude else {
      return AgentHookOutput(hookSpecificOutput: AgentHookSpecificOutput(hookEventName: event.rawValue))
    }

    let input = (try? JSONDecoder().decode(AgentHookInput.self, from: inputData))
    writeDebugLog(event: event, input: input)

    switch event {
    case .sessionStart:
      return output(event: event, additionalContext: sessionStartContext(input: input))
    case .userPromptSubmit:
      return output(event: event, additionalContext: userPromptContext(input: input))
    case .postToolUse, .preCompact, .sessionEnd:
      return output(event: event, additionalContext: nil)
    }
  }

  private func output(event: AgentHookEvent, additionalContext: String?) -> AgentHookOutput {
    let systemMessage = event == .sessionStart ? additionalContext?.components(separatedBy: "\n").first : nil
    return AgentHookOutput(
      systemMessage: systemMessage,
      hookSpecificOutput: AgentHookSpecificOutput(
        hookEventName: event.rawValue,
        additionalContext: additionalContext
      )
    )
  }

  private func sessionStartContext(input: AgentHookInput?) -> String? {
    var pointers: [String] = [
      "View your 1Context wiki at \(wikiURL)"
    ]
    if fileManager.fileExists(atPath: userContentDirectory.path) {
      pointers.append("1Context local wiki: \(userContentDirectory.path)")
    }
    if let repo = repoName(from: input?.cwd) {
      pointers.append("Current repo: \(repo)")
    }
    guard !pointers.isEmpty else { return nil }
    return pointers.joined(separator: "\n")
  }

  private func userPromptContext(input: AgentHookInput?) -> String? {
    guard let repo = repoName(from: input?.cwd) else { return nil }
    return "1Context repo pointer: \(repo)"
  }

  private func repoName(from cwd: String?) -> String? {
    guard let cwd, !cwd.isEmpty else { return nil }
    var url = URL(fileURLWithPath: cwd, isDirectory: true)
    for _ in 0..<8 {
      if fileManager.fileExists(atPath: url.appendingPathComponent(".git").path) {
        return url.lastPathComponent
      }
      let parent = url.deletingLastPathComponent()
      if parent.path == url.path { break }
      url = parent
    }
    return nil
  }

  private func writeDebugLog(event: AgentHookEvent, input: AgentHookInput?) {
    guard environment["ONECONTEXT_AGENT_HOOK_DEBUG"] == "1" else { return }
    do {
      try RuntimePermissions.ensurePrivateDirectory(paths.directory)
      let line = "\(ISO8601DateFormatter().string(from: Date())) event=\(event.rawValue) cwd=\(input?.cwd ?? "") session=\(input?.sessionID ?? "")\n"
      if fileManager.fileExists(atPath: paths.hookLogFile.path) {
        let handle = try FileHandle(forWritingTo: paths.hookLogFile)
        defer { try? handle.close() }
        try handle.seekToEnd()
        handle.write(Data(line.utf8))
      } else {
        try RuntimePermissions.writePrivateString(line, toFile: paths.hookLogFile.path)
      }
    } catch {
      // Hooks must stay non-blocking even when local debug logging is unavailable.
    }
  }
}

public struct AgentStatusLineRenderer {
  private let paths: AgentPaths
  private let environment: [String: String]

  public init(
    paths: AgentPaths = .current(),
    environment: [String: String] = ProcessInfo.processInfo.environment
  ) {
    self.paths = paths
    self.environment = environment
  }

  public func render(provider: AgentProvider, inputData: Data) -> String {
    guard provider == .claude else { return "" }
    let config = readConfig()
    let url = environment["ONECONTEXT_WIKI_URL"] ?? config.wikiURL
    return "\(config.statusLineLabel): \(url)"
  }

  private func readConfig() -> AgentConfig {
    guard let data = try? Data(contentsOf: paths.configFile),
      let config = try? JSONDecoder().decode(AgentConfig.self, from: data)
    else {
      return AgentConfig()
    }
    return config
  }
}
