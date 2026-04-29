import XCTest
@testable import OneContextPlatform

final class PathAndPermissionTests: XCTestCase {
  func testRuntimePathsHonorEnvironmentOverrides() {
    let root = URL(fileURLWithPath: "/tmp/1ctx-platform-test", isDirectory: true)
    let paths = RuntimePaths.current(environment: [
      "ONECONTEXT_USER_CONTENT_DIR": root.appendingPathComponent("user").path,
      "ONECONTEXT_APP_SUPPORT_DIR": root.appendingPathComponent("support").path,
      "ONECONTEXT_LOG_DIR": root.appendingPathComponent("logs").path,
      "ONECONTEXT_LOG_PATH": root.appendingPathComponent("logs/custom.log").path,
      "ONECONTEXT_CACHE_DIR": root.appendingPathComponent("cache").path,
      "ONECONTEXT_SOCKET_PATH": root.appendingPathComponent("run/custom.sock").path,
      "ONECONTEXT_PREFERENCES_PATH": root.appendingPathComponent("prefs.plist").path
    ])

    XCTAssertEqual(paths.userContentDirectory.path, "/tmp/1ctx-platform-test/user")
    XCTAssertEqual(paths.appSupportDirectory.path, "/tmp/1ctx-platform-test/support")
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
