import CryptoKit
import Darwin
import Foundation
import OneContextPlatform
import ServiceManagement

public enum LocalWebSetupConstants {
  public static let proxyLabel = "com.haptica.1context.local-web-proxy"
  public static let proxyExecutableName = "1context-local-web-proxy"
  public static let proxyPlistName = "\(proxyLabel).plist"
  public static let privilegedHTTPSPort = 443
}

public struct LocalWebSetupSystemPaths: Codable, Equatable, Sendable {
  public let appBundle: String
  public let supportDirectory: String
  public let binDirectory: String
  public let proxyExecutable: String
  public let launchDaemonPlist: String
  public let trustedRootCertificate: String
  public let trustedRootSHA1: String
  public let trustedRootSHA256: String
  public let setupMarker: String
  public let logDirectory: String
  public let proxyLog: String

  public init(environment: [String: String] = ProcessInfo.processInfo.environment) {
    let userSupport = "\(NSHomeDirectory())/Library/Application Support/1Context/local-web/setup"
    let userLog = "\(NSHomeDirectory())/Library/Logs/1Context"
    let appBundle = Self.appBundleURL(environment: environment)
    let bundleLaunchDaemon = appBundle
      .appendingPathComponent("Contents/Library/LaunchDaemons/\(LocalWebSetupConstants.proxyPlistName)")
      .path
    let bundledProxy = appBundle
      .appendingPathComponent("Contents/Resources/\(LocalWebSetupConstants.proxyExecutableName)")
      .path
    let support = environment["ONECONTEXT_LOCAL_WEB_SYSTEM_SUPPORT_DIR"]
      ?? userSupport
    let log = environment["ONECONTEXT_LOCAL_WEB_SYSTEM_LOG_DIR"]
      ?? userLog
    self.appBundle = appBundle.path
    self.supportDirectory = support
    self.binDirectory = "\(support)/bin"
    self.proxyExecutable = environment["ONECONTEXT_LOCAL_WEB_PROXY_EXECUTABLE_PATH"]
      ?? bundledProxy
    self.launchDaemonPlist = environment["ONECONTEXT_LOCAL_WEB_LAUNCH_DAEMON_PATH"]
      ?? bundleLaunchDaemon
    self.trustedRootCertificate = environment["ONECONTEXT_LOCAL_WEB_TRUSTED_ROOT_CERT_PATH"]
      ?? "\(support)/local-web-root.crt"
    self.trustedRootSHA1 = environment["ONECONTEXT_LOCAL_WEB_TRUSTED_ROOT_SHA1_PATH"]
      ?? "\(support)/local-web-root.sha1"
    self.trustedRootSHA256 = environment["ONECONTEXT_LOCAL_WEB_TRUSTED_ROOT_SHA256_PATH"]
      ?? "\(support)/local-web-root.sha256"
    self.setupMarker = environment["ONECONTEXT_LOCAL_WEB_SETUP_MARKER_PATH"]
      ?? "\(support)/local-web-setup.json"
    self.logDirectory = log
    self.proxyLog = environment["ONECONTEXT_LOCAL_WEB_PROXY_LOG_PATH"]
      ?? "\(log)/local-web-proxy.log"
  }

  private static func appBundleURL(environment: [String: String]) -> URL {
    if let override = environment["ONECONTEXT_APP_BUNDLE_PATH"], !override.isEmpty {
      return URL(fileURLWithPath: override, isDirectory: true)
    }

    if Bundle.main.bundleURL.pathExtension == "app" {
      return Bundle.main.bundleURL
    }

    var candidate = URL(fileURLWithPath: CommandLine.arguments[0]).standardizedFileURL
    while candidate.path != "/" {
      if candidate.pathExtension == "app" {
        return candidate
      }
      candidate.deleteLastPathComponent()
    }

    return Bundle.main.bundleURL
  }
}

public struct CertificateFingerprints: Codable, Equatable, Sendable {
  public let sha1: String
  public let sha256: String
}

public struct LocalWebSetupState: Codable, Equatable, Sendable {
  public let label: String
  public let targetHost: String
  public let targetURL: String
  public let backendHost: String
  public let backendPort: Int
  public let privilegedPort: Int
  public let sourceProxyExecutablePath: String?
  public let sourceProxyExecutableSHA256: String?
  public let installedProxyExecutableSHA256: String?
  public let userRootCertificatePath: String
  public let userRootCertificateExists: Bool
  public let userRootCertificateSHA1: String?
  public let userRootCertificateSHA256: String?
  public let systemPaths: LocalWebSetupSystemPaths
  public let proxyPlistInstalled: Bool
  public let proxyExecutableInstalled: Bool
  public let proxyServiceStatus: String
  public let proxyLaunchDaemonLoaded: Bool
  public let proxyPortReachable: Bool
  public let trustedRootCertificateInstalled: Bool
  public let trustedRootSHA1: String?
  public let trustedRootSHA256: String?

  public init(
    label: String,
    targetHost: String,
    targetURL: String,
    backendHost: String,
    backendPort: Int,
    privilegedPort: Int,
    sourceProxyExecutablePath: String?,
    sourceProxyExecutableSHA256: String?,
    installedProxyExecutableSHA256: String?,
    userRootCertificatePath: String,
    userRootCertificateExists: Bool,
    userRootCertificateSHA1: String?,
    userRootCertificateSHA256: String?,
    systemPaths: LocalWebSetupSystemPaths,
    proxyPlistInstalled: Bool,
    proxyExecutableInstalled: Bool,
    proxyServiceStatus: String,
    proxyLaunchDaemonLoaded: Bool,
    proxyPortReachable: Bool,
    trustedRootCertificateInstalled: Bool,
    trustedRootSHA1: String?,
    trustedRootSHA256: String?
  ) {
    self.label = label
    self.targetHost = targetHost
    self.targetURL = targetURL
    self.backendHost = backendHost
    self.backendPort = backendPort
    self.privilegedPort = privilegedPort
    self.sourceProxyExecutablePath = sourceProxyExecutablePath
    self.sourceProxyExecutableSHA256 = sourceProxyExecutableSHA256
    self.installedProxyExecutableSHA256 = installedProxyExecutableSHA256
    self.userRootCertificatePath = userRootCertificatePath
    self.userRootCertificateExists = userRootCertificateExists
    self.userRootCertificateSHA1 = userRootCertificateSHA1
    self.userRootCertificateSHA256 = userRootCertificateSHA256
    self.systemPaths = systemPaths
    self.proxyPlistInstalled = proxyPlistInstalled
    self.proxyExecutableInstalled = proxyExecutableInstalled
    self.proxyServiceStatus = proxyServiceStatus
    self.proxyLaunchDaemonLoaded = proxyLaunchDaemonLoaded
    self.proxyPortReachable = proxyPortReachable
    self.trustedRootCertificateInstalled = trustedRootCertificateInstalled
    self.trustedRootSHA1 = trustedRootSHA1
    self.trustedRootSHA256 = trustedRootSHA256
  }

  public var privilegedBindReady: Bool {
    proxyPlistInstalled
      && proxyExecutableInstalled
      && proxyExecutableCurrent
      && proxyServiceStatus == "enabled"
      && proxyLaunchDaemonLoaded
      && proxyPortReachable
  }

  public var proxyExecutableCurrent: Bool {
    guard let sourceProxyExecutableSHA256,
      let installedProxyExecutableSHA256
    else {
      return true
    }
    return sourceProxyExecutableSHA256 == installedProxyExecutableSHA256
  }

  public var localCATrustReady: Bool {
    userRootCertificateExists
      && trustedRootCertificateInstalled
      && userRootCertificateSHA256 != nil
      && userRootCertificateSHA256 == trustedRootSHA256
  }
}

public struct LocalWebSetupAssets: Codable, Equatable, Sendable {
  public let proxyExecutable: String
  public let rootCertificate: String
  public let rootCertificateSHA1: String
  public let rootCertificateSHA256: String
  public let backendHost: String
  public let backendPort: Int
  public let publicHost: String
  public let publicPort: Int
}

public struct LocalWebSetupInstallResult: Codable, Equatable, Sendable {
  public let action: String
  public let setup: LocalWebSetupSnapshot
  public let localWeb: LocalWebSnapshot?
}

public enum LocalWebSetupInspector {
  public static func inspect(
    runtimePaths: RuntimePaths = .current(),
    environment: [String: String] = ProcessInfo.processInfo.environment,
    fileManager: FileManager = .default,
    host: String = LocalWebDefaults.wikiHost,
    backendHost: String = LocalWebDefaults.bindHost,
    backendPort: Int = LocalWebDefaults.wikiPort,
    targetURL: String = LocalWebDefaults.defaultWikiURL,
    sourceProxyExecutable: URL? = nil
  ) -> LocalWebSetupState {
    let paths = LocalWebPaths(runtimePaths: runtimePaths)
    let systemPaths = LocalWebSetupSystemPaths(environment: environment)
    let rootCertificate = localHTTPSRootCertificateURL(paths: paths)
    let userFingerprints = try? certificateFingerprints(at: rootCertificate)
    let trustedSHA1 = readTrimmed(systemPaths.trustedRootSHA1, fileManager: fileManager)
    let trustedSHA256 = readTrimmed(systemPaths.trustedRootSHA256, fileManager: fileManager)
    let trustedCertificateInKeychain = trustedSHA1.map {
      keychainContainsCertificate(sha1: $0, keychainPath: userKeychainPath(environment: environment, fileManager: fileManager))
    } ?? false
    let proxyServiceStatus = serviceStatus(environment: environment)
    let proxyPlistInstalled = fileManager.fileExists(atPath: systemPaths.launchDaemonPlist)
      || proxyServiceStatus != "notFound"
    let proxyExecutableInstalled = fileManager.isExecutableFile(atPath: systemPaths.proxyExecutable)
      || sourceProxyExecutable.map { fileManager.isExecutableFile(atPath: $0.path) } == true
    let sourceProxySHA256 = sourceProxyExecutable.flatMap { fileSHA256(at: $0, fileManager: fileManager) }
    let installedProxySHA256 = fileSHA256(at: URL(fileURLWithPath: systemPaths.proxyExecutable), fileManager: fileManager)
      ?? sourceProxySHA256
    let launchctlOutput = launchctlPrint(label: LocalWebSetupConstants.proxyLabel)
    let launchDaemonLoaded = environment["ONECONTEXT_LOCAL_WEB_ASSUME_PROXY_LOADED"] == "1"
      || (proxyServiceStatus == "enabled" && launchctlOutput.map(launchctlHasPID) == true)
    let portReachable = environment["ONECONTEXT_LOCAL_WEB_ASSUME_PROXY_PORT_REACHABLE"] == "1"
      || (proxyPlistInstalled && proxyExecutableInstalled
        && proxyOwnsPrivilegedPort(host: backendHost, port: LocalWebSetupConstants.privilegedHTTPSPort))

    return LocalWebSetupState(
      label: LocalWebSetupConstants.proxyLabel,
      targetHost: host,
      targetURL: targetURL,
      backendHost: backendHost,
      backendPort: backendPort,
      privilegedPort: LocalWebSetupConstants.privilegedHTTPSPort,
      sourceProxyExecutablePath: sourceProxyExecutable?.path,
      sourceProxyExecutableSHA256: sourceProxySHA256,
      installedProxyExecutableSHA256: installedProxySHA256,
      userRootCertificatePath: rootCertificate.path,
      userRootCertificateExists: fileManager.fileExists(atPath: rootCertificate.path),
      userRootCertificateSHA1: userFingerprints?.sha1,
      userRootCertificateSHA256: userFingerprints?.sha256,
      systemPaths: systemPaths,
      proxyPlistInstalled: proxyPlistInstalled,
      proxyExecutableInstalled: proxyExecutableInstalled,
      proxyServiceStatus: proxyServiceStatus,
      proxyLaunchDaemonLoaded: launchDaemonLoaded,
      proxyPortReachable: portReachable,
      trustedRootCertificateInstalled: trustedCertificateInKeychain,
      trustedRootSHA1: trustedSHA1,
      trustedRootSHA256: trustedSHA256
    )
  }

  public static func localHTTPSRootCertificateURL(paths: LocalWebPaths) -> URL {
    paths.caddyDirectory
      .appendingPathComponent("data/caddy/pki/authorities/local/root.crt")
  }

  public static func certificateFingerprints(at url: URL) throws -> CertificateFingerprints {
    let data = try Data(contentsOf: url)
    let der = try certificateDERData(fromPEMOrDER: data)
    return CertificateFingerprints(
      sha1: hex(Insecure.SHA1.hash(data: der)),
      sha256: hex(SHA256.hash(data: der))
    )
  }

  public static func fileSHA256(at url: URL, fileManager: FileManager = .default) -> String? {
    guard fileManager.fileExists(atPath: url.path),
      let data = try? Data(contentsOf: url)
    else {
      return nil
    }
    return hex(SHA256.hash(data: data))
  }

  private static func certificateDERData(fromPEMOrDER data: Data) throws -> Data {
    guard let text = String(data: data, encoding: .utf8),
      text.contains("-----BEGIN CERTIFICATE-----")
    else {
      return data
    }
    let base64 = text
      .split(separator: "\n")
      .filter { !$0.hasPrefix("-----") }
      .joined()
    guard let der = Data(base64Encoded: base64) else {
      throw LocalWebSetupInstallerError.invalidCertificate(url: "PEM data")
    }
    return der
  }

  private static func hex<D: Sequence>(_ digest: D) -> String where D.Element == UInt8 {
    digest.map { String(format: "%02X", $0) }.joined()
  }

  private static func readTrimmed(_ path: String, fileManager: FileManager) -> String? {
    guard fileManager.fileExists(atPath: path) else { return nil }
    return (try? String(contentsOfFile: path, encoding: .utf8))?
      .trimmingCharacters(in: .whitespacesAndNewlines)
      .uppercased()
  }

  private static func serviceStatus(environment: [String: String]) -> String {
    if let override = environment["ONECONTEXT_LOCAL_WEB_SERVICE_STATUS"], !override.isEmpty {
      return override
    }

    if #available(macOS 13.0, *) {
      switch SMAppService.daemon(plistName: LocalWebSetupConstants.proxyPlistName).status {
      case .notRegistered:
        return "notRegistered"
      case .enabled:
        return "enabled"
      case .requiresApproval:
        return "requiresApproval"
      case .notFound:
        return "notFound"
      @unknown default:
        return "unknown"
      }
    }

    return "notFound"
  }

  private static func launchctlPrint(label: String) -> String? {
    let result = runCapture("/bin/launchctl", ["print", "system/\(label)"])
    return result.status == 0 ? result.stdout : nil
  }

  private static func launchctlHasPID(_ output: String) -> Bool {
    output.split(separator: "\n").contains { line in
      line.trimmingCharacters(in: .whitespaces).hasPrefix("pid =")
    }
  }

  private static func userKeychainPath(environment: [String: String], fileManager: FileManager) -> String {
    if let override = environment["ONECONTEXT_LOCAL_WEB_USER_KEYCHAIN_PATH"], !override.isEmpty {
      return override
    }
    let modern = "\(NSHomeDirectory())/Library/Keychains/login.keychain-db"
    if fileManager.fileExists(atPath: modern) {
      return modern
    }
    return "\(NSHomeDirectory())/Library/Keychains/login.keychain"
  }

  private static func keychainContainsCertificate(sha1: String, keychainPath: String) -> Bool {
    let result = runCapture("/usr/bin/security", [
      "find-certificate",
      "-Z",
      "-a",
      keychainPath
    ])
    guard result.status == 0 else { return false }
    return result.stdout.uppercased().contains("SHA-1 HASH: \(sha1.uppercased())")
  }

  private static func proxyOwnsPrivilegedPort(host: String, port: Int) -> Bool {
    let proxyResult = runCapture("/usr/sbin/lsof", [
      "-nP",
      "-a",
      "-c",
      "1context",
      "-iTCP:\(port)",
      "-sTCP:LISTEN"
    ])
    if proxyResult.status == 0, !proxyResult.stdout.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
      return true
    }

    let result = runCapture("/usr/sbin/lsof", [
      "-nP",
      "-iTCP:\(port)",
      "-sTCP:LISTEN"
    ])
    if result.status == 0, !result.stdout.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
      return false
    }
    return tcpConnects(host: host, port: port)
  }

  private static func tcpConnects(host: String, port: Int) -> Bool {
    let fd = socket(AF_INET, SOCK_STREAM, 0)
    guard fd >= 0 else { return false }
    defer { close(fd) }

    let flags = fcntl(fd, F_GETFL, 0)
    _ = fcntl(fd, F_SETFL, flags | O_NONBLOCK)

    var address = sockaddr_in()
    address.sin_len = UInt8(MemoryLayout<sockaddr_in>.size)
    address.sin_family = sa_family_t(AF_INET)
    address.sin_port = in_port_t(port).bigEndian
    guard inet_pton(AF_INET, host, &address.sin_addr) == 1 else { return false }

    let connectStatus = withUnsafePointer(to: &address) { pointer in
      pointer.withMemoryRebound(to: sockaddr.self, capacity: 1) { sockaddrPointer in
        connect(fd, sockaddrPointer, socklen_t(MemoryLayout<sockaddr_in>.size))
      }
    }
    if connectStatus == 0 { return true }
    guard errno == EINPROGRESS else { return false }

    var descriptor = pollfd(fd: fd, events: Int16(POLLOUT), revents: 0)
    let ready = poll(&descriptor, 1, 250)
    guard ready > 0, descriptor.revents & Int16(POLLOUT) != 0 else { return false }

    var error: Int32 = 0
    var length = socklen_t(MemoryLayout<Int32>.size)
    guard getsockopt(fd, SOL_SOCKET, SO_ERROR, &error, &length) == 0 else { return false }
    return error == 0
  }
}

public struct LocalWebSetupInstaller {
  private let manager: CaddyManager
  private let environment: [String: String]
  private let fileManager: FileManager

  public init(
    manager: CaddyManager? = nil,
    environment: [String: String] = ProcessInfo.processInfo.environment,
    fileManager: FileManager = .default
  ) {
    var localHTTPSEnvironment = environment
    localHTTPSEnvironment["ONECONTEXT_WIKI_URL_MODE"] = LocalWebURLMode.localHTTPSPortless.rawValue
    self.manager = manager ?? CaddyManager(environment: localHTTPSEnvironment, fileManager: fileManager)
    self.environment = localHTTPSEnvironment
    self.fileManager = fileManager
  }

  public func status() -> LocalWebSetupSnapshot {
    manager.diagnostics().setup
  }

  public func install() throws -> LocalWebSetupInstallResult {
    try rejectRootInvocation()
    let assets = try manager.prepareLocalHTTPSAssets()
    let systemPaths = LocalWebSetupSystemPaths(environment: environment)
    try requireInstalledApplication(systemPaths: systemPaths)
    try installUserCertificateTrust(assets: assets, systemPaths: systemPaths)
    try registerProxyService()
    restartProxyServiceIfLoaded()
    let snapshot = waitForSetupReadiness()
    guard snapshot.ready else {
      throw LocalWebSetupInstallerError.setupStillIncomplete(snapshot.blockingSummary)
    }
    let localWeb = try manager.start()
    return LocalWebSetupInstallResult(action: "install", setup: manager.diagnostics().setup, localWeb: localWeb)
  }

  public func uninstall() throws -> LocalWebSetupInstallResult {
    try rejectRootInvocation()
    let systemPaths = LocalWebSetupSystemPaths(environment: environment)
    try unregisterProxyService()
    try removeUserCertificateTrust(systemPaths: systemPaths)
    manager.stop()
    return LocalWebSetupInstallResult(action: "uninstall", setup: manager.diagnostics().setup, localWeb: nil)
  }

  private func installUserCertificateTrust(
    assets: LocalWebSetupAssets,
    systemPaths: LocalWebSetupSystemPaths
  ) throws {
    try fileManager.createDirectory(
      atPath: systemPaths.supportDirectory,
      withIntermediateDirectories: true
    )

    let keychain = userKeychainPath()
    if let oldSHA1 = readTrimmed(URL(fileURLWithPath: systemPaths.trustedRootSHA1)),
      !oldSHA1.isEmpty,
      oldSHA1.uppercased() != assets.rootCertificateSHA1.uppercased()
    {
      _ = runCapture("/usr/bin/security", [
        "delete-certificate",
        "-Z",
        oldSHA1,
        keychain
      ])
    }

    if fileManager.fileExists(atPath: systemPaths.trustedRootCertificate) {
      try fileManager.removeItem(atPath: systemPaths.trustedRootCertificate)
    }
    try fileManager.copyItem(atPath: assets.rootCertificate, toPath: systemPaths.trustedRootCertificate)

    _ = runCapture("/usr/bin/security", [
      "delete-certificate",
      "-Z",
      assets.rootCertificateSHA1,
      keychain
    ])
    let trustResult = runCapture("/usr/bin/security", [
      "add-trusted-cert",
      "-r",
      "trustRoot",
      "-p",
      "ssl",
      "-p",
      "basic",
      "-k",
      keychain,
      systemPaths.trustedRootCertificate
    ])
    guard trustResult.status == 0 else {
      try? fileManager.removeItem(atPath: systemPaths.trustedRootCertificate)
      throw LocalWebSetupInstallerError.certificateTrustFailed(commandOutput(trustResult))
    }

    try writePublicString(assets.rootCertificateSHA1 + "\n", to: systemPaths.trustedRootSHA1)
    try writePublicString(assets.rootCertificateSHA256 + "\n", to: systemPaths.trustedRootSHA256)
    try writePublicString(setupMarkerJSON(assets: assets) + "\n", to: systemPaths.setupMarker)
  }

  private func removeUserCertificateTrust(systemPaths: LocalWebSetupSystemPaths) throws {
    let keychain = userKeychainPath()
    if fileManager.fileExists(atPath: systemPaths.trustedRootCertificate) {
      _ = runCapture("/usr/bin/security", [
        "remove-trusted-cert",
        systemPaths.trustedRootCertificate
      ])
    }
    if let sha1 = readTrimmed(URL(fileURLWithPath: systemPaths.trustedRootSHA1)), !sha1.isEmpty {
      _ = runCapture("/usr/bin/security", [
        "delete-certificate",
        "-Z",
        sha1,
        keychain
      ])
    }
    for path in [
      systemPaths.trustedRootCertificate,
      systemPaths.trustedRootSHA1,
      systemPaths.trustedRootSHA256,
      systemPaths.setupMarker
    ] where fileManager.fileExists(atPath: path) {
      try fileManager.removeItem(atPath: path)
    }
  }

  private func registerProxyService() throws {
    let service = SMAppService.daemon(plistName: LocalWebSetupConstants.proxyPlistName)
    switch service.status {
    case .enabled:
      if proxyRequirementSatisfied() {
        return
      }
      do {
        try service.unregister()
        try service.register()
      } catch {
        if service.status == .requiresApproval {
          SMAppService.openSystemSettingsLoginItems()
          throw LocalWebSetupInstallerError.backgroundItemRequiresApproval(serviceApprovalMessage)
        }
        throw LocalWebSetupInstallerError.serviceRegistrationFailed(error.localizedDescription)
      }
    case .requiresApproval:
      SMAppService.openSystemSettingsLoginItems()
      throw LocalWebSetupInstallerError.backgroundItemRequiresApproval(serviceApprovalMessage)
    case .notRegistered, .notFound:
      do {
        try service.register()
      } catch {
        if service.status == .requiresApproval {
          SMAppService.openSystemSettingsLoginItems()
          throw LocalWebSetupInstallerError.backgroundItemRequiresApproval(serviceApprovalMessage)
        }
        throw LocalWebSetupInstallerError.serviceRegistrationFailed(error.localizedDescription)
      }
    @unknown default:
      throw LocalWebSetupInstallerError.serviceRegistrationFailed("Unknown ServiceManagement status.")
    }

    if service.status == .requiresApproval {
      SMAppService.openSystemSettingsLoginItems()
      throw LocalWebSetupInstallerError.backgroundItemRequiresApproval(serviceApprovalMessage)
    }
  }

  private func requireInstalledApplication(systemPaths: LocalWebSetupSystemPaths) throws {
    if environment["ONECONTEXT_ALLOW_NON_APPLICATIONS_LOCAL_WEB_SETUP"] == "1" {
      return
    }

    let app = URL(fileURLWithPath: systemPaths.appBundle, isDirectory: true)
      .standardizedFileURL
      .resolvingSymlinksInPath()
    guard app.pathExtension == "app",
      app.pathComponents.count >= 3,
      app.pathComponents[0] == "/",
      app.pathComponents[1] == "Applications"
    else {
      throw LocalWebSetupInstallerError.appInstallRequired
    }
  }

  private func proxyRequirementSatisfied() -> Bool {
    manager.diagnostics().setup.requirements.first { $0.id == "local-web.privileged-bind" }?.status == .satisfied
  }

  private func waitForSetupReadiness(timeout: TimeInterval = 5.0) -> LocalWebSetupSnapshot {
    let deadline = Date().addingTimeInterval(timeout)
    var snapshot = manager.diagnostics().setup
    while !snapshot.ready, Date() < deadline {
      Thread.sleep(forTimeInterval: 0.2)
      snapshot = manager.diagnostics().setup
    }
    return snapshot
  }

  private func unregisterProxyService() throws {
    let service = SMAppService.daemon(plistName: LocalWebSetupConstants.proxyPlistName)
    switch service.status {
    case .enabled, .requiresApproval:
      try service.unregister()
    case .notRegistered, .notFound:
      return
    @unknown default:
      return
    }
  }

  private func restartProxyServiceIfLoaded() {
    _ = runCapture("/bin/launchctl", [
      "kill",
      "TERM",
      "system/\(LocalWebSetupConstants.proxyLabel)"
    ])
  }

  private func setupMarkerJSON(assets: LocalWebSetupAssets) -> String {
    let payload: [String: Any] = [
      "schema_version": 1,
      "installed_at": ISO8601DateFormatter().string(from: Date()),
      "label": LocalWebSetupConstants.proxyLabel,
      "public_host": assets.publicHost,
      "public_port": assets.publicPort,
      "backend_host": assets.backendHost,
      "backend_port": assets.backendPort,
      "root_certificate_sha1": assets.rootCertificateSHA1,
      "root_certificate_sha256": assets.rootCertificateSHA256
    ]
    let data = (try? JSONSerialization.data(withJSONObject: payload, options: [.prettyPrinted, .sortedKeys]))
      ?? Data("{}".utf8)
    return String(decoding: data, as: UTF8.self)
  }

  private func readTrimmed(_ url: URL) -> String? {
    (try? String(contentsOf: url, encoding: .utf8))?
      .trimmingCharacters(in: .whitespacesAndNewlines)
  }

  private var serviceApprovalMessage: String {
    "Open System Settings and allow 1Context, then open the wiki again."
  }

  private func rejectRootInvocation() throws {
    if geteuid() == 0 || environment["SUDO_USER"] != nil {
      throw LocalWebSetupInstallerError.rootUserUnsupported
    }
  }

  private func userKeychainPath() -> String {
    if let override = environment["ONECONTEXT_LOCAL_WEB_USER_KEYCHAIN_PATH"], !override.isEmpty {
      return override
    }
    let modern = "\(NSHomeDirectory())/Library/Keychains/login.keychain-db"
    if fileManager.fileExists(atPath: modern) {
      return modern
    }
    return "\(NSHomeDirectory())/Library/Keychains/login.keychain"
  }

  private func writePublicString(_ value: String, to path: String) throws {
    let url = URL(fileURLWithPath: path)
    try fileManager.createDirectory(
      at: url.deletingLastPathComponent(),
      withIntermediateDirectories: true
    )
    try value.write(to: url, atomically: true, encoding: .utf8)
    chmod(url.path, S_IRUSR | S_IWUSR | S_IRGRP | S_IROTH)
  }

  private func commandOutput(_ result: (status: Int32, stdout: String, stderr: String)) -> String {
    let output = [result.stderr, result.stdout]
      .map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
      .filter { !$0.isEmpty }
      .joined(separator: "\n")
    return output.isEmpty ? "Command failed with status \(result.status)." : output
  }
}

public enum LocalWebSetupInstallerError: Error, LocalizedError, Equatable {
  case invalidCertificate(url: String)
  case proxyExecutableMissing
  case certificateTrustFailed(String)
  case backgroundItemRequiresApproval(String)
  case serviceRegistrationFailed(String)
  case setupStillIncomplete(String)
  case rootUserUnsupported
  case appInstallRequired

  public var errorDescription: String? {
    switch self {
    case .invalidCertificate(let url):
      return "Could not read local HTTPS root certificate: \(url)"
    case .proxyExecutableMissing:
      return "Bundled local HTTPS proxy was not found"
    case .certificateTrustFailed(let message):
      return message.isEmpty ? "Could not trust the local HTTPS certificate" : message
    case .backgroundItemRequiresApproval(let message):
      return message.isEmpty ? "1Context needs background item approval" : message
    case .serviceRegistrationFailed(let message):
      return message.isEmpty ? "Could not grant network permissions" : message
    case .setupStillIncomplete(let message):
      return message
    case .rootUserUnsupported:
      return "Run 1Context setup as your macOS user, not with sudo. macOS permissions must be granted by the signed app for the logged-in user."
    case .appInstallRequired:
      return "Install 1Context in Applications, then grant Local Wiki Access."
    }
  }
}

private func runCapture(_ executable: String, _ arguments: [String]) -> (status: Int32, stdout: String, stderr: String) {
  let process = Process()
  process.executableURL = URL(fileURLWithPath: executable)
  process.arguments = arguments
  let stdout = Pipe()
  let stderr = Pipe()
  process.standardOutput = stdout
  process.standardError = stderr

  do {
    try process.run()
    process.waitUntilExit()
  } catch {
    return (1, "", error.localizedDescription)
  }

  return (
    process.terminationStatus,
    String(data: stdout.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8) ?? "",
    String(data: stderr.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8) ?? ""
  )
}
