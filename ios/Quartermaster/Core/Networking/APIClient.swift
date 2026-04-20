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

    // MARK: - Accounts

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
        let _: EmptyResponse = try await send(
            method: "POST",
            path: "/auth/logout",
            body: Optional<EmptyBody>.none,
            authenticated: true,
        )
    }

    func me() async throws -> Me {
        try await get("/auth/me", authenticated: true)
    }

    // MARK: - Locations

    func locations() async throws -> [Location] {
        try await get("/locations", authenticated: true)
    }

    // MARK: - Units

    func units() async throws -> [Unit] {
        try await get("/units", authenticated: true)
    }

    // MARK: - Products

    func searchProducts(query: String, limit: Int = 20) async throws -> [Product] {
        var components = URLComponents()
        components.path = "/products/search"
        components.queryItems = [
            URLQueryItem(name: "q", value: query),
            URLQueryItem(name: "limit", value: String(limit)),
        ]
        let path = components.url?.absoluteString ?? "/products/search"
        let response: ProductSearchResponse = try await get(path, authenticated: true)
        return response.items
    }

    func lookupBarcode(_ barcode: String) async throws -> BarcodeLookupResponse {
        let encoded = barcode.addingPercentEncoding(withAllowedCharacters: .urlPathAllowed) ?? barcode
        return try await get("/products/by-barcode/\(encoded)", authenticated: true)
    }

    func createProduct(_ request: CreateProductRequest) async throws -> Product {
        try await post("/products", body: request, authenticated: true)
    }

    func getProduct(id: UUID) async throws -> Product {
        try await get("/products/\(id.uuidString.lowercased())", authenticated: true)
    }

    func updateProduct(id: UUID, request: UpdateProductRequest) async throws -> Product {
        try await send(
            method: "PATCH",
            path: "/products/\(id.uuidString.lowercased())",
            body: request,
            authenticated: true,
        )
    }

    func deleteProduct(id: UUID) async throws {
        let _: EmptyResponse = try await send(
            method: "DELETE",
            path: "/products/\(id.uuidString.lowercased())",
            body: Optional<EmptyBody>.none,
            authenticated: true,
        )
    }

    func refreshProduct(id: UUID) async throws -> Product {
        try await post(
            "/products/\(id.uuidString.lowercased())/refresh",
            body: EmptyBody(),
            authenticated: true,
        )
    }

    // MARK: - Stock

    func listStock(
        locationID: UUID? = nil,
        productID: UUID? = nil,
        expiringBefore: String? = nil,
    ) async throws -> [StockBatch] {
        var components = URLComponents()
        components.path = "/stock"
        var items: [URLQueryItem] = []
        if let locationID {
            items.append(.init(name: "location_id", value: locationID.uuidString.lowercased()))
        }
        if let productID {
            items.append(.init(name: "product_id", value: productID.uuidString.lowercased()))
        }
        if let expiringBefore {
            items.append(.init(name: "expiring_before", value: expiringBefore))
        }
        if !items.isEmpty { components.queryItems = items }
        let path = components.url?.absoluteString ?? "/stock"
        let response: StockListResponse = try await get(path, authenticated: true)
        return response.items
    }

    func createStock(_ request: CreateStockRequest) async throws -> StockBatch {
        try await post("/stock", body: request, authenticated: true)
    }

    func updateStock(id: UUID, request: UpdateStockRequest) async throws -> StockBatch {
        try await send(
            method: "PATCH",
            path: "/stock/\(id.uuidString.lowercased())",
            body: request,
            authenticated: true,
        )
    }

    func deleteStock(id: UUID) async throws {
        let _: EmptyResponse = try await send(
            method: "DELETE",
            path: "/stock/\(id.uuidString.lowercased())",
            body: Optional<EmptyBody>.none,
            authenticated: true,
        )
    }

    func consumeStock(_ request: ConsumeRequest) async throws -> ConsumeResponse {
        try await post("/stock/consume", body: request, authenticated: true)
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
            let url = baseURL.appendingPathComponent(path).absoluteString
            // appendingPathComponent URL-encodes the whole thing if it has a
            // query string, so fall back to manual URL construction when the
            // path contains a "?".
            let finalURL: URL
            if path.contains("?") {
                finalURL = URL(string: baseURL.absoluteString + path) ?? baseURL
            } else {
                finalURL = URL(string: url) ?? baseURL.appendingPathComponent(path)
            }

            var request = URLRequest(url: finalURL)
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
