import XCTest

@testable import Quartermaster

final class ExpiryDateParserTests: XCTestCase {
  func testFindsIsoDateAmongBatchText() {
    let candidate = ExpiryDateParser.bestCandidate(
      in: "LOT A18 BBE 2026-07-14 12:09",
      today: date("2026-05-06")
    )

    XCTAssertEqual(candidate.map { StockBatch.yyyymmdd.string(from: $0.date) }, "2026-07-14")
    XCTAssertEqual(candidate?.precision, .day)
  }

  func testMonthYearUsesLastDayOfMonth() {
    let candidate = ExpiryDateParser.bestCandidate(
      in: "EXP 05-2026 L221B",
      today: date("2026-01-01")
    )

    XCTAssertEqual(candidate.map { StockBatch.yyyymmdd.string(from: $0.date) }, "2026-05-31")
    XCTAssertEqual(candidate?.precision, .month)
  }

  func testYearMonthUsesLastDayOfMonth() {
    let candidate = ExpiryDateParser.bestCandidate(
      in: "Best before 2028/02",
      today: date("2026-01-01")
    )

    XCTAssertEqual(candidate.map { StockBatch.yyyymmdd.string(from: $0.date) }, "2028-02-29")
    XCTAssertEqual(candidate?.precision, .month)
  }

  func testUnambiguousSlashDateParsesDayMonthYear() {
    let candidate = ExpiryDateParser.bestCandidate(
      in: "use by 28/06/2026 batch 04",
      today: date("2026-05-06")
    )

    XCTAssertEqual(candidate.map { StockBatch.yyyymmdd.string(from: $0.date) }, "2026-06-28")
  }

  func testAmbiguousSlashDateIsIgnored() {
    let candidate = ExpiryDateParser.bestCandidate(
      in: "best before 06/07/2026",
      today: date("2026-05-06")
    )

    XCTAssertNil(candidate)
  }

  func testMonthNameYearUsesLastDayOfMonth() {
    let candidate = ExpiryDateParser.bestCandidate(
      in: "EXP SEP 2026 L889",
      today: date("2026-05-06")
    )

    XCTAssertEqual(candidate.map { StockBatch.yyyymmdd.string(from: $0.date) }, "2026-09-30")
  }

  func testPastDatesAreIgnored() {
    let candidate = ExpiryDateParser.bestCandidate(
      in: "LOT 2024-08-01 EXP 2026-08-01",
      today: date("2026-05-06")
    )

    XCTAssertEqual(candidate.map { StockBatch.yyyymmdd.string(from: $0.date) }, "2026-08-01")
  }

  private func date(_ value: String) -> Date {
    guard let date = StockBatch.yyyymmdd.date(from: value) else {
      XCTFail("Invalid test date \(value)")
      return Date(timeIntervalSince1970: 0)
    }
    return date
  }
}
