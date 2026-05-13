import XCTest
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
