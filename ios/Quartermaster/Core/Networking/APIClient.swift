import Foundation

actor APIClient {
    private let baseURL: URL
    private let tokenStore: TokenStore
    private let session: URLSession
    private var refreshTask: Task<Void, Error>?

    init(baseURL: URL, tokenStore: TokenStore, session: URLSession = .shared) {
        self.baseURL = baseURL
        self.tokenStore = tokenStore
        self.session = session
    }

    // MARK: - Endpoints

    func register(username: String, password: String, email: String?) async throws -> TokenPair {
        let body = RegisterRequest(
            username: username,
            password: password,
            email: email,
            deviceLabel: Self.deviceLabel,
        )
        return try await post("/auth/register", body: body, authenticated: false)
    }

    func login(username: String, password: String) async throws -> TokenPair {
        let body = LoginRequest(username: username, password: password, deviceLabel: Self.deviceLabel)
        return try await post("/auth/login", body: body, authenticated: false)
    }

    func logout() async throws {
        let _: EmptyResponse = try await post("/auth/logout", body: EmptyBody(), authenticated: true)
    }

    func me() async throws -> Me {
        try await get("/auth/me", authenticated: true)
    }

    func locations() async throws -> [Location] {
        try await get("/locations", authenticated: true)
    }

    // MARK: - Plumbing

    private func get<T: Decodable>(_ path: String, authenticated: Bool) async throws -> T {
        try await send(method: "GET", path: path, body: Optional<EmptyBody>.none, authenticated: authenticated)
    }

    private func post<B: Encodable, T: Decodable>(_ path: String, body: B, authenticated: Bool) async throws -> T {
        try await send(method: "POST", path: path, body: body, authenticated: authenticated)
    }

    private func send<B: Encodable, T: Decodable>(
        method: String,
        path: String,
        body: B?,
        authenticated: Bool,
    ) async throws -> T {
        var attemptedRefresh = false
        while true {
            var request = URLRequest(url: baseURL.appendingPathComponent(path))
            request.httpMethod = method
            request.setValue("application/json", forHTTPHeaderField: "Accept")
            if let body {
                request.setValue("application/json", forHTTPHeaderField: "Content-Type")
                request.httpBody = try encoder.encode(body)
            }
            if authenticated, let token = await tokenStore.accessToken {
                request.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
            }

            let data: Data
            let response: URLResponse
            do {
                (data, response) = try await session.data(for: request)
            } catch let err as URLError {
                throw APIError.transport(err)
            } catch {
                throw APIError.unknown
            }

            guard let http = response as? HTTPURLResponse else {
                throw APIError.unknown
            }

            if http.statusCode == 401, authenticated, !attemptedRefresh {
                attemptedRefresh = true
                do {
                    try await refreshIfPossible()
                } catch {
                    await tokenStore.clear()
                    throw APIError.unauthorized
                }
                continue
            }

            if !(200..<300).contains(http.statusCode) {
                let body = try? decoder.decode(APIErrorBody.self, from: data)
                if http.statusCode == 401 { throw APIError.unauthorized }
                throw APIError.server(status: http.statusCode, body: body)
            }

            if T.self is EmptyResponse.Type {
                return EmptyResponse() as! T
            }
            do {
                return try decoder.decode(T.self, from: data)
            } catch {
                throw APIError.decoding(error)
            }
        }
    }

    private func refreshIfPossible() async throws {
        if let existing = refreshTask {
            try await existing.value
            return
        }
        let task = Task<Void, Error> {
            guard let refresh = await tokenStore.refreshToken else {
                throw APIError.unauthorized
            }
            let body = RefreshRequest(refreshToken: refresh)
            var req = URLRequest(url: baseURL.appendingPathComponent("/auth/refresh"))
            req.httpMethod = "POST"
            req.setValue("application/json", forHTTPHeaderField: "Content-Type")
            req.httpBody = try encoder.encode(body)
            let (data, resp) = try await session.data(for: req)
            guard let http = resp as? HTTPURLResponse, (200..<300).contains(http.statusCode) else {
                throw APIError.unauthorized
            }
            let pair = try decoder.decode(TokenPair.self, from: data)
            await tokenStore.store(pair)
        }
        refreshTask = task
        defer { refreshTask = nil }
        try await task.value
    }

    private let encoder: JSONEncoder = {
        let e = JSONEncoder()
        return e
    }()

    private let decoder: JSONDecoder = {
        let d = JSONDecoder()
        return d
    }()

    private struct EmptyBody: Encodable {}
    private struct EmptyResponse: Decodable {}

    private static let deviceLabel: String? = {
        #if os(iOS)
        return "iOS"
        #else
        return nil
        #endif
    }()
}
