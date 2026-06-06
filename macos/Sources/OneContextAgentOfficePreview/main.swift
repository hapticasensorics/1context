import AppKit
import OneContextAgentOffice

@main
enum AgentOfficePreviewMain {
  @MainActor
  static func main() {
    let app = NSApplication.shared
    let delegate = AgentOfficePreviewDelegate()
    app.delegate = delegate
    app.setActivationPolicy(.regular)
    app.run()
  }
}

@MainActor
private final class AgentOfficePreviewDelegate: NSObject, NSApplicationDelegate {
  private let officeSize = CGSize(width: 512, height: 288)
  private var window: NSWindow?
  private var officeView: AgentOfficeView?
  private var timer: Timer?
  private var tick = 0

  func applicationDidFinishLaunching(_ notification: Notification) {
    let officeView = AgentOfficeView(frame: NSRect(origin: .zero, size: officeSize))
    officeView.autoresizingMask = [.width, .height]

    let window = NSWindow(
      contentRect: officeView.frame,
      styleMask: [.titled, .closable, .miniaturizable, .resizable],
      backing: .buffered,
      defer: false
    )
    window.title = "Agent Office Preview"
    window.contentView = officeView
    window.minSize = NSSize(width: 410, height: 230)
    window.center()
    window.makeKeyAndOrderFront(nil)

    self.officeView = officeView
    self.window = window
    officeView.startAnimating()
    applyNextSnapshot()
    timer = Timer.scheduledTimer(withTimeInterval: 1.8, repeats: true) { [weak self] _ in
      Task { @MainActor in
        self?.applyNextSnapshot()
      }
    }
    NSApp.activate(ignoringOtherApps: true)
  }

  func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
    true
  }

  private func applyNextSnapshot() {
    officeView?.apply(snapshot: snapshot(step: tick))
    tick += 1
  }

  private func snapshot(step: Int) -> AgentOfficeRunSnapshot {
    let movingLifecycle: AgentOfficeLifecycle = step.isMultiple(of: 2) ? .working : .born
    let rate = step.isMultiple(of: 3) ? 24_500.0 : 182.0
    return AgentOfficeRunSnapshot(
      runID: "preview-\(step)",
      status: "running",
      monthlyTokenUsage: 1_284_932 + (step * 1_337),
      tokensPerSecond: rate,
      activeTurnCount: 4,
      plannedCount: 1,
      completedCount: 2,
      failedCount: 1,
      agents: [
        AgentOfficeAgentSnapshot(
          unitID: "scribe-active",
          jobID: "memory.hourly.scribe",
          role: "scribe",
          phase: "birth",
          lifecycle: movingLifecycle,
          summary: "Writing source notes"
        ),
        AgentOfficeAgentSnapshot(
          unitID: "biographer-working",
          jobID: "memory.wiki.biographer",
          role: "biographer",
          phase: "turn",
          lifecycle: .working,
          summary: "Drafting profile"
        ),
        AgentOfficeAgentSnapshot(
          unitID: "librarian-mail",
          jobID: "memory.wiki.librarian",
          role: "librarian",
          phase: "mail",
          lifecycle: .postedMail,
          hasTalkReceipt: true,
          summary: "Handing off note"
        ),
        AgentOfficeAgentSnapshot(
          unitID: "curator-waiting",
          jobID: "memory.wiki.context_curator",
          role: "curator",
          phase: "waiting",
          lifecycle: .waiting,
          summary: "Waiting for turns"
        ),
        AgentOfficeAgentSnapshot(
          unitID: "redactor-failed",
          jobID: "memory.wiki.redactor",
          role: "contradiction",
          phase: "settled",
          lifecycle: .failed,
          summary: "Needs retry",
          error: "Preview failure"
        ),
        AgentOfficeAgentSnapshot(
          unitID: "publisher-complete",
          jobID: "memory.wiki.publisher",
          role: "publisher",
          phase: "settled",
          lifecycle: .completed,
          summary: "Settled turn"
        )
      ]
    )
  }
}
