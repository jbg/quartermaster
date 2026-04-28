import Foundation
import UIKit

#if DEBUG
  enum QuartermasterUITestLaunchArgument {
    static let depletedInventory = "--quartermaster-ui-test-depleted-inventory"
  }

  extension AppState {
    static func uiTestFixtureIfRequested(
      arguments: [String] = ProcessInfo.processInfo.arguments
    ) -> AppState? {
      guard arguments.contains(QuartermasterUITestLaunchArgument.depletedInventory) else {
        return nil
      }
      return AppState(
        tokenStore: UITestTokenStore(),
        api: UITestFixtureAPI(),
        notifications: AppStateNotifications(
          currentAuthorization: { .denied },
          requestAuthorization: { false },
          registerForRemoteNotifications: {}
        )
      )
    }
  }

  private actor UITestTokenStore: AppStateTokenStore {
    var accessToken: String? = "ui-test-access"
    var refreshToken: String? = "ui-test-refresh"

    func store(_ pair: TokenPair) {
      accessToken = pair.accessToken
      refreshToken = pair.refreshToken
    }

    func clear() {
      accessToken = nil
      refreshToken = nil
    }
  }

  private actor UITestFixtureAPI: AppStateAPI {
    private let productID = "11111111-1111-1111-1111-111111111111"
    private let locationID = "22222222-2222-2222-2222-222222222222"
    private let activeBatchID = "33333333-3333-3333-3333-333333333333"
    private let depletedBatchID = "44444444-4444-4444-4444-444444444444"

    func register(username: String, password: String, inviteCode: String?)
      async throws -> TokenPair
    { try decodeFixture(from: tokenPairJSON) }
    func onboardingStatus() async throws -> OnboardingStatus { throw APIError.unknown }
    func createOnboardingHousehold(
      username: String,
      password: String,
      householdName: String,
      timezone: String
    ) async throws -> TokenPair { try decodeFixture(from: tokenPairJSON) }
    func joinOnboardingInvite(username: String, password: String, inviteCode: String)
      async throws -> TokenPair
    { try decodeFixture(from: tokenPairJSON) }

    func login(username: String, password: String) async throws -> TokenPair {
      try decodeFixture(from: tokenPairJSON)
    }

    func requestEmailVerification(email: String) async throws -> RequestEmailVerificationResponse {
      RequestEmailVerificationResponse(
        expiresAt: "2026-04-28T12:30:00.000Z",
        pendingEmail: email
      )
    }
    func confirmEmailVerification(code: String) async throws -> Me { try await me() }
    func clearRecoveryEmail() async throws -> Me { try await me() }

    func logout() async throws {}
    func me() async throws -> Me { try decodeFixture(from: meJSON) }
    func switchHousehold(householdID: String) async throws -> Me { try await me() }
    func createHousehold(name: String, timezone: String) async throws -> Me { try await me() }
    func currentHousehold() async throws -> HouseholdDetail { throw APIError.unknown }
    func updateCurrentHousehold(name: String, timezone: String) async throws -> HouseholdDetail {
      throw APIError.unknown
    }
    func householdMembers() async throws -> [Member] { [] }
    func removeHouseholdMember(userID: String) async throws {}
    func householdInvites() async throws -> [Invite] { [] }
    func createInvite(expiresAt: String, maxUses: Int, role: MembershipRole) async throws
      -> Invite
    {
      throw APIError.unknown
    }
    func revokeInvite(id: String) async throws {}
    func redeemInvite(code: String) async throws {}
    func locations() async throws -> [Location] { try decodeFixture(from: "[\(locationJSON)]") }
    func createLocation(name: String, kind: String, sortOrder: Int?) async throws -> Location {
      throw APIError.unknown
    }
    func updateLocation(id: String, name: String, kind: String, sortOrder: Int) async throws
      -> Location
    { throw APIError.unknown }
    func deleteLocation(id: String) async throws {}
    func units() async throws -> [Unit] { try decodeFixture(from: unitsJSON) }
    func searchProducts(query: String, limit: Int, includeDeleted: Bool) async throws -> [Product] {
      []
    }
    func lookupBarcode(_ barcode: String) async throws -> BarcodeLookupResponse {
      throw APIError.unknown
    }
    func createProduct(_ request: CreateProductRequest) async throws -> Product {
      throw APIError.unknown
    }
    func getProduct(id: String) async throws -> Product {
      try decodeFixture(from: productJSON)
    }
    func updateProduct(id: String, request: UpdateProductRequest) async throws -> Product {
      throw APIError.unknown
    }
    func deleteProduct(id: String) async throws {}
    func refreshProduct(id: String) async throws -> Product { throw APIError.unknown }
    func restoreProduct(id: String) async throws -> Product { throw APIError.unknown }

    func listStock(
      locationID requestedLocationID: String?,
      productID requestedProductID: String?,
      expiringBefore: String?,
      includeDepleted: Bool
    ) async throws -> [StockBatch] {
      let batches: [StockBatch] = try decodeFixture(
        from: "[\(activeBatchJSON),\(depletedBatchJSON)]"
      )
      return batches.filter { batch in
        if let requestedLocationID, batch.locationID != requestedLocationID { return false }
        if let requestedProductID, batch.product.id != requestedProductID { return false }
        if !includeDepleted, batch.depletedAt != nil { return false }
        return true
      }
    }

    func getStock(id: String) async throws -> StockBatch {
      let batches = try await listStock(
        locationID: nil,
        productID: nil,
        expiringBefore: nil,
        includeDepleted: true
      )
      guard let batch = batches.first(where: { $0.id == id }) else { throw APIError.unknown }
      return batch
    }

    func createStock(_ request: CreateStockRequest) async throws -> StockBatch {
      throw APIError.unknown
    }
    func updateStock(id: String, request: UpdateStockRequest) async throws -> StockBatch {
      try await getStock(id: id)
    }
    func deleteStock(id: String) async throws {}
    func consumeStock(_ request: ConsumeRequest) async throws -> ConsumeResponse {
      throw APIError.unknown
    }
    func listStockEvents(beforeCreatedAt: String?, beforeID: String?, limit: Int) async throws
      -> StockEventListResponse
    { try decodeFixture(from: stockEventListJSON) }
    func listBatchEvents(id: String, beforeCreatedAt: String?, beforeID: String?, limit: Int)
      async throws -> StockEventListResponse
    { try decodeFixture(from: stockEventListJSON) }
    func restoreStock(id: String) async throws -> StockBatch { try await getStock(id: id) }
    func restoreManyStock(ids: [String]) async throws -> RestoreManyResponse {
      try decodeFixture(from: #"{"restored":[]}"#)
    }
    func listReminders(afterFireAt: String?, afterID: String?, limit: Int) async throws
      -> ReminderListResponse
    { try decodeFixture(from: #"{"items":[],"next_after_fire_at":null,"next_after_id":null}"#) }
    func presentReminder(id: String) async throws {}
    func ackReminder(id: String) async throws {}
    func openReminder(id: String) async throws {}
    func registerDevice(
      deviceID: String,
      pushToken: String?,
      pushAuthorization: PushAuthorizationStatus,
      appVersion: String?
    ) async throws {}

    private var tokenPairJSON: String {
      """
      {
        "access_token": "ui-test-access",
        "refresh_token": "ui-test-refresh",
        "token_type": "bearer",
        "expires_in": 1800
      }
      """
    }

    private var meJSON: String {
      """
      {
        "user": {
          "id": "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa",
          "username": "ui-smoke",
          "email": null,
          "email_verified_at": null,
          "pending_email": null,
          "pending_email_verification_expires_at": null
        },
        "current_household": {
          "id": "bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb",
          "name": "Smoke Household",
          "timezone": "UTC",
          "role": "admin",
          "joined_at": "2026-04-22T12:00:00Z"
        },
        "households": [{
          "id": "bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb",
          "name": "Smoke Household",
          "timezone": "UTC",
          "role": "admin",
          "joined_at": "2026-04-22T12:00:00Z"
        }],
        "public_base_url": "https://quartermaster.example.com"
      }
      """
    }

    private var locationJSON: String {
      """
      {
        "id": "\(locationID)",
        "name": "Smoke Pantry",
        "kind": "pantry",
        "sort_order": 0
      }
      """
    }

    private var unitsJSON: String {
      """
      [
        {"code": "g", "family": "mass", "to_base_milli": 1000},
        {"code": "kg", "family": "mass", "to_base_milli": 1000000}
      ]
      """
    }

    private var productJSON: String {
      """
      {
        "id": "\(productID)",
        "name": "Smoke Oats",
        "brand": "Fixture",
        "barcode": null,
        "image_url": null,
        "family": "mass",
        "preferred_unit": "g",
        "source": "manual",
        "deleted_at": null
      }
      """
    }

    private var activeBatchJSON: String {
      """
      {
        "id": "\(activeBatchID)",
        "product": \(productJSON),
        "location_id": "\(locationID)",
        "location_name": "Smoke Pantry",
        "initial_quantity": "500",
        "quantity": "500",
        "unit": "g",
        "created_at": "2026-04-22T10:00:00Z",
        "expires_on": "2026-05-01",
        "opened_on": null,
        "note": "Active smoke batch",
        "depleted_at": null
      }
      """
    }

    private var depletedBatchJSON: String {
      """
      {
        "id": "\(depletedBatchID)",
        "product": \(productJSON),
        "location_id": "\(locationID)",
        "location_name": "Smoke Pantry",
        "initial_quantity": "250",
        "quantity": "0",
        "unit": "g",
        "created_at": "2026-04-20T10:00:00Z",
        "expires_on": "2026-04-25",
        "opened_on": null,
        "note": "Depleted smoke batch",
        "depleted_at": "2026-04-24T10:00:00Z"
      }
      """
    }

    private var stockEventListJSON: String {
      """
      {
        "items": [{
          "id": "55555555-5555-5555-5555-555555555555",
          "event_type": "consume",
          "quantity_delta": "-250",
          "unit": "g",
          "created_at": "2026-04-24T10:00:00Z",
          "batch_id": "\(depletedBatchID)",
          "product": \(productJSON),
          "batch_expires_on": "2026-04-25",
          "consume_request_id": "66666666-6666-6666-6666-666666666666",
          "created_by_username": "ui-smoke",
          "note": null
        }],
        "next_before": null,
        "next_before_id": null
      }
      """
    }
  }

  private func decodeFixture<T: Decodable>(from json: String) throws -> T {
    try JSONDecoder().decode(T.self, from: Data(json.utf8))
  }
#endif
