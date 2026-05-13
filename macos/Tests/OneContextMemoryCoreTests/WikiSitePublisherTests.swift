import Darwin
import Foundation
import XCTest
@testable import OneContextMemoryCore

final class WikiSitePublisherTests: XCTestCase {
  func testRefreshRendersAllWikiFamiliesBeforePublishing() throws {
    let root = try temporaryRoot()
    defer { try? FileManager.default.removeItem(at: root) }
    let bundle = try writePublishableBundle(root: root)
    try writeFakePython(root: root)

    let appSupport = root.appendingPathComponent("Application Support/1Context", isDirectory: true)
    let logDirectory = root.appendingPathComponent("Logs/1Context", isDirectory: true)
    try fileManager.createDirectory(at: logDirectory, withIntermediateDirectories: true)
    let log = logDirectory.appendingPathComponent("memory-core-args.log")
    let publisher = WikiSitePublisher(
      memoryPaths: paths(root: root),
      environment: [
        "ONECONTEXT_APP_SUPPORT_DIR": appSupport.path,
        "ONECONTEXT_LOG_DIR": logDirectory.path,
        "ONECONTEXT_MEMORY_CORE_BUNDLE_DIR": bundle.path
      ]
    )
    let siteRoot = appSupport.appendingPathComponent("wiki-site", isDirectory: true)

    _ = try publisher.publish(
      paths: WikiSitePublishPaths(
        current: siteRoot.appendingPathComponent("current", isDirectory: true),
        next: siteRoot.appendingPathComponent("next", isDirectory: true),
        previous: siteRoot.appendingPathComponent("previous", isDirectory: true)
      ),
      refresh: true
    )

    let logText = try String(contentsOf: log, encoding: .utf8)
    XCTAssertTrue(logText.contains("wiki ensure --json"))
    XCTAssertTrue(logText.contains("wiki render --no-evidence --json"))
    XCTAssertFalse(logText.contains("wiki render for-you --no-evidence --json"))
    XCTAssertTrue(fileManager.fileExists(atPath: siteRoot.appendingPathComponent("current/your-context.html").path))
    XCTAssertFalse(fileManager.fileExists(atPath: siteRoot.appendingPathComponent("current/goal.html").path))
    XCTAssertFalse(fileManager.fileExists(atPath: siteRoot.appendingPathComponent("current/goal.md").path))
    XCTAssertFalse(fileManager.fileExists(atPath: siteRoot.appendingPathComponent("current/your-context.md").path))
    XCTAssertFalse(fileManager.fileExists(atPath: siteRoot.appendingPathComponent("current/render-manifest.json").path))

    let manifestURL = siteRoot.appendingPathComponent("current/publish-manifest.json")
    let manifestData = try Data(contentsOf: manifestURL)
    let manifest = try XCTUnwrap(JSONSerialization.jsonObject(with: manifestData) as? [String: Any])
    let files = try XCTUnwrap(manifest["files"] as? [String])
    XCTAssertTrue(files.contains("your-context.html"))
    XCTAssertTrue(files.contains("api/wiki/site.json"))
    XCTAssertFalse(files.contains("goal.html"))
    XCTAssertFalse(files.contains("goal.md"))
    XCTAssertFalse(files.contains("your-context.md"))
    XCTAssertFalse(files.contains("render-manifest.json"))
    for publishedFile in files {
      XCTAssertFalse(publishedFile.hasPrefix("/"), publishedFile)
      XCTAssertFalse(publishedFile.contains(root.path), publishedFile)
      XCTAssertFalse(publishedFile.contains("/Users/"), publishedFile)
      XCTAssertFalse(publishedFile.contains("/dev/1context-public-launch"), publishedFile)
    }
  }

  private var fileManager: FileManager { .default }

  private func paths(root: URL) -> MemoryCorePaths {
    MemoryCorePaths(
      directory: root.appendingPathComponent("Application Support/1Context/memory-core", isDirectory: true),
      logFile: root.appendingPathComponent("Logs/1Context/memory-core.log")
    )
  }

  private func temporaryRoot() throws -> URL {
    let root = FileManager.default.temporaryDirectory
      .appendingPathComponent("1ctx-wiki-publisher-\(UUID().uuidString)", isDirectory: true)
    try fileManager.createDirectory(at: root, withIntermediateDirectories: true)
    return root
  }

  private func writePublishableBundle(root: URL) throws -> URL {
    let bundle = root.appendingPathComponent("bundle", isDirectory: true)
    let forYou = bundle.appendingPathComponent("wiki/menu/10-for-you/10-for-you", isDirectory: true)
    let operatorGoal = bundle.appendingPathComponent("wiki/menu/99-operator/10-goal", isDirectory: true)
    try fileManager.createDirectory(at: bundle.appendingPathComponent("bin", isDirectory: true), withIntermediateDirectories: true)
    try fileManager.createDirectory(at: forYou, withIntermediateDirectories: true)
    try fileManager.createDirectory(at: operatorGoal, withIntermediateDirectories: true)
    try fileManager.createDirectory(at: bundle.appendingPathComponent("wiki-engine/theme/css", isDirectory: true), withIntermediateDirectories: true)
    try fileManager.createDirectory(at: bundle.appendingPathComponent("wiki-engine/theme/js", isDirectory: true), withIntermediateDirectories: true)
    try fileManager.createDirectory(at: bundle.appendingPathComponent("wiki-engine/tools", isDirectory: true), withIntermediateDirectories: true)
    try fileManager.createDirectory(at: bundle.appendingPathComponent("src/onectx/wiki", isDirectory: true), withIntermediateDirectories: true)
    try "fixture\n".write(to: bundle.appendingPathComponent("pyproject.toml"), atomically: true, encoding: .utf8)
    try "fixture\n".write(to: bundle.appendingPathComponent("uv.lock"), atomically: true, encoding: .utf8)
    try "body {}\n".write(to: bundle.appendingPathComponent("wiki-engine/theme/css/theme.css"), atomically: true, encoding: .utf8)
    try "window.__onecontext = true;\n".write(to: bundle.appendingPathComponent("wiki-engine/theme/js/enhance.js"), atomically: true, encoding: .utf8)
    try "render\n".write(to: bundle.appendingPathComponent("wiki-engine/tools/render-to-dir.mjs"), atomically: true, encoding: .utf8)
    try "render\n".write(to: bundle.appendingPathComponent("src/onectx/wiki/render.py"), atomically: true, encoding: .utf8)
    try "id = \"for-you\"\n".write(to: forYou.appendingPathComponent("family.toml"), atomically: true, encoding: .utf8)
    try """
    id = "goal"
    label = "Goal"
    route = "/goal"

    [policies]
    publish_to_user_wiki = false
    audience = "operator"
    """.write(to: operatorGoal.appendingPathComponent("family.toml"), atomically: true, encoding: .utf8)

    let executable = bundle.appendingPathComponent("bin/1context-memory-core")
    try """
    #!/bin/sh
    log="${ONECONTEXT_LOG_DIR:-}/memory-core-args.log"
    if [ -n "$log" ]; then printf '%s\\n' "$*" >> "$log"; fi
    root="$(cd "$(dirname "$0")/.." && pwd)"
    if [ "$1" = "wiki" ] && [ "$2" = "render" ]; then
      mkdir -p "$root/wiki/menu/10-for-you/10-for-you/generated" "$root/wiki/menu/99-operator/10-goal/generated" "$root/wiki/generated"
      printf '{"for-you":{"slug":"for-you-2026-05-09"}}\\n' > "$root/wiki/menu/10-for-you/10-for-you/generated/latest_for_family.json"
      printf '<html>For You</html>\\n' > "$root/wiki/menu/10-for-you/10-for-you/generated/for-you-2026-05-09.html"
      printf '<html>Your Context</html>\\n' > "$root/wiki/menu/10-for-you/10-for-you/generated/your-context.html"
      printf '# Your Context\\n' > "$root/wiki/menu/10-for-you/10-for-you/generated/your-context.md"
      printf '{"source":"/Users/paulhan/dev/1context-public-launch"}\\n' > "$root/wiki/menu/10-for-you/10-for-you/generated/render-manifest.json"
      printf '<html>Goal</html>\\n' > "$root/wiki/menu/99-operator/10-goal/generated/goal.html"
      printf '# Goal\\n' > "$root/wiki/menu/99-operator/10-goal/generated/goal.md"
      printf '{"schema_version":"test"}\\n' > "$root/wiki/generated/site-manifest.json"
      printf '{"schema_version":"test"}\\n' > "$root/wiki/generated/content-index.json"
      printf '{"schema_version":"test"}\\n' > "$root/wiki/generated/wiki-stats.json"
    fi
    printf '{"status":"ok","schema_version":1}\\n'
    """.write(to: executable, atomically: true, encoding: .utf8)
    chmod(executable.path, 0o700)
    return bundle
  }

  private func writeFakePython(root: URL) throws {
    let python = paths(root: root).directory.appendingPathComponent("venv/bin/python3")
    try fileManager.createDirectory(at: python.deletingLastPathComponent(), withIntermediateDirectories: true)
    try "#!/bin/sh\nexit 0\n".write(to: python, atomically: true, encoding: .utf8)
    chmod(python.path, 0o700)
  }
}
