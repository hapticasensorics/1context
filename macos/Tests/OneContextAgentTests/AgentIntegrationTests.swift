import Foundation
import XCTest
@testable import OneContextAgent

final class AgentIntegrationTests: XCTestCase {
  func testAgentPathsUseRuntimeAppSupportAgentDirectory() {
    let paths = AgentPaths.current(environment: [
      "ONECONTEXT_APP_SUPPORT_DIR": "/tmp/1ctx-agent-test/support"
    ])

    XCTAssertEqual(paths.directory.path, "/tmp/1ctx-agent-test/support/agent")
    XCTAssertEqual(paths.configFile.path, "/tmp/1ctx-agent-test/support/agent/config.json")
    XCTAssertEqual(paths.stateFile.path, "/tmp/1ctx-agent-test/support/agent/integrations.json")
    XCTAssertEqual(paths.hookLogFile.path, "/tmp/1ctx-agent-test/support/agent/hook.log")
  }

  func testInstallMergesClaudeHooksIdempotently() throws {
    let root = try temporaryRoot()
    defer { try? FileManager.default.removeItem(at: root) }
    let settings = root.appendingPathComponent(".claude/settings.json")
    let state = AgentPaths(directory: root.appendingPathComponent("agent", isDirectory: true))
    try FileManager.default.createDirectory(at: settings.deletingLastPathComponent(), withIntermediateDirectories: true)
    try Data("""
    {
      "theme": "dark",
      "hooks": {
        "UserPromptSubmit": [
          {
            "hooks": [
              { "type": "command", "command": "echo existing" }
            ]
          }
        ]
      }
    }
    """.utf8).write(to: settings)

    let manager = AgentIntegrationManager(
      paths: state,
      claudeSettingsPath: settings,
      executablePath: "/opt/homebrew/bin/1context"
    )

    _ = try manager.install()
    _ = try manager.install()

    let object = try readObject(settings)
    XCTAssertEqual(object["theme"] as? String, "dark")
    let hooks = try XCTUnwrap(object["hooks"] as? [String: Any])

    for event in AgentHookEvent.allCases {
      let groups = try XCTUnwrap(hooks[event.rawValue] as? [[String: Any]])
      let managedCount = groups.flatMap { ($0["hooks"] as? [[String: Any]]) ?? [] }
        .filter { ($0["command"] as? String)?.contains(" agent hook --provider claude --event \(event.rawValue)") == true }
        .count
      XCTAssertEqual(managedCount, 1, event.rawValue)
    }

    let promptGroups = try XCTUnwrap(hooks["UserPromptSubmit"] as? [[String: Any]])
    let promptCommands = promptGroups.flatMap { ($0["hooks"] as? [[String: Any]]) ?? [] }
      .compactMap { $0["command"] as? String }
    XCTAssertTrue(promptCommands.contains("echo existing"))

    let postGroups = try XCTUnwrap(hooks["PostToolUse"] as? [[String: Any]])
    XCTAssertEqual(postGroups.last?["matcher"] as? String, "*")
    let statusLine = try XCTUnwrap(object["statusLine"] as? [String: Any])
    XCTAssertEqual(statusLine["type"] as? String, "command")
    XCTAssertTrue((statusLine["command"] as? String)?.contains(" agent statusline --provider claude") == true)
    XCTAssertEqual(statusLine["refreshInterval"] as? Int, 30)
    XCTAssertTrue(FileManager.default.fileExists(atPath: state.configFile.path))
    XCTAssertTrue(FileManager.default.fileExists(atPath: state.stateFile.path))
  }

  func testInstallPreservesExistingNonManagedStatusLine() throws {
    let root = try temporaryRoot()
    defer { try? FileManager.default.removeItem(at: root) }
    let settings = root.appendingPathComponent("settings.json")
    try writeObject([
      "statusLine": [
        "type": "command",
        "command": "echo keep"
      ]
    ], to: settings)

    let manager = AgentIntegrationManager(
      paths: AgentPaths(directory: root.appendingPathComponent("agent", isDirectory: true)),
      claudeSettingsPath: settings,
      executablePath: "/opt/homebrew/bin/1context"
    )

    let report = try manager.install()

    XCTAssertEqual(report.claudeStatus.statusLineDetail, "existing non-1Context status line; not modified")
    let object = try readObject(settings)
    let statusLine = try XCTUnwrap(object["statusLine"] as? [String: Any])
    XCTAssertEqual(statusLine["command"] as? String, "echo keep")
  }

  func testUnsafeClaudeSettingsAreNotModified() throws {
    let root = try temporaryRoot()
    defer { try? FileManager.default.removeItem(at: root) }
    let settings = root.appendingPathComponent("settings.json")
    try Data("{ nope".utf8).write(to: settings)

    let manager = AgentIntegrationManager(
      paths: AgentPaths(directory: root.appendingPathComponent("agent", isDirectory: true)),
      claudeSettingsPath: settings,
      executablePath: "/opt/homebrew/bin/1context"
    )

    let report = try manager.install()

    XCTAssertEqual(report.claudeStatus.state, .manualReview)
    XCTAssertEqual(try String(contentsOf: settings), "{ nope")
  }

  func testUninstallRemovesOnlyManagedClaudeHooks() throws {
    let root = try temporaryRoot()
    defer { try? FileManager.default.removeItem(at: root) }
    let settings = root.appendingPathComponent("settings.json")
    let manager = AgentIntegrationManager(
      paths: AgentPaths(directory: root.appendingPathComponent("agent", isDirectory: true)),
      claudeSettingsPath: settings,
      executablePath: "/opt/homebrew/bin/1context"
    )

    _ = try manager.install()
    var object = try readObject(settings)
    var hooks = try XCTUnwrap(object["hooks"] as? [String: Any])
    var groups = try XCTUnwrap(hooks["SessionStart"] as? [[String: Any]])
    groups.append([
      "hooks": [
        ["type": "command", "command": "echo keep"]
      ]
    ])
    hooks["SessionStart"] = groups
    object["hooks"] = hooks
    try writeObject(object, to: settings)

    let report = try manager.uninstall()

    XCTAssertEqual(report.claudeStatus.state, .notInstalled)
    let uninstalled = try readObject(settings)
    let uninstalledHooks = try XCTUnwrap(uninstalled["hooks"] as? [String: Any])
    let sessionGroups = try XCTUnwrap(uninstalledHooks["SessionStart"] as? [[String: Any]])
    let commands = sessionGroups.flatMap { ($0["hooks"] as? [[String: Any]]) ?? [] }
      .compactMap { $0["command"] as? String }
    XCTAssertEqual(commands, ["echo keep"])
    XCTAssertNil(uninstalledHooks["PostToolUse"])
    XCTAssertNil(uninstalled["statusLine"])
  }

  func testHookExecutorReturnsTinyContextAndNoOpSuccess() throws {
    let root = try temporaryRoot()
    defer { try? FileManager.default.removeItem(at: root) }
    let wiki = root.appendingPathComponent("1Context", isDirectory: true)
    let repo = root.appendingPathComponent("repo", isDirectory: true)
    try FileManager.default.createDirectory(at: wiki, withIntermediateDirectories: true)
    try FileManager.default.createDirectory(at: repo.appendingPathComponent(".git", isDirectory: true), withIntermediateDirectories: true)

    let executor = AgentHookExecutor(
      paths: AgentPaths(directory: root.appendingPathComponent("agent", isDirectory: true)),
      userContentDirectory: wiki,
      wikiURL: "http://localhost:3210",
      environment: [:]
    )
    let input = Data("""
    {"session_id":"abc","cwd":"\(repo.path)","hook_event_name":"SessionStart"}
    """.utf8)

    let start = executor.execute(provider: .claude, event: .sessionStart, inputData: input)
    XCTAssertEqual(start.hookSpecificOutput?.hookEventName, "SessionStart")
    XCTAssertEqual(start.systemMessage, "View your 1Context wiki at http://localhost:3210")
    XCTAssertTrue(start.hookSpecificOutput?.additionalContext?.contains("View your 1Context wiki at http://localhost:3210") == true)
    XCTAssertTrue(start.hookSpecificOutput?.additionalContext?.contains("1Context local wiki") == true)
    XCTAssertTrue(start.hookSpecificOutput?.additionalContext?.contains("Current repo: repo") == true)

    let postTool = executor.execute(provider: .claude, event: .postToolUse, inputData: Data("{}".utf8))
    XCTAssertNil(postTool.systemMessage)
    XCTAssertEqual(postTool.hookSpecificOutput?.hookEventName, "PostToolUse")
    XCTAssertNil(postTool.hookSpecificOutput?.additionalContext)
  }

  func testHookExecutorReadsWikiURLFromLiveConfig() throws {
    let root = try temporaryRoot()
    defer { try? FileManager.default.removeItem(at: root) }
    let paths = AgentPaths(directory: root.appendingPathComponent("agent", isDirectory: true))
    try FileManager.default.createDirectory(at: paths.directory, withIntermediateDirectories: true)
    try writeAgentConfig(AgentConfig(wikiURL: "http://localhost:4100"), to: paths.configFile)

    let first = AgentHookExecutor(paths: paths, environment: [:])
      .execute(provider: .claude, event: .sessionStart, inputData: Data("{}".utf8))
    XCTAssertEqual(first.systemMessage, "View your 1Context wiki at http://localhost:4100")
    XCTAssertTrue(first.hookSpecificOutput?.additionalContext?.contains("http://localhost:4100") == true)

    try writeAgentConfig(AgentConfig(wikiURL: "http://localhost:4101"), to: paths.configFile)

    let second = AgentHookExecutor(paths: paths, environment: [:])
      .execute(provider: .claude, event: .sessionStart, inputData: Data("{}".utf8))
    XCTAssertEqual(second.systemMessage, "View your 1Context wiki at http://localhost:4101")
    XCTAssertTrue(second.hookSpecificOutput?.additionalContext?.contains("http://localhost:4101") == true)
  }

  func testStatusLineRendererReadsLiveConfig() throws {
    let root = try temporaryRoot()
    defer { try? FileManager.default.removeItem(at: root) }
    let paths = AgentPaths(directory: root.appendingPathComponent("agent", isDirectory: true))
    try FileManager.default.createDirectory(at: paths.directory, withIntermediateDirectories: true)
    try writeAgentConfig(
      AgentConfig(wikiURL: "http://localhost:5200", statusLineLabel: "1Context wiki private"),
      to: paths.configFile
    )

    let first = AgentStatusLineRenderer(paths: paths, environment: [:])
      .render(provider: .claude, inputData: Data("{}".utf8))
    XCTAssertEqual(first, "1Context wiki private: http://localhost:5200")

    try writeAgentConfig(AgentConfig(wikiURL: "http://localhost:5201"), to: paths.configFile)
    let second = AgentStatusLineRenderer(paths: paths, environment: [:])
      .render(provider: .claude, inputData: Data("{}".utf8))
    XCTAssertEqual(second, "1Context wiki: http://localhost:5201")
  }

  private func temporaryRoot() throws -> URL {
    let root = FileManager.default.temporaryDirectory
      .appendingPathComponent("1ctx-agent-\(UUID().uuidString)", isDirectory: true)
    try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
    return root
  }

  private func readObject(_ url: URL) throws -> [String: Any] {
    let data = try Data(contentsOf: url)
    return try XCTUnwrap(JSONSerialization.jsonObject(with: data) as? [String: Any])
  }

  private func writeObject(_ object: [String: Any], to url: URL) throws {
    let data = try JSONSerialization.data(withJSONObject: object, options: [.prettyPrinted, .sortedKeys])
    try data.write(to: url)
  }

  private func writeAgentConfig(_ config: AgentConfig, to url: URL) throws {
    let encoder = JSONEncoder()
    encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
    try encoder.encode(config).write(to: url)
  }
}
