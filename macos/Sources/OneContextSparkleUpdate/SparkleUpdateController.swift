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
  private let retrySleeper: @Sendable (UInt64) async -> Void
  private var updater: SPUUpdater?
  private var userDriver: AppManagedSparkleUserDriver?
  private var observation: SparkleFrameworkObservation?

  public var onUpdateInformationChanged: (() -> Void)?

  public init(
    configuration: SparkleUpdaterConfiguration = .current(),
    appContext: AppUpdateContext = .current(),
    startUpdater: Bool = true,
    retrySleeper: @escaping @Sendable (UInt64) async -> Void = { nanoseconds in
      try? await Task.sleep(nanoseconds: nanoseconds)
    }
  ) {
    self.configuration = configuration
    self.appContext = appContext
    self.retrySleeper = retrySleeper
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
    executeControllerEffects(userDriver?.prepareForUserInitiatedCheck() ?? [])
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
    executeControllerEffects(userDriver?.prepareForMandatoryAutomaticCheck() ?? [])
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

  private func executeControllerEffects(_ effects: [AppManagedSparkleUpdateEffect]) {
    for effect in effects {
      switch effect {
      case .clearObservation:
        observation = nil
        onUpdateInformationChanged?()
      case .checkForeground:
        guard let updater, updater.canCheckForUpdates else { continue }
        updater.checkForUpdates()
      case .checkBackground:
        guard let updater else { continue }
        configureAutomaticUpdateDefaults()
        guard updater.automaticallyChecksForUpdates, updater.automaticallyDownloadsUpdates else {
          continue
        }
        updater.checkForUpdatesInBackground()
      case .scheduleRetry(let retryRequest):
        let retrySleeper = retrySleeper
        Task { @MainActor [weak self] in
          await retrySleeper(retryRequest.delayNanoseconds)
          self?.retryUpdateCheck(retryRequest)
        }
      case .askUser, .install, .dismiss, .showSupportAlert, .showUpToDate:
        continue
      }
    }
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
      executeControllerEffects([.scheduleRetry(retryRequest)])
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
    guard let effects = userDriver?.prepareForFailureRetry(retryRequest) else {
      return
    }
    executeControllerEffects(effects)
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
  let attemptNumber: Int
  let remainingBudget: Int
  let delayNanoseconds: UInt64

  var remainingSilentRetries: Int {
    remainingBudget
  }
}

struct AppManagedSparkleUpdateFailure: Equatable {
  let domain: String
  let code: Int
  let localizedDescription: String

  init(_ error: Error) {
    let nsError = error as NSError
    self.domain = nsError.domain
    self.code = nsError.code
    self.localizedDescription = nsError.localizedDescription
  }
}

enum AppManagedSparkleUpdateEvent: Equatable {
  case start(mode: AppManagedSparkleCheckMode)
  case retryTimerFired(AppManagedSparkleFailureRetryRequest)
  case observedUpdateMetadata(isCriticalUpdate: Bool)
  case foundUpdate(isCriticalUpdate: Bool, isInformationOnlyUpdate: Bool)
  case userAcceptedUpdate
  case userDismissedUpdate
  case didNotFindUpdate
  case downloadStarted
  case readyToInstall
  case installing
  case checkFailed(AppManagedSparkleUpdateFailure)
  case updateAttemptFailed(AppManagedSparkleUpdateFailure)
  case userRequestedRetry(sessionID: UInt64)
  case finished
}

enum AppManagedSparkleUpdateEffect: Equatable {
  case checkForeground
  case checkBackground
  case askUser
  case install
  case dismiss
  case scheduleRetry(AppManagedSparkleFailureRetryRequest)
  case showSupportAlert(sessionID: UInt64)
  case showUpToDate
  case clearObservation
}

private extension AppManagedSparkleUpdateEvent {
  var logDescription: String {
    switch self {
    case .start(let mode):
      return "start(mode:\(mode))"
    case .retryTimerFired(let request):
      return "retryTimerFired(sourceSessionID:\(request.sourceSessionID),kind:\(request.kind),attempt:\(request.attemptNumber),remainingBudget:\(request.remainingBudget))"
    case .observedUpdateMetadata(let isCriticalUpdate):
      return "observedUpdateMetadata(critical:\(isCriticalUpdate))"
    case .foundUpdate(let isCriticalUpdate, let isInformationOnlyUpdate):
      return "foundUpdate(critical:\(isCriticalUpdate),informationOnly:\(isInformationOnlyUpdate))"
    case .userAcceptedUpdate:
      return "userAcceptedUpdate"
    case .userDismissedUpdate:
      return "userDismissedUpdate"
    case .didNotFindUpdate:
      return "didNotFindUpdate"
    case .downloadStarted:
      return "downloadStarted"
    case .readyToInstall:
      return "readyToInstall"
    case .installing:
      return "installing"
    case .checkFailed(let failure):
      return "checkFailed(domain:\(failure.domain),code:\(failure.code))"
    case .updateAttemptFailed(let failure):
      return "updateAttemptFailed(domain:\(failure.domain),code:\(failure.code))"
    case .userRequestedRetry(let sessionID):
      return "userRequestedRetry(sessionID:\(sessionID))"
    case .finished:
      return "finished"
    }
  }
}

private extension AppManagedSparkleUpdateEffect {
  var logDescription: String {
    switch self {
    case .checkForeground:
      return "checkForeground"
    case .checkBackground:
      return "checkBackground"
    case .askUser:
      return "askUser"
    case .install:
      return "install"
    case .dismiss:
      return "dismiss"
    case .scheduleRetry(let request):
      return "scheduleRetry(sourceSessionID:\(request.sourceSessionID),kind:\(request.kind),attempt:\(request.attemptNumber),remainingBudget:\(request.remainingBudget))"
    case .showSupportAlert(let sessionID):
      return "showSupportAlert(sessionID:\(sessionID))"
    case .showUpToDate:
      return "showUpToDate"
    case .clearObservation:
      return "clearObservation"
    }
  }
}

struct AppManagedSparkleUpdateSession: Equatable {
  let id: UInt64
  var mode: AppManagedSparkleCheckMode
  var phase: AppManagedSparkleUpdatePhase
  var retryAttemptNumber: Int
  var remainingRetryBudget: Int
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

  var remainingSilentRetries: Int {
    get { remainingRetryBudget }
    set { remainingRetryBudget = newValue }
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

struct AppManagedSparkleUpdateState: Equatable {
  var nextSessionID: UInt64
  var session: AppManagedSparkleUpdateSession

  static let initial = AppManagedSparkleUpdateState(
    nextSessionID: 0,
    session: AppManagedSparkleUpdateSession(
      id: 0,
      mode: .automaticMandatory,
      phase: .finished,
      retryAttemptNumber: 0,
      remainingRetryBudget: AppManagedSparkleUserDriverPolicy.silentFailureRetryBudget(
        for: .automaticMandatory
      )
    )
  )
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
    shouldPresentFailure(
      for: AppManagedSparkleUpdateFailure(error),
      mode: mode,
      attemptedInstall: attemptedInstall
    )
  }

  static func shouldPresentFailure(
    for failure: AppManagedSparkleUpdateFailure,
    mode: AppManagedSparkleCheckMode,
    attemptedInstall: Bool
  ) -> Bool {
    if failure.domain == SUSparkleErrorDomain {
      switch failure.code {
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
    failureDisposition(
      for: AppManagedSparkleUpdateFailure(error),
      mode: mode,
      attemptedInstall: attemptedInstall,
      remainingSilentRetries: remainingSilentRetries
    )
  }

  static func failureDisposition(
    for failure: AppManagedSparkleUpdateFailure,
    mode: AppManagedSparkleCheckMode,
    attemptedInstall: Bool,
    remainingSilentRetries: Int
  ) -> AppManagedSparkleFailureDisposition {
    if isBenignSparkleFailure(failure) {
      return .ignore
    }
    if mode == .automaticMandatory, !attemptedInstall {
      return silentRetryDispositionOrIgnore(
        for: mode,
        remainingSilentRetries: remainingSilentRetries
      )
    }
    guard shouldPresentFailure(
      for: failure,
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
    isBenignSparkleFailure(AppManagedSparkleUpdateFailure(error))
  }

  private static func isBenignSparkleFailure(_ failure: AppManagedSparkleUpdateFailure) -> Bool {
    if failure.domain == SUSparkleErrorDomain {
      switch failure.code {
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

enum AppManagedSparkleUpdateReducer {
  static func reduce(
    state: AppManagedSparkleUpdateState,
    event: AppManagedSparkleUpdateEvent
  ) -> (state: AppManagedSparkleUpdateState, effects: [AppManagedSparkleUpdateEffect]) {
    var state = state
    var effects: [AppManagedSparkleUpdateEffect] = []

    switch event {
    case .start(let mode):
      state.startSession(
        mode: mode,
        retryAttemptNumber: 0,
        remainingRetryBudget: AppManagedSparkleUserDriverPolicy.silentFailureRetryBudget(for: mode)
      )
      effects.append(.clearObservation)
      effects.append(checkEffect(for: mode))

    case .retryTimerFired(let retryRequest):
      guard state.session.id == retryRequest.sourceSessionID else {
        return (state, [])
      }
      let retryBudget: Int
      switch retryRequest.kind {
      case .silent:
        retryBudget = retryRequest.remainingBudget
      case .userRequested:
        retryBudget = AppManagedSparkleUserDriverPolicy.silentFailureRetryBudget(
          for: retryRequest.mode
        )
      }
      state.startSession(
        mode: retryRequest.mode,
        retryAttemptNumber: retryRequest.attemptNumber,
        remainingRetryBudget: retryBudget
      )
      effects.append(.clearObservation)
      effects.append(checkEffect(for: retryRequest.mode))

    case .observedUpdateMetadata(let isCriticalUpdate):
      state.session.recordUpdateFound(isCriticalUpdate: isCriticalUpdate)

    case .foundUpdate(let isCriticalUpdate, let isInformationOnlyUpdate):
      state.session.recordUpdateFound(isCriticalUpdate: isCriticalUpdate)
      switch AppManagedSparkleUserDriverPolicy.decision(
        mode: state.session.mode,
        isCriticalUpdate: isCriticalUpdate,
        isInformationOnlyUpdate: isInformationOnlyUpdate
      ) {
      case .installWithoutPrompt:
        state.session.recordReadyToInstall()
        effects.append(.install)
      case .askUser:
        effects.append(.askUser)
      case .dismiss:
        effects.append(.dismiss)
      }

    case .userAcceptedUpdate:
      state.session.recordReadyToInstall()
      effects.append(.install)

    case .userDismissedUpdate:
      effects.append(.dismiss)

    case .didNotFindUpdate:
      if state.session.mode == .userInitiated {
        effects.append(.showUpToDate)
      }

    case .downloadStarted:
      state.session.recordDownloadStarted()

    case .readyToInstall:
      let shouldInstall = state.session.installWasOffered || state.session.criticalUpdateWasFound
      if shouldInstall {
        state.session.recordReadyToInstall()
      } else {
        debugAssertIllegalTransition("Sparkle reported ready-to-install before an install was offered.")
      }
      effects.append(shouldInstall ? .install : .dismiss)

    case .installing:
      state.session.recordInstalling()

    case .checkFailed(let failure):
      handleFailure(
        failure,
        representsUpdateAttempt: false,
        state: &state,
        effects: &effects
      )

    case .updateAttemptFailed(let failure):
      handleFailure(
        failure,
        representsUpdateAttempt: true,
        state: &state,
        effects: &effects
      )

    case .userRequestedRetry(let sessionID):
      guard state.session.id == sessionID else {
        return (state, [])
      }
      let retryRequest = AppManagedSparkleFailureRetryRequest(
        sourceSessionID: state.session.id,
        mode: state.session.mode,
        kind: .userRequested,
        attemptNumber: state.session.retryAttemptNumber + 1,
        remainingBudget: AppManagedSparkleUserDriverPolicy.silentFailureRetryBudget(
          for: state.session.mode
        ),
        delayNanoseconds: 250_000_000
      )
      state.session.retryRequest = retryRequest
      effects.append(.scheduleRetry(retryRequest))

    case .finished:
      if state.session.retryRequest == nil, state.session.phase != .failed {
        state.session.recordFinished()
      }
    }

    return (state, effects)
  }

  private static func handleFailure(
    _ failure: AppManagedSparkleUpdateFailure,
    representsUpdateAttempt: Bool,
    state: inout AppManagedSparkleUpdateState,
    effects: inout [AppManagedSparkleUpdateEffect]
  ) {
    guard !state.session.didHandleFailure, !state.session.didPresentFailure else {
      return
    }
    state.session.didHandleFailure = true
    let disposition = AppManagedSparkleUserDriverPolicy.failureDisposition(
      for: failure,
      mode: state.session.mode,
      attemptedInstall: representsUpdateAttempt,
      remainingSilentRetries: state.session.remainingRetryBudget
    )
    switch disposition {
    case .ignore:
      break
    case .silentRetry(let remainingRetries, let delaySeconds):
      state.session.recordFailed()
      state.session.remainingRetryBudget = remainingRetries
      let retryRequest = AppManagedSparkleFailureRetryRequest(
        sourceSessionID: state.session.id,
        mode: state.session.mode,
        kind: .silent,
        attemptNumber: state.session.retryAttemptNumber + 1,
        remainingBudget: remainingRetries,
        delayNanoseconds: AppManagedSparkleUserDriverPolicy.retryDelayNanoseconds(
          seconds: delaySeconds
        )
      )
      state.session.retryRequest = retryRequest
      effects.append(.scheduleRetry(retryRequest))
    case .presentFailure:
      state.session.recordFailed()
      state.session.didPresentFailure = true
      effects.append(.showSupportAlert(sessionID: state.session.id))
    }
  }

  private static func checkEffect(for mode: AppManagedSparkleCheckMode) -> AppManagedSparkleUpdateEffect {
    switch AppManagedSparkleUserDriverPolicy.checkInvocation(for: mode) {
    case .foreground:
      return .checkForeground
    case .background:
      return .checkBackground
    }
  }

  private static func debugAssertIllegalTransition(_ message: String) {
    #if DEBUG
      assertionFailure(message)
    #endif
  }
}

private extension AppManagedSparkleUpdateState {
  mutating func startSession(
    mode: AppManagedSparkleCheckMode,
    retryAttemptNumber: Int,
    remainingRetryBudget: Int
  ) {
    nextSessionID += 1
    session = AppManagedSparkleUpdateSession(
      id: nextSessionID,
      mode: mode,
      phase: .checking,
      retryAttemptNumber: retryAttemptNumber,
      remainingRetryBudget: remainingRetryBudget
    )
  }
}

struct AppManagedSparkleTransitionEvidence: Equatable {
  let sessionID: UInt64
  let oldPhase: AppManagedSparkleUpdatePhase
  let event: String
  let newPhase: AppManagedSparkleUpdatePhase
  let effects: [String]
}

@MainActor
final class AppManagedSparkleUserDriver: NSObject, SPUUserDriver {
  private let policy: UpdateUserFacingPolicy
  private var reducerState = AppManagedSparkleUpdateState.initial
  private(set) var transitionEvidence: [AppManagedSparkleTransitionEvidence] = []

  var currentCycleRepresentsUpdateAttempt: Bool {
    reducerState.session.failureRepresentsUpdateAttempt
  }

  var currentSession: AppManagedSparkleUpdateSession {
    reducerState.session
  }

  init(policy: UpdateUserFacingPolicy) {
    self.policy = policy
    super.init()
  }

  @discardableResult
  func prepareForMandatoryAutomaticCheck() -> [AppManagedSparkleUpdateEffect] {
    apply(.start(mode: .automaticMandatory))
  }

  @discardableResult
  func prepareForUserInitiatedCheck() -> [AppManagedSparkleUpdateEffect] {
    apply(.start(mode: .userInitiated))
  }

  func recordUpdateFound(isCriticalUpdate: Bool) {
    apply(.observedUpdateMetadata(isCriticalUpdate: isCriticalUpdate))
  }

  func recordUpdateInstallationWillBegin() {
    apply(.observedUpdateMetadata(isCriticalUpdate: true))
    apply(.installing)
  }

  func prepareForFailureRetry(
    _ retryRequest: AppManagedSparkleFailureRetryRequest
  ) -> [AppManagedSparkleUpdateEffect]? {
    let effects = apply(.retryTimerFired(retryRequest))
    return effects.isEmpty ? nil : effects
  }

  func finishUpdateSessionAndConsumeRetryRequest() -> AppManagedSparkleFailureRetryRequest? {
    let retryRequest = reducerState.session.retryRequest
    if retryRequest != nil {
      reducerState.session.retryRequest = nil
      return retryRequest
    }
    apply(.finished)
    return nil
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
    let effects = apply(.foundUpdate(
      isCriticalUpdate: appcastItem.isCriticalUpdate,
      isInformationOnlyUpdate: appcastItem.isInformationOnlyUpdate
    ))
    replyWithEffects(effects, appcastItem: appcastItem, reply: reply)
  }

  func showUpdateReleaseNotes(with downloadData: SPUDownloadData) {
    guard policy.showReleaseNotesInUpdateWindow else { return }
  }

  func showUpdateReleaseNotesFailedToDownloadWithError(_ error: Error) {}

  func showUpdateNotFoundWithError(_ error: Error) async {
    executeUserFacingEffects(apply(.didNotFindUpdate))
  }

  func showUpdaterError(_ error: Error) async {
    handleFailureIfAppropriate(for: error)
  }

  func showDownloadInitiated(cancellation: @escaping () -> Void) {
    apply(.downloadStarted)
  }

  func showDownloadDidReceiveExpectedContentLength(_ expectedContentLength: UInt64) {}

  func showDownloadDidReceiveData(ofLength length: UInt64) {}

  func showDownloadDidStartExtractingUpdate() {
    apply(.downloadStarted)
  }

  func showExtractionReceivedProgress(_ progress: Double) {}

  func showReady(toInstallAndRelaunch reply: @escaping (SPUUserUpdateChoice) -> Void) {
    replyWithEffects(apply(.readyToInstall), appcastItem: nil, reply: reply)
  }

  func showInstallingUpdate(
    withApplicationTerminated applicationTerminated: Bool,
    retryTerminatingApplication: @escaping () -> Void
  ) {
    apply(.installing)
  }

  func showUpdateInstalledAndRelaunched(_ relaunched: Bool) async {
    apply(.finished)
    guard policy.postInstallMessageEnabled else { return }
    presentAlert(
      title: policy.postInstallTitle,
      message: policy.postInstallBody(displayVersion: oneContextVersion)
    )
  }

  func dismissUpdateInstallation() {
    apply(.finished)
  }

  func showUpdateInFocus() {}

  func handleFailureIfAppropriate(for error: Error) {
    let event: AppManagedSparkleUpdateEvent = currentCycleRepresentsUpdateAttempt
      ? .updateAttemptFailed(AppManagedSparkleUpdateFailure(error))
      : .checkFailed(AppManagedSparkleUpdateFailure(error))
    executeUserFacingEffects(apply(event))
  }

  private func replyWithEffects(
    _ effects: [AppManagedSparkleUpdateEffect],
    appcastItem: SUAppcastItem?,
    reply: @escaping (SPUUserUpdateChoice) -> Void
  ) {
    for effect in effects {
      switch effect {
      case .install:
        reply(.install)
        return
      case .dismiss:
        reply(.dismiss)
        return
      case .askUser:
        guard let appcastItem else {
          reply(.dismiss)
          return
        }
        if confirmInstall(appcastItem) {
          replyWithEffects(apply(.userAcceptedUpdate), appcastItem: appcastItem, reply: reply)
        } else {
          replyWithEffects(apply(.userDismissedUpdate), appcastItem: appcastItem, reply: reply)
        }
        return
      case .checkForeground, .checkBackground, .scheduleRetry, .showSupportAlert,
           .showUpToDate, .clearObservation:
        continue
      }
    }
    reply(.dismiss)
  }

  private func executeUserFacingEffects(_ effects: [AppManagedSparkleUpdateEffect]) {
    for effect in effects {
      switch effect {
      case .showUpToDate:
        presentAlert(title: "1Context is up to date.", message: "")
      case .showSupportAlert(let sessionID):
        if presentFailureAlert(title: policy.failureTitle, message: policy.failureBody) {
          _ = apply(.userRequestedRetry(sessionID: sessionID))
        }
      case .checkForeground, .checkBackground, .askUser, .install, .dismiss, .scheduleRetry,
           .clearObservation:
        continue
      }
    }
  }

  @discardableResult
  private func apply(_ event: AppManagedSparkleUpdateEvent) -> [AppManagedSparkleUpdateEffect] {
    let oldPhase = reducerState.session.phase
    let result = AppManagedSparkleUpdateReducer.reduce(state: reducerState, event: event)
    reducerState = result.state
    let effectDescriptions = result.effects.map(\.logDescription)
    let evidence = AppManagedSparkleTransitionEvidence(
      sessionID: reducerState.session.id,
      oldPhase: oldPhase,
      event: event.logDescription,
      newPhase: reducerState.session.phase,
      effects: effectDescriptions
    )
    transitionEvidence.append(evidence)
    sparkleUpdateLog.info(
      "Sparkle reducer transition sessionID=\(evidence.sessionID, privacy: .public) oldPhase=\(String(describing: evidence.oldPhase), privacy: .public) event=\(evidence.event, privacy: .public) newPhase=\(String(describing: evidence.newPhase), privacy: .public) effects=\(effectDescriptions.joined(separator: ","), privacy: .public) retryAttempt=\(self.reducerState.session.retryAttemptNumber, privacy: .public) remainingBudget=\(self.reducerState.session.remainingRetryBudget, privacy: .public)"
    )
    return result.effects
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
