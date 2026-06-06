import AppKit
import OneContextPlatform

@MainActor
public final class AgentOfficeWindowController: NSWindowController {
  private let adapter: AgentOfficeRuntimeAdapter
  private let officeView: AgentOfficeView
  private var refreshTimer: Timer?
  private var refreshTask: Task<Void, Never>?
  public var onVisibilityChanged: ((Bool) -> Void)?

  public var isOfficeVisible: Bool {
    window?.isVisible == true
  }

  public init(
    runtimePaths: RuntimePaths,
    adapter: AgentOfficeRuntimeAdapter? = nil
  ) {
    let officeSize = CGSize(width: 512, height: 288)
    self.adapter = adapter ?? AgentOfficeRuntimeAdapter(runtimePaths: runtimePaths)
    self.officeView = AgentOfficeView(frame: NSRect(origin: .zero, size: officeSize))
    officeView.autoresizingMask = [.width, .height]

    let window = NSWindow(
      contentRect: officeView.frame,
      styleMask: [.borderless, .resizable],
      backing: .buffered,
      defer: false
    )
    window.title = "1Context Agent Office"
    window.contentView = officeView
    window.isOpaque = false
    window.backgroundColor = .clear
    window.hasShadow = true
    window.level = .floating
    window.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary]
    window.isMovableByWindowBackground = true
    window.minSize = NSSize(width: 410, height: 230)

    super.init(window: window)
    window.delegate = self
  }

  required init?(coder: NSCoder) {
    fatalError("init(coder:) has not been implemented")
  }

  public func show() {
    showWindow(nil)
    window?.makeKeyAndOrderFront(nil)
    NSApp.activate(ignoringOtherApps: true)
    officeView.startAnimating()
    startPolling()
    refreshNow()
    onVisibilityChanged?(true)
  }

  public func closeOffice() {
    refreshTimer?.invalidate()
    refreshTimer = nil
    refreshTask?.cancel()
    refreshTask = nil
    officeView.stopAnimating()
    close()
    onVisibilityChanged?(false)
  }

  private func startPolling() {
    guard refreshTimer == nil else { return }
    refreshTimer = Timer.scheduledTimer(withTimeInterval: 3.0, repeats: true) { [weak self] _ in
      Task { @MainActor in
        self?.refreshNow()
      }
    }
  }

  private func refreshNow() {
    guard refreshTask == nil else { return }
    let adapter = adapter
    refreshTask = Task { [weak self] in
      let snapshot = await Task.detached(priority: .utility) {
        adapter.latestSnapshot()
      }.value
      guard let self, !Task.isCancelled else { return }
      self.officeView.apply(snapshot: snapshot)
      self.refreshTask = nil
    }
  }
}

extension AgentOfficeWindowController: NSWindowDelegate {
  public func windowWillClose(_ notification: Notification) {
    refreshTimer?.invalidate()
    refreshTimer = nil
    refreshTask?.cancel()
    refreshTask = nil
    officeView.stopAnimating()
    onVisibilityChanged?(false)
  }
}
