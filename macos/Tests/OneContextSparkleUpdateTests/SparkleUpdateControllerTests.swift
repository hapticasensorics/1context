import XCTest
@testable import OneContextSparkleUpdate

final class SparkleUpdateControllerTests: XCTestCase {
  func testCriticalUpdatesInstallWithoutPrompt() {
    let decision = AppManagedSparkleUserDriverPolicy.decision(
      mode: .automaticMandatory,
      isCriticalUpdate: true,
      isInformationOnlyUpdate: false
    )

    XCTAssertEqual(decision, .installWithoutPrompt)
  }

  func testAutomaticChecksDismissNonMandatoryUpdates() {
    let decision = AppManagedSparkleUserDriverPolicy.decision(
      mode: .automaticMandatory,
      isCriticalUpdate: false,
      isInformationOnlyUpdate: false
    )

    XCTAssertEqual(decision, .dismiss)
  }

  func testManualChecksCanAskForNonMandatoryUpdates() {
    let decision = AppManagedSparkleUserDriverPolicy.decision(
      mode: .userInitiated,
      isCriticalUpdate: false,
      isInformationOnlyUpdate: false
    )

    XCTAssertEqual(decision, .askUser)
  }

  func testInformationOnlyUpdatesAreNeverAutoInstalled() {
    let decision = AppManagedSparkleUserDriverPolicy.decision(
      mode: .automaticMandatory,
      isCriticalUpdate: true,
      isInformationOnlyUpdate: true
    )

    XCTAssertEqual(decision, .dismiss)
  }

  func testCheckInvocationMatchesUserIntent() {
    XCTAssertEqual(
      AppManagedSparkleUserDriverPolicy.checkInvocation(for: .automaticMandatory),
      .background
    )
    XCTAssertEqual(
      AppManagedSparkleUserDriverPolicy.checkInvocation(for: .userInitiated),
      .foreground
    )
  }

  func testRetryDelayOverrideSupportsShortSmokeHorizons() {
    XCTAssertEqual(
      AppManagedSparkleUserDriverPolicy.silentFailureRetryDelays(
        for: .automaticMandatory,
        environment: ["ONECONTEXT_SPARKLE_AUTOMATIC_RETRY_DELAYS_SECONDS": "0.1, 0.2, 0.3"]
      ),
      [0.1, 0.2, 0.3]
    )
    XCTAssertEqual(
      AppManagedSparkleUserDriverPolicy.silentFailureRetryDelays(
        for: .userInitiated,
        environment: ["ONECONTEXT_SPARKLE_MANUAL_RETRY_DELAYS_SECONDS": "0"]
      ),
      [0]
    )
    XCTAssertEqual(
      AppManagedSparkleUserDriverPolicy.silentFailureRetryDelays(
        for: .automaticMandatory,
        environment: ["ONECONTEXT_SPARKLE_AUTOMATIC_RETRY_DELAYS_SECONDS": "not-a-delay"]
      ),
      [15, 90, 300]
    )
  }

  func testNoUpdateErrorsDoNotShowFailureAlert() {
    let error = NSError(domain: "SUSparkleErrorDomain", code: 1001)

    XCTAssertFalse(AppManagedSparkleUserDriverPolicy.shouldPresentFailure(
      for: error,
      mode: .automaticMandatory,
      attemptedInstall: false
    ))
  }

  func testCancelledInstallErrorsDoNotShowFailureAlert() {
    for code in [4007, 4008] {
      let error = NSError(domain: "SUSparkleErrorDomain", code: code)

      XCTAssertFalse(AppManagedSparkleUserDriverPolicy.shouldPresentFailure(
        for: error,
        mode: .userInitiated,
        attemptedInstall: true
      ))
    }
  }

  func testAutomaticMandatoryFailuresUseMultipleSilentRetriesBeforeAlert() {
    let error = NSError(domain: "SUSparkleErrorDomain", code: 2001)
    var remainingRetries = AppManagedSparkleUserDriverPolicy.silentFailureRetryBudget(
      for: .automaticMandatory
    )
    var observedDelays: [TimeInterval] = []

    XCTAssertGreaterThanOrEqual(remainingRetries, 2)

    while remainingRetries > 0 {
      let disposition = AppManagedSparkleUserDriverPolicy.failureDisposition(
        for: error,
        mode: .automaticMandatory,
        attemptedInstall: true,
        remainingSilentRetries: remainingRetries
      )
      guard case .silentRetry(let nextRemainingRetries, let delaySeconds) = disposition else {
        XCTFail("Expected silent retry, got \(disposition)")
        return
      }
      XCTAssertEqual(nextRemainingRetries, remainingRetries - 1)
      observedDelays.append(delaySeconds)
      remainingRetries -= 1
    }

    XCTAssertEqual(observedDelays, [15, 90, 300])
    XCTAssertEqual(
      AppManagedSparkleUserDriverPolicy.failureDisposition(
        for: error,
        mode: .automaticMandatory,
        attemptedInstall: true,
        remainingSilentRetries: remainingRetries
      ),
      .presentFailure
    )
  }

  func testAutomaticCheckFailuresWithoutAttemptedInstallRetryThenStayQuiet() {
    let error = NSError(domain: "SUSparkleErrorDomain", code: 2001)
    var remainingRetries = AppManagedSparkleUserDriverPolicy.silentFailureRetryBudget(
      for: .automaticMandatory
    )
    var observedDelays: [TimeInterval] = []

    while remainingRetries > 0 {
      let disposition = AppManagedSparkleUserDriverPolicy.failureDisposition(
        for: error,
        mode: .automaticMandatory,
        attemptedInstall: false,
        remainingSilentRetries: remainingRetries
      )
      guard case .silentRetry(let nextRemainingRetries, let delaySeconds) = disposition else {
        XCTFail("Expected silent retry, got \(disposition)")
        return
      }
      XCTAssertEqual(nextRemainingRetries, remainingRetries - 1)
      observedDelays.append(delaySeconds)
      remainingRetries -= 1
    }

    XCTAssertEqual(observedDelays, [15, 90, 300])
    XCTAssertEqual(
      AppManagedSparkleUserDriverPolicy.failureDisposition(
        for: error,
        mode: .automaticMandatory,
        attemptedInstall: false,
        remainingSilentRetries: remainingRetries
      ),
      .ignore
    )
  }

  @MainActor
  func testAutomaticCriticalUpdateFoundCountsAsUpdateAttemptForLaterFailures() {
    let driver = AppManagedSparkleUserDriver(policy: .default)
    driver.prepareForMandatoryAutomaticCheck()

    XCTAssertFalse(driver.currentCycleRepresentsUpdateAttempt)
    XCTAssertEqual(driver.currentSession.phase, .checking)

    driver.recordUpdateFound(isCriticalUpdate: true)

    XCTAssertTrue(driver.currentCycleRepresentsUpdateAttempt)
    XCTAssertTrue(driver.currentSession.updateWasFound)
    XCTAssertTrue(driver.currentSession.criticalUpdateWasFound)
    XCTAssertEqual(driver.currentSession.phase, .updateFound)
    XCTAssertEqual(
      AppManagedSparkleUserDriverPolicy.failureDisposition(
        for: NSError(domain: "SUSparkleErrorDomain", code: 2001),
        mode: .automaticMandatory,
        attemptedInstall: driver.currentCycleRepresentsUpdateAttempt,
        remainingSilentRetries: 0
      ),
      .presentFailure
    )
  }

  @MainActor
  func testAutomaticOptionalUpdateFoundDoesNotTurnCheckFailureIntoFailedInstall() {
    let driver = AppManagedSparkleUserDriver(policy: .default)
    driver.prepareForMandatoryAutomaticCheck()
    driver.recordUpdateFound(isCriticalUpdate: false)

    XCTAssertFalse(driver.currentCycleRepresentsUpdateAttempt)
    XCTAssertTrue(driver.currentSession.updateWasFound)
    XCTAssertFalse(driver.currentSession.criticalUpdateWasFound)
    XCTAssertEqual(
      AppManagedSparkleUserDriverPolicy.failureDisposition(
        for: NSError(domain: "SUSparkleErrorDomain", code: 2001),
        mode: .automaticMandatory,
        attemptedInstall: driver.currentCycleRepresentsUpdateAttempt,
        remainingSilentRetries: 0
      ),
      .ignore
    )
  }

  @MainActor
  func testSilentRetryKeepsBudgetAndStartsFreshSession() throws {
    let driver = AppManagedSparkleUserDriver(policy: .default)
    driver.prepareForMandatoryAutomaticCheck()
    let sourceSessionID = driver.currentSession.id
    let startingBudget = driver.currentSession.remainingSilentRetries

    driver.handleFailureIfAppropriate(for: NSError(domain: "SUSparkleErrorDomain", code: 2001))
    let retryRequest = driver.finishUpdateSessionAndConsumeRetryRequest()

    XCTAssertNotNil(retryRequest)
    XCTAssertEqual(retryRequest?.sourceSessionID, sourceSessionID)
    XCTAssertEqual(retryRequest?.remainingSilentRetries, startingBudget - 1)
    XCTAssertEqual(driver.currentSession.phase, .failed)

    XCTAssertEqual(driver.prepareForFailureRetry(try XCTUnwrap(retryRequest)), true)
    XCTAssertNotEqual(driver.currentSession.id, sourceSessionID)
    XCTAssertEqual(driver.currentSession.mode, .automaticMandatory)
    XCTAssertEqual(driver.currentSession.phase, .checking)
    XCTAssertEqual(driver.currentSession.remainingSilentRetries, startingBudget - 1)
  }

  @MainActor
  func testStaleRetryCannotOverwriteNewerSession() throws {
    let driver = AppManagedSparkleUserDriver(policy: .default)
    driver.prepareForMandatoryAutomaticCheck()
    driver.handleFailureIfAppropriate(for: NSError(domain: "SUSparkleErrorDomain", code: 2001))
    let retryRequest = try XCTUnwrap(driver.finishUpdateSessionAndConsumeRetryRequest())

    driver.prepareForUserInitiatedCheck()

    XCTAssertFalse(driver.prepareForFailureRetry(retryRequest))
    XCTAssertEqual(driver.currentSession.mode, .userInitiated)
    XCTAssertEqual(driver.currentSession.phase, .checking)
  }

  @MainActor
  func testUserRequestedRetryResetsRetryBudget() {
    let driver = AppManagedSparkleUserDriver(policy: .default)
    driver.prepareForUserInitiatedCheck()
    let retryRequest = AppManagedSparkleFailureRetryRequest(
      sourceSessionID: driver.currentSession.id,
      mode: .userInitiated,
      kind: .userRequested,
      remainingSilentRetries: 0,
      delayNanoseconds: 0
    )

    XCTAssertTrue(driver.prepareForFailureRetry(retryRequest))
    XCTAssertEqual(driver.currentSession.mode, .userInitiated)
    XCTAssertEqual(
      driver.currentSession.remainingSilentRetries,
      AppManagedSparkleUserDriverPolicy.silentFailureRetryBudget(for: .userInitiated)
    )
  }

  @MainActor
  func testUpdateSessionTracksDownloadAndInstallPhases() {
    let driver = AppManagedSparkleUserDriver(policy: .default)
    driver.prepareForMandatoryAutomaticCheck()

    driver.recordUpdateFound(isCriticalUpdate: true)
    driver.showDownloadInitiated(cancellation: {})
    XCTAssertTrue(driver.currentSession.downloadBegan)
    XCTAssertEqual(driver.currentSession.phase, .downloadStarted)

    driver.recordUpdateInstallationWillBegin()
    XCTAssertTrue(driver.currentSession.installBegan)
    XCTAssertEqual(driver.currentSession.phase, .installing)
  }

  func testManualFailuresUseShorterSilentRetryBudget() {
    let automaticBudget = AppManagedSparkleUserDriverPolicy.silentFailureRetryBudget(
      for: .automaticMandatory
    )
    let manualBudget = AppManagedSparkleUserDriverPolicy.silentFailureRetryBudget(
      for: .userInitiated
    )

    XCTAssertGreaterThanOrEqual(manualBudget, 1)
    XCTAssertLessThan(manualBudget, automaticBudget)
    XCTAssertEqual(AppManagedSparkleUserDriverPolicy.silentFailureRetryDelays(for: .userInitiated), [2])
  }

  func testNoUpdateAndCancelFailuresDoNotConsumeSilentRetryBudgetOrAlert() {
    for code in [1001, 4007, 4008] {
      let error = NSError(domain: "SUSparkleErrorDomain", code: code)

      XCTAssertEqual(
        AppManagedSparkleUserDriverPolicy.failureDisposition(
          for: error,
          mode: .automaticMandatory,
          attemptedInstall: true,
          remainingSilentRetries: 3
        ),
        .ignore
      )
    }
  }

  func testFinalFailurePresentationKeepsControlledRetryButton() {
    let error = NSError(domain: "SUSparkleErrorDomain", code: 2001)

    XCTAssertEqual(
      AppManagedSparkleUserDriverPolicy.failureDisposition(
        for: error,
        mode: .userInitiated,
        attemptedInstall: true,
        remainingSilentRetries: 0
      ),
      .presentFailure
    )
    XCTAssertEqual(
      AppManagedSparkleUserDriverPolicy.failureAlertButtonTitles,
      ["Try Again", "OK"]
    )
  }

  func testRealUpdateErrorsShowControlledFailureAlert() {
    for code in [1000, 2001] {
      let error = NSError(domain: "SUSparkleErrorDomain", code: code)

      XCTAssertFalse(AppManagedSparkleUserDriverPolicy.shouldPresentFailure(
        for: error,
        mode: .automaticMandatory,
        attemptedInstall: false
      ))
      XCTAssertTrue(AppManagedSparkleUserDriverPolicy.shouldPresentFailure(
        for: error,
        mode: .automaticMandatory,
        attemptedInstall: true
      ))
      XCTAssertTrue(AppManagedSparkleUserDriverPolicy.shouldPresentFailure(
        for: error,
        mode: .userInitiated,
        attemptedInstall: false
      ))
    }
  }
}
