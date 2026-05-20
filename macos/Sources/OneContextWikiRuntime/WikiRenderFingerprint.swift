import CryptoKit
import Foundation

public enum WikiRenderFingerprint {
  public static func compute(root: URL, fileManager: FileManager = .default) throws -> String {
    var isDirectory: ObjCBool = false
    guard fileManager.fileExists(atPath: root.path, isDirectory: &isDirectory), isDirectory.boolValue else {
      throw WikiEngineRendererError.missingSourceRoot(root.path)
    }

    guard let enumerator = fileManager.enumerator(
      at: root,
      includingPropertiesForKeys: [.isRegularFileKey],
      options: [.skipsHiddenFiles]
    ) else {
      return sha256Hex(Data())
    }

    let userWikiRoot = root.deletingLastPathComponent()
    let rootPath = userWikiRoot.standardizedFileURL.path
    var files: [URL] = []
    let wikiConfig = userWikiRoot.appendingPathComponent("wiki.toml")
    if fileManager.fileExists(atPath: wikiConfig.path) {
      files.append(wikiConfig)
    }
    for case let url as URL in enumerator {
      let values = try? url.resourceValues(forKeys: [.isRegularFileKey])
      if values?.isRegularFile == true, isPublishInput(url) {
        files.append(url)
      }
    }

    var hasher = SHA256()
    for file in files.sorted(by: { $0.path < $1.path }) {
      let path = file.standardizedFileURL.path
      let relativePath: String
      if path.hasPrefix(rootPath + "/") {
        relativePath = String(path.dropFirst(rootPath.count + 1))
      } else {
        relativePath = file.lastPathComponent
      }
      hasher.update(data: Data(relativePath.utf8))
      hasher.update(data: Data([0]))
      hasher.update(data: try Data(contentsOf: file))
      hasher.update(data: Data([0]))
    }
    return hasher.finalize().map { String(format: "%02x", $0) }.joined()
  }

  private static func sha256Hex(_ data: Data) -> String {
    SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined()
  }

  private static func isPublishInput(_ url: URL) -> Bool {
    guard url.deletingLastPathComponent().lastPathComponent == "source" else { return false }
    if url.pathExtension == "md" { return true }
    return url.lastPathComponent.hasSuffix(".tombstone.toml")
  }
}
