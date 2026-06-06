import AppKit

@MainActor
public final class AgentOfficeView: NSView {
  private let designSize = CGSize(width: 512, height: 288)
  private let desks = AgentOfficeDesk.defaultDesks
  private var snapshot = AgentOfficeRunSnapshot.idle
  private var animationPhase: CGFloat = 0
  private var animationTimer: Timer?

  private(set) var renderedTelemetry = AgentOfficeTelemetryText.idle
  private(set) var renderedAgents: [AgentOfficeRenderAgent] = []

  public override var isFlipped: Bool {
    true
  }

  public override init(frame frameRect: NSRect) {
    super.init(frame: frameRect)
    commonInit()
  }

  public required init?(coder: NSCoder) {
    super.init(coder: coder)
    commonInit()
  }

  public func apply(snapshot: AgentOfficeRunSnapshot) {
    self.snapshot = snapshot
    renderedTelemetry = AgentOfficeTelemetryText(snapshot: snapshot)
    renderedAgents = AgentOfficeRenderAgent.visibleAgents(from: snapshot.agents)
    needsDisplay = true
  }

  public override func viewWillMove(toWindow newWindow: NSWindow?) {
    super.viewWillMove(toWindow: newWindow)
    if newWindow == nil {
      stopAnimating()
    }
  }

  public func startAnimating() {
    guard animationTimer == nil else { return }
    animationTimer = Timer.scheduledTimer(withTimeInterval: 0.18, repeats: true) { [weak self] _ in
      Task { @MainActor in
        guard let self else { return }
        self.animationPhase += 1
        self.needsDisplay = true
      }
    }
  }

  public func stopAnimating() {
    animationTimer?.invalidate()
    animationTimer = nil
  }

  public override func draw(_ dirtyRect: NSRect) {
    super.draw(dirtyRect)

    let bounds = self.bounds
    let scale = min(bounds.width / designSize.width, bounds.height / designSize.height)
    let canvas = CGRect(
      x: bounds.midX - designSize.width * scale / 2,
      y: bounds.midY - designSize.height * scale / 2,
      width: designSize.width * scale,
      height: designSize.height * scale
    )

    NSGraphicsContext.current?.cgContext.saveGState()
    NSGraphicsContext.current?.cgContext.translateBy(x: canvas.minX, y: canvas.minY)
    NSGraphicsContext.current?.cgContext.scaleBy(x: scale, y: scale)
    drawOffice(in: CGRect(origin: .zero, size: designSize))
    NSGraphicsContext.current?.cgContext.restoreGState()
  }

  private func commonInit() {
    wantsLayer = true
    layer?.backgroundColor = NSColor.clear.cgColor
    apply(snapshot: .idle)
  }

  private func drawOffice(in rect: CGRect) {
    let shell = rect.insetBy(dx: 8, dy: 8)
    drawShadowedCard(shell)
    drawRoom(shell)
    drawTelemetry(in: shell)

    let deskCenters = deskPositions(in: shell)
    let activeRoles = Set(renderedAgents.filter(\.lifecycle.isActive).map(\.role))
    for desk in desks {
      guard let center = deskCenters[desk.role] else { continue }
      drawDesk(desk, at: center, active: activeRoles.contains(desk.role))
    }

    let grouped = Dictionary(grouping: renderedAgents, by: \.role)
    for role in orderedRoles(from: grouped.keys) {
      guard let roleAgents = grouped[role], let home = homePosition(for: role, deskCenters: deskCenters) else {
        continue
      }
      for (index, agent) in roleAgents.enumerated() {
        let point = agentPosition(agent, home: home, index: index, count: roleAgents.count)
        if agent.hasTalkReceipt || agent.lifecycle == .postedMail {
          drawMailPath(from: point, to: mailHub(in: shell), agentIndex: index)
        }
        drawAgent(agent, at: point, index: index)
      }
    }
  }

  private func drawShadowedCard(_ rect: CGRect) {
    NSColor(calibratedWhite: 0, alpha: 0.16).setFill()
    roundedRect(rect.offsetBy(dx: 4, dy: 5), radius: 18).fill()
    OfficePalette.paper.setFill()
    OfficePalette.ink.setStroke()
    let path = roundedRect(rect, radius: 18)
    path.lineWidth = 2
    path.fill()
    path.stroke()
  }

  private func drawRoom(_ shell: CGRect) {
    let header = CGRect(x: shell.minX + 3, y: shell.minY + 3, width: shell.width - 6, height: 54)
    let floor = CGRect(x: shell.minX + 3, y: header.maxY, width: shell.width - 6, height: shell.height - header.height - 6)

    OfficePalette.wall.setFill()
    roundedRect(header, radius: 14).fill()
    OfficePalette.floor.setFill()
    roundedRect(floor, radius: 14).fill()

    OfficePalette.floorLine.setStroke()
    for y in stride(from: floor.minY + 25, through: floor.maxY - 18, by: 28) {
      let line = NSBezierPath()
      line.move(to: CGPoint(x: floor.minX + 14, y: y))
      line.line(to: CGPoint(x: floor.maxX - 14, y: y))
      line.lineWidth = 1
      line.stroke()
    }

    drawMailHub(in: shell)
  }

  private func drawTelemetry(in shell: CGRect) {
    let title = "Agent Office"
    drawText(
      title,
      in: CGRect(x: shell.minX + 20, y: shell.minY + 16, width: 142, height: 20),
      font: .systemFont(ofSize: 17, weight: .semibold),
      color: OfficePalette.ink,
      alignment: .left
    )

    drawTelemetryPill(
      text: renderedTelemetry.month,
      rect: CGRect(x: shell.maxX - 286, y: shell.minY + 12, width: 130, height: 22),
      accent: OfficePalette.blue
    )
    drawTelemetryPill(
      text: renderedTelemetry.rate,
      rect: CGRect(x: shell.maxX - 150, y: shell.minY + 12, width: 130, height: 22),
      accent: OfficePalette.green
    )

    drawText(
      renderedTelemetry.status,
      in: CGRect(x: shell.minX + 20, y: shell.minY + 35, width: shell.width - 40, height: 15),
      font: .monospacedDigitSystemFont(ofSize: 10, weight: .medium),
      color: OfficePalette.mutedInk,
      alignment: .left
    )
  }

  private func drawTelemetryPill(text: String, rect: CGRect, accent: NSColor) {
    NSColor.white.withAlphaComponent(0.72).setFill()
    accent.withAlphaComponent(0.34).setStroke()
    let path = roundedRect(rect, radius: 8)
    path.lineWidth = 1
    path.fill()
    path.stroke()

    drawText(
      text,
      in: rect.insetBy(dx: 8, dy: 4),
      font: .monospacedDigitSystemFont(ofSize: 10, weight: .semibold),
      color: OfficePalette.ink,
      alignment: .center
    )
  }

  private func drawDesk(_ desk: AgentOfficeDesk, at center: CGPoint, active: Bool) {
    let deskRect = CGRect(x: center.x - 27, y: center.y - 12, width: 54, height: 25)
    let topRect = CGRect(x: center.x - 31, y: center.y - 17, width: 62, height: 13)
    let glowRect = topRect.insetBy(dx: -4, dy: -4)

    if active {
      OfficePalette.activeGlow.setFill()
      roundedRect(glowRect, radius: 10).fill()
    }

    OfficePalette.desk.setFill()
    OfficePalette.ink.setStroke()
    let body = roundedRect(deskRect, radius: 5)
    body.lineWidth = 1.4
    body.fill()
    body.stroke()

    OfficePalette.deskTop.setFill()
    let top = roundedRect(topRect, radius: 6)
    top.lineWidth = 1.4
    top.fill()
    top.stroke()

    OfficePalette.paper.setFill()
    roundedRect(CGRect(x: center.x - 22, y: center.y - 21, width: 14, height: 8), radius: 2).fill()
    OfficePalette.typewriter.setFill()
    roundedRect(CGRect(x: center.x + 5, y: center.y - 20, width: 15, height: 8), radius: 2).fill()

    let marker = CGRect(x: center.x + 22, y: center.y - 7, width: 8, height: 8)
    OfficePalette.roleColor(desk.role).setFill()
    roundedRect(marker, radius: 2).fill()

    drawText(
      desk.label,
      in: CGRect(x: center.x - 36, y: center.y + 18, width: 72, height: 13),
      font: .systemFont(ofSize: 9, weight: .medium),
      color: OfficePalette.mutedInk,
      alignment: .center
    )
  }

  private func drawMailHub(in shell: CGRect) {
    let hub = mailHub(in: shell)
    let table = CGRect(x: hub.x - 31, y: hub.y - 13, width: 62, height: 27)
    OfficePalette.mailTable.setFill()
    OfficePalette.ink.setStroke()
    let path = roundedRect(table, radius: 9)
    path.lineWidth = 1.3
    path.fill()
    path.stroke()

    OfficePalette.paper.setFill()
    roundedRect(CGRect(x: hub.x - 18, y: hub.y - 7, width: 18, height: 11), radius: 2).fill()
    OfficePalette.mail.setFill()
    roundedRect(CGRect(x: hub.x + 4, y: hub.y - 6, width: 15, height: 10), radius: 2).fill()
  }

  private func drawMailPath(from start: CGPoint, to end: CGPoint, agentIndex: Int) {
    OfficePalette.mail.withAlphaComponent(0.42).setStroke()
    let path = NSBezierPath()
    path.move(to: start)
    path.line(to: CGPoint(x: (start.x + end.x) / 2, y: min(start.y, end.y) - 15 - CGFloat(agentIndex % 3) * 3))
    path.line(to: end)
    path.lineWidth = 1.2
    path.stroke()

    let progress = CGFloat((Int(animationPhase) + agentIndex) % 6) / 5
    let envelope = CGPoint(x: start.x + (end.x - start.x) * progress, y: start.y + (end.y - start.y) * progress)
    OfficePalette.mail.setFill()
    roundedRect(CGRect(x: envelope.x - 5, y: envelope.y - 3, width: 10, height: 7), radius: 2).fill()
  }

  private func drawAgent(_ agent: AgentOfficeRenderAgent, at point: CGPoint, index: Int) {
    let isActive = agent.lifecycle.isActive
    let bob = isActive ? sin((animationPhase + CGFloat(index)) * 0.9) * 2.0 : 0
    let origin = CGPoint(x: point.x, y: point.y + bob)

    NSColor.black.withAlphaComponent(0.16).setFill()
    oval(CGRect(x: origin.x - 12, y: origin.y + 18, width: 24, height: 7)).fill()

    OfficePalette.roleColor(agent.role).setFill()
    OfficePalette.ink.setStroke()
    let body = roundedRect(CGRect(x: origin.x - 8, y: origin.y + 2, width: 16, height: 20), radius: 5)
    body.lineWidth = 1.1
    body.fill()
    body.stroke()

    OfficePalette.skin.setFill()
    let head = roundedRect(CGRect(x: origin.x - 7, y: origin.y - 7, width: 14, height: 13), radius: 5)
    head.lineWidth = 1
    head.fill()
    head.stroke()

    OfficePalette.statusColor(agent.lifecycle).setFill()
    oval(CGRect(x: origin.x + 6, y: origin.y - 9, width: 7, height: 7)).fill()

    if agent.hasTalkReceipt || agent.lifecycle == .postedMail {
      OfficePalette.paper.setFill()
      roundedRect(CGRect(x: origin.x + 8, y: origin.y + 5, width: 10, height: 7), radius: 2).fill()
    }

    drawText(
      agent.activity,
      in: CGRect(x: origin.x - 34, y: origin.y + 27, width: 68, height: 12),
      font: .systemFont(ofSize: 8, weight: .medium),
      color: OfficePalette.ink,
      alignment: .center
    )
  }

  private func deskPositions(in shell: CGRect) -> [String: CGPoint] {
    let topY = shell.minY + 112
    let bottomY = shell.minY + 223
    let columns: [CGFloat] = [59, 157, 255, 353, 451].map { shell.minX + $0 }
    var result: [String: CGPoint] = [:]
    for (index, desk) in desks.enumerated() {
      let column = index % columns.count
      let row = index / columns.count
      result[desk.role] = CGPoint(x: columns[column], y: row == 0 ? topY : bottomY)
    }
    return result
  }

  private func orderedRoles(from roles: Dictionary<String, [AgentOfficeRenderAgent]>.Keys) -> [String] {
    let roleSet = Set(roles)
    let known = desks.map(\.role).filter { roleSet.contains($0) }
    let unknown = roleSet.subtracting(known).sorted()
    return known + unknown
  }

  private func homePosition(for role: String, deskCenters: [String: CGPoint]) -> CGPoint? {
    if let known = deskCenters[role] {
      return known
    }
    if role.contains("scribe") {
      return deskCenters["scribe"]
    }
    if role.contains("bio") {
      return deskCenters["biographer"]
    }
    if role.contains("librarian") || role.contains("source") {
      return deskCenters["librarian"]
    }
    if role.contains("history") {
      return deskCenters["historian"]
    }
    if role.contains("publish") {
      return deskCenters["publisher"]
    }
    return deskCenters["curator"]
  }

  private func agentPosition(_ agent: AgentOfficeRenderAgent, home: CGPoint, index: Int, count: Int) -> CGPoint {
    let columns = min(max(count, 1), 3)
    let column = index % columns
    let row = index / columns
    let x = (CGFloat(column) - CGFloat(columns - 1) / 2) * 18
    let y = -39 - CGFloat(row) * 20

    switch agent.lifecycle {
    case .postedMail:
      return CGPoint(x: home.x + x, y: home.y - 57 - CGFloat(row) * 10)
    case .failed:
      return CGPoint(x: home.x + x, y: home.y - 44 - CGFloat(row) * 20)
    case .waiting, .planned, .idle:
      return CGPoint(x: home.x + x, y: home.y - 31 - CGFloat(row) * 18)
    case .born, .working, .completed, .retired:
      return CGPoint(x: home.x + x, y: home.y + y)
    }
  }

  private func mailHub(in shell: CGRect) -> CGPoint {
    CGPoint(x: shell.midX, y: shell.minY + 167)
  }

  private func roundedRect(_ rect: CGRect, radius: CGFloat) -> NSBezierPath {
    NSBezierPath(roundedRect: rect, xRadius: radius, yRadius: radius)
  }

  private func oval(_ rect: CGRect) -> NSBezierPath {
    NSBezierPath(ovalIn: rect)
  }

  private func drawText(
    _ text: String,
    in rect: CGRect,
    font: NSFont,
    color: NSColor,
    alignment: NSTextAlignment
  ) {
    let paragraph = NSMutableParagraphStyle()
    paragraph.alignment = alignment
    paragraph.lineBreakMode = .byTruncatingTail
    let attributes: [NSAttributedString.Key: Any] = [
      .font: font,
      .foregroundColor: color,
      .paragraphStyle: paragraph
    ]
    NSString(string: text).draw(in: rect, withAttributes: attributes)
  }
}

struct AgentOfficeTelemetryText: Equatable {
  var month: String
  var rate: String
  var status: String

  static let idle = AgentOfficeTelemetryText(month: "Month 0 tokens", rate: "idle tok/s", status: "idle")

  init(month: String, rate: String, status: String) {
    self.month = month
    self.rate = rate
    self.status = status
  }

  init(snapshot: AgentOfficeRunSnapshot) {
    month = "Month \(formatTokenCount(snapshot.monthlyTokenUsage)) tokens"
    if let tokensPerSecond = snapshot.tokensPerSecond, tokensPerSecond > 0 {
      rate = "\(formatTokenRate(tokensPerSecond)) tok/s"
    } else {
      rate = "settled tok/s"
    }

    let active = snapshot.activeTurnCount == 1 ? "1 active" : "\(snapshot.activeTurnCount) active"
    let completed = "\(snapshot.completedCount) done"
    let failed = snapshot.failedCount > 0 ? " | \(snapshot.failedCount) failed" : ""
    status = "\(snapshot.status) | \(active) | \(completed)\(failed)"
  }
}

struct AgentOfficeRenderAgent: Equatable {
  var unitID: String
  var role: String
  var lifecycle: AgentOfficeLifecycle
  var hasTalkReceipt: Bool
  var activity: String

  static func visibleAgents(from agents: [AgentOfficeAgentSnapshot]) -> [AgentOfficeRenderAgent] {
    Array(agents.suffix(18)).map { agent in
      AgentOfficeRenderAgent(
        unitID: agent.unitID,
        role: agent.role,
        lifecycle: agent.lifecycle,
        hasTalkReceipt: agent.hasTalkReceipt,
        activity: activity(for: agent)
      )
    }
  }

  private static func activity(for agent: AgentOfficeAgentSnapshot) -> String {
    let summary = agent.summary.trimmingCharacters(in: .whitespacesAndNewlines)
    if !summary.isEmpty && !genericSummaries.contains(summary.lowercased()) {
      return clamp(summary, to: 18)
    }

    switch agent.lifecycle {
    case .idle:
      return "idle"
    case .planned:
      return "queued"
    case .born:
      return "starting"
    case .working:
      return roleActivity(agent.role)
    case .postedMail:
      return "posting mail"
    case .completed:
      return "done"
    case .waiting:
      return "waiting"
    case .failed:
      return "retry"
    case .retired:
      return "retired"
    }
  }

  private static func roleActivity(_ role: String) -> String {
    switch role {
    case "scribe", "hourly_aggregate":
      return "writing"
    case "daily_editor":
      return "editing"
    case "biographer":
      return "profiling"
    case "context_curator", "curator":
      return "curating"
    case "librarian":
      return "filing"
    case "historian":
      return "reading"
    case "contradiction":
      return "checking"
    case "publisher":
      return "shipping"
    default:
      return "working"
    }
  }
}

private let genericSummaries: Set<String> = [
  "born",
  "done",
  "failed",
  "idle",
  "posted",
  "queued",
  "retired",
  "waiting",
  "working"
]

private enum OfficePalette {
  static let ink = NSColor(calibratedRed: 0.18, green: 0.17, blue: 0.15, alpha: 1)
  static let mutedInk = NSColor(calibratedRed: 0.35, green: 0.36, blue: 0.32, alpha: 1)
  static let paper = NSColor(calibratedRed: 0.98, green: 0.95, blue: 0.86, alpha: 0.96)
  static let wall = NSColor(calibratedRed: 0.74, green: 0.86, blue: 0.77, alpha: 1)
  static let floor = NSColor(calibratedRed: 0.78, green: 0.66, blue: 0.50, alpha: 1)
  static let floorLine = NSColor(calibratedRed: 0.44, green: 0.36, blue: 0.27, alpha: 0.24)
  static let desk = NSColor(calibratedRed: 0.37, green: 0.24, blue: 0.17, alpha: 1)
  static let deskTop = NSColor(calibratedRed: 0.70, green: 0.48, blue: 0.30, alpha: 1)
  static let typewriter = NSColor(calibratedRed: 0.23, green: 0.31, blue: 0.35, alpha: 1)
  static let skin = NSColor(calibratedRed: 0.95, green: 0.74, blue: 0.57, alpha: 1)
  static let mail = NSColor(calibratedRed: 0.35, green: 0.68, blue: 0.92, alpha: 1)
  static let mailTable = NSColor(calibratedRed: 0.25, green: 0.45, blue: 0.48, alpha: 1)
  static let activeGlow = NSColor(calibratedRed: 1.00, green: 0.86, blue: 0.36, alpha: 0.26)
  static let blue = NSColor(calibratedRed: 0.27, green: 0.58, blue: 0.88, alpha: 1)
  static let green = NSColor(calibratedRed: 0.20, green: 0.66, blue: 0.44, alpha: 1)

  static func roleColor(_ role: String) -> NSColor {
    switch role {
    case "scribe":
      return NSColor(calibratedRed: 0.24, green: 0.57, blue: 0.84, alpha: 1)
    case "hourly_aggregate":
      return NSColor(calibratedRed: 0.30, green: 0.69, blue: 0.50, alpha: 1)
    case "daily_editor":
      return NSColor(calibratedRed: 0.84, green: 0.56, blue: 0.24, alpha: 1)
    case "biographer":
      return NSColor(calibratedRed: 0.87, green: 0.42, blue: 0.52, alpha: 1)
    case "context_curator":
      return NSColor(calibratedRed: 0.49, green: 0.48, blue: 0.86, alpha: 1)
    case "librarian":
      return NSColor(calibratedRed: 0.26, green: 0.64, blue: 0.62, alpha: 1)
    case "historian":
      return NSColor(calibratedRed: 0.64, green: 0.48, blue: 0.30, alpha: 1)
    case "contradiction":
      return NSColor(calibratedRed: 0.88, green: 0.27, blue: 0.24, alpha: 1)
    case "publisher":
      return NSColor(calibratedRed: 0.38, green: 0.72, blue: 0.32, alpha: 1)
    default:
      return NSColor(calibratedRed: 0.52, green: 0.56, blue: 0.62, alpha: 1)
    }
  }

  static func statusColor(_ lifecycle: AgentOfficeLifecycle) -> NSColor {
    switch lifecycle {
    case .failed:
      return NSColor(calibratedRed: 0.93, green: 0.16, blue: 0.13, alpha: 1)
    case .waiting:
      return NSColor(calibratedRed: 0.94, green: 0.65, blue: 0.16, alpha: 1)
    case .completed, .postedMail, .retired:
      return NSColor(calibratedRed: 0.27, green: 0.72, blue: 0.35, alpha: 1)
    case .born, .working:
      return NSColor(calibratedRed: 0.25, green: 0.57, blue: 0.90, alpha: 1)
    case .idle, .planned:
      return NSColor(calibratedWhite: 0.78, alpha: 0.9)
    }
  }
}

private func formatTokenCount(_ tokens: Int) -> String {
  if tokens >= 1_000_000 {
    return String(format: "%.1fM", Double(tokens) / 1_000_000)
  }
  if tokens >= 10_000 {
    return String(format: "%.0fk", Double(tokens) / 1_000)
  }
  if tokens >= 1_000 {
    return String(format: "%.1fk", Double(tokens) / 1_000)
  }
  return "\(tokens)"
}

private func formatTokenRate(_ tokensPerSecond: Double) -> String {
  if tokensPerSecond >= 1_000 {
    return String(format: "%.1fk", tokensPerSecond / 1_000)
  }
  return String(format: "%.0f", tokensPerSecond)
}

private func clamp(_ text: String, to limit: Int) -> String {
  guard text.count > limit else { return text }
  return String(text.prefix(max(0, limit - 3))) + "..."
}
