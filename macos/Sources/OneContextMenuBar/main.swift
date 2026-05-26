import AppKit
import Darwin
import Foundation
import OneContextCore
import OneContextInstall
import OneContextLocalWeb
import OneContextPlatform
import OneContextProtocol
import OneContextSetup
import OneContextSparkleUpdate
import OneContextSupervisor
import OneContextUpdate

private enum Constants {
  static let appName = "1Context"
  static let runtimeRefreshMinimumInterval: TimeInterval = 5
  static let localWebStartupRetryDelays: [TimeInterval] = [1, 3, 10]
  static let setupReadinessPollInterval: TimeInterval = 1
  static let setupReadinessPollTimeout: TimeInterval = 120
  static let lastLaunchedVersionDefaultsKey = "OneContextLastLaunchedVersion"
  static let lastShownPostInstallVersionDefaultsKey = "OneContextLastShownPostInstallMessageVersion"
}

private struct ProcessCaptureResult: Sendable {
  let status: Int32
  let stdout: String
  let stderr: String
}

private enum PermissionProbeCommandResult: Sendable {
  case exit(Int32)
  case failure(String)
}

private final class PermissionProbeCommandCompletion: @unchecked Sendable {
  private let condition = NSCondition()
  private var result: PermissionProbeCommandResult?

  func finish(_ result: PermissionProbeCommandResult) {
    condition.lock()
    self.result = result
    condition.broadcast()
    condition.unlock()
  }

  func wait() -> PermissionProbeCommandResult {
    condition.lock()
    defer { condition.unlock() }
    while result == nil {
      condition.wait()
    }
    return result ?? .failure("permission capability probe did not return a result")
  }
}

nonisolated(unsafe) private var menuInstanceLockFD: Int32 = -1

private func acquireMenuInstanceLock() -> Bool {
  let paths = RuntimePaths.current()
  try? RuntimePermissions.ensurePrivateDirectory(paths.appSupportDirectory)
  try? RuntimePermissions.ensurePrivateDirectory(paths.runDirectory)

  let lockPath = paths.runDirectory.appendingPathComponent("1context-menu.lock").path
  let fd = open(lockPath, O_CREAT | O_RDWR, S_IRUSR | S_IWUSR)
  guard fd >= 0 else { return true }

  if flock(fd, LOCK_EX | LOCK_NB) != 0 {
    close(fd)
    return false
  }

  ftruncate(fd, 0)
  let pid = "\(getpid())\n"
  _ = pid.withCString { pointer in
    write(fd, pointer, strlen(pointer))
  }
  RuntimePermissions.ensurePrivateFile(lockPath)
  menuInstanceLockFD = fd
  return true
}

@MainActor
private func showFishAlert(_ message: String) {
  let alert = NSAlert()
  alert.messageText = message
  alert.icon = loadFishAlertIcon()
  alert.addButton(withTitle: "OK")
  NSApp.activate(ignoringOtherApps: true)
  alert.runModal()
}

@MainActor
private func loadFishAlertIcon() -> NSImage? {
  if let cached = AppDelegate.cachedFishAlertIcon {
    return cached.copy() as? NSImage
  }
  guard let image = menuBarIconURL().flatMap(NSImage.init(contentsOf:)) else {
    return nil
  }
  image.isTemplate = false
  image.size = NSSize(width: 64, height: 64)
  AppDelegate.cachedFishAlertIcon = image
  return image.copy() as? NSImage
}

private func menuBarIconURL() -> URL? {
  if let bundleURL = Bundle.main.url(forResource: "MenuBarIcon", withExtension: "png") {
    return bundleURL
  }

  for executable in executableURLCandidates() {
    let resources = executable
      .deletingLastPathComponent()
      .deletingLastPathComponent()
      .appendingPathComponent("Resources/MenuBarIcon.png")
    if FileManager.default.fileExists(atPath: resources.path) {
      return resources
    }
  }
  return nil
}

private func executableURLCandidates() -> [URL] {
  var candidates: [URL] = []
  if let current = currentExecutableURL() {
    candidates.append(current)
  }
  if let firstArgument = CommandLine.arguments.first, !firstArgument.isEmpty {
    candidates.append(URL(fileURLWithPath: firstArgument).resolvingSymlinksInPath())
  }
  return candidates
}

private func currentExecutableURL() -> URL? {
  var size = UInt32(0)
  _NSGetExecutablePath(nil, &size)
  var buffer = [CChar](repeating: 0, count: Int(size))
  guard _NSGetExecutablePath(&buffer, &size) == 0 else { return nil }
  let pathBytes = buffer.prefix { $0 != 0 }.map { UInt8(bitPattern: $0) }
  return URL(fileURLWithPath: String(decoding: pathBytes, as: UTF8.self)).resolvingSymlinksInPath()
}

private func runPermissionCapabilityProbeIfRequested() {
  guard CommandLine.arguments.contains("--permission-capability-probe") else { return }

  let outputPath = commandLineValue(after: "--json")
  let timeout = commandLineValue(after: "--timeout").flatMap(TimeInterval.init) ?? 5
  let skipBrowserExtension = !CommandLine.arguments.contains("--include-browser-extension")
  let completion = PermissionProbeCommandCompletion()

  Task.detached(priority: .userInitiated) {
    let probeReport = await OneContextPermissionCapabilityProbe.run(
      options: OneContextPermissionCapabilityProbeOptions(
        skipBrowserExtension: skipBrowserExtension,
        timeoutSeconds: timeout
      )
    )
    do {
      let encoder = JSONEncoder()
      encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
      let data = try encoder.encode(probeReport)
      if let outputPath {
        let url = URL(fileURLWithPath: outputPath)
        try FileManager.default.createDirectory(
          at: url.deletingLastPathComponent(),
          withIntermediateDirectories: true
        )
        try data.write(to: url, options: .atomic)
      } else if let text = String(data: data, encoding: .utf8) {
        print(text)
      }
      completion.finish(.exit(probeReport.passed ? 0 : 2))
    } catch {
      completion.finish(.failure(error.localizedDescription))
    }
  }

  switch completion.wait() {
  case .exit(let status):
    Foundation.exit(status)
  case .failure(let message):
    FileHandle.standardError.write(Data("permission capability probe failed: \(message)\n".utf8))
    Foundation.exit(1)
  }
}

private func commandLineValue(after flag: String) -> String? {
  guard let index = CommandLine.arguments.firstIndex(of: flag) else { return nil }
  let valueIndex = CommandLine.arguments.index(after: index)
  guard valueIndex < CommandLine.arguments.endIndex else { return nil }
  return CommandLine.arguments[valueIndex]
}

@MainActor
private final class AppSetupWindowController: NSWindowController {
  var onGrantLocalWikiAccess: (@MainActor () -> Void)?
  var onGrantScreenRecording: (@MainActor () -> Void)?
  var onGrantAccessibility: (@MainActor () -> Void)?
  var onGrantInputMonitoring: (@MainActor () -> Void)?
  var onGrantBrowserExtension: (@MainActor () -> Void)?
  var onGrantMicrophone: (@MainActor () -> Void)?
  var onGrantAutomation: (@MainActor () -> Void)?
  var onGrantFullDiskAccess: (@MainActor () -> Void)?
  var onOpenWiki: (@MainActor () -> Void)?
  var onRefresh: (@MainActor () -> Void)?

  private let messageLabel = NSTextField(labelWithString: "")
  private let localWikiRow = SetupRequirementRow(title: "Local Wiki Access")
  private let screenRecordingRow = SetupRequirementRow(title: "Screen & System Audio Recording")
  private let accessibilityRow = SetupRequirementRow(title: "Accessibility")
  private let inputMonitoringRow = SetupRequirementRow(title: "Input Monitoring")
  private let browserExtensionRow = SetupRequirementRow(title: "Browser Extension Permissions")
  private let microphoneRow = SetupRequirementRow(title: "Microphone")
  private let automationRow = SetupRequirementRow(title: "Automation")
  private let fullDiskAccessRow = SetupRequirementRow(title: "Full Disk Access")
  private let footer = NSStackView()
  private let refreshButton = NSButton(title: "Check Again", target: nil, action: nil)

  init() {
    let window = NSWindow(
      contentRect: NSRect(x: 0, y: 0, width: 620, height: 720),
      styleMask: [.titled, .closable, .miniaturizable],
      backing: .buffered,
      defer: false
    )
    window.title = "1Context Setup"
    window.isReleasedWhenClosed = false
    window.center()
    super.init(window: window)
    buildContent()
  }

  @available(*, unavailable)
  required init?(coder: NSCoder) {
    fatalError("init(coder:) has not been implemented")
  }

  func render(
    _ snapshot: OneContextAppSetupSnapshot,
    isGrantingLocalWikiAccess: Bool,
    message: String?
  ) {
    if let message, !message.isEmpty {
      messageLabel.stringValue = message
      messageLabel.isHidden = false
    } else {
      messageLabel.stringValue = ""
      messageLabel.isHidden = true
    }

    let localStatus: SetupRequirementRow.Status
    let localAction: SetupRequirementRow.Action?
    if isGrantingLocalWikiAccess {
      localStatus = .working("Granting")
      localAction = nil
    } else if snapshot.localWikiAccess.ready {
      localStatus = .granted
      localAction = nil
    } else {
      localStatus = .required
      localAction = SetupRequirementRow.Action(title: "Grant", handler: { [weak self] in
        self?.onGrantLocalWikiAccess?()
      })
    }
    localWikiRow.render(
      status: localStatus,
      action: localAction,
      detail: nil
    )
    renderRequiredPermissionRow(
      screenRecordingRow,
      permission: snapshot.rememberingPermissions.screenRecording,
      grantHandler: onGrantScreenRecording
    )
    renderRequiredPermissionRow(
      accessibilityRow,
      permission: snapshot.rememberingPermissions.accessibility,
      grantHandler: onGrantAccessibility
    )
    renderRequiredPermissionRow(
      inputMonitoringRow,
      permission: snapshot.rememberingPermissions.inputMonitoring,
      grantHandler: onGrantInputMonitoring
    )
    renderRequiredPermissionRow(
      browserExtensionRow,
      permission: snapshot.rememberingPermissions.browserExtension,
      grantHandler: onGrantBrowserExtension,
      actionTitle: "Install"
    )
    renderRequiredPermissionRow(
      microphoneRow,
      permission: snapshot.rememberingPermissions.microphone,
      grantHandler: onGrantMicrophone
    )
    renderRequiredPermissionRow(
      automationRow,
      permission: snapshot.rememberingPermissions.automation,
      grantHandler: onGrantAutomation,
      actionTitle: "Configure"
    )
    renderRequiredPermissionRow(
      fullDiskAccessRow,
      permission: snapshot.rememberingPermissions.fullDiskAccess,
      grantHandler: onGrantFullDiskAccess
    )
    setRefreshButtonVisible(!snapshot.requiredReady || isGrantingLocalWikiAccess)
  }

  private func buildContent() {
    guard let window else { return }
    let root = NSView()
    root.translatesAutoresizingMaskIntoConstraints = false
    window.contentView = root

    let stack = NSStackView()
    stack.orientation = .vertical
    stack.alignment = .leading
    stack.spacing = 18
    stack.translatesAutoresizingMaskIntoConstraints = false
    root.addSubview(stack)

    let header = NSStackView()
    header.orientation = .vertical
    header.alignment = .leading
    header.spacing = 0

    let title = NSTextField(labelWithString: "Set Up 1Context")
    title.font = .systemFont(ofSize: 28, weight: .bold)
    title.textColor = .labelColor
    header.addArrangedSubview(title)

    stack.addArrangedSubview(header)

    messageLabel.font = .systemFont(ofSize: 13, weight: .semibold)
    messageLabel.textColor = .controlAccentColor
    messageLabel.isHidden = true
    stack.addArrangedSubview(messageLabel)

    stack.addArrangedSubview(localWikiRow)
    localWikiRow.widthAnchor.constraint(equalTo: stack.widthAnchor).isActive = true

    stack.addArrangedSubview(screenRecordingRow)
    screenRecordingRow.widthAnchor.constraint(equalTo: stack.widthAnchor).isActive = true

    stack.addArrangedSubview(accessibilityRow)
    accessibilityRow.widthAnchor.constraint(equalTo: stack.widthAnchor).isActive = true

    stack.addArrangedSubview(inputMonitoringRow)
    inputMonitoringRow.widthAnchor.constraint(equalTo: stack.widthAnchor).isActive = true

    stack.addArrangedSubview(browserExtensionRow)
    browserExtensionRow.widthAnchor.constraint(equalTo: stack.widthAnchor).isActive = true

    stack.addArrangedSubview(microphoneRow)
    microphoneRow.widthAnchor.constraint(equalTo: stack.widthAnchor).isActive = true

    stack.addArrangedSubview(automationRow)
    automationRow.widthAnchor.constraint(equalTo: stack.widthAnchor).isActive = true

    stack.addArrangedSubview(fullDiskAccessRow)
    fullDiskAccessRow.widthAnchor.constraint(equalTo: stack.widthAnchor).isActive = true

    footer.orientation = .horizontal
    footer.alignment = .centerY
    footer.spacing = 8
    footer.translatesAutoresizingMaskIntoConstraints = false
    refreshButton.target = self
    refreshButton.action = #selector(refresh)
    refreshButton.bezelStyle = .rounded
    footer.addArrangedSubview(NSView())
    footer.addArrangedSubview(refreshButton)
    stack.addArrangedSubview(footer)
    footer.widthAnchor.constraint(equalTo: stack.widthAnchor).isActive = true

    NSLayoutConstraint.activate([
      stack.leadingAnchor.constraint(equalTo: root.leadingAnchor, constant: 28),
      stack.trailingAnchor.constraint(equalTo: root.trailingAnchor, constant: -28),
      stack.topAnchor.constraint(equalTo: root.topAnchor, constant: 28),
      stack.bottomAnchor.constraint(lessThanOrEqualTo: root.bottomAnchor, constant: -20)
    ])
  }

  @objc private func refresh() {
    onRefresh?()
  }

  private func renderRequiredPermissionRow(
    _ row: SetupRequirementRow,
    permission: OneContextRequiredPermissionSnapshot,
    grantHandler: (@MainActor () -> Void)?,
    actionTitle: String? = nil
  ) {
    let action: SetupRequirementRow.Action?
    if permission.ready {
      action = nil
    } else if permission.status == .notRequired {
      action = nil
    } else if let grantHandler {
      let title = permission.status == .needsRelaunch ? "Relaunch" : (actionTitle ?? "Grant")
      action = SetupRequirementRow.Action(title: title, handler: grantHandler)
    } else {
      action = nil
    }
    let status: SetupRequirementRow.Status
    switch permission.status {
    case .granted:
      status = .granted
    case .required:
      status = .required
    case .notRequired:
      status = .notRequired
    case .needsRelaunch:
      status = .working("Needs Relaunch")
    case .unavailable:
      status = .unavailable
    }
    row.render(
      status: status,
      action: action,
      detail: permission.ready ? nil : permission.detail
    )
  }

  private func setRefreshButtonVisible(_ visible: Bool) {
    refreshButton.isEnabled = visible
    if visible {
      if refreshButton.superview == nil {
        footer.addArrangedSubview(refreshButton)
      }
      refreshButton.isHidden = false
      return
    }

    refreshButton.isHidden = true
    if refreshButton.superview != nil {
      footer.removeArrangedSubview(refreshButton)
      refreshButton.removeFromSuperview()
    }
  }
}

@MainActor
private final class SetupRequirementRow: NSView {
  struct Action {
    let title: String
    let handler: @MainActor () -> Void
  }

  enum Status {
    case granted
    case required
    case working(String)
    case notRequired
    case unavailable

    var title: String {
      switch self {
      case .granted:
        return "Granted"
      case .required:
        return "Required"
      case .working(let title):
        return title
      case .notRequired:
        return "Not Required Yet"
      case .unavailable:
        return "Unavailable"
      }
    }

    var color: NSColor {
      switch self {
      case .granted:
        return .systemGreen
      case .required:
        return .systemOrange
      case .working:
        return .controlAccentColor
      case .notRequired:
        return .secondaryLabelColor
      case .unavailable:
        return .systemRed
      }
    }

    var showsEnabledState: Bool {
      switch self {
      case .granted:
        return true
      case .required, .working, .notRequired, .unavailable:
        return false
      }
    }
  }

  private let titleLabel: NSTextField
  private let detailLabel = NSTextField(labelWithString: "")
  private let actionButton = NSButton(title: "", target: nil, action: nil)
  private var actionHandler: (@MainActor () -> Void)?

  init(title: String) {
    self.titleLabel = NSTextField(labelWithString: title)
    super.init(frame: .zero)
    translatesAutoresizingMaskIntoConstraints = false
    buildContent()
  }

  @available(*, unavailable)
  required init?(coder: NSCoder) {
    fatalError("init(coder:) has not been implemented")
  }

  func render(status: Status, action: Action?, detail: String?) {
    let trimmedDetail = detail?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
    detailLabel.stringValue = trimmedDetail
    detailLabel.isHidden = trimmedDetail.isEmpty
    if let action {
      actionButton.title = action.title
      actionButton.attributedTitle = NSAttributedString(string: action.title)
      actionButton.isEnabled = true
      actionHandler = action.handler
    } else {
      actionButton.attributedTitle = NSAttributedString(
        string: status.title,
        attributes: [
          .foregroundColor: status.color,
          .font: NSFont.systemFont(ofSize: 13, weight: .semibold)
        ]
      )
      actionButton.isEnabled = status.showsEnabledState
      actionHandler = nil
    }
    actionButton.isHidden = false
  }

  private func buildContent() {
    let stack = NSStackView()
    stack.orientation = .horizontal
    stack.alignment = .centerY
    stack.spacing = 12
    stack.translatesAutoresizingMaskIntoConstraints = false
    addSubview(stack)

    let textStack = NSStackView()
    textStack.orientation = .vertical
    textStack.alignment = .leading
    textStack.spacing = 0

    titleLabel.font = .systemFont(ofSize: 15, weight: .semibold)
    textStack.addArrangedSubview(titleLabel)
    detailLabel.font = .systemFont(ofSize: 11)
    detailLabel.textColor = .secondaryLabelColor
    detailLabel.lineBreakMode = .byWordWrapping
    detailLabel.maximumNumberOfLines = 2
    detailLabel.isHidden = true
    textStack.addArrangedSubview(detailLabel)
    textStack.setContentHuggingPriority(.defaultLow, for: .horizontal)

    actionButton.target = self
    actionButton.action = #selector(runAction)
    actionButton.bezelStyle = .rounded
    actionButton.setContentHuggingPriority(.required, for: .horizontal)

    stack.addArrangedSubview(textStack)
    stack.addArrangedSubview(actionButton)

    NSLayoutConstraint.activate([
      heightAnchor.constraint(greaterThanOrEqualToConstant: 48),
      stack.leadingAnchor.constraint(equalTo: leadingAnchor),
      stack.trailingAnchor.constraint(equalTo: trailingAnchor),
      stack.topAnchor.constraint(equalTo: topAnchor, constant: 10),
      stack.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -10),
      actionButton.widthAnchor.constraint(greaterThanOrEqualToConstant: 118)
    ])
  }

  @objc private func runAction() {
    actionHandler?()
  }
}

@MainActor
private final class AppDelegate: NSObject, NSApplicationDelegate, NSMenuDelegate {
  static var cachedFishAlertIcon: NSImage?

  private var statusItem: NSStatusItem!
  private var timer: Timer?
  private var runtimeState: RuntimeState = .checking
  private var updateState: UpdateState = .upToDate
  private var renderedStateTitle: String?
  private var renderedVersionTitle: String?
  private var renderedUpdateTitle: String?
  private var renderedUpdateAction: Selector?
  private var isCheckingForUpdates = false
  private var isRepairingRuntime = false
  private var isWikiRefreshInFlight = false
  private var isLocalWebSetupInFlight = false
  private var isUninstallInFlight = false
  private var isReadinessRefreshInFlight = false
  private var isSetupReadinessCheckInFlight = false
  private var isMenuOpen = false
  private var didOfferLocalWebSetupAtLaunch = false
  private var cachedRequiredSetupReady = false
  private var setupReadinessPollingStartedAt: Date?
  private var setupReadinessPollingMessage: String?
  private var setupContinuation = OneContextSetupContinuation()
  private var pendingUpdateState: UpdateState?
  private var activeAlertMessage: String?
  private var lastAlertShownAt: [String: Date] = [:]
  private var renderGeneration = 0
  private var lastRuntimeRefreshStartedAt: Date?
  private var setupReadinessTimer: Timer?
  private let menu = NSMenu()
  private let stateItem = NSMenuItem(title: RuntimeState.checking.title, action: nil, keyEquivalent: "")
  private let openWikiItem = NSMenuItem(title: "Open Wiki", action: #selector(openWiki), keyEquivalent: "")
  private let refreshWikiItem = NSMenuItem(title: "Refresh Wiki", action: #selector(refreshWiki), keyEquivalent: "")
  private let settingsItem = NSMenuItem(title: "Settings", action: nil, keyEquivalent: "")
  private let settingsMenu = NSMenu()
  private let versionItem = NSMenuItem(title: "", action: nil, keyEquivalent: "")
  private let autoPublishCadenceItem = NSMenuItem(title: "Auto-Publish", action: nil, keyEquivalent: "")
  private let autoPublishCadenceMenu = NSMenu()
  private let autoPublishNoLimitItem = NSMenuItem(title: "No Limit", action: #selector(setWikiAutoPublishCadence), keyEquivalent: "")
  private let autoPublishOneMinuteItem = NSMenuItem(title: "1 Min", action: #selector(setWikiAutoPublishCadence), keyEquivalent: "")
  private let autoPublishThirtyMinutesItem = NSMenuItem(title: "30 Min", action: #selector(setWikiAutoPublishCadence), keyEquivalent: "")
  private let aboutItem = NSMenuItem(title: "About 1Context", action: #selector(showAbout), keyEquivalent: "")
  private let setupItem = NSMenuItem(title: "Setup...", action: #selector(showSetup), keyEquivalent: "")
  private let updateItem = NSMenuItem(title: "", action: nil, keyEquivalent: "")
  private let uninstallItem = NSMenuItem(title: "Uninstall 1Context...", action: #selector(uninstallOneContext), keyEquivalent: "")
  private let quitItem = NSMenuItem(title: "Quit", action: #selector(quit), keyEquivalent: "q")
  private let appVersion = Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String
    ?? oneContextVersion
  private let appIdentity = OneContextAppIdentity.current()
  private let localWeb = CaddyManager()
  private let localWebQueue = DispatchQueue(label: "com.haptica.1context.menu.local-web")
  private lazy var sparkleUpdater = SparkleUpdateController()
  private lazy var setupWindowController: AppSetupWindowController = {
    let controller = AppSetupWindowController()
    controller.onGrantLocalWikiAccess = { [weak self] in
      Task { [weak self] in
        _ = await self?.runLocalWebSetupFlow()
      }
    }
    controller.onGrantScreenRecording = { [weak self] in
      self?.requestScreenRecordingAccess()
    }
    controller.onGrantAccessibility = { [weak self] in
      self?.requestAccessibilityAccess()
    }
    controller.onGrantInputMonitoring = { [weak self] in
      self?.requestInputMonitoringAccess()
    }
    controller.onGrantBrowserExtension = { [weak self] in
      self?.requestBrowserExtensionAccess()
    }
    controller.onGrantMicrophone = { [weak self] in
      self?.requestMicrophoneAccess()
    }
    controller.onGrantAutomation = { [weak self] in
      self?.configureAutomationAccess()
    }
    controller.onGrantFullDiskAccess = { [weak self] in
      self?.requestFullDiskAccess()
    }
    controller.onOpenWiki = { [weak self] in
      self?.openWiki()
    }
    controller.onRefresh = { [weak self] in
      self?.reconcileSetupFromUserRefresh()
    }
    return controller
  }()

  private var currentVersion: String {
    appVersion
  }

  private func currentReadiness() -> OneContextAppReadinessSnapshot {
    let readiness = OneContextAppReadiness.current(localWeb: localWeb)
    cachedRequiredSetupReady = readiness.requiredSetupReady
    return readiness
  }

  private func refreshApplicationLifecycle(userInitiated: Bool, force: Bool = false) {
    guard !isReadinessRefreshInFlight || userInitiated else { return }
    isReadinessRefreshInFlight = true
    Task.detached(priority: userInitiated ? .userInitiated : .utility) {
      await Self.refreshRuntimePermissionProofsIfAuthorized()
      let readiness = Self.computeReadiness()
      await MainActor.run {
        self.isReadinessRefreshInFlight = false
        self.cachedRequiredSetupReady = readiness.requiredSetupReady
        guard readiness.requiredSetupReady else {
          self.setRuntimeState(.needsSetup)
          self.ensureRuntimeRunning(userInitiated: userInitiated, force: force, requiredSetupReady: false)
          return
        }
        self.startLocalWebEdge(requiredSetupReady: true)
        self.ensureRuntimeRunning(userInitiated: userInitiated, force: force, requiredSetupReady: true)
      }
    }
  }

  nonisolated private static func computeReadiness() -> OneContextAppReadinessSnapshot {
    OneContextAppReadiness.current(localWeb: CaddyManager())
  }

  nonisolated private static func refreshRuntimePermissionProofsIfAuthorized() async {
    await OneContextSystemPermissions.refreshRuntimeProofsIfAuthorized()
  }

  private func registerMenuLaunchAgent() {
    guard let appPath = Bundle.main.executableURL?.path else { return }
    Task.detached(priority: .utility) {
      try? await LaunchAgentManager().startMenu(appPath: appPath)
    }
  }

  private func refreshRequiredSetupCache() -> Bool {
    cachedRequiredSetupReady = currentReadiness().requiredSetupReady
    return cachedRequiredSetupReady
  }

  private func handleAppInstallAtLaunch() -> Bool {
    let planner = AppInstallPlanner()
    switch planner.recommendation(currentBundleURL: Bundle.main.bundleURL, currentVersion: currentVersion) {
    case .continueInPlace:
      return false
    case .moveToApplications(let request):
      return presentAppInstallRequest(request)
    }
  }

  private func presentAppInstallRequest(_ request: AppInstallRequest) -> Bool {
    switch request.existingRelation {
    case .newerVersion:
      return presentOpenInstalledAppPrompt(request)
    case .sameVersion:
      return request.existingInstallMatchesCurrent
        ? relaunchInstalledApp(request.destinationBundleURL)
        : presentMoveToApplicationsPrompt(request)
    case .none, .olderVersion, .unknownVersion:
      return presentMoveToApplicationsPrompt(request)
    }
  }

  private func presentMoveToApplicationsPrompt(_ request: AppInstallRequest) -> Bool {
    let alert = NSAlert()
    alert.messageText = "Install \(appIdentity.displayName)?"
    alert.informativeText = movePromptDetail(for: request)
    alert.icon = loadFishAlertIcon()
    alert.addButton(withTitle: "Install and Open")
    alert.addButton(withTitle: "Quit")
    NSApp.setActivationPolicy(.regular)
    NSApp.activate(ignoringOtherApps: true)
    guard alert.runModal() == .alertFirstButtonReturn else { return false }
    return moveAndRelaunch(request)
  }

  private func presentOpenInstalledAppPrompt(_ request: AppInstallRequest) -> Bool {
    let alert = NSAlert()
    alert.messageText = "Open the Installed \(appIdentity.displayName)?"
    alert.informativeText = "A newer \(appIdentity.displayName) is already installed. Open that copy to keep updates intact."
    alert.icon = loadFishAlertIcon()
    alert.addButton(withTitle: "Open Installed")
    alert.addButton(withTitle: "Quit")
    NSApp.setActivationPolicy(.regular)
    NSApp.activate(ignoringOtherApps: true)
    guard alert.runModal() == .alertFirstButtonReturn else { return false }
    return relaunchInstalledApp(request.destinationBundleURL)
  }

  private func movePromptDetail(for request: AppInstallRequest) -> String {
    switch request.existingRelation {
    case .none:
      return "\(appIdentity.displayName) needs to run from Applications so local wiki access and updates work reliably."
    case .olderVersion:
      return "This replaces the older installed copy and opens 1Context from Applications."
    case .unknownVersion:
      return "This replaces the installed copy and opens 1Context from Applications."
    case .sameVersion:
      return "This refreshes the installed copy and opens 1Context from Applications."
    case .newerVersion:
      return "A newer \(appIdentity.displayName) is already installed."
    }
  }

  private func moveAndRelaunch(_ request: AppInstallRequest) -> Bool {
    do {
      quitRunningInstalledApp(at: request.destinationBundleURL)
      let mover = AppInstallMover()
      try mover.install(request)
      try mover.relaunch(destinationBundleURL: request.destinationBundleURL)
      NSApp.terminate(nil)
      return true
    } catch {
      presentInstallError(error)
      return false
    }
  }

  private func quitRunningInstalledApp(at destination: URL) {
    let destinationPath = destination.standardizedFileURL.resolvingSymlinksInPath().path
    let currentPID = getpid()
    let apps = NSWorkspace.shared.runningApplications.filter { app in
      guard app.processIdentifier != currentPID,
        let bundleURL = app.bundleURL
      else {
        return false
      }
      return bundleURL.standardizedFileURL.resolvingSymlinksInPath().path == destinationPath
    }

    guard !apps.isEmpty else { return }
    for app in apps {
      app.terminate()
    }
    waitForInstalledApps(apps, timeout: 3.0)
    for app in apps where !app.isTerminated {
      app.forceTerminate()
    }
    waitForInstalledApps(apps, timeout: 2.0)
  }

  private func waitForInstalledApps(_ apps: [NSRunningApplication], timeout: TimeInterval) {
    let deadline = Date().addingTimeInterval(timeout)
    while Date() < deadline {
      if apps.allSatisfy(\.isTerminated) {
        return
      }
      RunLoop.current.run(mode: .default, before: Date().addingTimeInterval(0.1))
    }
  }

  private func relaunchInstalledApp(_ destination: URL) -> Bool {
    do {
      try AppInstallMover().relaunch(destinationBundleURL: destination)
      NSApp.terminate(nil)
      return true
    } catch {
      presentInstallError(error)
      return false
    }
  }

  private func presentInstallError(_ error: Error) {
    let alert = NSAlert()
    alert.messageText = "Could not move 1Context to Applications."
    alert.informativeText = error.localizedDescription
    alert.icon = loadFishAlertIcon()
    alert.addButton(withTitle: "OK")
    NSApp.setActivationPolicy(.regular)
    NSApp.activate(ignoringOtherApps: true)
    alert.runModal()
  }

  func applicationDidFinishLaunching(_ notification: Notification) {
    guard !handleAppInstallAtLaunch() else { return }
    guard acquireMenuInstanceLock() else {
      NSApp.terminate(nil)
      return
    }
    registerMenuLaunchAgent()
    NSApp.setActivationPolicy(.accessory)
    statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.squareLength)
    configureStatusIcon()
    configureMenu()
    startAppUpdater()
    presentPostInstallUpdateMessageIfNeeded()
    refreshMenuItems()
    scheduleLocalWebSetupRepairPrompt()
    refreshApplicationLifecycle(userInitiated: false, force: true)
    scheduleLocalWebEdgeStartupRetries()

    timer = Timer.scheduledTimer(withTimeInterval: 30, repeats: true) { [weak self] _ in
      Task { @MainActor in
        self?.refreshApplicationLifecycle(userInitiated: false, force: true)
      }
    }
  }

  private func configureStatusIcon() {
    guard let button = statusItem.button else { return }
    if let image = loadMenuIcon() {
      image.isTemplate = true
      image.size = NSSize(width: 18, height: 18)
      button.image = image
      button.toolTip = Constants.appName
    } else {
      button.title = "1C"
    }
  }

  private func presentPostInstallUpdateMessageIfNeeded(
    defaults: UserDefaults = .standard,
    currentVersion: String = oneContextVersion,
    policy: UpdateUserFacingPolicy = SparkleUpdaterConfiguration.current().userFacingPolicy
  ) {
    let previousVersion = defaults.string(forKey: Constants.lastLaunchedVersionDefaultsKey)
    let lastShownVersion = defaults.string(forKey: Constants.lastShownPostInstallVersionDefaultsKey)
    defaults.set(currentVersion, forKey: Constants.lastLaunchedVersionDefaultsKey)

    guard let message = PostInstallUpdateMessageGate.message(
      currentVersion: currentVersion,
      previousVersion: previousVersion,
      lastShownVersion: lastShownVersion,
      policy: policy
    ) else {
      return
    }

    let alert = NSAlert()
    alert.messageText = message.title
    alert.informativeText = message.body
    alert.icon = loadFishAlertIcon()
    alert.addButton(withTitle: "OK")
    NSApp.activate(ignoringOtherApps: true)
    alert.runModal()
    defaults.set(currentVersion, forKey: Constants.lastShownPostInstallVersionDefaultsKey)
  }

  private func loadMenuIcon() -> NSImage? {
    menuBarIconURL().flatMap(NSImage.init(contentsOf:))
  }

  private func configureMenu() {
    menu.autoenablesItems = false
    stateItem.isEnabled = false
    menu.addItem(stateItem)
    menu.addItem(openWikiItem)
    menu.addItem(refreshWikiItem)

    versionItem.isEnabled = false
    settingsMenu.addItem(versionItem)
    autoPublishNoLimitItem.representedObject = WikiAutomaticPublishCadence.noLimit.rawValue
    autoPublishOneMinuteItem.representedObject = WikiAutomaticPublishCadence.oneMinute.rawValue
    autoPublishThirtyMinutesItem.representedObject = WikiAutomaticPublishCadence.thirtyMinutes.rawValue
    autoPublishCadenceMenu.addItem(autoPublishNoLimitItem)
    autoPublishCadenceMenu.addItem(autoPublishOneMinuteItem)
    autoPublishCadenceMenu.addItem(autoPublishThirtyMinutesItem)
    autoPublishCadenceItem.submenu = autoPublishCadenceMenu
    settingsMenu.addItem(autoPublishCadenceItem)
    settingsMenu.addItem(setupItem)
    settingsMenu.addItem(aboutItem)
    settingsMenu.addItem(uninstallItem)
    settingsItem.submenu = settingsMenu
    menu.addItem(settingsItem)
    menu.addItem(updateItem)
    menu.addItem(quitItem)
    menu.delegate = self

    for item in [
      stateItem,
      openWikiItem,
      refreshWikiItem,
      settingsItem,
      versionItem,
      autoPublishCadenceItem,
      autoPublishNoLimitItem,
      autoPublishOneMinuteItem,
      autoPublishThirtyMinutesItem,
      setupItem,
      aboutItem,
      uninstallItem,
      updateItem,
      quitItem
    ] {
      item.target = self
      item.isEnabled = true
    }
    stateItem.isEnabled = false
    versionItem.isEnabled = false
    statusItem.menu = menu
  }

  private func startAppUpdater() {
    sparkleUpdater.onUpdateInformationChanged = { [weak self] in
      Task { @MainActor in
        await self?.refreshAppUpdateState()
      }
    }
    sparkleUpdater.checkForUpdatesInBackgroundOnLaunch()
    Task { @MainActor in
      await refreshAppUpdateState()
    }
  }

  private func refreshMenuItems() {
    let stateTitle = runtimeState.title
    if renderedStateTitle != stateTitle {
      stateItem.title = stateTitle
      renderedStateTitle = stateTitle
    }
    refreshWikiItem.title = isWikiRefreshInFlight ? "Refreshing Wiki..." : "Refresh Wiki"
    refreshWikiItem.isEnabled = !isWikiRefreshInFlight
    let setupReady = cachedRequiredSetupReady
    setupItem.title = isLocalWebSetupInFlight ? "Granting Setup..." : setupReady ? "Setup..." : "Finish Setup..."
    setupItem.isEnabled = true
    let cadence = OneContextAppSettings.wikiAutomaticPublishCadence(
      preferencesPath: RuntimePaths.current().preferencesPath
    )
    autoPublishCadenceItem.title = "Auto-Publish: \(cadence.title)"
    autoPublishNoLimitItem.state = cadence == .noLimit ? .on : .off
    autoPublishOneMinuteItem.state = cadence == .oneMinute ? .on : .off
    autoPublishThirtyMinutesItem.state = cadence == .thirtyMinutes ? .on : .off
    uninstallItem.title = isUninstallInFlight ? "Uninstalling 1Context..." : "Uninstall 1Context..."
    uninstallItem.isEnabled = !isUninstallInFlight

    let versionTitle = "Version \(appVersion)"
    if renderedVersionTitle != versionTitle {
      versionItem.title = versionTitle
      renderedVersionTitle = versionTitle
    }

    let updateTitle: String
    let updateAction: Selector
    switch updateState {
    case .upToDate:
      updateTitle = "Check for Updates"
      updateAction = #selector(checkForUpdatesNow)
    case .available:
      updateTitle = "Please Update"
      updateAction = #selector(openUpdateFlow)
    case .mandatoryAvailable:
      updateTitle = "Please Update"
      updateAction = #selector(openUpdateFlow)
    }

    if renderedUpdateTitle != updateTitle {
      updateItem.title = updateTitle
      renderedUpdateTitle = updateTitle
    }
    if renderedUpdateAction != updateAction {
      updateItem.action = updateAction
      renderedUpdateAction = updateAction
    }
  }

  private func setRuntimeState(_ newValue: RuntimeState, forceRender: Bool = false) {
    guard runtimeState != newValue else {
      if forceRender {
        refreshMenuItems()
      }
      return
    }
    runtimeState = newValue
    refreshMenuItems()
  }

  private func setUpdateState(_ newValue: UpdateState) {
    guard updateState != newValue else { return }
    if isMenuOpen {
      pendingUpdateState = newValue
      return
    }
    updateState = newValue
    refreshMenuItems()
  }

  private func refreshAppUpdateState() async {
    let snapshot = await sparkleUpdater.snapshot(currentVersion: appVersion)
    setUpdateState(Self.updateState(for: snapshot))
  }

  private nonisolated static func updateState(for snapshot: AppUpdateSnapshot) -> UpdateState {
    if snapshot.mandatoryUpdateAvailable {
      return .mandatoryAvailable
    }
    return snapshot.updateAvailable ? .available : .upToDate
  }

  private func presentMenuAlert(_ message: String) {
    let now = Date()
    if activeAlertMessage != nil { return }
    if let last = lastAlertShownAt[message], now.timeIntervalSince(last) < 30 {
      return
    }
    activeAlertMessage = message
    lastAlertShownAt[message] = now
    showFishAlert(message)
    activeAlertMessage = nil
  }

  func menuWillOpen(_ menu: NSMenu) {
    isMenuOpen = true
    renderGeneration += 1
    _ = refreshRequiredSetupCache()
    refreshMenuItems()
  }

  func menuDidClose(_ menu: NSMenu) {
    isMenuOpen = false
    guard pendingUpdateState != nil else {
      return
    }
    renderGeneration += 1
    let generation = renderGeneration
    DispatchQueue.main.async { [weak self] in
      guard let self else { return }
      guard !isMenuOpen, generation == renderGeneration else { return }
      if let pendingUpdateState {
        updateState = pendingUpdateState
        self.pendingUpdateState = nil
      }
      refreshMenuItems()
    }
  }

  private func ensureRuntimeRunning(
    userInitiated: Bool,
    force: Bool = false,
    requiredSetupReady: Bool? = nil
  ) {
    guard !isRepairingRuntime else { return }
    let setupReady = requiredSetupReady ?? refreshRequiredSetupCache()
    if !setupReady {
      setRuntimeState(.needsSetup)
    }
    if !userInitiated && !force && !shouldRefreshRuntimeState() {
      return
    }
    isRepairingRuntime = true
    lastRuntimeRefreshStartedAt = Date()

    Task.detached(priority: .utility) {
      let controller = RuntimeController()
      defer {
        Task { @MainActor in
          self.isRepairingRuntime = false
        }
      }

      do {
        let health = try autoreleasepool {
          try UnixJSONRPCClient().health()
        }
        guard health.version == oneContextVersion else {
          let restarted = try await controller.restart(startMenu: false)
          await MainActor.run {
            self.applyRuntimeHealth(restarted, fallbackSetupReady: setupReady)
          }
          return
        }
        await MainActor.run {
          self.applyRuntimeHealth(health, fallbackSetupReady: setupReady)
        }
        return
      } catch let error as UnixSocketError {
        switch error {
        case .connectFailed, .emptyResponse:
          do {
            let started = try await controller.start(startMenu: false)
            await MainActor.run {
              self.applyRuntimeHealth(started.health, fallbackSetupReady: setupReady)
            }
          } catch {
            await MainActor.run {
              self.setRuntimeState(.needsAttention)
            }
          }
        default:
          await self.markRuntimeNeedsAttention()
        }
      } catch {
        await self.markRuntimeNeedsAttention()
      }
    }
  }

  private func applyRuntimeHealth(_ health: RuntimeHealth, fallbackSetupReady: Bool) {
    let runtimeSetupReady = health.requiredSetupReady ?? fallbackSetupReady
    if runtimeSetupReady {
      setRuntimeState(.running)
      startLocalWebEdge(requiredSetupReady: true)
    } else {
      setRuntimeState(.needsSetup)
    }
  }

  private func shouldRefreshRuntimeState() -> Bool {
    guard let lastRuntimeRefreshStartedAt else { return true }
    return Date().timeIntervalSince(lastRuntimeRefreshStartedAt) >= Constants.runtimeRefreshMinimumInterval
  }

  private func markRuntimeNeedsAttention() {
    setRuntimeState(.needsAttention)
  }

  @objc private func openUpdateFlow() {
    Task {
      await runAppUpdateFlow()
    }
  }

  @objc private func checkForUpdatesNow() {
    guard !isCheckingForUpdates else { return }
    isCheckingForUpdates = true

    Task { @MainActor in
      defer {
        self.isCheckingForUpdates = false
        self.refreshMenuItems()
      }

      let snapshot = await self.sparkleUpdater.snapshot(currentVersion: self.appVersion)
      self.setUpdateState(Self.updateState(for: snapshot))
      guard snapshot.availability == .available else {
        self.presentAppUpdateSnapshot(snapshot)
        return
      }
      if !self.sparkleUpdater.checkForUpdates(self.updateItem) {
        self.presentAppUpdateSnapshot(snapshot)
      }
    }
  }

  private func runAppUpdateFlow(snapshot existingSnapshot: AppUpdateSnapshot? = nil) async {
    let snapshot: AppUpdateSnapshot
    if let existingSnapshot {
      snapshot = existingSnapshot
    } else {
      snapshot = await sparkleUpdater.snapshot(currentVersion: appVersion)
    }
    if snapshot.availability == .available {
      if snapshot.mandatoryUpdateAvailable, sparkleUpdater.checkForUpdatesAutomatically() {
        return
      }
      if sparkleUpdater.checkForUpdates(updateItem) {
        return
      }
    }
    presentAppUpdateSnapshot(snapshot)
  }

  private func presentAppUpdateSnapshot(_ snapshot: AppUpdateSnapshot) {
    setUpdateState(Self.updateState(for: snapshot))
    if snapshot.availability == .notConfigured {
      presentMenuAlert(snapshot.userFacingStatus)
      return
    }
    presentMenuAlert(snapshot.userFacingStatus)
  }

  @objc private func showAbout() {
    NSWorkspace.shared.open(oneContextGitHubURL)
  }

  @objc private func showSetup() {
    showSetupWindow(message: nil)
  }

  @objc private func setWikiAutoPublishCadence(_ sender: NSMenuItem) {
    guard let rawValue = sender.representedObject as? String,
      let cadence = WikiAutomaticPublishCadence.parse(rawValue)
    else {
      return
    }
    do {
      try OneContextAppSettings.setWikiAutomaticPublishCadence(
        cadence,
        preferencesPath: RuntimePaths.current().preferencesPath
      )
      refreshMenuItems()
    } catch {
      presentMenuAlert("Could not save wiki auto-publish setting.")
    }
  }

  private func showSetupWindow(message: String?) {
    updateSetupWindow(message: message)
    setupWindowController.showWindow(nil)
    setupWindowController.window?.makeKeyAndOrderFront(nil)
    NSApp.activate(ignoringOtherApps: true)
    startSetupReadinessObservation(message: message)
  }

  private func showSetupWindow(forBlockedAction action: OneContextBlockedSetupAction) {
    showSetupWindow(message: setupContinuation.block(action))
  }

  private func updateSetupWindow(message: String? = nil) {
    let appSetup = currentReadiness().setup
    setupWindowController.render(
      appSetup,
      isGrantingLocalWikiAccess: isLocalWebSetupInFlight,
      message: message
    )
  }

  private func reconcileSetupFromUserRefresh() {
    startSetupReadinessObservation(message: "Checking permission grants.")
  }

  @objc private func openWiki() {
    guard currentReadiness().requiredSetupReady else {
      showSetupWindow(forBlockedAction: .openWiki)
      return
    }
    openWikiItem.isEnabled = false
    Task {
      do {
        let snapshot = try await ensureLocalWebEdgeForOpen()
        let urlString = snapshot.url.isEmpty ? LocalWebDefaults.defaultWikiURL : snapshot.url
        guard let url = URL(string: urlString) else {
          throw MenuError.openWikiFailed(urlString)
        }
        try Self.openURLInDefaultBrowser(url)
        recordWikiURL(url.absoluteString)
        openWikiItem.isEnabled = true

        Task {
          do {
            let snapshot = try await ensureWikiOpen()
            recordWikiURL(snapshot.url)
          } catch {
            // Opening the last-good site is the user-visible action. Runtime
            // warm-up failures are reflected in status/diagnose.
          }
        }
      } catch {
        openWikiItem.isEnabled = true
        if Self.isLocalWebSetupRequired(error) {
          showSetupWindow(forBlockedAction: .openWiki)
          return
        }
        presentMenuAlert(Self.wikiBlockedMessage(for: error))
      }
    }
  }

  private nonisolated static func openURLInDefaultBrowser(_ url: URL) throws {
    let process = Process()
    process.executableURL = URL(fileURLWithPath: "/usr/bin/open")
    process.arguments = [url.absoluteString]
    try process.run()
    process.waitUntilExit()
    if process.terminationStatus != 0 {
      throw MenuError.openWikiFailed(url.absoluteString)
    }
  }

  private nonisolated static func wikiBlockedMessage(for error: Error) -> String {
    if let localWebError = error as? LocalWebError,
      case .setupRequired(let message) = localWebError
    {
      return "1Context needs setup before the wiki can open. \(message) Use Settings > Setup... to continue."
    }
    return "Could not open 1Context wiki."
  }

  private nonisolated static func isLocalWebSetupRequired(_ error: Error) -> Bool {
    guard let localWebError = error as? LocalWebError,
      case .setupRequired = localWebError
    else {
      return false
    }
    return true
  }

  @objc private func refreshWiki() {
    guard !isWikiRefreshInFlight else { return }
    guard currentReadiness().requiredSetupReady else {
      showSetupWindow(forBlockedAction: .refreshWiki)
      return
    }
    isWikiRefreshInFlight = true
    refreshMenuItems()
    Task {
      do {
        _ = try await RuntimeController().start(startMenu: false)
        _ = try await wikiRPC("wiki.refresh", timeout: 5)
        _ = try await waitForWikiRunning(timeout: 240)
        await MainActor.run {
          self.isWikiRefreshInFlight = false
          self.refreshMenuItems()
        }
      } catch {
        await MainActor.run {
          self.isWikiRefreshInFlight = false
          self.refreshMenuItems()
          self.presentMenuAlert(Self.wikiBlockedMessage(for: error))
        }
      }
    }
  }

  private enum UninstallChoice {
    case keepData
    case deleteData
  }

  @objc private func uninstallOneContext() {
    guard !isUninstallInFlight else { return }
    guard let choice = confirmUninstall() else { return }
    if choice == .deleteData, !confirmDeleteDataForUninstall() {
      return
    }

    isUninstallInFlight = true
    refreshMenuItems()

    var arguments = ["uninstall", "--menu-process"]
    if choice == .deleteData {
      arguments.append("--delete-data")
    }
    Task.detached(priority: .userInitiated) {
      let result = Self.runBundledCLI(arguments: arguments)
      await MainActor.run {
        self.isUninstallInFlight = false
        self.refreshMenuItems()
        if result.status == 0 {
          self.presentMenuAlert("\(self.appIdentity.displayName) was moved to Trash.")
          NSApp.terminate(nil)
        } else {
          self.presentMenuAlert(Self.uninstallFailureMessage(result))
        }
      }
    }
  }

  private func confirmUninstall() -> UninstallChoice? {
    let alert = NSAlert()
    alert.messageText = "Uninstall \(appIdentity.displayName)?"
    alert.informativeText = "This moves \(appIdentity.displayName) to Trash and removes background services and Local Wiki Access. Your wiki content stays unless you choose Delete Data."
    alert.icon = loadFishAlertIcon()
    alert.addButton(withTitle: "Uninstall")
    alert.addButton(withTitle: "Cancel")
    alert.addButton(withTitle: "Delete Data")
    NSApp.activate(ignoringOtherApps: true)

    switch alert.runModal() {
    case .alertFirstButtonReturn:
      return .keepData
    case .alertThirdButtonReturn:
      return .deleteData
    default:
      return nil
    }
  }

  private func confirmDeleteDataForUninstall() -> Bool {
    let alert = NSAlert()
    alert.messageText = "Delete \(appIdentity.displayName) Data?"
    alert.informativeText = "This removes app support files, logs, caches, and \(RuntimePaths.current().userContentDirectory.path) content owned by \(appIdentity.displayName)."
    alert.icon = loadFishAlertIcon()
    alert.addButton(withTitle: "Delete Data")
    alert.addButton(withTitle: "Cancel")
    NSApp.activate(ignoringOtherApps: true)
    return alert.runModal() == .alertFirstButtonReturn
  }

  private nonisolated static func runBundledCLI(arguments: [String]) -> ProcessCaptureResult {
    guard let executableURL = bundledCLIURL() else {
      return ProcessCaptureResult(status: 1, stdout: "", stderr: "Bundled 1context-cli was not found.")
    }

    let process = Process()
    process.executableURL = executableURL
    process.arguments = arguments
    let stdout = Pipe()
    let stderr = Pipe()
    process.standardOutput = stdout
    process.standardError = stderr

    do {
      try process.run()
      process.waitUntilExit()
    } catch {
      return ProcessCaptureResult(status: 1, stdout: "", stderr: error.localizedDescription)
    }

    let stdoutData = stdout.fileHandleForReading.readDataToEndOfFile()
    let stderrData = stderr.fileHandleForReading.readDataToEndOfFile()
    return ProcessCaptureResult(
      status: process.terminationStatus,
      stdout: String(data: stdoutData, encoding: .utf8) ?? "",
      stderr: String(data: stderrData, encoding: .utf8) ?? ""
    )
  }

  private nonisolated static func bundledCLIURL() -> URL? {
    let url = Bundle.main.bundleURL.appendingPathComponent("Contents/MacOS/1context-cli")
    return FileManager.default.isExecutableFile(atPath: url.path) ? url : nil
  }

  private nonisolated static func uninstallFailureMessage(_ result: ProcessCaptureResult) -> String {
    let detail = [result.stderr, result.stdout]
      .map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
      .first { !$0.isEmpty }
    return "Could not uninstall 1Context. \(detail ?? "Open 1context diagnose for details.")"
  }

  @objc private func quit() {
    timer?.invalidate()
    timer = nil
    let localWeb = self.localWeb
    if let statusItem {
      NSStatusBar.system.removeStatusItem(statusItem)
    }
    Task.detached {
      _ = try? await RuntimeController().stopForAppQuit()
      localWeb.stop()
      await MainActor.run {
        NSApp.terminate(nil)
      }
    }
  }

  func applicationWillTerminate(_ notification: Notification) {
    stopSetupReadinessPolling()
    localWeb.stop()
  }

  private func startLocalWebEdge(requiredSetupReady: Bool? = nil) {
    guard requiredSetupReady ?? cachedRequiredSetupReady else {
      setRuntimeState(.needsSetup)
      return
    }
    localWebQueue.async { [localWeb] in
      AppDelegate.startLocalWebEdge(localWeb)
    }
  }

  private func scheduleLocalWebEdgeStartupRetries() {
    for delay in Constants.localWebStartupRetryDelays {
      DispatchQueue.main.asyncAfter(deadline: .now() + delay) { [weak self] in
        self?.startLocalWebEdge()
      }
    }
  }

  private func scheduleLocalWebSetupRepairPrompt() {
    DispatchQueue.main.asyncAfter(deadline: .now() + 1.0) { [weak self] in
      guard let self else { return }
      Task { @MainActor in
        await self.offerLocalWebSetupRepairAtLaunch()
      }
    }
  }

  private func offerLocalWebSetupRepairAtLaunch() async {
    guard !didOfferLocalWebSetupAtLaunch else { return }
    guard !currentReadiness().requiredSetupReady else { return }
    didOfferLocalWebSetupAtLaunch = true
    showSetupWindow(message: "Finish setup to start 1Context Remembering.")
  }

  private func requestScreenRecordingAccess() {
    let readiness = currentReadiness()
    if readiness.setup.rememberingPermissions.screenRecording.status == .needsRelaunch {
      relaunchCurrentApp()
      return
    }

    startSetupReadinessObservation(message: "Grant Screen & System Audio Recording in System Settings.")
    let preferencesPath = RuntimePaths.current().preferencesPath
    let subject = OneContextSystemPermissions.currentPermissionSubject()
    let request = Task.detached(priority: .userInitiated) {
      await OneContextSystemPermissions.requestScreenAndSystemAudio(
        preferencesPath: preferencesPath,
        subject: subject
      )
    }
    Task { [weak self] in
      let outcome = await request.value
      guard let self else { return }
      switch outcome {
      case .granted:
        self.observeRememberingPermissionGrant(message: "Screen & System Audio Recording is ready.")
      case .needsRelaunch:
        self.observeRememberingPermissionGrant(message: "Relaunch 1Context to finish Screen & System Audio Recording.")
      case .required:
        self.openSystemSettingsPane(OneContextSystemPermissions.screenRecordingSettingsURL)
        self.observeRememberingPermissionGrant(message: "Grant Screen & System Audio Recording in System Settings.")
      case .unavailable(let detail):
        self.openSystemSettingsPane(OneContextSystemPermissions.screenRecordingSettingsURL)
        self.observeRememberingPermissionGrant(message: "Screen & System Audio Recording could not be proved. \(detail)")
      }
    }
  }

  private func requestAccessibilityAccess() {
    let granted = OneContextSystemPermissions.requestAccessibility()
    if !granted {
      openSystemSettingsPane(OneContextSystemPermissions.accessibilitySettingsURL)
    }
    observeRememberingPermissionGrant(message: "Grant Accessibility in System Settings.")
  }

  private func requestInputMonitoringAccess() {
    let granted = OneContextSystemPermissions.requestInputMonitoring()
    openSystemSettingsPane(OneContextSystemPermissions.inputMonitoringSettingsURL)
    observeRememberingPermissionGrant(
      message: granted
        ? "Input Monitoring is ready."
        : "Add 1Context in Input Monitoring, enable it, then return here."
    )
  }

  private func requestBrowserExtensionAccess() {
    if let url = URL(string: "https://chromewebstore.google.com/search/1context") {
      try? Self.openURLInDefaultBrowser(url)
    }
    observeRememberingPermissionGrant(message: "Install the 1Context browser extension and approve its requested browser permissions.")
  }

  private func requestMicrophoneAccess() {
    let preferencesPath = RuntimePaths.current().preferencesPath
    let subject = OneContextSystemPermissions.currentPermissionSubject()
    OneContextSystemPermissions.requestMicrophone(
      preferencesPath: preferencesPath,
      subject: subject
    ) { [weak self] granted in
      Task { @MainActor in
        guard let self else { return }
        if !granted {
          self.openSystemSettingsPane(OneContextSystemPermissions.microphoneSettingsURL)
        }
        self.observeRememberingPermissionGrant(message: "Grant Microphone in System Settings.")
      }
    }
  }

  private func configureAutomationAccess() {
    let preferencesPath = RuntimePaths.current().preferencesPath
    let availableTargets = OneContextSystemPermissions.installedAutomationTargets()
    guard !availableTargets.isEmpty else {
      showSetupWindow(message: "No supported Automation apps are installed yet.")
      return
    }

    let selectedIdentifiers = Set(
      OneContextSystemPermissions.selectedAutomationTargetBundleIdentifiers(preferencesPath: preferencesPath)
    )
    let alert = NSAlert()
    alert.messageText = "Choose Automation Apps"
    alert.informativeText = "1Context will ask macOS for control of only these apps."
    alert.icon = loadFishAlertIcon()
    alert.addButton(withTitle: "Continue")
    alert.addButton(withTitle: "Cancel")

    let stack = NSStackView()
    stack.orientation = .vertical
    stack.alignment = .leading
    stack.spacing = 6
    stack.translatesAutoresizingMaskIntoConstraints = false

    var options: [(button: NSButton, target: OneContextAutomationTarget)] = []
    for target in availableTargets {
      let button = NSButton(checkboxWithTitle: target.name, target: nil, action: nil)
      button.state = selectedIdentifiers.contains(target.bundleIdentifier) ? .on : .off
      stack.addArrangedSubview(button)
      options.append((button, target))
    }
    alert.accessoryView = stack

    NSApp.activate(ignoringOtherApps: true)
    guard alert.runModal() == .alertFirstButtonReturn else {
      observeRememberingPermissionGrant(message: "Choose Automation apps to continue.")
      return
    }

    let chosen = options
      .filter { $0.button.state == .on }
      .map(\.target.bundleIdentifier)
    do {
      try OneContextSystemPermissions.setAutomationTargetBundleIdentifiers(
        chosen,
        preferencesPath: preferencesPath
      )
      requestAutomationAccess()
    } catch {
      showSetupWindow(message: error.localizedDescription)
    }
  }

  private func requestAutomationAccess() {
    startSetupReadinessObservation(message: "Grant Automation in System Settings.")
    let preferencesPath = RuntimePaths.current().preferencesPath
    let subject = OneContextSystemPermissions.currentPermissionSubject()
    let request = Task.detached(priority: .userInitiated) {
      OneContextSystemPermissions.requestAutomation(
        preferencesPath: preferencesPath,
        subject: subject
      )
    }
    Task { [weak self] in
      let granted = await request.value
      guard let self else { return }
      if granted {
        self.observeRememberingPermissionGrant(message: "Automation is ready.")
      } else {
        self.openSystemSettingsPane(OneContextSystemPermissions.automationSettingsURL)
        self.observeRememberingPermissionGrant(message: "Grant Automation in System Settings.")
      }
    }
  }

  private func requestFullDiskAccess() {
    openSystemSettingsPane(OneContextSystemPermissions.fullDiskAccessSettingsURL)
    observeRememberingPermissionGrant(message: "Grant Full Disk Access in System Settings, then restart 1Context if macOS asks.")
  }

  private func relaunchCurrentApp() {
    do {
      try AppInstallMover().relaunch(destinationBundleURL: Bundle.main.bundleURL)
      NSApp.terminate(nil)
    } catch {
      showSetupWindow(message: "Could not relaunch 1Context. \(error.localizedDescription)")
    }
  }

  private func openSystemSettingsPane(_ rawURL: String) {
    guard let url = URL(string: rawURL) else { return }
    NSWorkspace.shared.open(url)
  }

  private func observeRememberingPermissionGrant(message: String) {
    let readiness = currentReadiness()
    if readiness.requiredSetupReady {
      completeRequiredSetup(readiness: readiness, message: "1Context setup is ready.")
      return
    }
    startSetupReadinessObservation(message: message)
  }

  private func runLocalWebSetupFlow() async -> Bool {
    if isLocalWebSetupInFlight {
      pollSetupReadiness()
      return false
    }

    let current = currentReadiness()
    guard !current.setup.localWikiAccess.ready else {
      return finishRequiredSetupStep(readiness: current, message: "Local Wiki Access is ready.")
    }

    startSetupReadinessPolling(message: "Grant Local Wiki Access in the macOS prompt.")
    do {
      let result = try await Task.detached(priority: .userInitiated) {
        try LocalWebSetupInstaller().install()
      }.value
      let readiness = currentReadiness()
      if let localWeb = result.localWeb {
        recordWikiURL(localWeb.url)
      } else {
        recordWikiURL(result.setup.targetURL)
      }
      return finishRequiredSetupStep(readiness: readiness, message: "Local Wiki Access is ready.")
    } catch {
      let readiness = currentReadiness()
      if readiness.requiredSetupReady {
        completeRequiredSetup(readiness: readiness, message: "1Context setup is ready.")
        return true
      }

      if let recovery = localWebSetupRecovery(for: error), recovery.keepWaiting {
        startSetupReadinessPolling(message: recovery.message)
        showSetupWindow(message: recovery.message)
        return false
      }

      stopSetupReadinessPolling()
      isLocalWebSetupInFlight = false
      refreshMenuItems()
      if let recovery = localWebSetupRecovery(for: error) {
        updateSetupWindow(message: recovery.message)
        showSetupWindow(message: recovery.message)
        return false
      }
      let message = "Could not finish setup. \(error.localizedDescription)"
      updateSetupWindow(message: message)
      showSetupWindow(message: message)
      return false
    }
  }

  private func finishRequiredSetupStep(readiness: OneContextAppReadinessSnapshot, message: String) -> Bool {
    if readiness.requiredSetupReady {
      completeRequiredSetup(readiness: readiness, message: "1Context setup is ready.")
      return true
    }

    isLocalWebSetupInFlight = false
    cachedRequiredSetupReady = readiness.requiredSetupReady
    refreshMenuItems()
    let nextMessage = readiness.setup.localWikiAccess.ready ? readiness.requiredSetupSummary : message
    startSetupReadinessObservation(message: nextMessage)
    return false
  }

  private struct LocalWebSetupRecovery {
    let message: String
    let keepWaiting: Bool
  }

  private func startSetupReadinessPolling(message: String) {
    isLocalWebSetupInFlight = true
    startSetupReadinessObservation(message: message)
  }

  private func startSetupReadinessObservation(message: String?) {
    let current = currentReadiness()
    if current.requiredSetupReady {
      completeRequiredSetup(readiness: current, message: message ?? "1Context setup is ready.")
      return
    }

    setupReadinessPollingStartedAt = Date()
    setupReadinessPollingMessage = message
    refreshMenuItems()
    updateSetupWindow(message: message)
    setupReadinessTimer?.invalidate()
    setupReadinessTimer = Timer.scheduledTimer(withTimeInterval: Constants.setupReadinessPollInterval, repeats: true) { [weak self] _ in
      Task { @MainActor in
        self?.pollSetupReadiness()
      }
    }
    pollSetupReadiness()
  }

  private func stopSetupReadinessPolling() {
    setupReadinessTimer?.invalidate()
    setupReadinessTimer = nil
    setupReadinessPollingStartedAt = nil
    setupReadinessPollingMessage = nil
    isSetupReadinessCheckInFlight = false
  }

  private func pollSetupReadiness() {
    guard isLocalWebSetupInFlight || setupWindowController.window?.isVisible == true else {
      stopSetupReadinessPolling()
      return
    }
    if let startedAt = setupReadinessPollingStartedAt,
      isLocalWebSetupInFlight,
      Date().timeIntervalSince(startedAt) > Constants.setupReadinessPollTimeout
    {
      stopSetupReadinessPolling()
      isLocalWebSetupInFlight = false
      refreshMenuItems()
      updateSetupWindow(message: "Allow 1Context in System Settings, then return here.")
      return
    }
    guard !isSetupReadinessCheckInFlight else { return }
    isSetupReadinessCheckInFlight = true
    let message = setupReadinessPollingMessage

    Task.detached(priority: .utility) {
      await Self.refreshRuntimePermissionProofsIfAuthorized()
      let readiness = Self.computeReadiness()
      await MainActor.run {
        self.isSetupReadinessCheckInFlight = false
        self.cachedRequiredSetupReady = readiness.requiredSetupReady
        guard readiness.requiredSetupReady else {
          self.setupWindowController.render(
            readiness.setup,
            isGrantingLocalWikiAccess: self.isLocalWebSetupInFlight,
            message: message
          )
          return
        }
        self.completeRequiredSetup(readiness: readiness, message: "1Context setup is ready.")
      }
    }
  }

  private func completeRequiredSetup(readiness: OneContextAppReadinessSnapshot, message: String) {
    stopSetupReadinessPolling()
    isLocalWebSetupInFlight = false
    cachedRequiredSetupReady = readiness.requiredSetupReady
    recordWikiURL(readiness.setup.localWikiAccess.targetURL)
    refreshMenuItems()
    updateSetupWindow(message: message)
    startRuntimeImmediatelyAfterSetup()
    refreshApplicationLifecycle(userInitiated: false, force: true)
    resumeBlockedSetupActionAfterSetup()
  }

  private func resumeBlockedSetupActionAfterSetup() {
    guard let action = setupContinuation.consumeResumableActionAfterSetup() else {
      return
    }
    Task { @MainActor in
      switch action {
      case .openWiki:
        self.openWiki()
      case .refreshWiki:
        self.refreshWiki()
      }
    }
  }

  private func startRuntimeImmediatelyAfterSetup() {
    setRuntimeState(.running, forceRender: true)
    startLocalWebEdge(requiredSetupReady: true)
    Task.detached(priority: .userInitiated) {
      do {
        try await RuntimeController().requestStart(startMenu: false)
      } catch {
        await MainActor.run {
          self.markRuntimeNeedsAttention()
        }
      }
    }
  }

  private func localWebSetupRecovery(for error: Error) -> LocalWebSetupRecovery? {
    guard let setupError = error as? LocalWebSetupInstallerError else {
      return nil
    }
    switch setupError {
    case .backgroundItemRequiresApproval:
      return LocalWebSetupRecovery(
        message: "Allow 1Context in System Settings, then return here.",
        keepWaiting: true
      )
    case .certificateTrustFailed:
      return LocalWebSetupRecovery(
        message: "Use Touch ID or your password to trust the local 1Context certificate.",
        keepWaiting: false
      )
    case .setupStillIncomplete:
      return LocalWebSetupRecovery(
        message: "Waiting for macOS to finish Local Wiki Access.",
        keepWaiting: true
      )
    case .appInstallRequired:
      return LocalWebSetupRecovery(
        message: "Install 1Context in Applications, then grant Local Wiki Access.",
        keepWaiting: false
      )
    case .invalidCertificate, .proxyExecutableMissing, .serviceRegistrationFailed, .rootUserUnsupported:
      return nil
    }
  }

  nonisolated private static func startLocalWebEdge(_ localWeb: CaddyManager) {
    do {
      let current = localWeb.status()
      if !current.running {
        _ = try localWeb.start()
      }
    } catch {
      recordLocalWebStartFailure(error)
    }
  }

  nonisolated private static func recordLocalWebStartFailure(_ error: Error) {
    let paths = RuntimePaths.current()
    let log = paths.logDirectory.appendingPathComponent("menu.log")
    let line = "[\(ISO8601DateFormatter().string(from: Date()))] local-web.start failed: \(error.localizedDescription)\n"
    do {
      try RuntimePermissions.ensurePrivateDirectory(paths.logDirectory)
      if !FileManager.default.fileExists(atPath: log.path) {
        FileManager.default.createFile(atPath: log.path, contents: nil)
        chmod(log.path, RuntimePermissions.privateFileMode)
      }
      let handle = try FileHandle(forWritingTo: log)
      try handle.seekToEnd()
      try handle.write(contentsOf: Data(line.utf8))
      try handle.close()
    } catch {
      // Menu bar logging must never affect startup.
    }
  }

  private func ensureLocalWebEdgeForOpen() async throws -> LocalWebSnapshot {
    try await Task.detached { [localWeb] in
      let current = localWeb.status()
      if current.running {
        return current
      }
      return try localWeb.start()
    }.value
  }

  private func ensureWikiOpen() async throws -> WikiMenuSnapshot {
    _ = try await RuntimeController().start(startMenu: false)
    if let snapshot = try? await wikiRPC("wiki.status", timeout: 5), snapshot.running {
      recordWikiURL(snapshot.url)
      return snapshot
    }
    _ = try await wikiRPC("wiki.start", timeout: 5)
    let snapshot = try await waitForWikiRunning(timeout: 240)
    recordWikiURL(snapshot.url)
    return snapshot
  }

  private func waitForWikiRunning(timeout: TimeInterval) async throws -> WikiMenuSnapshot {
    let deadline = Date().addingTimeInterval(timeout)
    var last = WikiMenuSnapshot(running: false, url: LocalWebDefaults.defaultWikiURL, health: "starting")
    repeat {
      last = try await wikiRPC("wiki.status", timeout: 5)
      if last.running { return last }
      try await Task.sleep(nanoseconds: 500_000_000)
    } while Date() < deadline
    throw MenuError.wikiTimedOut(last.health)
  }

  private func recordWikiURL(_ url: String) {
  }

  private func wikiRPC(_ method: String, timeout: TimeInterval) async throws -> WikiMenuSnapshot {
    let deadline = Date().addingTimeInterval(timeout)
    var lastError: Error?
    let clientTimeout = Int32(max(2_000, min(120_000, Int(timeout * 1_000))))
    repeat {
      do {
        let payload = try UnixJSONRPCClient(timeoutMilliseconds: clientTimeout).call(method: method)
        return WikiMenuSnapshot(payload: payload)
      } catch {
        lastError = error
        try await Task.sleep(nanoseconds: 250_000_000)
      }
    } while Date() < deadline
    throw lastError ?? MenuError.wikiTimedOut(method)
  }

}

runPermissionCapabilityProbeIfRequested()

private let app = NSApplication.shared
private let delegate = AppDelegate()
app.delegate = delegate
app.run()

private enum RuntimeState {
  case checking
  case running
  case needsSetup
  case needsAttention

  var title: String {
    switch self {
    case .checking:
      return "1Context"
    case .running:
      return "1Context Remembering"
    case .needsSetup:
      return "1Context Needs Setup"
    case .needsAttention:
      return "1Context Sick"
    }
  }
}

private enum UpdateState {
  case upToDate
  case available
  case mandatoryAvailable
}

private struct WikiMenuSnapshot {
  let running: Bool
  let url: String
  let health: String
  let renderState: String
  let renderLastStatus: String?
  let renderBackoffRemainingMilliseconds: Int

  init(running: Bool, url: String, health: String) {
    self.running = running
    self.url = url
    self.health = health
    self.renderState = health
    self.renderLastStatus = nil
    self.renderBackoffRemainingMilliseconds = 0
  }

  init(payload: [String: Any]) {
    self.running = payload["running"] as? Bool ?? false
    self.url = payload["url"] as? String ?? LocalWebDefaults.defaultWikiURL
    self.health = payload["health"] as? String ?? "unknown"
    let render = payload["render"] as? [String: Any] ?? [:]
    self.renderState = render["state"] as? String ?? self.health
    self.renderBackoffRemainingMilliseconds = render["backoff_remaining_ms"] as? Int ?? 0
    let last = render["last"] as? [String: Any]
    self.renderLastStatus = last?["status"] as? String
  }
}

private enum MenuError: Error, LocalizedError {
  case invalidWikiURL(String)
  case wikiTimedOut(String)
  case openWikiFailed(String)

  var errorDescription: String? {
    switch self {
    case .invalidWikiURL(let url):
      return "Invalid wiki URL: \(url)"
    case .wikiTimedOut(let health):
      return "Timed out preparing local wiki: \(health)"
    case .openWikiFailed(let url):
      return "Could not open wiki URL: \(url)"
    }
  }
}
