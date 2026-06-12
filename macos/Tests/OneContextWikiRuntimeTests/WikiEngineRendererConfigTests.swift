import Foundation
import XCTest

import OneContextPlatform
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
      executableURL: nil
    )

    XCTAssertEqual(config?.engineDirectory.standardizedFileURL, engine.standardizedFileURL)
    XCTAssertEqual(config?.renderTool.standardizedFileURL, engine.appendingPathComponent("tools/render-site.mjs").standardizedFileURL)
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
    XCTAssertEqual(config?.node.executable?.path, "/custom/node")
    XCTAssertEqual(config?.node.source, .envOverride)
  }

  func testResolveNodeEnvironmentOverrideWinsOverBundledRuntime() throws {
    let root = temporaryRoot()
    addTeardownBlock { try? FileManager.default.removeItem(at: root) }
    let resources = root.appendingPathComponent("Resources", isDirectory: true)
    try writeExecutable(at: resources.appendingPathComponent("node-runtime/bin/node"))

    let resolution = WikiEngineRendererConfig.resolveNode(
      environment: ["ONECONTEXT_NODE": "/custom/node"],
      resourceURL: resources,
      executableURL: nil,
      systemSearchPaths: []
    )

    XCTAssertEqual(resolution.source, .envOverride)
    XCTAssertEqual(resolution.executable?.path, "/custom/node")
  }

  func testResolveNodePrefersBundledRuntimeFromResources() throws {
    let root = temporaryRoot()
    addTeardownBlock { try? FileManager.default.removeItem(at: root) }
    let resources = root.appendingPathComponent("Resources", isDirectory: true)
    let bundledNode = resources.appendingPathComponent("node-runtime/bin/node")
    try writeExecutable(at: bundledNode)
    let systemBin = root.appendingPathComponent("system-bin", isDirectory: true)
    try writeExecutable(at: systemBin.appendingPathComponent("node"))

    let resolution = WikiEngineRendererConfig.resolveNode(
      environment: [:],
      resourceURL: resources,
      executableURL: nil,
      systemSearchPaths: [systemBin.path]
    )

    XCTAssertEqual(resolution.source, .bundled)
    XCTAssertEqual(resolution.executable?.standardizedFileURL, bundledNode.standardizedFileURL)
  }

  func testResolveNodeFindsBundledRuntimeRelativeToExecutable() throws {
    let root = temporaryRoot()
    addTeardownBlock { try? FileManager.default.removeItem(at: root) }
    let bundledNode = root.appendingPathComponent("Contents/Resources/node-runtime/bin/node")
    try writeExecutable(at: bundledNode)

    let resolution = WikiEngineRendererConfig.resolveNode(
      environment: [:],
      resourceURL: nil,
      executableURL: root.appendingPathComponent("Contents/MacOS/1contextd"),
      systemSearchPaths: []
    )

    XCTAssertEqual(resolution.source, .bundled)
    XCTAssertEqual(resolution.executable?.standardizedFileURL, bundledNode.standardizedFileURL)
  }

  func testResolveNodeFallsBackToSystemSearchPaths() throws {
    let root = temporaryRoot()
    addTeardownBlock { try? FileManager.default.removeItem(at: root) }
    let systemBin = root.appendingPathComponent("system-bin", isDirectory: true)
    let systemNode = systemBin.appendingPathComponent("node")
    try writeExecutable(at: systemNode)

    let resolution = WikiEngineRendererConfig.resolveNode(
      environment: [:],
      resourceURL: root.appendingPathComponent("Resources", isDirectory: true),
      executableURL: nil,
      systemSearchPaths: [root.appendingPathComponent("empty-bin").path, systemBin.path]
    )

    XCTAssertEqual(resolution.source, .system)
    XCTAssertEqual(resolution.executable?.standardizedFileURL, systemNode.standardizedFileURL)
  }

  func testResolveNodeReportsMissingRuntime() throws {
    let root = temporaryRoot()
    addTeardownBlock { try? FileManager.default.removeItem(at: root) }

    let resolution = WikiEngineRendererConfig.resolveNode(
      environment: [:],
      resourceURL: root.appendingPathComponent("Resources", isDirectory: true),
      executableURL: root.appendingPathComponent("Contents/MacOS/1contextd"),
      systemSearchPaths: []
    )

    XCTAssertEqual(resolution.source, .missing)
    XCTAssertNil(resolution.executable)
  }

  func testRenderThrowsTypedErrorWhenNodeRuntimeIsMissing() throws {
    let root = temporaryRoot()
    addTeardownBlock { try? FileManager.default.removeItem(at: root) }
    let engine = root.appendingPathComponent("wiki-engine", isDirectory: true)
    try writeExecutable(at: engine.appendingPathComponent("tools/render-site.mjs"))
    let paths = RuntimePaths(
      userContentDirectory: root.appendingPathComponent("1Context", isDirectory: true),
      appSupportDirectory: root.appendingPathComponent("Application Support/1Context", isDirectory: true),
      logDirectory: root.appendingPathComponent("Logs/1Context", isDirectory: true),
      cacheDirectory: root.appendingPathComponent("Caches/1Context", isDirectory: true)
    )
    try FileManager.default.createDirectory(
      at: paths.userWikiSourceDirectory,
      withIntermediateDirectories: true
    )

    let renderer = WikiEngineRenderer(config: WikiEngineRendererConfig(
      node: WikiEngineNodeResolution(executable: nil, source: .missing),
      engineDirectory: engine,
      renderTool: engine.appendingPathComponent("tools/render-site.mjs")
    ))

    XCTAssertThrowsError(try renderer.render(
      runtimePaths: paths,
      outputDirectory: root.appendingPathComponent("staging/site", isDirectory: true)
    )) { error in
      XCTAssertEqual(error as? WikiEngineRendererError, .missingNodeRuntime)
      XCTAssertTrue(
        (error as? WikiEngineRendererError)?.errorDescription?.contains("Node.js runtime is missing") == true
      )
    }
  }

  private func temporaryRoot() -> URL {
    let url = FileManager.default.temporaryDirectory
      .appendingPathComponent("1context-wiki-engine-config-\(UUID().uuidString)", isDirectory: true)
    try? FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
    return url
  }

  private func writeExecutable(at url: URL) throws {
    try FileManager.default.createDirectory(
      at: url.deletingLastPathComponent(),
      withIntermediateDirectories: true
    )
    XCTAssertTrue(FileManager.default.createFile(
      atPath: url.path,
      contents: Data("#!/bin/sh\nexit 0\n".utf8),
      attributes: [.posixPermissions: 0o755]
    ))
  }
}
