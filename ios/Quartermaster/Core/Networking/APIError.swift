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
            return body.message
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
