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

  func testRealUpdateErrorsShowControlledFailureAlert() {
    for code in [1000, 2001] {
      let error = NSError(domain: "SUSparkleErrorDomain", code: code)

      XCTAssertTrue(AppManagedSparkleUserDriverPolicy.shouldPresentFailure(
        for: error,
        mode: .automaticMandatory,
        attemptedInstall: false
      ))
    }
  }
}
