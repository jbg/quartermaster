import Foundation

/// Hand-written DTOs mirroring the backend's OpenAPI schemas. Swap for
/// generated types once swift-openapi-generator is wired up as an SPM plugin.

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

struct APIErrorBody: Codable, Sendable {
    let code: String
    let message: String
}

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
