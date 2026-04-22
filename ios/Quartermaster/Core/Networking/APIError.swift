import Foundation

enum APIError: Error, LocalizedError {
    case transport(URLError)
    case decoding(Error)
    case server(status: Int, body: APIErrorBody?)
    case unauthorized
    case unknown

    var userFacingMessage: String {
        switch self {
        case .transport(let err):
            return "Can't reach the server: \(err.localizedDescription)"
        case .decoding:
            return "Unexpected response from the server."
        case .server(_, let body?):
            switch body.code {
            case "last_admin_removal":
                return "You can't remove the last admin from the household."
            case "location_has_stock":
                return "This location still has active stock. Move, consume, or discard it first."
            case "invalid_invite":
                return "That invite is invalid, expired, revoked, or already used up."
            case "rate_limited":
                return "The server is asking us to slow down. Please try again in a moment."
            case "admin_only", "forbidden":
                return "You need household admin access for that action."
            default:
                return body.message
            }
        case .server(let status, nil):
            return "Server error (\(status))."
        case .unauthorized:
            return "Your session has expired. Please sign in again."
        case .unknown:
            return "Something went wrong."
        }
    }

    var errorDescription: String? { userFacingMessage }
}
