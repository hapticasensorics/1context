import Foundation
import OneContextCore
import OneContextUpdate
import Sparkle

@MainActor
public final class SparkleUpdateController {
  private let configuration: SparkleUpdaterConfiguration
  private let appContext: NativeUpdaterAppContext
  private let updaterController: SPUStandardUpdaterController?

  public init(
    configuration: SparkleUpdaterConfiguration = .current(),
    appContext: NativeUpdaterAppContext = .current(),
    startUpdater: Bool = true
  ) {
    self.configuration = configuration
    self.appContext = appContext

    if configuration.isConfigured, appContext.location.canInstallAppUpdates {
      self.updaterController = SPUStandardUpdaterController(
        startingUpdater: startUpdater,
        updaterDelegate: nil,
        userDriverDelegate: nil
      )
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

  public func snapshot(currentVersion: String = oneContextVersion) async -> NativeUpdateSnapshot {
    await SparkleNativeUpdater(
      configuration: configuration,
      appContext: appContext,
      driver: SparkleFrameworkStatusDriver(
        canCheckForUpdates: canCheckForUpdates
      )
    ).snapshot(currentVersion: currentVersion)
  }
}

private struct SparkleFrameworkStatusDriver: SparkleUpdateDriver, Sendable {
  let canCheckForUpdates: Bool

  func snapshot(
    currentVersion: String,
    configuration: SparkleUpdaterConfiguration
  ) async -> SparkleUpdateDriverSnapshot {
    SparkleUpdateDriverSnapshot(
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
