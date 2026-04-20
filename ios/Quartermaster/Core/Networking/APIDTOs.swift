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

    enum CodingKeys: String, CodingKey {
        case id, name, brand, family, barcode, source
        case preferredUnit = "preferred_unit"
        case imageURL = "image_url"
    }

    /// Formatted label for list rows: "Brand · Name" or just "Name".
    var displayTitle: String {
        if let brand, !brand.isEmpty {
            return "\(brand) · \(name)"
        }
        return name
    }
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
    let barcode: String?

    enum CodingKeys: String, CodingKey {
        case name, brand, family, barcode
        case preferredUnit = "preferred_unit"
    }
}

// MARK: - Stock

struct StockBatch: Codable, Sendable, Identifiable, Hashable {
    let id: UUID
    let product: Product
    let locationID: UUID
    let quantity: String
    let unit: String
    let expiresOn: String?
    let openedOn: String?
    let note: String?
    let createdAt: String

    enum CodingKeys: String, CodingKey {
        case id, product, quantity, unit, note
        case locationID = "location_id"
        case expiresOn = "expires_on"
        case openedOn = "opened_on"
        case createdAt = "created_at"
    }

    var expiresOnDate: Date? {
        guard let expiresOn else { return nil }
        return Self.yyyymmdd.date(from: expiresOn)
    }

    private static let yyyymmdd: DateFormatter = {
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

struct UpdateStockRequest: Encodable {
    var quantity: String?
    var unit: String?
    var locationID: UUID?
    var expiresOn: String?
    var note: String?

    enum CodingKeys: String, CodingKey {
        case quantity, unit, note
        case locationID = "location_id"
        case expiresOn = "expires_on"
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
    let quantity: String
    let unit: String
    let depleted: Bool

    enum CodingKeys: String, CodingKey {
        case quantity, unit, depleted
        case batchID = "batch_id"
    }
}

struct ConsumeResponse: Codable, Sendable {
    let consumed: [ConsumedBatch]
}

// MARK: - Errors

struct APIErrorBody: Codable, Sendable {
    let code: String
    let message: String
}

// MARK: - Request builders (legacy, kept for slice-1 compatibility)

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
