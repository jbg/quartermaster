import Foundation

/// Hand-written DTOs mirroring the backend's OpenAPI schemas. Swap for
/// generated types once swift-openapi-generator is wired up as an SPM plugin.

// MARK: - Accounts

struct TokenPair: Codable, Sendable {
    let accessToken: String
    let refreshToken: String
    let tokenType: String
    let expiresIn: Int

    enum CodingKeys: String, CodingKey {
        case accessToken = "access_token"
        case refreshToken = "refresh_token"
        case tokenType = "token_type"
        case expiresIn = "expires_in"
    }
}

struct User: Codable, Sendable, Identifiable, Hashable {
    let id: UUID
    let username: String
    let email: String?
}

struct Household: Codable, Sendable, Identifiable, Hashable {
    let id: UUID
    let name: String
}

struct Me: Codable, Sendable, Equatable {
    let user: User
    let household: Household?
}

// MARK: - Locations

struct Location: Codable, Sendable, Identifiable, Hashable {
    let id: UUID
    let name: String
    let kind: String
    let sortOrder: Int

    enum CodingKeys: String, CodingKey {
        case id, name, kind
        case sortOrder = "sort_order"
    }
}

// MARK: - Units

enum ProductFamily: String, Codable, CaseIterable, Sendable, Hashable {
    case mass
    case volume
    case count

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

struct Unit: Codable, Sendable, Hashable {
    let code: String
    let family: ProductFamily
    /// Conversion factor (× 1000) to the family's base unit.
    let toBaseMilli: Int64

    enum CodingKeys: String, CodingKey {
        case code, family
        case toBaseMilli = "to_base_milli"
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

struct Product: Codable, Sendable, Identifiable, Hashable {
    let id: UUID
    let name: String
    let brand: String?
    let family: ProductFamily
    let preferredUnit: String
    let imageURL: URL?
    let barcode: String?
    let source: String
    let deletedAt: String?

    enum CodingKeys: String, CodingKey {
        case id, name, brand, family, barcode, source
        case preferredUnit = "preferred_unit"
        case imageURL = "image_url"
        case deletedAt = "deleted_at"
    }

    var displayTitle: String {
        if let brand, !brand.isEmpty {
            return "\(brand) · \(name)"
        }
        return name
    }

    var isOFF: Bool { source == "openfoodfacts" }
    var isManual: Bool { source == "manual" }
    var isDeleted: Bool { deletedAt != nil }
}

struct ProductSearchResponse: Codable, Sendable {
    let items: [Product]
}

struct BarcodeLookupResponse: Codable, Sendable {
    let product: Product
    let source: String
}

struct CreateProductRequest: Encodable {
    let name: String
    let brand: String?
    let family: ProductFamily
    let preferredUnit: String?
    let imageURL: String?
    let barcode: String?

    enum CodingKeys: String, CodingKey {
        case name, brand, family, barcode
        case preferredUnit = "preferred_unit"
        case imageURL = "image_url"
    }
}

/// PATCH body for manual products. Encodes only the fields the caller
/// actually set; `clearBrand` / `clearImageURL` emit an explicit JSON null
/// so the backend's double-option deserializer can distinguish "leave alone"
/// from "clear".
struct UpdateProductRequest: Encodable {
    var name: String?
    var brand: String?
    var clearBrand: Bool = false
    var family: ProductFamily?
    var preferredUnit: String?
    var imageURL: String?
    var clearImageURL: Bool = false

    enum CodingKeys: String, CodingKey {
        case name, brand, family
        case preferredUnit = "preferred_unit"
        case imageURL = "image_url"
    }

    func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        if let name { try c.encode(name, forKey: .name) }
        if clearBrand {
            try c.encodeNil(forKey: .brand)
        } else if let brand {
            try c.encode(brand, forKey: .brand)
        }
        if let family { try c.encode(family, forKey: .family) }
        if let preferredUnit { try c.encode(preferredUnit, forKey: .preferredUnit) }
        if clearImageURL {
            try c.encodeNil(forKey: .imageURL)
        } else if let imageURL {
            try c.encode(imageURL, forKey: .imageURL)
        }
    }
}

// MARK: - Stock

struct StockBatch: Codable, Sendable, Identifiable, Hashable {
    let id: UUID
    let product: Product
    let locationID: UUID
    let initialQuantity: String
    let quantity: String
    let unit: String
    let expiresOn: String?
    let openedOn: String?
    let note: String?
    let createdAt: String

    enum CodingKeys: String, CodingKey {
        case id, product, quantity, unit, note
        case locationID = "location_id"
        case initialQuantity = "initial_quantity"
        case expiresOn = "expires_on"
        case openedOn = "opened_on"
        case createdAt = "created_at"
    }

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

struct StockListResponse: Codable, Sendable {
    let items: [StockBatch]
}

struct CreateStockRequest: Encodable {
    let productID: UUID
    let locationID: UUID
    let quantity: String
    let unit: String
    let expiresOn: String?
    let openedOn: String?
    let note: String?

    enum CodingKeys: String, CodingKey {
        case quantity, unit, note
        case productID = "product_id"
        case locationID = "location_id"
        case expiresOn = "expires_on"
        case openedOn = "opened_on"
    }
}

/// PATCH body for stock batches. `quantity` routes through an adjust event;
/// metadata fields go through plain column updates. `unit` is intentionally
/// absent — it's immutable after creation. Clearable date/note fields use
/// the same encoder-level explicit-null pattern as `UpdateProductRequest`.
struct UpdateStockRequest: Encodable {
    var quantity: String?
    var locationID: UUID?
    var expiresOn: String?
    var clearExpiresOn: Bool = false
    var openedOn: String?
    var clearOpenedOn: Bool = false
    var note: String?
    var clearNote: Bool = false

    enum CodingKeys: String, CodingKey {
        case quantity, note
        case locationID = "location_id"
        case expiresOn = "expires_on"
        case openedOn = "opened_on"
    }

    func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        if let quantity { try c.encode(quantity, forKey: .quantity) }
        if let locationID { try c.encode(locationID, forKey: .locationID) }
        if clearExpiresOn {
            try c.encodeNil(forKey: .expiresOn)
        } else if let expiresOn {
            try c.encode(expiresOn, forKey: .expiresOn)
        }
        if clearOpenedOn {
            try c.encodeNil(forKey: .openedOn)
        } else if let openedOn {
            try c.encode(openedOn, forKey: .openedOn)
        }
        if clearNote {
            try c.encodeNil(forKey: .note)
        } else if let note {
            try c.encode(note, forKey: .note)
        }
    }
}

struct ConsumeRequest: Encodable {
    let productID: UUID
    let locationID: UUID?
    let quantity: String
    let unit: String

    enum CodingKeys: String, CodingKey {
        case quantity, unit
        case productID = "product_id"
        case locationID = "location_id"
    }
}

struct ConsumedBatch: Codable, Sendable {
    let batchID: UUID
    /// Amount taken from the batch, in the batch's own unit.
    let quantity: String
    let unit: String
    /// Same amount converted to the unit the caller requested.
    let quantityInRequestedUnit: String
    let requestedUnit: String
    let depleted: Bool

    enum CodingKeys: String, CodingKey {
        case quantity, unit, depleted
        case batchID = "batch_id"
        case quantityInRequestedUnit = "quantity_in_requested_unit"
        case requestedUnit = "requested_unit"
    }
}

struct ConsumeResponse: Codable, Sendable {
    let consumed: [ConsumedBatch]
    let consumeRequestID: UUID

    enum CodingKeys: String, CodingKey {
        case consumed
        case consumeRequestID = "consume_request_id"
    }
}

// MARK: - Stock events

enum StockEventType: String, Codable, Sendable, Hashable {
    case add
    case consume
    case adjust
    case discard
    case restore

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

struct StockEvent: Codable, Sendable, Identifiable, Hashable {
    let id: UUID
    let eventType: StockEventType
    let quantityDelta: String
    let unit: String
    let batchExpiresOn: String?
    let note: String?
    let createdAt: String
    let createdByUsername: String?
    let batchID: UUID
    let product: Product
    let consumeRequestID: UUID?

    enum CodingKeys: String, CodingKey {
        case id, unit, note, product
        case eventType = "event_type"
        case quantityDelta = "quantity_delta"
        case batchExpiresOn = "batch_expires_on"
        case createdAt = "created_at"
        case createdByUsername = "created_by_username"
        case batchID = "batch_id"
        case consumeRequestID = "consume_request_id"
    }

    var batchExpiresOnDate: Date? {
        guard let batchExpiresOn else { return nil }
        return StockBatch.yyyymmdd.date(from: batchExpiresOn)
    }

    var createdAtDate: Date? {
        Self.iso.date(from: createdAt)
    }

    // ISO8601DateFormatter is documented as thread-safe; marking the shared
    // instance nonisolated(unsafe) satisfies Swift 6 strict concurrency.
    nonisolated(unsafe) private static let iso: ISO8601DateFormatter = {
        let f = ISO8601DateFormatter()
        f.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        return f
    }()
}

struct StockEventListResponse: Codable, Sendable {
    let items: [StockEvent]
    let nextBefore: String?
    let nextBeforeID: UUID?

    enum CodingKeys: String, CodingKey {
        case items
        case nextBefore = "next_before"
        case nextBeforeID = "next_before_id"
    }
}

struct RestoreManyRequest: Encodable {
    let ids: [UUID]
}

struct RestoreManyResponse: Codable, Sendable {
    let restored: [StockBatch]
}

// MARK: - Errors

struct APIErrorBody: Codable, Sendable {
    let code: String
    let message: String
    /// Populated on `code == "batch_not_restorable"` when the failure came
    /// from `POST /stock/restore-many` — identifies which batches were the
    /// problem so the UI can name them.
    let unrestorableIds: [UUID]?

    enum CodingKeys: String, CodingKey {
        case code, message
        case unrestorableIds = "unrestorable_ids"
    }
}

// MARK: - Request builders (auth, unchanged)

struct RegisterRequest: Encodable {
    let username: String
    let password: String
    let email: String?
    let deviceLabel: String?

    enum CodingKeys: String, CodingKey {
        case username, password, email
        case deviceLabel = "device_label"
    }
}

struct LoginRequest: Encodable {
    let username: String
    let password: String
    let deviceLabel: String?

    enum CodingKeys: String, CodingKey {
        case username, password
        case deviceLabel = "device_label"
    }
}

struct RefreshRequest: Encodable {
    let refreshToken: String

    enum CodingKeys: String, CodingKey {
        case refreshToken = "refresh_token"
    }
}
