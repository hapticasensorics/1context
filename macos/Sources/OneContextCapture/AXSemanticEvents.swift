import Foundation

public struct CaptureAXScrollBarContext: Codable, Equatable, Sendable {
  public var orientation: String
  public var role: String?
  public var isEnabled: Bool?
  public var value: Double?
  public var minValue: Double?
  public var maxValue: Double?
  public var valuePercent: Double?

  public init(
    orientation: String,
    role: String? = nil,
    isEnabled: Bool? = nil,
    value: Double? = nil,
    minValue: Double? = nil,
    maxValue: Double? = nil,
    valuePercent: Double? = nil
  ) {
    self.orientation = orientation
    self.role = role
    self.isEnabled = isEnabled
    self.value = value
    self.minValue = minValue
    self.maxValue = maxValue
    self.valuePercent = valuePercent
  }
}

public struct CaptureAXScrollContext: Codable, Equatable, Sendable {
  public var vertical: CaptureAXScrollBarContext?
  public var horizontal: CaptureAXScrollBarContext?

  public init(
    vertical: CaptureAXScrollBarContext? = nil,
    horizontal: CaptureAXScrollBarContext? = nil
  ) {
    self.vertical = vertical
    self.horizontal = horizontal
  }
}

public enum CaptureAXTransientUIKind: String, Codable, Equatable, Sendable {
  case modal
  case sheet
  case menu
  case popover
  case dialog
  case transient
}

public struct CaptureAXTransientUIState: Codable, Equatable, Sendable {
  public var kind: CaptureAXTransientUIKind
  public var visible: Bool
  public var source: String
  public var role: String?
  public var subrole: String?
  public var title: String?
  public var isModal: Bool?

  public init(
    kind: CaptureAXTransientUIKind,
    visible: Bool = true,
    source: String,
    role: String? = nil,
    subrole: String? = nil,
    title: String? = nil,
    isModal: Bool? = nil
  ) {
    self.kind = kind
    self.visible = visible
    self.source = source
    self.role = role
    self.subrole = subrole
    self.title = title
    self.isModal = isModal
  }
}

public enum CaptureAXSemanticEventKind: String, Codable, Equatable, Hashable, Sendable {
  case focusedWindowChanged = "focused_window_changed"
  case focusedElementChanged = "focused_element_changed"
  case valueChanged = "value_changed"
  case selectedTextChanged = "selected_text_changed"
  case transientUIStateChanged = "transient_ui_state_changed"

  public var captureEventType: String {
    "capture.ax_semantic.\(rawValue).v1"
  }
}

public struct CaptureAXSemanticEvent: Codable, Equatable, Sendable {
  public var schemaVersion: Int
  public var eventType: String
  public var kind: CaptureAXSemanticEventKind
  public var generatedAt: String
  public var source: String
  public var activeApplication: CaptureActiveApplication?
  public var focusedApplicationProcessID: Int32?
  public var matchedWindowID: UInt32?
  public var focusedWindow: CaptureAXNodeContext?
  public var focusedElement: CaptureAXNodeContext?
  public var valueShape: CaptureAXValueShape?
  public var selection: CaptureAXSelectionContext?
  public var transientUIState: CaptureAXTransientUIState?
  public var issues: [CaptureAXFocusedContextIssue]

  public init(
    schemaVersion: Int = 1,
    kind: CaptureAXSemanticEventKind,
    generatedAt: String,
    source: String = "ax_semantic_aggregator",
    eventType: String? = nil,
    activeApplication: CaptureActiveApplication? = nil,
    focusedApplicationProcessID: Int32? = nil,
    matchedWindowID: UInt32? = nil,
    focusedWindow: CaptureAXNodeContext? = nil,
    focusedElement: CaptureAXNodeContext? = nil,
    valueShape: CaptureAXValueShape? = nil,
    selection: CaptureAXSelectionContext? = nil,
    transientUIState: CaptureAXTransientUIState? = nil,
    issues: [CaptureAXFocusedContextIssue] = []
  ) {
    self.schemaVersion = schemaVersion
    self.eventType = eventType ?? kind.captureEventType
    self.kind = kind
    self.generatedAt = generatedAt
    self.source = source
    self.activeApplication = activeApplication
    self.focusedApplicationProcessID = focusedApplicationProcessID
    self.matchedWindowID = matchedWindowID
    self.focusedWindow = focusedWindow
    self.focusedElement = focusedElement
    self.valueShape = valueShape
    self.selection = selection
    self.transientUIState = transientUIState
    self.issues = issues
  }
}

public struct CaptureAXSemanticEventBufferSnapshot: Codable, Equatable, Sendable {
  public var capacity: Int
  public var eventCount: Int
  public var droppedCount: Int
  public var emittedCount: Int

  public init(capacity: Int, eventCount: Int, droppedCount: Int, emittedCount: Int) {
    self.capacity = capacity
    self.eventCount = eventCount
    self.droppedCount = droppedCount
    self.emittedCount = emittedCount
  }
}

public final class AXSemanticEventAggregator: @unchecked Sendable {
  public let capacity: Int

  private let lock = NSLock()
  private var events: [CaptureAXSemanticEvent] = []
  private var droppedCount = 0
  private var emittedCount = 0
  private var lastFocusedWindowSignature: String?
  private var lastFocusedElementSignature: String?
  private var lastValueSignature: String?
  private var lastSelectionSignature: String?
  private var lastTransientUISignature: TransientUISignature?

  public init(capacity: Int = 128) {
    self.capacity = max(1, capacity)
  }

  public func ingest(_ context: CaptureAXFocusedContext) -> [CaptureAXSemanticEvent] {
    lock.withLock {
      var emitted: [CaptureAXSemanticEvent] = []
      ingestLocked(context, into: &emitted)
      appendLocked(emitted)
      return emitted
    }
  }

  public func recentEvents(limit: Int? = nil) -> [CaptureAXSemanticEvent] {
    lock.withLock {
      guard let limit else { return events }
      return Array(events.suffix(max(0, limit)))
    }
  }

  public func snapshot() -> CaptureAXSemanticEventBufferSnapshot {
    lock.withLock {
      CaptureAXSemanticEventBufferSnapshot(
        capacity: capacity,
        eventCount: events.count,
        droppedCount: droppedCount,
        emittedCount: emittedCount
      )
    }
  }

  private func ingestLocked(
    _ context: CaptureAXFocusedContext,
    into emitted: inout [CaptureAXSemanticEvent]
  ) {
    let focusedWindowSignature = context.focusedWindow.map {
      nodeSignature($0, extra: [
        context.focusedApplicationProcessID.map(String.init),
        context.matchedWindowID.map(String.init)
      ])
    }
    emitIfChanged(
      signature: focusedWindowSignature,
      lastSignature: &lastFocusedWindowSignature,
      event: makeEvent(.focusedWindowChanged, context: context, window: context.focusedWindow),
      into: &emitted
    )

    let focusedElementSignature = context.focusedElement.map {
      nodeSignature($0, extra: [context.focusedApplicationProcessID.map(String.init)])
    }
    emitIfChanged(
      signature: focusedElementSignature,
      lastSignature: &lastFocusedElementSignature,
      event: makeEvent(.focusedElementChanged, context: context, element: context.focusedElement),
      into: &emitted
    )

    let valueSignature = context.focusedElement?.valueShape.map(valueShapeSignature)
    emitIfChanged(
      signature: valueSignature,
      lastSignature: &lastValueSignature,
      event: makeEvent(.valueChanged, context: context, element: context.focusedElement, valueShape: context.focusedElement?.valueShape),
      into: &emitted
    )

    let selectionSignature = context.focusedElement?.selection.map(selectionSignature)
    emitIfChanged(
      signature: selectionSignature,
      lastSignature: &lastSelectionSignature,
      event: makeEvent(.selectedTextChanged, context: context, element: context.focusedElement, selection: context.focusedElement?.selection),
      into: &emitted
    )

    let currentTransientSignature: TransientUISignature?
    if let transientState = context.primaryTransientUIState {
      currentTransientSignature = TransientUISignature(key: transientUISignature(transientState), state: transientState)
    } else {
      currentTransientSignature = nil
    }
    switch (lastTransientUISignature, currentTransientSignature) {
    case (nil, nil):
      break
    case (_, .some(let newSignature)) where lastTransientUISignature?.key == newSignature.key:
      break
    case (_, .some(let newSignature)):
      if let event = makeEvent(.transientUIStateChanged, context: context, transientUIState: newSignature.state) {
        emitted.append(event)
      }
      lastTransientUISignature = newSignature
    case (.some(let oldSignature), nil):
      var closedState = oldSignature.state
      closedState.visible = false
      if let event = makeEvent(.transientUIStateChanged, context: context, transientUIState: closedState) {
        emitted.append(event)
      }
      lastTransientUISignature = nil
    }
  }

  private func emitIfChanged(
    signature: String?,
    lastSignature: inout String?,
    event: CaptureAXSemanticEvent?,
    into emitted: inout [CaptureAXSemanticEvent]
  ) {
    guard let signature, let event else {
      lastSignature = nil
      return
    }
    guard signature != lastSignature else {
      return
    }
    lastSignature = signature
    emitted.append(event)
  }

  private func makeEvent(
    _ kind: CaptureAXSemanticEventKind,
    context: CaptureAXFocusedContext,
    window: CaptureAXNodeContext? = nil,
    element: CaptureAXNodeContext? = nil,
    valueShape: CaptureAXValueShape? = nil,
    selection: CaptureAXSelectionContext? = nil,
    transientUIState: CaptureAXTransientUIState? = nil
  ) -> CaptureAXSemanticEvent? {
    switch kind {
    case .focusedWindowChanged where window == nil:
      return nil
    case .focusedElementChanged where element == nil:
      return nil
    case .valueChanged where valueShape == nil:
      return nil
    case .selectedTextChanged where selection == nil:
      return nil
    case .transientUIStateChanged where transientUIState == nil:
      return nil
    default:
      return CaptureAXSemanticEvent(
        kind: kind,
        generatedAt: context.generatedAt,
        activeApplication: context.activeApplication,
        focusedApplicationProcessID: context.focusedApplicationProcessID,
        matchedWindowID: context.matchedWindowID,
        focusedWindow: window,
        focusedElement: element,
        valueShape: valueShape,
        selection: selection,
        transientUIState: transientUIState,
        issues: context.issues
      )
    }
  }

  private func appendLocked(_ newEvents: [CaptureAXSemanticEvent]) {
    guard !newEvents.isEmpty else { return }
    events.append(contentsOf: newEvents)
    emittedCount += newEvents.count
    if events.count > capacity {
      let overflow = events.count - capacity
      events.removeFirst(overflow)
      droppedCount += overflow
    }
  }

  private func nodeSignature(_ node: CaptureAXNodeContext, extra: [String?] = []) -> String {
    [
      extra.compactMap { $0 }.joined(separator: "|"),
      node.role ?? "",
      node.subrole ?? "",
      node.title ?? "",
      node.identifier ?? "",
      node.elementDescription ?? "",
      rectSignature(node.frame),
      node.isSensitive ? "sensitive" : "not_sensitive",
      node.redactionReasons.joined(separator: ",")
    ].joined(separator: "\u{1f}")
  }

  private func valueShapeSignature(_ shape: CaptureAXValueShape) -> String {
    [
      shape.kind,
      shape.characterCount.map(String.init) ?? "",
      shape.sourceAttribute ?? "",
      shape.redacted ? "redacted" : "clear"
    ].joined(separator: "\u{1f}")
  }

  private func selectionSignature(_ selection: CaptureAXSelectionContext) -> String {
    [
      rangeSignature(selection.range),
      selection.isInsertionPoint.map(String.init) ?? "",
      selection.selectedText ?? "",
      selection.selectedTextCharacterCount.map(String.init) ?? "",
      selection.selectedTextTruncated ? "truncated" : "complete",
      selection.selectedTextRedacted ? "redacted" : "clear"
    ].joined(separator: "\u{1f}")
  }

  private func transientUISignature(_ state: CaptureAXTransientUIState) -> String {
    [
      state.kind.rawValue,
      state.visible ? "visible" : "hidden",
      state.source,
      state.role ?? "",
      state.subrole ?? "",
      state.title ?? "",
      state.isModal.map(String.init) ?? ""
    ].joined(separator: "\u{1f}")
  }

  private func rectSignature(_ rect: CaptureRect?) -> String {
    guard let rect else { return "" }
    return [
      rounded(rect.x),
      rounded(rect.y),
      rounded(rect.width),
      rounded(rect.height)
    ].joined(separator: ",")
  }

  private func rangeSignature(_ range: CaptureAXTextRange?) -> String {
    guard let range else { return "" }
    return "\(range.location),\(range.length)"
  }

  private func rounded(_ value: Double) -> String {
    String(format: "%.2f", value)
  }
}

public extension CaptureAXFocusedContext {
  var primaryTransientUIState: CaptureAXTransientUIState? {
    focusedElement?.transientUI ?? focusedWindow?.transientUI
  }
}

private struct TransientUISignature {
  var key: String
  var state: CaptureAXTransientUIState
}
