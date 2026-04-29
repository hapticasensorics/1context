import AppKit
import Darwin
import Foundation
import OneContextRuntimeSupport

private enum Constants {
  static let appName = "1Context"
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
  let mainURL = Bundle.main.url(forResource: "MenuBarIcon", withExtension: "png")
  guard let image = mainURL.flatMap(NSImage.init(contentsOf:)) else {
    return nil
  }
  image.isTemplate = false
  image.size = NSSize(width: 64, height: 64)
  AppDelegate.cachedFishAlertIcon = image
  return image.copy() as? NSImage
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
  private var isMenuOpen = false
  private var pendingRuntimeState: RuntimeState?
  private var pendingUpdateState: UpdateState?
  private var renderGeneration = 0
  private let menu = NSMenu()
  private let stateItem = NSMenuItem(title: RuntimeState.checking.title, action: nil, keyEquivalent: "")
  private let settingsItem = NSMenuItem(title: "Settings", action: nil, keyEquivalent: "")
  private let settingsMenu = NSMenu()
  private let versionItem = NSMenuItem(title: "", action: nil, keyEquivalent: "")
  private let aboutItem = NSMenuItem(title: "About 1Context", action: #selector(showAbout), keyEquivalent: "")
  private let updateItem = NSMenuItem(title: "", action: nil, keyEquivalent: "")
  private let quitItem = NSMenuItem(title: "Quit", action: #selector(quit), keyEquivalent: "q")
  private let appVersion = Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String
    ?? oneContextVersion
  private let perfLoggingEnabled = ProcessInfo.processInfo.environment["ONECONTEXT_MENU_PERF_LOG"] == "1"

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
    runLaunchChores()
    ensureRuntimeRunning(userInitiated: false)

    timer = Timer.scheduledTimer(withTimeInterval: 30, repeats: true) { [weak self] _ in
      Task { @MainActor in
        self?.ensureRuntimeRunning(userInitiated: false)
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
    let mainURL = Bundle.main.url(forResource: "MenuBarIcon", withExtension: "png")
    return mainURL.flatMap(NSImage.init(contentsOf:))
  }

  private func configureMenu() {
    menu.autoenablesItems = false
    stateItem.isEnabled = false
    menu.addItem(stateItem)

    versionItem.isEnabled = false
    settingsMenu.addItem(versionItem)
    settingsMenu.addItem(aboutItem)
    settingsItem.submenu = settingsMenu
    menu.addItem(settingsItem)
    menu.addItem(updateItem)
    menu.addItem(quitItem)
    menu.delegate = self

    for item in [stateItem, settingsItem, versionItem, aboutItem, updateItem, quitItem] {
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

  private func setRuntimeState(_ newValue: RuntimeState) {
    guard runtimeState != newValue else { return }
    if isMenuOpen {
      pendingRuntimeState = newValue
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

  func menuWillOpen(_ menu: NSMenu) {
    let start = perfStart()
    refreshRuntimeStateForMenuOpen()
    isMenuOpen = true
    renderGeneration += 1
    perfLog("menu.willOpen", start: start)
  }

  private func refreshRuntimeStateForMenuOpen() {
    let desiredState = (try? String(contentsOfFile: RuntimePaths.current().desiredStatePath, encoding: .utf8))?
      .trimmingCharacters(in: .whitespacesAndNewlines)
    if desiredState == "stopped" {
      runtimeState = .stopped
      pendingRuntimeState = nil
      refreshMenuItems()
      return
    }
    ensureRuntimeRunning(userInitiated: false)
  }

  func menuDidClose(_ menu: NSMenu) {
    let start = perfStart()
    isMenuOpen = false
    guard pendingRuntimeState != nil || pendingUpdateState != nil else {
      perfLog("menu.didClose.noop", start: start)
      return
    }
    renderGeneration += 1
    let generation = renderGeneration
    DispatchQueue.main.async { [weak self] in
      guard let self else { return }
      guard !isMenuOpen, generation == renderGeneration else { return }
      if let pendingRuntimeState {
        runtimeState = pendingRuntimeState
        self.pendingRuntimeState = nil
      }
      if let pendingUpdateState {
        updateState = pendingUpdateState
        self.pendingUpdateState = nil
      }
      refreshMenuItems()
      perfLog("menu.didClose.deferredRender", start: start)
    }
  }

  private func ensureRuntimeRunning(userInitiated: Bool) {
    guard !isRepairingRuntime else { return }
    isRepairingRuntime = true

    Task.detached(priority: .utility) {
      let healthStart = await self.perfStart()
      let controller = RuntimeController()
      defer {
        Task { @MainActor in
          self.isRepairingRuntime = false
        }
      }

      do {
        let health = try UnixJSONRPCClient().health()
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

  private func markRuntimeNeedsAttention() {
    setRuntimeState(.needsAttention)
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

  private func showUpToDateMessage() {
    showFishAlert("1Context up to date.")
  }

  private func showUpdateCheckFailedMessage() {
    showFishAlert("Could not check for updates.")
  }

  private func confirmUpdate() -> Bool {
    let alert = NSAlert()
    alert.messageText = "Update 1Context with Homebrew?"
    alert.informativeText = "This may ask for your Mac password."
    alert.icon = loadFishAlertIcon()
    alert.addButton(withTitle: "Update")
    alert.addButton(withTitle: "Cancel")
    NSApp.activate(ignoringOtherApps: true)
    return alert.runModal() == .alertFirstButtonReturn
  }

  @objc private func showAbout() {
    NSWorkspace.shared.open(oneContextGitHubURL)
  }

  @objc private func quit() {
    timer?.invalidate()
    timer = nil
    if let statusItem {
      NSStatusBar.system.removeStatusItem(statusItem)
    }
    Task.detached {
      _ = try? await RuntimeController().quit(stopMenu: false)
      await MainActor.run {
        NSApp.terminate(nil)
      }
    }
  }

  private func runUpdateCommandInTerminal() {
    cleanupStaleUpdaterFiles()

    let menuExecutable = URL(fileURLWithPath: CommandLine.arguments[0])
      .resolvingSymlinksInPath()
    let cliExecutable = menuExecutable.deletingLastPathComponent()
      .appendingPathComponent("1context-cli")
      .path
    guard FileManager.default.isExecutableFile(atPath: cliExecutable) else {
      showFishAlert("Could not find 1Context updater.")
      return
    }

    let alertExecutable = menuExecutable.path
    let script = """
    #!/bin/zsh
    set -euo pipefail
    trap 'rm -f "$0"' EXIT

    printf '%s\\n' 'Updating 1Context with Homebrew.'
    printf '%s\\n\\n' 'If macOS asks for your password, type it here. Terminal will not show password characters.'
    if \(shellQuote(cliExecutable)) update; then
      \(shellQuote(alertExecutable)) --update-success-alert >/dev/null 2>&1 || osascript -e 'display dialog "1Context updated." buttons {"OK"} default button "OK"'
      printf '\\n%s\\n' 'Done. This window will close in 3 seconds.'
      sleep 3
      exit 0
    else
      status=$?
      osascript -e 'display dialog "Could not update 1Context." buttons {"OK"} default button "OK" with icon caution'
      printf '\\n%s\\n' 'Update failed. You can close this window.'
      exit $status
    fi
    """
    guard let scriptURL = writeUpdaterScript(script) else {
      showFishAlert("Could not prepare updater.")
      return
    }

    guard runTerminalScript(scriptURL.path) else {
      try? FileManager.default.removeItem(at: scriptURL)
      showFishAlert("Could not open updater.")
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
      return "1Context Running"
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
