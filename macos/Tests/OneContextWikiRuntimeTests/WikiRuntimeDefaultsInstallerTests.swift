import Foundation
import Testing

@testable import OneContextPlatform
@testable import OneContextWikiRuntime

@Suite("Wiki runtime defaults installer")
struct WikiRuntimeDefaultsInstallerTests {
  @Test("copies missing packaged defaults and preserves user edits")
  func copiesMissingDefaultsAndPreservesExistingFiles() throws {
    let root = try temporaryDirectory()
    defer { try? FileManager.default.removeItem(at: root) }

    let defaults = root.appendingPathComponent("defaults", isDirectory: true)
    let defaultOneContext = defaults.appendingPathComponent("1Context", isDirectory: true)
    try FileManager.default.createDirectory(
      at: defaultOneContext.appendingPathComponent("user-wiki/source", isDirectory: true),
      withIntermediateDirectories: true
    )
    try FileManager.default.createDirectory(
      at: defaultOneContext.appendingPathComponent("user-wiki/site", isDirectory: true),
      withIntermediateDirectories: true
    )
    try FileManager.default.createDirectory(
      at: defaultOneContext.appendingPathComponent("context-engine/prompts", isDirectory: true),
      withIntermediateDirectories: true
    )
    try "# Default Wiki\n".write(
      to: defaultOneContext.appendingPathComponent("user-wiki/wiki.toml"),
      atomically: true,
      encoding: .utf8
    )
    try "<!doctype html><title>Default</title>\n".write(
      to: defaultOneContext.appendingPathComponent("user-wiki/site/index.html"),
      atomically: true,
      encoding: .utf8
    )
    try "# Prompt\n".write(
      to: defaultOneContext.appendingPathComponent("context-engine/prompts/agent.md"),
      atomically: true,
      encoding: .utf8
    )

    let paths = RuntimePaths(
      userContentDirectory: root.appendingPathComponent("user", isDirectory: true),
      appSupportDirectory: root.appendingPathComponent("Application Support/1Context", isDirectory: true),
      logDirectory: root.appendingPathComponent("Logs/1Context", isDirectory: true),
      cacheDirectory: root.appendingPathComponent("Caches/1Context", isDirectory: true)
    )
    try RuntimePermissions.ensurePrivateDirectory(paths.userWikiDirectory)
    try "user edit\n".write(
      to: paths.userWikiDirectory.appendingPathComponent("wiki.toml"),
      atomically: true,
      encoding: .utf8
    )

    let installer = WikiRuntimeDefaultsInstaller(
      runtimePaths: paths,
      defaultsRoot: defaults,
      now: { Date(timeIntervalSince1970: 1_800_000_000) }
    )
    let result = try installer.installMissingDefaults()

    #expect(result.status == "installed_with_conflicts")
    #expect(result.source == "app-bundle://RuntimeDefaults/1Context")
    #expect(result.preserved.contains("user-wiki/wiki.toml"))
    #expect(result.copied.contains("user-wiki/site/index.html"))
    #expect(result.copied.contains("context-engine/prompts/agent.md"))
    #expect(result.proposals.contains("1Context/context-engine/proposals/wiki/runtime-defaults/user-wiki__wiki.toml.proposal.json"))
    #expect(
      try String(contentsOf: paths.userWikiDirectory.appendingPathComponent("wiki.toml"), encoding: .utf8)
        == "user edit\n"
    )
    #expect(FileManager.default.fileExists(atPath: paths.userWikiSiteDirectory.appendingPathComponent("index.html").path))

    let ledger = paths.appSupportSetupDirectory.appendingPathComponent("runtime-defaults-install.json")
    #expect(FileManager.default.fileExists(atPath: ledger.path))
    let ledgerText = try String(contentsOf: ledger, encoding: .utf8)
    #expect(!ledgerText.contains(root.path))
    let ledgerResult = try JSONDecoder().decode(WikiRuntimeDefaultsInstallResult.self, from: Data(ledgerText.utf8))
    #expect(ledgerResult.copied.contains("user-wiki/site/index.html"))
    #expect(ledgerResult.proposals == result.proposals)
    let proposal = paths.userContentDirectory.appendingPathComponent("context-engine/proposals/wiki/runtime-defaults/user-wiki__wiki.toml.proposal.json")
    #expect(FileManager.default.fileExists(atPath: proposal.path))
    let proposalText = try String(contentsOf: proposal, encoding: .utf8)
    #expect(!proposalText.contains(root.path))
    #expect(proposalText.contains("Packaged runtime default differs"))
  }

  @Test("records a missing-defaults ledger without failing setup")
  func recordsMissingDefaultsWithoutFailingSetup() throws {
    let root = try temporaryDirectory()
    defer { try? FileManager.default.removeItem(at: root) }

    let paths = RuntimePaths(
      userContentDirectory: root.appendingPathComponent("user", isDirectory: true),
      appSupportDirectory: root.appendingPathComponent("Application Support/1Context", isDirectory: true),
      logDirectory: root.appendingPathComponent("Logs/1Context", isDirectory: true),
      cacheDirectory: root.appendingPathComponent("Caches/1Context", isDirectory: true)
    )

    let result = try WikiRuntimeDefaultsInstaller(
      runtimePaths: paths,
      defaultsRoot: root.appendingPathComponent("missing", isDirectory: true)
    ).installMissingDefaults()

    #expect(result.status == "missing_defaults")
    let ledger = paths.appSupportSetupDirectory.appendingPathComponent("runtime-defaults-install.json")
    #expect(FileManager.default.fileExists(atPath: ledger.path))
  }

  private func temporaryDirectory() throws -> URL {
    let url = FileManager.default.temporaryDirectory
      .appendingPathComponent("1context-runtime-defaults-\(UUID().uuidString)", isDirectory: true)
    try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
    return url
  }
}
