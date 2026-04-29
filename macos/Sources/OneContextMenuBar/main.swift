import AppKit
import Foundation
import OneContextRuntimeSupport

private enum Constants {
  static let appName = "1Context"
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
  let bundleURL = Bundle.module.url(forResource: "MenuBarIcon", withExtension: "png")
  let mainURL = Bundle.main.url(forResource: "MenuBarIcon", withExtension: "png")
  guard let image = [bundleURL, mainURL].compactMap({ $0 }).compactMap(NSImage.init(contentsOf:)).first else {
    return nil
  }
  image.isTemplate = false
  image.size = NSSize(width: 64, height: 64)
  return image
}

@MainActor
private final class AppDelegate: NSObject, NSApplicationDelegate, NSMenuDelegate {
  private var statusItem: NSStatusItem!
  private var timer: Timer?
  private var runtimeState: RuntimeState = .checking
  private var updateState: UpdateState = .upToDate
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
    ensureRuntimeRunning(userInitiated: true)
    checkForUpdates(force: false)

    timer = Timer.scheduledTimer(withTimeInterval: 30, repeats: true) { [weak self] _ in
      Task { @MainActor in
        self?.ensureRuntimeRunning(userInitiated: false)
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

    let settingsItem = NSMenuItem(title: "Settings", action: nil, keyEquivalent: "")
    let settingsMenu = NSMenu()
    let versionItem = NSMenuItem(title: "Version \(currentVersion)", action: nil, keyEquivalent: "")
    versionItem.isEnabled = false
    settingsMenu.addItem(versionItem)
    settingsMenu.addItem(NSMenuItem(title: "About 1Context", action: #selector(showAbout), keyEquivalent: ""))
    settingsItem.submenu = settingsMenu
    menu.addItem(settingsItem)

    switch updateState {
    case .upToDate:
      menu.addItem(NSMenuItem(title: "Check for Updates", action: #selector(checkForUpdatesNow), keyEquivalent: ""))
    case .available:
      menu.addItem(NSMenuItem(title: "Please Update", action: #selector(openUpgradeCommand), keyEquivalent: ""))
    }

    menu.addItem(NSMenuItem(title: "Quit", action: #selector(quit), keyEquivalent: "q"))
    menu.items.forEach { $0.target = self }
    settingsMenu.items.forEach { $0.target = self }
    menu.delegate = self
    statusItem.menu = menu
  }

  func menuWillOpen(_ menu: NSMenu) {
    ensureRuntimeRunning(userInitiated: false)
  }

  private func ensureRuntimeRunning(userInitiated: Bool) {
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
          guard userInitiated || controller.shouldAutoStartRuntime() else { return }
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
      return
    }
    updateState = compareVersions(version, currentVersion) > 0 ? .available : .upToDate
  }

  private func checkForUpdates(force: Bool) {
    guard force || shouldCheckForUpdate() else { return }
    guard !isCheckingForUpdates else { return }
    isCheckingForUpdates = true

    Task {
      defer {
        Task { @MainActor in
          self.isCheckingForUpdates = false
        }
      }

      do {
        let result = try await UpdateChecker().check(force: force, currentVersion: currentVersion)
        await MainActor.run {
          self.updateState = result.updateAvailable ? .available : .upToDate
          self.rebuildMenu()
        }
      } catch {
        // Update failures stay quiet. The menu remains usable offline.
      }
    }
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

  private func updateStateURL() -> URL {
    UpdateStatePaths.current().file
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
          self.updateState = result.updateAvailable ? .available : .upToDate
          self.rebuildMenu()
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
    NSApp.terminate(nil)
  }

  private func runUpdateCommandInTerminal() {
    let alertExecutable = URL(fileURLWithPath: CommandLine.arguments[0])
      .resolvingSymlinksInPath()
      .path
    let fileURL = FileManager.default.temporaryDirectory
      .appendingPathComponent("1context-\(UUID().uuidString).command")
    let script = """
    #!/bin/zsh
    if \(oneContextHomebrewUpdateCommand); then
      \(shellQuote(alertExecutable)) --update-success-alert >/dev/null 2>&1 || osascript -e 'display dialog "1Context updated." buttons {"OK"} default button "OK"'
    else
      status=$?
      osascript -e 'display dialog "Could not update 1Context." buttons {"OK"} default button "OK" with icon caution'
      exit $status
    fi
    """
    try? script.write(to: fileURL, atomically: true, encoding: .utf8)
    chmod(fileURL.path, 0o700)
    NSWorkspace.shared.open(fileURL)
  }

  private func shellQuote(_ value: String) -> String {
    "'\(value.replacingOccurrences(of: "'", with: "'\\''"))'"
  }
}

if CommandLine.arguments.contains("--update-success-alert") {
  _ = NSApplication.shared
  NSApp.setActivationPolicy(.accessory)
  showFishAlert("1Context updated.")
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
