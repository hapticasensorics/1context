import AppKit
import Foundation
import OneContextCore
import OneContextUpdate
import Sparkle

@MainActor
public final class SparkleUpdateController: NSObject {
  private let configuration: SparkleUpdaterConfiguration
  private let appContext: NativeUpdaterAppContext
  private var updater: SPUUpdater?
  private var userDriver: AppManagedSparkleUserDriver?
  private var observation: SparkleFrameworkObservation?

  public var onUpdateInformationChanged: (() -> Void)?

  public init(
    configuration: SparkleUpdaterConfiguration = .current(),
    appContext: NativeUpdaterAppContext = .current(),
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
    updater.checkForUpdates()
    return true
  }

  @discardableResult
  public func probeForUpdateInformation() -> Bool {
    guard let updater, updater.canCheckForUpdates else {
      return false
    }
    updater.checkForUpdateInformation()
    return true
  }

  public func snapshot(currentVersion: String = oneContextVersion) async -> NativeUpdateSnapshot {
    await SparkleNativeUpdater(
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
      userDriver?.presentFailureIfAppropriate(for: error)
    }
    userDriver?.finishUpdateSession()
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

struct AppManagedSparkleUserDriverPolicy {
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

  static func shouldPresentFailure(
    mode: AppManagedSparkleCheckMode,
    attemptedInstall: Bool
  ) -> Bool {
    mode == .automaticMandatory || mode == .userInitiated || attemptedInstall
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
}

@MainActor
private final class AppManagedSparkleUserDriver: NSObject, SPUUserDriver {
  private let policy: UpdateUserFacingPolicy
  private var mode: AppManagedSparkleCheckMode = .automaticMandatory
  private var shouldInstallAndRelaunchWithoutPrompt = false
  private var didPresentFailure = false

  init(policy: UpdateUserFacingPolicy) {
    self.policy = policy
    super.init()
  }

  func prepareForMandatoryAutomaticCheck() {
    mode = .automaticMandatory
    shouldInstallAndRelaunchWithoutPrompt = false
    didPresentFailure = false
  }

  func prepareForUserInitiatedCheck() {
    mode = .userInitiated
    shouldInstallAndRelaunchWithoutPrompt = false
    didPresentFailure = false
  }

  func finishUpdateSession() {
    mode = .automaticMandatory
    shouldInstallAndRelaunchWithoutPrompt = false
    didPresentFailure = false
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
      mode: mode,
      isCriticalUpdate: appcastItem.isCriticalUpdate,
      isInformationOnlyUpdate: appcastItem.isInformationOnlyUpdate
    ) {
    case .installWithoutPrompt:
      shouldInstallAndRelaunchWithoutPrompt = true
      reply(.install)
    case .askUser:
      if confirmInstall(appcastItem) {
        shouldInstallAndRelaunchWithoutPrompt = true
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
    if mode == .userInitiated {
      presentAlert(title: "1Context is up to date.", message: "")
    }
  }

  func showUpdaterError(_ error: Error) async {
    presentFailureIfAppropriate(for: error)
  }

  func showDownloadInitiated(cancellation: @escaping () -> Void) {}

  func showDownloadDidReceiveExpectedContentLength(_ expectedContentLength: UInt64) {}

  func showDownloadDidReceiveData(ofLength length: UInt64) {}

  func showDownloadDidStartExtractingUpdate() {}

  func showExtractionReceivedProgress(_ progress: Double) {}

  func showReady(toInstallAndRelaunch reply: @escaping (SPUUserUpdateChoice) -> Void) {
    reply(shouldInstallAndRelaunchWithoutPrompt ? .install : .dismiss)
  }

  func showInstallingUpdate(
    withApplicationTerminated applicationTerminated: Bool,
    retryTerminatingApplication: @escaping () -> Void
  ) {}

  func showUpdateInstalledAndRelaunched(_ relaunched: Bool) async {
    guard policy.postInstallMessageEnabled else { return }
    presentAlert(
      title: policy.postInstallTitle,
      message: policy.postInstallBody(displayVersion: oneContextVersion)
    )
  }

  func dismissUpdateInstallation() {
    shouldInstallAndRelaunchWithoutPrompt = false
  }

  func showUpdateInFocus() {}

  func presentFailureIfAppropriate(for error: Error) {
    guard !didPresentFailure else { return }
    guard AppManagedSparkleUserDriverPolicy.shouldPresentFailure(
      for: error,
      mode: mode,
      attemptedInstall: shouldInstallAndRelaunchWithoutPrompt
    ) else {
      return
    }
    didPresentFailure = true
    presentAlert(title: policy.failureTitle, message: policy.failureBody)
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
}
