import Foundation

struct PasskeyCredentialSummary: Identifiable, Equatable {
  let id: String
  let label: String?
  let createdAt: String
  let lastUsedAt: String?
}

struct PasskeyRegistrationStart: Equatable {
  let ceremonyID: String
  let publicKey: Data
}

struct PasskeyLoginStart: Equatable {
  let ceremonyID: String
  let publicKey: Data
}

struct AuthHandoffPreview: Equatable {
  let id: String
  let sourceEmail: String
  let sourceDisplayName: String
  let householdID: String?
  let targetDeviceLabel: String?
  let expiresAt: String
}

struct AuthHandoffCreate: Equatable {
  let id: String
  let handoffURL: String
  let expiresAt: String
  let targetDeviceLabel: String?
}
