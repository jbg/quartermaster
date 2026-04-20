import Foundation

enum InventoryFilter: String, CaseIterable, Identifiable {
    case all = "All"
    case expiringSoon = "Expiring soon"
    case expired = "Expired"

    var id: String { rawValue }

    /// How far out "expiring soon" reaches.
    static let soonWindowDays: Int = 7

    func matches(_ batch: StockBatch) -> Bool {
        switch self {
        case .all:
            return true
        case .expired:
            guard let date = batch.expiresOnDate else { return false }
            return date < Calendar.current.startOfDay(for: .now)
        case .expiringSoon:
            guard let date = batch.expiresOnDate else { return false }
            let start = Calendar.current.startOfDay(for: .now)
            let end = Calendar.current.date(byAdding: .day, value: Self.soonWindowDays, to: start) ?? start
            return date >= start && date < end
        }
    }
}
