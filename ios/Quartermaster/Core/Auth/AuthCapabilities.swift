import Foundation

struct PasskeyCredentialSummary: Identifiable, Codable, Equatable {
  let id: String
  let label: String?
  let createdAt: String
  let lastUsedAt: String?
}

struct PasskeyRegistrationStart: Codable, Equatable {
  let ceremonyID: String
  let publicKey: Data
}

struct PasskeyLoginStart: Codable, Equatable {
  let ceremonyID: String
  let publicKey: Data
}

struct AuthHandoffPreview: Codable, Equatable {
  let id: String
  let sourceEmail: String
  let sourceDisplayName: String
  let householdID: String?
  let targetDeviceLabel: String?
  let expiresAt: String

  enum CodingKeys: String, CodingKey {
    case id
    case sourceEmail = "source_email"
    case sourceDisplayName = "source_display_name"
    case householdID = "household_id"
    case targetDeviceLabel = "target_device_label"
    case expiresAt = "expires_at"
  }
}

struct AuthHandoffCreate: Codable, Equatable {
  let id: String
  let handoffURL: String
  let expiresAt: String
  let targetDeviceLabel: String?

  enum CodingKeys: String, CodingKey {
    case id
    case handoffURL = "handoff_url"
    case expiresAt = "expires_at"
    case targetDeviceLabel = "target_device_label"
  }
}
