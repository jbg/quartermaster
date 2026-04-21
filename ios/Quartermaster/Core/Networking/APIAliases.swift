import Foundation

/// Typealiases from generated `Components.Schemas.*` to the flat names
/// feature views use, plus the `Identifiable` conformances and computed
/// helpers that the generator doesn't emit.
///
/// The generator's conventions worth knowing about:
///
/// * **All IDs are `Swift.String`, not `Foundation.UUID`**. The generator
///   treats `format: uuid` as an annotation on a string. IDs are opaque
///   at the client layer — parse to UUID only when the domain actually
///   needs it. Views pass them around as strings.
/// * **`snake_case` → `camelCase`** via `namingStrategy: idiomatic`:
///   `image_url` → `imageUrl`, `location_id` → `locationId`,
///   `next_before_id` → `nextBeforeId`. `URL` / `ID` abbreviations lose
///   their capital letters. Extensions below expose the old names as
///   computed aliases so feature views don't rename en masse.
/// * **No synthesised `Identifiable`**. We add it here per type.

// MARK: - Accounts

typealias TokenPair = Components.Schemas.TokenPair
typealias User = Components.Schemas.UserDto
typealias Household = Components.Schemas.HouseholdDto
typealias Me = Components.Schemas.MeResponse
typealias HouseholdDetail = Components.Schemas.HouseholdDetailDto
typealias UpdateHouseholdRequest = Components.Schemas.UpdateHouseholdRequest
typealias Member = Components.Schemas.MemberDto
typealias MembershipRole = Components.Schemas.MembershipRole
typealias Invite = Components.Schemas.InviteDto
typealias CreateInviteRequest = Components.Schemas.CreateInviteRequest
typealias RedeemInviteRequest = Components.Schemas.RedeemInviteRequest

extension User: Identifiable {}
extension Household: Identifiable {}
extension HouseholdDetail: Identifiable {}
extension Member: Identifiable {
    var id: String { user.id }
}
extension Invite: Identifiable {}

extension MembershipRole {
    var displayName: String {
        switch self {
        case .admin: "Admin"
        case .member: "Member"
        }
    }
}

// MARK: - Locations

typealias Location = Components.Schemas.LocationDto
typealias CreateLocationRequest = Components.Schemas.CreateLocationRequest
typealias UpdateLocationRequest = Components.Schemas.UpdateLocationRequest

extension Location: Identifiable {}

// MARK: - Units

typealias ProductFamily = Components.Schemas.UnitFamily
typealias Unit = Components.Schemas.UnitDto

extension ProductFamily {
    var displayName: String {
        switch self {
        case .mass: "Mass"
        case .volume: "Volume"
        case .count: "Count"
        }
    }

    var baseUnit: String {
        switch self {
        case .mass: "g"
        case .volume: "ml"
        case .count: "piece"
        }
    }
}

enum UnitConversion {
    static func convert(_ value: Decimal, fromCode: String, toCode: String, units: [Unit]) -> Decimal? {
        guard
            let from = units.first(where: { $0.code == fromCode }),
            let to = units.first(where: { $0.code == toCode }),
            from.family == to.family
        else { return nil }
        let fromFactor = Decimal(from.toBaseMilli) / 1000
        let toFactor = Decimal(to.toBaseMilli) / 1000
        return (value * fromFactor) / toFactor
    }

    static func sum(_ batches: [StockBatch], inUnit targetCode: String, units: [Unit]) -> Decimal? {
        var total = Decimal.zero
        for batch in batches {
            guard
                let qty = Decimal(string: batch.quantity),
                let converted = convert(qty, fromCode: batch.unit, toCode: targetCode, units: units)
            else { return nil }
            total += converted
        }
        return total
    }
}

// MARK: - Products

typealias Product = Components.Schemas.ProductDto
typealias ProductSource = Components.Schemas.ProductSource
typealias CreateProductRequest = Components.Schemas.CreateProductRequest
typealias UpdateProductRequest = APIOverrides.UpdateProductRequest
typealias ProductSearchResponse = Components.Schemas.ProductSearchResponse
typealias BarcodeLookupResponse = Components.Schemas.BarcodeLookupResponse

extension Product: Identifiable {
    /// Parsed URL; `imageUrl` on the DTO is a raw String.
    var imageURL: URL? { imageUrl.flatMap(URL.init(string:)) }
    var isDeleted: Bool { deletedAt != nil }
    var isOFF: Bool { source == .openfoodfacts }
    var isManual: Bool { source == .manual }
    var displayTitle: String {
        if let brand, !brand.isEmpty {
            return "\(brand) · \(name)"
        }
        return name
    }
}

// MARK: - Stock

typealias StockBatch = Components.Schemas.StockBatchDto
typealias StockListResponse = Components.Schemas.StockListResponse
typealias CreateStockRequest = Components.Schemas.CreateStockRequest
typealias UpdateStockRequest = APIOverrides.UpdateStockRequest
typealias ConsumeRequest = Components.Schemas.ConsumeRequest
typealias ConsumedBatch = Components.Schemas.ConsumedBatchDto
typealias ConsumeResponse = Components.Schemas.ConsumeResponse
typealias RestoreManyRequest = Components.Schemas.RestoreManyRequest
typealias RestoreManyResponse = Components.Schemas.RestoreManyResponse

extension StockBatch: Identifiable {
    /// Legacy alias for the camelCased `locationId` the generator emits.
    var locationID: String { locationId }
    var expiresOnDate: Date? {
        guard let expiresOn else { return nil }
        return Self.yyyymmdd.date(from: expiresOn)
    }
    var openedOnDate: Date? {
        guard let openedOn else { return nil }
        return Self.yyyymmdd.date(from: openedOn)
    }
    static let yyyymmdd: DateFormatter = {
        let f = DateFormatter()
        f.dateFormat = "yyyy-MM-dd"
        f.timeZone = .init(identifier: "UTC")
        f.locale = .init(identifier: "en_US_POSIX")
        return f
    }()
}

extension ConsumedBatch {
    var batchID: String { batchId }
}

extension ConsumeResponse {
    var consumeRequestID: String { consumeRequestId }
}

// MARK: - Stock events

typealias StockEvent = Components.Schemas.StockEventDto
typealias StockEventType = Components.Schemas.StockEventType
typealias StockEventListResponse = Components.Schemas.StockEventListResponse

extension StockEventType {
    var displayLabel: String {
        switch self {
        case .add: "Added"
        case .consume: "Consumed"
        case .adjust: "Adjusted"
        case .discard: "Discarded"
        case .restore: "Restored"
        }
    }

    var systemImage: String {
        switch self {
        case .add: "plus.circle"
        case .consume: "fork.knife"
        case .adjust: "pencil"
        case .discard: "trash"
        case .restore: "arrow.uturn.backward"
        }
    }
}

extension StockEvent: Identifiable {
    var batchID: String { batchId }
    var consumeRequestID: String? { consumeRequestId }
    var batchExpiresOnDate: Date? {
        guard let batchExpiresOn else { return nil }
        return StockBatch.yyyymmdd.date(from: batchExpiresOn)
    }
    var createdAtDate: Date? { Self.iso.date(from: createdAt) }
    nonisolated(unsafe) private static let iso: ISO8601DateFormatter = {
        let f = ISO8601DateFormatter()
        f.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        return f
    }()
}

extension StockEventListResponse {
    var nextBeforeID: String? { nextBeforeId }
}

// MARK: - Errors

typealias APIErrorBody = Components.Schemas.ApiErrorBody
