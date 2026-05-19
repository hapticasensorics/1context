import Foundation
import XCTest
@testable import OneContextPlatform
@testable import OneContextWikiRuntime

final class WikiRuntimeDefaultsScenarioTests: XCTestCase {
  private struct RouteManifest: Decodable {
    struct Route: Decodable {
      var route: String
    }

    var routes: [Route]
  }

  func testRuntimeTestScenariosInstallPreserveAndRenderUserData() throws {
    guard ProcessInfo.processInfo.environment["ONECONTEXT_RUNTIME_DEFAULTS_SCENARIOS"] == "1" else {
      throw XCTSkip("Runtime defaults scenario harness is opt-in.")
    }

    let repo = repoRoot()
    let defaultsRoot = packagedDefaultsRoot(repo: repo)
    let manifest = defaultsRoot.appendingPathComponent("1Context/.1context/runtime-defaults-manifest.json")
    guard FileManager.default.fileExists(atPath: manifest.path) else {
      throw XCTSkip("Packaged RuntimeDefaults manifest is not present at \(manifest.path).")
    }

    let engine = wikiEngineRoot(repo: repo)
    guard FileManager.default.fileExists(atPath: engine.appendingPathComponent("tools/render-site.mjs").path) else {
      throw XCTSkip("Wiki engine renderer is not present at \(engine.path).")
    }

    let lab = scenarioRoot(repo: repo)
    try resetDirectory(lab)

    try proveFreshRuntimeInstall(lab: lab, defaultsRoot: defaultsRoot, engine: engine)
    try proveUserEditPreservedWithProposal(lab: lab, defaultsRoot: defaultsRoot, engine: engine)
    try proveCustomPageMaterializesAndRenders(lab: lab, defaultsRoot: defaultsRoot, engine: engine, repo: repo)
  }

  private func proveFreshRuntimeInstall(lab: URL, defaultsRoot: URL, engine: URL) throws {
    let home = lab.appendingPathComponent("fresh-user", isDirectory: true)
    let paths = runtimePaths(home: home)

    let install = try installDefaults(paths: paths, defaultsRoot: defaultsRoot)
    XCTAssertEqual(install.status, "installed")
    XCTAssertEqual(install.source, "app-bundle://RuntimeDefaults/1Context")
    XCTAssertNotNil(install.packagedManifest?.releaseVersion)
    XCTAssertNotNil(install.packagedManifest?.gitCommit)
    XCTAssertNotNil(install.packagedManifest?.runtimeDefaultsSourceHash)
    XCTAssertNotNil(install.packagedManifest?.materializerHash)
    XCTAssertNotNil(install.packagedManifest?.rendererHash)
    XCTAssertEqual(install.packagedManifest?.renderStatus, "published")

    let render = render(paths: paths, engine: engine, trigger: "runtime-test.fresh")
    XCTAssertEqual(render.status, .published)
    assertRoutes(paths: paths, include: ["/for-you", "/for-you/talk", "/topics", "/topics/talk"])
    assertLedger(paths: paths, expectedStatus: "installed")
  }

  private func proveUserEditPreservedWithProposal(lab: URL, defaultsRoot: URL, engine: URL) throws {
    let home = lab.appendingPathComponent("preserve-user-edit", isDirectory: true)
    let paths = runtimePaths(home: home)
    let customWikiConfig = """
      schema_version = 1
      title = "User Edited 1Context"
      source_dir = "source"
      site_dir = "site"
      templates_dir = "templates"
      assets_dir = "assets"

      [site]
      home_route = "/"
      missing_route_behavior = "diagnose"
      navigation = []

      [defaults]
      operator_name = "Runtime Test Operator"
      access_tier = "private"
      asset_base = "."
      home_href = "/"
      template_pack = "runtime-test"

      [materialization]
      enabled = true
      create_talk = true
      overwrite_user_files = false
      """
    try writeString(customWikiConfig, to: paths.userWikiDirectory.appendingPathComponent("wiki.toml"))

    let install = try installDefaults(paths: paths, defaultsRoot: defaultsRoot)
    XCTAssertEqual(install.status, "installed_with_conflicts")
    XCTAssertTrue(install.preserved.contains("user-wiki/wiki.toml"))
    XCTAssertTrue(
      install.proposals.contains("1Context/context-engine/proposals/wiki/runtime-defaults/user-wiki__wiki.toml.proposal.json")
    )
    XCTAssertEqual(
      try String(contentsOf: paths.userWikiDirectory.appendingPathComponent("wiki.toml"), encoding: .utf8),
      customWikiConfig
    )

    let render = render(paths: paths, engine: engine, trigger: "runtime-test.preserve")
    XCTAssertEqual(render.status, .published)
    assertRoutes(paths: paths, include: ["/for-you", "/your-context", "/projects", "/topics"])
    assertLedger(paths: paths, expectedStatus: "installed_with_conflicts")
  }

  private func proveCustomPageMaterializesAndRenders(lab: URL, defaultsRoot: URL, engine: URL, repo: URL) throws {
    let home = lab.appendingPathComponent("custom-page", isDirectory: true)
    let paths = runtimePaths(home: home)

    let install = try installDefaults(paths: paths, defaultsRoot: defaultsRoot)
    XCTAssertEqual(install.status, "installed")

    let wikiConfigURL = paths.userWikiDirectory.appendingPathComponent("wiki.toml")
    var wikiConfig = try String(contentsOf: wikiConfigURL, encoding: .utf8)
    wikiConfig += """

      [[pages]]
      id = "dummy-custom"
      enabled = true
      title = "Dummy Custom"
      slug = "dummy-custom"
      route = "/dummy-custom"
      family_group = "custom"
      family_group_title = "Custom"
      family_id = "dummy-custom"
      family_title = "Dummy Custom"
      type = "context-page"
      template = "pages/context-page.md"
      talk_conventions_template = "talk/conventions.md"
      summary = "Runtime-test custom page generated from the fallback template."
      nav_order = 900
      """
    try writeString(wikiConfig, to: wikiConfigURL)

    let materializer = repo.appendingPathComponent("wiki-engine/tools/materialize-wiki-pages.py")
    _ = try run(
      executable: "/usr/bin/env",
      arguments: ["python3", materializer.path, home.path],
      currentDirectory: repo
    )

    let source = paths.userWikiSourceDirectory
      .appendingPathComponent("families/custom/dummy-custom/source/dummy-custom.md")
    let talkMeta = paths.userWikiSourceDirectory
      .appendingPathComponent("families/custom/dummy-custom/talk/dummy-custom.talk/_meta.yaml")
    XCTAssertTrue(FileManager.default.fileExists(atPath: source.path))
    XCTAssertTrue(FileManager.default.fileExists(atPath: talkMeta.path))
    XCTAssertFalse(try String(contentsOf: source, encoding: .utf8).contains("{{"))
    XCTAssertFalse(try String(contentsOf: talkMeta, encoding: .utf8).contains("{{"))

    let render = render(paths: paths, engine: engine, trigger: "runtime-test.custom-page")
    XCTAssertEqual(render.status, .published)
    assertRoutes(paths: paths, include: ["/dummy-custom", "/dummy-custom/talk"])
    assertLedger(paths: paths, expectedStatus: "installed")
  }

  private func installDefaults(paths: RuntimePaths, defaultsRoot: URL) throws -> WikiRuntimeDefaultsInstallResult {
    try WikiRuntimeDefaultsInstaller(
      runtimePaths: paths,
      defaultsRoot: defaultsRoot,
      now: { Date(timeIntervalSince1970: 1_800_000_000) }
    ).installMissingDefaults()
  }

  @discardableResult
  private func render(paths: RuntimePaths, engine: URL, trigger: String) -> WikiRenderResult {
    WikiRenderCoordinator(
      runtimePaths: paths,
      rendererConfig: WikiEngineRendererConfig(
        nodeExecutable: URL(fileURLWithPath: "/usr/bin/env"),
        engineDirectory: engine,
        renderTool: engine.appendingPathComponent("tools/render-site.mjs")
      ),
      now: { Date(timeIntervalSince1970: 1_800_000_000) }
    ).renderAndPublish(trigger: trigger)
  }

  private func assertRoutes(paths: RuntimePaths, include expectedRoutes: [String]) {
    let manifestURL = paths.userWikiSiteDirectory.appendingPathComponent(".1context/route-manifest.json")
    guard
      let data = try? Data(contentsOf: manifestURL),
      let manifest = try? JSONDecoder().decode(RouteManifest.self, from: data)
    else {
      return XCTFail("Missing or invalid route manifest at \(manifestURL.path)")
    }
    let routes = Set(manifest.routes.map(\.route))
    for route in expectedRoutes {
      XCTAssertTrue(routes.contains(route), "Missing route \(route) in \(manifestURL.path)")
    }

    let currentManifestURL = paths.appSupportDirectory
      .appendingPathComponent("wiki-site/current/.1context/route-manifest.json")
    XCTAssertTrue(FileManager.default.fileExists(atPath: currentManifestURL.path))
  }

  private func assertLedger(paths: RuntimePaths, expectedStatus: String) {
    let ledgerURL = paths.appSupportSetupDirectory.appendingPathComponent("runtime-defaults-install.json")
    guard
      let data = try? Data(contentsOf: ledgerURL),
      let ledger = try? JSONDecoder().decode(WikiRuntimeDefaultsInstallResult.self, from: data)
    else {
      return XCTFail("Missing or invalid defaults install ledger at \(ledgerURL.path)")
    }
    XCTAssertEqual(ledger.status, expectedStatus)
    XCTAssertNotNil(ledger.packagedManifest?.releaseVersion)
    XCTAssertNotNil(ledger.packagedManifest?.gitCommit)
  }

  private func runtimePaths(home: URL) -> RuntimePaths {
    RuntimePaths(
      userContentDirectory: home.appendingPathComponent("1Context", isDirectory: true),
      appSupportDirectory: home.appendingPathComponent("Library/Application Support/1Context", isDirectory: true),
      logDirectory: home.appendingPathComponent("Library/Logs/1Context", isDirectory: true),
      cacheDirectory: home.appendingPathComponent("Library/Caches/1Context", isDirectory: true)
    )
  }

  private func repoRoot() -> URL {
    var candidate = URL(fileURLWithPath: #filePath).deletingLastPathComponent()
    for _ in 0..<12 {
      if FileManager.default.fileExists(atPath: candidate.appendingPathComponent("wiki-engine/tools/render-site.mjs").path) {
        return candidate
      }
      candidate.deleteLastPathComponent()
    }
    return URL(fileURLWithPath: FileManager.default.currentDirectoryPath)
  }

  private func packagedDefaultsRoot(repo: URL) -> URL {
    if let path = ProcessInfo.processInfo.environment["ONECONTEXT_RUNTIME_DEFAULTS_DIR"], !path.isEmpty {
      return URL(fileURLWithPath: path, isDirectory: true)
    }
    return repo.appendingPathComponent("dist/1Context.app/Contents/Resources/RuntimeDefaults", isDirectory: true)
  }

  private func wikiEngineRoot(repo: URL) -> URL {
    if let path = ProcessInfo.processInfo.environment["ONECONTEXT_WIKI_ENGINE_DIR"], !path.isEmpty {
      return URL(fileURLWithPath: path, isDirectory: true)
    }
    return repo.appendingPathComponent("wiki-engine", isDirectory: true)
  }

  private func scenarioRoot(repo: URL) -> URL {
    if let path = ProcessInfo.processInfo.environment["ONECONTEXT_RUNTIME_DEFAULTS_SCENARIO_ROOT"], !path.isEmpty {
      return URL(fileURLWithPath: path, isDirectory: true)
    }
    return repo.appendingPathComponent("runtime-test/wiki-runtime-defaults-scenarios", isDirectory: true)
  }

  private func resetDirectory(_ url: URL) throws {
    try? FileManager.default.removeItem(at: url)
    try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
  }

  private func writeString(_ value: String, to url: URL) throws {
    try RuntimePermissions.ensurePrivateDirectory(url.deletingLastPathComponent())
    try RuntimePermissions.writePrivateData(Data(value.utf8), to: url)
  }

  private func run(executable: String, arguments: [String], currentDirectory: URL) throws -> String {
    let process = Process()
    process.executableURL = URL(fileURLWithPath: executable)
    process.arguments = arguments
    process.currentDirectoryURL = currentDirectory
    process.standardInput = FileHandle.nullDevice

    let output = Pipe()
    let error = Pipe()
    process.standardOutput = output
    process.standardError = error
    try process.run()
    process.waitUntilExit()

    let stdout = String(data: output.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8) ?? ""
    let stderr = String(data: error.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8) ?? ""
    guard process.terminationStatus == 0 else {
      throw NSError(
        domain: "WikiRuntimeDefaultsScenarioTests",
        code: Int(process.terminationStatus),
        userInfo: [NSLocalizedDescriptionKey: [stderr, stdout].filter { !$0.isEmpty }.joined(separator: "\n")]
      )
    }
    return stdout
  }
}
