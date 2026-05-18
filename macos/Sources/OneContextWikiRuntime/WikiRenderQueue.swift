import Foundation

public enum WikiRenderQueuePriority: String, Sendable {
  case automatic
  case manual
}

public enum WikiRenderQueueOutcomeStatus: String, Sendable {
  case published
  case failed
  case skipped
}

public struct WikiRenderQueueRequest: Equatable, Sendable {
  public var trigger: String
  public var priority: WikiRenderQueuePriority
  public var requestedAt: Date

  public init(trigger: String, priority: WikiRenderQueuePriority, requestedAt: Date = Date()) {
    self.trigger = trigger
    self.priority = priority
    self.requestedAt = requestedAt
  }
}

public struct WikiRenderQueueOutcome: Equatable, Sendable {
  public var status: WikiRenderQueueOutcomeStatus
  public var dirtyPages: Int
  public var rendererDurationMilliseconds: Int
  public var skipReason: String?
  public var error: String?

  public init(
    status: WikiRenderQueueOutcomeStatus,
    dirtyPages: Int,
    rendererDurationMilliseconds: Int,
    skipReason: String? = nil,
    error: String? = nil
  ) {
    self.status = status
    self.dirtyPages = dirtyPages
    self.rendererDurationMilliseconds = rendererDurationMilliseconds
    self.skipReason = skipReason
    self.error = error
  }
}

public struct WikiRenderQueueRecord: Equatable, Sendable {
  public var trigger: String
  public var priority: WikiRenderQueuePriority
  public var status: WikiRenderQueueOutcomeStatus
  public var requestedAt: Date
  public var startedAt: Date
  public var finishedAt: Date
  public var queueDelayMilliseconds: Int
  public var renderDurationMilliseconds: Int
  public var rendererDurationMilliseconds: Int
  public var dirtyPages: Int
  public var skipReason: String?
  public var error: String?
}

public struct WikiRenderQueueSnapshot: Equatable, Sendable {
  public var running: Bool
  public var scheduled: Bool
  public var pending: Bool
  public var activeTrigger: String?
  public var acceptedCount: Int
  public var coalescedCount: Int
  public var completedCount: Int
  public var failedCount: Int
  public var skippedCount: Int
  public var maxConcurrentRenders: Int
  public var backingOff: Bool
  public var backoffRemainingMilliseconds: Int
  public var history: [WikiRenderQueueRecord]
}

public final class WikiRenderQueue: @unchecked Sendable {
  public typealias Render = @Sendable (WikiRenderQueueRequest) -> WikiRenderQueueOutcome

  private let stateQueue = DispatchQueue(label: "com.haptica.1context.wiki-render-queue")
  private let workerQueue: DispatchQueue
  private let debounceInterval: TimeInterval
  private let failureBackoffInterval: TimeInterval
  private let now: @Sendable () -> Date
  private let render: Render
  private let historyLimit: Int

  private var scheduledRequest: WikiRenderQueueRequest?
  private var scheduledToken = 0
  private var pendingRequest: WikiRenderQueueRequest?
  private var runningRequest: WikiRenderQueueRequest?
  private var backoffUntil: Date?
  private var acceptedCount = 0
  private var coalescedCount = 0
  private var completedCount = 0
  private var failedCount = 0
  private var skippedCount = 0
  private var activeRenders = 0
  private var maxConcurrentRenders = 0
  private var history: [WikiRenderQueueRecord] = []

  public init(
    debounceInterval: TimeInterval = 0.5,
    failureBackoffInterval: TimeInterval = 5,
    workerQueue: DispatchQueue = DispatchQueue.global(qos: .utility),
    now: @escaping @Sendable () -> Date = Date.init,
    historyLimit: Int = 50,
    render: @escaping Render
  ) {
    self.debounceInterval = debounceInterval
    self.failureBackoffInterval = failureBackoffInterval
    self.workerQueue = workerQueue
    self.now = now
    self.historyLimit = historyLimit
    self.render = render
  }

  public func request(trigger: String, priority: WikiRenderQueuePriority) {
    let request = WikiRenderQueueRequest(trigger: trigger, priority: priority, requestedAt: now())
    stateQueue.sync {
      acceptedCount += 1
      if runningRequest != nil {
        pendingRequest = merge(existing: pendingRequest, incoming: request)
        coalescedCount += 1
        signalStateChanged()
        return
      }
      if scheduledRequest != nil {
        scheduledRequest = merge(existing: scheduledRequest, incoming: request)
        coalescedCount += 1
        scheduleCurrentRequest()
        return
      }
      scheduledRequest = request
      scheduleCurrentRequest()
    }
  }

  public func snapshot() -> WikiRenderQueueSnapshot {
    stateQueue.sync {
      let remaining = max(0, backoffUntil?.timeIntervalSince(now()) ?? 0)
      return WikiRenderQueueSnapshot(
        running: runningRequest != nil,
        scheduled: scheduledRequest != nil,
        pending: pendingRequest != nil,
        activeTrigger: runningRequest?.trigger ?? scheduledRequest?.trigger ?? pendingRequest?.trigger,
        acceptedCount: acceptedCount,
        coalescedCount: coalescedCount,
        completedCount: completedCount,
        failedCount: failedCount,
        skippedCount: skippedCount,
        maxConcurrentRenders: maxConcurrentRenders,
        backingOff: remaining > 0,
        backoffRemainingMilliseconds: Int((remaining * 1_000).rounded()),
        history: history
      )
    }
  }

  @discardableResult
  public func waitUntilIdle(timeout: TimeInterval) -> Bool {
    let deadline = Date().addingTimeInterval(timeout)
    while Date() < deadline {
      if isIdle() { return true }
      Thread.sleep(forTimeInterval: 0.01)
    }
    return isIdle()
  }

  private func merge(
    existing: WikiRenderQueueRequest?,
    incoming: WikiRenderQueueRequest
  ) -> WikiRenderQueueRequest {
    guard let existing else { return incoming }
    if incoming.priority == .manual { return incoming }
    if existing.priority == .manual { return existing }
    return incoming
  }

  private func scheduleCurrentRequest() {
    guard let request = scheduledRequest else { return }
    scheduledToken += 1
    let token = scheduledToken
    let delay = delay(for: request)
    signalStateChanged()
    stateQueue.asyncAfter(deadline: .now() + delay) { [weak self] in
      self?.startScheduledRequest(token: token)
    }
  }

  private func delay(for request: WikiRenderQueueRequest) -> TimeInterval {
    guard request.priority != .manual else { return 0 }
    let backoffRemaining = max(0, backoffUntil?.timeIntervalSince(now()) ?? 0)
    return max(debounceInterval, backoffRemaining)
  }

  private func startScheduledRequest(token: Int) {
    guard token == scheduledToken, let request = scheduledRequest, runningRequest == nil else {
      return
    }
    scheduledRequest = nil
    runningRequest = request
    activeRenders += 1
    maxConcurrentRenders = max(maxConcurrentRenders, activeRenders)
    let startedAt = now()
    signalStateChanged()

    workerQueue.async { [weak self] in
      guard let self else { return }
      let outcome = self.render(request)
      let finishedAt = self.now()
      self.stateQueue.async {
        self.finish(request: request, outcome: outcome, startedAt: startedAt, finishedAt: finishedAt)
      }
    }
  }

  private func finish(
    request: WikiRenderQueueRequest,
    outcome: WikiRenderQueueOutcome,
    startedAt: Date,
    finishedAt: Date
  ) {
    activeRenders = max(0, activeRenders - 1)
    runningRequest = nil
    completedCount += 1
    if outcome.status == .failed {
      failedCount += 1
      backoffUntil = finishedAt.addingTimeInterval(failureBackoffInterval)
    }
    if outcome.status == .skipped {
      skippedCount += 1
    }

    history.append(WikiRenderQueueRecord(
      trigger: request.trigger,
      priority: request.priority,
      status: outcome.status,
      requestedAt: request.requestedAt,
      startedAt: startedAt,
      finishedAt: finishedAt,
      queueDelayMilliseconds: milliseconds(from: request.requestedAt, to: startedAt),
      renderDurationMilliseconds: milliseconds(from: startedAt, to: finishedAt),
      rendererDurationMilliseconds: outcome.rendererDurationMilliseconds,
      dirtyPages: outcome.dirtyPages,
      skipReason: outcome.skipReason,
      error: outcome.error
    ))
    if history.count > historyLimit {
      history.removeFirst(history.count - historyLimit)
    }

    if let pending = pendingRequest {
      pendingRequest = nil
      scheduledRequest = pending
      scheduleCurrentRequest()
    } else {
      signalStateChanged()
    }
  }

  private func milliseconds(from start: Date, to end: Date) -> Int {
    max(0, Int((end.timeIntervalSince(start) * 1_000).rounded()))
  }

  private func isIdle() -> Bool {
    let snapshot = stateQueue.sync {
      runningRequest == nil && scheduledRequest == nil && pendingRequest == nil
    }
    return snapshot
  }

  private func signalStateChanged() {
  }
}
