import CryptoKit
import Foundation
import XCTest
@testable import OneContextWikiRuntime
import OneContextPlatform

final class WikiInventoryTests: XCTestCase {
  func testPageLedgerAppendsAndReadsEvents() throws {
    let root = temporaryRoot()
    let paths = testRuntimePaths(root: root)
    let ledger = WikiPageLedger(runtimePaths: paths)
    let event = WikiPageLedgerEvent(
      event: "page.created",
      page: "topics",
      at: "2026-05-19T08:00:00Z",
      actor: WikiPageLedgerActor(kind: "agent", name: "unit-test"),
      origin: "created_from_template"
    )

    try ledger.append(event)

    XCTAssertEqual(try ledger.readEvents(), [event])
    XCTAssertTrue(FileManager.default.fileExists(atPath: paths.userWikiDirectory.appendingPathComponent(".1context/page-ledger.jsonl").path))
  }

  func testInventoryReportsTemplateUneditedRenderedPage() throws {
    let root = temporaryRoot()
    let paths = testRuntimePaths(root: root)
    try writeTopicsFixture(paths: paths, body: "# Topics\n")
    let source = topicsSource(paths: paths)
    let sourceHash = try sha256(url: source)
    try WikiPageLedger(runtimePaths: paths).append(WikiPageLedgerEvent(
      event: "page.created",
      page: "topics",
      at: "2026-05-19T08:00:00Z",
      actor: WikiPageLedgerActor(kind: "system", name: "runtime-defaults"),
      origin: "created_from_template"
    ))
    try WikiPageLedger(runtimePaths: paths).append(WikiPageLedgerEvent(
      event: "template.baseline",
      page: "topics",
      at: "2026-05-19T08:00:00Z",
      sourceSHA256: sourceHash,
      templateSHA256: "template-hash"
    ))
    try writeRenderedSite(paths: paths, sourceFingerprint: try WikiRenderFingerprint.compute(root: paths.userWikiSourceDirectory))

    let inventory = try WikiInventoryCompiler(runtimePaths: paths).compile()
    let page = try XCTUnwrap(inventory.pages.first)

    XCTAssertEqual(page.id, "topics")
    XCTAssertEqual(page.state, .rendered)
    XCTAssertEqual(page.contentState, .templateUnedited)
    XCTAssertEqual(page.origin, "created_from_template")
    XCTAssertEqual(page.templateState, "template_unedited")
    XCTAssertTrue(page.flags.templateDerived)
    XCTAssertFalse(page.flags.runtimeDefault)
    XCTAssertTrue(page.flags.customCreated)
    XCTAssertFalse(page.flags.userEdited)
    XCTAssertTrue(page.flags.talkReady)
    XCTAssertEqual(page.nextAction, "open")
    XCTAssertEqual(page.handles.source, "user-wiki://page/topics/source")
  }

  func testInventoryReportsEditedPageNeedingPublish() throws {
    let root = temporaryRoot()
    let paths = testRuntimePaths(root: root)
    try writeTopicsFixture(paths: paths, body: "# Topics\n")
    let originalHash = try sha256(url: topicsSource(paths: paths))
    try WikiPageLedger(runtimePaths: paths).append(WikiPageLedgerEvent(
      event: "page.created",
      page: "topics",
      at: "2026-05-19T08:00:00Z",
      origin: "created_from_template"
    ))
    try WikiPageLedger(runtimePaths: paths).append(WikiPageLedgerEvent(
      event: "template.baseline",
      page: "topics",
      at: "2026-05-19T08:00:00Z",
      sourceSHA256: originalHash
    ))
    try writeRenderedSite(paths: paths, sourceFingerprint: try WikiRenderFingerprint.compute(root: paths.userWikiSourceDirectory))
    try writeString("---\ntitle: Topics\nslug: topics\n---\n# Topics\nEdited.\n", to: topicsSource(paths: paths))

    let page = try XCTUnwrap(try WikiInventoryCompiler(runtimePaths: paths).compile().pages.first)

    XCTAssertEqual(page.state, .needsPublish)
    XCTAssertEqual(page.contentState, .edited)
    XCTAssertEqual(page.templateState, "edited_from_template")
    XCTAssertTrue(page.flags.stale)
    XCTAssertTrue(page.flags.userEdited)
    XCTAssertEqual(page.nextAction, "publish")
  }

  func testInventoryReportsConfiguredPageMissingSource() throws {
    let root = temporaryRoot()
    let paths = testRuntimePaths(root: root)
    try writeWikiConfig(paths: paths)

    let page = try XCTUnwrap(try WikiInventoryCompiler(runtimePaths: paths).compile().pages.first)

    XCTAssertEqual(page.state, .sourceMissing)
    XCTAssertEqual(page.contentState, .missingSource)
    XCTAssertEqual(page.nextAction, "create")
    XCTAssertEqual(page.allowedActions, ["create", "validate"])
    XCTAssertEqual(page.validation.highestSeverity, "error")
  }

  func testInventoryCanReportRuntimeDefaultOriginFromWikiConfig() throws {
    let root = temporaryRoot()
    let paths = testRuntimePaths(root: root)
    try writeWikiConfig(paths: paths, origin: "runtime_default")

    let page = try XCTUnwrap(try WikiInventoryCompiler(runtimePaths: paths).compile().pages.first)

    XCTAssertEqual(page.origin, "runtime_default")
    XCTAssertTrue(page.flags.templateDerived)
    XCTAssertTrue(page.flags.runtimeDefault)
    XCTAssertFalse(page.flags.customCreated)
  }

  private func temporaryRoot() -> URL {
    URL(fileURLWithPath: NSTemporaryDirectory())
      .appendingPathComponent("1context-wiki-inventory-tests-\(UUID().uuidString)", isDirectory: true)
  }

  private func testRuntimePaths(root: URL) -> RuntimePaths {
    RuntimePaths(
      userContentDirectory: root.appendingPathComponent("1Context", isDirectory: true),
      appSupportDirectory: root.appendingPathComponent("Application Support/1Context", isDirectory: true),
      logDirectory: root.appendingPathComponent("Logs/1Context", isDirectory: true),
      cacheDirectory: root.appendingPathComponent("Caches/1Context", isDirectory: true)
    )
  }

  private func writeTopicsFixture(paths: RuntimePaths, body: String) throws {
    try writeWikiConfig(paths: paths)
    try writeString("---\ntitle: Topics\nslug: topics\n---\n\(body)", to: topicsSource(paths: paths))
    let talk = paths.userWikiSourceDirectory
      .appendingPathComponent("families/reference/topics/talk/topics.talk", isDirectory: true)
    try writeString("title: Topics Talk\n", to: talk.appendingPathComponent("_meta.yaml"))
    try writeString("# Conventions\n", to: talk.appendingPathComponent("_conventions.md"))
    try writeString("# Curator\n", to: talk.appendingPathComponent("_curator.md"))
  }

  private func writeWikiConfig(paths: RuntimePaths, origin: String? = nil) throws {
    let originLine = origin.map { "origin = \"\($0)\"\n" } ?? ""
    try writeString(
      """
      [[pages]]
      id = "topics"
      enabled = true
      title = "Topics"
      slug = "topics"
      route = "/topics"
      family_group = "reference"
      family_id = "topics"
      type = "index"
      \(originLine)
      """,
      to: paths.userWikiDirectory.appendingPathComponent("wiki.toml")
    )
  }

  private func topicsSource(paths: RuntimePaths) -> URL {
    paths.userWikiSourceDirectory
      .appendingPathComponent("families/reference/topics/source", isDirectory: true)
      .appendingPathComponent("topics.md")
  }

  private func writeRenderedSite(paths: RuntimePaths, sourceFingerprint: String) throws {
    try writeString("<!doctype html><h1>Topics</h1>", to: paths.userWikiSiteDirectory.appendingPathComponent("topics.html"))
    try writeString(
      sourceFingerprint + "\n",
      to: paths.userWikiSiteDirectory.appendingPathComponent(".1context/source-fingerprint.txt")
    )
  }

  private func writeString(_ value: String, to url: URL) throws {
    try RuntimePermissions.ensurePrivateDirectory(url.deletingLastPathComponent())
    try RuntimePermissions.writePrivateData(Data(value.utf8), to: url)
  }

  private func sha256(url: URL) throws -> String {
    SHA256.hash(data: try Data(contentsOf: url)).map { String(format: "%02x", $0) }.joined()
  }
}
