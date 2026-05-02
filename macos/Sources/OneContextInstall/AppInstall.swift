import Foundation
import CryptoKit
import OneContextCore

public enum AppInstallEnvironment {
  public static let skipPromptKey = "ONECONTEXT_SKIP_APP_INSTALL_PROMPT"
  public static let destinationKey = "ONECONTEXT_APP_INSTALL_DESTINATION"
}

public enum ExistingInstallRelation: String, Sendable {
  case none
  case sameVersion
  case olderVersion
  case newerVersion
  case unknownVersion
}

public struct AppInstallRequest: Sendable, Equatable {
  public let currentBundleURL: URL
  public let destinationBundleURL: URL
  public let currentVersion: String
  public let existingVersion: String?
  public let currentBundleIdentifier: String?
  public let existingBundleIdentifier: String?
  public let currentProxyExecutableSHA256: String?
  public let existingProxyExecutableSHA256: String?
  public let existingRelation: ExistingInstallRelation

  public var existingInstallMatchesCurrent: Bool {
    guard existingRelation == .sameVersion else { return false }
    guard let currentBundleIdentifier,
      currentBundleIdentifier == existingBundleIdentifier
    else {
      return false
    }
    guard let currentProxyExecutableSHA256,
      currentProxyExecutableSHA256 == existingProxyExecutableSHA256
    else {
      return false
    }
    return true
  }

  public init(
    currentBundleURL: URL,
    destinationBundleURL: URL,
    currentVersion: String,
    existingVersion: String?,
    currentBundleIdentifier: String? = nil,
    existingBundleIdentifier: String? = nil,
    currentProxyExecutableSHA256: String? = nil,
    existingProxyExecutableSHA256: String? = nil,
    existingRelation: ExistingInstallRelation
  ) {
    self.currentBundleURL = currentBundleURL
    self.destinationBundleURL = destinationBundleURL
    self.currentVersion = currentVersion
    self.existingVersion = existingVersion
    self.currentBundleIdentifier = currentBundleIdentifier
    self.existingBundleIdentifier = existingBundleIdentifier
    self.currentProxyExecutableSHA256 = currentProxyExecutableSHA256
    self.existingProxyExecutableSHA256 = existingProxyExecutableSHA256
    self.existingRelation = existingRelation
  }
}

public enum AppInstallRecommendation: Sendable, Equatable {
  case continueInPlace(String)
  case moveToApplications(AppInstallRequest)
}

public struct AppInstallPlanner {
  public let environment: [String: String]
  public let fileManager: FileManager

  public init(
    environment: [String: String] = ProcessInfo.processInfo.environment,
    fileManager: FileManager = .default
  ) {
    self.environment = environment
    self.fileManager = fileManager
  }

  public func recommendation(
    currentBundleURL: URL,
    currentVersion: String,
    existingVersionReader: (URL) -> String? = AppInstallPlanner.bundleVersion
  ) -> AppInstallRecommendation {
    if environment[AppInstallEnvironment.skipPromptKey] == "1" {
      return .continueInPlace("Install prompt disabled by environment.")
    }

    let destination = Self.destinationBundleURL(environment: environment)
    if sameFileSystemLocation(currentBundleURL, destination) {
      return .continueInPlace("1Context is already running from Applications.")
    }

    let existingVersion = fileManager.fileExists(atPath: destination.path)
      ? existingVersionReader(destination)
      : nil
    let relation = existingRelation(currentVersion: currentVersion, existingVersion: existingVersion)
    return .moveToApplications(AppInstallRequest(
      currentBundleURL: currentBundleURL,
      destinationBundleURL: destination,
      currentVersion: currentVersion,
      existingVersion: existingVersion,
      currentBundleIdentifier: Self.bundleIdentifier(at: currentBundleURL),
      existingBundleIdentifier: fileManager.fileExists(atPath: destination.path)
        ? Self.bundleIdentifier(at: destination)
        : nil,
      currentProxyExecutableSHA256: Self.proxyExecutableSHA256(at: currentBundleURL),
      existingProxyExecutableSHA256: fileManager.fileExists(atPath: destination.path)
        ? Self.proxyExecutableSHA256(at: destination)
        : nil,
      existingRelation: relation
    ))
  }

  public static func bundleIdentifier(at bundleURL: URL) -> String? {
    infoPlist(at: bundleURL)?["CFBundleIdentifier"] as? String
  }

  public static func bundleVersion(at bundleURL: URL) -> String? {
    infoPlist(at: bundleURL)?["CFBundleShortVersionString"] as? String
  }

  public static func proxyExecutableSHA256(at bundleURL: URL) -> String? {
    let proxy = bundleURL
      .appendingPathComponent("Contents/Resources/1context-local-web-proxy")
    guard FileManager.default.fileExists(atPath: proxy.path),
      let data = try? Data(contentsOf: proxy)
    else {
      return nil
    }
    return SHA256Digest.hex(data)
  }

  private static func infoPlist(at bundleURL: URL) -> [String: Any]? {
    let infoPlist = bundleURL.appendingPathComponent("Contents/Info.plist")
    guard let data = try? Data(contentsOf: infoPlist),
      let plist = try? PropertyListSerialization.propertyList(from: data, options: [], format: nil) as? [String: Any]
    else {
      return nil
    }
    return plist
  }

  public static func destinationBundleURL(
    environment: [String: String] = ProcessInfo.processInfo.environment
  ) -> URL {
    if let path = environment[AppInstallEnvironment.destinationKey], !path.isEmpty {
      return URL(fileURLWithPath: path, isDirectory: true)
    }
    return URL(fileURLWithPath: "/Applications/1Context.app", isDirectory: true)
  }

  private func existingRelation(
    currentVersion: String,
    existingVersion: String?
  ) -> ExistingInstallRelation {
    guard let existingVersion, !existingVersion.isEmpty else {
      return fileManager.fileExists(atPath: Self.destinationBundleURL(environment: environment).path)
        ? .unknownVersion
        : .none
    }
    let comparison = compareVersions(existingVersion, currentVersion)
    if comparison == 0 { return .sameVersion }
    return comparison < 0 ? .olderVersion : .newerVersion
  }

  private func sameFileSystemLocation(_ lhs: URL, _ rhs: URL) -> Bool {
    let left = lhs.standardizedFileURL.resolvingSymlinksInPath().path
    let right = rhs.standardizedFileURL.resolvingSymlinksInPath().path
    return left == right
  }
}

private enum SHA256Digest {
  static func hex(_ data: Data) -> String {
    #if canImport(CryptoKit)
    return CryptoKit.SHA256.hash(data: data).map { String(format: "%02X", $0) }.joined()
    #else
    return "\(data.count)"
    #endif
  }
}

public enum AppInstallMoveError: Error, LocalizedError, Sendable {
  case sourceMissing(String)
  case couldNotCreateDestinationParent(String)
  case copyFailed(String)
  case relaunchFailed(String)

  public var errorDescription: String? {
    switch self {
    case .sourceMissing(let path):
      return "Could not find the app at \(path)."
    case .couldNotCreateDestinationParent(let path):
      return "Could not prepare \(path)."
    case .copyFailed(let message):
      return message
    case .relaunchFailed(let message):
      return message
    }
  }
}

public struct AppInstallMover {
  public let fileManager: FileManager

  public init(fileManager: FileManager = .default) {
    self.fileManager = fileManager
  }

  public func install(_ request: AppInstallRequest) throws {
    let source = request.currentBundleURL
    let destination = request.destinationBundleURL
    guard fileManager.fileExists(atPath: source.path) else {
      throw AppInstallMoveError.sourceMissing(source.path)
    }

    let parent = destination.deletingLastPathComponent()
    do {
      try fileManager.createDirectory(at: parent, withIntermediateDirectories: true)
    } catch {
      throw AppInstallMoveError.couldNotCreateDestinationParent(parent.path)
    }

    let backup = parent.appendingPathComponent(".1Context.previous-\(UUID().uuidString).app")
    var movedExisting = false
    do {
      if fileManager.fileExists(atPath: destination.path) {
        try fileManager.moveItem(at: destination, to: backup)
        movedExisting = true
      }
      try copyBundle(from: source, to: destination)
      if movedExisting {
        try? fileManager.removeItem(at: backup)
      }
    } catch {
      if fileManager.fileExists(atPath: destination.path) {
        try? fileManager.removeItem(at: destination)
      }
      if movedExisting, fileManager.fileExists(atPath: backup.path) {
        try? fileManager.moveItem(at: backup, to: destination)
      }
      throw AppInstallMoveError.copyFailed("Could not move 1Context to Applications. \(error.localizedDescription)")
    }
  }

  public func relaunch(destinationBundleURL: URL) throws {
    let process = Process()
    process.executableURL = URL(fileURLWithPath: "/usr/bin/open")
    process.arguments = ["-na", destinationBundleURL.path]
    do {
      try process.run()
      process.waitUntilExit()
    } catch {
      throw AppInstallMoveError.relaunchFailed("Could not relaunch 1Context from Applications. \(error.localizedDescription)")
    }
    guard process.terminationStatus == 0 else {
      throw AppInstallMoveError.relaunchFailed("Could not relaunch 1Context from Applications.")
    }
  }

  private func copyBundle(from source: URL, to destination: URL) throws {
    let process = Process()
    process.executableURL = URL(fileURLWithPath: "/usr/bin/ditto")
    process.arguments = [
      "--norsrc",
      "--noqtn",
      source.path,
      destination.path
    ]
    try process.run()
    process.waitUntilExit()
    guard process.terminationStatus == 0 else {
      throw AppInstallMoveError.copyFailed("ditto exited with status \(process.terminationStatus).")
    }
  }
}

public enum AppBundleTrashEnvironment {
  public static let allowNonApplicationsKey = "ONECONTEXT_ALLOW_NON_APPLICATIONS_APP_TRASH"
  public static let trashDestinationKey = "ONECONTEXT_APP_TRASH_DESTINATION"
}

public enum AppBundleTrashError: Error, LocalizedError, Sendable, Equatable {
  case notAppBundle(String)
  case wrongBundleIdentifier(String?)
  case unsafeApplicationPath(String)

  public var errorDescription: String? {
    switch self {
    case .notAppBundle(let path):
      return "\(path) is not an app bundle."
    case .wrongBundleIdentifier(let identifier):
      return "Refusing to remove app bundle with identifier \(identifier ?? "missing")."
    case .unsafeApplicationPath(let path):
      return "Refusing to remove app outside /Applications: \(path)."
    }
  }
}

public struct AppBundleTrasher {
  public let environment: [String: String]
  public let fileManager: FileManager

  public init(
    environment: [String: String] = ProcessInfo.processInfo.environment,
    fileManager: FileManager = .default
  ) {
    self.environment = environment
    self.fileManager = fileManager
  }

  @discardableResult
  public func trash(_ bundleURL: URL) throws -> URL? {
    let bundle = bundleURL.standardizedFileURL.resolvingSymlinksInPath()
    guard fileManager.fileExists(atPath: bundle.path) else { return nil }
    try validate(bundle)

    if let destination = environment[AppBundleTrashEnvironment.trashDestinationKey], !destination.isEmpty {
      return try moveToTrashDirectory(bundle, destination: URL(fileURLWithPath: destination, isDirectory: true))
    }

    var resultingURL: NSURL?
    try fileManager.trashItem(at: bundle, resultingItemURL: &resultingURL)
    return resultingURL as URL?
  }

  private func validate(_ bundle: URL) throws {
    guard bundle.pathExtension == "app" else {
      throw AppBundleTrashError.notAppBundle(bundle.path)
    }
    guard AppInstallPlanner.bundleIdentifier(at: bundle) == "com.haptica.1context" else {
      throw AppBundleTrashError.wrongBundleIdentifier(AppInstallPlanner.bundleIdentifier(at: bundle))
    }
    guard environment[AppBundleTrashEnvironment.allowNonApplicationsKey] == "1"
      || bundle.deletingLastPathComponent().path == "/Applications"
    else {
      throw AppBundleTrashError.unsafeApplicationPath(bundle.path)
    }
  }

  private func moveToTrashDirectory(_ bundle: URL, destination: URL) throws -> URL {
    try fileManager.createDirectory(at: destination, withIntermediateDirectories: true)
    let target = availableTrashURL(for: bundle.lastPathComponent, in: destination)
    try fileManager.moveItem(at: bundle, to: target)
    return target
  }

  private func availableTrashURL(for lastPathComponent: String, in directory: URL) -> URL {
    let baseName = (lastPathComponent as NSString).deletingPathExtension
    let pathExtension = (lastPathComponent as NSString).pathExtension
    var candidate = directory.appendingPathComponent(lastPathComponent, isDirectory: true)
    var suffix = 2
    while fileManager.fileExists(atPath: candidate.path) {
      let name = pathExtension.isEmpty ? "\(baseName) \(suffix)" : "\(baseName) \(suffix).\(pathExtension)"
      candidate = directory.appendingPathComponent(name, isDirectory: true)
      suffix += 1
    }
    return candidate
  }
}
