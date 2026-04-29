import AppKit
import Foundation
import OneContextRuntimeSupport

private enum Constants {
  static let appName = "1Context"
  static let releasesURL = URL(string: "https://github.com/hapticasensorics/1context/releases")!
}

private struct UpdateInfo {
  let latestVersion: String
  let notesURL: URL?
  let installCommand: String
  let isAvailable: Bool
}

@MainActor
private final class AppDelegate: NSObject, NSApplicationDelegate {
  private var statusItem: NSStatusItem!
  private var timer: Timer?
  private var runtimeState: RuntimeState = .checking
  private var updateInfo: UpdateInfo?
  private var updateStatus = "Checking for Updates..."
  private var isCheckingForUpdates = false
  private var isRepairingRuntime = false

  private var currentVersion: String {
    Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String
      ?? oneContextVersion
  }

  func applicationDidFinishLaunching(_ notification: Notification) {
    NSApp.setActivationPolicy(.accessory)
    statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.squareLength)
    configureStatusIcon()
    loadCachedUpdateState()
    rebuildMenu()
    ensureRuntimeRunning()
    checkForUpdates(force: false)

    timer = Timer.scheduledTimer(withTimeInterval: 30, repeats: true) { [weak self] _ in
      Task { @MainActor in
        self?.ensureRuntimeRunning()
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

  private func loadMenuIcon() -> NSImage? {
    let bundleURL = Bundle.module.url(forResource: "MenuBarIcon", withExtension: "png")
    let mainURL = Bundle.main.url(forResource: "MenuBarIcon", withExtension: "png")
    return [bundleURL, mainURL].compactMap { $0 }.compactMap(NSImage.init(contentsOf:)).first
  }

  private func rebuildMenu() {
    let menu = NSMenu()

    let stateItem = NSMenuItem(
      title: runtimeState.title,
      action: nil,
      keyEquivalent: ""
    )
    stateItem.isEnabled = false
    menu.addItem(stateItem)

    let versionItem = NSMenuItem(title: "Version \(currentVersion)", action: nil, keyEquivalent: "")
    versionItem.isEnabled = false
    menu.addItem(versionItem)

    let updateItem = NSMenuItem(title: updateStatus, action: nil, keyEquivalent: "")
    updateItem.isEnabled = false
    menu.addItem(updateItem)

    menu.addItem(NSMenuItem.separator())

    if updateInfo?.isAvailable == true {
      menu.addItem(NSMenuItem(title: "Update 1Context...", action: #selector(openUpgradeCommand), keyEquivalent: ""))
    } else {
      menu.addItem(NSMenuItem(title: "Check for Updates", action: #selector(forceUpdateCheck), keyEquivalent: ""))
    }

    switch runtimeState {
    case .running:
      menu.addItem(NSMenuItem(title: "Status", action: #selector(showStatus), keyEquivalent: ""))
      menu.addItem(NSMenuItem(title: "View Logs", action: #selector(viewLogs), keyEquivalent: ""))
    case .stopped:
      menu.addItem(NSMenuItem(title: "Starting 1Context...", action: nil, keyEquivalent: ""))
    case .needsAttention:
      menu.addItem(NSMenuItem(title: "Restart 1Context", action: #selector(restartOneContext), keyEquivalent: ""))
      menu.addItem(NSMenuItem(title: "View Logs", action: #selector(viewLogs), keyEquivalent: ""))
      menu.addItem(NSMenuItem(title: "Status", action: #selector(showStatus), keyEquivalent: ""))
    case .checking:
      break
    }

    if updateInfo?.notesURL != nil {
      menu.addItem(NSMenuItem(title: "Open Release Notes", action: #selector(openReleaseNotes), keyEquivalent: ""))
    }

    menu.addItem(NSMenuItem.separator())
    let settings = NSMenuItem(title: "Settings", action: nil, keyEquivalent: "")
    let settingsMenu = NSMenu()
    settingsMenu.addItem(NSMenuItem.separator())
    settingsMenu.addItem(NSMenuItem(title: "About 1Context", action: #selector(openAbout), keyEquivalent: ""))
    settings.submenu = settingsMenu
    menu.addItem(settings)

    menu.addItem(NSMenuItem.separator())
    menu.addItem(NSMenuItem(title: "Quit 1Context", action: #selector(quit), keyEquivalent: "q"))
    menu.items.forEach { $0.target = self }
    settingsMenu.items.forEach { $0.target = self }
    statusItem.menu = menu
  }

  private func ensureRuntimeRunning() {
    guard !isRepairingRuntime else { return }
    isRepairingRuntime = true

    Task.detached(priority: .utility) {
      let controller = RuntimeController()
      defer {
        Task { @MainActor in
          self.isRepairingRuntime = false
        }
      }

      do {
        _ = try UnixJSONRPCClient().health()
        await MainActor.run {
          self.runtimeState = .running
          self.rebuildMenu()
        }
        return
      } catch let error as UnixSocketError {
        switch error {
        case .connectFailed, .emptyResponse:
          await MainActor.run {
            self.runtimeState = .stopped
            self.rebuildMenu()
          }
          guard controller.shouldAutoStartRuntime() else { return }
          do {
            _ = try await controller.start()
            await MainActor.run {
              self.runtimeState = .running
              self.rebuildMenu()
            }
          } catch {
            await MainActor.run {
              self.runtimeState = .needsAttention
              self.rebuildMenu()
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
    runtimeState = .needsAttention
    rebuildMenu()
  }

  private func loadCachedUpdateState() {
    guard let state = readUpdateState(),
      let version = state["last_seen_latest"] as? String
    else {
      updateStatus = "Update Status Unknown"
      return
    }

    let info = UpdateInfo(
      latestVersion: version,
      notesURL: (state["notes_url"] as? String).flatMap(URL.init(string:)),
      installCommand: (state["install"] as? String) ?? oneContextHomebrewUpdateCommand,
      isAvailable: compareVersions(version, currentVersion) > 0
    )
    updateInfo = info
    updateStatus = statusText(for: info)
  }

  private func checkForUpdates(force: Bool) {
    guard force || shouldCheckForUpdate() else { return }
    guard !isCheckingForUpdates else { return }

    isCheckingForUpdates = true
    updateStatus = "Checking for Updates..."
    rebuildMenu()

    Task {
      do {
        let result = try await UpdateChecker().check(force: force, currentVersion: currentVersion)
        guard let release = result.latest else { throw URLError(.badServerResponse) }
        let update = UpdateInfo(
          latestVersion: release.version,
          notesURL: release.notesURL,
          installCommand: release.installCommand,
          isAvailable: result.updateAvailable
        )
        finishUpdateCheck(update: update)
      } catch {
        finishUpdateCheck(update: nil)
      }
    }
  }

  private func finishUpdateCheck(update: UpdateInfo?) {
    isCheckingForUpdates = false
    if let update {
      updateInfo = update
      updateStatus = statusText(for: update)
    } else if updateInfo == nil {
      updateStatus = "Could Not Check for Updates"
    }
    rebuildMenu()
  }

  private func statusText(for update: UpdateInfo) -> String {
    update.isAvailable
      ? "Update Available: \(update.latestVersion)"
      : "Up to Date"
  }

  private func shouldCheckForUpdate() -> Bool {
    guard let state = readUpdateState(),
      let checked = state["last_checked_at"] as? String,
      let date = ISO8601DateFormatter().date(from: checked)
    else {
      return true
    }
    return Date().timeIntervalSince(date) >= oneContextUpdateCheckInterval
  }

  private func readUpdateState() -> [String: Any]? {
    guard let data = try? Data(contentsOf: updateStateURL()) else { return nil }
    return try? JSONSerialization.jsonObject(with: data) as? [String: Any]
  }

  private func writeUpdateState(_ update: UpdateInfo) {
    let url = updateStateURL()
    try? FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
    var state: [String: Any] = [
      "last_checked_at": ISO8601DateFormatter().string(from: Date()),
      "last_seen_latest": update.latestVersion,
      "install": oneContextHomebrewUpdateCommand
    ]
    if let notesURL = update.notesURL {
      state["notes_url"] = notesURL.absoluteString
    }
    if let data = try? JSONSerialization.data(withJSONObject: state, options: [.prettyPrinted, .sortedKeys]) {
      try? (data + Data([UInt8(ascii: "\n")])).write(to: url, options: .atomic)
    }
  }

  private func updateStateURL() -> URL {
    if let override = ProcessInfo.processInfo.environment["ONECONTEXT_UPDATE_STATE_DIR"] {
      return URL(fileURLWithPath: override, isDirectory: true).appendingPathComponent("update-check.json")
    }
    return FileManager.default.homeDirectoryForCurrentUser
      .appendingPathComponent(".config/1context/update-check.json")
  }

  @objc private func forceUpdateCheck() {
    checkForUpdates(force: true)
  }

  @objc private func openUpgradeCommand() {
    runInTerminal(updateInfo?.installCommand ?? oneContextHomebrewUpdateCommand)
  }

  @objc private func startOneContext() {
    runInTerminal("1context start")
  }

  @objc private func restartOneContext() {
    runInTerminal("1context restart")
  }

  @objc private func showStatus() {
    runInTerminal("1context status")
  }

  @objc private func viewLogs() {
    NSWorkspace.shared.open(URL(fileURLWithPath: RuntimePaths.current().logPath))
  }

  @objc private func openReleaseNotes() {
    NSWorkspace.shared.open(updateInfo?.notesURL ?? Constants.releasesURL)
  }

  @objc private func openAbout() {
    NSWorkspace.shared.open(oneContextGitHubURL)
  }

  @objc private func quit() {
    timer?.invalidate()
    timer = nil
    Task.detached(priority: .userInitiated) {
      _ = try? await RuntimeController().stop()
      await MainActor.run {
        NSApp.terminate(nil)
      }
    }
  }

  private func runInTerminal(_ command: String) {
    let fileURL = FileManager.default.temporaryDirectory
      .appendingPathComponent("1context-\(UUID().uuidString).command")
    let script = """
    #!/bin/zsh
    \(command)
    echo
    echo "Done. You can close this window."
    """
    try? script.write(to: fileURL, atomically: true, encoding: .utf8)
    chmod(fileURL.path, 0o700)
    NSWorkspace.shared.open(fileURL)
  }
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
      return "1Context Needs Attention"
    }
  }
}
