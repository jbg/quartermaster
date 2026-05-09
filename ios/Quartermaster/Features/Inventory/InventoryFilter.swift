import Foundation

enum InventoryFilter: String, CaseIterable, Identifiable {
  case all = "All"
  case expiringSoon = "Expiring soon"
  case expired = "Expired"

  var id: String { rawValue }

  /// How far out "expiring soon" reaches.
  static let soonWindowDays: Int = 7

  @MainActor
  func matches(_ batch: StockBatch, using appState: AppState) -> Bool {
    switch self {
    case .all:
      return !isDepleted(batch)
    case .expired:
      guard !isDepleted(batch) else { return false }
      guard let days = appState.householdDayDifference(for: batch.expiresOn) else { return false }
      return days < 0
    case .expiringSoon:
      guard !isDepleted(batch) else { return false }
      guard let days = appState.householdDayDifference(for: batch.expiresOn) else { return false }
      return days >= 0 && days < Self.soonWindowDays
    }
  }
}
