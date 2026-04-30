import AppKit
import Darwin
import Foundation
import OneContextAgent
import OneContextLocalWeb
import OneContextRuntimeSupport

private enum Constants {
  static let appName = "1Context"
  static let runtimeRefreshMinimumInterval: TimeInterval = 5
  static let localWebStartupRetryDelays: [TimeInterval] = [1, 3, 10]
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
  private var isRuntimeActionInFlight = false
  private var isWikiRefreshInFlight = false
  private var isMenuOpen = false
  private var pendingUpdateState: UpdateState?
  private var activeAlertMessage: String?
  private var lastAlertShownAt: [String: Date] = [:]
  private var renderGeneration = 0
  private var lastRuntimeRefreshStartedAt: Date?
  private var desiredStateSource: DispatchSourceFileSystemObject?
  private var desiredStateDescriptor: Int32 = -1
  private var desiredRuntimeIntent: RuntimeIntent = .running
  private let menu = NSMenu()
  private let stateItem = NSMenuItem(title: RuntimeState.checking.title, action: nil, keyEquivalent: "")
  private let startStopItem = NSMenuItem(title: "Stop", action: #selector(toggleRuntime), keyEquivalent: "")
  private let openWikiItem = NSMenuItem(title: "Open Wiki", action: #selector(openWiki), keyEquivalent: "")
  private let refreshWikiItem = NSMenuItem(title: "Refresh Wiki", action: #selector(refreshWiki), keyEquivalent: "")
  private let settingsItem = NSMenuItem(title: "Settings", action: nil, keyEquivalent: "")
  private let settingsMenu = NSMenu()
  private let versionItem = NSMenuItem(title: "", action: nil, keyEquivalent: "")
  private let aboutItem = NSMenuItem(title: "About 1Context", action: #selector(showAbout), keyEquivalent: "")
  private let updateItem = NSMenuItem(title: "", action: nil, keyEquivalent: "")
  private let quitItem = NSMenuItem(title: "Quit", action: #selector(quit), keyEquivalent: "q")
  private let appVersion = Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String
    ?? oneContextVersion
  private let perfLoggingEnabled = ProcessInfo.processInfo.environment["ONECONTEXT_MENU_PERF_LOG"] == "1"
  private let localWeb = CaddyManager()
  private let localWebQueue = DispatchQueue(label: "com.haptica.1context.menu.local-web")

  private var currentVersion: String {
    appVersion
  }

  func applicationDidFinishLaunching(_ notification: Notification) {
    let start = perfStart()
    NSApp.setActivationPolicy(.accessory)
    statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.squareLength)
    configureStatusIcon()
    configureMenu()
    refreshMenuItems()
    startDesiredStateMonitor()
    startLocalWebEdge()
    scheduleLocalWebEdgeStartupRetries()
    runLaunchChores()
    ensureRuntimeRunning(userInitiated: false, force: true)

    timer = Timer.scheduledTimer(withTimeInterval: 30, repeats: true) { [weak self] _ in
      Task { @MainActor in
        self?.ensureRuntimeRunning(userInitiated: false, force: true)
        self?.startLocalWebEdge()
      }
    }
    perfLog("launch.ready", start: start)
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

  private func loadMenuIcon() -> NSImage? {
    menuBarIconURL().flatMap(NSImage.init(contentsOf:))
  }

  private func configureMenu() {
    menu.autoenablesItems = false
    stateItem.isEnabled = false
    menu.addItem(stateItem)
    menu.addItem(startStopItem)
    menu.addItem(openWikiItem)
    menu.addItem(refreshWikiItem)

    versionItem.isEnabled = false
    settingsMenu.addItem(versionItem)
    settingsMenu.addItem(aboutItem)
    settingsItem.submenu = settingsMenu
    menu.addItem(settingsItem)
    menu.addItem(updateItem)
    menu.addItem(quitItem)
    menu.delegate = self

    for item in [stateItem, startStopItem, openWikiItem, refreshWikiItem, settingsItem, versionItem, aboutItem, updateItem, quitItem] {
      item.target = self
      item.isEnabled = true
    }
    stateItem.isEnabled = false
    versionItem.isEnabled = false
    statusItem.menu = menu
  }

  private func refreshMenuItems() {
    let start = perfStart()
    let stateTitle = runtimeState.title
    if renderedStateTitle != stateTitle {
      stateItem.title = stateTitle
      renderedStateTitle = stateTitle
    }
    startStopItem.title = runtimeState == .stopped ? "Start" : "Stop"
    startStopItem.isEnabled = !isRuntimeActionInFlight
    refreshWikiItem.title = isWikiRefreshInFlight ? "Refreshing Wiki..." : "Refresh Wiki"
    refreshWikiItem.isEnabled = !isWikiRefreshInFlight

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
      updateAction = #selector(openUpgradeCommand)
    }

    if renderedUpdateTitle != updateTitle {
      updateItem.title = updateTitle
      renderedUpdateTitle = updateTitle
    }
    if renderedUpdateAction != updateAction {
      updateItem.action = updateAction
      renderedUpdateAction = updateAction
    }
    perfLog("menu.render", start: start)
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
    let start = perfStart()
    isMenuOpen = true
    renderGeneration += 1
    perfLog("menu.willOpen", start: start)
  }

  func menuDidClose(_ menu: NSMenu) {
    let start = perfStart()
    isMenuOpen = false
    guard pendingUpdateState != nil else {
      perfLog("menu.didClose.noop", start: start)
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
      perfLog("menu.didClose.deferredRender", start: start)
    }
  }

  private func ensureRuntimeRunning(userInitiated: Bool, force: Bool = false) {
    guard !isRepairingRuntime else { return }
    guard userInitiated || desiredRuntimeIntent == .running else {
      setRuntimeState(.stopped)
      return
    }
    if !userInitiated && !force && !shouldRefreshRuntimeState() {
      perfLog("runtime.refresh.skipped")
      return
    }
    isRepairingRuntime = true
    lastRuntimeRefreshStartedAt = Date()

    Task.detached(priority: .utility) {
      let healthStart = await self.perfStart()
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
        let shouldShowRunning = await MainActor.run {
          self.desiredRuntimeIntent == .running
        }
        guard shouldShowRunning else {
          await MainActor.run {
            self.setRuntimeState(.stopped)
            self.perfLog("runtime.health.ignoredStoppedIntent", start: healthStart)
          }
          return
        }
        guard health.version == oneContextVersion else {
          _ = try await controller.restart(startMenu: false)
          await MainActor.run {
            self.perfLog("runtime.repair.versionMismatch", start: healthStart)
            self.setRuntimeState(.running)
          }
          return
        }
        await MainActor.run {
          self.perfLog("runtime.health.ok", start: healthStart)
          self.setRuntimeState(.running)
          self.startLocalWebEdge()
        }
        return
      } catch let error as UnixSocketError {
        switch error {
        case .connectFailed, .emptyResponse:
          guard userInitiated || controller.shouldAutoStartRuntime() else {
            await MainActor.run {
              self.setRuntimeState(.stopped)
            }
            return
          }
          do {
            _ = try await controller.start(startMenu: false)
            await MainActor.run {
              self.perfLog("runtime.start.ok", start: healthStart)
              self.setRuntimeState(.running)
              self.startLocalWebEdge()
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

  private func shouldRefreshRuntimeState() -> Bool {
    guard let lastRuntimeRefreshStartedAt else { return true }
    return Date().timeIntervalSince(lastRuntimeRefreshStartedAt) >= Constants.runtimeRefreshMinimumInterval
  }

  private func markRuntimeNeedsAttention() {
    setRuntimeState(.needsAttention)
  }

  private func startDesiredStateMonitor() {
    Task.detached(priority: .utility) {
      let intent = Self.readDesiredRuntimeIntentFromDisk()
      await MainActor.run {
        self.applyDesiredRuntimeIntent(intent)
      }
    }

    let paths = RuntimePaths.current()
    try? RuntimePermissions.ensurePrivateDirectory(paths.appSupportDirectory)
    let descriptor = open(paths.appSupportDirectory.path, O_EVTONLY)
    guard descriptor >= 0 else { return }

    desiredStateDescriptor = descriptor
    let source = DispatchSource.makeFileSystemObjectSource(
      fileDescriptor: descriptor,
      eventMask: [.write, .delete, .rename, .attrib, .extend],
      queue: DispatchQueue.main
    )
    source.setEventHandler { [weak self] in
      let intent = Self.readDesiredRuntimeIntentFromDisk()
      self?.applyDesiredRuntimeIntent(intent)
    }
    source.setCancelHandler { [descriptor] in
      close(descriptor)
    }
    desiredStateSource = source
    source.resume()
  }

  private func applyDesiredRuntimeIntent(_ intent: RuntimeIntent) {
    guard desiredRuntimeIntent != intent else { return }
    desiredRuntimeIntent = intent
    switch intent {
    case .running:
      if runtimeState == .stopped {
        setRuntimeState(.checking)
      } else {
        refreshMenuItems()
      }
      ensureRuntimeRunning(userInitiated: false)
    case .stopped:
      setRuntimeState(.stopped)
    }
  }

  nonisolated private static func readDesiredRuntimeIntentFromDisk() -> RuntimeIntent {
    let state = (try? String(contentsOfFile: RuntimePaths.current().desiredStatePath, encoding: .utf8))?
      .trimmingCharacters(in: .whitespacesAndNewlines)
    return state == "stopped" ? .stopped : .running
  }

  private func loadCachedUpdateState() {
    Task.detached(priority: .utility) {
      let start = await self.perfStart()
      let state = Self.readUpdateStateFromDisk()
      guard let version = state?["last_seen_latest"] as? String else {
        return
      }
      let currentVersion = self.appVersion
      let cachedState: UpdateState = compareVersions(version, currentVersion) > 0 ? .available : .upToDate
      await MainActor.run {
        self.perfLog("update.cache.read", start: start)
        self.setUpdateState(cachedState)
      }
    }
  }

  private func checkForUpdates(force: Bool) {
    guard !isCheckingForUpdates else { return }
    isCheckingForUpdates = true

    Task.detached(priority: .utility) {
      let start = await self.perfStart()
      defer {
        Task { @MainActor in
          self.isCheckingForUpdates = false
        }
      }

      do {
        guard force || Self.shouldCheckForUpdateFromDisk() else {
          await MainActor.run {
            self.perfLog("update.check.skipped", start: start)
          }
          return
        }
        let result = try await UpdateChecker().check(force: force, currentVersion: self.appVersion)
        await MainActor.run {
          self.perfLog("update.check.done", start: start)
          self.setUpdateState(result.updateAvailable ? .available : .upToDate)
        }
      } catch {
        // Update failures stay quiet. The menu remains usable offline.
      }
    }
  }

  nonisolated private static func shouldCheckForUpdateFromDisk() -> Bool {
    guard let state = readUpdateStateFromDisk(),
      let checked = state["last_checked_at"] as? String,
      let date = ISO8601DateFormatter().date(from: checked)
    else {
      return true
    }
    return Date().timeIntervalSince(date) >= oneContextUpdateCheckInterval
  }

  nonisolated private static func readUpdateStateFromDisk() -> [String: Any]? {
    guard let data = try? Data(contentsOf: UpdateStatePaths.current().file) else { return nil }
    return try? JSONSerialization.jsonObject(with: data) as? [String: Any]
  }

  @objc private func openUpgradeCommand() {
    guard confirmUpdate() else { return }
    runUpdateCommandInTerminal()
  }

  @objc private func checkForUpdatesNow() {
    guard !isCheckingForUpdates else { return }
    isCheckingForUpdates = true

    Task {
      defer {
        Task { @MainActor in
          self.isCheckingForUpdates = false
        }
      }

      do {
        let result = try await UpdateChecker().check(force: true, currentVersion: currentVersion)
        await MainActor.run {
          self.setUpdateState(result.updateAvailable ? .available : .upToDate)
          if result.updateAvailable {
            if self.confirmUpdate() {
              self.runUpdateCommandInTerminal()
            }
          } else {
            self.showUpToDateMessage()
          }
        }
      } catch {
        await MainActor.run {
          self.showUpdateCheckFailedMessage()
        }
      }
    }
  }

  @objc private func toggleRuntime() {
    guard !isRuntimeActionInFlight else { return }
    let targetIntent: RuntimeIntent = runtimeState == .stopped ? .running : .stopped
    desiredRuntimeIntent = targetIntent
    isRuntimeActionInFlight = true
    setRuntimeState(targetIntent == .running ? .checking : .stopped, forceRender: true)

    Task.detached(priority: .userInitiated) {
      let start = await self.perfStart()
      do {
        let controller = RuntimeController()
        if targetIntent == .running {
          _ = try await controller.start(startMenu: false)
          await MainActor.run {
            self.perfLog("runtime.userStart.ok", start: start)
            self.isRuntimeActionInFlight = false
            self.setRuntimeState(.running, forceRender: true)
          }
        } else {
          _ = try await controller.stop()
          await MainActor.run {
            self.perfLog("runtime.userStop.ok", start: start)
            self.isRuntimeActionInFlight = false
            self.setRuntimeState(.stopped, forceRender: true)
          }
        }
      } catch {
        await MainActor.run {
          self.perfLog("runtime.userToggle.failed", start: start)
          self.isRuntimeActionInFlight = false
          self.setRuntimeState(.needsAttention, forceRender: true)
          self.presentMenuAlert("Could not \(targetIntent == .running ? "start" : "stop") 1Context.")
        }
      }
    }
  }

  private func showUpToDateMessage() {
    presentMenuAlert("1Context up to date.")
  }

  private func showUpdateCheckFailedMessage() {
    presentMenuAlert("Could not check for updates.")
  }

  private func confirmUpdate() -> Bool {
    let alert = NSAlert()
    alert.messageText = "Update 1Context?"
    alert.informativeText = "A Terminal window will open and run Homebrew. If macOS asks for your password, Terminal will hide password characters."
    alert.icon = loadFishAlertIcon()
    alert.addButton(withTitle: "Update")
    alert.addButton(withTitle: "Cancel")
    NSApp.activate(ignoringOtherApps: true)
    return alert.runModal() == .alertFirstButtonReturn
  }

  @objc private func showAbout() {
    NSWorkspace.shared.open(oneContextGitHubURL)
  }

  @objc private func openWiki() {
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
        presentMenuAlert("Could not open 1Context wiki.")
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

  @objc private func refreshWiki() {
    guard !isWikiRefreshInFlight else { return }
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
          self.presentMenuAlert("Could not publish 1Context wiki.")
        }
      }
    }
  }

  @objc private func quit() {
    timer?.invalidate()
    timer = nil
    let localWeb = self.localWeb
    if let statusItem {
      NSStatusBar.system.removeStatusItem(statusItem)
    }
    Task.detached {
      _ = try? await RuntimeController().quit(stopMenu: false)
      localWeb.stop()
      await MainActor.run {
        NSApp.terminate(nil)
      }
    }
  }

  func applicationWillTerminate(_ notification: Notification) {
    localWeb.stop()
  }

  private func startLocalWebEdge() {
    localWebQueue.async { [localWeb] in
      AppDelegate.startLocalWebEdge(localWeb)
    }
  }

  private func scheduleLocalWebEdgeStartupRetries() {
    for delay in Constants.localWebStartupRetryDelays {
      localWebQueue.asyncAfter(deadline: .now() + delay) { [localWeb] in
        AppDelegate.startLocalWebEdge(localWeb)
      }
    }
  }

  nonisolated private static func startLocalWebEdge(_ localWeb: CaddyManager) {
    do {
      let current = localWeb.status()
      let snapshot = current.running ? current : try localWeb.start()
      try AgentConfigStore.writeWikiURL(snapshot.url)
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
    try? AgentConfigStore.writeWikiURL(url)
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

  private func runUpdateCommandInTerminal() {
    cleanupStaleUpdaterFiles()

    let menuExecutable = URL(fileURLWithPath: CommandLine.arguments[0])
      .resolvingSymlinksInPath()
    let cliExecutable = menuExecutable.deletingLastPathComponent()
      .appendingPathComponent("1context-cli")
      .path
    guard FileManager.default.isExecutableFile(atPath: cliExecutable) else {
      presentMenuAlert("Could not find 1Context updater.")
      return
    }

    let alertExecutable = menuExecutable.path
    let script = """
    #!/bin/zsh
    set -euo pipefail
    trap 'rm -f "$0"' EXIT

    printf '%s\\n' 'Updating 1Context...'
    printf '%s\\n\\n' 'If prompted, enter your Mac password. Terminal will hide password characters.'
    if \(shellQuote(cliExecutable)) update; then
      \(shellQuote(alertExecutable)) --update-success-alert >/dev/null 2>&1 || osascript -e 'display dialog "1Context updated." buttons {"OK"} default button "OK"'
      printf '\\n%s\\n' 'Done.'
      exit 0
    else
      status=$?
      osascript -e 'display dialog "Could not update 1Context." buttons {"OK"} default button "OK" with icon caution'
      printf '\\n%s\\n' 'Update failed. You can close this window.'
      exit $status
    fi
    """
    guard let scriptURL = writeUpdaterScript(script) else {
      presentMenuAlert("Could not prepare updater.")
      return
    }

    guard runTerminalScript(scriptURL.path) else {
      try? FileManager.default.removeItem(at: scriptURL)
      presentMenuAlert("Could not open updater.")
      return
    }
  }

  private func writeUpdaterScript(_ script: String) -> URL? {
    let url = FileManager.default.temporaryDirectory
      .appendingPathComponent("1context-update-\(UUID().uuidString).zsh")

    do {
      try script.write(to: url, atomically: true, encoding: .utf8)
      chmod(url.path, S_IRUSR | S_IWUSR | S_IXUSR)
      return url
    } catch {
      return nil
    }
  }

  private func runTerminalScript(_ scriptPath: String) -> Bool {
    do {
      let process = Process()
      process.executableURL = URL(fileURLWithPath: "/usr/bin/osascript")
      process.arguments = [
        "-e", "on run argv",
        "-e", "set scriptPath to item 1 of argv",
        "-e", "tell application \"Terminal\"",
        "-e", "activate",
        "-e", "do script \"/bin/zsh \" & quoted form of scriptPath",
        "-e", "end tell",
        "-e", "end run",
        scriptPath,
      ]
      try process.run()
      process.waitUntilExit()
      return process.terminationStatus == 0
    } catch {
      return false
    }
  }

  private func shellQuote(_ value: String) -> String {
    "'\(value.replacingOccurrences(of: "'", with: "'\\''"))'"
  }

  private func cleanupStaleUpdaterFiles() {
    let temporaryDirectory = FileManager.default.temporaryDirectory
    guard let contents = try? FileManager.default.contentsOfDirectory(
      at: temporaryDirectory,
      includingPropertiesForKeys: [.isDirectoryKey],
      options: [.skipsHiddenFiles]
    ) else {
      return
    }

    for url in contents {
      let name = url.lastPathComponent
      if (name.hasPrefix("1context-") && name.hasSuffix(".command"))
        || name.hasPrefix("1context-update-")
      {
        try? FileManager.default.removeItem(at: url)
      }
    }
  }

  private func runLaunchChores() {
    Task.detached(priority: .utility) {
      let cleanupStart = await self.perfStart()
      Self.cleanupStaleUpdaterFilesOnDisk()
      await MainActor.run {
        self.perfLog("launch.cleanupTemp", start: cleanupStart)
      }
      await self.loadCachedUpdateState()
      await self.checkForUpdates(force: false)
    }
  }

  nonisolated private static func cleanupStaleUpdaterFilesOnDisk() {
    let temporaryDirectory = FileManager.default.temporaryDirectory
    guard let contents = try? FileManager.default.contentsOfDirectory(
      at: temporaryDirectory,
      includingPropertiesForKeys: [.isDirectoryKey],
      options: [.skipsHiddenFiles]
    ) else {
      return
    }

    for url in contents {
      let name = url.lastPathComponent
      if (name.hasPrefix("1context-") && name.hasSuffix(".command"))
        || name.hasPrefix("1context-update-")
      {
        try? FileManager.default.removeItem(at: url)
      }
    }
  }

  private func perfStart() -> UInt64 {
    DispatchTime.now().uptimeNanoseconds
  }

  private func perfLog(_ event: String, start: UInt64? = nil) {
    guard perfLoggingEnabled else { return }
    let suffix: String
    if let start {
      let elapsedMs = Double(DispatchTime.now().uptimeNanoseconds - start) / 1_000_000
      suffix = String(format: " %.2fms", elapsedMs)
    } else {
      suffix = ""
    }
    fputs("[1context-menu-perf] \(event)\(suffix)\n", stderr)
  }
}

if CommandLine.arguments.contains("--update-success-alert") {
  _ = NSApplication.shared
  NSApp.setActivationPolicy(.accessory)
  showFishAlert("1Context updated.")
  Foundation.exit(0)
}

guard acquireMenuInstanceLock() else {
  Foundation.exit(0)
}

private let app = NSApplication.shared
private let delegate = AppDelegate()
app.delegate = delegate
app.run()

private enum RuntimeState {
  case checking
  case running
  case stopped
  case needsAttention

  var title: String {
    switch self {
    case .checking:
      return "1Context"
    case .running:
      return "1Context Remembering"
    case .stopped:
      return "1Context Stopped"
    case .needsAttention:
      return "1Context Sick"
    }
  }
}

private enum UpdateState {
  case upToDate
  case available
}

private enum RuntimeIntent {
  case running
  case stopped
}

private struct WikiMenuSnapshot {
  let running: Bool
  let url: String
  let health: String

  init(running: Bool, url: String, health: String) {
    self.running = running
    self.url = url
    self.health = health
  }

  init(payload: [String: Any]) {
    self.running = payload["running"] as? Bool ?? false
    self.url = payload["url"] as? String ?? "http://wiki.1context.localhost:17319/your-context"
    self.health = payload["health"] as? String ?? "unknown"
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
