import XCTest

final class DepletedInventoryUITests: XCTestCase {
  func testDepletedBatchOpensHistoryAndHidesMutationAffordances() {
    let app = XCUIApplication()
    app.launchArguments = ["--quartermaster-ui-test-depleted-inventory"]
    app.launch()

    let productRow = app.buttons["inventory.product.11111111-1111-1111-1111-111111111111"]
    XCTAssertTrue(productRow.waitForExistence(timeout: 5))
    XCTAssertTrue(productRow.isHittable)
    XCTAssertFalse(app.staticTexts["500 g matching"].exists)
    XCTAssertFalse(app.staticTexts["500 g total"].exists)
    XCTAssertFalse(app.staticTexts["· 1/2 batches"].exists)
    productRow.tap()

    XCTAssertTrue(app.buttons["batch.consume"].waitForExistence(timeout: 5))

    let activeRow =
      app.descendants(matching: .any)["batch.row.active.33333333-3333-3333-3333-333333333333"]
    XCTAssertTrue(activeRow.waitForExistence(timeout: 5))
    XCTAssertTrue(app.buttons["batch.consume"].exists)
    XCTAssertTrue(app.buttons["batch.consume"].isEnabled)

    let depletedRow =
      app.descendants(matching: .any)["batch.row.depleted.44444444-4444-4444-4444-444444444444"]
    XCTAssertFalse(depletedRow.exists)

    let toggle = app.buttons["batch.toggle-depleted"]
    XCTAssertTrue(toggle.waitForExistence(timeout: 5))
    toggle.tap()

    XCTAssertTrue(depletedRow.waitForExistence(timeout: 5))
    depletedRow.tap()

    XCTAssertTrue(app.navigationBars["Batch history"].waitForExistence(timeout: 5))
    XCTAssertFalse(app.navigationBars["Edit batch"].exists)
    XCTAssertFalse(app.buttons["Delete"].exists)
  }
}
