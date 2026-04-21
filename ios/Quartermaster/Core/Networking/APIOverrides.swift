import Foundation

/// Hand-rolled tri-state Encodable types pointed at by
/// `openapi-generator-config.yaml`'s `typeOverrides.schemas`. The generated
/// `Client` uses these in place of what it would otherwise synthesise for
/// `UpdateStockRequest` / `UpdateProductRequest` — schemas whose wire
/// contract distinguishes *absent* from *null*, which the generator can't
/// express natively (upstream apple/swift-openapi-generator#419).
///
/// Only the outbound (encode) path matters: the client never receives
/// these as responses.
enum APIOverrides {
    /// PATCH body for stock batches. `quantity` routes through an adjust
    /// event server-side; metadata fields go through plain column updates.
    /// `unit` is intentionally absent — it's immutable after creation.
    struct UpdateStockRequest: Encodable, Sendable, Hashable {
        var quantity: String?
        var locationID: String?
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

    /// PATCH body for manual products. `family` uses the generated
    /// `UnitFamily` enum so the wire value stays typed.
    struct UpdateProductRequest: Encodable, Sendable, Hashable {
        var name: String?
        var brand: String?
        var clearBrand: Bool = false
        var family: Components.Schemas.UnitFamily?
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
}
