import Foundation
import Testing
@testable import OneContextAgentOffice
import OneContextPlatform

@Suite("Agent office runtime adapter")
struct AgentOfficeRuntimeAdapterTests {
  @MainActor
  @Test("renders lightweight token telemetry and role activities")
  func rendersLightweightTelemetryAndActivities() {
    let view = AgentOfficeView(frame: CGRect(origin: .zero, size: CGSize(width: 512, height: 288)))
    view.apply(snapshot: AgentOfficeRunSnapshot(
      runID: "dummy-office",
      status: "running",
      monthlyTokenUsage: 1_234_567,
      tokensPerSecond: 24_500,
      activeTurnCount: 3,
      agents: [
        AgentOfficeAgentSnapshot(unitID: "born", jobID: "memory.hourly.scribe", role: "scribe", phase: "birth", lifecycle: .born),
        AgentOfficeAgentSnapshot(unitID: "working", jobID: "memory.wiki.biographer", role: "biographer", phase: "turn", lifecycle: .working),
        AgentOfficeAgentSnapshot(unitID: "posted", jobID: "memory.wiki.librarian", role: "librarian", phase: "mail", lifecycle: .postedMail, hasTalkReceipt: true),
        AgentOfficeAgentSnapshot(unitID: "waiting", jobID: "memory.wiki.curator", role: "curator", phase: "wait", lifecycle: .waiting),
        AgentOfficeAgentSnapshot(unitID: "failed", jobID: "memory.wiki.redactor", role: "contradiction", phase: "settled", lifecycle: .failed),
        AgentOfficeAgentSnapshot(unitID: "completed", jobID: "memory.wiki.publisher", role: "publisher", phase: "settled", lifecycle: .completed)
      ]
    ))

    #expect(view.renderedTelemetry.month == "Month 1.2M tokens")
    #expect(view.renderedTelemetry.rate == "24.5k tok/s")
    #expect(view.renderedTelemetry.status == "running | 3 active | 0 done")
    #expect(view.renderedAgents.map(\.activity) == [
      "starting",
      "profiling",
      "posting mail",
      "waiting",
      "retry",
      "done"
    ])

    view.apply(snapshot: AgentOfficeRunSnapshot(
      runID: "dummy-office-pending",
      status: "running",
      monthlyTokenUsage: 42_000,
      activeTurnCount: 4
    ))
    #expect(view.renderedTelemetry.month == "Month 42k tokens")
    #expect(view.renderedTelemetry.rate == "settled tok/s")
    #expect(view.renderedTelemetry.status == "running | 4 active | 0 done")
  }

  @Test("returns idle when no wiki update run exists")
  func idleWhenNoRunsExist() {
    let temp = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
    let adapter = AgentOfficeRuntimeAdapter(
      runtimePaths: testRuntimePaths(root: temp),
      updateDirectories: [temp.appendingPathComponent("wiki-updates", isDirectory: true)]
    )

    let snapshot = adapter.latestSnapshot()

    #expect(snapshot.status == "idle")
    #expect(snapshot.agents.isEmpty)
  }

  @Test("normalizes latest update jobs into office agents")
  func normalizesLatestUpdateJobs() throws {
    let temp = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
    let updateRoot = temp.appendingPathComponent("wiki-updates", isDirectory: true)
    let olderRun = updateRoot.appendingPathComponent("older", isDirectory: true)
    let newerRun = updateRoot.appendingPathComponent("newer", isDirectory: true)
    try FileManager.default.createDirectory(at: olderRun, withIntermediateDirectories: true)
    try FileManager.default.createDirectory(at: newerRun, withIntermediateDirectories: true)
    try writeUpdateJSON(
      at: olderRun.appendingPathComponent("update.json"),
      runID: "older",
      status: "completed",
      jobs: []
    )
    try writeUpdateJSON(
      at: newerRun.appendingPathComponent("update.json"),
      runID: "newer",
      status: "completed",
      jobs: [
        """
        {
          "phase": "scribe_wave",
          "job_id": "memory.hourly.scribe",
          "run_id": "newer-01-memory-hourly-scribe",
          "status": "completed",
          "harness_call": {"status": "accepted"},
          "harness_turn_start": {"status": "accepted"},
          "harness_adapter_events": [{"kind": "runtime_wakeup_accepted"}],
          "harness_turn_complete": {"evidence": {"outcome": "done"}},
          "talk_receipt": {"status": "appended"}
        }
        """,
        """
        {
          "phase": "specialist_wave",
          "job_id": "memory.wiki.biographer",
          "run_id": "newer-02-memory-wiki-biographer",
          "status": "failed",
          "error": "timed out"
        }
        """
      ]
    )
    try FileManager.default.setAttributes(
      [.modificationDate: Date(timeIntervalSince1970: 100)],
      ofItemAtPath: olderRun.appendingPathComponent("update.json").path
    )
    try FileManager.default.setAttributes(
      [.modificationDate: Date(timeIntervalSince1970: 200)],
      ofItemAtPath: newerRun.appendingPathComponent("update.json").path
    )

    let adapter = AgentOfficeRuntimeAdapter(
      runtimePaths: testRuntimePaths(root: temp),
      updateDirectories: [updateRoot]
    )

    let snapshot = adapter.latestSnapshot()

    #expect(snapshot.runID == "newer")
    #expect(snapshot.status == "completed")
    #expect(snapshot.agents.count == 2)
    #expect(snapshot.agents[0].role == "scribe")
    #expect(snapshot.agents[0].lifecycle == .postedMail)
    #expect(snapshot.agents[0].hasTalkReceipt)
    #expect(snapshot.agents[1].role == "biographer")
    #expect(snapshot.agents[1].lifecycle == .failed)
  }

  @Test("sums current-month token usage and ignores prior-month runs")
  func sumsCurrentMonthTokenUsage() throws {
    let temp = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
    let updateRoot = temp.appendingPathComponent("wiki-updates", isDirectory: true)
    let currentRun = updateRoot.appendingPathComponent("current", isDirectory: true)
    let priorRun = updateRoot.appendingPathComponent("prior", isDirectory: true)
    let currentStdout = temp.appendingPathComponent("current-run.stdout.jsonl")
    let priorStdout = temp.appendingPathComponent("prior-run.stdout.jsonl")
    try FileManager.default.createDirectory(at: currentRun, withIntermediateDirectories: true)
    try FileManager.default.createDirectory(at: priorRun, withIntermediateDirectories: true)
    try writeCodexUsageJSONL(at: currentStdout, input: 100, output: 20, reasoning: 5)
    try writeCodexUsageJSONL(at: priorStdout, input: 9_000, output: 900, reasoning: 90)
    try writeUpdateJSON(
      at: currentRun.appendingPathComponent("update.json"),
      runID: "current",
      status: "completed",
      jobs: [
        """
        {
          "phase": "scribe_wave",
          "job_id": "memory.hourly.scribe",
          "run_id": "current-01-memory-hourly-scribe",
          "status": "completed",
          "stdout_path": "\(currentStdout.path)"
        }
        """
      ]
    )
    try writeUpdateJSON(
      at: priorRun.appendingPathComponent("update.json"),
      runID: "prior",
      status: "completed",
      jobs: [
        """
        {
          "phase": "scribe_wave",
          "job_id": "memory.hourly.scribe",
          "run_id": "prior-01-memory-hourly-scribe",
          "status": "completed",
          "stdout_path": "\(priorStdout.path)"
        }
        """
      ]
    )
    try setModificationDate(Date(timeIntervalSince1970: 1_780_640_000), for: currentRun.appendingPathComponent("update.json"))
    try setModificationDate(Date(timeIntervalSince1970: 1_777_960_000), for: priorRun.appendingPathComponent("update.json"))

    let adapter = AgentOfficeRuntimeAdapter(
      runtimePaths: testRuntimePaths(root: temp),
      updateDirectories: [updateRoot],
      calendar: utcCalendar,
      now: { Date(timeIntervalSince1970: 1_781_000_000) }
    )

    let snapshot = adapter.latestSnapshot()

    #expect(snapshot.monthlyTokenUsage == 125)
    #expect(!snapshot.monthlyTokenUsageIsApproximate)
  }

  @Test("reads token usage from sibling agent-session stdout when update omits stdout path")
  func readsAgentSessionStdoutWhenUpdateOmitsStdoutPath() throws {
    let temp = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
    let updateRoot = temp.appendingPathComponent("memory/runtime/wiki-updates", isDirectory: true)
    let run = updateRoot.appendingPathComponent("current", isDirectory: true)
    let unitID = "current-01-memory-hourly-scribe"
    let agentSession = temp.appendingPathComponent("memory/runtime/agent-sessions/\(unitID)", isDirectory: true)
    try FileManager.default.createDirectory(at: run, withIntermediateDirectories: true)
    try FileManager.default.createDirectory(at: agentSession, withIntermediateDirectories: true)
    try writeCodexUsageJSONL(at: agentSession.appendingPathComponent("run.stdout.jsonl"), input: 70, output: 12, reasoning: 3)
    try writeUpdateJSON(
      at: run.appendingPathComponent("update.json"),
      runID: "current",
      status: "running",
      jobs: [
        """
        {
          "phase": "scribe_wave",
          "job_id": "memory.hourly.scribe",
          "run_id": "\(unitID)",
          "status": "running",
          "harness_turn_start": {"started_at": "2026-06-05T12:00:00Z"}
        }
        """
      ]
    )
    try setModificationDate(Date(timeIntervalSince1970: 1_780_640_000), for: run.appendingPathComponent("update.json"))

    let adapter = AgentOfficeRuntimeAdapter(
      runtimePaths: testRuntimePaths(root: temp),
      updateDirectories: [updateRoot],
      calendar: utcCalendar,
      now: { parseTestDate("2026-06-05T12:01:00Z") }
    )

    let snapshot = adapter.latestSnapshot()

    #expect(snapshot.monthlyTokenUsage == 85)
    #expect(!snapshot.monthlyTokenUsageIsApproximate)
    #expect(snapshot.activeTurnCount == 1)
    #expect(snapshot.tokensPerSecond == nil)
    #expect(!snapshot.tokensPerSecondIsApproximate)
  }

  @Test("overlays active harness units without counting estimates")
  func overlaysActiveHarnessUnitsWithoutCountingEstimates() throws {
    let temp = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
    let runtimePaths = testRuntimePaths(root: temp)
    let updateRoot = temp.appendingPathComponent("wiki-updates", isDirectory: true)
    let run = updateRoot.appendingPathComponent("company", isDirectory: true)
    try FileManager.default.createDirectory(at: run, withIntermediateDirectories: true)
    try writeUpdateJSON(
      at: run.appendingPathComponent("update.json"),
      runID: "company",
      status: "running",
      jobs: [
        plannedJob(unitID: "company-01-memory-hourly-scribe", jobID: "memory.hourly.scribe", estimatedTokens: 120),
        plannedJob(unitID: "company-02-memory-wiki-biographer", jobID: "memory.wiki.biographer", estimatedTokens: 240)
      ]
    )
    try setModificationDate(Date(timeIntervalSince1970: 1_780_640_000), for: run.appendingPathComponent("update.json"))
    let harnessRoot = runtimePaths.contextEngineDirectory.appendingPathComponent("agents/harness/units", isDirectory: true)
    try writeHarnessUnit(
      root: harnessRoot,
      unitID: "company-01-memory-hourly-scribe",
      jobID: "memory.hourly.scribe",
      role: "hourly-scribe",
      phase: "scribe_wave",
      startedAt: "2026-06-05T12:00:00Z"
    )
    try writeHarnessUnit(
      root: harnessRoot,
      unitID: "company-02-memory-wiki-biographer",
      jobID: "memory.wiki.biographer",
      role: "biographer",
      phase: "specialist_wave",
      startedAt: "2026-06-05T12:00:00Z"
    )

    let adapter = AgentOfficeRuntimeAdapter(
      runtimePaths: runtimePaths,
      updateDirectories: [updateRoot],
      calendar: utcCalendar,
      now: { parseTestDate("2026-06-05T12:01:00Z") }
    )

    let snapshot = adapter.latestSnapshot()

    #expect(snapshot.agents.map(\.lifecycle) == [.working, .working])
    #expect(snapshot.activeTurnCount == 2)
    #expect(snapshot.monthlyTokenUsage == 0)
    #expect(!snapshot.monthlyTokenUsageIsApproximate)
    #expect(snapshot.tokensPerSecond == nil)
    #expect(!snapshot.tokensPerSecondIsApproximate)
  }

  @Test("uses active harness run before update json exists")
  func usesActiveHarnessRunBeforeUpdateJSONExists() throws {
    let temp = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
    let runtimePaths = testRuntimePaths(root: temp)
    let harnessRoot = runtimePaths.contextEngineDirectory.appendingPathComponent("agents/harness/units", isDirectory: true)
    try writeHarnessUnit(
      root: harnessRoot,
      unitID: "company-01-memory-hourly-scribe",
      jobID: "memory.hourly.scribe",
      role: "hourly-scribe",
      phase: "scribe_wave",
      startedAt: "2026-06-05T12:00:00Z"
    )

    let adapter = AgentOfficeRuntimeAdapter(
      runtimePaths: runtimePaths,
      updateDirectories: [temp.appendingPathComponent("wiki-updates", isDirectory: true)],
      calendar: utcCalendar,
      now: { parseTestDate("2026-06-05T12:01:00Z") }
    )

    let snapshot = adapter.latestSnapshot()

    #expect(snapshot.runID == "company")
    #expect(snapshot.status == "running")
    #expect(snapshot.agents.count == 1)
    #expect(snapshot.agents[0].lifecycle == .working)
    #expect(snapshot.activeTurnCount == 1)
  }

  @Test("keeps settled harness units in active run for exact token rate")
  func keepsSettledHarnessUnitsInActiveRunForExactTokenRate() throws {
    let temp = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
    let runtimePaths = testRuntimePaths(root: temp)
    let harnessRoot = runtimePaths.contextEngineDirectory.appendingPathComponent("agents/harness/units", isDirectory: true)
    let completedUnitID = "company-01-memory-hourly-scribe"
    let activeUnitID = "company-02-memory-wiki-biographer"
    let completedSession = runtimePaths.contextEngineDirectory.appendingPathComponent("memory/runtime/agent-sessions/\(completedUnitID)", isDirectory: true)
    try FileManager.default.createDirectory(at: completedSession, withIntermediateDirectories: true)
    try writeCodexUsageJSONL(at: completedSession.appendingPathComponent("run.stdout.jsonl"), input: 100, output: 40, reasoning: 20)
    try writeCompletedHarnessUnit(
      root: harnessRoot,
      unitID: completedUnitID,
      jobID: "memory.hourly.scribe",
      role: "hourly-scribe",
      phase: "scribe_wave",
      startedAt: "2026-06-05T12:00:00Z",
      completedAt: "2026-06-05T12:00:20Z",
      durationMS: 20_000,
      runDirectory: completedSession
    )
    try writeHarnessUnit(
      root: harnessRoot,
      unitID: activeUnitID,
      jobID: "memory.wiki.biographer",
      role: "biographer",
      phase: "specialist_wave",
      startedAt: "2026-06-05T12:00:30Z"
    )

    let adapter = AgentOfficeRuntimeAdapter(
      runtimePaths: runtimePaths,
      updateDirectories: [temp.appendingPathComponent("wiki-updates", isDirectory: true)],
      calendar: utcCalendar,
      now: { parseTestDate("2026-06-05T12:01:00Z") }
    )

    let snapshot = adapter.latestSnapshot()

    #expect(snapshot.runID == "company")
    #expect(snapshot.agents.count == 2)
    #expect(snapshot.agents.map(\.lifecycle) == [.completed, .working])
    #expect(snapshot.activeTurnCount == 1)
    #expect(snapshot.monthlyTokenUsage == 160)
    #expect(abs((snapshot.tokensPerSecond ?? 0) - 8.0) < 0.001)
  }

  @Test("reports exact settled token rate from completed harness turns")
  func reportsExactSettledTokenRateFromCompletedHarnessTurns() throws {
    let temp = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
    let runtimePaths = testRuntimePaths(root: temp)
    let updateRoot = temp.appendingPathComponent("wiki-updates", isDirectory: true)
    let run = updateRoot.appendingPathComponent("company", isDirectory: true)
    let unitID = "company-01-memory-hourly-scribe"
    let harnessRoot = runtimePaths.contextEngineDirectory.appendingPathComponent("agents/harness/units", isDirectory: true)
    let agentSession = runtimePaths.contextEngineDirectory.appendingPathComponent("memory/runtime/agent-sessions/\(unitID)", isDirectory: true)
    try FileManager.default.createDirectory(at: run, withIntermediateDirectories: true)
    try FileManager.default.createDirectory(at: agentSession, withIntermediateDirectories: true)
    try writeCodexUsageJSONL(at: agentSession.appendingPathComponent("run.stdout.jsonl"), input: 100, output: 40, reasoning: 20)
    try writeCompletedHarnessUnit(
      root: harnessRoot,
      unitID: unitID,
      jobID: "memory.hourly.scribe",
      role: "hourly-scribe",
      phase: "scribe_wave",
      startedAt: "2026-06-05T12:00:00Z",
      completedAt: "2026-06-05T12:00:20Z",
      durationMS: 20_000,
      runDirectory: agentSession
    )
    try writeUpdateJSON(
      at: run.appendingPathComponent("update.json"),
      runID: "company",
      status: "completed",
      jobs: [
        """
        {
          "phase": "scribe_wave",
          "job_id": "memory.hourly.scribe",
          "run_id": "\(unitID)",
          "status": "completed"
        }
        """
      ]
    )
    try setModificationDate(Date(timeIntervalSince1970: 1_780_640_000), for: run.appendingPathComponent("update.json"))

    let adapter = AgentOfficeRuntimeAdapter(
      runtimePaths: runtimePaths,
      updateDirectories: [updateRoot],
      calendar: utcCalendar,
      now: { parseTestDate("2026-06-05T12:01:00Z") }
    )

    let snapshot = adapter.latestSnapshot()

    #expect(snapshot.monthlyTokenUsage == 160)
    #expect(abs((snapshot.tokensPerSecond ?? 0) - 8.0) < 0.001)
    #expect(snapshot.activeTurnCount == 0)
  }
}

private func testRuntimePaths(root: URL) -> RuntimePaths {
  RuntimePaths(
    userContentDirectory: root.appendingPathComponent("content", isDirectory: true),
    appSupportDirectory: root.appendingPathComponent("support", isDirectory: true),
    logDirectory: root.appendingPathComponent("logs", isDirectory: true),
    cacheDirectory: root.appendingPathComponent("cache", isDirectory: true)
  )
}

private func writeUpdateJSON(at url: URL, runID: String, status: String, jobs: [String]) throws {
  let body = """
  {
    "run_id": "\(runID)",
    "status": "\(status)",
    "planned_count": \(jobs.count),
    "completed_count": 1,
    "failed_count": 1,
    "jobs": [
      \(jobs.joined(separator: ",\n"))
    ]
  }
  """
  try body.write(to: url, atomically: true, encoding: .utf8)
}

private var utcCalendar: Calendar {
  var calendar = Calendar(identifier: .gregorian)
  calendar.timeZone = TimeZone(secondsFromGMT: 0)!
  return calendar
}

private func setModificationDate(_ date: Date, for url: URL) throws {
  try FileManager.default.setAttributes([.modificationDate: date], ofItemAtPath: url.path)
}

private func writeCodexUsageJSONL(at url: URL, input: Int, output: Int, reasoning: Int) throws {
  let body = """
  {"type":"turn.completed","usage":{"input_tokens":\(input),"output_tokens":\(output),"reasoning_output_tokens":\(reasoning)}}
  """
  try body.write(to: url, atomically: true, encoding: .utf8)
}

private func plannedJob(unitID: String, jobID: String, estimatedTokens: Int) -> String {
  """
  {
    "phase": "planned_wave",
    "job_id": "\(jobID)",
    "run_id": "\(unitID)",
    "status": "planned",
    "plan": {
      "params": {
        "source_packet_estimated_tokens": "\(estimatedTokens)"
      }
    }
  }
  """
}

private func writeHarnessUnit(
  root: URL,
  unitID: String,
  jobID: String,
  role: String,
  phase: String,
  startedAt: String
) throws {
  let receipts = root.appendingPathComponent(unitID, isDirectory: true).appendingPathComponent("receipts", isDirectory: true)
  try FileManager.default.createDirectory(at: receipts, withIntermediateDirectories: true)
  let called = """
  {
    "kind": "agent_called",
    "at": "\(startedAt)",
    "unit_id": "\(unitID)",
    "evidence": {
      "certificate": {
        "identity": {"job_id": "\(jobID)", "agent_id": "\(role)"},
        "role": "\(role)",
        "runtime": {
          "run_dir": "\(root.deletingLastPathComponent().path)/agent-sessions/\(unitID)"
        }
      },
      "metadata": {
        "called_at": "\(startedAt)"
      }
    }
  }
  """
  let started = """
  {
    "kind": "turn_started",
    "at": "\(startedAt)",
    "unit_id": "\(unitID)",
    "evidence": {
      "started_at": "\(startedAt)",
      "metadata": {
        "metadata": {
          "job_id": "\(jobID)",
          "phase": "\(phase)"
        }
      }
    }
  }
  """
  try called.write(to: receipts.appendingPathComponent("agent-receipt-000-called.json"), atomically: true, encoding: .utf8)
  try started.write(to: receipts.appendingPathComponent("agent-receipt-001-turn-started.json"), atomically: true, encoding: .utf8)
}

private func writeCompletedHarnessUnit(
  root: URL,
  unitID: String,
  jobID: String,
  role: String,
  phase: String,
  startedAt: String,
  completedAt: String,
  durationMS: Int,
  runDirectory: URL
) throws {
  try writeHarnessUnit(
    root: root,
    unitID: unitID,
    jobID: jobID,
    role: role,
    phase: phase,
    startedAt: startedAt
  )
  let receipts = root.appendingPathComponent(unitID, isDirectory: true).appendingPathComponent("receipts", isDirectory: true)
  let calledPath = receipts.appendingPathComponent("agent-receipt-000-called.json")
  var called = try String(contentsOf: calledPath, encoding: .utf8)
  called = called.replacingOccurrences(
    of: "\"run_dir\": \"\(root.deletingLastPathComponent().path)/agent-sessions/\(unitID)\"",
    with: "\"run_dir\": \"\(runDirectory.path)\""
  )
  try called.write(to: calledPath, atomically: true, encoding: .utf8)

  let completed = """
  {
    "kind": "turn_completed",
    "at": "\(completedAt)",
    "unit_id": "\(unitID)",
    "evidence": {
      "completed_at": "\(completedAt)",
      "lifecycle_state": "done",
      "next_state": "done",
      "usage": {
        "duration_ms": \(durationMS),
        "input_tokens": 0,
        "output_tokens": 0,
        "total_tokens": 0
      }
    }
  }
  """
  try completed.write(to: receipts.appendingPathComponent("agent-receipt-002-turn-completed.json"), atomically: true, encoding: .utf8)
}

private func parseTestDate(_ value: String) -> Date {
  ISO8601DateFormatter().date(from: value)!
}
