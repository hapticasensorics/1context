import Foundation
import Darwin
import OneContextAgentRuntime
import OneContextCapture
import OneContextCore
import OneContextLocalWeb
import OneContextMemoryRuntime
import OneContextPlatform
import OneContextProtocol
import OneContextSetup
import OneContextSupervisor
import OneContextWikiRuntime

nonisolated(unsafe) private var signalSocketPath: UnsafeMutablePointer<CChar>?
nonisolated(unsafe) private var signalPIDPath: UnsafeMutablePointer<CChar>?
nonisolated(unsafe) private var signalLogPath: UnsafeMutablePointer<CChar>?

private let daemonLogMaxBytes: UInt64 = 1_048_576
private let cacheMaxBytes: UInt64 = 50 * 1024 * 1024
private let cacheMaxAge: TimeInterval = 7 * 24 * 60 * 60
private let maxActiveClients = 32
private let requestDeadlineSeconds: TimeInterval = 2
private let uxEventPersistenceInterval: TimeInterval = 2.0
private let continuousMetadataSamplerCooldownSeconds: TimeInterval = 5.0
private let continuousMetadataSamplerDurationSeconds: TimeInterval = 0.75
private let continuousMetadataSamplerMaxFrames = 6
private let memoryWikiUpdateInitialDelay: TimeInterval = 10 * 60
private let memoryWikiUpdateInterval: TimeInterval = 12 * 60 * 60
private let memoryWikiUpdateCursorName = "wiki_agent_sessions_v1"
private let memoryWikiBackfillImportTicks = 4
private let memoryWikiBackfillWindowDays = 30
private let memoryWikiBackfillMaxEvents = 5_000
private let memoryWikiBackfillMaxLines = 250_000
private let memoryWikiBackfillQueryLimit = 2_400
private let memoryWikiIncrementalImportTicks = 1
private let memoryWikiIncrementalWindowDays = 7
private let memoryWikiIncrementalMaxEvents = 1_500
private let memoryWikiIncrementalMaxLines = 100_000
private let memoryWikiIncrementalQueryLimit = 1_200
private let memoryWikiUpdateTimeoutSeconds = 240

private enum CaptureDaemonError: LocalizedError {
  case snapshotTimedOut
  case snapshotUnavailable
  case metadataSampleTimedOut
  case metadataSampleUnavailable

  var errorDescription: String? {
    switch self {
    case .snapshotTimedOut:
      return "Timed out while building capture snapshot"
    case .snapshotUnavailable:
      return "Capture snapshot did not return a result"
    case .metadataSampleTimedOut:
      return "Timed out while sampling active-window metadata"
    case .metadataSampleUnavailable:
      return "Active-window metadata sample did not return a result"
    }
  }
}

private struct CaptureUXRuntimeSnapshot {
  var status: OneContextUXEventTapStatus
  var motionHints: UXMotionHints
  var startupWired: Bool
  var metadataSampleFusionEnabled: Bool
  var startupError: String?
}

private final class ContinuousActiveWindowMetadataSampler: @unchecked Sendable {
  typealias SampleOperation = @Sendable (TimeInterval, Int) async throws -> ActiveWindowMetadataSample

  private let queue = DispatchQueue(label: "com.haptica.1contextd.continuous-metadata-sampler")
  private let cooldownSeconds: TimeInterval
  private let durationSeconds: TimeInterval
  private let maxFrames: Int
  private let sample: SampleOperation
  private let log: @Sendable (String) -> Void

  private var running = false
  private var triggerCount = 0
  private var sampleCount = 0
  private var persistedFrameCount = 0
  private var skippedCount = 0
  private var lastTriggerAt: String?
  private var lastSampleAt: String?
  private var lastError: String?
  private var lastCompletedAt: Date?

  init(
    cooldownSeconds: TimeInterval = continuousMetadataSamplerCooldownSeconds,
    durationSeconds: TimeInterval = continuousMetadataSamplerDurationSeconds,
    maxFrames: Int = continuousMetadataSamplerMaxFrames,
    sample: @escaping SampleOperation,
    log: @escaping @Sendable (String) -> Void
  ) {
    self.cooldownSeconds = cooldownSeconds
    self.durationSeconds = min(1.0, max(0.1, durationSeconds))
    self.maxFrames = max(1, maxFrames)
    self.sample = sample
    self.log = log
  }

  func trigger(afterPersisting anchors: [UXEventAnchor]) {
    guard anchors.contains(where: { Self.shouldTrigger(from: $0.kind) }) else { return }
    let now = Date()
    let bypassCooldown = anchors.contains(where: { $0.kind == .focusTransition })
    queue.async { [weak self] in
      self?.triggerLocked(now: now, bypassCooldown: bypassCooldown)
    }
  }

  func statusPayload() -> [String: Any] {
    queue.sync {
      [
        "enabled": true,
        "running": running,
        "trigger_count": triggerCount,
        "sample_count": sampleCount,
        "persisted_frame_count": persistedFrameCount,
        "skipped_count": skippedCount,
        "last_trigger_at": (lastTriggerAt ?? NSNull()) as Any,
        "last_sample_at": (lastSampleAt ?? NSNull()) as Any,
        "last_error": (lastError ?? NSNull()) as Any,
        "cooldown_seconds": cooldownSeconds,
        "duration_seconds": durationSeconds,
        "max_frames": maxFrames
      ]
    }
  }

  private func triggerLocked(now: Date, bypassCooldown: Bool) {
    triggerCount += 1
    lastTriggerAt = SCStreamFrameMetadataParser.isoTimestamp(now)

    if running {
      skippedCount += 1
      return
    }

    if !bypassCooldown,
      let lastCompletedAt,
      now.timeIntervalSince(lastCompletedAt) < cooldownSeconds
    {
      skippedCount += 1
      return
    }

    running = true
    sampleCount += 1
    let sampleID = sampleCount
    let startedAt = SCStreamFrameMetadataParser.isoTimestamp(now)
    lastSampleAt = startedAt
    let durationSeconds = self.durationSeconds
    let maxFrames = self.maxFrames
    let sample = sample

    Task.detached(priority: .utility) { [weak self, sample] in
      do {
        let result = try await sample(durationSeconds, maxFrames)
        self?.finishSample(
          sampleID: sampleID,
          persistedEventCount: result.persistedEventCount,
          persistErrors: result.persistErrors
        )
      } catch {
        self?.finishSample(sampleID: sampleID, error: error)
      }
    }
  }

  private func finishSample(sampleID: Int, persistedEventCount: Int, persistErrors: [String]) {
    let completedAt = Date()
    queue.async { [weak self] in
      guard let self else { return }
      running = false
      lastCompletedAt = completedAt
      persistedFrameCount += persistedEventCount
      lastError = persistErrors.first
      log("continuous metadata sample completed id=\(sampleID) persisted_frames=\(persistedEventCount)")
    }
  }

  private func finishSample(sampleID: Int, error: Error) {
    let completedAt = Date()
    queue.async { [weak self] in
      guard let self else { return }
      running = false
      lastCompletedAt = completedAt
      lastError = error.localizedDescription
      log("continuous metadata sample failed id=\(sampleID): \(error.localizedDescription)")
    }
  }

  private static func shouldTrigger(from kind: UXEventAnchorKind) -> Bool {
    switch kind {
    case .scrollBurst, .pointer, .keyboardActivity, .shortcut, .focusTransition:
      return true
    case .modifiers:
      return false
    }
  }
}

final class Logger: @unchecked Sendable {
  private let path: String
  private let queue = DispatchQueue(label: "com.haptica.1contextd.logger")

  init(path: String) {
    self.path = path
  }

  func write(_ message: String) {
    queue.sync {
      rotateIfNeeded()
      let timestamp = ISO8601DateFormatter().string(from: Date())
      let line = "[\(timestamp)] \(message)\n"
      guard let data = line.data(using: .utf8) else { return }

      if FileManager.default.fileExists(atPath: path),
        let handle = try? FileHandle(forWritingTo: URL(fileURLWithPath: path))
      {
        defer {
          try? handle.close()
          RuntimePermissions.ensurePrivateFile(path)
        }
        _ = try? handle.seekToEnd()
        try? handle.write(contentsOf: data)
      } else {
        try? RuntimePermissions.writePrivateData(data, to: URL(fileURLWithPath: path))
      }
    }
  }

  private func rotateIfNeeded() {
    let fileManager = FileManager.default
    guard let attributes = try? fileManager.attributesOfItem(atPath: path),
      let size = attributes[.size] as? NSNumber,
      size.uint64Value >= daemonLogMaxBytes
    else {
      return
    }

    let current = URL(fileURLWithPath: path)
    let rotated = URL(fileURLWithPath: path + ".1")
    try? fileManager.removeItem(at: rotated)
    try? fileManager.moveItem(at: current, to: rotated)
    RuntimePermissions.ensurePrivateFile(rotated.path)
  }
}

final class OneContextDaemon: @unchecked Sendable {
  private let paths = RuntimePaths.current()
  private let startedAt = Date()
  private let acceptQueue = DispatchQueue(label: "com.haptica.1contextd.accept")
  private let clientQueue = DispatchQueue(label: "com.haptica.1contextd.clients", attributes: .concurrent)
  private let wikiPublicationQueue = DispatchQueue(label: "com.haptica.1contextd.wiki-publication")
  private let memoryWikiUpdateQueue = DispatchQueue(label: "com.haptica.1contextd.memory-wiki-update")
  private let memoryWikiUpdateStateLock = NSLock()
  private let activeClients = DispatchSemaphore(value: maxActiveClients)
  private var listenFD: Int32 = -1
  private lazy var logger = Logger(path: paths.logPath)
  private lazy var localWeb = CaddyManager(runtimePaths: paths)
  private lazy var memoryDaemon = MemoryDaemonProcessClient(runtimePaths: paths)
  private lazy var memoryCore = MemoryCoreProcessClient(runtimePaths: paths)
  private lazy var captureLogStore = OneContextCaptureLogStore(runtimePaths: paths)
  private lazy var axSemanticEventAggregator = AXSemanticEventAggregator(capacity: 256)
  private let axSemanticPersistenceQueue = DispatchQueue(label: "com.haptica.1contextd.ax-semantic-persistence")
  private var axSemanticPersistedEventCount = 0
  private var axSemanticLastPersistedAt: String?
  private var axSemanticLastPersistenceError: String?
  private lazy var uxEventTap = OneContextUXEventTap(
    queueCapacity: 2_048,
    owner: daemonUXEventTapOwner(),
    startupWired: true
  )
  private let uxEventPersistenceQueue = DispatchQueue(label: "com.haptica.1contextd.ux-event-persistence")
  private var uxEventPersistenceTimer: DispatchSourceTimer?
  private var uxEventPersistedAnchorCount = 0
  private var uxEventLastPersistedAt: String?
  private var uxEventLastPersistenceFlushAt: String?
  private var uxEventPersistenceError: String?
  private var memoryWikiUpdateTimer: DispatchSourceTimer?
  private var memoryWikiUpdateInFlight = false
  private var memoryWikiUpdateCompletedCount = 0
  private var memoryWikiUpdateSkippedCount = 0
  private var memoryWikiUpdateLastTrigger: String?
  private var memoryWikiUpdateLastStartedAt: Date?
  private var memoryWikiUpdateLastCompletedAt: Date?
  private var memoryWikiUpdateLastStatus: String?
  private var memoryWikiUpdateLastError: String?
  private var uxEventTapStartupWired = false
  private var uxEventTapStartupError: String?
  private lazy var continuousMetadataSampler = ContinuousActiveWindowMetadataSampler(
    sample: { [weak self] durationSeconds, maxFrames in
      guard let self else { throw CaptureDaemonError.metadataSampleUnavailable }
      return try await self.continuousActiveWindowMetadataSample(
        durationSeconds: durationSeconds,
        maxFrames: maxFrames
      )
    },
    log: { [weak self] message in
      self?.logger.write(message)
    }
  )
  private lazy var wikiCore = WikiCoreProcessClient(runtimePaths: paths)
  private lazy var wikiRendererConfig = WikiEngineRendererConfig.discover()
  private lazy var wikiCoreRPC = WikiCoreRPCBridge(client: wikiCore, rendererConfig: wikiRendererConfig)
  private lazy var agentHarness = AgentHarnessProcessClient(runtimePaths: paths)
  private lazy var agentHarnessRPC = AgentHarnessRPCBridge(client: agentHarness)
  private lazy var wikiRenderCoordinator = WikiRenderCoordinator(
    runtimePaths: paths,
    rendererConfig: wikiRendererConfig,
    log: { [weak self] message in
      self?.logger.write(message)
    }
  )
  private lazy var wikiAPI = WikiLocalAPIServer(
    config: WikiLocalAPIConfig(),
    handler: WikiLocalAPIHandler(paths: LocalWebPaths(runtimePaths: paths), renderState: { [weak self] in
      self?.wikiRenderState ?? "idle"
    }, memoryStatus: { [weak self] in
      self?.memoryDaemon.status().payload ?? ["status": "unavailable"]
    }, memoryViewport: { [weak self] query in
      guard let self else {
        throw MemoryProtocolClientError("memory daemon unavailable")
      }
      let limit = query["limit"].flatMap(Int.init) ?? 200
      return try self.memoryDaemon.queryViewport(MemoryViewportQuery(
        limit: limit,
        source: query["source"],
        startTime: query["start_time"] ?? query["start"],
        endTime: query["end_time"] ?? query["end"]
      ))
    }, memoryObject: { [weak self] query in
      guard let self else {
        throw MemoryProtocolClientError("memory daemon unavailable")
      }
      let objectIDs = Self.csvValues(query["object_ids"] ?? query["object_id"] ?? query["id"])
      return try self.memoryDaemon.hydrateObjects(MemoryObjectHydrationQuery(objectIDs: objectIDs))
    }, memoryDensity: { [weak self] query in
      guard let self else {
        throw MemoryProtocolClientError("memory daemon unavailable")
      }
      let sources = Self.csvValues(query["sources"] ?? query["source"])
      return try self.memoryDaemon.queryDensity(MemoryDensityQuery(
        startTime: query["start_time"] ?? query["start"],
        endTime: query["end_time"] ?? query["end"],
        bucket: query["bucket"] ?? "1m",
        sources: sources
      ))
    }, memoryEdges: { [weak self] query in
      guard let self else {
        throw MemoryProtocolClientError("memory daemon unavailable")
      }
      let objectID = query["object_id"] ?? query["id"] ?? ""
      return try self.memoryDaemon.queryEdges(MemoryEdgesQuery(
        objectID: objectID,
        direction: query["direction"] ?? "both",
        edgeKind: query["edge_kind"],
        limit: query["limit"].flatMap(Int.init) ?? 200,
        includeObjectSummaries: query["include_object_summaries"] != "false"
      ))
    }, memorySearch: { [weak self] query in
      guard let self else {
        throw MemoryProtocolClientError("memory daemon unavailable")
      }
      return try self.memoryDaemon.searchText(MemorySearchTextQuery(
        query: query["query"] ?? query["q"] ?? "",
        limit: query["limit"].flatMap(Int.init) ?? 50,
        source: query["source"]
      ))
    })
  )
  private lazy var wikiRenderQueue = WikiRenderQueue(
    debounceInterval: 0.5,
    failureBackoffInterval: 5,
    automaticMinimumInterval: { [weak self] in
      guard let self else { return WikiAutomaticPublishCadence.defaultValue.minimumAutomaticInterval }
      return OneContextAppSettings
        .wikiAutomaticPublishCadence(preferencesPath: self.paths.preferencesPath)
        .minimumAutomaticInterval
    },
    render: { [weak self] request in
      self?.performWikiRender(request) ?? WikiRenderQueueOutcome(
        status: .failed,
        dirtyPages: 0,
        rendererDurationMilliseconds: 0,
        error: "daemon unavailable"
      )
    }
  )

  private static func csvValues(_ value: String?) -> [String] {
    value?
      .split(separator: ",")
      .map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
      .filter { !$0.isEmpty && $0 != "all" } ?? []
  }

  func run() throws {
    umask(0o077)
    signal(SIGPIPE, SIG_IGN)
    try prepareDirectories()
    startPersistentUXEventTap()
    startUXEventPersistenceTimer()
    try startSocket()
    try writePIDFile()
    installSignalHandlers()
    logger.write("1Context runtime started pid=\(getpid()) socket=\(paths.socketPath)")
    repairMenuLaunchAgentInBackground()
    startMemoryDaemon()
    startWikiAPI()
    publishWikiInBackground(refresh: false)
    startAutomaticMemoryWikiUpdateTimer()
    acceptQueue.async { [self] in
      acceptLoop()
    }
    RunLoop.main.run()
    cleanup()
  }

  private func repairMenuLaunchAgentInBackground() {
    guard let menuAppPath = menuExecutablePath() else {
      logger.write("menu launch agent repair skipped: bundled menu executable not found")
      return
    }

    Task.detached(priority: .utility) { [logger] in
      do {
        try await LaunchAgentManager().startMenu(appPath: menuAppPath)
        logger.write("menu launch agent ready path=\(menuAppPath)")
      } catch {
        logger.write("menu launch agent repair failed: \(error.localizedDescription)")
      }
    }
  }

  private func menuExecutablePath() -> String? {
    let fileManager = FileManager.default
    let executableDirectory = URL(fileURLWithPath: CommandLine.arguments[0])
      .resolvingSymlinksInPath()
      .deletingLastPathComponent()
    let candidates = [
      executableDirectory.appendingPathComponent("1Context").path,
      paths.identity.appBundleURL.appendingPathComponent("Contents/MacOS/1Context").path
    ]
    return candidates.first { fileManager.isExecutableFile(atPath: $0) }
  }

  private func prepareDirectories() throws {
    try RuntimePermissions.ensurePrivateDirectory(paths.userContentDirectory)
    try RuntimePermissions.ensurePrivateDirectory(paths.userWikiDirectory)
    try RuntimePermissions.ensurePrivateDirectory(paths.userWikiSourceDirectory)
    try RuntimePermissions.ensurePrivateDirectory(paths.userWikiSiteDirectory)
    try RuntimePermissions.ensurePrivateDirectory(paths.contextEngineDirectory)
    try RuntimePermissions.ensurePrivateDirectory(paths.contextEngineIndexesDirectory)
    try RuntimePermissions.ensurePrivateDirectory(paths.appSupportDirectory)
    try RuntimePermissions.ensurePrivateDirectory(paths.appSupportIndexesDirectory)
    try RuntimePermissions.ensurePrivateDirectory(paths.appSupportSetupDirectory)
    try RuntimePermissions.ensurePrivateDirectory(paths.runDirectory)
    try RuntimePermissions.ensurePrivateDirectory(paths.logDirectory)
    try RuntimePermissions.ensurePrivateDirectory(paths.cacheDirectory)
    try RuntimePermissions.ensurePrivateDirectory(paths.renderCacheDirectory)
    try RuntimePermissions.ensurePrivateDirectory(paths.downloadCacheDirectory)
    try OneContextCapturePaths(runtimePaths: paths).ensureDirectories()
    _ = try WikiRuntimeDefaultsInstaller(runtimePaths: paths).installMissingDefaults()
    RuntimePermissions.repairRuntimePaths(paths)
    pruneCaches()
  }

  private func writePIDFile() throws {
    try RuntimePermissions.writePrivateString("\(getpid())\n", toFile: paths.pidPath)
  }

  private func startSocket() throws {
    if FileManager.default.fileExists(atPath: paths.socketPath) {
      let attributes = try? FileManager.default.attributesOfItem(atPath: paths.socketPath)
      if attributes?[.type] as? FileAttributeType == .typeSocket {
        if isSocketAcceptingConnections(paths.socketPath) {
          throw UnixSocketError.socketPathExists(paths.socketPath)
        }
        unlink(paths.socketPath)
      } else {
        throw UnixSocketError.socketPathExists(paths.socketPath)
      }
    }

    listenFD = socket(AF_UNIX, SOCK_STREAM, 0)
    guard listenFD >= 0 else { throw UnixSocketError.socketFailed }
    setNoSigPipe(listenFD)

    let bindResult = try withUnixSocketAddress(path: paths.socketPath) { pointer, length in
      Darwin.bind(listenFD, pointer, length)
    }
    guard bindResult == 0 else {
      close(listenFD)
      throw UnixSocketError.connectFailed(paths.socketPath)
    }
    chmod(paths.socketPath, 0o600)

    guard listen(listenFD, 16) == 0 else {
      close(listenFD)
      throw UnixSocketError.socketFailed
    }
  }

  private func installSignalHandlers() {
    signalSocketPath = strdup(paths.socketPath)
    signalPIDPath = strdup(paths.pidPath)
    signalLogPath = strdup(paths.logPath)
    signal(SIGTERM) { _ in
      writeSignalLog("1Context runtime stopping signal=SIGTERM\n")
      if let socketPath = signalSocketPath {
        unlink(socketPath)
      }
      if let pidPath = signalPIDPath {
        unlink(pidPath)
      }
      _exit(0)
    }
    signal(SIGINT) { _ in
      writeSignalLog("1Context runtime stopping signal=SIGINT\n")
      if let socketPath = signalSocketPath {
        unlink(socketPath)
      }
      if let pidPath = signalPIDPath {
        unlink(pidPath)
      }
      _exit(0)
    }
  }

  private func acceptLoop() {
    while listenFD >= 0 {
      let clientFD = accept(listenFD, nil, nil)
      if clientFD < 0 { continue }
      guard activeClients.wait(timeout: .now()) == .success else {
        close(clientFD)
        continue
      }
      clientQueue.async { [self] in
        defer {
          close(clientFD)
          activeClients.signal()
        }
        autoreleasepool {
          handle(clientFD: clientFD)
        }
      }
    }
  }

  private func handle(clientFD: Int32) {
    setNoSigPipe(clientFD)
    guard let request = readLine(from: clientFD) else { return }
    let response = responseData(for: request)
    _ = writeAll(response, to: clientFD)
  }

  private func writeAll(_ data: Data, to fd: Int32) -> Bool {
    data.withUnsafeBytes { rawBuffer in
      guard let baseAddress = rawBuffer.baseAddress else { return false }
      var sent = 0
      while sent < data.count {
        let count = write(fd, baseAddress.advanced(by: sent), data.count - sent)
        if count > 0 {
          sent += count
        } else if count < 0 && errno == EINTR {
          continue
        } else {
          return false
        }
      }
      return true
    }
  }

  private func isSocketAcceptingConnections(_ path: String) -> Bool {
    let fd = socket(AF_UNIX, SOCK_STREAM, 0)
    guard fd >= 0 else { return false }
    defer { close(fd) }
    setNoSigPipe(fd)
    let result = try? withUnixSocketAddress(path: path) { pointer, length in
      connect(fd, pointer, length)
    }
    return result == 0
  }

  private func setNoSigPipe(_ fd: Int32) {
    var enabled: Int32 = 1
    setsockopt(fd, SOL_SOCKET, SO_NOSIGPIPE, &enabled, socklen_t(MemoryLayout<Int32>.size))
  }

  private func readLine(from fd: Int32) -> Data? {
    var data = Data()
    var byte: UInt8 = 0
    let deadline = Date().addingTimeInterval(requestDeadlineSeconds)

    while true {
      let remaining = deadline.timeIntervalSinceNow
      guard remaining > 0 else { return nil }
      let timeoutMs = max(1, min(2_000, Int32(remaining * 1_000)))
      var pollFD = pollfd(fd: fd, events: Int16(POLLIN), revents: 0)
      guard poll(&pollFD, 1, timeoutMs) > 0 else { return nil }
      guard read(fd, &byte, 1) == 1 else { break }
      if byte == UInt8(ascii: "\n") { break }
      data.append(byte)
      if data.count > 64 * 1024 { return nil }
    }

    return data.isEmpty ? nil : data
  }

  private func responseData(for requestData: Data) -> Data {
    let id: Any
    let method: String
    let params: [String: Any]

    do {
      let object = try JSONSerialization.jsonObject(with: requestData)
      guard let request = object as? [String: Any],
        let requestMethod = request["method"] as? String
      else {
        return encode(error: "Invalid request", id: NSNull())
      }

      id = request["id"] ?? NSNull()
      method = requestMethod
      params = request["params"] as? [String: Any] ?? [:]
    } catch {
      return encode(error: "Invalid JSON", id: NSNull())
    }

    switch method {
    case "health", "status":
      return encode(result: healthPayload(), id: id)
    case "version":
      return encode(result: ["version": oneContextVersion], id: id)
    case "capture.status":
      return encode(result: captureStatusPayload(), id: id)
    case "capture.snapshot":
      do {
        return encode(result: try captureSnapshotPayload(), id: id)
      } catch {
        logger.write("capture.snapshot failed: \(error.localizedDescription)")
        return encode(error: error.localizedDescription, id: id)
      }
    case "capture.active_window_metadata_sample":
      do {
        return encode(result: try captureActiveWindowMetadataSamplePayload(params: params), id: id)
      } catch {
        logger.write("capture.active_window_metadata_sample failed: \(error.localizedDescription)")
        return encode(error: error.localizedDescription, id: id)
      }
    case "capture.ux.status":
      return encode(result: captureUXStatusPayload(), id: id)
    case "capture.ux.probe":
      return encode(result: captureUXProbePayload(params: params), id: id)
    case "memory.status":
      return encode(result: memoryDaemon.status().payload, id: id)
    case "memory.benchmark":
      do {
        return encode(result: try memoryBenchmarkPayload(params: params), id: id)
      } catch {
        logger.write("memory.benchmark failed: \(error.localizedDescription)")
        return encode(error: error.localizedDescription, id: id)
      }
    case "memory.update_wiki":
      logger.write("memory.update_wiki requested")
      return encode(result: memoryUpdateWikiPayload(params: params), id: id)
    case "wiki.status":
      let snapshot = wikiStatus()
      return encode(result: wikiPayload(snapshot), id: id)
    case "wiki.publish":
      do {
        let receipt = try publishViaCoreAndAppMirror(params: params)
        return encode(result: receipt, id: id)
      } catch {
        logger.write("\(method) failed: \(error.localizedDescription)")
        return encode(result: wikiPublishFailurePayload(error, params: params), id: id)
      }
    case "wiki.start":
      logger.write("wiki.start requested")
      let current = wikiStatus()
      if current.running {
        logger.write("wiki.start already running")
        return encode(result: wikiPayload(current), id: id)
      }
      publishWikiInBackground(refresh: false)
      logger.write("wiki.start accepted")
      return encode(result: wikiPayload(pendingWikiSnapshot(health: "starting")), id: id)
    case "wiki.refresh":
      logger.write("wiki.refresh requested")
      publishWikiInBackground(refresh: true)
      logger.write("wiki.refresh accepted")
      return encode(result: wikiPayload(pendingWikiSnapshot(health: "refreshing")), id: id)
    case "wiki.stop":
      logger.write("wiki.stop requested")
      return encode(result: wikiPayload(wikiStatus()), id: id)
    default:
      if AgentHarnessRPCBridge.supports(method: method) {
        return agentHarnessResponse(method: method, params: params, id: id)
      }
      guard WikiCoreRPCBridge.supports(method: method) else {
        return encode(error: "Unknown method: \(method)", id: id)
      }
      return wikiCoreResponse(method: method, params: params, id: id)
    }
  }

  private func wikiCoreResponse(method: String, params: [String: Any], id: Any) -> Data {
    do {
      return encode(result: try wikiCoreRPC.call(method: method, params: params), id: id)
    } catch {
      logger.write("\(method) failed: \(error.localizedDescription)")
      return encode(error: error.localizedDescription, id: id)
    }
  }

  private func agentHarnessResponse(method: String, params: [String: Any], id: Any) -> Data {
    do {
      return encode(result: try agentHarnessRPC.call(method: method, params: params), id: id)
    } catch let error as AgentHarnessProcessError {
      logger.write("\(method) failed: \(error.localizedDescription)")
      if let payload = error.structuredPayload {
        return encode(result: payload, id: id)
      }
      return encode(error: error.localizedDescription, id: id)
    } catch {
      logger.write("\(method) failed: \(error.localizedDescription)")
      return encode(error: error.localizedDescription, id: id)
    }
  }

  private func captureStatusPayload() -> [String: Any] {
    let capturePaths = OneContextCapturePaths(runtimePaths: paths)
    let uxSnapshot = captureUXRuntimeSnapshot()
    let uxStatusPayload = captureUXStatusPayload(from: uxSnapshot)
    let capturePathMetadata = captureStatusPathMetadata(capturePaths)
    let permissionMetadata = capturePermissionDerivedMetadata(
      capturePaths: capturePathMetadata,
      uxStatus: uxSnapshot.status
    )
    return [
      "schema_version": 1,
      "surface": "capture_status",
      "root_directory": capturePaths.rootDirectory.path,
      "events_directory": capturePaths.eventsDirectory.path,
      "windows_directory": capturePaths.windowsDirectory.path,
      "media_directory": capturePaths.mediaDirectory.path,
      "window_unit_of_truth": true,
      "available_methods": [
        "capture.status",
        "capture.snapshot",
        "capture.active_window_metadata_sample",
        "capture.ux.status",
        "capture.ux.probe"
      ],
      "motion_hints": (try? CaptureJSON.dictionary(uxSnapshot.motionHints)) ?? [:],
      "metadata_sample_fusion": [
        "ux_motion_hints_enabled": uxSnapshot.metadataSampleFusionEnabled,
        "source": uxSnapshot.metadataSampleFusionEnabled ? "persistent_ux_event_tap" : "unavailable",
        "pixels_untouched": true
      ],
      "continuous_metadata_sampler": continuousMetadataSampler.statusPayload(),
      "permission_derived_metadata": (try? CaptureJSON.dictionary(permissionMetadata)) ?? [:],
      "ax_semantic_events": axSemanticPersistencePayload(),
      "ux_event_tap": uxStatusPayload
    ]
  }

  private func captureStatusPathMetadata(_ capturePaths: OneContextCapturePaths) -> CaptureStatusPathMetadata {
    CaptureStatusPathMetadata(
      rootDirectory: capturePaths.rootDirectory.path,
      eventsDirectory: capturePaths.eventsDirectory.path,
      windowsDirectory: capturePaths.windowsDirectory.path,
      mediaDirectory: capturePaths.mediaDirectory.path
    )
  }

  private func capturePermissionDerivedMetadata(
    capturePaths: CaptureStatusPathMetadata,
    uxStatus: OneContextUXEventTapStatus
  ) -> CapturePermissionDerivedMetadata {
    let subject = OneContextSystemPermissions.currentPermissionSubject()
    let daemonIdentity = captureDaemonProcessIdentity()
    let permissionSubject = capturePermissionSubjectIdentity(subject)
    let screenProof = capturePermissionProofSummary(
      key: OneContextSystemPermissions.screenCaptureProofKey,
      subject: subject
    )
    let systemAudioProof = capturePermissionProofSummary(
      key: OneContextSystemPermissions.systemAudioProofKey,
      subject: subject
    )
    let inputMonitoringProof = capturePermissionProofSummary(
      key: OneContextSystemPermissions.inputMonitoringProofKey,
      subject: subject
    )

    let permissionPlatform = OneContextPermissionPlatform.live
    let accessibilityReady = permissionPlatform.accessibilityTrusted()
    let screenRuntimeReady = permissionPlatform.screenPreflight()
    let inputMonitoringRuntimeReady = permissionPlatform.inputPreflight()
    let inputMonitoringReady = uxStatus.tapActive && inputMonitoringProof.matchesCurrentSubject
    let screenReady = screenRuntimeReady && screenProof.matchesCurrentSubject
    let systemAudioReady = screenRuntimeReady && systemAudioProof.matchesCurrentSubject

    return CapturePermissionDerivedMetadata(
      generatedAt: SCStreamFrameMetadataParser.isoTimestamp(Date()),
      processIdentities: [daemonIdentity, permissionSubject],
      capturePaths: capturePaths,
      signals: [
        "accessibility": CapturePermissionSignalMetadata(
          ready: accessibilityReady,
          status: accessibilityReady ? "granted" : "required",
          source: "AXIsProcessTrusted plus AX focused-context reads during capture snapshots",
          ownerRole: daemonIdentity.role,
          permissionSubjectRole: permissionSubject.role,
          note: "Focused-context availability is reported as permission readiness only; capture.status does not read raw focused text, geometry, or window contents.",
          focusedContext: CaptureFocusedContextAvailability(
            available: accessibilityReady,
            trusted: accessibilityReady,
            status: accessibilityReady ? "permission_granted" : "not_trusted",
            source: "OneContextWindowIndexer.focusedContext"
          )
        ),
        "input_monitoring": CapturePermissionSignalMetadata(
          ready: inputMonitoringReady,
          status: inputMonitoringReady ? "granted" : (inputMonitoringRuntimeReady ? "runtime_granted_unproved" : "required"),
          source: "persistent listen-only CGEventTap owned by 1contextd",
          ownerRole: daemonIdentity.role,
          permissionSubjectRole: permissionSubject.role,
          note: "Only aggregate input-event counts and timing hints are exposed; no raw keystrokes, key codes, text, or coordinates are included.",
          eventTap: CaptureInputEventTapMetadata(
            active: uxStatus.tapActive,
            lifecycleState: uxStatus.lifecycleState,
            eventTap: uxStatus.eventTap,
            tapOptions: uxStatus.tapOptions,
            eventMask: uxStatus.eventMask,
            observedEventCount: uxStatus.observedEventCount,
            queueDepth: uxStatus.queueDepth,
            droppedCount: uxStatus.droppedCount,
            coalescedCount: uxStatus.coalescedCount,
            lastEventAt: uxStatus.lastEventAt
          ),
          proof: inputMonitoringProof
        ),
        "screen_capture": CapturePermissionSignalMetadata(
          ready: screenReady,
          status: screenReady ? "granted" : (screenRuntimeReady ? "proof_required" : "required"),
          source: "ScreenCaptureKit screen-frame proof recorded by setup/runtime probes",
          ownerRole: daemonIdentity.role,
          permissionSubjectRole: permissionSubject.role,
          proof: screenProof
        ),
        "system_audio": CapturePermissionSignalMetadata(
          ready: systemAudioReady,
          status: systemAudioReady ? "granted" : (screenRuntimeReady ? "proof_required" : "required"),
          source: "ScreenCaptureKit system-audio sample proof recorded by setup/runtime probes",
          ownerRole: daemonIdentity.role,
          permissionSubjectRole: permissionSubject.role,
          proof: systemAudioProof
        )
      ]
    )
  }

  private func captureDaemonProcessIdentity() -> CaptureProcessIdentity {
    CaptureProcessIdentity(
      role: "daemon_process",
      pid: Int(getpid()),
      executablePath: OneContextAppIdentity.currentExecutableURL()?.path ?? CommandLine.arguments.first ?? "unknown",
      bundleIdentifier: Bundle.main.bundleIdentifier ?? paths.identity.bundleIdentifier
    )
  }

  private func capturePermissionSubjectIdentity(_ subject: OneContextPermissionSubject) -> CaptureProcessIdentity {
    CaptureProcessIdentity(
      role: "permission_subject",
      executablePath: subject.executablePath,
      bundleIdentifier: subject.bundleIdentifier,
      appVersion: subject.appVersion,
      designatedRequirementSHA256: subject.designatedRequirementSHA256
    )
  }

  private func capturePermissionProofSummary(
    key: String,
    subject: OneContextPermissionSubject
  ) -> CapturePermissionProofSummary {
    guard let proof = capturePermissionProof(key) else {
      return CapturePermissionProofSummary(
        proofKey: key,
        recorded: false,
        matchesCurrentSubject: false
      )
    }

    return CapturePermissionProofSummary(
      proofKey: key,
      recorded: true,
      matchesCurrentSubject: capturePermissionProofMatches(proof, subject: subject),
      method: proof["method"] as? String,
      provedAt: proof["proved_at"] as? String,
      details: capturePermissionProofDetails(proof["details"])
    )
  }

  private func capturePermissionProof(_ key: String) -> [String: Any]? {
    guard let preferences = NSDictionary(contentsOfFile: paths.preferencesPath) as? [String: Any] else {
      return nil
    }
    return preferences[key] as? [String: Any]
  }

  private func capturePermissionProofMatches(
    _ proof: [String: Any],
    subject: OneContextPermissionSubject
  ) -> Bool {
    guard let proofSubject = proof["subject"] as? [String: Any] else {
      return false
    }
    return proofSubject["bundle_identifier"] as? String == subject.bundleIdentifier
      && proofSubject["designated_requirement_sha256"] as? String == subject.designatedRequirementSHA256
      && proofSubject["app_version"] as? String == subject.appVersion
  }

  private func capturePermissionProofDetails(_ value: Any?) -> [String: String] {
    guard let details = value as? [String: Any] else { return [:] }
    var result: [String: String] = [:]
    for (key, value) in details {
      switch value {
      case let string as String:
        result[key] = string
      case let bool as Bool:
        result[key] = bool ? "true" : "false"
      case let number as NSNumber:
        result[key] = number.stringValue
      default:
        continue
      }
    }
    return result
  }

  private func captureUXStatusPayload() -> [String: Any] {
    captureUXStatusPayload(from: captureUXRuntimeSnapshot())
  }

  private func captureUXStatusPayload(from snapshot: CaptureUXRuntimeSnapshot) -> [String: Any] {
    var payload = (try? CaptureJSON.dictionary(snapshot.status)) ?? [:]
    payload["surface"] = "capture_ux_event_tap_status"
    payload["startup_wired"] = snapshot.startupWired
    payload["motion_hints"] = (try? CaptureJSON.dictionary(snapshot.motionHints)) ?? [:]
    payload["metadata_sample_fusion_enabled"] = snapshot.metadataSampleFusionEnabled
    if let startupError = snapshot.startupError {
      payload["startup_error"] = startupError
    }
    payload["permission_subject"] = daemonUXPermissionSubjectPayload()
    payload["jsonl_persistence"] = uxEventPersistencePayload()
    payload["tcc_identity_note"] = "UX event taps are created by the 1contextd daemon process. Input Monitoring must be granted to this app/daemon identity before a persistent tap is enabled."
    return payload
  }

  private func captureUXRuntimeSnapshot(note: String? = nil) -> CaptureUXRuntimeSnapshot {
    let now = Date()
    if uxEventTapStartupWired {
      let motionHints = uxEventTap.motionHints(now: now)
      let status = uxEventTap.status(note: note ?? "Persistent UX event tap is feeding capture motion hints.")
      return CaptureUXRuntimeSnapshot(
        status: status,
        motionHints: motionHints,
        startupWired: true,
        metadataSampleFusionEnabled: status.tapActive,
        startupError: uxEventTapStartupError
      )
    }

    let startupError = uxEventTapStartupError
    return CaptureUXRuntimeSnapshot(
      status: OneContextUXEventTapStatus.inactive(
        startupWired: true,
        owner: daemonUXEventTapOwner(),
        lifecycleState: startupError == nil ? "starting" : "degraded",
        lastError: startupError,
        note: startupError.map { "Persistent UX event tap failed to start: \($0)" }
          ?? "Persistent UX event tap is not available; metadata samples use ScreenCaptureKit signals only."
      ),
      motionHints: UXMotionHints(
        generatedAt: SCStreamFrameMetadataParser.isoTimestamp(now),
        scrollEventRecently: false,
        keyboardActivityRecently: false,
        estimatedScrollDY: 0,
        focusedRecently: false
      ),
      startupWired: true,
      metadataSampleFusionEnabled: false,
      startupError: startupError
    )
  }

  private func currentUXMotionHintsForMetadataFusion() -> UXMotionHints? {
    let snapshot = captureUXRuntimeSnapshot(note: "Persistent UX event tap is feeding active-window metadata samples.")
    return snapshot.metadataSampleFusionEnabled ? snapshot.motionHints : nil
  }

  private func captureUXProbePayload(params: [String: Any]) -> [String: Any] {
    let persistentSnapshot = captureUXRuntimeSnapshot()
    if persistentSnapshot.status.tapActive {
      let now = Date()
      let report = OneContextUXEventTapProbeReport(
        generatedAt: SCStreamFrameMetadataParser.isoTimestamp(now),
        tapCreated: false,
        observedEventCount: persistentSnapshot.status.observedEventCount,
        anchors: [],
        motionHints: persistentSnapshot.motionHints,
        tapStatus: persistentSnapshot.status,
        errorMessage: "Persistent daemon UX event tap is active; short-lived probe skipped to avoid creating a competing CGEventTap."
      )
      var payload = (try? CaptureJSON.dictionary(report)) ?? [:]
      payload["surface"] = "capture_ux_event_tap_probe"
      payload["probe_skipped"] = true
      payload["permission_subject"] = daemonUXPermissionSubjectPayload()
      payload["tcc_identity_note"] = "The persistent 1contextd UX tap owns the daemon event lane; probe status reused it instead of creating a second tap."
      return payload
    }

    let timeoutSeconds: TimeInterval
    if let number = params["timeout_seconds"] as? NSNumber {
      timeoutSeconds = number.doubleValue
    } else {
      timeoutSeconds = params["timeout_seconds"] as? TimeInterval ?? 1
    }
    let report = OneContextUXEventTap.probe(timeoutSeconds: timeoutSeconds)
    var payload = (try? CaptureJSON.dictionary(report)) ?? [:]
    payload["surface"] = "capture_ux_event_tap_probe"
    payload["permission_subject"] = daemonUXPermissionSubjectPayload()
    payload["tcc_identity_note"] = "This probe creates a short-lived listen-only CGEventTap inside 1contextd; it does not start the background UX tap."
    return payload
  }

  private func daemonUXEventTapOwner() -> OneContextUXEventTapOwner {
    OneContextUXEventTapOwner(
      pid: Int(getpid()),
      executable: OneContextAppIdentity.currentExecutableURL()?.path ?? CommandLine.arguments.first ?? "unknown",
      bundle: Bundle.main.bundleIdentifier ?? paths.identity.bundleIdentifier
    )
  }

  private func daemonUXPermissionSubjectPayload() -> [String: Any] {
    let subject = OneContextSystemPermissions.currentPermissionSubject()
    var payload = (try? CaptureJSON.dictionary(subject)) ?? [
      "bundleIdentifier": subject.bundleIdentifier,
      "executablePath": subject.executablePath,
      "appVersion": subject.appVersion
    ]
    payload["tap_owner_process"] = "1contextd"
    payload["tap_owner_pid"] = Int(getpid())
    payload["tap_owner_executable_path"] = OneContextAppIdentity.currentExecutableURL()?.path ?? CommandLine.arguments.first ?? "unknown"
    payload["tap_owner_bundle_identifier"] = Bundle.main.bundleIdentifier ?? paths.identity.bundleIdentifier
    return payload
  }

  private func captureSnapshotPayload() throws -> [String: Any] {
    let runtimePaths = paths
    let statusPayload = captureStatusPayload()
    let semaphore = DispatchSemaphore(value: 0)
    final class CaptureResultBox: @unchecked Sendable {
      var result: Result<[String: Any], Error>?
    }
    let box = CaptureResultBox()

    let task = Task {
      do {
        let indexer = OneContextWindowIndexer(
          uxMotionHintsProvider: { [weak self] in
            self?.currentUXMotionHintsForMetadataFusion()
          }
        )
        let recorder = OneContextCaptureRecorder(runtimePaths: runtimePaths, indexer: indexer)
        let snapshot = try await recorder.recordWindowSnapshot()
        self.persistAXSemanticEvents(from: snapshot.focusedContext)
        var payload = try CaptureJSON.dictionary(snapshot)
        payload["surface"] = "capture_window_snapshot"
        payload["stored"] = true
        payload["capture_status"] = statusPayload
        box.result = .success(payload)
      } catch {
        box.result = .failure(error)
      }
      semaphore.signal()
    }

    guard semaphore.wait(timeout: .now() + 5) == .success else {
      task.cancel()
      throw CaptureDaemonError.snapshotTimedOut
    }

    switch box.result {
    case .success(let payload):
      return payload
    case .failure(let error):
      throw error
    case .none:
      throw CaptureDaemonError.snapshotUnavailable
    }
  }

  private func captureActiveWindowMetadataSamplePayload(params: [String: Any]) throws -> [String: Any] {
    let statusPayload = captureStatusPayload()
    let durationSeconds = max(0.25, min(10, doubleParam(params["duration_seconds"]) ?? 3))
    let maxFrames = max(1, min(120, intParam(params["max_frames"]) ?? 30))
    let semaphore = DispatchSemaphore(value: 0)
    final class CaptureMetadataResultBox: @unchecked Sendable {
      var result: Result<[String: Any], Error>?
    }
    let box = CaptureMetadataResultBox()

    let task = Task {
      do {
        let sample = try await self.continuousActiveWindowMetadataSample(
          durationSeconds: durationSeconds,
          maxFrames: maxFrames
        )
        var payload = try CaptureJSON.dictionary(sample)
        payload["operation"] = "capture.active_window_metadata_sample"
        payload["stored"] = sample.persistedEventCount > 0
        payload["capture_status"] = statusPayload
        box.result = .success(payload)
      } catch {
        box.result = .failure(error)
      }
      semaphore.signal()
    }

    guard semaphore.wait(timeout: .now() + durationSeconds + 8) == .success else {
      task.cancel()
      throw CaptureDaemonError.metadataSampleTimedOut
    }

    switch box.result {
    case .success(let payload):
      return payload
    case .failure(let error):
      throw error
    case .none:
      throw CaptureDaemonError.metadataSampleUnavailable
    }
  }

  private func continuousActiveWindowMetadataSample(
    durationSeconds: TimeInterval,
    maxFrames: Int
  ) async throws -> ActiveWindowMetadataSample {
    let stream = ActiveWindowMetadataStream(
      runtimePaths: paths,
      indexer: OneContextWindowIndexer(
        uxMotionHintsProvider: { [weak self] in
          self?.currentUXMotionHintsForMetadataFusion()
        }
      ),
      uxMotionHintsProvider: { [weak self] in
        self?.currentUXMotionHintsForMetadataFusion()
      },
      focusedContextHandler: { [weak self] focusedContext in
        self?.persistAXSemanticEvents(from: focusedContext)
      }
    )
    return try await stream.sample(durationSeconds: durationSeconds, maxFrames: maxFrames)
  }

  private func intParam(_ value: Any?) -> Int? {
    switch value {
    case let int as Int:
      return int
    case let number as NSNumber:
      return number.intValue
    case let string as String:
      return Int(string)
    default:
      return nil
    }
  }

  private func doubleParam(_ value: Any?) -> Double? {
    switch value {
    case let double as Double:
      return double
    case let number as NSNumber:
      return number.doubleValue
    case let string as String:
      return Double(string)
    default:
      return nil
    }
  }

  private func boolParam(_ value: Any?) -> Bool? {
    switch value {
    case let bool as Bool:
      return bool
    case let number as NSNumber:
      return number.boolValue
    case let string as String:
      let normalized = string.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
      if ["1", "true", "yes", "y"].contains(normalized) {
        return true
      }
      if ["0", "false", "no", "n"].contains(normalized) {
        return false
      }
      return nil
    default:
      return nil
    }
  }

  private func stringParam(_ value: Any?) -> String? {
    guard let string = value as? String else { return nil }
    let trimmed = string.trimmingCharacters(in: .whitespacesAndNewlines)
    return trimmed.isEmpty ? nil : trimmed
  }

  private func memoryBenchmarkPayload(params: [String: Any]) throws -> [String: Any] {
    let maxEvents = params["max_events"] as? Int ?? 1_000
    let maxLines = params["max_lines"] as? Int ?? 50_000
    return try memoryDaemon.benchmark(maxEvents: maxEvents, maxLines: maxLines)
  }

  private func memoryUpdateWikiPayload(params: [String: Any]) -> [String: Any] {
    let provider = stringParam(params["provider"]) ?? "codex"
    let executeAgents = boolParam(params["execute_agents"]) ?? false
    let importSources = boolParam(params["import_sources"]) ?? true
    let importTicks = intParam(params["import_ticks"]) ?? memoryWikiBackfillImportTicks
    let sourceWindowDays = intParam(params["source_window_days"]) ?? memoryWikiBackfillWindowDays
    let sourceMaxEvents = intParam(params["source_max_events"]) ?? memoryWikiBackfillMaxEvents
    let sourceMaxLines = intParam(params["source_max_lines"]) ?? memoryWikiBackfillMaxLines
    let sourceQueryLimit = intParam(params["source_query_limit"]) ?? memoryWikiBackfillQueryLimit
    let sourceCursorName = stringParam(params["source_cursor_name"]) ?? memoryWikiUpdateCursorName
    let maxConcurrent = intParam(params["max_concurrent"])
    let timeoutSeconds = intParam(params["timeout_seconds"]) ?? memoryWikiUpdateTimeoutSeconds
    let runID = stringParam(params["run_id"])
    let trigger = stringParam(params["trigger"]) ?? "memory.update_wiki.manual"
    var payload: [String: Any] = [
      "surface": "memory_update_wiki",
      "status": "accepted",
      "trigger": trigger,
      "provider": provider,
      "execute_agents": executeAgents,
      "import_sources": importSources,
      "import_ticks": importTicks,
      "source_window_days": sourceWindowDays,
      "source_max_events": sourceMaxEvents,
      "source_max_lines": sourceMaxLines,
      "source_query_limit": sourceQueryLimit
    ]
    payload["source_cursor_name"] = sourceCursorName

    guard beginMemoryWikiUpdate(trigger: trigger) else {
      payload["status"] = "already_running"
      payload["memory_core_status"] = "already_running"
      payload["memory_core_error"] = "Another wiki update is already running."
      payload["memory_update"] = memoryWikiUpdateStatusPayload()
      payload["wiki"] = wikiPayload(pendingWikiSnapshot(health: "updating"))
      logger.write("memory.update_wiki skipped already_running trigger=\(trigger)")
      return payload
    }

    var finalStatus = "completed"
    var finalError: String?
    do {
      let result = try performMemoryCoreWikiUpdate(
        provider: provider,
        runID: runID,
        executeAgents: executeAgents,
        maxConcurrent: maxConcurrent,
        importSources: importSources,
        importTicks: importTicks,
        sourceWindowDays: sourceWindowDays,
        sourceMaxEvents: sourceMaxEvents,
        sourceMaxLines: sourceMaxLines,
        sourceQueryLimit: sourceQueryLimit,
        sourceCursorName: sourceCursorName,
        timeoutSeconds: timeoutSeconds
      )
      payload["memoryd_bin"] = result.memorydBin?.path
      payload["memory_core"] = result.payload
      finalStatus = memoryCoreOperationStatus(result.payload)
      payload["memory_core_status"] = finalStatus == "failed" ? "failed" : "ok"
      if finalStatus == "failed" {
        finalError = "memory-core update-wiki returned failed"
      }
    } catch {
      logger.write("memory.update_wiki memory-core unavailable: \(error.localizedDescription)")
      payload["memory_core_status"] = "unavailable"
      payload["memory_core_error"] = error.localizedDescription
      finalStatus = "failed"
      finalError = error.localizedDescription
    }

    finishMemoryWikiUpdate(status: finalStatus, error: finalError)
    publishWikiInBackground(refresh: true)
    payload["wiki"] = wikiPayload(pendingWikiSnapshot(health: "updating"))
    payload["memory_update"] = memoryWikiUpdateStatusPayload()
    logger.write("memory.update_wiki accepted memory_core_status=\(payload["memory_core_status"] ?? "unknown")")
    return payload
  }

  private func wikiPayload(_ snapshot: LocalWebSnapshot) -> [String: Any] {
    let renderSnapshot = wikiRenderQueue.snapshot()
    let appSnapshot = localWeb.status()
    let publishStatus = wikiRenderPayload(renderSnapshot)
    var payload: [String: Any] = [
      "surface": "wiki_app_status",
      "running": snapshot.running,
      "url": snapshot.url,
      "route": snapshot.route,
      "health": snapshot.health,
      "api": wikiAPIPayload(),
      "app_status": localWebPayload(appSnapshot),
      "publish_status": publishStatus,
      "render": publishStatus,
      "memory_update": memoryWikiUpdateStatusPayload()
    ]
    if let pid = snapshot.pid {
      payload["pid"] = Int(pid)
    }
    if let lastError = snapshot.lastError {
      payload["lastError"] = lastError
    }
    return payload
  }

  private func localWebPayload(_ snapshot: LocalWebSnapshot) -> [String: Any] {
    var payload: [String: Any] = [
      "surface": "local_web_status",
      "running": snapshot.running,
      "url": snapshot.url,
      "route": snapshot.route,
      "health": snapshot.health
    ]
    if let pid = snapshot.pid {
      payload["pid"] = Int(pid)
    }
    if let lastError = snapshot.lastError {
      payload["lastError"] = lastError
    }
    return payload
  }

  private func healthPayload() -> [String: Any] {
    let readiness = OneContextAppReadiness.current(localWeb: localWeb)
    return [
      "status": "ok",
      "version": oneContextVersion,
      "currentTime": ISO8601DateFormatter().string(from: Date()),
      "uptimeSeconds": max(0, Int(Date().timeIntervalSince(startedAt))),
      "pid": Int(getpid()),
      "memory": memoryDaemon.status().payload,
      "requiredSetupReady": readiness.requiredSetupReady,
      "requiredSetupSummary": readiness.requiredSetupSummary
    ]
  }

  private func startMemoryDaemon() {
    memoryDaemon.startIfAvailable { [logger] message in
      logger.write(message)
    }
  }

  private func startPersistentUXEventTap() {
    uxEventTapStartupWired = true
    do {
      try uxEventTap.startPersistent()
      uxEventTapStartupError = nil
      logger.write("persistent UX event tap started on dedicated capture runloop")
    } catch {
      uxEventTapStartupError = error.localizedDescription
      logger.write("persistent UX event tap degraded: \(error.localizedDescription)")
    }
  }

  private func startUXEventPersistenceTimer() {
    uxEventPersistenceQueue.sync {
      guard uxEventPersistenceTimer == nil else { return }
      let timer = DispatchSource.makeTimerSource(queue: uxEventPersistenceQueue)
      timer.schedule(
        deadline: .now() + uxEventPersistenceInterval,
        repeating: uxEventPersistenceInterval
      )
      timer.setEventHandler { [weak self] in
        self?.persistUXEventAnchorsLocked(force: false)
      }
      uxEventPersistenceTimer = timer
      timer.resume()
    }
    logger.write("UX event JSONL persistence timer started interval=\(uxEventPersistenceInterval)s")
  }

  private func stopUXEventPersistenceTimer() {
    uxEventPersistenceQueue.sync {
      uxEventPersistenceTimer?.cancel()
      uxEventPersistenceTimer = nil
    }
  }

  private func flushUXEventAnchorsToStore(force: Bool) {
    uxEventPersistenceQueue.sync {
      persistUXEventAnchorsLocked(force: force)
    }
  }

  private func persistAXSemanticEvents(from focusedContext: CaptureAXFocusedContext?) {
    guard let focusedContext else { return }
    axSemanticPersistenceQueue.sync {
      let events = axSemanticEventAggregator.ingest(focusedContext)
      guard !events.isEmpty else { return }

      do {
        try captureLogStore.prepare()
        _ = try captureLogStore.appendAXSemanticEvents(events)
        axSemanticPersistedEventCount += events.count
        axSemanticLastPersistedAt = events.last?.generatedAt ?? focusedContext.generatedAt
        axSemanticLastPersistenceError = nil
      } catch {
        axSemanticLastPersistenceError = error.localizedDescription
        logger.write("AX semantic event persistence failed: \(error.localizedDescription)")
      }
    }
  }

  private func axSemanticPersistencePayload() -> [String: Any] {
    axSemanticPersistenceQueue.sync {
      [
        "enabled": true,
        "source": "AXSemanticEventAggregator",
        "persisted_event_count": axSemanticPersistedEventCount,
        "last_persisted_at": (axSemanticLastPersistedAt ?? NSNull()) as Any,
        "last_error": (axSemanticLastPersistenceError ?? NSNull()) as Any,
        "buffer": (try? CaptureJSON.dictionary(axSemanticEventAggregator.snapshot())) ?? [:]
      ]
    }
  }

  private func persistUXEventAnchorsLocked(force: Bool) {
    guard uxEventTapStartupWired else { return }

    let now = Date()
    let result = force ? uxEventTap.flush(now: now) : uxEventTap.poll(now: now)
    uxEventLastPersistenceFlushAt = SCStreamFrameMetadataParser.isoTimestamp(now)

    guard !result.anchors.isEmpty else { return }

    do {
      try captureLogStore.prepare()
      _ = try captureLogStore.appendUXEventAnchors(result.anchors)
      uxEventPersistedAnchorCount += result.anchors.count
      uxEventLastPersistedAt = result.anchors.last?.endedAt ?? SCStreamFrameMetadataParser.isoTimestamp(now)
      uxEventPersistenceError = nil
      continuousMetadataSampler.trigger(afterPersisting: result.anchors)
    } catch {
      uxEventPersistenceError = error.localizedDescription
      logger.write("UX event anchor persistence failed: \(error.localizedDescription)")
    }
  }

  private func uxEventPersistencePayload() -> [String: Any] {
    uxEventPersistenceQueue.sync {
      [
        "enabled": uxEventTapStartupWired,
        "flush_interval_seconds": uxEventPersistenceInterval,
        "events_directory": OneContextCapturePaths(runtimePaths: paths).eventsDirectory.path,
        "persisted_anchor_count": uxEventPersistedAnchorCount,
        "last_persisted_at": (uxEventLastPersistedAt ?? NSNull()) as Any,
        "last_flush_at": (uxEventLastPersistenceFlushAt ?? NSNull()) as Any,
        "last_error": (uxEventPersistenceError ?? NSNull()) as Any
      ]
    }
  }

  private func startWikiAPI() {
    do {
      let snapshot = try wikiAPI.start()
      logger.write("wiki API started url=\(snapshot.url)")
    } catch {
      logger.write("wiki API failed: \(error.localizedDescription)")
    }
  }

  private func wikiStatus() -> LocalWebSnapshot {
    let renderSnapshot = wikiRenderQueue.snapshot()
    if renderSnapshot.running || renderSnapshot.scheduled || renderSnapshot.pending {
      return pendingWikiSnapshot(health: wikiHealth(for: renderSnapshot))
    }
    return localWeb.status()
  }

  private func pendingWikiSnapshot(health: String) -> LocalWebSnapshot {
    let current = localWeb.status()
    return LocalWebSnapshot(running: false, url: current.url, pid: current.pid, route: current.route, health: health)
  }

  private func publishWikiInBackground(refresh: Bool) {
    wikiRenderQueue.request(
      trigger: refresh ? "wiki.refresh" : "wiki.prepare",
      priority: refresh ? .manual : .automatic
    )
  }

  private func startAutomaticMemoryWikiUpdateTimer() {
    memoryWikiUpdateQueue.async { [weak self] in
      guard let self, self.memoryWikiUpdateTimer == nil else { return }
      let timer = DispatchSource.makeTimerSource(queue: self.memoryWikiUpdateQueue)
      timer.schedule(
        deadline: .now() + memoryWikiUpdateInitialDelay,
        repeating: memoryWikiUpdateInterval,
        leeway: .seconds(60)
      )
      timer.setEventHandler { [weak self] in
        self?.performScheduledMemoryWikiUpdate()
      }
      self.memoryWikiUpdateTimer = timer
      timer.resume()
      self.logger.write(
        "automatic memory.update_wiki timer started interval=\(Int(memoryWikiUpdateInterval))s initial_delay=\(Int(memoryWikiUpdateInitialDelay))s cursor=\(memoryWikiUpdateCursorName)"
      )
    }
  }

  private func performScheduledMemoryWikiUpdate() {
    let firstProcessPass = memoryWikiUpdateCompletedRunCount() == 0
    let trigger = firstProcessPass ? "memory.update_wiki.automatic.backfill" : "memory.update_wiki.automatic.incremental"
    guard beginMemoryWikiUpdate(trigger: trigger) else {
      logger.write("automatic memory.update_wiki skipped already_running")
      return
    }

    let importTicks = firstProcessPass ? memoryWikiBackfillImportTicks : memoryWikiIncrementalImportTicks
    let sourceWindowDays = firstProcessPass ? memoryWikiBackfillWindowDays : memoryWikiIncrementalWindowDays
    let sourceMaxEvents = firstProcessPass ? memoryWikiBackfillMaxEvents : memoryWikiIncrementalMaxEvents
    let sourceMaxLines = firstProcessPass ? memoryWikiBackfillMaxLines : memoryWikiIncrementalMaxLines
    let sourceQueryLimit = firstProcessPass ? memoryWikiBackfillQueryLimit : memoryWikiIncrementalQueryLimit
    let runID = "automatic-\(automaticWikiUpdateRunIDTimestamp())"

    do {
      let result = try performMemoryCoreWikiUpdate(
        provider: "codex",
        runID: runID,
        executeAgents: false,
        maxConcurrent: nil,
        importSources: true,
        importTicks: importTicks,
        sourceWindowDays: sourceWindowDays,
        sourceMaxEvents: sourceMaxEvents,
        sourceMaxLines: sourceMaxLines,
        sourceQueryLimit: sourceQueryLimit,
        sourceCursorName: memoryWikiUpdateCursorName,
        timeoutSeconds: memoryWikiUpdateTimeoutSeconds
      )
      let status = memoryCoreOperationStatus(result.payload)
      finishMemoryWikiUpdate(status: status, error: status == "failed" ? "memory-core update-wiki returned failed" : nil)
      publishWikiInBackground(refresh: false)
      logger.write(
        "automatic memory.update_wiki completed trigger=\(trigger) status=\(status) run_id=\(runID) memoryd=\(result.memorydBin?.path ?? "unavailable")"
      )
    } catch {
      finishMemoryWikiUpdate(status: "failed", error: error.localizedDescription)
      logger.write("automatic memory.update_wiki failed trigger=\(trigger): \(error.localizedDescription)")
    }
  }

  private func performMemoryCoreWikiUpdate(
    provider: String,
    runID: String?,
    executeAgents: Bool,
    maxConcurrent: Int?,
    importSources: Bool,
    importTicks: Int,
    sourceWindowDays: Int,
    sourceMaxEvents: Int,
    sourceMaxLines: Int,
    sourceQueryLimit: Int,
    sourceCursorName: String,
    timeoutSeconds: Int
  ) throws -> (payload: [String: Any], memorydBin: URL?) {
    let wikiCoreBin = try? wikiCore.discoverExecutable()
    let memorydBin = memoryDaemon.discoverExecutable()
    let payload = try memoryCore.updateWiki(
      provider: provider,
      runID: runID,
      executeAgents: executeAgents,
      maxConcurrent: maxConcurrent,
      timeoutSeconds: timeoutSeconds,
      importSources: importSources,
      importTicks: importTicks,
      sourceWindowDays: sourceWindowDays,
      sourceMaxEvents: sourceMaxEvents,
      sourceMaxLines: sourceMaxLines,
      sourceQueryLimit: sourceQueryLimit,
      sourceCursorName: sourceCursorName,
      memorydBin: memorydBin,
      runtimeRoot: paths.userContentDirectory,
      wikiCoreBin: wikiCoreBin
    )
    return (payload: payload, memorydBin: memorydBin)
  }

  private func memoryCoreOperationStatus(_ payload: [String: Any]) -> String {
    if let result = payload["result"] as? [String: Any] {
      return memoryCoreOperationStatus(result)
    }
    let status = (payload["status"] as? String) ?? ""
    if status == "completed" || status == "drafted" || status == "written" {
      return "completed"
    }
    if status == "ok" {
      return "completed"
    }
    if status == "failed" {
      return "failed"
    }
    if status.isEmpty {
      return "completed"
    }
    return status
  }

  private func beginMemoryWikiUpdate(trigger: String) -> Bool {
    memoryWikiUpdateStateLock.lock()
    defer { memoryWikiUpdateStateLock.unlock() }
    if memoryWikiUpdateInFlight {
      memoryWikiUpdateSkippedCount += 1
      return false
    }
    memoryWikiUpdateInFlight = true
    memoryWikiUpdateLastTrigger = trigger
    memoryWikiUpdateLastStartedAt = Date()
    memoryWikiUpdateLastStatus = "running"
    memoryWikiUpdateLastError = nil
    return true
  }

  private func finishMemoryWikiUpdate(status: String, error: String?) {
    memoryWikiUpdateStateLock.lock()
    defer { memoryWikiUpdateStateLock.unlock() }
    memoryWikiUpdateInFlight = false
    memoryWikiUpdateCompletedCount += 1
    memoryWikiUpdateLastCompletedAt = Date()
    memoryWikiUpdateLastStatus = status
    memoryWikiUpdateLastError = error
  }

  private func memoryWikiUpdateCompletedRunCount() -> Int {
    memoryWikiUpdateStateLock.lock()
    defer { memoryWikiUpdateStateLock.unlock() }
    return memoryWikiUpdateCompletedCount
  }

  private func memoryWikiUpdateStatusPayload() -> [String: Any] {
    memoryWikiUpdateStateLock.lock()
    defer { memoryWikiUpdateStateLock.unlock() }
    var payload: [String: Any] = [
      "surface": "memory_update_wiki_status",
      "state": memoryWikiUpdateInFlight ? "running" : (memoryWikiUpdateLastStatus ?? "idle"),
      "running": memoryWikiUpdateInFlight,
      "cursor_name": memoryWikiUpdateCursorName,
      "automatic_interval_seconds": Int(memoryWikiUpdateInterval),
      "automatic_initial_delay_seconds": Int(memoryWikiUpdateInitialDelay),
      "completed_count": memoryWikiUpdateCompletedCount,
      "skipped_count": memoryWikiUpdateSkippedCount,
      "backfill_window_days": memoryWikiBackfillWindowDays,
      "incremental_window_days": memoryWikiIncrementalWindowDays
    ]
    if let trigger = memoryWikiUpdateLastTrigger {
      payload["last_trigger"] = trigger
    }
    if let started = memoryWikiUpdateLastStartedAt {
      payload["last_started_at"] = ISO8601DateFormatter().string(from: started)
    }
    if let completed = memoryWikiUpdateLastCompletedAt {
      payload["last_completed_at"] = ISO8601DateFormatter().string(from: completed)
    }
    if let error = memoryWikiUpdateLastError {
      payload["last_error"] = error
    }
    return payload
  }

  private func automaticWikiUpdateRunIDTimestamp() -> String {
    let formatter = DateFormatter()
    formatter.calendar = Calendar(identifier: .gregorian)
    formatter.locale = Locale(identifier: "en_US_POSIX")
    formatter.timeZone = TimeZone(secondsFromGMT: 0)
    formatter.dateFormat = "yyyyMMdd-HHmmss"
    return formatter.string(from: Date())
  }

  private var wikiRenderState: String {
    let snapshot = wikiRenderQueue.snapshot()
    guard snapshot.running || snapshot.scheduled || snapshot.pending else {
      return "idle"
    }
    return wikiHealth(for: snapshot)
  }

  private func wikiHealth(for snapshot: WikiRenderQueueSnapshot) -> String {
    if snapshot.backingOff { return "backoff" }
    return snapshot.activeTrigger == "wiki.refresh" ? "refreshing" : "starting"
  }

  private func performWikiRender(_ request: WikiRenderQueueRequest) -> WikiRenderQueueOutcome {
    wikiPublicationQueue.sync {
      performWikiRenderLocked(request)
    }
  }

  private func performWikiRenderLocked(_ request: WikiRenderQueueRequest) -> WikiRenderQueueOutcome {
    let startedAt = Date()
    do {
      if request.trigger == "wiki.prepare" {
        try localWeb.ensureStaticSupportFiles()
      }

      let result = wikiRenderCoordinator.renderAndPublish(trigger: request.trigger)
      let duration = max(0, Int((Date().timeIntervalSince(startedAt) * 1_000).rounded()))
      let snapshot = localWeb.status()
      let skipped = result.message.contains("Skipped renderer; accepted inputs unchanged")
      logger.write("wiki site ready trigger=\(request.trigger) status=\(result.status.rawValue) skipped=\(skipped) url=\(snapshot.url)")
      if result.status == .failed {
        return WikiRenderQueueOutcome(
          status: .failed,
          dirtyPages: 0,
          rendererDurationMilliseconds: duration,
          error: result.message
        )
      }
      return WikiRenderQueueOutcome(
        status: skipped ? .skipped : .published,
        dirtyPages: skipped ? 0 : 1,
        rendererDurationMilliseconds: duration,
        skipReason: skipped ? "accepted_inputs_unchanged" : nil
      )
    } catch {
      let duration = max(0, Int((Date().timeIntervalSince(startedAt) * 1_000).rounded()))
      logger.write("wiki site prepare failed trigger=\(request.trigger): \(error.localizedDescription)")
      return WikiRenderQueueOutcome(
        status: .failed,
        dirtyPages: 0,
        rendererDurationMilliseconds: duration,
        error: error.localizedDescription
      )
    }
  }

  private func publishViaCoreAndAppMirror(params: [String: Any]) throws -> [String: Any] {
    try wikiPublicationQueue.sync {
      var receipt = try wikiCoreRPC.call(method: "wiki.publish", params: params)
      normalizePublishReceipt(&receipt, params: params)
      guard let status = receipt["status"] as? String, ["published", "skipped"].contains(status) else {
        return receipt
      }

      let trigger = publishTrigger(receipt: receipt, params: params)
      let appPublish = wikiRenderCoordinator.publishExistingSite(
        trigger: trigger,
        successMessage: "Validated Rust-published user-wiki://site, then published app-support://wiki-site/current."
      )
      receipt["app_publish"] = wikiRenderResultPayload(appPublish)
      if appPublish.status == .failed {
        receipt["status"] = "failed"
        receipt["next_action"] = "repair_publish_mirror"
        receipt["repair_hints"] = [
          "Rust publish completed, but the app-visible site mirror failed. Inspect app_publish.message and retry wiki.publish after repairing the rendered site or file permissions."
        ]
      }
      return receipt
    }
  }

  private func normalizePublishReceipt(_ receipt: inout [String: Any], params: [String: Any]) {
    if receipt["schema_version"] == nil {
      receipt["schema_version"] = 1
    }
    if receipt["operation"] == nil {
      receipt["operation"] = "wiki.publish"
    }
    if (receipt["trigger"] as? String)?.isEmpty != false {
      receipt["trigger"] = publishTrigger(receipt: receipt, params: params)
    }
  }

  private func wikiPublishFailurePayload(_ error: Error, params: [String: Any]) -> [String: Any] {
    let message = error.localizedDescription
    return [
      "schema_version": 1,
      "operation": "wiki.publish",
      "status": "failed",
      "trigger": publishTrigger(receipt: nil, params: params),
      "error": [
        "message": message
      ],
      "next_action": "repair_publish",
      "repair_hints": [
        "wiki.publish failed before the app-visible site mirror could be updated. Inspect daemon logs and retry after repairing the reported issue."
      ]
    ]
  }

  private func publishTrigger(receipt: [String: Any]?, params: [String: Any]) -> String {
    (receipt?["trigger"] as? String).flatMap { $0.isEmpty ? nil : $0 }
      ?? (params["trigger"] as? String).flatMap { $0.isEmpty ? nil : $0 }
      ?? "wiki.publish"
  }

  private func wikiRenderResultPayload(_ result: WikiRenderResult) -> [String: Any] {
    [
      "schema_version": result.schemaVersion,
      "status": result.status.rawValue,
      "trigger": result.trigger,
      "published_at": result.publishedAt,
      "source_site": result.sourceSite,
      "published_site": result.publishedSite,
      "message": result.message
    ]
  }

  private func wikiAPIPayload() -> [String: Any] {
    let snapshot = wikiAPI.snapshot
    var payload: [String: Any] = [
      "running": snapshot.running,
      "url": snapshot.url,
      "health": snapshot.health,
      "port": snapshot.port
    ]
    if let lastError = snapshot.lastError {
      payload["lastError"] = lastError
    }
    return payload
  }

  private func wikiRenderPayload(_ snapshot: WikiRenderQueueSnapshot) -> [String: Any] {
    let cadence = OneContextAppSettings.wikiAutomaticPublishCadence(preferencesPath: paths.preferencesPath)
    var payload: [String: Any] = [
      "surface": "wiki_publish_queue_status",
      "state": wikiRenderState,
      "running": snapshot.running,
      "scheduled": snapshot.scheduled,
      "pending": snapshot.pending,
      "accepted_count": snapshot.acceptedCount,
      "coalesced_count": snapshot.coalescedCount,
      "completed_count": snapshot.completedCount,
      "failed_count": snapshot.failedCount,
      "skipped_count": snapshot.skippedCount,
      "max_concurrent_renders": snapshot.maxConcurrentRenders,
      "backing_off": snapshot.backingOff,
      "backoff_remaining_ms": snapshot.backoffRemainingMilliseconds,
      "automatic_cadence": cadence.rawValue,
      "automatic_cadence_label": cadence.title,
      "automatic_cadence_limited": snapshot.automaticCadenceRemainingMilliseconds > 0,
      "automatic_cadence_remaining_ms": snapshot.automaticCadenceRemainingMilliseconds
    ]
    if snapshot.automaticCadenceRemainingMilliseconds > 0 {
      payload["earliest_next_automatic_publish_at"] = ISO8601DateFormatter().string(
        from: Date().addingTimeInterval(Double(snapshot.automaticCadenceRemainingMilliseconds) / 1_000)
      )
    }
    if let activeTrigger = snapshot.activeTrigger {
      payload["active_trigger"] = activeTrigger
    }
    if let last = snapshot.history.last {
      payload["last"] = [
        "trigger": last.trigger,
        "priority": last.priority.rawValue,
        "status": last.status.rawValue,
        "queue_delay_ms": last.queueDelayMilliseconds,
        "render_duration_ms": last.renderDurationMilliseconds,
        "renderer_duration_ms": last.rendererDurationMilliseconds,
        "dirty_pages": last.dirtyPages,
        "skip_reason": (last.skipReason ?? NSNull()) as Any,
        "error": (last.error ?? NSNull()) as Any
      ]
      if let error = last.error {
        payload["lastError"] = error
      }
    }
    return payload
  }

  private func encode(result: [String: Any], id: Any) -> Data {
    let payload: [String: Any] = [
      "jsonrpc": "2.0",
      "id": id,
      "result": result
    ]
    return encode(payload)
  }

  private func encode(error message: String, id: Any) -> Data {
    let payload: [String: Any] = [
      "jsonrpc": "2.0",
      "id": id,
      "error": [
        "code": -32603,
        "message": message
      ]
    ]
    return encode(payload)
  }

  private func encode(_ payload: [String: Any]) -> Data {
    let data = (try? JSONSerialization.data(withJSONObject: payload)) ?? Data("{}".utf8)
    return data + Data([UInt8(ascii: "\n")])
  }

  private func pruneCaches() {
    pruneCacheDirectory(paths.renderCacheDirectory)
    pruneCacheDirectory(paths.downloadCacheDirectory)
  }

  private func pruneCacheDirectory(_ directory: URL) {
    let fileManager = FileManager.default
    guard let enumerator = fileManager.enumerator(
      at: directory,
      includingPropertiesForKeys: [.isRegularFileKey, .contentModificationDateKey, .fileSizeKey],
      options: [.skipsHiddenFiles]
    ) else {
      return
    }

    let now = Date()
    var files: [(url: URL, size: UInt64, modifiedAt: Date)] = []

    for case let url as URL in enumerator {
      guard let values = try? url.resourceValues(
        forKeys: [.isRegularFileKey, .contentModificationDateKey, .fileSizeKey]
      ), values.isRegularFile == true
      else {
        continue
      }

      let modifiedAt = values.contentModificationDate ?? .distantPast
      if now.timeIntervalSince(modifiedAt) > cacheMaxAge {
        try? fileManager.removeItem(at: url)
        continue
      }

      files.append((url, UInt64(values.fileSize ?? 0), modifiedAt))
    }

    var totalBytes = files.reduce(UInt64(0)) { $0 + $1.size }
    guard totalBytes > cacheMaxBytes else { return }

    for file in files.sorted(by: { $0.modifiedAt < $1.modifiedAt }) {
      try? fileManager.removeItem(at: file.url)
      totalBytes = totalBytes > file.size ? totalBytes - file.size : 0
      if totalBytes <= cacheMaxBytes { break }
    }
  }

  private func cleanup() {
    flushUXEventAnchorsToStore(force: true)
    stopUXEventPersistenceTimer()
    stopAutomaticMemoryWikiUpdateTimer()
    uxEventTap.stop()
    wikiAPI.stop()
    memoryDaemon.stop()
    if listenFD >= 0 {
      close(listenFD)
    }
    unlink(paths.socketPath)
    unlink(paths.pidPath)
    if let socketPath = signalSocketPath {
      free(socketPath)
      signalSocketPath = nil
    }
    if let pidPath = signalPIDPath {
      free(pidPath)
      signalPIDPath = nil
    }
    if let logPath = signalLogPath {
      free(logPath)
      signalLogPath = nil
    }
    logger.write("1Context runtime stopped")
  }

  private func stopAutomaticMemoryWikiUpdateTimer() {
    memoryWikiUpdateQueue.sync {
      memoryWikiUpdateTimer?.cancel()
      memoryWikiUpdateTimer = nil
    }
  }
}

private func writeSignalLog(_ message: StaticString) {
  guard let logPath = signalLogPath else { return }
  let fd = open(logPath, O_WRONLY | O_CREAT | O_APPEND, S_IRUSR | S_IWUSR)
  guard fd >= 0 else { return }
  message.withUTF8Buffer { buffer in
    if let baseAddress = buffer.baseAddress {
      _ = write(fd, baseAddress, buffer.count)
    }
  }
  close(fd)
}

do {
  try OneContextDaemon().run()
} catch {
  let paths = RuntimePaths.current()
  try? FileManager.default.createDirectory(at: paths.logDirectory, withIntermediateDirectories: true)
  Logger(path: paths.logPath).write("1Context runtime failed: \(error.localizedDescription)")
  fputs("1Context runtime failed: \(error.localizedDescription)\n", stderr)
  exit(1)
}
