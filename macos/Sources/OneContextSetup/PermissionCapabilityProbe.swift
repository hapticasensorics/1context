import AppKit
@preconcurrency import ApplicationServices
import AVFoundation
import CoreGraphics
import CoreMedia
import Foundation
import ScreenCaptureKit

public enum OneContextPermissionCapabilityStatus: String, Codable, Sendable {
  case passed
  case failed
  case skipped
}

public struct OneContextPermissionCapabilityCheck: Codable, Equatable, Sendable {
  public let id: String
  public let title: String
  public let status: OneContextPermissionCapabilityStatus
  public let detail: String
  public let evidence: [String: String]

  public init(
    id: String,
    title: String,
    status: OneContextPermissionCapabilityStatus,
    detail: String,
    evidence: [String: String] = [:]
  ) {
    self.id = id
    self.title = title
    self.status = status
    self.detail = detail
    self.evidence = evidence
  }
}

public struct OneContextPermissionCapabilityProbeOptions: Codable, Equatable, Sendable {
  public let skipBrowserExtension: Bool
  public let timeoutSeconds: TimeInterval

  public init(skipBrowserExtension: Bool = true, timeoutSeconds: TimeInterval = 5) {
    self.skipBrowserExtension = skipBrowserExtension
    self.timeoutSeconds = timeoutSeconds
  }
}

public struct OneContextPermissionCapabilityProbeReport: Codable, Equatable, Sendable {
  public let schema: String
  public let generatedAt: String
  public let bundleIdentifier: String
  public let appVersion: String
  public let executablePath: String
  public let overallStatus: OneContextPermissionCapabilityStatus
  public let checks: [OneContextPermissionCapabilityCheck]

  public var passed: Bool {
    overallStatus == .passed
  }
}

public enum OneContextPermissionCapabilityProbe {
  public static func run(
    options: OneContextPermissionCapabilityProbeOptions = OneContextPermissionCapabilityProbeOptions()
  ) async -> OneContextPermissionCapabilityProbeReport {
    let subject = OneContextSystemPermissions.currentPermissionSubject()
    var checks: [OneContextPermissionCapabilityCheck] = []

    checks.append(await screenAndSystemAudio(timeout: options.timeoutSeconds))
    checks.append(microphone(timeout: options.timeoutSeconds))
    checks.append(accessibility())
    checks.append(inputMonitoring(timeout: options.timeoutSeconds))
    if options.skipBrowserExtension {
      checks.append(.skipped(id: "browser_extension", title: "Browser Extension Permissions", detail: "Skipped because the real browser extension is not built yet."))
    } else {
      checks.append(browserExtension(subject: subject))
    }
    checks.append(automation(subject: subject))
    checks.append(fullDiskAccess())

    let failed = checks.contains { $0.status == .failed }
    return OneContextPermissionCapabilityProbeReport(
      schema: "1context.permission-capability-probe.v1",
      generatedAt: ISO8601DateFormatter().string(from: Date()),
      bundleIdentifier: subject.bundleIdentifier,
      appVersion: subject.appVersion,
      executablePath: subject.executablePath,
      overallStatus: failed ? .failed : .passed,
      checks: checks
    )
  }

  private static func browserExtension(
    subject: OneContextPermissionSubject
  ) -> OneContextPermissionCapabilityCheck {
    if OneContextSystemPermissions.browserExtensionGranted(subject: subject) {
      return .passed(
        id: "browser_extension",
        title: "Browser Extension Permissions",
        detail: "Native-message proof includes extension identity and required browser permissions."
      )
    }
    return .failed(
      id: "browser_extension",
      title: "Browser Extension Permissions",
      detail: "No usable browser extension native-message proof is recorded for this signed app."
    )
  }

  private static func screenAndSystemAudio(timeout: TimeInterval) async -> OneContextPermissionCapabilityCheck {
    guard #available(macOS 13.0, *) else {
      return .failed(
        id: "screen_system_audio",
        title: "Screen & System Audio Recording",
        detail: "ScreenCaptureKit proof requires macOS 13 or newer."
      )
    }

    do {
      let evidence = try await ScreenSystemAudioCapabilityProof(timeout: timeout).run()
      return .passed(
        id: "screen_system_audio",
        title: "Screen & System Audio Recording",
        detail: "ScreenCaptureKit delivered a screen frame and system-audio sample.",
        evidence: evidence
      )
    } catch {
      return .failed(
        id: "screen_system_audio",
        title: "Screen & System Audio Recording",
        detail: error.localizedDescription
      )
    }
  }

  private static func microphone(timeout: TimeInterval) -> OneContextPermissionCapabilityCheck {
    guard AVCaptureDevice.authorizationStatus(for: .audio) == .authorized else {
      return .failed(
        id: "microphone",
        title: "Microphone",
        detail: "AVCapture reports microphone access is not authorized for this app."
      )
    }

    do {
      let evidence = try MicrophoneCapabilityProof(timeout: timeout).run()
      return .passed(
        id: "microphone",
        title: "Microphone",
        detail: "AVCapture delivered a microphone audio sample.",
        evidence: evidence
      )
    } catch {
      return .failed(
        id: "microphone",
        title: "Microphone",
        detail: error.localizedDescription
      )
    }
  }

  private static func accessibility() -> OneContextPermissionCapabilityCheck {
    guard AXIsProcessTrusted() else {
      return .failed(
        id: "accessibility",
        title: "Accessibility",
        detail: "AXIsProcessTrusted is false for this app."
      )
    }

    guard let frontmostApplication = NSWorkspace.shared.frontmostApplication else {
      return .failed(
        id: "accessibility",
        title: "Accessibility",
        detail: "NSWorkspace did not report a frontmost application to probe through Accessibility."
      )
    }

    let appElement = AXUIElementCreateApplication(frontmostApplication.processIdentifier)
    var roleValue: CFTypeRef?
    let roleError = AXUIElementCopyAttributeValue(appElement, kAXRoleAttribute as CFString, &roleValue)
    var titleValue: CFTypeRef?
    _ = AXUIElementCopyAttributeValue(appElement, kAXTitleAttribute as CFString, &titleValue)
    var windowsValue: CFTypeRef?
    let windowsError = AXUIElementCopyAttributeValue(appElement, kAXWindowsAttribute as CFString, &windowsValue)

    guard roleError == .success, let role = roleValue as? String, !role.isEmpty else {
      return .failed(
        id: "accessibility",
        title: "Accessibility",
        detail: "Could not read AX metadata from the frontmost app. AX role error: \(roleError.rawValue)."
      )
    }

    let windowCount = (windowsValue as? [Any])?.count
    return .passed(
      id: "accessibility",
      title: "Accessibility",
      detail: "Accessibility returned frontmost application metadata.",
      evidence: [
        "frontmost_app_pid": "\(frontmostApplication.processIdentifier)",
        "frontmost_app_bundle_identifier": frontmostApplication.bundleIdentifier ?? "",
        "frontmost_app_title": (titleValue as? String) ?? frontmostApplication.localizedName ?? "",
        "frontmost_app_ax_role": role,
        "frontmost_app_windows_readable": windowsError == .success ? "true" : "false",
        "frontmost_app_window_count": windowCount.map(String.init) ?? ""
      ]
    )
  }

  private static func inputMonitoring(timeout: TimeInterval) -> OneContextPermissionCapabilityCheck {
    do {
      let evidence = try InputMonitoringCapabilityProof(timeout: timeout).run()
      return .passed(
        id: "input_monitoring",
        title: "Input Monitoring",
        detail: "A session event tap observed a posted input event.",
        evidence: evidence
      )
    } catch {
      return .failed(
        id: "input_monitoring",
        title: "Input Monitoring",
        detail: error.localizedDescription
      )
    }
  }

  private static func automation(subject: OneContextPermissionSubject) -> OneContextPermissionCapabilityCheck {
    let targets = OneContextSystemPermissions.installedAutomationTargets(
      OneContextSystemPermissions.selectedAutomationTargets()
    )
    guard !targets.isEmpty else {
      return .failed(
        id: "automation",
        title: "Automation",
        detail: "No configured Automation target apps are installed."
      )
    }

    var passed: [String] = []
    var failed: [String] = []
    var evidence: [String: String] = [:]
    for target in targets {
      do {
        let value = try OneContextSystemPermissions.proveAutomationTarget(target)
        passed.append(target.name)
        evidence["\(target.bundleIdentifier).name"] = value
      } catch {
        failed.append("\(target.name): \(error.localizedDescription)")
      }
    }

    guard failed.isEmpty else {
      var detail = "Configured Automation target probe failed: \(failed.joined(separator: "; "))."
      if !passed.isEmpty {
        detail += " Passed: \(passed.joined(separator: ", "))."
      }
      return .failed(
        id: "automation",
        title: "Automation",
        detail: detail,
        evidence: evidence
      )
    }

    _ = OneContextSystemPermissions.refreshAutomationProofsIfAuthorized(
      subject: subject,
      targets: targets
    )
    guard OneContextSystemPermissions.automationGranted(subject: subject, targets: targets) else {
      return .failed(
        id: "automation",
        title: "Automation",
        detail: "Configured apps answered AppleEvents, but 1Context could not persist matching per-target Automation proof.",
        evidence: evidence
      )
    }

    return .passed(
      id: "automation",
      title: "Automation",
      detail: "Configured Automation target apps answered harmless AppleEvents and matching per-target proof was recorded.",
      evidence: evidence.merging([
        "target_count": "\(targets.count)",
        "target_names": targets.map(\.name).joined(separator: ", ")
      ]) { current, _ in current }
    )
  }

  private static func fullDiskAccess() -> OneContextPermissionCapabilityCheck {
    let report = OneContextSystemPermissions.fullDiskAccessProbeReport()
    guard report.existingProbeCount > 0 else {
      return .failed(
        id: "full_disk_access",
        title: "Full Disk Access",
        detail: "No protected Full Disk Access probe paths exist on this Mac."
      )
    }
    guard report.failedProbeLabels.isEmpty else {
      return .failed(
        id: "full_disk_access",
        title: "Full Disk Access",
        detail: "Could not read protected probe(s): \(report.failedProbeLabels.joined(separator: ", ")).",
        evidence: ["existing_probe_count": "\(report.existingProbeCount)"]
      )
    }
    return .passed(
      id: "full_disk_access",
      title: "Full Disk Access",
      detail: "Protected Full Disk Access probe paths were readable.",
      evidence: ["existing_probe_count": "\(report.existingProbeCount)"]
    )
  }

}

private func permissionCapabilityError(_ message: String) -> NSError {
  NSError(
    domain: "OneContextPermissionCapabilityProbe",
    code: 1,
    userInfo: [NSLocalizedDescriptionKey: message]
  )
}

private func waitForCapabilitySemaphore(_ semaphore: DispatchSemaphore, timeout: TimeInterval) async -> Bool {
  await withCheckedContinuation { continuation in
    DispatchQueue.global(qos: .utility).async {
      continuation.resume(returning: semaphore.wait(timeout: .now() + timeout) == .success)
    }
  }
}

private extension OneContextPermissionCapabilityCheck {
  static func passed(
    id: String,
    title: String,
    detail: String,
    evidence: [String: String] = [:]
  ) -> OneContextPermissionCapabilityCheck {
    OneContextPermissionCapabilityCheck(
      id: id,
      title: title,
      status: .passed,
      detail: detail,
      evidence: evidence
    )
  }

  static func failed(
    id: String,
    title: String,
    detail: String,
    evidence: [String: String] = [:]
  ) -> OneContextPermissionCapabilityCheck {
    OneContextPermissionCapabilityCheck(
      id: id,
      title: title,
      status: .failed,
      detail: detail,
      evidence: evidence
    )
  }

  static func skipped(
    id: String,
    title: String,
    detail: String,
    evidence: [String: String] = [:]
  ) -> OneContextPermissionCapabilityCheck {
    OneContextPermissionCapabilityCheck(
      id: id,
      title: title,
      status: .skipped,
      detail: detail,
      evidence: evidence
    )
  }
}

@available(macOS 13.0, *)
private final class ScreenSystemAudioCapabilityProof: NSObject, SCStreamOutput {
  private let timeout: TimeInterval
  private let screenSemaphore = DispatchSemaphore(value: 0)
  private let audioSemaphore = DispatchSemaphore(value: 0)
  private let stateQueue = DispatchQueue(label: "com.haptica.1context.permission-capability.screen-audio")
  private var screenFrameCount = 0
  private var audioSampleCount = 0
  private var firstScreenFrameMetadata: [String: String] = [:]

  init(timeout: TimeInterval) {
    self.timeout = timeout
  }

  func run() async throws -> [String: String] {
    let content = try await SCShareableContent.excludingDesktopWindows(false, onScreenWindowsOnly: true)
    guard let display = content.displays.first else {
      throw permissionCapabilityError("No captureable display was available.")
    }

    let filter = SCContentFilter(display: display, excludingWindows: [])
    let configuration = SCStreamConfiguration()
    configuration.width = 64
    configuration.height = 64
    configuration.minimumFrameInterval = CMTime(value: 1, timescale: 5)
    configuration.queueDepth = 3
    configuration.showsCursor = false
    configuration.capturesAudio = true
    configuration.excludesCurrentProcessAudio = false

    let stream = SCStream(filter: filter, configuration: configuration, delegate: nil)
    let queue = DispatchQueue(label: "com.haptica.1context.permission-capability.screencapturekit")
    try stream.addStreamOutput(self, type: .screen, sampleHandlerQueue: queue)
    try stream.addStreamOutput(self, type: .audio, sampleHandlerQueue: queue)

    try await startCapture(stream)
    let beeper = SystemAudioStimulus()
    beeper.start()
    let screenWaiter = screenSemaphore
    let audioWaiter = audioSemaphore
    let screenFrameArrived = await waitForCapabilitySemaphore(screenWaiter, timeout: timeout)
    let audioSampleArrived = await waitForCapabilitySemaphore(audioWaiter, timeout: timeout)
    beeper.stop()
    try await stopCapture(stream)

    guard screenFrameArrived else {
      throw permissionCapabilityError("ScreenCaptureKit started but did not deliver a screen frame.")
    }
    guard audioSampleArrived else {
      throw permissionCapabilityError("ScreenCaptureKit started but did not deliver a system-audio sample.")
    }

    let proofState = stateQueue.sync {
      (screenFrameCount, audioSampleCount, firstScreenFrameMetadata)
    }
    var evidence = [
      "display_count": "\(content.displays.count)",
      "screen_frame_count": "\(proofState.0)",
      "system_audio_sample_count": "\(proofState.1)"
    ]
    evidence.merge(proofState.2) { current, _ in current }
    return evidence
  }

  func stream(_ stream: SCStream, didOutputSampleBuffer sampleBuffer: CMSampleBuffer, of type: SCStreamOutputType) {
    guard CMSampleBufferIsValid(sampleBuffer) else { return }
    switch type {
    case .screen:
      let metadata = Self.screenFrameMetadataEvidence(sampleBuffer)
      let shouldSignal = stateQueue.sync { () -> Bool in
        screenFrameCount += 1
        if screenFrameCount == 1 {
          firstScreenFrameMetadata = metadata
        }
        return screenFrameCount == 1
      }
      if shouldSignal {
        screenSemaphore.signal()
      }
    case .audio:
      guard CMSampleBufferGetNumSamples(sampleBuffer) > 0 else { return }
      let shouldSignal = stateQueue.sync { () -> Bool in
        audioSampleCount += 1
        return audioSampleCount == 1
      }
      if shouldSignal {
        audioSemaphore.signal()
      }
    case .microphone:
      return
    @unknown default:
      return
    }
  }

  private func startCapture(_ stream: SCStream) async throws {
    try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Void, Error>) in
      stream.startCapture { error in
        if let error {
          continuation.resume(throwing: error)
        } else {
          continuation.resume()
        }
      }
    }
  }

  private func stopCapture(_ stream: SCStream) async throws {
    try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Void, Error>) in
      stream.stopCapture { error in
        if let error {
          continuation.resume(throwing: error)
        } else {
          continuation.resume()
        }
      }
    }
  }

  private static func screenFrameMetadataEvidence(_ sampleBuffer: CMSampleBuffer) -> [String: String] {
    guard let attachmentsArray = CMSampleBufferGetSampleAttachmentsArray(sampleBuffer, createIfNecessary: false)
      as? [[SCStreamFrameInfo: Any]],
      let attachments = attachmentsArray.first
    else {
      return ["screen_first_frame_attachments": "missing"]
    }

    var evidence: [String: String] = ["screen_first_frame_attachments": "present"]
    if let status = attachments[.status] as? NSNumber {
      evidence["screen_first_frame_status_raw"] = "\(status.intValue)"
    } else if let status = attachments[.status] as? Int {
      evidence["screen_first_frame_status_raw"] = "\(status)"
    }
    if let displayTime = attachments[.displayTime] as? NSNumber {
      evidence["screen_first_frame_display_time"] = "\(displayTime.uint64Value)"
    }
    if let scaleFactor = attachments[.scaleFactor] as? NSNumber {
      evidence["screen_first_frame_scale_factor"] = "\(scaleFactor.doubleValue)"
    }
    if let contentScale = attachments[.contentScale] as? NSNumber {
      evidence["screen_first_frame_content_scale"] = "\(contentScale.doubleValue)"
    }
    evidence["screen_first_frame_has_content_rect"] = attachments[.contentRect] == nil ? "false" : "true"
    if let dirtyRects = attachments[.dirtyRects] as? [Any] {
      evidence["screen_first_frame_dirty_rect_count"] = "\(dirtyRects.count)"
    } else {
      evidence["screen_first_frame_dirty_rect_count"] = attachments[.dirtyRects] == nil ? "" : "malformed"
    }
    return evidence
  }
}

private final class SystemAudioStimulus: @unchecked Sendable {
  private let lock = NSLock()
  private var active = false

  func start() {
    lock.withLock { active = true }
    DispatchQueue.global(qos: .utility).async { [weak self] in
      while self?.isActive == true {
        NSSound.beep()
        Thread.sleep(forTimeInterval: 0.5)
      }
    }
  }

  func stop() {
    lock.withLock { active = false }
  }

  private var isActive: Bool {
    lock.withLock { active }
  }
}

private final class MicrophoneCapabilityProof: NSObject, AVCaptureAudioDataOutputSampleBufferDelegate {
  private let timeout: TimeInterval
  private let sampleSemaphore = DispatchSemaphore(value: 0)
  private let stateQueue = DispatchQueue(label: "com.haptica.1context.permission-capability.microphone")
  private var sampleCount = 0

  init(timeout: TimeInterval) {
    self.timeout = timeout
  }

  func run() throws -> [String: String] {
    guard let device = AVCaptureDevice.default(for: .audio) else {
      throw permissionCapabilityError("No default microphone capture device was available.")
    }

    let session = AVCaptureSession()
    session.beginConfiguration()
    let input = try AVCaptureDeviceInput(device: device)
    guard session.canAddInput(input) else {
      throw permissionCapabilityError("AVCaptureSession could not add the microphone input.")
    }
    session.addInput(input)

    let output = AVCaptureAudioDataOutput()
    let queue = DispatchQueue(label: "com.haptica.1context.permission-capability.microphone-output")
    output.setSampleBufferDelegate(self, queue: queue)
    guard session.canAddOutput(output) else {
      throw permissionCapabilityError("AVCaptureSession could not add the microphone sample output.")
    }
    session.addOutput(output)
    session.commitConfiguration()

    session.startRunning()
    let sampleArrived = sampleSemaphore.wait(timeout: .now() + timeout) == .success
    let running = session.isRunning
    session.stopRunning()

    guard running else {
      throw permissionCapabilityError("AVCaptureSession did not enter the running state.")
    }
    guard sampleArrived else {
      throw permissionCapabilityError("AVCaptureSession ran but did not deliver a microphone sample.")
    }

    return [
      "device_unique_id": device.uniqueID,
      "sample_count": "\(stateQueue.sync { sampleCount })"
    ]
  }

  func captureOutput(
    _ output: AVCaptureOutput,
    didOutput sampleBuffer: CMSampleBuffer,
    from connection: AVCaptureConnection
  ) {
    guard CMSampleBufferIsValid(sampleBuffer), CMSampleBufferGetNumSamples(sampleBuffer) > 0 else { return }
    let shouldSignal = stateQueue.sync { () -> Bool in
      sampleCount += 1
      return sampleCount == 1
    }
    if shouldSignal {
      sampleSemaphore.signal()
    }
  }
}

private final class InputMonitoringCapabilityProof {
  private let timeout: TimeInterval

  init(timeout: TimeInterval) {
    self.timeout = timeout
  }

  func run() throws -> [String: String] {
    let state = InputMonitoringTapState()
    let mask = CGEventMask(1 << CGEventType.mouseMoved.rawValue)
    guard let tap = CGEvent.tapCreate(
      tap: .cgSessionEventTap,
      place: .headInsertEventTap,
      options: .listenOnly,
      eventsOfInterest: mask,
      callback: inputMonitoringTapCallback,
      userInfo: Unmanaged.passUnretained(state).toOpaque()
    ) else {
      throw permissionCapabilityError("Could not create a session event tap. Input Monitoring is probably not usable for this app.")
    }

    guard let source = CFMachPortCreateRunLoopSource(kCFAllocatorDefault, tap, 0) else {
      throw permissionCapabilityError("Could not create a run-loop source for the input event tap.")
    }
    CFRunLoopAddSource(CFRunLoopGetCurrent(), source, .commonModes)
    CGEvent.tapEnable(tap: tap, enable: true)
    defer {
      CGEvent.tapEnable(tap: tap, enable: false)
      CFRunLoopRemoveSource(CFRunLoopGetCurrent(), source, .commonModes)
    }

    let point = CGEvent(source: nil)?.location ?? CGPoint(x: 0, y: 0)
    CGEvent(
      mouseEventSource: nil,
      mouseType: .mouseMoved,
      mouseCursorPosition: point,
      mouseButton: .left
    )?.post(tap: .cghidEventTap)

    let deadline = Date().addingTimeInterval(timeout)
    while !state.observed, Date() < deadline {
      RunLoop.current.run(mode: .default, before: Date().addingTimeInterval(0.05))
    }

    guard state.observed else {
      throw permissionCapabilityError("The session event tap was created but did not observe an input event before timeout.")
    }
    return [
      "observed_event_type": state.observedEventType,
      "event_tap": "cgSessionEventTap"
    ]
  }
}

private final class InputMonitoringTapState {
  private let lock = NSLock()
  private var storedObserved = false
  private var storedObservedEventType = ""

  var observed: Bool {
    lock.withLock { storedObserved }
  }

  var observedEventType: String {
    lock.withLock { storedObservedEventType }
  }

  func markObserved(_ type: CGEventType) {
    lock.withLock {
      storedObserved = true
      storedObservedEventType = "\(type.rawValue)"
    }
  }
}

private let inputMonitoringTapCallback: CGEventTapCallBack = { _, type, event, refcon in
  if let refcon {
    Unmanaged<InputMonitoringTapState>.fromOpaque(refcon).takeUnretainedValue().markObserved(type)
  }
  return Unmanaged.passUnretained(event)
}
