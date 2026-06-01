import Foundation
import HTTPTypes
import OpenAPIRuntime
import OpenAPIURLSession

private struct MealPlanSlotBody: Encodable {
  let key: String
  let label: String
}

private struct MealPlanGenerateBody: Encodable {
  let title: String?
  let dates: [String]
  let slots: [MealPlanSlotBody]
  let constraints: [String: String] = [:]
}

private struct MealPlanRefreshBody: Decodable {
  let plan: MealPlan
}

/// Facade over the generated `Client`. Feature views still call
/// `appState.api.listStockEvents(...)` etc.; the facade keeps those call sites
/// stable while the API boundary uses generated OpenAPI request/response
/// shapes. A few endpoints still use explicit URLSession calls, but those
/// calls encode and decode generated `Components.Schemas.*` types before
/// adapting into app-facing models.
actor APIClient: AppStateAPI {
  private let client: Client
  private let tokenStore: TokenStore
  private let baseURL: URL
  private let session: URLSession
  private let labelPrinterClient: LabelPrinterSending
  private let jsonDecoder: JSONDecoder
  private let jsonEncoder: JSONEncoder

  init(
    baseURL: URL,
    tokenStore: TokenStore,
    session: URLSession = .shared,
    labelPrinterClient: LabelPrinterSending = LabelPrinterClient()
  ) {
    self.tokenStore = tokenStore
    self.baseURL = baseURL
    self.session = session
    self.labelPrinterClient = labelPrinterClient
    self.jsonDecoder = JSONDecoder()
    self.jsonEncoder = JSONEncoder()
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

  func onboardingStatus() async throws -> OnboardingStatus {
    let response = try await client.onboardingStatus(.init())
    switch response {
    case .ok(let ok): return try ok.body.json
    case .undocumented(let statusCode, _):
      throw APIError.server(status: statusCode, body: nil)
    }
  }

  func createOnboardingHousehold(
    email: String,
    displayName: String,
    password: String,
    householdName: String,
    timezone: String
  ) async throws -> TokenPair {
    let body = Operations.OnboardingCreateHousehold.Input.Body.json(
      .init(
        deviceLabel: Self.deviceLabel,
        displayName: displayName,
        email: email,
        householdName: householdName,
        password: password,
        timezone: timezone,
      ))
    let response = try await client.onboardingCreateHousehold(.init(body: body))
    switch response {
    case .created(let ok): return try ok.body.json
    case .badRequest(let err): throw APIError.server(status: 400, body: try? err.body.json)
    case .forbidden(let err): throw APIError.server(status: 403, body: try? err.body.json)
    case .conflict(let err): throw APIError.server(status: 409, body: try? err.body.json)
    case .tooManyRequests(let err): throw APIError.server(status: 429, body: try? err.body.json)
    case .undocumented(let statusCode, _):
      throw APIError.server(status: statusCode, body: nil)
    }
  }

  func joinOnboardingInvite(
    email: String, displayName: String, password: String, inviteCode: String
  )
    async throws -> TokenPair
  {
    let body = Operations.OnboardingJoinInvite.Input.Body.json(
      .init(
        deviceLabel: Self.deviceLabel,
        displayName: displayName,
        email: email,
        inviteCode: inviteCode,
        password: password,
      ))
    let response = try await client.onboardingJoinInvite(.init(body: body))
    switch response {
    case .created(let ok): return try ok.body.json
    case .badRequest(let err): throw APIError.server(status: 400, body: try? err.body.json)
    case .forbidden(let err): throw APIError.server(status: 403, body: try? err.body.json)
    case .conflict(let err): throw APIError.server(status: 409, body: try? err.body.json)
    case .tooManyRequests(let err): throw APIError.server(status: 429, body: try? err.body.json)
    case .undocumented(let statusCode, _):
      throw APIError.server(status: statusCode, body: nil)
    }
  }

  func register(email: String, displayName: String, password: String, inviteCode: String? = nil)
    async throws -> TokenPair
  {
    let body = Operations.AuthRegister.Input.Body.json(
      .init(
        deviceLabel: Self.deviceLabel,
        displayName: displayName,
        email: email,
        inviteCode: inviteCode,
        password: password,
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

  func login(email: String, password: String) async throws -> TokenPair {
    let body = Operations.AuthLogin.Input.Body.json(
      .init(
        deviceLabel: Self.deviceLabel,
        email: email,
        password: password,
      ))
    let response = try await client.authLogin(.init(body: body))
    switch response {
    case .ok(let ok): return try ok.body.json
    case .unauthorized(let err): throw APIError.server(status: 401, body: try? err.body.json)
    case .tooManyRequests(let err): throw APIError.server(status: 429, body: try? err.body.json)
    case .undocumented(let statusCode, _): throw APIError.server(status: statusCode, body: nil)
    }
  }

  func listPasskeys() async throws -> [PasskeyCredentialSummary] {
    let response: Components.Schemas.PasskeyListResponse = try await rawJSON(
      "GET",
      "/auth/passkeys",
      body: Optional<Int>.none,
      auth: true
    )
    return response.credentials.map(Self.passkeySummary)
  }

  func startPasskeyRegistration(label: String?) async throws -> PasskeyRegistrationStart {
    let response: Components.Schemas.PasskeyRegistrationStartResponse = try await rawJSON(
      "POST",
      "/auth/passkeys/register/start",
      body: Components.Schemas.PasskeyRegistrationStartRequest(label: label),
      auth: true
    )
    return PasskeyRegistrationStart(
      ceremonyID: response.ceremonyId, publicKey: try Self.openAPIValueData(response.publicKey))
  }

  func finishPasskeyRegistration(ceremonyID: String, credentialJSON: Data, label: String?)
    async throws -> PasskeyCredentialSummary
  {
    let response: Components.Schemas.PasskeyCredentialDto = try await rawJSON(
      "POST",
      "/auth/passkeys/register/finish",
      body: Components.Schemas.PasskeyRegistrationFinishRequest(
        ceremonyId: ceremonyID,
        credential: try Self.openAPIValue(from: credentialJSON),
        label: label
      ),
      auth: true,
      successStatus: 201
    )
    return Self.passkeySummary(response)
  }

  func startPasskeyLogin(email: String) async throws -> PasskeyLoginStart {
    let response: Components.Schemas.PasskeyLoginStartResponse = try await rawJSON(
      "POST",
      "/auth/passkeys/login/start",
      body: Components.Schemas.PasskeyLoginStartRequest(email: email),
      auth: false
    )
    return PasskeyLoginStart(
      ceremonyID: response.ceremonyId,
      publicKey: try Self.openAPIValueData(response.publicKey))
  }

  func finishPasskeyLogin(ceremonyID: String, credentialJSON: Data) async throws -> TokenPair {
    let pair: TokenPair = try await rawJSON(
      "POST",
      "/auth/passkeys/login/finish",
      body: Components.Schemas.PasskeyLoginFinishRequest(
        ceremonyId: ceremonyID,
        credential: try Self.openAPIValue(from: credentialJSON),
        deviceLabel: Self.deviceLabel
      ),
      auth: false
    )
    await tokenStore.store(pair)
    return pair
  }

  func deletePasskey(id: String) async throws {
    try await rawUnit("DELETE", "/auth/passkeys/\(id)", auth: true)
  }

  func createAuthHandoff(targetDeviceLabel: String?, serverURL: String?) async throws
    -> AuthHandoffCreate
  {
    let response: Components.Schemas.AuthHandoffCreateResponse = try await rawJSON(
      "POST",
      "/auth/handoffs",
      body: Components.Schemas.CreateAuthHandoffRequest(
        serverUrl: serverURL,
        targetDeviceLabel: targetDeviceLabel),
      auth: true,
      successStatus: 201
    )
    return AuthHandoffCreate(
      id: response.id,
      handoffURL: response.handoffUrl,
      expiresAt: response.expiresAt,
      targetDeviceLabel: response.targetDeviceLabel)
  }

  func cancelAuthHandoff(id: String) async throws {
    try await rawUnit("DELETE", "/auth/handoffs/\(id)", auth: true)
  }

  func previewAuthHandoff(id: String, token: String) async throws -> AuthHandoffPreview {
    let response: Components.Schemas.AuthHandoffPreviewResponse = try await rawJSON(
      "POST",
      "/auth/handoffs/preview",
      body: Components.Schemas.AuthHandoffTokenRequest(id: id, token: token),
      auth: false
    )
    return AuthHandoffPreview(
      id: response.id,
      sourceEmail: response.sourceEmail,
      sourceDisplayName: response.sourceDisplayName,
      householdID: response.householdId,
      targetDeviceLabel: response.targetDeviceLabel,
      expiresAt: response.expiresAt)
  }

  func acceptAuthHandoff(id: String, token: String, deviceLabel: String?) async throws -> TokenPair
  {
    let pair: TokenPair = try await rawJSON(
      "POST",
      "/auth/handoffs/accept",
      body: Components.Schemas.AuthHandoffAcceptRequest(
        deviceLabel: deviceLabel ?? Self.deviceLabel,
        id: id,
        token: token),
      auth: false
    )
    await tokenStore.store(pair)
    return pair
  }

  func requestPasswordReset(email: String) async throws {
    let body = Operations.AuthPasswordResetRequest.Input.Body.json(.init(email: email))
    let response = try await client.authPasswordResetRequest(.init(body: body))
    switch response {
    case .accepted: return
    case .badRequest(let err): throw APIError.server(status: 400, body: try? err.body.json)
    case .tooManyRequests(let err): throw APIError.server(status: 429, body: try? err.body.json)
    case .undocumented(let statusCode, _): throw APIError.server(status: statusCode, body: nil)
    }
  }

  func confirmPasswordReset(email: String, newPassword: String, code: String) async throws {
    let body = Operations.AuthPasswordResetConfirm.Input.Body.json(
      .init(
        code: code,
        email: email,
        newPassword: newPassword,
        token: nil
      ))
    let response = try await client.authPasswordResetConfirm(.init(body: body))
    switch response {
    case .noContent: return
    case .badRequest(let err): throw APIError.server(status: 400, body: try? err.body.json)
    case .tooManyRequests(let err): throw APIError.server(status: 429, body: try? err.body.json)
    case .undocumented(let statusCode, _): throw APIError.server(status: statusCode, body: nil)
    }
  }

  func requestEmailVerification(email: String) async throws -> RequestEmailVerificationResponse {
    let body = Operations.AuthEmailVerificationRequest.Input.Body.json(.init(email: email))
    let response = try await client.authEmailVerificationRequest(.init(body: body))
    switch response {
    case .ok(let ok): return try ok.body.json
    case .badRequest(let err): throw APIError.server(status: 400, body: try? err.body.json)
    case .unauthorized: throw APIError.unauthorized
    case .serviceUnavailable(let err): throw APIError.server(status: 503, body: try? err.body.json)
    case .undocumented(let statusCode, _): throw APIError.server(status: statusCode, body: nil)
    }
  }

  func confirmEmailVerification(code: String) async throws -> Me {
    let body = Operations.AuthEmailVerificationConfirm.Input.Body.json(.init(code: code))
    let response = try await client.authEmailVerificationConfirm(.init(body: body))
    switch response {
    case .ok(let ok): return try ok.body.json
    case .badRequest(let err): throw APIError.server(status: 400, body: try? err.body.json)
    case .unauthorized: throw APIError.unauthorized
    case .undocumented(let statusCode, _): throw APIError.server(status: statusCode, body: nil)
    }
  }

  func clearRecoveryEmail() async throws -> Me {
    let response = try await client.authEmailClear(.init())
    switch response {
    case .ok(let ok): return try ok.body.json
    case .unauthorized: throw APIError.unauthorized
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
    let response = try await client.authSwitchHousehold(
      .init(
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
    let response = try await client.householdCreate(
      .init(
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

  func updateCurrentHousehold(
    name: String,
    timezone: String,
    measurementSystem: MeasurementSystem,
  ) async throws -> HouseholdDetail {
    let response = try await client.householdCurrentUpdate(
      .init(
        body: .json(.init(measurementSystem: measurementSystem, name: name, timezone: timezone)),
      ))
    switch response {
    case .ok(let ok): return try ok.body.json
    case .forbidden(let err): throw APIError.server(status: 403, body: try? err.body.json)
    case .undocumented(let statusCode, _):
      throw APIError.server(status: statusCode, body: nil)
    }
  }

  func exportCurrentHousehold() async throws -> HouseholdExportDocument {
    let response = try await client.householdCurrentExport(.init())
    switch response {
    case .ok(let ok): return try ok.body.json
    case .forbidden(let err): throw APIError.server(status: 403, body: try? err.body.json)
    case .undocumented(let statusCode, _):
      throw APIError.server(status: statusCode, body: nil)
    }
  }

  func importHousehold(_ document: HouseholdExportDocument) async throws -> Me {
    let response = try await client.householdImport(.init(body: .json(document)))
    switch response {
    case .created(let created): return try created.body.json
    case .badRequest(let err): throw APIError.server(status: 400, body: try? err.body.json)
    case .undocumented(let statusCode, _):
      throw APIError.server(status: statusCode, body: nil)
    }
  }

  func requestCurrentHouseholdDeletion(confirmationName: String) async throws
    -> DeleteHouseholdResponse
  {
    let response = try await client.householdCurrentDeletionRequest(
      .init(body: .json(.init(confirmationName: confirmationName))))
    switch response {
    case .accepted(let accepted): return try accepted.body.json
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

  func createInvite(maxUses: Int, role: MembershipRole) async throws -> Invite {
    let response = try await client.householdInviteCreate(
      .init(
        body: .json(.init(maxUses: Int64(maxUses), roleGranted: role)),
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
    let response = try await client.locationsCreate(
      .init(
        body: .json(.init(kind: kind, name: name, sortOrder: sortOrder.map(Int64.init))),
      ))
    switch response {
    case .created(let ok): return try ok.body.json
    case .undocumented(let statusCode, _):
      throw APIError.server(status: statusCode, body: nil)
    }
  }

  func updateLocation(id: String, name: String, kind: String, sortOrder: Int) async throws
    -> Location
  {
    let response = try await client.locationsUpdate(
      .init(
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

  func searchProducts(query: String, limit: Int = 20, includeDeleted: Bool = false) async throws
    -> [Product]
  {
    let response = try await client.productSearch(
      .init(
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
    let response = try await client.productByBarcode(
      .init(
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
    let response = try await client.productUpdate(
      .init(
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

  func openFoodFactsCredentialStatus() async throws -> OpenFoodFactsCredentialStatusResponse {
    let response = try await client.accountOpenfoodfactsStatus(.init())
    switch response {
    case .ok(let ok): return try ok.body.json
    case .unauthorized(let err): throw APIError.server(status: 401, body: try? err.body.json)
    case .undocumented(let statusCode, _):
      throw APIError.server(status: statusCode, body: nil)
    }
  }

  func saveOpenFoodFactsCredentials(username: String, password: String) async throws
    -> OpenFoodFactsCredentialStatusResponse
  {
    let body = Operations.AccountOpenfoodfactsPut.Input.Body.json(
      .init(password: password, username: username))
    let response = try await client.accountOpenfoodfactsPut(.init(body: body))
    switch response {
    case .ok(let ok): return try ok.body.json
    case .badRequest(let err): throw APIError.server(status: 400, body: try? err.body.json)
    case .unauthorized(let err): throw APIError.server(status: 401, body: try? err.body.json)
    case .serviceUnavailable(let err): throw APIError.server(status: 503, body: try? err.body.json)
    case .undocumented(let statusCode, _):
      throw APIError.server(status: statusCode, body: nil)
    }
  }

  func deleteOpenFoodFactsCredentials() async throws {
    let response = try await client.accountOpenfoodfactsDelete(.init())
    switch response {
    case .noContent: return
    case .unauthorized(let err): throw APIError.server(status: 401, body: try? err.body.json)
    case .undocumented(let statusCode, _):
      throw APIError.server(status: statusCode, body: nil)
    }
  }

  func offContributionPreview(productID: String) async throws -> OffContributionPreviewResponse {
    let response = try await client.productOffContributionPreview(.init(path: .init(id: productID)))
    switch response {
    case .ok(let ok): return try ok.body.json
    case .notFound(let err): throw APIError.server(status: 404, body: try? err.body.json)
    case .undocumented(let statusCode, _):
      throw APIError.server(status: statusCode, body: nil)
    }
  }

  func contributeProductToOFF(id: String) async throws -> OffContributionResponse {
    let response = try await client.productOffContribution(.init(path: .init(id: id)))
    switch response {
    case .ok(let ok): return try ok.body.json
    case .badRequest(let err): throw APIError.server(status: 400, body: try? err.body.json)
    case .unauthorized(let err): throw APIError.server(status: 401, body: try? err.body.json)
    case .notFound(let err): throw APIError.server(status: 404, body: try? err.body.json)
    case .conflict(let err): throw APIError.server(status: 409, body: try? err.body.json)
    case .preconditionRequired(let err):
      throw APIError.server(status: 428, body: try? err.body.json)
    case .badGateway(let err): throw APIError.server(status: 502, body: try? err.body.json)
    case .serviceUnavailable(let err): throw APIError.server(status: 503, body: try? err.body.json)
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
    includeDepleted: Bool = false,
  ) async throws -> [StockBatch] {
    let response = try await client.stockList(
      .init(
        query: .init(
          locationId: locationID,
          productId: productID,
          expiringBefore: expiringBefore,
          includeDepleted: includeDepleted,
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
    let response = try await client.stockUpdate(
      .init(
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

  // MARK: - Storage Vessels

  func storageVessels() async throws -> [StorageVessel] {
    let response = try await client.storageVesselsList(.init())
    switch response {
    case .ok(let ok): return try ok.body.json
    case .unauthorized: throw APIError.unauthorized
    case .undocumented(let statusCode, _):
      throw APIError.server(status: statusCode, body: nil)
    }
  }

  func createStorageVessel(
    name: String,
    tareWeight: String,
    tareUnit: String,
    sortOrder: Int? = nil,
  ) async throws -> StorageVessel {
    let request = CreateStorageVesselRequest(
      name: name,
      sortOrder: sortOrder.map(Int64.init),
      tareUnit: tareUnit,
      tareWeight: tareWeight,
    )
    let response = try await client.storageVesselsCreate(.init(body: .json(request)))
    switch response {
    case .created(let ok): return try ok.body.json
    case .undocumented(let statusCode, _):
      throw APIError.server(status: statusCode, body: nil)
    }
  }

  func updateStorageVessel(
    id: String,
    name: String,
    tareWeight: String,
    tareUnit: String,
    sortOrder: Int,
  ) async throws -> StorageVessel {
    let request = UpdateStorageVesselRequest(
      name: name,
      sortOrder: Int64(sortOrder),
      tareUnit: tareUnit,
      tareWeight: tareWeight,
    )
    let response = try await client.storageVesselsUpdate(
      .init(path: .init(id: id), body: .json(request)))
    switch response {
    case .ok(let ok): return try ok.body.json
    case .undocumented(let statusCode, _):
      throw APIError.server(status: statusCode, body: nil)
    }
  }

  func deleteStorageVessel(id: String) async throws {
    let response = try await client.storageVesselsDelete(.init(path: .init(id: id)))
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

  func splitStock(id: String, request: SplitStockRequest) async throws -> SplitStockResponse {
    let response = try await client.stockSplit(
      .init(path: .init(id: id), body: .json(request)))
    switch response {
    case .ok(let ok): return try ok.body.json
    case .badRequest(let err): throw APIError.server(status: 400, body: try? err.body.json)
    case .notFound(let err): throw APIError.server(status: 404, body: try? err.body.json)
    case .undocumented(let statusCode, _):
      throw APIError.server(status: statusCode, body: nil)
    }
  }

  func printStockLabel(
    id: String,
    copies: Int = 1,
    includeQuantity: Bool = false,
    labelSize: LabelPrintSize = .standard,
  ) async throws
    -> PrintStockLabelResponse
  {
    let request = PrintStockLabelRequest(
      copies: Int32(copies),
      dryRun: nil,
      includeQuantity: includeQuantity,
      labelSize: labelSize.rawValue,
      printerId: nil,
    )
    let response = try await client.stockLabelPrint(
      .init(
        path: .init(id: id),
        body: .json(request),
      ))
    switch response {
    case .ok(let ok): return try ok.body.json
    case .badRequest(let err):
      let body = try? err.body.json
      if body?.message.contains("client delivery") == true {
        return try await renderAndSendStockLabel(id: id, request: request)
      }
      throw APIError.server(status: 400, body: body)
    case .notFound(let err): throw APIError.server(status: 404, body: try? err.body.json)
    case .undocumented(let statusCode, _):
      throw APIError.server(status: statusCode, body: nil)
    }
  }

  private func renderAndSendStockLabel(id: String, request: PrintStockLabelRequest) async throws
    -> PrintStockLabelResponse
  {
    let response = try await client.stockLabelRender(
      .init(path: .init(id: id), body: .json(request)))
    let artifact: RenderLabelResponse
    switch response {
    case .ok(let ok):
      artifact = try ok.body.json
    case .badRequest(let err):
      throw APIError.server(status: 400, body: try? err.body.json)
    case .notFound(let err):
      throw APIError.server(status: 404, body: try? err.body.json)
    case .undocumented(let statusCode, _):
      throw APIError.server(status: statusCode, body: nil)
    }

    guard let payload = Data(base64Encoded: artifact.payload) else {
      throw APIError.decoding(
        DecodingError.dataCorrupted(
          .init(
            codingPath: [],
            debugDescription: "Label artifact payload is not valid base64"
          )))
    }
    try await labelPrinterClient.send(payload, to: artifact.address, port: Int(artifact.port))
    return PrintStockLabelResponse(
      batchId: artifact.batchId,
      batchUrl: artifact.batchUrl,
      copies: Int32(artifact.copies),
      printerId: artifact.printerId,
      status: .sent
    )
  }

  func listStockEvents(
    beforeCreatedAt: String? = nil,
    beforeID: String? = nil,
    limit: Int = 50,
  ) async throws -> StockEventListResponse {
    let response = try await client.stockListEvents(
      .init(
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
    let response = try await client.stockListBatchEvents(
      .init(
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
    let response = try await client.stockRestoreMany(
      .init(
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
    let response = try await client.remindersList(
      .init(
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
    let response = try await client.deviceRegister(
      .init(
        body: .json(
          .init(
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

  // MARK: - Recipes and automation

  func recipes() async throws -> [RecipeSummary] {
    let response: RecipeListResponse = try await rawJSON(
      "GET", "recipes", body: Optional<Int>.none, auth: true)
    return response.items
  }

  func getRecipe(id: String) async throws -> Recipe {
    try await rawJSON("GET", "recipes/\(id)", body: Optional<Int>.none, auth: true)
  }

  func preflightRecipe(_ recipe: Recipe, allowPartial: Bool) async throws
    -> RecipeExecutionPreflight
  {
    try await rawJSON(
      "POST",
      "recipes/executions/preflight",
      body: Self.executionRequest(recipe: recipe, allowPartial: allowPartial),
      auth: true)
  }

  func executeRecipe(_ recipe: Recipe, allowPartial: Bool) async throws -> RecipeExecutionResult {
    var request = Self.executionRequest(recipe: recipe, allowPartial: allowPartial)
    request.idempotencyKey = UUID().uuidString
    return try await rawJSON("POST", "recipes/executions", body: request, auth: true)
  }

  func aiStatus() async throws -> AiStatus {
    try await rawJSON("GET", "ai/status", body: Optional<Int>.none, auth: true)
  }

  func pantrySuggestions() async throws -> [PantrySuggestion] {
    let response: PantrySuggestionListResponse = try await rawJSON(
      "GET", "pantry/suggestions", body: Optional<Int>.none, auth: true)
    return response.items
  }

  func createPantrySuggestions(generateRecipeIdeas: Bool) async throws -> PantrySuggestionsResponse
  {
    try await rawJSON(
      "POST",
      "pantry/suggestions",
      body: Components.Schemas.CreatePantrySuggestionsRequest(
        generateRecipeIdeas: generateRecipeIdeas,
        maxAiSuggestions: 2,
        maxMissingRequired: 2),
      auth: true)
  }

  func mealPlans() async throws -> [MealPlanSummary] {
    let response: MealPlanListResponse = try await rawJSON(
      "GET", "meal-plans", body: Optional<Int>.none, auth: true)
    return response.items
  }

  func getMealPlan(id: String) async throws -> MealPlan {
    try await rawJSON("GET", "meal-plans/\(id)", body: Optional<Int>.none, auth: true)
  }

  func generateMealPlan(title: String?, dates: [String]) async throws -> MealPlan {
    try await rawJSON(
      "POST",
      "meal-plans/generate",
      body: MealPlanGenerateBody(
        title: title,
        dates: dates,
        slots: [
          MealPlanSlotBody(key: "breakfast", label: "Breakfast"),
          MealPlanSlotBody(key: "lunch", label: "Lunch"),
          MealPlanSlotBody(key: "dinner", label: "Dinner"),
        ]),
      auth: true,
      successStatus: 201)
  }

  func refreshMealPlan(id: String) async throws -> MealPlan {
    let response: MealPlanRefreshBody = try await rawJSON(
      "POST", "meal-plans/\(id)/refresh", body: Optional<Int>.none, auth: true)
    return response.plan
  }

  func executeMealPlanMeal(planID: String, mealID: String) async throws -> RecipeExecutionResult {
    try await rawJSON(
      "POST",
      "meal-plans/\(planID)/meals/\(mealID)/execute",
      body: Optional<Int>.none,
      auth: true)
  }

  func skipMealPlanMeal(planID: String, mealID: String) async throws -> MealPlan {
    try await rawJSON(
      "POST",
      "meal-plans/\(planID)/meals/\(mealID)/skip",
      body: Optional<Int>.none,
      auth: true)
  }

  func generateCartDraft() async throws -> ReplenishmentCreateCartDraftResponse {
    try await rawJSON(
      "POST",
      "replenishment/cart-drafts",
      body: Components.Schemas.ReplenishmentCreateCartDraftRequest(
        includeAiExplanation: true,
        submitTrusted: false,
        supplierId: "mock"),
      auth: true,
      successStatus: 201)
  }

  func getCartRun(id: String) async throws -> ReplenishmentCartRun {
    try await rawJSON("GET", "replenishment/cart-runs/\(id)", body: Optional<Int>.none, auth: true)
  }

  func getSupplierCartDraft(id: String) async throws -> SupplierCartDraft {
    try await rawJSON("GET", "suppliers/cart-drafts/\(id)", body: Optional<Int>.none, auth: true)
  }

  func submitSupplierCartDraft(id: String) async throws -> SupplierOrder {
    try await rawJSON(
      "POST",
      "suppliers/cart-drafts/\(id)/submit",
      body: Optional<Int>.none,
      auth: true,
      successStatus: 201)
  }

  func receiveSupplierOrder(id: String, productID: String, locationID: String) async throws
    -> SupplierOrder
  {
    try await rawJSON(
      "POST",
      "suppliers/orders/\(id)/receive",
      body: Components.Schemas.SupplierReceiveOrderRequest(
        lines: [
          Components.Schemas.SupplierReceiveLineRequest(
            expiresOn: nil,
            locationId: locationID,
            note: "received from iOS cart review",
            productId: productID,
            quantity: "1000",
            unit: "g")
        ]),
      auth: true)
  }

  private func rawJSON<RequestBody: Encodable, ResponseBody: Decodable>(
    _ method: String,
    _ path: String,
    body: RequestBody?,
    auth: Bool,
    successStatus: Int = 200
  ) async throws -> ResponseBody {
    let data = try await rawData(method, path, body: body, auth: auth, successStatus: successStatus)
    return try jsonDecoder.decode(ResponseBody.self, from: data)
  }

  private func rawUnit(_ method: String, _ path: String, auth: Bool) async throws {
    _ = try await rawData(method, path, body: Optional<Int>.none, auth: auth, successStatus: 204)
  }

  private func rawData<RequestBody: Encodable>(
    _ method: String,
    _ path: String,
    body: RequestBody?,
    auth: Bool,
    successStatus: Int
  ) async throws -> Data {
    let url =
      path.trimmingCharacters(in: CharacterSet(charactersIn: "/"))
      .split(separator: "/")
      .reduce(baseURL.appendingPathComponent("api").appendingPathComponent("v1")) {
        $0.appendingPathComponent(String($1))
      }
    var request = URLRequest(url: url)
    request.httpMethod = method
    request.setValue("application/json", forHTTPHeaderField: "Content-Type")
    if let body {
      request.httpBody = try jsonEncoder.encode(body)
    }
    if auth {
      guard let token = await tokenStore.accessToken else { throw APIError.unauthorized }
      request.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
    }
    let (data, response) = try await session.data(for: request)
    guard let http = response as? HTTPURLResponse else {
      throw APIError.server(status: 0, body: nil)
    }
    guard http.statusCode == successStatus else {
      let body = try? jsonDecoder.decode(APIErrorBody.self, from: data)
      throw APIError.server(status: http.statusCode, body: body)
    }
    return data
  }

  private static let deviceLabel: String? = {
    #if os(iOS)
      return "iOS"
    #else
      return nil
    #endif
  }()

  private static func passkeySummary(_ credential: Components.Schemas.PasskeyCredentialDto)
    -> PasskeyCredentialSummary
  {
    PasskeyCredentialSummary(
      id: credential.id,
      label: credential.label,
      createdAt: credential.createdAt,
      lastUsedAt: credential.lastUsedAt)
  }

  private static func openAPIValue(from data: Data) throws -> OpenAPIValueContainer {
    try JSONDecoder().decode(OpenAPIValueContainer.self, from: data)
  }

  private static func openAPIValueData(_ value: OpenAPIValueContainer) throws -> Data {
    try JSONEncoder().encode(value)
  }

  private static func executionRequest(recipe: Recipe, allowPartial: Bool)
    -> Components.Schemas.RecipeExecutionRequest
  {
    Components.Schemas.RecipeExecutionRequest(
      allowPartial: allowPartial,
      idempotencyKey: nil,
      ingredients: recipe.version.ingredients.compactMap { ingredient in
        guard let amount = ingredient.quantity.amount, let unit = ingredient.quantity.unit else {
          return nil
        }
        return Components.Schemas.RecipeExecutionIngredientRequest(
          displayName: ingredient.displayName,
          ingredientId: ingredient.ingredientId,
          lineId: ingredient.id ?? ingredient.displayName,
          locationId: nil,
          optional: ingredient.optional ?? false,
          preparation: ingredient.preparation,
          productId: ingredient.productId,
          quantity: amount,
          substitutionOf: nil,
          unit: unit)
      },
      outputs: [],
      recipeId: recipe.id,
      recipeName: recipe.name,
      recipeVersionId: recipe.version.id,
      servingScale: "1",
      useExpiringFirst: true)
  }
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
      let refreshURL =
        baseURL
        .appendingPathComponent("api")
        .appendingPathComponent("v1")
        .appendingPathComponent("auth")
        .appendingPathComponent("refresh")
      var req = URLRequest(url: refreshURL)
      req.httpMethod = "POST"
      req.setValue("application/json", forHTTPHeaderField: "Content-Type")
      req.httpBody = try JSONEncoder().encode(
        Components.Schemas.RefreshRequest(refreshToken: refreshToken))
      let (data, response) = try await session.data(for: req)
      guard
        let http = response as? HTTPURLResponse,
        (200..<300).contains(http.statusCode)
      else {
        throw APIError.unauthorized
      }
      let pair = try JSONDecoder().decode(TokenPair.self, from: data)
      await tokenStore.store(pair)
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
    "onboarding_create_household",
    "onboarding_join_invite",
    "onboarding_status",
  ]
}
