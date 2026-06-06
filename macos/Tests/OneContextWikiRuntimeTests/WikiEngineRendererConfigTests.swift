import Foundation
import XCTest

@testable import OneContextWikiRuntime

final class WikiEngineRendererConfigTests: XCTestCase {
  func testDiscoverUsesBundledWikiEngineResource() throws {
    let root = temporaryRoot()
    addTeardownBlock { try? FileManager.default.removeItem(at: root) }
    let resources = root.appendingPathComponent("Resources", isDirectory: true)
    let engine = resources.appendingPathComponent("WikiEngine", isDirectory: true)
    try FileManager.default.createDirectory(
      at: engine.appendingPathComponent("tools", isDirectory: true),
      withIntermediateDirectories: true
    )
    XCTAssertTrue(FileManager.default.createFile(
      atPath: engine.appendingPathComponent("tools/render-site.mjs").path,
      contents: Data()
    ))

    let config = WikiEngineRendererConfig.discover(
      environment: [:],
      resourceURL: resources,
      executableURL: nil,
      nodeSearchPaths: []
    )

    XCTAssertEqual(config?.engineDirectory.standardizedFileURL, engine.standardizedFileURL)
    XCTAssertEqual(config?.renderTool.standardizedFileURL, engine.appendingPathComponent("tools/render-site.mjs").standardizedFileURL)
    XCTAssertEqual(config?.nodeExecutable.path, "/usr/bin/env")
  }

  func testDiscoverFindsNodeFromSearchPath() throws {
    let root = temporaryRoot()
    addTeardownBlock { try? FileManager.default.removeItem(at: root) }
    let resources = root.appendingPathComponent("Resources", isDirectory: true)
    let engine = resources.appendingPathComponent("WikiEngine", isDirectory: true)
    try FileManager.default.createDirectory(
      at: engine.appendingPathComponent("tools", isDirectory: true),
      withIntermediateDirectories: true
    )
    XCTAssertTrue(FileManager.default.createFile(
      atPath: engine.appendingPathComponent("tools/render-site.mjs").path,
      contents: Data()
    ))
    let node = root.appendingPathComponent("bin/node")
    try FileManager.default.createDirectory(at: node.deletingLastPathComponent(), withIntermediateDirectories: true)
    XCTAssertTrue(FileManager.default.createFile(atPath: node.path, contents: Data()))
    try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: node.path)

    let config = WikiEngineRendererConfig.discover(
      environment: [:],
      resourceURL: resources,
      executableURL: nil,
      nodeSearchPaths: [node.path]
    )

    XCTAssertEqual(config?.nodeExecutable.standardizedFileURL, node.standardizedFileURL)
  }

  func testDiscoverEnvironmentOverrideWins() throws {
    let root = temporaryRoot()
    addTeardownBlock { try? FileManager.default.removeItem(at: root) }
    let resources = root.appendingPathComponent("Resources", isDirectory: true)
    let engine = root.appendingPathComponent("CustomWikiEngine", isDirectory: true)

    let config = WikiEngineRendererConfig.discover(
      environment: [
        "ONECONTEXT_WIKI_ENGINE_DIR": engine.path,
        "ONECONTEXT_NODE": "/custom/node"
      ],
      resourceURL: resources,
      executableURL: nil
    )

    XCTAssertEqual(config?.engineDirectory.standardizedFileURL, engine.standardizedFileURL)
    XCTAssertEqual(config?.nodeExecutable.path, "/custom/node")
  }

  private func temporaryRoot() -> URL {
    let url = FileManager.default.temporaryDirectory
      .appendingPathComponent("1context-wiki-engine-config-\(UUID().uuidString)", isDirectory: true)
    try? FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
    return url
  }
}
