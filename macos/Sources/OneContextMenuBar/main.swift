import AppKit
import Foundation
import OneContextRuntimeSupport

private enum Constants {
  static let appName = "1Context"
}

@MainActor
private final class AppDelegate: NSObject, NSApplicationDelegate {
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

    let settingsItem = NSMenuItem(title: "Settings", action: nil, keyEquivalent: "")
    let settingsMenu = NSMenu()
    let versionItem = NSMenuItem(title: "Version \(currentVersion)", action: nil, keyEquivalent: "")
    versionItem.isEnabled = false
    settingsMenu.addItem(versionItem)
    settingsMenu.addItem(NSMenuItem(title: "About 1Context", action: #selector(showAbout), keyEquivalent: ""))
    switch updateState {
    case .upToDate:
      let updateItem = NSMenuItem(title: "Up to Date", action: nil, keyEquivalent: "")
      updateItem.isEnabled = false
      settingsMenu.addItem(updateItem)
    case .available:
      settingsMenu.addItem(NSMenuItem(title: "Update 1Context...", action: #selector(openUpgradeCommand), keyEquivalent: ""))
    }
    settingsItem.submenu = settingsMenu
    menu.addItem(settingsItem)

    menu.addItem(NSMenuItem(title: "Quit", action: #selector(quit), keyEquivalent: "q"))
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
    runInTerminal(oneContextHomebrewUpdateCommand)
  }

  @objc private func showAbout() {
    NSApp.orderFrontStandardAboutPanel(options: [
      .applicationName: "1Context",
      .applicationVersion: currentVersion,
      .version: currentVersion,
      .credits: NSAttributedString(string: "Own your context.")
    ])
  }

  @objc private func quit() {
    timer?.invalidate()
    timer = nil
    NSApp.terminate(nil)
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
      return "1Context Paused"
    case .needsAttention:
      return "1Context Sick"
    }
  }
}

private enum UpdateState {
  case upToDate
  case available
}
