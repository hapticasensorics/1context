import Foundation
import OneContextCore
import OneContextUpdate
import Sparkle

@MainActor
public final class SparkleUpdateController: NSObject {
  private let configuration: SparkleUpdaterConfiguration
  private let appContext: NativeUpdaterAppContext
  private var updaterController: SPUStandardUpdaterController?
  private var observation: SparkleFrameworkObservation?

  public var onUpdateInformationChanged: (() -> Void)?

  public init(
    configuration: SparkleUpdaterConfiguration = .current(),
    appContext: NativeUpdaterAppContext = .current(),
    startUpdater: Bool = true
  ) {
    self.configuration = configuration
    self.appContext = appContext
    self.updaterController = nil
    super.init()

    if configuration.isConfigured, appContext.location.canInstallAppUpdates {
      self.updaterController = SPUStandardUpdaterController(
        startingUpdater: startUpdater,
        updaterDelegate: self,
        userDriverDelegate: nil
      )
      configureAutomaticUpdateDefaults()
    } else {
      self.updaterController = nil
    }
  }

  public var canCheckForUpdates: Bool {
    updaterController?.updater.canCheckForUpdates ?? false
  }

  @discardableResult
  public func checkForUpdates(_ sender: Any? = nil) -> Bool {
    guard let updaterController, updaterController.updater.canCheckForUpdates else {
      return false
    }
    updaterController.checkForUpdates(sender)
    return true
  }

  @discardableResult
  public func checkForUpdatesInBackgroundOnLaunch() -> Bool {
    guard let updater = updaterController?.updater else {
      return false
    }
    configureAutomaticUpdateDefaults()
    guard updater.automaticallyChecksForUpdates else {
      return false
    }
    updater.checkForUpdatesInBackground()
    return true
  }

  @discardableResult
  public func probeForUpdateInformation() -> Bool {
    guard let updaterController, updaterController.updater.canCheckForUpdates else {
      return false
    }
    updaterController.updater.checkForUpdateInformation()
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
    guard let updater = updaterController?.updater else { return }
    if configuration.automaticChecksEnabled {
      updater.automaticallyChecksForUpdates = true
    }
    if configuration.automaticDownloadsEnabled, updater.allowsAutomaticUpdates {
      updater.automaticallyDownloadsUpdates = true
    }
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
      if observation.updateAvailable {
        status = observation.mandatoryUpdateAvailable
          ? "1Context \(displayedVersion) is a mandatory update."
          : "1Context \(displayedVersion) is available."
        nextAction = observation.canInstallUpdates
          ? "Install the Sparkle update from the app."
          : "Wait for the current Sparkle update session to finish."
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
