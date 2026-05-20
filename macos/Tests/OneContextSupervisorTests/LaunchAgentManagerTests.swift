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

  func testStartMenuRefusesDisposableRuntimePathsBeforeLaunchctlMutation() async throws {
    let home = temporaryRoot()
    let runtimeRoot = temporaryRoot()
    defer {
      try? FileManager.default.removeItem(at: home)
      try? FileManager.default.removeItem(at: runtimeRoot)
    }
    let recorder = ProcessCallRecorder()
    let manager = testManager(homeRoot: home, runtimeRoot: runtimeRoot) { executable, arguments, _ in
      await recorder.record(executable: executable, arguments: arguments)
    }

    let appPath = runtimeRoot.appendingPathComponent("Applications/1Context.app/Contents/MacOS/1Context").path
    do {
      try await manager.startMenu(appPath: appPath)
      XCTFail("startMenu should reject nonstandard runtime paths before touching launchctl")
    } catch {
      XCTAssertTrue(error.localizedDescription.contains("nonstandard runtime paths"))
    }

    let processCallCount = await recorder.count()
    XCTAssertEqual(processCallCount, 0)
    XCTAssertFalse(
      FileManager.default.fileExists(
        atPath: home.appendingPathComponent("Library/LaunchAgents/com.haptica.1context.menu.plist").path
      )
    )
  }

  func testStopMenuRefusesDisposableRuntimePathsWithoutRemovingRealPlist() async throws {
    let home = temporaryRoot()
    let runtimeRoot = temporaryRoot()
    defer {
      try? FileManager.default.removeItem(at: home)
      try? FileManager.default.removeItem(at: runtimeRoot)
    }
    let launchAgents = home.appendingPathComponent("Library/LaunchAgents", isDirectory: true)
    try FileManager.default.createDirectory(at: launchAgents, withIntermediateDirectories: true)
    let plist = launchAgents.appendingPathComponent("com.haptica.1context.menu.plist")
    try "sentinel\n".write(to: plist, atomically: true, encoding: .utf8)

    let recorder = ProcessCallRecorder()
    let manager = testManager(homeRoot: home, runtimeRoot: runtimeRoot) { executable, arguments, _ in
      await recorder.record(executable: executable, arguments: arguments)
    }

    await manager.stopMenu()

    let processCallCount = await recorder.count()
    XCTAssertEqual(processCallCount, 0)
    XCTAssertEqual(try String(contentsOf: plist, encoding: .utf8), "sentinel\n")
  }

  private func testManager(
    root: URL,
    processRunner: @escaping @Sendable (String, [String], TimeInterval) async -> ProcessResult
  ) -> LaunchAgentManager {
    testManager(homeRoot: root, runtimeRoot: root, processRunner: processRunner)
  }

  private func testManager(
    homeRoot: URL,
    runtimeRoot: URL,
    processRunner: @escaping @Sendable (String, [String], TimeInterval) async -> ProcessResult
  ) -> LaunchAgentManager {
    let paths = RuntimePaths(
      userContentDirectory: runtimeRoot.appendingPathComponent("1Context", isDirectory: true),
      appSupportDirectory: runtimeRoot.appendingPathComponent("Library/Application Support/1Context", isDirectory: true),
      logDirectory: runtimeRoot.appendingPathComponent("Library/Logs/1Context", isDirectory: true),
      cacheDirectory: runtimeRoot.appendingPathComponent("Library/Caches/1Context", isDirectory: true),
      preferencesPath: runtimeRoot.appendingPathComponent("Library/Preferences/com.haptica.1context.plist").path
    )
    return LaunchAgentManager(
      homeDirectory: homeRoot,
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

private actor ProcessCallRecorder {
  private var calls: [(executable: String, arguments: [String])] = []

  func record(executable: String, arguments: [String]) -> ProcessResult {
    calls.append((executable: executable, arguments: arguments))
    return (0, "", "")
  }

  func count() -> Int {
    calls.count
  }
}
