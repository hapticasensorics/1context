import Foundation
import XCTest
@testable import OneContextSupervisor
import OneContextPlatform

final class LaunchAgentManagerTests: XCTestCase {
  func testStartMenuWritesCrashRecoveringLoginAgent() async throws {
    let root = temporaryRoot()
    defer { try? FileManager.default.removeItem(at: root) }
    let manager = testManager(root: root) { executable, arguments, _ in
      if executable == "/bin/launchctl", arguments.first == "print" {
        return (1, "", "not loaded")
      }
      return (0, "", "")
    }

    let appPath = root.appendingPathComponent("Applications/1Context.app/Contents/MacOS/1Context").path
    try await manager.startMenu(appPath: appPath)

    let plist = root.appendingPathComponent("Library/LaunchAgents/com.haptica.1context.menu.plist")
    let data = try Data(contentsOf: plist)
    let object = try XCTUnwrap(try PropertyListSerialization.propertyList(from: data, format: nil) as? [String: Any])

    XCTAssertEqual(object["Label"] as? String, "com.haptica.1context.menu")
    XCTAssertEqual(object["ProgramArguments"] as? [String], [appPath])
    XCTAssertEqual(object["RunAtLoad"] as? Bool, true)
    let keepAlive = try XCTUnwrap(object["KeepAlive"] as? [String: Bool])
    XCTAssertEqual(keepAlive["SuccessfulExit"], false)
    XCTAssertEqual(keepAlive["Crashed"], true)
  }

  func testStartRuntimeCreatesUserDirectoriesAndLaunchAgent() async throws {
    let root = temporaryRoot()
    defer { try? FileManager.default.removeItem(at: root) }
    let manager = testManager(root: root) { executable, arguments, _ in
      if executable == "/bin/launchctl", arguments.first == "bootstrap" {
        return (0, "", "")
      }
      return (1, "", "not loaded")
    }

    let daemonPath = root.appendingPathComponent("Applications/1Context.app/Contents/MacOS/1contextd").path
    try await manager.start(daemonPath: daemonPath)

    let plist = root.appendingPathComponent("Library/LaunchAgents/com.haptica.1context.plist")
    let data = try Data(contentsOf: plist)
    let object = try XCTUnwrap(try PropertyListSerialization.propertyList(from: data, format: nil) as? [String: Any])

    XCTAssertEqual(object["Label"] as? String, "com.haptica.1context")
    XCTAssertEqual(object["ProgramArguments"] as? [String], [daemonPath])
    XCTAssertEqual(object["RunAtLoad"] as? Bool, true)
    XCTAssertEqual(object["KeepAlive"] as? Bool, true)
    XCTAssertTrue(FileManager.default.fileExists(atPath: root.appendingPathComponent("Library/Application Support/1Context/run").path))
    XCTAssertTrue(FileManager.default.fileExists(atPath: root.appendingPathComponent("Library/Logs/1Context").path))
  }

  private func testManager(
    root: URL,
    processRunner: @escaping @Sendable (String, [String], TimeInterval) async -> ProcessResult
  ) -> LaunchAgentManager {
    let paths = RuntimePaths(
      userContentDirectory: root.appendingPathComponent("1Context", isDirectory: true),
      appSupportDirectory: root.appendingPathComponent("Library/Application Support/1Context", isDirectory: true),
      logDirectory: root.appendingPathComponent("Library/Logs/1Context", isDirectory: true),
      cacheDirectory: root.appendingPathComponent("Library/Caches/1Context", isDirectory: true),
      preferencesPath: root.appendingPathComponent("Library/Preferences/com.haptica.1context.plist").path
    )
    return LaunchAgentManager(
      homeDirectory: root,
      runtimePaths: paths,
      uid: 501,
      isRootLifecycleRejected: { false },
      processRunner: processRunner
    )
  }

  private func temporaryRoot() -> URL {
    URL(fileURLWithPath: NSTemporaryDirectory(), isDirectory: true)
      .appendingPathComponent("1context-supervisor-tests-\(UUID().uuidString)", isDirectory: true)
  }
}
