import Foundation
import XCTest
@testable import OneContextWikiRuntime

final class WikiRenderQueueTests: XCTestCase {
  func testRapidRequestsAreSingleFlightAndCoalesced() throws {
    let counters = RenderCounters()

    let queue = WikiRenderQueue(debounceInterval: 0.02, failureBackoffInterval: 0.1) { _ in
      counters.begin()
      Thread.sleep(forTimeInterval: 0.02)
      counters.end()
      return WikiRenderQueueOutcome(
        status: .published,
        dirtyPages: 1,
        rendererDurationMilliseconds: 20
      )
    }

    for _ in 0..<100 {
      queue.request(trigger: "wiki.refresh", priority: .automatic)
    }

    XCTAssertTrue(queue.waitUntilIdle(timeout: 3))
    let snapshot = queue.snapshot()
    XCTAssertEqual(snapshot.acceptedCount, 100)
    XCTAssertGreaterThanOrEqual(snapshot.coalescedCount, 99)
    XCTAssertEqual(snapshot.maxConcurrentRenders, 1)
    XCTAssertEqual(counters.maxActive, 1)
  }

  func testFailureBackoffDelaysAutomaticButManualRunsImmediately() throws {
    let counters = RenderCounters()
    let queue = WikiRenderQueue(debounceInterval: 0.01, failureBackoffInterval: 0.5) { request in
      let currentRun = counters.incrementRuns()

      if currentRun == 1 {
        return WikiRenderQueueOutcome(
          status: .failed,
          dirtyPages: 1,
          rendererDurationMilliseconds: 1,
          error: "fixture failure"
        )
      }
      return WikiRenderQueueOutcome(
        status: .published,
        dirtyPages: request.priority == .manual ? 2 : 1,
        rendererDurationMilliseconds: 1
      )
    }

    queue.request(trigger: "wiki.refresh", priority: .automatic)
    XCTAssertTrue(queue.waitUntilIdle(timeout: 2))
    XCTAssertEqual(queue.snapshot().failedCount, 1)

    queue.request(trigger: "wiki.refresh", priority: .automatic)
    Thread.sleep(forTimeInterval: 0.05)
    XCTAssertEqual(queue.snapshot().completedCount, 1)

    queue.request(trigger: "wiki.refresh", priority: .manual)
    XCTAssertTrue(queue.waitUntilIdle(timeout: 2))

    let snapshot = queue.snapshot()
    XCTAssertEqual(snapshot.completedCount, 2)
    XCTAssertEqual(snapshot.history.last?.priority, .manual)
    XCTAssertEqual(snapshot.history.last?.dirtyPages, 2)
  }

  func testRecordsDurationsDirtyPagesAndSkipReason() throws {
    let queue = WikiRenderQueue(debounceInterval: 0, failureBackoffInterval: 0.1) { _ in
      WikiRenderQueueOutcome(
        status: .skipped,
        dirtyPages: 0,
        rendererDurationMilliseconds: 0,
        skipReason: "accepted_inputs_unchanged"
      )
    }

    queue.request(trigger: "wiki.refresh", priority: .manual)
    XCTAssertTrue(queue.waitUntilIdle(timeout: 2))

    let snapshot = queue.snapshot()
    XCTAssertEqual(snapshot.skippedCount, 1)
    let record = try XCTUnwrap(snapshot.history.last)
    XCTAssertEqual(record.status, .skipped)
    XCTAssertEqual(record.trigger, "wiki.refresh")
    XCTAssertEqual(record.dirtyPages, 0)
    XCTAssertEqual(record.rendererDurationMilliseconds, 0)
    XCTAssertEqual(record.skipReason, "accepted_inputs_unchanged")
    XCTAssertGreaterThanOrEqual(record.renderDurationMilliseconds, 0)
    XCTAssertGreaterThanOrEqual(record.queueDelayMilliseconds, 0)
  }
}

private final class RenderCounters: @unchecked Sendable {
  private let lock = NSLock()
  private var activeValue = 0
  private var maxActiveValue = 0
  private var runValue = 0

  var maxActive: Int {
    lock.lock()
    defer { lock.unlock() }
    return maxActiveValue
  }

  func begin() {
    lock.lock()
    activeValue += 1
    maxActiveValue = max(maxActiveValue, activeValue)
    lock.unlock()
  }

  func end() {
    lock.lock()
    activeValue -= 1
    lock.unlock()
  }

  func incrementRuns() -> Int {
    lock.lock()
    defer { lock.unlock() }
    runValue += 1
    return runValue
  }
}
