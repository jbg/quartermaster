import XCTest

final class AppStoreScreenshotUITests: XCTestCase {
  private var app: XCUIApplication!

  override func setUp() {
    super.setUp()
    continueAfterFailure = false
    app = XCUIApplication()
    app.launchArguments = ["--quartermaster-ui-test-app-store-screenshots"]
  }

  func testCaptureCoreInventoryStory() {
    app.launch()

    let productRow = app.buttons["inventory.product.11111111-1111-1111-1111-111111111111"]
    XCTAssertTrue(productRow.waitForExistence(timeout: 5))
    capture("01-inventory")

    let addStockButton = app.buttons["Add stock"].firstMatch
    XCTAssertTrue(addStockButton.waitForExistence(timeout: 5))
    addStockButton.tap()
    let productSearchField = app.searchFields["Search products"]
    if !productSearchField.waitForExistence(timeout: 3) {
      addStockButton.tap()
    }
    XCTAssertTrue(productSearchField.waitForExistence(timeout: 5))
    productSearchField.tap()
    productSearchField.typeText("oat")
    XCTAssertTrue(
      app.buttons.containing(.staticText, identifier: "Smoke Oats").firstMatch.waitForExistence(
        timeout: 5))
    capture("02-product-search")

    app.terminate()
    app.launch()
    XCTAssertTrue(productRow.waitForExistence(timeout: 5))

    productRow.tap()
    XCTAssertTrue(app.buttons["batch.consume"].waitForExistence(timeout: 5))
    capture("03-batches")

    app.buttons["batch.toggle-depleted"].tap()
    let depletedRow =
      app.descendants(matching: .any)["batch.row.depleted.44444444-4444-4444-4444-444444444444"]
    XCTAssertTrue(depletedRow.waitForExistence(timeout: 5))
    depletedRow.tap()
    XCTAssertTrue(app.navigationBars["Batch history"].waitForExistence(timeout: 5))
    capture("04-batch-history")

    app.terminate()
    app.launch()
    XCTAssertTrue(productRow.waitForExistence(timeout: 5))
    app.tabBars.buttons["Reminders"].tap()
    XCTAssertTrue(app.navigationBars["Reminders"].waitForExistence(timeout: 5))
    capture("05-reminders")

    app.tabBars.buttons["Settings"].tap()
    XCTAssertTrue(app.navigationBars["Settings"].waitForExistence(timeout: 5))
    capture("06-settings")

    let pairButton = app.buttons["Pair signed-in device"]
    for _ in 0..<4 where !pairButton.exists {
      app.swipeUp()
    }
    XCTAssertTrue(pairButton.waitForExistence(timeout: 5))
    pairButton.tap()
    XCTAssertTrue(app.navigationBars["Pair signed-in device"].waitForExistence(timeout: 5))
    app.buttons["Create handoff code"].tap()
    XCTAssertTrue(app.images["Authenticated handoff QR code"].waitForExistence(timeout: 5))
    capture("07-pair-device")
  }

  private func capture(_ name: String) {
    let attachment = XCTAttachment(screenshot: app.screenshot())
    attachment.name = name
    attachment.lifetime = .keepAlways
    add(attachment)
  }
}
