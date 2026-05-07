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
}
