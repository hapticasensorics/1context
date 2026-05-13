import AppKit
import Foundation
import OneContextCore
import OneContextUpdate
import OSLog
import Sparkle

private let sparkleUpdateLog = Logger(
  subsystem: "com.haptica.1context",
  category: "SparkleUpdate"
)

@MainActor
public final class SparkleUpdateController: NSObject {
  private let configuration: SparkleUpdaterConfiguration
  private let appContext: AppUpdateContext
  private var updater: SPUUpdater?
  private var userDriver: AppManagedSparkleUserDriver?
  private var observation: SparkleFrameworkObservation?

  public var onUpdateInformationChanged: (() -> Void)?

  public init(
    configuration: SparkleUpdaterConfiguration = .current(),
    appContext: AppUpdateContext = .current(),
    startUpdater: Bool = true
  ) {
    self.configuration = configuration
    self.appContext = appContext
    self.updater = nil
    self.userDriver = nil
    super.init()

    if configuration.isConfigured, appContext.location.canInstallAppUpdates {
      let userDriver = AppManagedSparkleUserDriver(policy: configuration.userFacingPolicy)
      let updater = SPUUpdater(
        hostBundle: .main,
        applicationBundle: .main,
        userDriver: userDriver,
        delegate: self
      )
      self.userDriver = userDriver
      self.updater = updater
      if startUpdater {
        do {
          try updater.start()
        } catch {
          recordUpdaterStartupError(error)
          self.updater = nil
          self.userDriver = nil
          return
        }
      }
      configureAutomaticUpdateDefaults()
    } else {
      self.updater = nil
      self.userDriver = nil
    }
  }

  public var canCheckForUpdates: Bool {
    updater?.canCheckForUpdates ?? false
  }

  @discardableResult
  public func checkForUpdates(_ sender: Any? = nil) -> Bool {
    guard let updater, updater.canCheckForUpdates else {
      return false
    }
    userDriver?.prepareForUserInitiatedCheck()
    updater.checkForUpdates()
    return true
  }

  @discardableResult
  public func checkForUpdatesInBackgroundOnLaunch() -> Bool {
    checkForUpdatesAutomatically()
  }

  @discardableResult
  public func checkForUpdatesAutomatically() -> Bool {
    guard let updater else {
      return false
    }
    configureAutomaticUpdateDefaults()
    guard updater.automaticallyChecksForUpdates, updater.automaticallyDownloadsUpdates else {
      return false
    }
    userDriver?.prepareForMandatoryAutomaticCheck()
    updater.checkForUpdatesInBackground()
    return true
  }

  public func snapshot(currentVersion: String = oneContextVersion) async -> AppUpdateSnapshot {
    await SparkleUpdateSnapshotProvider(
      configuration: configuration,
      appContext: appContext,
      driver: SparkleFrameworkStatusDriver(
        canCheckForUpdates: canCheckForUpdates,
        observation: observation
      )
    ).snapshot(currentVersion: currentVersion)
  }

  private func configureAutomaticUpdateDefaults() {
    guard let updater else { return }
    if configuration.automaticChecksEnabled {
      updater.automaticallyChecksForUpdates = true
    }
    if configuration.automaticDownloadsEnabled, updater.allowsAutomaticUpdates {
      updater.automaticallyDownloadsUpdates = true
    }
    if let scheduledCheckInterval = configuration.scheduledCheckInterval {
      updater.updateCheckInterval = scheduledCheckInterval
    }
  }

  private func recordUpdaterStartupError(_ error: Error?) {
    observation = SparkleFrameworkObservation(
      latestVersion: nil,
      updateAvailable: false,
      mandatoryUpdateAvailable: false,
      minimumUpdateVersion: nil,
      minimumAutoupdateVersion: nil,
      canInstallUpdates: false,
      startupErrorDescription: error?.localizedDescription ?? "Sparkle updater could not start."
    )
    onUpdateInformationChanged?()
  }
}

extension SparkleUpdateController: SPUUpdaterDelegate {
  public func updater(_ updater: SPUUpdater, didFindValidUpdate item: SUAppcastItem) {
    userDriver?.recordUpdateFound(isCriticalUpdate: item.isCriticalUpdate)
    observation = SparkleFrameworkObservation(
      latestVersion: item.displayVersionString,
      updateAvailable: true,
      mandatoryUpdateAvailable: item.isCriticalUpdate,
      minimumUpdateVersion: item.minimumUpdateVersion,
      minimumAutoupdateVersion: item.minimumAutoupdateVersion,
      canInstallUpdates: updater.canCheckForUpdates
    )
    onUpdateInformationChanged?()
  }

  public func updater(
    _ updater: SPUUpdater,
    willInstallUpdateOnQuit item: SUAppcastItem,
    immediateInstallationBlock immediateInstallHandler: @escaping () -> Void
  ) -> Bool {
    guard item.isCriticalUpdate else {
      return false
    }
    userDriver?.recordUpdateInstallationWillBegin()
    Task { @MainActor in
      immediateInstallHandler()
    }
    return true
  }

  public func updaterDidNotFindUpdate(_ updater: SPUUpdater) {
    recordNoUpdate(updater)
  }

  public func updaterDidNotFindUpdate(_ updater: SPUUpdater, error: Error) {
    recordNoUpdate(updater)
  }

  public func updater(
    _ updater: SPUUpdater,
    didFinishUpdateCycleFor updateCheck: SPUUpdateCheck,
    error: Error?
  ) {
    if let error {
      userDriver?.handleFailureIfAppropriate(for: error)
    }
    let retryRequest = userDriver?.finishUpdateSessionAndConsumeRetryRequest()
    if let retryRequest {
      Task { @MainActor [weak self] in
        try? await Task.sleep(nanoseconds: retryRequest.delayNanoseconds)
        self?.retryUpdateCheck(retryRequest)
      }
    }
  }

  private func recordNoUpdate(_ updater: SPUUpdater) {
    observation = SparkleFrameworkObservation(
      latestVersion: nil,
      updateAvailable: false,
      mandatoryUpdateAvailable: false,
      minimumUpdateVersion: nil,
      minimumAutoupdateVersion: nil,
      canInstallUpdates: updater.canCheckForUpdates
    )
    onUpdateInformationChanged?()
  }

  private func retryUpdateCheck(_ retryRequest: AppManagedSparkleFailureRetryRequest) {
    switch retryRequest.mode {
    case .automaticMandatory:
      guard let updater else { return }
      configureAutomaticUpdateDefaults()
      guard updater.automaticallyChecksForUpdates, updater.automaticallyDownloadsUpdates else {
        return
      }
      guard userDriver?.prepareForFailureRetry(retryRequest) == true else { return }
      updater.checkForUpdatesInBackground()
    case .userInitiated:
      guard let updater, updater.canCheckForUpdates else { return }
      guard userDriver?.prepareForFailureRetry(retryRequest) == true else { return }
      updater.checkForUpdates()
    }
  }
}

private struct SparkleFrameworkObservation: Sendable {
  let latestVersion: String?
  let updateAvailable: Bool
  let mandatoryUpdateAvailable: Bool
  let minimumUpdateVersion: String?
  let minimumAutoupdateVersion: String?
  let canInstallUpdates: Bool
  let startupErrorDescription: String?

  init(
    latestVersion: String?,
    updateAvailable: Bool,
    mandatoryUpdateAvailable: Bool,
    minimumUpdateVersion: String?,
    minimumAutoupdateVersion: String?,
    canInstallUpdates: Bool,
    startupErrorDescription: String? = nil
  ) {
    self.latestVersion = latestVersion
    self.updateAvailable = updateAvailable
    self.mandatoryUpdateAvailable = mandatoryUpdateAvailable
    self.minimumUpdateVersion = minimumUpdateVersion
    self.minimumAutoupdateVersion = minimumAutoupdateVersion
    self.canInstallUpdates = canInstallUpdates
    self.startupErrorDescription = startupErrorDescription
  }
}

private struct SparkleFrameworkStatusDriver: SparkleUpdateDriver, Sendable {
  let canCheckForUpdates: Bool
  let observation: SparkleFrameworkObservation?

  func snapshot(
    currentVersion: String,
    configuration: SparkleUpdaterConfiguration
  ) async -> SparkleUpdateDriverSnapshot {
    if let observation {
      let latestVersion = observation.latestVersion
      let displayedVersion = latestVersion ?? "latest"
      let status: String
      let nextAction: String
      if let startupErrorDescription = observation.startupErrorDescription {
        status = "Sparkle updater could not start."
        nextAction = startupErrorDescription
      } else if observation.updateAvailable {
        status = observation.mandatoryUpdateAvailable
          ? "1Context \(displayedVersion) is a mandatory update."
          : "1Context \(displayedVersion) is available."
        if observation.canInstallUpdates {
          nextAction = observation.mandatoryUpdateAvailable
            ? "Keep 1Context open while Sparkle installs and relaunches automatically."
            : "Install the Sparkle update from the app."
        } else {
          nextAction = "Wait for the current Sparkle update session to finish."
        }
      } else {
        status = "1Context is up to date."
        nextAction = "No action needed."
      }
      return SparkleUpdateDriverSnapshot(
        availability: .available,
        latestVersion: latestVersion,
        updateAvailable: observation.updateAvailable,
        mandatoryUpdateAvailable: observation.mandatoryUpdateAvailable,
        minimumUpdateVersion: observation.minimumUpdateVersion,
        minimumAutoupdateVersion: observation.minimumAutoupdateVersion,
        canInstallUpdates: observation.canInstallUpdates,
        userFacingStatus: status,
        nextAction: nextAction
      )
    }

    return SparkleUpdateDriverSnapshot(
      availability: .available,
      latestVersion: nil,
      updateAvailable: false,
      canInstallUpdates: canCheckForUpdates,
      userFacingStatus: canCheckForUpdates
        ? "1Context can check for updates."
        : "1Context is preparing the updater.",
      nextAction: canCheckForUpdates
        ? "Choose Check for Updates from the app menu."
        : "Try again after the updater finishes starting."
    )
  }
}

enum AppManagedSparkleCheckMode: Equatable {
  case automaticMandatory
  case userInitiated
}

enum AppManagedSparkleUpdateDecision: Equatable {
  case installWithoutPrompt
  case askUser
  case dismiss
}

enum AppManagedSparkleUpdateCheckInvocation: Equatable {
  case foreground
  case background
}

enum AppManagedSparkleFailureDisposition: Equatable {
  case ignore
  case silentRetry(remainingRetries: Int, delaySeconds: TimeInterval)
  case presentFailure
}

enum AppManagedSparkleUpdatePhase: Equatable {
  case checking
  case updateFound
  case downloadStarted
  case readyToInstall
  case installing
  case finished
  case failed
}

enum AppManagedSparkleFailureRetryKind: Equatable {
  case silent
  case userRequested
}

struct AppManagedSparkleFailureRetryRequest: Equatable {
  let sourceSessionID: UInt64
  let mode: AppManagedSparkleCheckMode
  let kind: AppManagedSparkleFailureRetryKind
  let remainingSilentRetries: Int
  let delayNanoseconds: UInt64
}

struct AppManagedSparkleUpdateSession: Equatable {
  let id: UInt64
  var mode: AppManagedSparkleCheckMode
  var phase: AppManagedSparkleUpdatePhase
  var remainingSilentRetries: Int
  var updateWasFound = false
  var criticalUpdateWasFound = false
  var downloadBegan = false
  var installWasOffered = false
  var installBegan = false
  var didPresentFailure = false
  var didHandleFailure = false
  var retryRequest: AppManagedSparkleFailureRetryRequest?

  var userInitiated: Bool {
    mode == .userInitiated
  }

  var failureRepresentsUpdateAttempt: Bool {
    criticalUpdateWasFound || downloadBegan || installWasOffered || installBegan
  }

  mutating func recordUpdateFound(isCriticalUpdate: Bool) {
    updateWasFound = true
    criticalUpdateWasFound = criticalUpdateWasFound || isCriticalUpdate
    phase = .updateFound
  }

  mutating func recordDownloadStarted() {
    downloadBegan = true
    phase = .downloadStarted
  }

  mutating func recordReadyToInstall() {
    installWasOffered = true
    phase = .readyToInstall
  }

  mutating func recordInstalling() {
    installBegan = true
    phase = .installing
  }

  mutating func recordFinished() {
    phase = .finished
  }

  mutating func recordFailed() {
    phase = .failed
  }
}

struct AppManagedSparkleUserDriverPolicy {
  static let failureAlertButtonTitles = ["Try Again", "OK"]

  static func decision(
    mode: AppManagedSparkleCheckMode,
    isCriticalUpdate: Bool,
    isInformationOnlyUpdate: Bool
  ) -> AppManagedSparkleUpdateDecision {
    if isInformationOnlyUpdate {
      return .dismiss
    }
    if isCriticalUpdate {
      return .installWithoutPrompt
    }
    switch mode {
    case .automaticMandatory:
      return .dismiss
    case .userInitiated:
      return .askUser
    }
  }

  static func silentFailureRetryBudget(for mode: AppManagedSparkleCheckMode) -> Int {
    silentFailureRetryDelays(for: mode).count
  }

  static func silentFailureRetryDelays(
    for mode: AppManagedSparkleCheckMode,
    environment: [String: String] = ProcessInfo.processInfo.environment
  ) -> [TimeInterval] {
    let overrideKey: String
    switch mode {
    case .automaticMandatory:
      overrideKey = "ONECONTEXT_SPARKLE_AUTOMATIC_RETRY_DELAYS_SECONDS"
    case .userInitiated:
      overrideKey = "ONECONTEXT_SPARKLE_MANUAL_RETRY_DELAYS_SECONDS"
    }
    if let override = retryDelayOverride(environment[overrideKey]) {
      return override
    }
    switch mode {
    case .automaticMandatory:
      return [15, 90, 300]
    case .userInitiated:
      return [2]
    }
  }

  private static func retryDelayOverride(_ value: String?) -> [TimeInterval]? {
    guard let value, !value.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
      return nil
    }
    let parts = value.split(separator: ",")
    var delays: [TimeInterval] = []
    for raw in parts {
      let trimmed = raw.trimmingCharacters(in: .whitespacesAndNewlines)
      guard let parsed = TimeInterval(trimmed), parsed >= 0 else {
        return nil
      }
      delays.append(parsed)
    }
    return delays.isEmpty ? nil : delays
  }

  static func checkInvocation(for mode: AppManagedSparkleCheckMode) -> AppManagedSparkleUpdateCheckInvocation {
    switch mode {
    case .automaticMandatory:
      return .background
    case .userInitiated:
      return .foreground
    }
  }

  static func shouldPresentFailure(
    mode: AppManagedSparkleCheckMode,
    attemptedInstall: Bool
  ) -> Bool {
    mode == .userInitiated || attemptedInstall
  }

  static func shouldPresentFailure(
    for error: Error,
    mode: AppManagedSparkleCheckMode,
    attemptedInstall: Bool
  ) -> Bool {
    let nsError = error as NSError
    if nsError.domain == SUSparkleErrorDomain {
      switch nsError.code {
      case 1001, 4007, 4008:
        return false
      default:
        break
      }
    }
    return shouldPresentFailure(mode: mode, attemptedInstall: attemptedInstall)
  }

  static func failureDisposition(
    for error: Error,
    mode: AppManagedSparkleCheckMode,
    attemptedInstall: Bool,
    remainingSilentRetries: Int
  ) -> AppManagedSparkleFailureDisposition {
    if isBenignSparkleFailure(error) {
      return .ignore
    }
    if mode == .automaticMandatory, !attemptedInstall {
      return silentRetryDispositionOrIgnore(
        for: mode,
        remainingSilentRetries: remainingSilentRetries
      )
    }
    guard shouldPresentFailure(
      for: error,
      mode: mode,
      attemptedInstall: attemptedInstall
    ) else {
      return .ignore
    }
    guard remainingSilentRetries > 0 else {
      return .presentFailure
    }
    return silentRetryDispositionOrIgnore(
      for: mode,
      remainingSilentRetries: remainingSilentRetries
    )
  }

  private static func isBenignSparkleFailure(_ error: Error) -> Bool {
    let nsError = error as NSError
    if nsError.domain == SUSparkleErrorDomain {
      switch nsError.code {
      case 1001, 4007, 4008:
        return true
      default:
        break
      }
    }
    return false
  }

  private static func silentRetryDispositionOrIgnore(
    for mode: AppManagedSparkleCheckMode,
    remainingSilentRetries: Int
  ) -> AppManagedSparkleFailureDisposition {
    guard remainingSilentRetries > 0 else {
      return .ignore
    }
    let remainingRetries = remainingSilentRetries - 1
    let delays = silentFailureRetryDelays(for: mode)
    let delayIndex = max(0, min(delays.count - 1, delays.count - remainingSilentRetries))
    return .silentRetry(
      remainingRetries: remainingRetries,
      delaySeconds: delays[delayIndex]
    )
  }

  static func retryDelayNanoseconds(seconds: TimeInterval) -> UInt64 {
    UInt64(max(0, seconds) * 1_000_000_000)
  }
}

@MainActor
final class AppManagedSparkleUserDriver: NSObject, SPUUserDriver {
  private let policy: UpdateUserFacingPolicy
  private var nextSessionID: UInt64 = 0
  private var session = AppManagedSparkleUpdateSession(
    id: 0,
    mode: .automaticMandatory,
    phase: .finished,
    remainingSilentRetries: AppManagedSparkleUserDriverPolicy.silentFailureRetryBudget(
      for: .automaticMandatory
    )
  )

  var currentCycleRepresentsUpdateAttempt: Bool {
    session.failureRepresentsUpdateAttempt
  }

  var currentSession: AppManagedSparkleUpdateSession {
    session
  }

  init(policy: UpdateUserFacingPolicy) {
    self.policy = policy
    super.init()
  }

  func prepareForMandatoryAutomaticCheck() {
    beginUpdateSession(mode: .automaticMandatory, resetRetryBudget: true)
  }

  func prepareForUserInitiatedCheck() {
    beginUpdateSession(mode: .userInitiated, resetRetryBudget: true)
  }

  func recordUpdateFound(isCriticalUpdate: Bool) {
    session.recordUpdateFound(isCriticalUpdate: isCriticalUpdate)
  }

  func recordUpdateInstallationWillBegin() {
    session.recordUpdateFound(isCriticalUpdate: true)
    session.recordInstalling()
  }

  func prepareForFailureRetry(_ retryRequest: AppManagedSparkleFailureRetryRequest) -> Bool {
    guard session.id == retryRequest.sourceSessionID else {
      return false
    }
    beginUpdateSession(
      mode: retryRequest.mode,
      resetRetryBudget: retryRequest.kind == .userRequested,
      remainingSilentRetries: retryRequest.remainingSilentRetries
    )
    return true
  }

  func finishUpdateSessionAndConsumeRetryRequest() -> AppManagedSparkleFailureRetryRequest? {
    let retryRequest = session.retryRequest
    session.retryRequest = nil
    if retryRequest == nil, session.phase != .failed {
      session.recordFinished()
    }
    return retryRequest
  }

  func show(
    _ request: SPUUpdatePermissionRequest,
    reply: @escaping (SUUpdatePermissionResponse) -> Void
  ) {
    reply(SUUpdatePermissionResponse(
      automaticUpdateChecks: true,
      automaticUpdateDownloading: NSNumber(value: true),
      sendSystemProfile: false
    ))
  }

  func showUserInitiatedUpdateCheck(cancellation: @escaping () -> Void) {}

  func showUpdateFound(
    with appcastItem: SUAppcastItem,
    state: SPUUserUpdateState,
    reply: @escaping (SPUUserUpdateChoice) -> Void
  ) {
    switch AppManagedSparkleUserDriverPolicy.decision(
      mode: session.mode,
      isCriticalUpdate: appcastItem.isCriticalUpdate,
      isInformationOnlyUpdate: appcastItem.isInformationOnlyUpdate
    ) {
    case .installWithoutPrompt:
      recordUpdateFound(isCriticalUpdate: appcastItem.isCriticalUpdate)
      session.recordReadyToInstall()
      reply(.install)
    case .askUser:
      if confirmInstall(appcastItem) {
        recordUpdateFound(isCriticalUpdate: appcastItem.isCriticalUpdate)
        session.recordReadyToInstall()
        reply(.install)
      } else {
        reply(.dismiss)
      }
    case .dismiss:
      reply(.dismiss)
    }
  }

  func showUpdateReleaseNotes(with downloadData: SPUDownloadData) {
    guard policy.showReleaseNotesInUpdateWindow else { return }
  }

  func showUpdateReleaseNotesFailedToDownloadWithError(_ error: Error) {}

  func showUpdateNotFoundWithError(_ error: Error) async {
    if session.mode == .userInitiated {
      presentAlert(title: "1Context is up to date.", message: "")
    }
  }

  func showUpdaterError(_ error: Error) async {
    handleFailureIfAppropriate(for: error)
  }

  func showDownloadInitiated(cancellation: @escaping () -> Void) {
    session.recordDownloadStarted()
  }

  func showDownloadDidReceiveExpectedContentLength(_ expectedContentLength: UInt64) {}

  func showDownloadDidReceiveData(ofLength length: UInt64) {}

  func showDownloadDidStartExtractingUpdate() {
    session.recordDownloadStarted()
  }

  func showExtractionReceivedProgress(_ progress: Double) {}

  func showReady(toInstallAndRelaunch reply: @escaping (SPUUserUpdateChoice) -> Void) {
    let shouldInstall = session.installWasOffered || session.criticalUpdateWasFound
    if shouldInstall {
      session.recordReadyToInstall()
    }
    reply(shouldInstall ? .install : .dismiss)
  }

  func showInstallingUpdate(
    withApplicationTerminated applicationTerminated: Bool,
    retryTerminatingApplication: @escaping () -> Void
  ) {
    session.recordInstalling()
  }

  func showUpdateInstalledAndRelaunched(_ relaunched: Bool) async {
    session.recordFinished()
    guard policy.postInstallMessageEnabled else { return }
    presentAlert(
      title: policy.postInstallTitle,
      message: policy.postInstallBody(displayVersion: oneContextVersion)
    )
  }

  func dismissUpdateInstallation() {
    session.recordFinished()
  }

  func showUpdateInFocus() {}

  func handleFailureIfAppropriate(for error: Error) {
    guard !session.didHandleFailure, !session.didPresentFailure else { return }
    session.didHandleFailure = true
    let attemptedInstall = currentCycleRepresentsUpdateAttempt

    let disposition = AppManagedSparkleUserDriverPolicy.failureDisposition(
      for: error,
      mode: session.mode,
      attemptedInstall: attemptedInstall,
      remainingSilentRetries: session.remainingSilentRetries
    )
    let nsError = error as NSError
    sparkleUpdateLog.warning(
      "Sparkle update cycle error sessionID=\(self.session.id, privacy: .public) mode=\(String(describing: self.session.mode), privacy: .public) phase=\(String(describing: self.session.phase), privacy: .public) attemptedInstall=\(attemptedInstall, privacy: .public) updateWasFound=\(self.session.updateWasFound, privacy: .public) criticalUpdateFound=\(self.session.criticalUpdateWasFound, privacy: .public) downloadBegan=\(self.session.downloadBegan, privacy: .public) installWasOffered=\(self.session.installWasOffered, privacy: .public) installBegan=\(self.session.installBegan, privacy: .public) domain=\(nsError.domain, privacy: .public) code=\(nsError.code, privacy: .public) disposition=\(String(describing: disposition), privacy: .public) description=\(nsError.localizedDescription, privacy: .public)"
    )

    switch disposition {
    case .ignore:
      return
    case .silentRetry(let remainingRetries, let delaySeconds):
      session.recordFailed()
      session.remainingSilentRetries = remainingRetries
      session.retryRequest = AppManagedSparkleFailureRetryRequest(
        sourceSessionID: session.id,
        mode: session.mode,
        kind: .silent,
        remainingSilentRetries: remainingRetries,
        delayNanoseconds: AppManagedSparkleUserDriverPolicy.retryDelayNanoseconds(
          seconds: delaySeconds
        )
      )
    case .presentFailure:
      session.recordFailed()
      session.didPresentFailure = true
      if presentFailureAlert(title: policy.failureTitle, message: policy.failureBody) {
        session.retryRequest = AppManagedSparkleFailureRetryRequest(
          sourceSessionID: session.id,
          mode: session.mode,
          kind: .userRequested,
          remainingSilentRetries: AppManagedSparkleUserDriverPolicy.silentFailureRetryBudget(
            for: session.mode
          ),
          delayNanoseconds: 250_000_000
        )
      }
    }
  }

  private func beginUpdateSession(
    mode: AppManagedSparkleCheckMode,
    resetRetryBudget: Bool,
    remainingSilentRetries: Int? = nil
  ) {
    nextSessionID += 1
    let retryBudget: Int
    if resetRetryBudget {
      retryBudget = AppManagedSparkleUserDriverPolicy.silentFailureRetryBudget(for: mode)
    } else if let remainingSilentRetries {
      retryBudget = remainingSilentRetries
    } else {
      retryBudget = session.remainingSilentRetries
    }
    session = AppManagedSparkleUpdateSession(
      id: nextSessionID,
      mode: mode,
      phase: .checking,
      remainingSilentRetries: retryBudget
    )
  }

  private func confirmInstall(_ appcastItem: SUAppcastItem) -> Bool {
    let alert = NSAlert()
    alert.messageText = policy.optionalPromptTitle
    alert.informativeText = policy.optionalPromptBody(displayVersion: appcastItem.displayVersionString)
    alert.addButton(withTitle: "Update")
    alert.addButton(withTitle: "Later")
    NSApp.activate(ignoringOtherApps: true)
    return alert.runModal() == .alertFirstButtonReturn
  }

  private func presentAlert(title: String, message: String) {
    let alert = NSAlert()
    alert.messageText = title
    alert.informativeText = message
    alert.addButton(withTitle: "OK")
    NSApp.activate(ignoringOtherApps: true)
    alert.runModal()
  }

  private func presentFailureAlert(title: String, message: String) -> Bool {
    let alert = NSAlert()
    alert.messageText = title
    alert.informativeText = message
    for buttonTitle in AppManagedSparkleUserDriverPolicy.failureAlertButtonTitles {
      alert.addButton(withTitle: buttonTitle)
    }
    NSApp.activate(ignoringOtherApps: true)
    return alert.runModal() == .alertFirstButtonReturn
  }
}
