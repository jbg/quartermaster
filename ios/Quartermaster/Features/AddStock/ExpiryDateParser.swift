import Foundation

struct ExpiryDateCandidate: Identifiable, Hashable {
  enum Precision: Hashable {
    case day
    case month
  }

  let date: Date
  let sourceText: String
  let precision: Precision

  var id: String {
    "\(StockBatch.yyyymmdd.string(from: date))-\(sourceText)-\(precision)"
  }
}

enum ExpiryDateParser {
  static func bestCandidate(
    in text: String,
    today: Date = Date(),
    calendar inputCalendar: Calendar = expiryCalendar,
  ) -> ExpiryDateCandidate? {
    candidates(in: text, today: today, calendar: inputCalendar).first
  }

  static func candidates(
    in text: String,
    today: Date = Date(),
    calendar inputCalendar: Calendar = expiryCalendar,
  ) -> [ExpiryDateCandidate] {
    var calendar = inputCalendar
    calendar.timeZone = TimeZone(secondsFromGMT: 0) ?? .gmt
    let todayStart = calendar.startOfDay(for: today)
    var found: [ExpiryDateCandidate] = []

    found.append(
      contentsOf: numericYearFirstCandidates(in: text, calendar: calendar))
    found.append(
      contentsOf: numericMonthYearCandidates(in: text, calendar: calendar))
    found.append(
      contentsOf: numericAmbiguousOrderCandidates(in: text, calendar: calendar))
    found.append(contentsOf: monthNameCandidates(in: text, calendar: calendar))

    var seen = Set<String>()
    return
      found
      .filter { $0.date >= todayStart }
      .filter { candidate in
        let key = StockBatch.yyyymmdd.string(from: candidate.date)
        guard !seen.contains(key) else { return false }
        seen.insert(key)
        return true
      }
      .sorted { lhs, rhs in
        if lhs.date != rhs.date { return lhs.date < rhs.date }
        return lhs.precision.sortOrder < rhs.precision.sortOrder
      }
  }

  private static let expiryCalendar: Calendar = {
    var calendar = Calendar(identifier: .gregorian)
    calendar.timeZone = TimeZone(secondsFromGMT: 0) ?? .gmt
    return calendar
  }()

  private static func numericYearFirstCandidates(
    in text: String,
    calendar: Calendar,
  ) -> [ExpiryDateCandidate] {
    regexMatches(
      #"(?<!\d)((?:19|20)\d{2})[\s./-](0?[1-9]|1[0-2])(?:[\s./-]([0-3]?\d))?(?!\d)"#,
      in: text
    ).compactMap { match in
      guard
        let year = Int(match.groups[0]),
        let month = Int(match.groups[1])
      else { return nil }
      if let dayText = match.groups[safe: 2], let day = Int(dayText) {
        return candidate(year: year, month: month, day: day, source: match.text, calendar: calendar)
      }
      return monthEndCandidate(year: year, month: month, source: match.text, calendar: calendar)
    }
  }

  private static func numericMonthYearCandidates(
    in text: String,
    calendar: Calendar,
  ) -> [ExpiryDateCandidate] {
    regexMatches(
      #"(?<![\d./-])(0?[1-9]|1[0-2])[\s./-]((?:19|20)\d{2})(?!\d)"#,
      in: text
    ).compactMap { match in
      guard
        let month = Int(match.groups[0]),
        let year = Int(match.groups[1])
      else { return nil }
      return monthEndCandidate(year: year, month: month, source: match.text, calendar: calendar)
    }
  }

  private static func numericAmbiguousOrderCandidates(
    in text: String,
    calendar: Calendar,
  ) -> [ExpiryDateCandidate] {
    regexMatches(
      #"(?<!\d)([0-3]?\d)[\s./-]([0-3]?\d)[\s./-]((?:19|20)\d{2})(?!\d)"#,
      in: text
    ).compactMap { match in
      guard
        let first = Int(match.groups[0]),
        let second = Int(match.groups[1]),
        let year = Int(match.groups[2])
      else { return nil }

      if first > 12 && second <= 12 {
        return candidate(
          year: year, month: second, day: first, source: match.text, calendar: calendar)
      }
      if second > 12 && first <= 12 {
        return candidate(
          year: year, month: first, day: second, source: match.text, calendar: calendar)
      }
      return nil
    }
  }

  private static func monthNameCandidates(
    in text: String,
    calendar: Calendar,
  ) -> [ExpiryDateCandidate] {
    let monthPattern =
      #"jan(?:uary)?|feb(?:ruary)?|mar(?:ch)?|apr(?:il)?|may|jun(?:e)?|jul(?:y)?|aug(?:ust)?|sep(?:t(?:ember)?)?|oct(?:ober)?|nov(?:ember)?|dec(?:ember)?"#
    let dayMonthYearPattern =
      "(?i)(?<![A-Za-z])([0-3]?\\d)[\\s.,-]+(" + monthPattern
      + ")[\\s.,-]+((?:19|20)\\d{2})(?!\\d)"
    let monthYearPattern =
      "(?i)(?<![A-Za-z])(" + monthPattern + ")[\\s.,-]+((?:19|20)\\d{2})(?!\\d)"
    var results: [ExpiryDateCandidate] = []

    results.append(
      contentsOf: regexMatches(
        dayMonthYearPattern,
        in: text
      ).compactMap { match in
        guard
          let day = Int(match.groups[0]),
          let month = monthNumber(match.groups[1]),
          let year = Int(match.groups[2])
        else { return nil }
        return candidate(year: year, month: month, day: day, source: match.text, calendar: calendar)
      })

    results.append(
      contentsOf: regexMatches(
        monthYearPattern,
        in: text
      ).compactMap { match in
        guard
          let month = monthNumber(match.groups[0]),
          let year = Int(match.groups[1])
        else { return nil }
        return monthEndCandidate(year: year, month: month, source: match.text, calendar: calendar)
      })

    return results
  }

  private static func candidate(
    year: Int,
    month: Int,
    day: Int,
    source: String,
    calendar: Calendar,
  ) -> ExpiryDateCandidate? {
    guard valid(year: year, month: month, day: day, calendar: calendar) else { return nil }
    var components = DateComponents()
    components.calendar = calendar
    components.timeZone = calendar.timeZone
    components.year = year
    components.month = month
    components.day = day
    guard let date = calendar.date(from: components) else { return nil }
    return ExpiryDateCandidate(date: date, sourceText: source, precision: .day)
  }

  private static func monthEndCandidate(
    year: Int,
    month: Int,
    source: String,
    calendar: Calendar,
  ) -> ExpiryDateCandidate? {
    guard
      let start = candidate(year: year, month: month, day: 1, source: source, calendar: calendar)?
        .date,
      let range = calendar.range(of: .day, in: .month, for: start)
    else { return nil }
    return candidate(
      year: year,
      month: month,
      day: range.count,
      source: source,
      calendar: calendar
    ).map { ExpiryDateCandidate(date: $0.date, sourceText: source, precision: .month) }
  }

  private static func valid(year: Int, month: Int, day: Int, calendar: Calendar) -> Bool {
    guard (1...12).contains(month), (1...31).contains(day) else { return false }
    var components = DateComponents()
    components.calendar = calendar
    components.timeZone = calendar.timeZone
    components.year = year
    components.month = month
    components.day = day
    guard let date = calendar.date(from: components) else { return false }
    let roundTrip = calendar.dateComponents([.year, .month, .day], from: date)
    return roundTrip.year == year && roundTrip.month == month && roundTrip.day == day
  }

  private static func monthNumber(_ text: String) -> Int? {
    switch text.lowercased().prefix(3) {
    case "jan": return 1
    case "feb": return 2
    case "mar": return 3
    case "apr": return 4
    case "may": return 5
    case "jun": return 6
    case "jul": return 7
    case "aug": return 8
    case "sep": return 9
    case "oct": return 10
    case "nov": return 11
    case "dec": return 12
    default: return nil
    }
  }

  private static func regexMatches(_ pattern: String, in text: String) -> [RegexMatch] {
    guard let regex = try? NSRegularExpression(pattern: pattern) else { return [] }
    let nsText = text as NSString
    let range = NSRange(location: 0, length: nsText.length)
    return regex.matches(in: text, range: range).map { result in
      let groups = (1..<result.numberOfRanges).map { index -> String in
        let range = result.range(at: index)
        guard range.location != NSNotFound else { return "" }
        return nsText.substring(with: range)
      }
      return RegexMatch(text: nsText.substring(with: result.range), groups: groups)
    }
  }
}

private struct RegexMatch {
  let text: String
  let groups: [String]
}

extension Array {
  fileprivate subscript(safe index: Int) -> Element? {
    indices.contains(index) ? self[index] : nil
  }
}

extension ExpiryDateCandidate.Precision {
  var sortOrder: Int {
    switch self {
    case .day: return 0
    case .month: return 1
    }
  }
}
