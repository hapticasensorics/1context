@preconcurrency import ApplicationServices
import CoreGraphics
import Foundation

public enum OneContextUXEventTapError: LocalizedError, Equatable {
  case tapCreateFailed
  case runLoopSourceCreateFailed
  case persistentStartupTimedOut

  public var errorDescription: String? {
    switch self {
    case .tapCreateFailed:
      return "Could not create a listen-only CGEventTap for UX event anchors. Input Monitoring may not be granted for this process."
    case .runLoopSourceCreateFailed:
      return "Could not create a run-loop source for the UX event tap."
    case .persistentStartupTimedOut:
      return "Timed out while starting the persistent UX event tap thread."
    }
  }
}

public struct OneContextUXEventTapOwner: Codable, Equatable, Sendable {
  public var pid: Int
  public var executable: String
  public var bundle: String?

  public init(pid: Int, executable: String, bundle: String? = nil) {
    self.pid = pid
    self.executable = executable
    self.bundle = bundle
  }

  enum CodingKeys: String, CodingKey {
    case pid = "tap_owner_pid"
    case executable = "tap_owner_executable"
    case bundle = "tap_owner_bundle"
  }
}

public struct OneContextUXEventTapStatus: Codable, Equatable, Sendable {
  public var schemaVersion: Int
  public var startupWired: Bool
  public var lifecycleState: String
  public var tapActive: Bool
  public var tapOwnerPID: Int?
  public var tapOwnerExecutable: String?
  public var tapOwnerBundle: String?
  public var eventTap: String
  public var tapOptions: String
  public var eventMask: [String]
  public var lastEventAt: String?
  public var disabledCount: Int
  public var disabledByTimeoutCount: Int
  public var disabledByUserInputCount: Int
  public var reenableAttemptCount: Int
  public var reenableSuccessCount: Int
  public var reenableFailureCount: Int
  public var droppedCount: Int
  public var coalescedCount: Int
  public var queueDepth: Int
  public var observedEventCount: Int
  public var callbackCount: Int
  public var callbackLastMicroseconds: Double
  public var callbackAverageMicroseconds: Double
  public var callbackMaxMicroseconds: Double
  public var targetProcessIDAvailable: Bool
  public var targetProcessIDObservedCount: Int
  public var recentTargetProcessID: Int32?
  public var lastDisabledReason: String?
  public var lastReenableAt: String?
  public var lastError: String?
  public var note: String?

  public init(
    schemaVersion: Int = 1,
    startupWired: Bool = false,
    lifecycleState: String = "inactive",
    tapActive: Bool,
    tapOwnerPID: Int? = nil,
    tapOwnerExecutable: String? = nil,
    tapOwnerBundle: String? = nil,
    eventTap: String = "cgSessionEventTap",
    tapOptions: String = "listenOnly",
    eventMask: [String] = OneContextUXEventTap.eventTypeNames,
    lastEventAt: String?,
    disabledCount: Int,
    disabledByTimeoutCount: Int = 0,
    disabledByUserInputCount: Int = 0,
    reenableAttemptCount: Int = 0,
    reenableSuccessCount: Int = 0,
    reenableFailureCount: Int = 0,
    droppedCount: Int,
    coalescedCount: Int,
    queueDepth: Int,
    observedEventCount: Int,
    callbackCount: Int,
    callbackLastMicroseconds: Double = 0,
    callbackAverageMicroseconds: Double,
    callbackMaxMicroseconds: Double,
    targetProcessIDAvailable: Bool = false,
    targetProcessIDObservedCount: Int = 0,
    recentTargetProcessID: Int32? = nil,
    lastDisabledReason: String? = nil,
    lastReenableAt: String? = nil,
    lastError: String? = nil,
    note: String? = nil
  ) {
    self.schemaVersion = schemaVersion
    self.startupWired = startupWired
    self.lifecycleState = lifecycleState
    self.tapActive = tapActive
    self.tapOwnerPID = tapOwnerPID
    self.tapOwnerExecutable = tapOwnerExecutable
    self.tapOwnerBundle = tapOwnerBundle
    self.eventTap = eventTap
    self.tapOptions = tapOptions
    self.eventMask = eventMask
    self.lastEventAt = lastEventAt
    self.disabledCount = disabledCount
    self.disabledByTimeoutCount = disabledByTimeoutCount
    self.disabledByUserInputCount = disabledByUserInputCount
    self.reenableAttemptCount = reenableAttemptCount
    self.reenableSuccessCount = reenableSuccessCount
    self.reenableFailureCount = reenableFailureCount
    self.droppedCount = droppedCount
    self.coalescedCount = coalescedCount
    self.queueDepth = queueDepth
    self.observedEventCount = observedEventCount
    self.callbackCount = callbackCount
    self.callbackLastMicroseconds = callbackLastMicroseconds
    self.callbackAverageMicroseconds = callbackAverageMicroseconds
    self.callbackMaxMicroseconds = callbackMaxMicroseconds
    self.targetProcessIDAvailable = targetProcessIDAvailable
    self.targetProcessIDObservedCount = targetProcessIDObservedCount
    self.recentTargetProcessID = recentTargetProcessID
    self.lastDisabledReason = lastDisabledReason
    self.lastReenableAt = lastReenableAt
    self.lastError = lastError
    self.note = note
  }

  public static func inactive(
    startupWired: Bool = false,
    owner: OneContextUXEventTapOwner? = nil,
    lifecycleState: String = "inactive",
    lastError: String? = nil,
    note: String? = nil
  ) -> OneContextUXEventTapStatus {
    OneContextUXEventTapStatus(
      startupWired: startupWired,
      lifecycleState: lifecycleState,
      tapActive: false,
      tapOwnerPID: owner?.pid,
      tapOwnerExecutable: owner?.executable,
      tapOwnerBundle: owner?.bundle,
      lastEventAt: nil,
      disabledCount: 0,
      droppedCount: 0,
      coalescedCount: 0,
      queueDepth: 0,
      observedEventCount: 0,
      callbackCount: 0,
      callbackAverageMicroseconds: 0,
      callbackMaxMicroseconds: 0,
      lastError: lastError,
      note: note
    )
  }

  enum CodingKeys: String, CodingKey {
    case schemaVersion = "schema_version"
    case startupWired = "startup_wired"
    case lifecycleState = "lifecycle_state"
    case tapActive = "tap_active"
    case tapOwnerPID = "tap_owner_pid"
    case tapOwnerExecutable = "tap_owner_executable"
    case tapOwnerBundle = "tap_owner_bundle"
    case eventTap = "event_tap"
    case tapOptions = "tap_options"
    case eventMask = "event_mask"
    case lastEventAt = "last_event_at"
    case disabledCount = "disabled_count"
    case disabledByTimeoutCount = "disabled_by_timeout_count"
    case disabledByUserInputCount = "disabled_by_user_input_count"
    case reenableAttemptCount = "reenable_attempt_count"
    case reenableSuccessCount = "reenable_success_count"
    case reenableFailureCount = "reenable_failure_count"
    case droppedCount = "dropped_count"
    case coalescedCount = "coalesced_count"
    case queueDepth = "queue_depth"
    case observedEventCount = "observed_event_count"
    case callbackCount = "callback_count"
    case callbackLastMicroseconds = "callback_last_us"
    case callbackAverageMicroseconds = "callback_average_us"
    case callbackMaxMicroseconds = "callback_max_us"
    case targetProcessIDAvailable = "target_pid_available"
    case targetProcessIDObservedCount = "target_pid_observed_count"
    case recentTargetProcessID = "recent_target_process_id"
    case lastDisabledReason = "last_disabled_reason"
    case lastReenableAt = "last_reenable_at"
    case lastError = "last_error"
    case note
  }
}

public struct OneContextUXEventTapPollResult: Codable, Equatable, Sendable {
  public var anchors: [UXEventAnchor]
  public var motionHints: UXMotionHints
  public var status: OneContextUXEventTapStatus

  public init(anchors: [UXEventAnchor], motionHints: UXMotionHints, status: OneContextUXEventTapStatus) {
    self.anchors = anchors
    self.motionHints = motionHints
    self.status = status
  }

  enum CodingKeys: String, CodingKey {
    case anchors
    case motionHints = "motion_hints"
    case status
  }
}

public struct OneContextUXEventTapProbeReport: Codable, Equatable, Sendable {
  public var schemaVersion: Int
  public var generatedAt: String
  public var tapCreated: Bool
  public var observedEventCount: Int
  public var anchors: [UXEventAnchor]
  public var motionHints: UXMotionHints
  public var tapStatus: OneContextUXEventTapStatus
  public var errorMessage: String?

  public init(
    schemaVersion: Int = 1,
    generatedAt: String,
    tapCreated: Bool,
    observedEventCount: Int,
    anchors: [UXEventAnchor],
    motionHints: UXMotionHints,
    tapStatus: OneContextUXEventTapStatus,
    errorMessage: String? = nil
  ) {
    self.schemaVersion = schemaVersion
    self.generatedAt = generatedAt
    self.tapCreated = tapCreated
    self.observedEventCount = observedEventCount
    self.anchors = anchors
    self.motionHints = motionHints
    self.tapStatus = tapStatus
    self.errorMessage = errorMessage
  }

  enum CodingKeys: String, CodingKey {
    case schemaVersion = "schema_version"
    case generatedAt = "generated_at"
    case tapCreated = "tap_created"
    case observedEventCount = "observed_event_count"
    case anchors
    case motionHints = "motion_hints"
    case tapStatus = "tap_status"
    case errorMessage = "error_message"
  }
}

private final class UXEventTapStartupBox: @unchecked Sendable {
  private let lock = NSLock()
  private var result: Result<Void, Error>?

  func set(_ result: Result<Void, Error>) {
    lock.withLock {
      guard self.result == nil else { return }
      self.result = result
    }
  }

  func get() -> Result<Void, Error>? {
    lock.withLock { result }
  }
}

public final class OneContextUXEventTap: @unchecked Sendable {
  public static let eventTypes: [CGEventType] = [
    .scrollWheel,
    .leftMouseDown,
    .leftMouseUp,
    .rightMouseDown,
    .rightMouseUp,
    .otherMouseDown,
    .otherMouseUp,
    .leftMouseDragged,
    .rightMouseDragged,
    .otherMouseDragged,
    .flagsChanged,
    .keyDown,
    .keyUp
  ]

  public static var eventTypeNames: [String] {
    eventTypes.map(eventName)
  }

  public static var eventMask: CGEventMask {
    eventTypes.reduce(CGEventMask(0)) { mask, type in
      mask | CGEventMask(1 << type.rawValue)
    }
  }

  private let queue: UXEventPrimitiveRingBuffer
  private let aggregator: UXEventAggregator
  private let callbackState: UXEventTapCallbackState
  private let lock = NSLock()
  private var tap: CFMachPort?
  private var source: CFRunLoopSource?
  private var runLoop: CFRunLoop?
  private var serviceTimer: CFRunLoopTimer?
  private var thread: Thread?
  private var pendingAnchors: [UXEventAnchor] = []
  private let pendingAnchorCapacity = 1_024

  public init(
    queueCapacity: Int = 512,
    aggregator: UXEventAggregator = UXEventAggregator(),
    owner: OneContextUXEventTapOwner? = nil,
    startupWired: Bool = false
  ) {
    let queue = UXEventPrimitiveRingBuffer(capacity: queueCapacity)
    self.queue = queue
    self.aggregator = aggregator
    self.callbackState = UXEventTapCallbackState(queue: queue, owner: owner, startupWired: startupWired)
  }

  deinit {
    stop()
  }

  public func start(on runLoop: CFRunLoop = CFRunLoopGetCurrent()) throws {
    try installTap(on: runLoop, lifecycleState: "probe_running")
  }

  public func startPersistent(
    threadName: String = "com.haptica.1contextd.ux-event-tap",
    startupTimeout: TimeInterval = 2
  ) throws {
    let semaphore = DispatchSemaphore(value: 0)
    let startup = UXEventTapStartupBox()
    var startedThread = false

    try lock.withLock {
      guard tap == nil, thread == nil else { return }
      callbackState.prepareForPersistentStart()
      callbackState.setLifecycle("starting")
      let worker = Thread { [weak self] in
        self?.runPersistentTap(startup: startup, semaphore: semaphore)
      }
      worker.name = threadName
      thread = worker
      worker.start()
      startedThread = true
    }

    guard startedThread else { return }

    guard semaphore.wait(timeout: .now() + startupTimeout) == .success else {
      callbackState.setError(OneContextUXEventTapError.persistentStartupTimedOut.localizedDescription)
      throw OneContextUXEventTapError.persistentStartupTimedOut
    }

    switch startup.get() {
    case .success:
      return
    case .failure(let error):
      throw error
    case .none:
      throw OneContextUXEventTapError.persistentStartupTimedOut
    }
  }

  public func stop() {
    let state = lock.withLock {
      (runLoop: runLoop, hasThread: thread != nil)
    }

    guard state.hasThread, let runLoop = state.runLoop else {
      uninstallTap()
      return
    }

    let semaphore = DispatchSemaphore(value: 0)
    CFRunLoopPerformBlock(runLoop, CFRunLoopMode.commonModes.rawValue) { [weak self] in
      self?.uninstallTap()
      CFRunLoopStop(runLoop)
      semaphore.signal()
    }
    CFRunLoopWakeUp(runLoop)
    _ = semaphore.wait(timeout: .now() + 2)
  }

  private func runPersistentTap(startup: UXEventTapStartupBox, semaphore: DispatchSemaphore) {
    autoreleasepool {
      guard let runLoop = CFRunLoopGetCurrent() else {
        let error = OneContextUXEventTapError.runLoopSourceCreateFailed
        callbackState.setError(error.localizedDescription)
        callbackState.setLifecycle("degraded")
        startup.set(.failure(error))
        semaphore.signal()
        return
      }
      do {
        try installTap(on: runLoop, lifecycleState: "running")
        installServiceTimer(on: runLoop)
        startup.set(.success(()))
        semaphore.signal()

        while callbackState.shouldKeepRunning {
          _ = CFRunLoopRunInMode(.defaultMode, 0.5, true)
        }

        uninstallTap(lifecycleState: "stopped")
      } catch {
        callbackState.setError(error.localizedDescription)
        callbackState.setLifecycle("degraded")
        startup.set(.failure(error))
        semaphore.signal()
        uninstallTap(lifecycleState: "degraded")
      }
    }
  }

  private func installTap(on runLoop: CFRunLoop, lifecycleState: String) throws {
    try lock.withLock {
      guard tap == nil else { return }

      guard let tap = CGEvent.tapCreate(
        tap: .cgSessionEventTap,
        place: .headInsertEventTap,
        options: .listenOnly,
        eventsOfInterest: Self.eventMask,
        callback: uxEventTapCallback,
        userInfo: Unmanaged.passUnretained(callbackState).toOpaque()
      ) else {
        callbackState.setLifecycle("degraded")
        callbackState.setError(OneContextUXEventTapError.tapCreateFailed.localizedDescription)
        throw OneContextUXEventTapError.tapCreateFailed
      }

      guard let source = CFMachPortCreateRunLoopSource(kCFAllocatorDefault, tap, 0) else {
        CFMachPortInvalidate(tap)
        callbackState.setLifecycle("degraded")
        callbackState.setError(OneContextUXEventTapError.runLoopSourceCreateFailed.localizedDescription)
        throw OneContextUXEventTapError.runLoopSourceCreateFailed
      }

      CFRunLoopAddSource(runLoop, source, .commonModes)
      CGEvent.tapEnable(tap: tap, enable: true)
      self.tap = tap
      self.source = source
      self.runLoop = runLoop
      callbackState.setActive(true)
      callbackState.setLifecycle(lifecycleState)
      callbackState.clearError()
    }
  }

  private func installServiceTimer(on runLoop: CFRunLoop) {
    let context = UnsafeMutableRawPointer(Unmanaged.passUnretained(self).toOpaque())
    var timerContext = CFRunLoopTimerContext(
      version: 0,
      info: context,
      retain: nil,
      release: nil,
      copyDescription: nil
    )
    guard let timer = CFRunLoopTimerCreate(
      kCFAllocatorDefault,
      CFAbsoluteTimeGetCurrent() + 0.25,
      0.25,
      0,
      0,
      uxEventTapServiceTimerCallback,
      &timerContext
    ) else {
      return
    }
    lock.withLock {
      serviceTimer = timer
    }
    CFRunLoopAddTimer(runLoop, timer, .commonModes)
  }

  fileprivate func servicePersistentLane(now: Date = Date()) {
    appendPendingAnchors(aggregator.ingest(queue.drain(), now: now))
    reenableIfNeeded()
  }

  private func uninstallTap(lifecycleState: String = "stopped") {
    lock.withLock {
      callbackState.stopRunning()
      if let serviceTimer {
        CFRunLoopTimerInvalidate(serviceTimer)
      }
      if let tap {
        CGEvent.tapEnable(tap: tap, enable: false)
      }
      if let source, let runLoop {
        CFRunLoopRemoveSource(runLoop, source, .commonModes)
      }
      tap = nil
      source = nil
      runLoop = nil
      serviceTimer = nil
      thread = nil
      callbackState.setActive(false)
      callbackState.setLifecycle(lifecycleState)
    }
  }

  public func poll(now: Date = Date()) -> OneContextUXEventTapPollResult {
    reenableIfNeeded()
    var anchors = drainPendingAnchors()
    anchors.append(contentsOf: aggregator.ingest(queue.drain(), now: now))
    let aggregatorState = aggregator.currentState(now: now)
    return OneContextUXEventTapPollResult(
      anchors: anchors,
      motionHints: aggregatorState.motionHints,
      status: status(note: nil, aggregatorState: aggregatorState)
    )
  }

  public func flush(now: Date = Date()) -> OneContextUXEventTapPollResult {
    reenableIfNeeded()
    var anchors = drainPendingAnchors()
    anchors.append(contentsOf: aggregator.ingest(queue.drain(), now: now))
    anchors.append(contentsOf: aggregator.flush(now: now))
    let aggregatorState = aggregator.currentState(now: now)
    return OneContextUXEventTapPollResult(
      anchors: anchors,
      motionHints: aggregatorState.motionHints,
      status: status(note: nil, aggregatorState: aggregatorState)
    )
  }

  public func motionHints(now: Date = Date()) -> UXMotionHints {
    servicePersistentLane(now: now)
    return aggregator.motionHints(now: now)
  }

  public func status(note: String? = nil, now: Date = Date()) -> OneContextUXEventTapStatus {
    servicePersistentLane(now: now)
    return status(note: note, aggregatorState: aggregator.currentState(now: now))
  }

  private func status(
    note: String?,
    aggregatorState: UXEventAggregatorCurrentState
  ) -> OneContextUXEventTapStatus {
    return callbackState.status(
      queueSnapshot: queue.snapshot(),
      aggregationSnapshot: aggregatorState.aggregationSnapshot,
      recentTargetProcessID: aggregatorState.motionHints.recentTargetProcessID,
      note: note
    )
  }

  private func appendPendingAnchors(_ anchors: [UXEventAnchor]) {
    guard !anchors.isEmpty else { return }
    lock.withLock {
      if anchors.count >= pendingAnchorCapacity {
        pendingAnchors.removeAll(keepingCapacity: true)
        pendingAnchors.append(contentsOf: anchors.suffix(pendingAnchorCapacity))
        return
      }
      let overflow = pendingAnchors.count + anchors.count - pendingAnchorCapacity
      if overflow > 0 {
        if overflow >= pendingAnchors.count {
          pendingAnchors.removeAll(keepingCapacity: true)
        } else {
          pendingAnchors.removeFirst(overflow)
        }
      }
      pendingAnchors.append(contentsOf: anchors)
    }
  }

  private func drainPendingAnchors() -> [UXEventAnchor] {
    lock.withLock {
      let anchors = pendingAnchors
      pendingAnchors.removeAll(keepingCapacity: true)
      return anchors
    }
  }

  public static func probe(timeoutSeconds: TimeInterval = 1) -> OneContextUXEventTapProbeReport {
    let timeout = min(max(timeoutSeconds, 0.05), 10)
    let startedAt = Date()
    let tap = OneContextUXEventTap(queueCapacity: 128)

    do {
      try tap.start()
      let deadline = Date().addingTimeInterval(timeout)
      while Date() < deadline {
        _ = RunLoop.current.run(mode: .default, before: min(deadline, Date().addingTimeInterval(0.05)))
        if tap.status().observedEventCount > 0 {
          break
        }
      }
      let poll = tap.poll(now: Date())
      let status = tap.status(note: "Short-lived probe tap; persistent daemon lane owns the default UX event stream.")
      tap.stop()
      return OneContextUXEventTapProbeReport(
        generatedAt: UXEventTime.isoString(startedAt),
        tapCreated: true,
        observedEventCount: status.observedEventCount,
        anchors: poll.anchors,
        motionHints: poll.motionHints,
        tapStatus: status
      )
    } catch {
      let now = Date()
      return OneContextUXEventTapProbeReport(
        generatedAt: UXEventTime.isoString(now),
        tapCreated: false,
        observedEventCount: 0,
        anchors: [],
        motionHints: UXMotionHints(
          generatedAt: UXEventTime.isoString(now),
          scrollEventRecently: false,
          keyboardActivityRecently: false,
          estimatedScrollDY: 0,
          focusedRecently: false
        ),
        tapStatus: .inactive(note: "Short-lived probe tap creation failed."),
        errorMessage: error.localizedDescription
      )
    }
  }

  private func reenableIfNeeded() {
    guard callbackState.beginReenableAttemptIfNeeded() else { return }
    lock.withLock {
      guard let tap else {
        callbackState.finishReenableAttempt(success: false, error: "tap_missing")
        return
      }
      CGEvent.tapEnable(tap: tap, enable: true)
      let enabled = CGEvent.tapIsEnabled(tap: tap)
      callbackState.finishReenableAttempt(success: enabled, error: enabled ? nil : "tap_enable_failed")
    }
  }

  static func primitive(from type: CGEventType, event: CGEvent, at time: Date) -> UXEventPrimitive? {
    let flagsRaw = event.flags.rawValue
    let rawTargetPID = event.getIntegerValueField(.eventTargetUnixProcessID)
    let targetPID = rawTargetPID > 0 ? Int32(rawTargetPID) : nil

    switch type {
    case .scrollWheel:
      let pointDY = event.getDoubleValueField(.scrollWheelEventPointDeltaAxis1)
      let pointDX = event.getDoubleValueField(.scrollWheelEventPointDeltaAxis2)
      let lineDY = Double(event.getIntegerValueField(.scrollWheelEventDeltaAxis1))
      let lineDX = Double(event.getIntegerValueField(.scrollWheelEventDeltaAxis2))
      return UXEventPrimitive(
        time: time,
        kind: .scrollWheel,
        scrollDeltaX: pointDX != 0 ? pointDX : lineDX,
        scrollDeltaY: pointDY != 0 ? pointDY : lineDY,
        modifierFlagsRaw: flagsRaw,
        isMomentumScroll: event.getIntegerValueField(.scrollWheelEventMomentumPhase) != 0,
        targetProcessID: targetPID
      )
    case .leftMouseDown, .leftMouseUp, .rightMouseDown, .rightMouseUp,
      .otherMouseDown, .otherMouseUp, .leftMouseDragged, .rightMouseDragged,
      .otherMouseDragged:
      let location = event.location
      return UXEventPrimitive(
        time: time,
        kind: primitiveKind(for: type),
        locationX: location.x,
        locationY: location.y,
        modifierFlagsRaw: flagsRaw,
        clickState: Int(event.getIntegerValueField(.mouseEventClickState)),
        targetProcessID: targetPID
      )
    case .flagsChanged:
      return UXEventPrimitive(
        time: time,
        kind: .flagsChanged,
        modifierFlagsRaw: flagsRaw,
        targetProcessID: targetPID
      )
    case .keyDown, .keyUp:
      return UXEventPrimitive(
        time: time,
        kind: type == .keyDown ? .keyDown : .keyUp,
        modifierFlagsRaw: flagsRaw,
        isAutoRepeat: event.getIntegerValueField(.keyboardEventAutorepeat) != 0,
        targetProcessID: targetPID
      )
    default:
      return nil
    }
  }

  private static func primitiveKind(for type: CGEventType) -> UXEventPrimitiveKind {
    switch type {
    case .leftMouseDown:
      return .leftMouseDown
    case .leftMouseUp:
      return .leftMouseUp
    case .rightMouseDown:
      return .rightMouseDown
    case .rightMouseUp:
      return .rightMouseUp
    case .otherMouseDown:
      return .otherMouseDown
    case .otherMouseUp:
      return .otherMouseUp
    case .leftMouseDragged:
      return .leftMouseDragged
    case .rightMouseDragged:
      return .rightMouseDragged
    case .otherMouseDragged:
      return .otherMouseDragged
    default:
      return .otherMouseDown
    }
  }

  private static func eventName(_ type: CGEventType) -> String {
    switch type {
    case .scrollWheel:
      return "scroll_wheel"
    case .leftMouseDown:
      return "left_mouse_down"
    case .leftMouseUp:
      return "left_mouse_up"
    case .rightMouseDown:
      return "right_mouse_down"
    case .rightMouseUp:
      return "right_mouse_up"
    case .otherMouseDown:
      return "other_mouse_down"
    case .otherMouseUp:
      return "other_mouse_up"
    case .leftMouseDragged:
      return "left_mouse_dragged"
    case .rightMouseDragged:
      return "right_mouse_dragged"
    case .otherMouseDragged:
      return "other_mouse_dragged"
    case .flagsChanged:
      return "flags_changed"
    case .keyDown:
      return "key_down"
    case .keyUp:
      return "key_up"
    default:
      return "unknown_\(type.rawValue)"
    }
  }
}

private final class UXEventTapCallbackState: @unchecked Sendable {
  private let queue: UXEventPrimitiveRingBuffer
  private let owner: OneContextUXEventTapOwner?
  private let startupWired: Bool
  private let lock = NSLock()
  private var active = false
  private var keepRunning = true
  private var lifecycleState = "inactive"
  private var disabled = 0
  private var disabledByTimeout = 0
  private var disabledByUserInput = 0
  private var needsReenable = false
  private var reenableAttempts = 0
  private var reenableSuccesses = 0
  private var reenableFailures = 0
  private var lastEvent: Date?
  private var lastDisabledReason: String?
  private var lastReenableAt: Date?
  private var lastError: String?
  private var observed = 0
  private var targetProcessIDObserved = 0
  private var callbackCount = 0
  private var callbackLastNanoseconds: UInt64 = 0
  private var callbackTotalNanoseconds: UInt64 = 0
  private var callbackMaxNanoseconds: UInt64 = 0

  init(queue: UXEventPrimitiveRingBuffer, owner: OneContextUXEventTapOwner?, startupWired: Bool) {
    self.queue = queue
    self.owner = owner
    self.startupWired = startupWired
  }

  var shouldKeepRunning: Bool {
    lock.withLock { keepRunning }
  }

  func prepareForPersistentStart() {
    lock.withLock {
      keepRunning = true
      lifecycleState = "starting"
      lastError = nil
    }
  }

  func stopRunning() {
    lock.withLock {
      keepRunning = false
    }
  }

  func setActive(_ isActive: Bool) {
    lock.withLock {
      active = isActive
      if isActive {
        needsReenable = false
      }
    }
  }

  func setLifecycle(_ state: String) {
    lock.withLock {
      lifecycleState = state
    }
  }

  func setError(_ message: String?) {
    lock.withLock {
      lastError = message
    }
  }

  func clearError() {
    setError(nil)
  }

  func enqueue(_ event: UXEventPrimitive, callbackDurationNanoseconds: UInt64) {
    queue.push(event)
    lock.withLock {
      lastEvent = event.time
      observed += 1
      if event.targetProcessID != nil {
        targetProcessIDObserved += 1
      }
      recordCallbackDurationLocked(callbackDurationNanoseconds)
    }
  }

  func markDisabled(reason: String, callbackDurationNanoseconds: UInt64) {
    lock.withLock {
      active = false
      lifecycleState = "degraded"
      disabled += 1
      lastDisabledReason = reason
      if reason == "timeout" {
        disabledByTimeout += 1
      } else if reason == "user_input" {
        disabledByUserInput += 1
      }
      needsReenable = true
      recordCallbackDurationLocked(callbackDurationNanoseconds)
    }
  }

  func recordCallbackDuration(_ nanoseconds: UInt64) {
    lock.withLock {
      recordCallbackDurationLocked(nanoseconds)
    }
  }

  func beginReenableAttemptIfNeeded() -> Bool {
    lock.withLock {
      guard needsReenable else { return false }
      needsReenable = false
      reenableAttempts += 1
      return true
    }
  }

  func finishReenableAttempt(success: Bool, error: String?) {
    lock.withLock {
      lastReenableAt = Date()
      active = success
      if success {
        lifecycleState = "running"
        reenableSuccesses += 1
        lastError = nil
      } else {
        lifecycleState = "degraded"
        needsReenable = true
        reenableFailures += 1
        lastError = error
      }
    }
  }

  func status(
    queueSnapshot: UXEventQueueSnapshot,
    aggregationSnapshot: UXEventAggregationSnapshot,
    recentTargetProcessID: Int32?,
    note: String?
  ) -> OneContextUXEventTapStatus {
    lock.withLock {
      let average = callbackCount > 0
        ? (Double(callbackTotalNanoseconds) / Double(callbackCount)) / 1_000
        : 0
      return OneContextUXEventTapStatus(
        startupWired: startupWired,
        lifecycleState: lifecycleState,
        tapActive: active,
        tapOwnerPID: owner?.pid,
        tapOwnerExecutable: owner?.executable,
        tapOwnerBundle: owner?.bundle,
        lastEventAt: lastEvent.map(UXEventTime.isoString),
        disabledCount: disabled,
        disabledByTimeoutCount: disabledByTimeout,
        disabledByUserInputCount: disabledByUserInput,
        reenableAttemptCount: reenableAttempts,
        reenableSuccessCount: reenableSuccesses,
        reenableFailureCount: reenableFailures,
        droppedCount: queueSnapshot.droppedCount,
        coalescedCount: aggregationSnapshot.coalescedCount,
        queueDepth: queueSnapshot.queueDepth,
        observedEventCount: observed,
        callbackCount: callbackCount,
        callbackLastMicroseconds: rounded(Double(callbackLastNanoseconds) / 1_000),
        callbackAverageMicroseconds: rounded(average),
        callbackMaxMicroseconds: rounded(Double(callbackMaxNanoseconds) / 1_000),
        targetProcessIDAvailable: targetProcessIDObserved > 0,
        targetProcessIDObservedCount: targetProcessIDObserved,
        recentTargetProcessID: recentTargetProcessID,
        lastDisabledReason: lastDisabledReason,
        lastReenableAt: lastReenableAt.map(UXEventTime.isoString),
        lastError: lastError,
        note: note
      )
    }
  }

  private func recordCallbackDurationLocked(_ nanoseconds: UInt64) {
    callbackCount += 1
    callbackLastNanoseconds = nanoseconds
    callbackTotalNanoseconds += nanoseconds
    callbackMaxNanoseconds = max(callbackMaxNanoseconds, nanoseconds)
  }

  private func rounded(_ value: Double) -> Double {
    (value * 100).rounded() / 100
  }
}

private let uxEventTapServiceTimerCallback: CFRunLoopTimerCallBack = { _, info in
  guard let info else { return }
  let tap = Unmanaged<OneContextUXEventTap>.fromOpaque(info).takeUnretainedValue()
  tap.servicePersistentLane()
}

private let uxEventTapCallback: CGEventTapCallBack = { _, type, event, refcon in
  let started = DispatchTime.now().uptimeNanoseconds
  guard let refcon else {
    return Unmanaged.passUnretained(event)
  }

  let state = Unmanaged<UXEventTapCallbackState>.fromOpaque(refcon).takeUnretainedValue()
  if type == .tapDisabledByTimeout || type == .tapDisabledByUserInput {
    let reason = type == .tapDisabledByTimeout ? "timeout" : "user_input"
    state.markDisabled(reason: reason, callbackDurationNanoseconds: DispatchTime.now().uptimeNanoseconds - started)
    return Unmanaged.passUnretained(event)
  }

  if let primitive = OneContextUXEventTap.primitive(from: type, event: event, at: Date()) {
    state.enqueue(primitive, callbackDurationNanoseconds: DispatchTime.now().uptimeNanoseconds - started)
  } else {
    state.recordCallbackDuration(DispatchTime.now().uptimeNanoseconds - started)
  }
  return Unmanaged.passUnretained(event)
}
