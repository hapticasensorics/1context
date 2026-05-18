import XCTest
import Darwin
@testable import OneContextPlatform

final class PathAndPermissionTests: XCTestCase {
  func testRuntimePathsCanBeConstructedForFixtureRoots() {
    let root = URL(fileURLWithPath: "/tmp/1ctx-platform-test", isDirectory: true)
    let paths = RuntimePaths(
      userContentDirectory: root.appendingPathComponent("user", isDirectory: true),
      appSupportDirectory: root.appendingPathComponent("support", isDirectory: true),
      logDirectory: root.appendingPathComponent("logs", isDirectory: true),
      cacheDirectory: root.appendingPathComponent("cache", isDirectory: true),
      socketPath: root.appendingPathComponent("run/custom.sock").path,
      logPath: root.appendingPathComponent("logs/custom.log").path,
      preferencesPath: root.appendingPathComponent("prefs.plist").path
    )

    XCTAssertEqual(paths.userContentDirectory.path, "/tmp/1ctx-platform-test/user")
    XCTAssertEqual(paths.userWikiDirectory.path, "/tmp/1ctx-platform-test/user/user-wiki")
    XCTAssertEqual(paths.userWikiSourceDirectory.path, "/tmp/1ctx-platform-test/user/user-wiki/source")
    XCTAssertEqual(paths.userWikiSiteDirectory.path, "/tmp/1ctx-platform-test/user/user-wiki/site")
    XCTAssertEqual(paths.contextEngineDirectory.path, "/tmp/1ctx-platform-test/user/context-engine")
    XCTAssertEqual(paths.contextEngineIndexesDirectory.path, "/tmp/1ctx-platform-test/user/context-engine/indexes")
    XCTAssertEqual(paths.appSupportDirectory.path, "/tmp/1ctx-platform-test/support")
    XCTAssertEqual(paths.appSupportIndexesDirectory.path, "/tmp/1ctx-platform-test/support/indexes")
    XCTAssertEqual(paths.lanceDBIndexDirectory.path, "/tmp/1ctx-platform-test/support/indexes/lancedb")
    XCTAssertEqual(paths.appSupportSetupDirectory.path, "/tmp/1ctx-platform-test/support/setup")
    XCTAssertEqual(paths.runDirectory.path, "/tmp/1ctx-platform-test/support/run")
    XCTAssertEqual(paths.logPath, "/tmp/1ctx-platform-test/logs/custom.log")
    XCTAssertEqual(paths.socketPath, "/tmp/1ctx-platform-test/run/custom.sock")
    XCTAssertEqual(paths.preferencesPath, "/tmp/1ctx-platform-test/prefs.plist")
  }

  func testRuntimePermissionsWritePrivateFilesAndDirectories() throws {
    let root = FileManager.default.temporaryDirectory
      .appendingPathComponent("1ctx-permissions-\(UUID().uuidString)", isDirectory: true)
    defer { try? FileManager.default.removeItem(at: root) }

    try RuntimePermissions.ensurePrivateDirectory(root)
    let file = root.appendingPathComponent("state")
    try RuntimePermissions.writePrivateString("running\n", toFile: file.path)

    XCTAssertEqual(try mode(root), 0o700)
    XCTAssertEqual(try mode(file), 0o600)
  }

  func testDebugRuntimeHomeOverrideUsesProductionShapeUnderFixtureRoot() {
    #if DEBUG
    let key = "ONECONTEXT_DEV_RUNTIME_HOME"
    let oldValue = getenv(key).map { String(cString: $0) }
    defer {
      if let oldValue {
        setenv(key, oldValue, 1)
      } else {
        unsetenv(key)
      }
    }

    setenv(key, "/tmp/1ctx-dev-runtime-home", 1)
    let paths = RuntimePaths.current()

    XCTAssertEqual(paths.userContentDirectory.path, "/tmp/1ctx-dev-runtime-home/1Context")
    XCTAssertEqual(paths.userWikiDirectory.path, "/tmp/1ctx-dev-runtime-home/1Context/user-wiki")
    XCTAssertEqual(paths.contextEngineDirectory.path, "/tmp/1ctx-dev-runtime-home/1Context/context-engine")
    XCTAssertEqual(
      paths.appSupportDirectory.path,
      "/tmp/1ctx-dev-runtime-home/Library/Application Support/1Context"
    )
    #endif
  }

  func testProductionRuntimePathsDoNotRequireDebugRuntimeHomeOverride() {
    #if DEBUG
    let key = "ONECONTEXT_DEV_RUNTIME_HOME"
    let oldValue = getenv(key).map { String(cString: $0) }
    defer {
      if let oldValue {
        setenv(key, oldValue, 1)
      } else {
        unsetenv(key)
      }
    }

    unsetenv(key)
    #endif

    let home = FileManager.default.homeDirectoryForCurrentUser
    let paths = RuntimePaths.current()

    XCTAssertEqual(paths.userContentDirectory.path, home.appendingPathComponent("1Context").path)
    XCTAssertEqual(paths.userWikiDirectory.path, home.appendingPathComponent("1Context/user-wiki").path)
    XCTAssertEqual(paths.contextEngineDirectory.path, home.appendingPathComponent("1Context/context-engine").path)
    XCTAssertEqual(
      paths.appSupportDirectory.path,
      home.appendingPathComponent("Library/Application Support/1Context").path
    )
    XCTAssertEqual(paths.logDirectory.path, home.appendingPathComponent("Library/Logs/1Context").path)
    XCTAssertEqual(paths.cacheDirectory.path, home.appendingPathComponent("Library/Caches/1Context").path)
  }

  func testPlistEscapeEscapesXMLSpecialCharacters() {
    XCTAssertEqual(
      plistEscape("<tag attr=\"one&two\">it's</tag>"),
      "&lt;tag attr=&quot;one&amp;two&quot;&gt;it&apos;s&lt;/tag&gt;"
    )
  }

  private func mode(_ url: URL) throws -> Int {
    let attrs = try FileManager.default.attributesOfItem(atPath: url.path)
    return (attrs[.posixPermissions] as? NSNumber)?.intValue ?? -1
  }
}
