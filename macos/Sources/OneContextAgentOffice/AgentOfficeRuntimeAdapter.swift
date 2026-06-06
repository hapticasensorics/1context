import Foundation
import OneContextPlatform

public final class AgentOfficeRuntimeAdapter: @unchecked Sendable {
  private let runtimePaths: RuntimePaths
  private let fileManager: FileManager
  private let environment: [String: String]
  private let explicitUpdateDirectories: [URL]
  private let explicitHarnessUnitDirectories: [URL]
  private let explicitAgentSessionDirectories: [URL]
  private let calendar: Calendar
  private let now: () -> Date
  private var stdoutTokenCache: [String: CachedTokenMeasurement] = [:]

  public init(
    runtimePaths: RuntimePaths,
    fileManager: FileManager = .default,
    environment: [String: String] = ProcessInfo.processInfo.environment,
    updateDirectories: [URL] = [],
    harnessUnitDirectories: [URL] = [],
    agentSessionDirectories: [URL] = [],
    calendar: Calendar = .current,
    now: @escaping () -> Date = Date.init
  ) {
    self.runtimePaths = runtimePaths
    self.fileManager = fileManager
    self.environment = environment
    self.explicitUpdateDirectories = updateDirectories
    self.explicitHarnessUnitDirectories = harnessUnitDirectories
    self.explicitAgentSessionDirectories = agentSessionDirectories
    self.calendar = calendar
    self.now = now
  }

  public func latestSnapshot() -> AgentOfficeRunSnapshot {
    let updates = updateJSONFiles()
    let allHarnessUnits = harnessUnitSnapshots()
    let nowDate = now()
    let monthlyUsage = monthlyTokenUsage(from: updates, harnessUnits: allHarnessUnits, now: nowDate)
    if let harnessRun = latestActiveHarnessRun(from: allHarnessUnits),
      harnessRun.updatedAt > (latestUpdateJSON(from: updates)?.modifiedAt ?? .distantPast)
    {
      var snapshot = harnessRun.snapshot
      applyTokenTelemetry(
        to: &snapshot,
        monthlyUsage: monthlyUsage,
        settledRate: harnessRun.settledRate,
        activeTurnCount: harnessRun.activeTurnCount
      )
      return snapshot
    }
    guard let update = latestOfficeUpdateJSON(from: updates, allHarnessUnits: allHarnessUnits) else {
      var snapshot = AgentOfficeRunSnapshot.idle
      applyTokenTelemetry(
        to: &snapshot,
        monthlyUsage: monthlyUsage,
        settledRate: nil,
        activeTurnCount: 0
      )
      return snapshot
    }
    do {
      let data = try Data(contentsOf: update.url)
      let receipt = try JSONDecoder().decode(WikiUpdateReceipt.self, from: data)
      let harnessUnits = harnessSnapshots(for: receipt, allHarnessUnits: allHarnessUnits)
      var snapshot = receipt.snapshot(sourcePath: update.url.path, updatedAt: update.modifiedAt, harnessUnits: harnessUnits)
      applyTokenTelemetry(
        to: &snapshot,
        monthlyUsage: monthlyUsage,
        settledRate: settledTokenRate(from: receipt, update: update, harnessUnits: harnessUnits),
        activeTurnCount: activeTurnCount(from: receipt, harnessUnits: harnessUnits)
      )
      return snapshot
    } catch {
      var snapshot = AgentOfficeRunSnapshot(
        runID: update.url.deletingLastPathComponent().lastPathComponent,
        status: "unreadable",
        sourcePath: update.url.path,
        updatedAt: update.modifiedAt,
        failedCount: 1,
        agents: [
          AgentOfficeAgentSnapshot(
            unitID: "agent-office-adapter",
            jobID: "agent.office.read_update",
            role: "curator",
            phase: "monitor",
            lifecycle: .failed,
            error: error.localizedDescription
          )
        ]
      )
      applyTokenTelemetry(
        to: &snapshot,
        monthlyUsage: monthlyUsage,
        settledRate: nil,
        activeTurnCount: 0
      )
      return snapshot
    }
  }

  public func updateDirectories() -> [URL] {
    var result = explicitUpdateDirectories
    if let override = environment["ONECONTEXT_AGENT_OFFICE_WIKI_UPDATES"], !override.isEmpty {
      result.append(URL(fileURLWithPath: override, isDirectory: true))
    }
    if let memoryRoot = environment["ONECONTEXT_MEMORY_CORE_ROOT"], !memoryRoot.isEmpty {
      result.append(URL(fileURLWithPath: memoryRoot, isDirectory: true).appendingPathComponent("memory/runtime/wiki-updates", isDirectory: true))
    }
    result.append(runtimePaths.contextEngineDirectory.appendingPathComponent("memory/runtime/wiki-updates", isDirectory: true))
    result.append(runtimePaths.contextEngineDirectory.appendingPathComponent("wiki-updates", isDirectory: true))
    result.append(URL(fileURLWithPath: fileManager.currentDirectoryPath, isDirectory: true).appendingPathComponent("memory-core/memory/runtime/wiki-updates", isDirectory: true))
    result += memoryCoreRootCandidates().map {
      $0.appendingPathComponent("memory/runtime/wiki-updates", isDirectory: true)
    }
    return dedupeURLs(result)
  }

  public func harnessUnitDirectories() -> [URL] {
    var result = explicitHarnessUnitDirectories
    if let override = environment["ONECONTEXT_AGENT_OFFICE_HARNESS_UNITS"], !override.isEmpty {
      result.append(URL(fileURLWithPath: override, isDirectory: true))
    }
    result.append(runtimePaths.contextEngineDirectory.appendingPathComponent("agents/harness/units", isDirectory: true))
    return dedupeURLs(result)
  }

  private func updateJSONFiles() -> [UpdateJSONFile] {
    var updates: [UpdateJSONFile] = []
    var seen: Set<String> = []
    for directory in updateDirectories() where fileManager.fileExists(atPath: directory.path) {
      let children = (try? fileManager.contentsOfDirectory(
        at: directory,
        includingPropertiesForKeys: [.contentModificationDateKey],
        options: [.skipsHiddenFiles]
      )) ?? []
      for child in children {
        let candidate = child.appendingPathComponent("update.json")
        guard fileManager.fileExists(atPath: candidate.path) else { continue }
        let standardizedPath = candidate.standardizedFileURL.path
        guard !seen.contains(standardizedPath) else { continue }
        seen.insert(standardizedPath)
        let modifiedAt = modificationDate(for: candidate) ?? .distantPast
        updates.append(UpdateJSONFile(url: candidate, runDirectory: child, updateRoot: directory, modifiedAt: modifiedAt))
      }
    }
    return updates
  }

  private func latestUpdateJSON(from updates: [UpdateJSONFile]) -> UpdateJSONFile? {
    updates.max { left, right in
      left.modifiedAt < right.modifiedAt
    }
  }

  private func latestOfficeUpdateJSON(
    from updates: [UpdateJSONFile],
    allHarnessUnits: [HarnessUnitSnapshot]
  ) -> UpdateJSONFile? {
    var latestActive: (update: UpdateJSONFile, activity: Date)?
    for update in updates {
      guard
        let data = try? Data(contentsOf: update.url),
        let receipt = try? JSONDecoder().decode(WikiUpdateReceipt.self, from: data)
      else {
        continue
      }
      let harnessUnits = harnessSnapshots(for: receipt, allHarnessUnits: allHarnessUnits)
      guard receipt.isActive || harnessUnits.values.contains(where: \.isActive) else {
        continue
      }
      let activity = ([update.modifiedAt] + harnessUnits.values.compactMap(\.lastActiveAt)).max() ?? update.modifiedAt
      if latestActive == nil || activity > (latestActive?.activity ?? .distantPast) {
        latestActive = (update, activity)
      }
    }
    return latestActive?.update ?? latestUpdateJSON(from: updates)
  }

  private func modificationDate(for url: URL) -> Date? {
    let values = try? url.resourceValues(forKeys: [.contentModificationDateKey])
    return values?.contentModificationDate
  }

  private func harnessUnitSnapshots() -> [HarnessUnitSnapshot] {
    var snapshots: [HarnessUnitSnapshot] = []
    var seen: Set<String> = []
    for directory in harnessUnitDirectories() where fileManager.fileExists(atPath: directory.path) {
      let children = (try? fileManager.contentsOfDirectory(
        at: directory,
        includingPropertiesForKeys: [.contentModificationDateKey],
        options: [.skipsHiddenFiles]
      )) ?? []
      for child in children {
        let isDirectory = (try? child.resourceValues(forKeys: [.isDirectoryKey]))?.isDirectory == true
        guard isDirectory else { continue }
        let unitID = child.lastPathComponent
        guard !seen.contains(unitID) else { continue }
        seen.insert(unitID)
        snapshots.append(harnessSnapshot(unitID: unitID, unitDirectory: child))
      }
    }
    return snapshots
  }

  private func harnessSnapshots(
    for receipt: WikiUpdateReceipt,
    allHarnessUnits: [HarnessUnitSnapshot]
  ) -> [String: HarnessUnitSnapshot] {
    let unitIDs = Set((receipt.jobs ?? []).map(\.unitID))
    guard !unitIDs.isEmpty else { return [:] }

    var snapshots: [String: HarnessUnitSnapshot] = [:]
    for unit in allHarnessUnits where unitIDs.contains(unit.unitID) {
      snapshots[unit.unitID] = unit
    }
    for directory in harnessUnitDirectories() where fileManager.fileExists(atPath: directory.path) {
      for unitID in unitIDs where snapshots[unitID] == nil {
        let unitDirectory = directory.appendingPathComponent(unitID, isDirectory: true)
        guard fileManager.fileExists(atPath: unitDirectory.path) else { continue }
        snapshots[unitID] = harnessSnapshot(unitID: unitID, unitDirectory: unitDirectory)
      }
    }
    return snapshots
  }

  private func harnessSnapshot(unitID: String, unitDirectory: URL) -> HarnessUnitSnapshot {
    let receiptDirectory = unitDirectory.appendingPathComponent("receipts", isDirectory: true)
    let receiptFiles = ((try? fileManager.contentsOfDirectory(
      at: receiptDirectory,
      includingPropertiesForKeys: [.contentModificationDateKey],
      options: [.skipsHiddenFiles]
    )) ?? []).filter { $0.pathExtension == "json" }

    var snapshot = HarnessUnitSnapshot(unitID: unitID)
    for receiptFile in receiptFiles.sorted(by: { $0.lastPathComponent < $1.lastPathComponent }) {
      guard
        let data = try? Data(contentsOf: receiptFile),
        let receipt = try? JSONDecoder().decode(LooseJSONObject.self, from: data)
      else {
        continue
      }
      snapshot.apply(receipt: receipt, tokenUsage: explicitTokenUsage(in: receipt.value))
    }
    if snapshot.tokenUsage == nil,
      let stdoutPath = snapshot.stdoutPath
    {
      let stdoutURL = URL(fileURLWithPath: stdoutPath)
      if fileManager.fileExists(atPath: stdoutURL.path),
        let stdoutUsage = stdoutTokenUsage(url: stdoutURL)
      {
        snapshot.tokenUsage = stdoutUsage
      }
    }
    return snapshot
  }

  private func latestActiveHarnessRun(from units: [HarnessUnitSnapshot]) -> HarnessRunSnapshot? {
    let activeUnits = units.filter(\.isActive)
    guard !activeUnits.isEmpty else { return nil }
    let grouped = Dictionary(grouping: activeUnits) { unit in
      runID(forUnitID: unit.unitID)
    }
    let selected = grouped.max { left, right in
      let leftActivity = left.value.compactMap { $0.lastActiveAt }.max() ?? .distantPast
      let rightActivity = right.value.compactMap { $0.lastActiveAt }.max() ?? .distantPast
      return leftActivity < rightActivity
    }
    guard let selected else { return nil }
    let selectedRunID = selected.key
    let runUnits: [HarnessUnitSnapshot] = units
      .filter { runID(forUnitID: $0.unitID) == selectedRunID }
      .sorted { $0.unitID < $1.unitID }
    let agents: [AgentOfficeAgentSnapshot] = runUnits.map { $0.agentSnapshot() }
    let updatedAt = runUnits.compactMap { $0.lastActiveAt }.max() ?? now()
    let completedCount = agents.filter { $0.lifecycle == .completed || $0.lifecycle == .postedMail }.count
    let failedCount = agents.filter { $0.lifecycle == .failed }.count
    let activeTurnCount = runUnits.filter { $0.isActive }.count
    let snapshot = AgentOfficeRunSnapshot(
      runID: selectedRunID,
      status: "running",
      sourcePath: harnessUnitDirectories().first?.path ?? "",
      updatedAt: updatedAt,
      plannedCount: agents.count,
      completedCount: completedCount,
      failedCount: failedCount,
      agents: agents
    )
    return HarnessRunSnapshot(
      snapshot: snapshot,
      updatedAt: updatedAt,
      activeTurnCount: activeTurnCount,
      settledRate: settledTokenRate(from: runUnits)
    )
  }

  private func runID(forUnitID unitID: String) -> String {
    let parts = unitID.split(separator: "-", omittingEmptySubsequences: false)
    guard let index = parts.firstIndex(where: { part in
      part.count == 2 && part.allSatisfy(\.isNumber)
    }), index > parts.startIndex else {
      return unitID
    }
    return parts[..<index].joined(separator: "-")
  }

  private func monthlyTokenUsage(
    from updates: [UpdateJSONFile],
    harnessUnits: [HarnessUnitSnapshot],
    now: Date
  ) -> TokenMeasurement {
    guard let month = calendar.dateInterval(of: .month, for: now) else {
      return .zero
    }

    var seenRunIDs: Set<String> = []
    var seenUnitIDs: Set<String> = []
    var usage = TokenMeasurement.zero
    let harnessUnitMap = Dictionary(uniqueKeysWithValues: harnessUnits.map { ($0.unitID, $0) })
    for unit in harnessUnits {
      guard
        let accountedAt = unit.accountedAt,
        month.contains(accountedAt),
        let tokenUsage = unit.tokenUsage,
        tokenUsage.tokens > 0
      else {
        continue
      }
      seenUnitIDs.insert(unit.unitID)
      usage.add(tokenUsage)
    }

    for update in updates where month.contains(update.modifiedAt) {
      guard
        let data = try? Data(contentsOf: update.url),
        let receipt = try? JSONDecoder().decode(WikiUpdateReceipt.self, from: data)
      else {
        continue
      }
      let runID = receipt.runID ?? update.runDirectory.lastPathComponent
      guard !seenRunIDs.contains(runID) else { continue }
      seenRunIDs.insert(runID)
      usage.add(runTokenUsage(
        from: receipt,
        update: update,
        harnessUnits: harnessUnitMap,
        ignoringUnitIDs: seenUnitIDs
      ))
    }
    return usage
  }

  private func runTokenUsage(
    from receipt: WikiUpdateReceipt,
    update: UpdateJSONFile,
    harnessUnits: [String: HarnessUnitSnapshot],
    ignoringUnitIDs: Set<String> = []
  ) -> TokenMeasurement {
    var usage = TokenMeasurement.zero
    for job in receipt.jobs ?? [] {
      guard !ignoringUnitIDs.contains(job.unitID) else { continue }
      if let jobUsage = jobTokenUsage(from: job, update: update, harnessUnit: harnessUnits[job.unitID]) {
        usage.add(jobUsage)
      }
    }
    if usage.tokens > 0 {
      return usage
    }
    return explicitTokenUsage(in: receipt.raw.value) ?? .zero
  }

  private func jobTokenUsage(
    from job: WikiUpdateJobReceipt,
    update: UpdateJSONFile,
    harnessUnit: HarnessUnitSnapshot?
  ) -> TokenMeasurement? {
    if let harnessUsage = harnessUnit?.tokenUsage, harnessUsage.tokens > 0 {
      return harnessUsage
    }
    if let stdoutPath = job.stdoutPath ?? harnessUnit?.stdoutPath,
      let stdoutUsage = stdoutTokenUsage(path: stdoutPath, update: update)
    {
      return stdoutUsage
    }
    if let stdoutUsage = agentSessionStdoutTokenUsage(for: job, update: update) {
      return stdoutUsage
    }
    if let explicit = explicitTokenUsage(in: job.raw.value) {
      return explicit
    }
    return nil
  }

  private func stdoutTokenUsage(path: String, update: UpdateJSONFile) -> TokenMeasurement? {
    guard let url = resolveArtifactPath(path, update: update) else {
      return nil
    }
    return stdoutTokenUsage(url: url)
  }

  private func agentSessionStdoutTokenUsage(for job: WikiUpdateJobReceipt, update: UpdateJSONFile) -> TokenMeasurement? {
    guard let url = agentSessionStdoutURL(unitID: job.unitID, update: update) else {
      return nil
    }
    return stdoutTokenUsage(url: url)
  }

  private func agentSessionStdoutURL(unitID: String, update: UpdateJSONFile) -> URL? {
    guard !unitID.isEmpty, unitID != "unknown" else { return nil }
    for directory in agentSessionDirectories(for: update) where fileManager.fileExists(atPath: directory.path) {
      let candidate = directory
        .appendingPathComponent(unitID, isDirectory: true)
        .appendingPathComponent("run.stdout.jsonl")
      if fileManager.fileExists(atPath: candidate.path) {
        return candidate
      }
    }
    return nil
  }

  private func agentSessionDirectories(for update: UpdateJSONFile) -> [URL] {
    var result = explicitAgentSessionDirectories
    if let override = environment["ONECONTEXT_AGENT_OFFICE_AGENT_SESSIONS"], !override.isEmpty {
      result.append(URL(fileURLWithPath: override, isDirectory: true))
    }
    if let memoryRoot = environment["ONECONTEXT_MEMORY_CORE_ROOT"], !memoryRoot.isEmpty {
      result.append(URL(fileURLWithPath: memoryRoot, isDirectory: true).appendingPathComponent("memory/runtime/agent-sessions", isDirectory: true))
    }
    result.append(update.updateRoot.deletingLastPathComponent().appendingPathComponent("agent-sessions", isDirectory: true))
    result.append(runtimePaths.contextEngineDirectory.appendingPathComponent("memory/runtime/agent-sessions", isDirectory: true))
    result.append(URL(fileURLWithPath: fileManager.currentDirectoryPath, isDirectory: true).appendingPathComponent("memory-core/memory/runtime/agent-sessions", isDirectory: true))
    result += memoryCoreRootCandidates().map {
      $0.appendingPathComponent("memory/runtime/agent-sessions", isDirectory: true)
    }
    return dedupeURLs(result)
  }

  private func stdoutTokenUsage(url: URL) -> TokenMeasurement? {
    let modifiedAt = modificationDate(for: url)
    let cacheKey = url.standardizedFileURL.path
    if let cached = stdoutTokenCache[cacheKey], cached.modifiedAt == modifiedAt {
      return cached.measurement
    }

    var usage = TokenMeasurement.zero
    if let contents = try? String(contentsOf: url, encoding: .utf8) {
      for line in contents.split(whereSeparator: \.isNewline) {
        guard
          let data = String(line).data(using: .utf8),
          let object = try? JSONDecoder().decode(LooseJSONObject.self, from: data),
          object.string(at: ["type"]) == "turn.completed",
          let usageObject = object.value(at: ["usage"]),
          let measurement = tokenUsageComponents(in: usageObject)
        else {
          continue
        }
        usage.add(measurement)
      }
    }

    let measurement = usage.tokens > 0 ? usage : nil
    stdoutTokenCache[cacheKey] = CachedTokenMeasurement(modifiedAt: modifiedAt, measurement: measurement)
    return measurement
  }

  private func resolveArtifactPath(_ path: String, update: UpdateJSONFile) -> URL? {
    let direct = URL(fileURLWithPath: path)
    if path.hasPrefix("/"), fileManager.fileExists(atPath: direct.path) {
      return direct
    }

    var bases: [URL] = [
      update.runDirectory,
      update.updateRoot,
      update.updateRoot.deletingLastPathComponent(),
      update.updateRoot.deletingLastPathComponent().deletingLastPathComponent(),
      update.updateRoot.deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent(),
      URL(fileURLWithPath: fileManager.currentDirectoryPath, isDirectory: true)
    ]
    bases += memoryCoreRootCandidates()

    for base in dedupeURLs(bases) {
      let candidate = base.appendingPathComponent(path)
      if fileManager.fileExists(atPath: candidate.path) {
        return candidate
      }
    }
    return nil
  }

  private func explicitTokenUsage(in value: AnyEquatable) -> TokenMeasurement? {
    let paths: [[String]] = [
      [],
      ["usage"],
      ["token_usage"],
      ["tokens"],
      ["metadata", "usage"],
      ["metrics", "usage"],
      ["evidence", "usage"],
      ["evidence", "metadata"],
      ["harness_turn_complete", "usage"],
      ["harness_turn_complete", "receipt", "evidence", "usage"],
      ["harness_turn_complete", "receipt", "evidence", "metadata"],
      ["harness_turn_complete", "unit", "metadata"]
    ]
    for path in paths {
      let candidate = path.isEmpty ? value : value.value(at: path)
      guard let candidate, let usage = tokenUsageComponents(in: candidate) else { continue }
      return usage
    }
    return nil
  }

  private func tokenUsageComponents(in value: AnyEquatable) -> TokenMeasurement? {
    let totalKeys = ["total_tokens", "totalTokens", "tokens_total", "token_total", "accounted_total_tokens"]
    for key in totalKeys {
      if let total = value.int(at: [key]), total > 0 {
        return TokenMeasurement(tokens: total)
      }
    }

    let input = value.int(at: ["input_tokens"]) ?? value.int(at: ["prompt_tokens"]) ?? 0
    let output = value.int(at: ["output_tokens"]) ?? value.int(at: ["completion_tokens"]) ?? 0
    let reasoning = value.int(at: ["reasoning_output_tokens"]) ?? value.int(at: ["reasoning_tokens"]) ?? 0
    let total = input + output + reasoning
    guard total > 0 else { return nil }
    return TokenMeasurement(tokens: total)
  }

  private func applyTokenTelemetry(
    to snapshot: inout AgentOfficeRunSnapshot,
    monthlyUsage: TokenMeasurement,
    settledRate: TokenRate?,
    activeTurnCount: Int
  ) {
    snapshot.monthlyTokenUsage = monthlyUsage.tokens
    snapshot.monthlyTokenUsageIsApproximate = false
    snapshot.tokensPerSecond = settledRate?.tokensPerSecond
    snapshot.tokensPerSecondIsApproximate = false
    snapshot.activeTurnCount = activeTurnCount
  }

  private func settledTokenRate(
    from receipt: WikiUpdateReceipt,
    update: UpdateJSONFile,
    harnessUnits: [String: HarnessUnitSnapshot]
  ) -> TokenRate? {
    var tokenTotal = 0
    var durationTotal: TimeInterval = 0
    for job in receipt.jobs ?? [] {
      let harnessUnit = harnessUnits[job.unitID]
      guard
        let usage = jobTokenUsage(from: job, update: update, harnessUnit: harnessUnit),
        usage.tokens > 0
      else {
        continue
      }
      let duration = harnessUnit?.settledDuration ?? job.settledDuration
      guard let duration, duration > 0 else { continue }
      tokenTotal += usage.tokens
      durationTotal += duration
    }
    guard tokenTotal > 0, durationTotal > 0 else { return nil }
    return TokenRate(tokensPerSecond: Double(tokenTotal) / durationTotal)
  }

  private func settledTokenRate(from units: [HarnessUnitSnapshot]) -> TokenRate? {
    let settled = units.filter {
      ($0.tokenUsage?.tokens ?? 0) > 0 && ($0.settledDuration ?? 0) > 0
    }
    guard !settled.isEmpty else { return nil }
    let tokens = settled.reduce(0) { $0 + ($1.tokenUsage?.tokens ?? 0) }
    let duration = settled.reduce(TimeInterval(0)) { $0 + ($1.settledDuration ?? 0) }
    guard tokens > 0, duration > 0 else { return nil }
    return TokenRate(tokensPerSecond: Double(tokens) / duration)
  }

  private func activeTurnCount(
    from receipt: WikiUpdateReceipt,
    harnessUnits: [String: HarnessUnitSnapshot]
  ) -> Int {
    var activeUnitIDs: Set<String> = []
    for job in receipt.jobs ?? [] {
      if harnessUnits[job.unitID]?.isActive ?? job.isActive {
        activeUnitIDs.insert(job.unitID)
      }
    }
    return activeUnitIDs.count
  }

  private func memoryCoreRootCandidates() -> [URL] {
    var candidates: [URL] = []
    if let resourceURL = Bundle.main.resourceURL {
      let marker = resourceURL.appendingPathComponent("DevMemoryCoreRoot.txt")
      if let text = try? String(contentsOf: marker, encoding: .utf8) {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        if !trimmed.isEmpty {
          candidates.append(URL(fileURLWithPath: trimmed, isDirectory: true))
        }
      }
    }
    candidates += ancestorDirectories(from: Bundle.main.executableURL)
    candidates += ancestorDirectories(from: URL(fileURLWithPath: CommandLine.arguments.first ?? fileManager.currentDirectoryPath))
    return candidates.compactMap(normalizeMemoryCoreRoot)
  }

  private func normalizeMemoryCoreRoot(_ candidate: URL) -> URL? {
    let directory = candidate.hasDirectoryPath ? candidate : candidate.deletingLastPathComponent()
    if directory.lastPathComponent == "memory-core",
      fileManager.fileExists(atPath: directory.appendingPathComponent("pyproject.toml").path)
    {
      return directory
    }
    let child = directory.appendingPathComponent("memory-core", isDirectory: true)
    if fileManager.fileExists(atPath: child.appendingPathComponent("pyproject.toml").path) {
      return child
    }
    return nil
  }

  private func ancestorDirectories(from url: URL?) -> [URL] {
    guard var cursor = url else { return [] }
    if !cursor.hasDirectoryPath {
      cursor.deleteLastPathComponent()
    }
    var result: [URL] = []
    for _ in 0..<12 {
      result.append(cursor)
      let next = cursor.deletingLastPathComponent()
      if next.path == cursor.path {
        break
      }
      cursor = next
    }
    return result
  }

  private func dedupeURLs(_ urls: [URL]) -> [URL] {
    var seen: Set<String> = []
    var result: [URL] = []
    for url in urls {
      let path = url.standardizedFileURL.path
      guard !seen.contains(path) else { continue }
      seen.insert(path)
      result.append(url)
    }
    return result
  }
}

private struct UpdateJSONFile {
  var url: URL
  var runDirectory: URL
  var updateRoot: URL
  var modifiedAt: Date
}

private struct TokenMeasurement: Equatable {
  var tokens: Int

  static let zero = TokenMeasurement(tokens: 0)

  mutating func add(_ other: TokenMeasurement) {
    tokens += other.tokens
  }
}

private struct TokenRate: Equatable {
  var tokensPerSecond: Double

  static let zero = TokenRate(tokensPerSecond: 0)
}

private struct CachedTokenMeasurement {
  var modifiedAt: Date?
  var measurement: TokenMeasurement?
}

private let activeStatusValues: Set<String> = [
  "active",
  "born",
  "in_progress",
  "queued",
  "running",
  "started",
  "working"
]

private struct HarnessRunSnapshot {
  var snapshot: AgentOfficeRunSnapshot
  var updatedAt: Date
  var activeTurnCount: Int
  var settledRate: TokenRate?
}

private struct HarnessUnitSnapshot {
  var unitID: String
  var jobID: String?
  var phase: String?
  var role: String?
  var lifecycle: AgentOfficeLifecycle = .planned
  var hasTalkReceipt = false
  var startedAt: Date?
  var completedAt: Date?
  var lastActiveAt: Date?
  var runDirectoryPath: String?
  var stdoutPath: String?
  var tokenUsage: TokenMeasurement?
  var settledDuration: TimeInterval?

  var isActive: Bool {
    lifecycle.isActive
  }

  var accountedAt: Date? {
    guard (tokenUsage?.tokens ?? 0) > 0 else { return nil }
    return completedAt ?? lastActiveAt
  }

  mutating func apply(receipt: LooseJSONObject, tokenUsage: TokenMeasurement?) {
    let kind = receipt.string(at: ["kind"]) ?? ""
    let at = receipt.string(at: ["at"]).flatMap(parseISO8601Date)
    if let at {
      lastActiveAt = max(lastActiveAt ?? at, at)
    }
    if let tokenUsage, tokenUsage.tokens > 0 {
      self.tokenUsage = tokenUsage
    }
    if settledDuration == nil {
      settledDuration = receipt.durationSeconds
    }

    jobID = jobID
      ?? receipt.string(at: ["evidence", "certificate", "identity", "job_id"])
      ?? receipt.string(at: ["evidence", "certificate", "birth_inputs", "identity", "job_id"])
      ?? receipt.string(at: ["evidence", "metadata", "metadata", "job_id"])
    phase = phase ?? receipt.string(at: ["evidence", "metadata", "metadata", "phase"])
    role = role
      ?? receipt.string(at: ["evidence", "certificate", "role"])
      ?? receipt.string(at: ["evidence", "certificate", "birth_inputs", "role"])
      ?? receipt.string(at: ["evidence", "certificate", "identity", "agent_id"])
    runDirectoryPath = runDirectoryPath
      ?? receipt.string(at: ["evidence", "certificate", "runtime", "run_dir"])
      ?? receipt.string(at: ["evidence", "certificate", "birth_inputs", "runtime", "run_dir"])
    if let runDirectoryPath, stdoutPath == nil {
      stdoutPath = URL(fileURLWithPath: runDirectoryPath, isDirectory: true)
        .appendingPathComponent("run.stdout.jsonl").path
    }

    if kind == "agent_called" {
      lifecycle = lifecycle == .planned ? .born : lifecycle
      startedAt = startedAt ?? receipt.string(at: ["evidence", "metadata", "called_at"]).flatMap(parseISO8601Date) ?? at
    } else if kind == "turn_started" {
      lifecycle = .working
      startedAt = startedAt
        ?? receipt.string(at: ["evidence", "started_at"]).flatMap(parseISO8601Date)
        ?? at
    } else if kind == "turn_completed" {
      let nextState = receipt.string(at: ["evidence", "next_state"]) ?? receipt.string(at: ["evidence", "lifecycle_state"])
      lifecycle = nextState == "failed" ? .failed : .completed
      completedAt = receipt.string(at: ["evidence", "completed_at"]).flatMap(parseISO8601Date) ?? at
      if settledDuration == nil {
        settledDuration = receipt.durationSeconds
      }
    }

    if receipt.containsString("wiki.talk.append") || receipt.containsString("talk_receipt") {
      hasTalkReceipt = true
    }
  }

  func activeElapsed(now: Date) -> TimeInterval? {
    guard isActive, let startedAt else { return nil }
    let elapsed = now.timeIntervalSince(startedAt)
    return elapsed > 0 ? elapsed : nil
  }

  func agentSnapshot() -> AgentOfficeAgentSnapshot {
    let jobID = jobID ?? unitID
    let phase = phase ?? ""
    return AgentOfficeAgentSnapshot(
      unitID: unitID,
      jobID: jobID,
      role: normalizedRole(jobID: jobID, phase: phase, role: role),
      phase: phase,
      lifecycle: lifecycle,
      hasTalkReceipt: hasTalkReceipt,
      summary: summary(for: lifecycle),
      error: ""
    )
  }

  private func summary(for lifecycle: AgentOfficeLifecycle) -> String {
    switch lifecycle {
    case .idle:
      return "idle"
    case .planned:
      return "queued"
    case .born:
      return "born"
    case .working:
      return "working"
    case .postedMail:
      return "posted"
    case .completed:
      return "done"
    case .waiting:
      return "waiting"
    case .failed:
      return "failed"
    case .retired:
      return "retired"
    }
  }
}

private struct WikiUpdateReceipt: Decodable {
  var raw: LooseJSONObject
  var runID: String?
  var status: String?
  var plannedCount: Int?
  var completedCount: Int?
  var failedCount: Int?
  var jobs: [WikiUpdateJobReceipt]?

  enum CodingKeys: String, CodingKey {
    case runID = "run_id"
    case status
    case plannedCount = "planned_count"
    case completedCount = "completed_count"
    case failedCount = "failed_count"
    case jobs
  }

  init(from decoder: Decoder) throws {
    raw = try LooseJSONObject(from: decoder)
    let container = try decoder.container(keyedBy: CodingKeys.self)
    runID = try container.decodeIfPresent(String.self, forKey: .runID)
    status = try container.decodeIfPresent(String.self, forKey: .status)
    plannedCount = try container.decodeIfPresent(Int.self, forKey: .plannedCount)
    completedCount = try container.decodeIfPresent(Int.self, forKey: .completedCount)
    failedCount = try container.decodeIfPresent(Int.self, forKey: .failedCount)
    jobs = try container.decodeIfPresent([WikiUpdateJobReceipt].self, forKey: .jobs)
  }

  func snapshot(sourcePath: String, updatedAt: Date?, harnessUnits: [String: HarnessUnitSnapshot] = [:]) -> AgentOfficeRunSnapshot {
    let normalizedJobs = (jobs ?? []).map { $0.agentSnapshot(harnessUnit: harnessUnits[$0.unitID]) }
    return AgentOfficeRunSnapshot(
      runID: runID ?? "",
      status: status ?? "unknown",
      sourcePath: sourcePath,
      updatedAt: updatedAt,
      plannedCount: plannedCount ?? normalizedJobs.count,
      completedCount: completedCount ?? normalizedJobs.filter { $0.lifecycle == .completed || $0.lifecycle == .postedMail }.count,
      failedCount: failedCount ?? normalizedJobs.filter { $0.lifecycle == .failed }.count,
      agents: normalizedJobs
    )
  }

  var isActive: Bool {
    if let status, activeStatusValues.contains(status.lowercased()) {
      return true
    }
    return (jobs ?? []).contains { $0.isActive }
  }

  func activeElapsed(now: Date) -> TimeInterval? {
    let startedAt = (jobs ?? []).compactMap { $0.activeStartedAt }.min()
    guard let startedAt else { return nil }
    let elapsed = now.timeIntervalSince(startedAt)
    return elapsed > 0 ? elapsed : nil
  }
}

private struct WikiUpdateJobReceipt: Decodable {
  var raw: LooseJSONObject
  var phase: String?
  var jobID: String?
  var runID: String?
  var status: String?
  var error: String?
  var durationMS: Int?
  var stdoutPath: String?
  var stderrPath: String?
  var agentReportPath: String?
  var harnessCall: LooseJSONObject?
  var harnessTurnStart: LooseJSONObject?
  var harnessTurnComplete: LooseJSONObject?
  var harnessAdapterEvents: [LooseJSONObject]?
  var talkReceipt: LooseJSONObject?

  enum CodingKeys: String, CodingKey {
    case phase
    case jobID = "job_id"
    case runID = "run_id"
    case status
    case error
    case durationMS = "duration_ms"
    case stdoutPath = "stdout_path"
    case stderrPath = "stderr_path"
    case agentReportPath = "agent_report_path"
    case harnessCall = "harness_call"
    case harnessTurnStart = "harness_turn_start"
    case harnessTurnComplete = "harness_turn_complete"
    case harnessAdapterEvents = "harness_adapter_events"
    case talkReceipt = "talk_receipt"
  }

  init(from decoder: Decoder) throws {
    raw = try LooseJSONObject(from: decoder)
    let container = try decoder.container(keyedBy: CodingKeys.self)
    phase = try container.decodeIfPresent(String.self, forKey: .phase)
    jobID = try container.decodeIfPresent(String.self, forKey: .jobID)
    runID = try container.decodeIfPresent(String.self, forKey: .runID)
    status = try container.decodeIfPresent(String.self, forKey: .status)
    error = try container.decodeIfPresent(String.self, forKey: .error)
    durationMS = try container.decodeIfPresent(Int.self, forKey: .durationMS)
    stdoutPath = try container.decodeIfPresent(String.self, forKey: .stdoutPath)
    stderrPath = try container.decodeIfPresent(String.self, forKey: .stderrPath)
    agentReportPath = try container.decodeIfPresent(String.self, forKey: .agentReportPath)
    harnessCall = try container.decodeIfPresent(LooseJSONObject.self, forKey: .harnessCall)
    harnessTurnStart = try container.decodeIfPresent(LooseJSONObject.self, forKey: .harnessTurnStart)
    harnessTurnComplete = try container.decodeIfPresent(LooseJSONObject.self, forKey: .harnessTurnComplete)
    harnessAdapterEvents = try container.decodeIfPresent([LooseJSONObject].self, forKey: .harnessAdapterEvents)
    talkReceipt = try container.decodeIfPresent(LooseJSONObject.self, forKey: .talkReceipt)
  }

  var unitID: String {
    runID ?? jobID ?? "unknown"
  }

  func agentSnapshot(harnessUnit: HarnessUnitSnapshot? = nil) -> AgentOfficeAgentSnapshot {
    let jobID = self.jobID ?? harnessUnit?.jobID ?? "unknown"
    let phase = self.phase ?? harnessUnit?.phase ?? ""
    let lifecycle = harnessUnit?.lifecycle ?? normalizedLifecycle()
    return AgentOfficeAgentSnapshot(
      unitID: unitID,
      jobID: jobID,
      role: normalizedRole(jobID: jobID, phase: phase, role: harnessUnit?.role),
      phase: phase,
      lifecycle: lifecycle,
      hasTalkReceipt: talkReceipt != nil || harnessUnit?.hasTalkReceipt == true,
      summary: summary(for: lifecycle),
      error: error ?? ""
    )
  }

  private func normalizedLifecycle() -> AgentOfficeLifecycle {
    if status == "failed" {
      return .failed
    }
    if let complete = harnessTurnComplete {
      let outcome = complete.string(at: ["receipt", "evidence", "outcome"])
        ?? complete.string(at: ["evidence", "outcome"])
      if status == "completed", talkReceipt != nil {
        return .postedMail
      }
      if status == "completed" || outcome == "done" {
        return .completed
      }
      if outcome == "waiting" {
        return .waiting
      }
    }
    if status == "completed" {
      return talkReceipt == nil ? .completed : .postedMail
    }
    if status == "planned" {
      return .planned
    }
    let adapterKinds = harnessAdapterEvents?.compactMap { $0.string(at: ["kind"]) } ?? []
    if adapterKinds.contains("runtime_wakeup_accepted") {
      return talkReceipt == nil ? .working : .postedMail
    }
    if harnessTurnStart != nil {
      return .working
    }
    if harnessCall != nil {
      return .born
    }
    return .planned
  }

  private func summary(for lifecycle: AgentOfficeLifecycle) -> String {
    switch lifecycle {
    case .idle:
      return "idle"
    case .planned:
      return "queued"
    case .born:
      return "born"
    case .working:
      return "working"
    case .postedMail:
      return "posted"
    case .completed:
      return "done"
    case .waiting:
      return "waiting"
    case .failed:
      return "failed"
    case .retired:
      return "retired"
    }
  }

  var isActive: Bool {
    normalizedLifecycle().isActive || activeStatusValues.contains((status ?? "").lowercased())
  }

  var activeStartedAt: Date? {
    guard isActive else { return nil }
    let candidates = [
      harnessTurnStart?.string(at: ["evidence", "started_at"]),
      harnessTurnStart?.string(at: ["started_at"]),
      harnessTurnStart?.string(at: ["at"]),
      harnessTurnStart?.string(at: ["receipt", "at"]),
      harnessCall?.string(at: ["issued_at"]),
      harnessCall?.string(at: ["at"])
    ]
    return candidates.compactMap { $0.flatMap(parseISO8601Date) }.min()
  }

  var settledDuration: TimeInterval? {
    if let durationMS, durationMS > 0 {
      return Double(durationMS) / 1_000
    }
    if let complete = harnessTurnComplete?.durationSeconds {
      return complete
    }
    return raw.durationSeconds
  }

  func activeElapsed(now: Date) -> TimeInterval? {
    guard let startedAt = activeStartedAt else { return nil }
    let elapsed = now.timeIntervalSince(startedAt)
    return elapsed > 0 ? elapsed : nil
  }
}

private struct LooseJSONObject: Decodable, Equatable {
  var value: AnyEquatable

  init(from decoder: Decoder) throws {
    value = try AnyEquatable(from: decoder)
  }

  func value(at path: [String]) -> AnyEquatable? {
    value.value(at: path)
  }

  func string(at path: [String]) -> String? {
    value.string(at: path)
  }

  func containsString(_ needle: String) -> Bool {
    value.containsString(needle)
  }

  var durationSeconds: TimeInterval? {
    value.durationSeconds
  }
}

private enum AnyEquatable: Decodable, Equatable {
  case string(String)
  case number(Double)
  case bool(Bool)
  case object([String: AnyEquatable])
  case array([AnyEquatable])
  case null

  init(from decoder: Decoder) throws {
    let container = try decoder.singleValueContainer()
    if container.decodeNil() {
      self = .null
    } else if let value = try? container.decode(Bool.self) {
      self = .bool(value)
    } else if let value = try? container.decode(Double.self) {
      self = .number(value)
    } else if let value = try? container.decode(String.self) {
      self = .string(value)
    } else if let value = try? container.decode([String: AnyEquatable].self) {
      self = .object(value)
    } else {
      self = .array(try container.decode([AnyEquatable].self))
    }
  }

  func value(at path: [String]) -> AnyEquatable? {
    guard let key = path.first else {
      return self
    }
    guard case .object(let object) = self, let child = object[key] else {
      return nil
    }
    return child.value(at: Array(path.dropFirst()))
  }

  func string(at path: [String]) -> String? {
    guard let value = value(at: path) else {
      return nil
    }
    if case .string(let string) = value {
      return string
    }
    return nil
  }

  func double(at path: [String]) -> Double? {
    guard let value = value(at: path) else {
      return nil
    }
    switch value {
    case .number(let number):
      return number
    case .string(let string):
      return Double(string)
    case .bool(let bool):
      return bool ? 1 : 0
    case .object, .array, .null:
      return nil
    }
  }

  func int(at path: [String]) -> Int? {
    guard let double = double(at: path), double.isFinite else {
      return nil
    }
    return Int(double.rounded())
  }

  func containsString(_ needle: String) -> Bool {
    switch self {
    case .string(let string):
      return string.contains(needle)
    case .number, .bool, .null:
      return false
    case .object(let object):
      return object.values.contains { $0.containsString(needle) }
    case .array(let array):
      return array.contains { $0.containsString(needle) }
    }
  }

  var durationSeconds: TimeInterval? {
    let millisecondPaths: [[String]] = [
      ["duration_ms"],
      ["usage", "duration_ms"],
      ["evidence", "duration_ms"],
      ["evidence", "usage", "duration_ms"],
      ["metadata", "total_duration_ms"],
      ["evidence", "metadata", "total_duration_ms"]
    ]
    for path in millisecondPaths {
      if let milliseconds = double(at: path), milliseconds > 0 {
        return milliseconds / 1_000
      }
    }

    let secondPaths: [[String]] = [
      ["duration_seconds"],
      ["duration_s"],
      ["usage", "duration_seconds"],
      ["evidence", "duration_seconds"],
      ["evidence", "usage", "duration_seconds"]
    ]
    for path in secondPaths {
      if let seconds = double(at: path), seconds > 0 {
        return seconds
      }
    }
    return nil
  }
}

private func parseISO8601Date(_ value: String) -> Date? {
  let fractionalFormatter = ISO8601DateFormatter()
  fractionalFormatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
  if let date = fractionalFormatter.date(from: value) {
    return date
  }
  let formatter = ISO8601DateFormatter()
  formatter.formatOptions = [.withInternetDateTime]
  return formatter.date(from: value)
}

private func normalizedRole(jobID: String, phase: String, role: String? = nil) -> String {
  let id = jobID.lowercased()
  let phase = phase.lowercased()
  let role = (role ?? "").lowercased()
  if id.contains("aggregate_scribe") || phase.contains("aggregate") {
    return "hourly_aggregate"
  }
  if id.contains("scribe") || role.contains("scribe") {
    return "scribe"
  }
  if id.contains("daily.editor") || id.contains("daily_editor") || role.contains("daily") {
    return "daily_editor"
  }
  if id.contains("biographer") || role.contains("biographer") {
    return "biographer"
  }
  if id.contains("context_curator") || role.contains("context") {
    return "context_curator"
  }
  if id.contains("for_you_curator") || id.contains("curator") || role.contains("curator") {
    return "curator"
  }
  if id.contains("librarian") || id.contains("source") || role.contains("librarian") || role.contains("source") {
    return "librarian"
  }
  if id.contains("historian") || id.contains("concept") || role.contains("historian") {
    return "historian"
  }
  if id.contains("contradiction") || role.contains("contradiction") {
    return "contradiction"
  }
  if id.contains("publisher") || id.contains("redactor") || phase.contains("promotion") || role.contains("redactor") {
    return "publisher"
  }
  return "curator"
}
