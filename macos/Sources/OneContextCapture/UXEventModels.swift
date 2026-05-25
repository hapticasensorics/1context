import CoreGraphics
import Foundation

public enum UXEventPrimitiveKind: String, Equatable, Sendable {
  case scrollWheel = "scroll_wheel"
  case leftMouseDown = "left_mouse_down"
  case leftMouseUp = "left_mouse_up"
  case rightMouseDown = "right_mouse_down"
  case rightMouseUp = "right_mouse_up"
  case otherMouseDown = "other_mouse_down"
  case otherMouseUp = "other_mouse_up"
  case leftMouseDragged = "left_mouse_dragged"
  case rightMouseDragged = "right_mouse_dragged"
  case otherMouseDragged = "other_mouse_dragged"
  case flagsChanged = "flags_changed"
  case keyDown = "key_down"
  case keyUp = "key_up"
}

public enum UXPointerButton: String, Codable, Equatable, Sendable {
  case left
  case right
  case other
  case unknown
}

public struct UXEventPrimitive: Equatable, Sendable {
  public var time: Date
  public var kind: UXEventPrimitiveKind
  public var locationX: Double?
  public var locationY: Double?
  public var scrollDeltaX: Double
  public var scrollDeltaY: Double
  public var modifierFlagsRaw: UInt64
  public var isAutoRepeat: Bool
  public var isMomentumScroll: Bool
  public var clickState: Int
  public var targetProcessID: Int32?

  public init(
    time: Date,
    kind: UXEventPrimitiveKind,
    locationX: Double? = nil,
    locationY: Double? = nil,
    scrollDeltaX: Double = 0,
    scrollDeltaY: Double = 0,
    modifierFlagsRaw: UInt64 = 0,
    isAutoRepeat: Bool = false,
    isMomentumScroll: Bool = false,
    clickState: Int = 0,
    targetProcessID: Int32? = nil
  ) {
    self.time = time
    self.kind = kind
    self.locationX = locationX
    self.locationY = locationY
    self.scrollDeltaX = scrollDeltaX
    self.scrollDeltaY = scrollDeltaY
    self.modifierFlagsRaw = modifierFlagsRaw
    self.isAutoRepeat = isAutoRepeat
    self.isMomentumScroll = isMomentumScroll
    self.clickState = clickState
    self.targetProcessID = targetProcessID
  }
}

public enum UXEventAnchorKind: String, Codable, Equatable, Hashable, Sendable {
  case scrollBurst = "scroll_burst"
  case pointer = "pointer"
  case modifiers = "modifiers"
  case keyboardActivity = "keyboard_activity"

  public var captureEventType: String {
    switch self {
    case .scrollBurst:
      return "capture.ux.scroll_burst.v1"
    case .pointer:
      return "capture.ux.pointer.v1"
    case .modifiers:
      return "capture.ux.modifiers.v1"
    case .keyboardActivity:
      return "capture.ux.keyboard_activity.v1"
    }
  }
}

public enum UXPointerAction: String, Codable, Equatable, Sendable {
  case down
  case up
  case click
  case drag
}

public enum UXPointerDominantAxis: String, Codable, Equatable, Sendable {
  case horizontal
  case vertical
  case mixed
  case none
}

public struct UXScrollBurstSummary: Codable, Equatable, Sendable {
  public var eventCount: Int
  public var totalDX: Double
  public var totalDY: Double
  public var maxAbsDY: Double
  public var momentumEventCount: Int
  public var durationMilliseconds: Int

  public init(
    eventCount: Int,
    totalDX: Double,
    totalDY: Double,
    maxAbsDY: Double,
    momentumEventCount: Int,
    durationMilliseconds: Int
  ) {
    self.eventCount = eventCount
    self.totalDX = totalDX
    self.totalDY = totalDY
    self.maxAbsDY = maxAbsDY
    self.momentumEventCount = momentumEventCount
    self.durationMilliseconds = durationMilliseconds
  }

  enum CodingKeys: String, CodingKey {
    case eventCount = "event_count"
    case totalDX = "total_dx"
    case totalDY = "total_dy"
    case maxAbsDY = "max_abs_dy"
    case momentumEventCount = "momentum_event_count"
    case durationMilliseconds = "duration_ms"
  }
}

public struct UXPointerSummary: Codable, Equatable, Sendable {
  public var action: UXPointerAction
  public var button: UXPointerButton
  public var eventCount: Int
  public var durationMilliseconds: Int
  public var distancePoints: Double
  public var dominantAxis: UXPointerDominantAxis
  public var clickCount: Int

  public init(
    action: UXPointerAction,
    button: UXPointerButton,
    eventCount: Int,
    durationMilliseconds: Int,
    distancePoints: Double,
    dominantAxis: UXPointerDominantAxis,
    clickCount: Int
  ) {
    self.action = action
    self.button = button
    self.eventCount = eventCount
    self.durationMilliseconds = durationMilliseconds
    self.distancePoints = distancePoints
    self.dominantAxis = dominantAxis
    self.clickCount = clickCount
  }

  enum CodingKeys: String, CodingKey {
    case action
    case button
    case eventCount = "event_count"
    case durationMilliseconds = "duration_ms"
    case distancePoints = "distance_points"
    case dominantAxis = "dominant_axis"
    case clickCount = "click_count"
  }
}

public struct UXModifierSummary: Codable, Equatable, Sendable {
  public var activeModifiers: [String]
  public var changedModifiers: [String]

  public init(activeModifiers: [String], changedModifiers: [String]) {
    self.activeModifiers = activeModifiers
    self.changedModifiers = changedModifiers
  }

  enum CodingKeys: String, CodingKey {
    case activeModifiers = "active_modifiers"
    case changedModifiers = "changed_modifiers"
  }
}

public struct UXKeyboardActivitySummary: Codable, Equatable, Sendable {
  public var eventCount: Int
  public var keyDownCount: Int
  public var keyUpCount: Int
  public var autoRepeatCount: Int
  public var modifiedKeyEventCount: Int
  public var durationMilliseconds: Int

  public init(
    eventCount: Int,
    keyDownCount: Int,
    keyUpCount: Int,
    autoRepeatCount: Int,
    modifiedKeyEventCount: Int,
    durationMilliseconds: Int
  ) {
    self.eventCount = eventCount
    self.keyDownCount = keyDownCount
    self.keyUpCount = keyUpCount
    self.autoRepeatCount = autoRepeatCount
    self.modifiedKeyEventCount = modifiedKeyEventCount
    self.durationMilliseconds = durationMilliseconds
  }

  enum CodingKeys: String, CodingKey {
    case eventCount = "event_count"
    case keyDownCount = "key_down_count"
    case keyUpCount = "key_up_count"
    case autoRepeatCount = "auto_repeat_count"
    case modifiedKeyEventCount = "modified_key_event_count"
    case durationMilliseconds = "duration_ms"
  }
}

public struct UXEventAnchor: Codable, Equatable, Sendable {
  public var schemaVersion: Int
  public var kind: UXEventAnchorKind
  public var source: String
  public var startedAt: String
  public var endedAt: String
  public var recentTargetProcessID: Int32?
  public var scroll: UXScrollBurstSummary?
  public var pointer: UXPointerSummary?
  public var modifiers: UXModifierSummary?
  public var keyboardActivity: UXKeyboardActivitySummary?

  public init(
    schemaVersion: Int = 1,
    kind: UXEventAnchorKind,
    source: String = "cg_event_tap",
    startedAt: String,
    endedAt: String,
    recentTargetProcessID: Int32? = nil,
    scroll: UXScrollBurstSummary? = nil,
    pointer: UXPointerSummary? = nil,
    modifiers: UXModifierSummary? = nil,
    keyboardActivity: UXKeyboardActivitySummary? = nil
  ) {
    self.schemaVersion = schemaVersion
    self.kind = kind
    self.source = source
    self.startedAt = startedAt
    self.endedAt = endedAt
    self.recentTargetProcessID = recentTargetProcessID
    self.scroll = scroll
    self.pointer = pointer
    self.modifiers = modifiers
    self.keyboardActivity = keyboardActivity
  }

  public var captureEventType: String {
    kind.captureEventType
  }

  enum CodingKeys: String, CodingKey {
    case schemaVersion = "schema_version"
    case kind
    case source
    case startedAt = "started_at"
    case endedAt = "ended_at"
    case recentTargetProcessID = "recent_target_process_id"
    case scroll
    case pointer
    case modifiers
    case keyboardActivity = "keyboard_activity"
  }
}

public struct UXMotionHints: Codable, Equatable, Sendable {
  public var generatedAt: String
  public var scrollEventRecently: Bool
  public var keyboardActivityRecently: Bool
  public var estimatedScrollDY: Double
  public var focusedRecently: Bool
  public var recentTargetProcessID: Int32?

  public init(
    generatedAt: String,
    scrollEventRecently: Bool,
    keyboardActivityRecently: Bool,
    estimatedScrollDY: Double,
    focusedRecently: Bool,
    recentTargetProcessID: Int32? = nil
  ) {
    self.generatedAt = generatedAt
    self.scrollEventRecently = scrollEventRecently
    self.keyboardActivityRecently = keyboardActivityRecently
    self.estimatedScrollDY = estimatedScrollDY
    self.focusedRecently = focusedRecently
    self.recentTargetProcessID = recentTargetProcessID
  }

  enum CodingKeys: String, CodingKey {
    case generatedAt
    case scrollEventRecently
    case keyboardActivityRecently
    case estimatedScrollDY
    case focusedRecently
    case recentTargetProcessID = "recent_target_process_id"
  }
}

public struct UXEventQueueSnapshot: Codable, Equatable, Sendable {
  public var capacity: Int
  public var queueDepth: Int
  public var droppedCount: Int
  public var enqueuedCount: Int

  public init(capacity: Int, queueDepth: Int, droppedCount: Int, enqueuedCount: Int) {
    self.capacity = capacity
    self.queueDepth = queueDepth
    self.droppedCount = droppedCount
    self.enqueuedCount = enqueuedCount
  }

  enum CodingKeys: String, CodingKey {
    case capacity
    case queueDepth = "queue_depth"
    case droppedCount = "dropped_count"
    case enqueuedCount = "enqueued_count"
  }
}

public final class UXEventPrimitiveRingBuffer: @unchecked Sendable {
  public let capacity: Int

  private let lock = NSLock()
  private var storage: [UXEventPrimitive?]
  private var head = 0
  private var count = 0
  private var dropped = 0
  private var enqueued = 0

  public init(capacity: Int = 512) {
    self.capacity = max(1, capacity)
    self.storage = Array(repeating: nil, count: max(1, capacity))
  }

  public func push(_ event: UXEventPrimitive) {
    lock.withLock {
      if count == capacity {
        storage[head] = event
        head = (head + 1) % capacity
        dropped += 1
      } else {
        storage[(head + count) % capacity] = event
        count += 1
      }
      enqueued += 1
    }
  }

  public func drain() -> [UXEventPrimitive] {
    lock.withLock {
      guard count > 0 else { return [] }
      let drainedCount = count
      var drained: [UXEventPrimitive] = []
      drained.reserveCapacity(drainedCount)

      let firstRunCount = min(drainedCount, capacity - head)
      for offset in 0..<firstRunCount {
        let index = head + offset
        if let event = storage[index] {
          drained.append(event)
        }
        storage[index] = nil
      }
      if firstRunCount < drainedCount {
        for index in 0..<(drainedCount - firstRunCount) {
          if let event = storage[index] {
            drained.append(event)
          }
          storage[index] = nil
        }
      }

      head = 0
      count = 0
      return drained
    }
  }

  public func snapshot() -> UXEventQueueSnapshot {
    lock.withLock {
      UXEventQueueSnapshot(
        capacity: capacity,
        queueDepth: count,
        droppedCount: dropped,
        enqueuedCount: enqueued
      )
    }
  }
}

public struct UXEventAggregationSnapshot: Equatable, Sendable {
  public var coalescedCount: Int
  public var emittedAnchorCount: Int

  public init(coalescedCount: Int, emittedAnchorCount: Int) {
    self.coalescedCount = coalescedCount
    self.emittedAnchorCount = emittedAnchorCount
  }
}

struct UXEventAggregatorCurrentState: Equatable, Sendable {
  var motionHints: UXMotionHints
  var aggregationSnapshot: UXEventAggregationSnapshot

  init(motionHints: UXMotionHints, aggregationSnapshot: UXEventAggregationSnapshot) {
    self.motionHints = motionHints
    self.aggregationSnapshot = aggregationSnapshot
  }
}

public final class UXEventAggregator: @unchecked Sendable {
  public var scrollBurstGap: TimeInterval
  public var keyboardBurstGap: TimeInterval
  public var modifierDebounce: TimeInterval
  public var recentWindow: TimeInterval

  private let lock = NSLock()
  private var scrollBurst: ScrollBurstState?
  private var pointerState: PointerState?
  private var keyboardBurst: KeyboardBurstState?
  private var lastModifierRaw: UInt64 = 0
  private var lastModifierEmittedAt: Date = .distantPast
  private var lastScrollAt: Date?
  private var lastKeyboardAt: Date?
  private var lastFocusAt: Date?
  private var lastTargetProcessID: Int32?
  private var lastTargetProcessAt: Date?
  private var lastEstimatedScrollDY: Double = 0
  private var coalesced = 0
  private var emitted = 0

  public init(
    scrollBurstGap: TimeInterval = 0.18,
    keyboardBurstGap: TimeInterval = 0.35,
    modifierDebounce: TimeInterval = 0.08,
    recentWindow: TimeInterval = 1.25
  ) {
    self.scrollBurstGap = scrollBurstGap
    self.keyboardBurstGap = keyboardBurstGap
    self.modifierDebounce = modifierDebounce
    self.recentWindow = recentWindow
  }

  public func ingest(_ primitives: [UXEventPrimitive], now: Date = Date()) -> [UXEventAnchor] {
    let orderedPrimitives = Self.chronologicalPrimitives(primitives)
    return lock.withLock {
      var anchors: [UXEventAnchor] = []
      for event in orderedPrimitives {
        finalizeExpired(before: event.time, into: &anchors)
        ingestLocked(event, into: &anchors)
      }
      finalizeExpired(before: now, into: &anchors)
      emitted += anchors.count
      return anchors
    }
  }

  public func flush(now: Date = Date()) -> [UXEventAnchor] {
    lock.withLock {
      var anchors: [UXEventAnchor] = []
      finalizeScrollBurst(into: &anchors)
      finalizeKeyboardBurst(into: &anchors)
      emitted += anchors.count
      _ = now
      return anchors
    }
  }

  public func motionHints(now: Date = Date()) -> UXMotionHints {
    lock.withLock {
      motionHintsLocked(now: now)
    }
  }

  public func snapshot() -> UXEventAggregationSnapshot {
    lock.withLock {
      aggregationSnapshotLocked()
    }
  }

  func currentState(now: Date = Date()) -> UXEventAggregatorCurrentState {
    lock.withLock {
      UXEventAggregatorCurrentState(
        motionHints: motionHintsLocked(now: now),
        aggregationSnapshot: aggregationSnapshotLocked()
      )
    }
  }

  private static func chronologicalPrimitives(_ primitives: [UXEventPrimitive]) -> [UXEventPrimitive] {
    guard primitives.count > 1 else { return primitives }
    for index in 1..<primitives.count where primitives[index].time < primitives[index - 1].time {
      return primitives.sorted { $0.time < $1.time }
    }
    return primitives
  }

  private func motionHintsLocked(now: Date) -> UXMotionHints {
    UXMotionHints(
      generatedAt: UXEventTime.isoString(now),
      scrollEventRecently: lastScrollAt.map { now.timeIntervalSince($0) <= recentWindow } ?? false,
      keyboardActivityRecently: lastKeyboardAt.map { now.timeIntervalSince($0) <= recentWindow } ?? false,
      estimatedScrollDY: lastEstimatedScrollDY,
      focusedRecently: lastFocusAt.map { now.timeIntervalSince($0) <= recentWindow } ?? false,
      recentTargetProcessID: lastTargetProcessAt.map { now.timeIntervalSince($0) <= recentWindow } == true
        ? lastTargetProcessID
        : nil
    )
  }

  private func aggregationSnapshotLocked() -> UXEventAggregationSnapshot {
    UXEventAggregationSnapshot(coalescedCount: coalesced, emittedAnchorCount: emitted)
  }

  private func ingestLocked(_ event: UXEventPrimitive, into anchors: inout [UXEventAnchor]) {
    switch event.kind {
    case .scrollWheel:
      recordTargetProcess(for: event)
      ingestScroll(event)
    case .leftMouseDown, .rightMouseDown, .otherMouseDown:
      recordTargetProcess(for: event)
      finalizePointer(into: &anchors)
      let button = pointerButton(for: event.kind)
      pointerState = PointerState(
        button: button,
        startedAt: event.time,
        endedAt: event.time,
        startX: event.locationX,
        startY: event.locationY,
        lastX: event.locationX,
        lastY: event.locationY,
        eventCount: 1,
        clickCount: max(1, event.clickState),
        dragged: false,
        recentTargetProcessID: sanitizedTargetProcessID(for: event)
      )
      lastFocusAt = event.time
      anchors.append(pointerAnchor(action: .down, event: event, button: button))
    case .leftMouseUp, .rightMouseUp, .otherMouseUp:
      recordTargetProcess(for: event)
      lastFocusAt = event.time
      if let pointerState, pointerState.button == pointerButton(for: event.kind) {
        var finished = pointerState
        finished.endedAt = event.time
        finished.lastX = event.locationX ?? finished.lastX
        finished.lastY = event.locationY ?? finished.lastY
        finished.eventCount += 1
        finished.clickCount = max(finished.clickCount, event.clickState)
        if let targetProcessID = sanitizedTargetProcessID(for: event) {
          finished.recentTargetProcessID = targetProcessID
        }
        anchors.append(pointerAnchor(from: finished, fallbackAction: finished.isClick ? .click : .up))
        self.pointerState = nil
      } else {
        anchors.append(pointerAnchor(action: .up, event: event, button: pointerButton(for: event.kind)))
      }
    case .leftMouseDragged, .rightMouseDragged, .otherMouseDragged:
      recordTargetProcess(for: event)
      lastFocusAt = event.time
      if var state = pointerState {
        state.endedAt = event.time
        state.lastX = event.locationX ?? state.lastX
        state.lastY = event.locationY ?? state.lastY
        state.eventCount += 1
        state.dragged = true
        if let targetProcessID = sanitizedTargetProcessID(for: event) {
          state.recentTargetProcessID = targetProcessID
        }
        pointerState = state
        coalesced += 1
      } else {
        anchors.append(pointerAnchor(action: .drag, event: event, button: pointerButton(for: event.kind)))
      }
    case .flagsChanged:
      recordTargetProcess(for: event)
      ingestModifier(event, into: &anchors)
    case .keyDown, .keyUp:
      ingestKeyboard(event)
    }
  }

  private func ingestScroll(_ event: UXEventPrimitive) {
    lastScrollAt = event.time
    lastEstimatedScrollDY = event.scrollDeltaY
    let eventTargetProcessID = sanitizedTargetProcessID(for: event)
    if var burst = scrollBurst {
      burst.endedAt = event.time
      burst.eventCount += 1
      burst.totalDX += event.scrollDeltaX
      burst.totalDY += event.scrollDeltaY
      burst.maxAbsDY = max(burst.maxAbsDY, abs(event.scrollDeltaY))
      if event.isMomentumScroll {
        burst.momentumEventCount += 1
      }
      if let eventTargetProcessID {
        burst.recentTargetProcessID = eventTargetProcessID
      }
      scrollBurst = burst
      coalesced += 1
    } else {
      scrollBurst = ScrollBurstState(
        startedAt: event.time,
        endedAt: event.time,
        eventCount: 1,
        totalDX: event.scrollDeltaX,
        totalDY: event.scrollDeltaY,
        maxAbsDY: abs(event.scrollDeltaY),
        momentumEventCount: event.isMomentumScroll ? 1 : 0,
        recentTargetProcessID: eventTargetProcessID
      )
    }
  }

  private func ingestKeyboard(_ event: UXEventPrimitive) {
    recordTargetProcess(for: event)
    lastKeyboardAt = event.time
    lastFocusAt = event.time
    let modified = UXModifierNames.modifiedKeyMaskActive(event.modifierFlagsRaw)
    if var burst = keyboardBurst {
      burst.endedAt = event.time
      burst.eventCount += 1
      burst.keyDownCount += event.kind == .keyDown ? 1 : 0
      burst.keyUpCount += event.kind == .keyUp ? 1 : 0
      burst.autoRepeatCount += event.isAutoRepeat ? 1 : 0
      burst.modifiedKeyEventCount += modified ? 1 : 0
      if let targetProcessID = sanitizedTargetProcessID(for: event) {
        burst.recentTargetProcessID = targetProcessID
      }
      keyboardBurst = burst
      coalesced += 1
    } else {
      keyboardBurst = KeyboardBurstState(
        startedAt: event.time,
        endedAt: event.time,
        eventCount: 1,
        keyDownCount: event.kind == .keyDown ? 1 : 0,
        keyUpCount: event.kind == .keyUp ? 1 : 0,
        autoRepeatCount: event.isAutoRepeat ? 1 : 0,
        modifiedKeyEventCount: modified ? 1 : 0,
        recentTargetProcessID: sanitizedTargetProcessID(for: event)
      )
    }
  }

  private func recordTargetProcess(for event: UXEventPrimitive) {
    guard let targetProcessID = sanitizedTargetProcessID(for: event) else {
      return
    }
    lastTargetProcessID = targetProcessID
    lastTargetProcessAt = event.time
  }

  private func sanitizedTargetProcessID(for event: UXEventPrimitive) -> Int32? {
    guard let targetProcessID = event.targetProcessID, targetProcessID > 0 else {
      return nil
    }
    return targetProcessID
  }

  private func ingestModifier(_ event: UXEventPrimitive, into anchors: inout [UXEventAnchor]) {
    let raw = event.modifierFlagsRaw
    let changedRaw = raw ^ lastModifierRaw
    guard changedRaw != 0 else { return }
    guard event.time.timeIntervalSince(lastModifierEmittedAt) >= modifierDebounce else {
      lastModifierRaw = raw
      coalesced += 1
      return
    }

    anchors.append(
      UXEventAnchor(
        kind: .modifiers,
        startedAt: UXEventTime.isoString(event.time),
        endedAt: UXEventTime.isoString(event.time),
        recentTargetProcessID: sanitizedTargetProcessID(for: event),
        modifiers: UXModifierSummary(
          activeModifiers: UXModifierNames.names(raw),
          changedModifiers: UXModifierNames.names(changedRaw)
        )
      )
    )
    lastModifierRaw = raw
    lastModifierEmittedAt = event.time
  }

  private func finalizeExpired(before time: Date, into anchors: inout [UXEventAnchor]) {
    if let burst = scrollBurst, time.timeIntervalSince(burst.endedAt) >= scrollBurstGap {
      finalizeScrollBurst(into: &anchors)
    }
    if let burst = keyboardBurst, time.timeIntervalSince(burst.endedAt) >= keyboardBurstGap {
      finalizeKeyboardBurst(into: &anchors)
    }
  }

  private func finalizeScrollBurst(into anchors: inout [UXEventAnchor]) {
    guard let burst = scrollBurst else { return }
    anchors.append(
      UXEventAnchor(
        kind: .scrollBurst,
        startedAt: UXEventTime.isoString(burst.startedAt),
        endedAt: UXEventTime.isoString(burst.endedAt),
        recentTargetProcessID: burst.recentTargetProcessID,
        scroll: UXScrollBurstSummary(
          eventCount: burst.eventCount,
          totalDX: rounded(burst.totalDX),
          totalDY: rounded(burst.totalDY),
          maxAbsDY: rounded(burst.maxAbsDY),
          momentumEventCount: burst.momentumEventCount,
          durationMilliseconds: durationMilliseconds(from: burst.startedAt, to: burst.endedAt)
        )
      )
    )
    scrollBurst = nil
  }

  private func finalizeKeyboardBurst(into anchors: inout [UXEventAnchor]) {
    guard let burst = keyboardBurst else { return }
    anchors.append(
      UXEventAnchor(
        kind: .keyboardActivity,
        startedAt: UXEventTime.isoString(burst.startedAt),
        endedAt: UXEventTime.isoString(burst.endedAt),
        recentTargetProcessID: burst.recentTargetProcessID,
        keyboardActivity: UXKeyboardActivitySummary(
          eventCount: burst.eventCount,
          keyDownCount: burst.keyDownCount,
          keyUpCount: burst.keyUpCount,
          autoRepeatCount: burst.autoRepeatCount,
          modifiedKeyEventCount: burst.modifiedKeyEventCount,
          durationMilliseconds: durationMilliseconds(from: burst.startedAt, to: burst.endedAt)
        )
      )
    )
    keyboardBurst = nil
  }

  private func finalizePointer(into anchors: inout [UXEventAnchor]) {
    guard let state = pointerState else { return }
    anchors.append(pointerAnchor(from: state, fallbackAction: state.dragged ? .drag : .down))
    pointerState = nil
  }

  private func pointerAnchor(action: UXPointerAction, event: UXEventPrimitive, button: UXPointerButton) -> UXEventAnchor {
    UXEventAnchor(
      kind: .pointer,
      startedAt: UXEventTime.isoString(event.time),
      endedAt: UXEventTime.isoString(event.time),
      recentTargetProcessID: sanitizedTargetProcessID(for: event),
      pointer: UXPointerSummary(
        action: action,
        button: button,
        eventCount: 1,
        durationMilliseconds: 0,
        distancePoints: 0,
        dominantAxis: .none,
        clickCount: max(0, event.clickState)
      )
    )
  }

  private func pointerAnchor(from state: PointerState, fallbackAction: UXPointerAction) -> UXEventAnchor {
    let dx = (state.lastX ?? state.startX ?? 0) - (state.startX ?? state.lastX ?? 0)
    let dy = (state.lastY ?? state.startY ?? 0) - (state.startY ?? state.lastY ?? 0)
    let distance = sqrt((dx * dx) + (dy * dy))
    let action: UXPointerAction = state.dragged ? .drag : fallbackAction
    return UXEventAnchor(
      kind: .pointer,
      startedAt: UXEventTime.isoString(state.startedAt),
      endedAt: UXEventTime.isoString(state.endedAt),
      recentTargetProcessID: state.recentTargetProcessID,
      pointer: UXPointerSummary(
        action: action,
        button: state.button,
        eventCount: state.eventCount,
        durationMilliseconds: durationMilliseconds(from: state.startedAt, to: state.endedAt),
        distancePoints: rounded(distance),
        dominantAxis: dominantAxis(dx: dx, dy: dy),
        clickCount: state.clickCount
      )
    )
  }

  private func pointerButton(for kind: UXEventPrimitiveKind) -> UXPointerButton {
    switch kind {
    case .leftMouseDown, .leftMouseUp, .leftMouseDragged:
      return .left
    case .rightMouseDown, .rightMouseUp, .rightMouseDragged:
      return .right
    case .otherMouseDown, .otherMouseUp, .otherMouseDragged:
      return .other
    default:
      return .unknown
    }
  }

  private func dominantAxis(dx: Double, dy: Double) -> UXPointerDominantAxis {
    let absX = abs(dx)
    let absY = abs(dy)
    if absX < 1, absY < 1 { return .none }
    if absX > absY * 1.5 { return .horizontal }
    if absY > absX * 1.5 { return .vertical }
    return .mixed
  }

  private func rounded(_ value: Double) -> Double {
    (value * 100).rounded() / 100
  }

  private func durationMilliseconds(from start: Date, to end: Date) -> Int {
    max(0, Int((end.timeIntervalSince(start) * 1_000).rounded()))
  }

  private struct ScrollBurstState {
    var startedAt: Date
    var endedAt: Date
    var eventCount: Int
    var totalDX: Double
    var totalDY: Double
    var maxAbsDY: Double
    var momentumEventCount: Int
    var recentTargetProcessID: Int32?
  }

  private struct PointerState {
    var button: UXPointerButton
    var startedAt: Date
    var endedAt: Date
    var startX: Double?
    var startY: Double?
    var lastX: Double?
    var lastY: Double?
    var eventCount: Int
    var clickCount: Int
    var dragged: Bool
    var recentTargetProcessID: Int32?

    var isClick: Bool {
      guard !dragged else { return false }
      let dx = (lastX ?? startX ?? 0) - (startX ?? lastX ?? 0)
      let dy = (lastY ?? startY ?? 0) - (startY ?? lastY ?? 0)
      return sqrt((dx * dx) + (dy * dy)) <= 5
    }
  }

  private struct KeyboardBurstState {
    var startedAt: Date
    var endedAt: Date
    var eventCount: Int
    var keyDownCount: Int
    var keyUpCount: Int
    var autoRepeatCount: Int
    var modifiedKeyEventCount: Int
    var recentTargetProcessID: Int32?
  }
}

enum UXEventTime {
  private static let formatterKey = "com.haptica.1context.capture.uxEventTimeFormatter"

  static func isoString(_ date: Date) -> String {
    let threadDictionary = Thread.current.threadDictionary
    let formatter: ISO8601DateFormatter
    if let cached = threadDictionary[formatterKey] as? ISO8601DateFormatter {
      formatter = cached
    } else {
      let cached = ISO8601DateFormatter()
      cached.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
      threadDictionary[formatterKey] = cached
      formatter = cached
    }
    return formatter.string(from: date)
  }
}

enum UXModifierNames {
  static func modifiedKeyMaskActive(_ raw: UInt64) -> Bool {
    raw & (CGEventFlags.maskCommand.rawValue | CGEventFlags.maskControl.rawValue | CGEventFlags.maskAlternate.rawValue) != 0
  }

  static func names(_ raw: UInt64) -> [String] {
    var names: [String] = []
    if raw & CGEventFlags.maskCommand.rawValue != 0 { names.append("command") }
    if raw & CGEventFlags.maskControl.rawValue != 0 { names.append("control") }
    if raw & CGEventFlags.maskAlternate.rawValue != 0 { names.append("option") }
    if raw & CGEventFlags.maskShift.rawValue != 0 { names.append("shift") }
    if raw & CGEventFlags.maskAlphaShift.rawValue != 0 { names.append("caps_lock") }
    if raw & CGEventFlags.maskSecondaryFn.rawValue != 0 { names.append("fn") }
    return names
  }
}
