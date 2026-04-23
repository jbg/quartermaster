import Foundation
import HTTPTypes
import OpenAPIRuntime
import OpenAPIURLSession

/// Facade over the generated `Client`. Every feature view still calls
/// `appState.api.listStockEvents(...)` etc. — the signatures haven't moved;
/// the wire layer did. All JSON serialisation, URL construction, Codable
/// key mapping, and operation dispatch now live inside the generated
/// `Operations.*` machinery. We keep the facade so call sites stay stable
/// and our tailored error translation (`APIError.server(status:, body:)`)
/// remains the uniform surface for the rest of the app.
actor APIClient: AppStateAPI {
    private let client: Client
    private let tokenStore: TokenStore

    init(baseURL: URL, tokenStore: TokenStore, session: URLSession = .shared) {
        self.tokenStore = tokenStore
        let auth = AuthMiddleware(
            baseURL: baseURL,
            tokenStore: tokenStore,
            session: session,
        )
        let transport = URLSessionTransport(
            configuration: .init(session: session),
        )
        self.client = Client(
            serverURL: baseURL,
            transport: transport,
            middlewares: [auth],
        )
    }

    // MARK: - Accounts

    func register(username: String, password: String, email: String?, inviteCode: String? = nil) async throws -> TokenPair {
        let body = Operations.AuthRegister.Input.Body.json(.init(
            deviceLabel: Self.deviceLabel,
            email: email,
            inviteCode: inviteCode,
            password: password,
            username: username,
        ))
        let response = try await client.authRegister(.init(body: body))
        switch response {
        case .created(let ok):
            return try ok.body.json
        case .badRequest(let err):
            throw APIError.server(status: 400, body: try? err.body.json)
        case .forbidden(let err):
            throw APIError.server(status: 403, body: try? err.body.json)
        case .conflict(let err):
            throw APIError.server(status: 409, body: try? err.body.json)
        case .tooManyRequests(let err):
            throw APIError.server(status: 429, body: try? err.body.json)
        case .undocumented(let statusCode, _):
            throw APIError.server(status: statusCode, body: nil)
        }
    }

    func login(username: String, password: String) async throws -> TokenPair {
        let body = Operations.AuthLogin.Input.Body.json(.init(
            deviceLabel: Self.deviceLabel,
            password: password,
            username: username,
        ))
        let response = try await client.authLogin(.init(body: body))
        switch response {
        case .ok(let ok): return try ok.body.json
        case .unauthorized(let err): throw APIError.server(status: 401, body: try? err.body.json)
        case .tooManyRequests(let err): throw APIError.server(status: 429, body: try? err.body.json)
        case .undocumented(let statusCode, _): throw APIError.server(status: statusCode, body: nil)
        }
    }

    func logout() async throws {
        let response = try await client.authLogout(.init())
        switch response {
        case .noContent: return
        case .unauthorized: throw APIError.unauthorized
        case .undocumented(let statusCode, _):
            throw APIError.server(status: statusCode, body: nil)
        }
    }

    func me() async throws -> Me {
        let response = try await client.authMe(.init())
        switch response {
        case .ok(let ok): return try ok.body.json
        case .unauthorized: throw APIError.unauthorized
        case .undocumented(let statusCode, _):
            throw APIError.server(status: statusCode, body: nil)
        }
    }

    func switchHousehold(householdID: String) async throws -> Me {
        let response = try await client.authSwitchHousehold(.init(
            body: .json(.init(householdId: householdID)),
        ))
        switch response {
        case .ok(let ok): return try ok.body.json
        case .unauthorized: throw APIError.unauthorized
        case .forbidden(let err): throw APIError.server(status: 403, body: try? err.body.json)
        case .undocumented(let statusCode, _):
            throw APIError.server(status: statusCode, body: nil)
        }
    }

    // MARK: - Households

    func createHousehold(name: String, timezone: String) async throws -> Me {
        let response = try await client.householdCreate(.init(
            body: .json(.init(name: name, timezone: timezone)),
        ))
        switch response {
        case .created(let ok): return try ok.body.json
        case .badRequest(let err): throw APIError.server(status: 400, body: try? err.body.json)
        case .undocumented(let statusCode, _):
            throw APIError.server(status: statusCode, body: nil)
        }
    }

    func currentHousehold() async throws -> HouseholdDetail {
        let response = try await client.householdCurrentGet(.init())
        switch response {
        case .ok(let ok): return try ok.body.json
        case .unauthorized: throw APIError.unauthorized
        case .undocumented(let statusCode, _):
            throw APIError.server(status: statusCode, body: nil)
        }
    }

    func updateCurrentHousehold(name: String, timezone: String) async throws -> HouseholdDetail {
        let response = try await client.householdCurrentUpdate(.init(
            body: .json(.init(name: name, timezone: timezone)),
        ))
        switch response {
        case .ok(let ok): return try ok.body.json
        case .forbidden(let err): throw APIError.server(status: 403, body: try? err.body.json)
        case .undocumented(let statusCode, _):
            throw APIError.server(status: statusCode, body: nil)
        }
    }

    func householdMembers() async throws -> [Member] {
        let response = try await client.householdMembersList(.init())
        switch response {
        case .ok(let ok): return try ok.body.json
        case .undocumented(let statusCode, _):
            throw APIError.server(status: statusCode, body: nil)
        }
    }

    func removeHouseholdMember(userID: String) async throws {
        let response = try await client.householdMemberRemove(.init(path: .init(userId: userID)))
        switch response {
        case .noContent: return
        case .forbidden(let err): throw APIError.server(status: 403, body: try? err.body.json)
        case .conflict(let err): throw APIError.server(status: 409, body: try? err.body.json)
        case .undocumented(let statusCode, _):
            throw APIError.server(status: statusCode, body: nil)
        }
    }

    func householdInvites() async throws -> [Invite] {
        let response = try await client.householdInvitesList(.init())
        switch response {
        case .ok(let ok): return try ok.body.json
        case .undocumented(let statusCode, _):
            throw APIError.server(status: statusCode, body: nil)
        }
    }

    func createInvite(expiresAt: String, maxUses: Int, role: MembershipRole) async throws -> Invite {
        let response = try await client.householdInviteCreate(.init(
            body: .json(.init(expiresAt: expiresAt, maxUses: Int64(maxUses), roleGranted: role)),
        ))
        switch response {
        case .created(let ok): return try ok.body.json
        case .forbidden(let err): throw APIError.server(status: 403, body: try? err.body.json)
        case .undocumented(let statusCode, _):
            throw APIError.server(status: statusCode, body: nil)
        }
    }

    func revokeInvite(id: String) async throws {
        let response = try await client.inviteRevoke(.init(path: .init(id: id)))
        switch response {
        case .noContent: return
        case .forbidden(let err): throw APIError.server(status: 403, body: try? err.body.json)
        case .undocumented(let statusCode, _):
            throw APIError.server(status: statusCode, body: nil)
        }
    }

    func redeemInvite(code: String) async throws {
        let response = try await client.inviteRedeem(.init(body: .json(.init(inviteCode: code))))
        switch response {
        case .noContent: return
        case .badRequest(let err): throw APIError.server(status: 400, body: try? err.body.json)
        case .undocumented(let statusCode, _):
            throw APIError.server(status: statusCode, body: nil)
        }
    }

    // MARK: - Locations

    func locations() async throws -> [Location] {
        let response = try await client.locationsList(.init())
        switch response {
        case .ok(let ok): return try ok.body.json
        case .unauthorized: throw APIError.unauthorized
        case .undocumented(let statusCode, _):
            throw APIError.server(status: statusCode, body: nil)
        }
    }

    func createLocation(name: String, kind: String, sortOrder: Int? = nil) async throws -> Location {
        let response = try await client.locationsCreate(.init(
            body: .json(.init(kind: kind, name: name, sortOrder: sortOrder.map(Int64.init))),
        ))
        switch response {
        case .created(let ok): return try ok.body.json
        case .undocumented(let statusCode, _):
            throw APIError.server(status: statusCode, body: nil)
        }
    }

    func updateLocation(id: String, name: String, kind: String, sortOrder: Int) async throws -> Location {
        let response = try await client.locationsUpdate(.init(
            path: .init(id: id),
            body: .json(.init(kind: kind, name: name, sortOrder: Int64(sortOrder))),
        ))
        switch response {
        case .ok(let ok): return try ok.body.json
        case .undocumented(let statusCode, _):
            throw APIError.server(status: statusCode, body: nil)
        }
    }

    func deleteLocation(id: String) async throws {
        let response = try await client.locationsDelete(.init(path: .init(id: id)))
        switch response {
        case .noContent: return
        case .conflict(let err): throw APIError.server(status: 409, body: try? err.body.json)
        case .undocumented(let statusCode, _):
            throw APIError.server(status: statusCode, body: nil)
        }
    }

    // MARK: - Units

    func units() async throws -> [Unit] {
        let response = try await client.unitsList(.init())
        switch response {
        case .ok(let ok): return try ok.body.json
        case .undocumented(let statusCode, _):
            throw APIError.server(status: statusCode, body: nil)
        }
    }

    // MARK: - Products

    func searchProducts(query: String, limit: Int = 20, includeDeleted: Bool = false) async throws -> [Product] {
        let response = try await client.productSearch(.init(
            query: .init(q: query, limit: Int64(limit), includeDeleted: includeDeleted),
        ))
        switch response {
        case .ok(let ok):
            let payload = try ok.body.json
            return payload.items
        case .unauthorized: throw APIError.unauthorized
        case .undocumented(let statusCode, _):
            throw APIError.server(status: statusCode, body: nil)
        }
    }

    func lookupBarcode(_ barcode: String) async throws -> BarcodeLookupResponse {
        let response = try await client.productByBarcode(.init(
            path: .init(barcode: barcode),
        ))
        switch response {
        case .ok(let ok): return try ok.body.json
        case .badRequest(let err): throw APIError.server(status: 400, body: try? err.body.json)
        case .notFound(let err): throw APIError.server(status: 404, body: try? err.body.json)
        case .tooManyRequests(let err): throw APIError.server(status: 429, body: try? err.body.json)
        case .badGateway(let err): throw APIError.server(status: 502, body: try? err.body.json)
        case .undocumented(let statusCode, _):
            throw APIError.server(status: statusCode, body: nil)
        }
    }

    func createProduct(_ request: CreateProductRequest) async throws -> Product {
        let response = try await client.productCreate(.init(body: .json(request)))
        switch response {
        case .created(let ok): return try ok.body.json
        case .badRequest(let err): throw APIError.server(status: 400, body: try? err.body.json)
        case .undocumented(let statusCode, _):
            throw APIError.server(status: statusCode, body: nil)
        }
    }

    func getProduct(id: String) async throws -> Product {
        let response = try await client.productGet(.init(path: .init(id: id)))
        switch response {
        case .ok(let ok): return try ok.body.json
        case .notFound: throw APIError.server(status: 404, body: nil)
        case .undocumented(let statusCode, _):
            throw APIError.server(status: statusCode, body: nil)
        }
    }

    func updateProduct(id: String, request: UpdateProductRequest) async throws -> Product {
        let response = try await client.productUpdate(.init(
            path: .init(id: id),
            body: .json(request),
        ))
        switch response {
        case .ok(let ok): return try ok.body.json
        case .badRequest(let err): throw APIError.server(status: 400, body: try? err.body.json)
        case .forbidden(let err): throw APIError.server(status: 403, body: try? err.body.json)
        case .notFound(let err): throw APIError.server(status: 404, body: try? err.body.json)
        case .conflict(let err): throw APIError.server(status: 409, body: try? err.body.json)
        case .undocumented(let statusCode, _):
            throw APIError.server(status: statusCode, body: nil)
        }
    }

    func deleteProduct(id: String) async throws {
        let response = try await client.productDelete(.init(path: .init(id: id)))
        switch response {
        case .noContent: return
        case .forbidden(let err): throw APIError.server(status: 403, body: try? err.body.json)
        case .notFound(let err): throw APIError.server(status: 404, body: try? err.body.json)
        case .conflict(let err): throw APIError.server(status: 409, body: try? err.body.json)
        case .undocumented(let statusCode, _):
            throw APIError.server(status: statusCode, body: nil)
        }
    }

    func refreshProduct(id: String) async throws -> Product {
        let response = try await client.productRefresh(.init(path: .init(id: id)))
        switch response {
        case .ok(let ok): return try ok.body.json
        case .badRequest(let err): throw APIError.server(status: 400, body: try? err.body.json)
        case .notFound(let err): throw APIError.server(status: 404, body: try? err.body.json)
        case .badGateway(let err): throw APIError.server(status: 502, body: try? err.body.json)
        case .undocumented(let statusCode, _):
            throw APIError.server(status: statusCode, body: nil)
        }
    }

    func restoreProduct(id: String) async throws -> Product {
        let response = try await client.productRestore(.init(path: .init(id: id)))
        switch response {
        case .ok(let ok): return try ok.body.json
        case .forbidden(let err): throw APIError.server(status: 403, body: try? err.body.json)
        case .notFound(let err): throw APIError.server(status: 404, body: try? err.body.json)
        case .undocumented(let statusCode, _):
            throw APIError.server(status: statusCode, body: nil)
        }
    }

    // MARK: - Stock

    func listStock(
        locationID: String? = nil,
        productID: String? = nil,
        expiringBefore: String? = nil,
    ) async throws -> [StockBatch] {
        let response = try await client.stockList(.init(
            query: .init(
                locationId: locationID,
                productId: productID,
                expiringBefore: expiringBefore,
            ),
        ))
        switch response {
        case .ok(let ok):
            return try ok.body.json.items
        case .unauthorized: throw APIError.unauthorized
        case .undocumented(let statusCode, _):
            throw APIError.server(status: statusCode, body: nil)
        }
    }

    func getStock(id: String) async throws -> StockBatch {
        let response = try await client.stockGet(.init(path: .init(id: id)))
        switch response {
        case .ok(let ok): return try ok.body.json
        case .notFound(let err): throw APIError.server(status: 404, body: try? err.body.json)
        case .undocumented(let statusCode, _):
            throw APIError.server(status: statusCode, body: nil)
        }
    }

    func createStock(_ request: CreateStockRequest) async throws -> StockBatch {
        let response = try await client.stockCreate(.init(body: .json(request)))
        switch response {
        case .created(let ok): return try ok.body.json
        case .badRequest(let err): throw APIError.server(status: 400, body: try? err.body.json)
        case .notFound(let err): throw APIError.server(status: 404, body: try? err.body.json)
        case .undocumented(let statusCode, _):
            throw APIError.server(status: statusCode, body: nil)
        }
    }

    func updateStock(id: String, request: UpdateStockRequest) async throws -> StockBatch {
        let response = try await client.stockUpdate(.init(
            path: .init(id: id),
            body: .json(request),
        ))
        switch response {
        case .ok(let ok): return try ok.body.json
        case .badRequest(let err): throw APIError.server(status: 400, body: try? err.body.json)
        case .notFound(let err): throw APIError.server(status: 404, body: try? err.body.json)
        case .undocumented(let statusCode, _):
            throw APIError.server(status: statusCode, body: nil)
        }
    }

    func deleteStock(id: String) async throws {
        let response = try await client.stockDelete(.init(path: .init(id: id)))
        switch response {
        case .noContent: return
        case .notFound(let err): throw APIError.server(status: 404, body: try? err.body.json)
        case .undocumented(let statusCode, _):
            throw APIError.server(status: statusCode, body: nil)
        }
    }

    func consumeStock(_ request: ConsumeRequest) async throws -> ConsumeResponse {
        let response = try await client.stockConsume(.init(body: .json(request)))
        switch response {
        case .ok(let ok): return try ok.body.json
        case .badRequest(let err): throw APIError.server(status: 400, body: try? err.body.json)
        case .notFound(let err): throw APIError.server(status: 404, body: try? err.body.json)
        case .undocumented(let statusCode, _):
            throw APIError.server(status: statusCode, body: nil)
        }
    }

    func listStockEvents(
        beforeCreatedAt: String? = nil,
        beforeID: String? = nil,
        limit: Int = 50,
    ) async throws -> StockEventListResponse {
        let response = try await client.stockListEvents(.init(
            query: .init(
                beforeCreatedAt: beforeCreatedAt,
                beforeId: beforeID,
                limit: Int64(limit),
            ),
        ))
        switch response {
        case .ok(let ok): return try ok.body.json
        case .unauthorized: throw APIError.unauthorized
        case .tooManyRequests(let err): throw APIError.server(status: 429, body: try? err.body.json)
        case .undocumented(let statusCode, _):
            throw APIError.server(status: statusCode, body: nil)
        }
    }

    func listBatchEvents(
        id: String,
        beforeCreatedAt: String? = nil,
        beforeID: String? = nil,
        limit: Int = 50,
    ) async throws -> StockEventListResponse {
        let response = try await client.stockListBatchEvents(.init(
            path: .init(id: id),
            query: .init(
                beforeCreatedAt: beforeCreatedAt,
                beforeId: beforeID,
                limit: Int64(limit),
            ),
        ))
        switch response {
        case .ok(let ok): return try ok.body.json
        case .notFound(let err): throw APIError.server(status: 404, body: try? err.body.json)
        case .tooManyRequests(let err): throw APIError.server(status: 429, body: try? err.body.json)
        case .undocumented(let statusCode, _):
            throw APIError.server(status: statusCode, body: nil)
        }
    }

    func restoreStock(id: String) async throws -> StockBatch {
        let response = try await client.stockRestore(.init(path: .init(id: id)))
        switch response {
        case .ok(let ok): return try ok.body.json
        case .notFound(let err): throw APIError.server(status: 404, body: try? err.body.json)
        case .conflict(let err): throw APIError.server(status: 409, body: try? err.body.json)
        case .undocumented(let statusCode, _):
            throw APIError.server(status: statusCode, body: nil)
        }
    }

    func restoreManyStock(ids: [String]) async throws -> RestoreManyResponse {
        let response = try await client.stockRestoreMany(.init(
            body: .json(.init(ids: ids)),
        ))
        switch response {
        case .ok(let ok): return try ok.body.json
        case .badRequest(let err): throw APIError.server(status: 400, body: try? err.body.json)
        case .conflict(let err): throw APIError.server(status: 409, body: try? err.body.json)
        case .undocumented(let statusCode, _):
            throw APIError.server(status: statusCode, body: nil)
        }
    }

    func listReminders(
        afterFireAt: String? = nil,
        afterID: String? = nil,
        limit: Int = 50,
    ) async throws -> ReminderListResponse {
        let response = try await client.remindersList(.init(
            query: .init(
                afterFireAt: afterFireAt,
                afterId: afterID,
                limit: Int64(limit),
            ),
        ))
        switch response {
        case .ok(let ok): return try ok.body.json
        case .badRequest(let err): throw APIError.server(status: 400, body: try? err.body.json)
        case .unauthorized: throw APIError.unauthorized
        case .undocumented(let statusCode, _):
            throw APIError.server(status: statusCode, body: nil)
        }
    }

    func presentReminder(id: String) async throws {
        let response = try await client.remindersPresent(.init(path: .init(id: id)))
        switch response {
        case .noContent: return
        case .unauthorized: throw APIError.unauthorized
        case .notFound(let err): throw APIError.server(status: 404, body: try? err.body.json)
        case .undocumented(let statusCode, _):
            throw APIError.server(status: statusCode, body: nil)
        }
    }

    func ackReminder(id: String) async throws {
        let response = try await client.remindersAck(.init(path: .init(id: id)))
        switch response {
        case .noContent: return
        case .unauthorized: throw APIError.unauthorized
        case .notFound(let err): throw APIError.server(status: 404, body: try? err.body.json)
        case .undocumented(let statusCode, _):
            throw APIError.server(status: statusCode, body: nil)
        }
    }

    func openReminder(id: String) async throws {
        let response = try await client.remindersOpen(.init(path: .init(id: id)))
        switch response {
        case .noContent: return
        case .unauthorized: throw APIError.unauthorized
        case .notFound(let err): throw APIError.server(status: 404, body: try? err.body.json)
        case .undocumented(let statusCode, _):
            throw APIError.server(status: statusCode, body: nil)
        }
    }

    func registerDevice(
        deviceID: String,
        pushToken: String?,
        pushAuthorization: PushAuthorizationStatus,
        appVersion: String?,
    ) async throws {
        let response = try await client.deviceRegister(.init(
            body: .json(.init(
                appVersion: appVersion,
                deviceId: deviceID,
                platform: "ios",
                pushAuthorization: pushAuthorization,
                pushToken: pushToken,
            )),
        ))
        switch response {
        case .noContent: return
        case .unauthorized: throw APIError.unauthorized
        case .undocumented(let statusCode, _):
            throw APIError.server(status: statusCode, body: nil)
        }
    }

    private static let deviceLabel: String? = {
        #if os(iOS)
        return "iOS"
        #else
        return nil
        #endif
    }()
}

// MARK: - Auth middleware

/// Attaches the bearer token from `TokenStore` on every operation that
/// isn't itself an auth endpoint. On 401, serialises a single refresh
/// call (concurrent 401s coalesce on the same refresh task) and retries
/// the original request with the new token. Failure to refresh clears
/// the stored tokens and surfaces as an unauthenticated response.
private actor AuthMiddleware: ClientMiddleware {
    private let baseURL: URL
    private let tokenStore: TokenStore
    private let session: URLSession
    private var inFlightRefresh: Task<Void, Error>?

    init(baseURL: URL, tokenStore: TokenStore, session: URLSession) {
        self.baseURL = baseURL
        self.tokenStore = tokenStore
        self.session = session
    }

    nonisolated func intercept(
        _ request: HTTPRequest,
        body: HTTPBody?,
        baseURL: URL,
        operationID: String,
        next: @Sendable (HTTPRequest, HTTPBody?, URL) async throws -> (HTTPResponse, HTTPBody?),
    ) async throws -> (HTTPResponse, HTTPBody?) {
        let bypasses = Self.noAuthOperations.contains(operationID)

        var authedRequest = request
        if !bypasses, let token = await tokenStore.accessToken {
            authedRequest.headerFields[.authorization] = "Bearer \(token)"
        }

        let (response, responseBody) = try await next(authedRequest, body, baseURL)

        guard !bypasses, response.status.code == 401 else {
            return (response, responseBody)
        }

        // Try to refresh once. Concurrent 401s coalesce on the same task.
        do {
            try await runRefresh()
        } catch {
            await tokenStore.clear()
            return (response, responseBody)
        }

        // Retry with the fresh token. Note: the request body is reused.
        var retryRequest = request
        if let newToken = await tokenStore.accessToken {
            retryRequest.headerFields[.authorization] = "Bearer \(newToken)"
        }
        return try await next(retryRequest, body, baseURL)
    }

    private func runRefresh() async throws {
        if let existing = inFlightRefresh {
            try await existing.value
            return
        }
        let task = Task<Void, Error> { [baseURL, tokenStore, session] in
            guard let refreshToken = await tokenStore.refreshToken else {
                throw APIError.unauthorized
            }
            var req = URLRequest(url: baseURL.appendingPathComponent("/auth/refresh"))
            req.httpMethod = "POST"
            req.setValue("application/json", forHTTPHeaderField: "Content-Type")
            req.httpBody = try JSONEncoder().encode(RefreshBody(refreshToken: refreshToken))
            let (data, response) = try await session.data(for: req)
            guard
                let http = response as? HTTPURLResponse,
                (200..<300).contains(http.statusCode)
            else {
                throw APIError.unauthorized
            }
            let pair = try JSONDecoder().decode(StoredPair.self, from: data)
            await tokenStore.store(TokenPair(
                accessToken: pair.accessToken,
                expiresIn: Int64(pair.expiresIn),
                refreshToken: pair.refreshToken,
                tokenType: pair.tokenType,
            ))
        }
        inFlightRefresh = task
        defer { inFlightRefresh = nil }
        try await task.value
    }

    /// Operations that must never be retried with a bearer or rerouted
    /// through the refresh loop — otherwise a bad refresh token would
    /// infinitely trigger itself.
    private static let noAuthOperations: Set<String> = [
        "auth_login",
        "auth_register",
        "auth_refresh",
    ]
}

// Intermediate types used by the refresh plumbing — mirror the wire
// contract without going through the generated `Client` (which would
// itself route through this middleware and recurse).
private struct RefreshBody: Encodable {
    let refreshToken: String
    enum CodingKeys: String, CodingKey { case refreshToken = "refresh_token" }
}

private struct StoredPair: Decodable {
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
