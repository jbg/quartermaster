import XCTest
@testable import Quartermaster

@MainActor
final class AppStateReminderTests: XCTestCase {
    func testInitialLoadQueuesUnpresentedRemindersOnceAndDismissAdvancesQueue() async {
        let api = FakeAPI(
            reminderResponses: [
                .success(reminderListResponse([reminder(id: "r1"), reminder(id: "r2")])),
                .success(reminderListResponse([reminder(id: "r1"), reminder(id: "r2")])),
            ]
        )
        let appState = makeAppState(api: api)
        appState.phase = .authenticated(me())

        await appState.loadReminderInbox(limit: 50)
        XCTAssertEqual(appState.reminders.map(\.id), ["r1", "r2"])
        XCTAssertEqual(appState.activeReminder?.id, "r1")
        let presentedIDs = await api.presentedReminderIDs()
        XCTAssertEqual(presentedIDs, ["r1", "r2"])

        await appState.refreshRemindersAfterUserAction(limit: 50)
        let refreshedPresentedIDs = await api.presentedReminderIDs()
        XCTAssertEqual(refreshedPresentedIDs, ["r1", "r2"])

        appState.dismissActiveReminder()
        XCTAssertEqual(appState.activeReminder?.id, "r2")
        XCTAssertEqual(appState.reminders.map(\.id), ["r1", "r2"])
    }

    func testReminderSnapshotsSortByExpiryFireTimeAndID() async {
        let laterExpiry = reminder(
            id: "cccccccc-cccc-cccc-cccc-cccccccccccc",
            expiresOn: "2026-04-26",
            householdFireLocalAt: "2026-04-25T09:00:00Z"
        )
        let laterFire = reminder(
            id: "bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb",
            expiresOn: "2026-04-24",
            householdFireLocalAt: "2026-04-24T10:00:00Z"
        )
        let first = reminder(
            id: "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa",
            expiresOn: "2026-04-24",
            householdFireLocalAt: "2026-04-24T09:00:00Z"
        )
        let missingExpiry = reminder(
            id: "dddddddd-dddd-dddd-dddd-dddddddddddd",
            expiresOn: nil,
            householdFireLocalAt: "2026-04-23T09:00:00Z"
        )
        let api = FakeAPI(
            reminderResponses: [
                .success(reminderListResponse([laterExpiry, laterFire, missingExpiry, first]))
            ]
        )
        let appState = makeAppState(api: api)
        appState.phase = .authenticated(me())

        await appState.loadReminderInbox(limit: 50)

        XCTAssertEqual(
            appState.reminders.map(\.id),
            [
                "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa",
                "bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb",
                "cccccccc-cccc-cccc-cccc-cccccccccccc",
                "dddddddd-dddd-dddd-dddd-dddddddddddd",
            ]
        )
    }

    func testInitialLoadShowsLoadingUntilFirstReminderFetchCompletes() async {
        let gate = AsyncGate()
        let api = FakeAPI(
            reminderResponses: [.success(reminderListResponse([reminder()]))],
            reminderGate: gate
        )
        let appState = makeAppState(api: api)
        appState.phase = .authenticated(me())

        let task = Task { await appState.loadReminderInbox(limit: 50) }
        await eventually { appState.isLoadingReminders }
        XCTAssertTrue(appState.isLoadingReminders)

        await gate.release()
        await task.value

        XCTAssertFalse(appState.isLoadingReminders)
        XCTAssertEqual(appState.reminders.count, 1)
    }

    func testUserInitiatedRefreshClearsPriorErrorAndSurfacesFreshFailure() async {
        let api = FakeAPI(
            reminderResponses: [.failure(APIError.server(status: 503, body: apiErrorBody(message: "Fresh failure")))]
        )
        let appState = makeAppState(api: api)
        appState.phase = .authenticated(me())
        appState.reminderInboxError = "Old error"

        await appState.refreshRemindersAfterUserAction(limit: 50)

        XCTAssertEqual(appState.reminderInboxError, "Fresh failure")
        XCTAssertFalse(appState.isLoadingReminders)
    }

    func testSilentRefreshDoesNotToggleVisibleLoadingState() async {
        let api = FakeAPI(reminderResponses: [.success(reminderListResponse([]))])
        let appState = makeAppState(api: api)
        appState.phase = .authenticated(me())
        appState.isLoadingReminders = true

        await appState.refreshRemindersSilently(limit: 50)

        XCTAssertTrue(appState.isLoadingReminders)
    }

    func testReminderRefreshRetriesAfterHouseholdScopedForbidden() async {
        let api = FakeAPI(
            meResponses: [.success(me())],
            reminderResponses: [
                .failure(APIError.server(status: 403, body: nil)),
                .success(reminderListResponse([reminder()])),
            ]
        )
        let appState = makeAppState(api: api)
        appState.phase = .authenticated(me())

        await appState.refreshRemindersAfterUserAction(limit: 50)

        XCTAssertEqual(appState.reminders.map(\.id), ["55555555-5555-5555-5555-555555555555"])
        let listReminderCalls = await api.listReminderCallCount()
        XCTAssertEqual(listReminderCalls, 2)
    }

    func testReminderRefreshFallsBackToNoHouseholdAfterForbiddenRecovery() async {
        let api = FakeAPI(
            meResponses: [.success(me(currentHouseholdJSON: nil, householdsJSON: []))],
            reminderResponses: [.failure(APIError.server(status: 403, body: nil))]
        )
        let appState = makeAppState(api: api)
        appState.phase = .authenticated(me())
        appState.reminders = [reminder()]
        appState.activeReminder = reminder()

        await appState.refreshRemindersAfterUserAction(limit: 50)

        XCTAssertTrue(appState.reminders.isEmpty)
        XCTAssertNil(appState.activeReminder)
        XCTAssertNil(appState.reminderInboxError)
    }

    func testUnauthorizedReminderRefreshClearsSession() async {
        let tokenStore = FakeTokenStore(accessToken: "access", refreshToken: "refresh")
        let api = FakeAPI(reminderResponses: [.failure(APIError.unauthorized)])
        let appState = makeAppState(tokenStore: tokenStore, api: api)
        appState.phase = .authenticated(me())

        await appState.loadReminderInbox(limit: 50)

        XCTAssertEqual(appState.phase, .unauthenticated)
        let accessToken = await tokenStore.currentAccessToken()
        XCTAssertNil(accessToken)
    }

    func testAcknowledgeReminderRestoresStateOnFailureAndClearsInFlight() async {
        let reminder = reminder()
        let api = FakeAPI(
            ackResponses: [.failure(APIError.server(status: 503, body: apiErrorBody(message: "Ack failed")))]
        )
        let appState = makeAppState(api: api)
        appState.phase = .authenticated(me())
        appState.reminders = [reminder]
        appState.activeReminder = reminder

        await appState.acknowledgeReminder(id: reminder.id)

        XCTAssertEqual(appState.reminders.map(\.id), [reminder.id])
        XCTAssertEqual(appState.activeReminder?.id, reminder.id)
        XCTAssertFalse(appState.isReminderActionInFlight(id: reminder.id))
        XCTAssertEqual(appState.lastError, "Ack failed")
    }

    func testOpenReminderSetsFallbackTargetAndRefreshesState() async {
        let first = reminder()
        let second = reminder(id: "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa")
        let api = FakeAPI(
            reminderResponses: [
                .success(reminderListResponse([first, second])),
                .success(reminderListResponse([second])),
            ],
            openResponses: [.success(())]
        )
        let appState = makeAppState(api: api)
        appState.phase = .authenticated(me())
        await appState.loadReminderInbox(limit: 50)

        appState.openActiveReminder()
        await eventually { appState.pendingInventoryTarget?.highlightBatchID == first.batchID }

        XCTAssertEqual(appState.pendingInventoryTarget?.productID, first.productID)
        let openedReminderIDs = await api.openedReminderIDs()
        XCTAssertEqual(openedReminderIDs, [first.id])
        XCTAssertNotEqual(appState.activeReminder?.id, first.id)
    }

    func testOpenReminderFailureStillKeepsFallbackInventoryTarget() async throws {
        let payload = try XCTUnwrap(
            ReminderPushPayload(
                userInfo: [
                    "reminder_id": "55555555-5555-5555-5555-555555555555",
                    "batch_id": "33333333-3333-3333-3333-333333333333",
                    "product_id": "44444444-4444-4444-4444-444444444444",
                    "location_id": "22222222-2222-2222-2222-222222222222",
                ]
            )
        )
        let api = FakeAPI(
            reminderResponses: [.success(reminderListResponse([]))],
            openResponses: [.failure(APIError.server(status: 502, body: apiErrorBody(message: "Open failed")))]
        )
        let appState = makeAppState(api: api)
        appState.phase = .authenticated(me())

        await appState.handleRemoteNotification(payload, opened: true)

        XCTAssertEqual(appState.pendingInventoryTarget?.productID, payload.productID)
        XCTAssertEqual(appState.pendingInventoryTarget?.highlightBatchID, payload.batchID)
    }

    func testLogoutClearsReminderState() async {
        let api = FakeAPI()
        let appState = makeAppState(api: api)
        appState.phase = .authenticated(me())
        appState.reminders = [reminder()]
        appState.activeReminder = reminder()
        appState.isLoadingReminders = true
        appState.reminderInboxError = "Boom"
        appState.pendingInventoryTarget = InventoryTarget(
            productID: "44444444-4444-4444-4444-444444444444",
            locationID: "22222222-2222-2222-2222-222222222222",
            highlightBatchID: "33333333-3333-3333-3333-333333333333"
        )

        await appState.logout()

        XCTAssertEqual(appState.phase, .unauthenticated)
        XCTAssertTrue(appState.reminders.isEmpty)
        XCTAssertNil(appState.activeReminder)
        XCTAssertFalse(appState.isLoadingReminders)
        XCTAssertNil(appState.reminderInboxError)
    }

    private func makeAppState(
        tokenStore: FakeTokenStore = FakeTokenStore(),
        api: FakeAPI
    ) -> AppState {
        AppState(
            tokenStore: tokenStore,
            api: api,
            notifications: AppStateNotifications(
                currentAuthorization: { .denied },
                requestAuthorization: { false },
                registerForRemoteNotifications: {}
            )
        )
    }

    private func eventually(
        timeoutNanoseconds: UInt64 = 1_000_000_000,
        condition: @escaping @MainActor () -> Bool
    ) async {
        let deadline = DispatchTime.now().uptimeNanoseconds + timeoutNanoseconds
        while DispatchTime.now().uptimeNanoseconds < deadline {
            if condition() {
                return
            }
            await Task.yield()
        }
        XCTFail("condition was not met before timeout")
    }
}

private actor AsyncGate {
    private var continuation: CheckedContinuation<Void, Never>?
    private var released = false

    func wait() async {
        if released { return }
        await withCheckedContinuation { continuation in
            self.continuation = continuation
        }
    }

    func release() {
        released = true
        continuation?.resume()
        continuation = nil
    }
}

private actor FakeTokenStore: AppStateTokenStore {
    var accessToken: String?
    var refreshToken: String?

    init(accessToken: String? = "access", refreshToken: String? = "refresh") {
        self.accessToken = accessToken
        self.refreshToken = refreshToken
    }

    func store(_ pair: TokenPair) {
        accessToken = pair.accessToken
        refreshToken = pair.refreshToken
    }

    func clear() {
        accessToken = nil
        refreshToken = nil
    }

    func currentAccessToken() -> String? {
        accessToken
    }
}

private actor FakeAPI: AppStateAPI {
    private var meResponses: [Result<Me, Error>]
    private var reminderResponses: [Result<ReminderListResponse, Error>]
    private var ackResponses: [Result<Void, Error>]
    private var openResponses: [Result<Void, Error>]
    private let reminderGate: AsyncGate?
    private var presentedIDs: [String] = []
    private var openedIDs: [String] = []
    private var listReminderCalls = 0

    init(
        meResponses: [Result<Me, Error>] = [.success(defaultMe())],
        reminderResponses: [Result<ReminderListResponse, Error>] = [.success(reminderListResponse([]))],
        ackResponses: [Result<Void, Error>] = [.success(())],
        openResponses: [Result<Void, Error>] = [.success(())],
        reminderGate: AsyncGate? = nil
    ) {
        self.meResponses = meResponses
        self.reminderResponses = reminderResponses
        self.ackResponses = ackResponses
        self.openResponses = openResponses
        self.reminderGate = reminderGate
    }

    func presentedReminderIDs() -> [String] { presentedIDs }
    func openedReminderIDs() -> [String] { openedIDs }
    func listReminderCallCount() -> Int { listReminderCalls }

    func register(username: String, password: String, email: String?, inviteCode: String?) async throws -> TokenPair { tokenPair() }
    func login(username: String, password: String) async throws -> TokenPair { tokenPair() }
    func logout() async throws {}
    func me() async throws -> Me { try next(&meResponses) }
    func switchHousehold(householdID: String) async throws -> Me { try await me() }
    func createHousehold(name: String, timezone: String) async throws -> Me { try await me() }
    func currentHousehold() async throws -> HouseholdDetail { fatalError("unused") }
    func updateCurrentHousehold(name: String, timezone: String) async throws -> HouseholdDetail { fatalError("unused") }
    func householdMembers() async throws -> [Member] { [] }
    func removeHouseholdMember(userID: String) async throws {}
    func householdInvites() async throws -> [Invite] { [] }
    func createInvite(expiresAt: String, maxUses: Int, role: MembershipRole) async throws -> Invite { fatalError("unused") }
    func revokeInvite(id: String) async throws {}
    func redeemInvite(code: String) async throws {}
    func locations() async throws -> [Location] { [] }
    func createLocation(name: String, kind: String, sortOrder: Int?) async throws -> Location { fatalError("unused") }
    func updateLocation(id: String, name: String, kind: String, sortOrder: Int) async throws -> Location { fatalError("unused") }
    func deleteLocation(id: String) async throws {}
    func units() async throws -> [Quartermaster.Unit] { [] }
    func searchProducts(query: String, limit: Int, includeDeleted: Bool) async throws -> [Product] { [] }
    func lookupBarcode(_ barcode: String) async throws -> BarcodeLookupResponse { fatalError("unused") }
    func createProduct(_ request: CreateProductRequest) async throws -> Product { fatalError("unused") }
    func getProduct(id: String) async throws -> Product { fatalError("unused") }
    func updateProduct(id: String, request: UpdateProductRequest) async throws -> Product { fatalError("unused") }
    func deleteProduct(id: String) async throws {}
    func refreshProduct(id: String) async throws -> Product { fatalError("unused") }
    func restoreProduct(id: String) async throws -> Product { fatalError("unused") }
    func listStock(locationID: String?, productID: String?, expiringBefore: String?, includeDepleted: Bool) async throws -> [StockBatch] { [] }
    func getStock(id: String) async throws -> StockBatch { fatalError("unused") }
    func createStock(_ request: CreateStockRequest) async throws -> StockBatch { fatalError("unused") }
    func updateStock(id: String, request: UpdateStockRequest) async throws -> StockBatch { fatalError("unused") }
    func deleteStock(id: String) async throws {}
    func consumeStock(_ request: ConsumeRequest) async throws -> ConsumeResponse { fatalError("unused") }
    func listStockEvents(beforeCreatedAt: String?, beforeID: String?, limit: Int) async throws -> StockEventListResponse { fatalError("unused") }
    func listBatchEvents(id: String, beforeCreatedAt: String?, beforeID: String?, limit: Int) async throws -> StockEventListResponse { fatalError("unused") }
    func restoreStock(id: String) async throws -> StockBatch { fatalError("unused") }
    func restoreManyStock(ids: [String]) async throws -> RestoreManyResponse { fatalError("unused") }

    func listReminders(afterFireAt: String?, afterID: String?, limit: Int) async throws -> ReminderListResponse {
        listReminderCalls += 1
        if let reminderGate {
            await reminderGate.wait()
        }
        return try next(&reminderResponses)
    }

    func presentReminder(id: String) async throws {
        presentedIDs.append(id)
    }

    func ackReminder(id: String) async throws {
        _ = id
        try next(&ackResponses)
    }

    func openReminder(id: String) async throws {
        openedIDs.append(id)
        try next(&openResponses)
    }

    func registerDevice(deviceID: String, pushToken: String?, pushAuthorization: PushAuthorizationStatus, appVersion: String?) async throws {}

    private func next<T>(_ responses: inout [Result<T, Error>]) throws -> T {
        let result: Result<T, Error>
        if responses.isEmpty {
            result = .failure(APIError.unknown)
        } else {
            result = responses.removeFirst()
        }
        return try result.get()
    }
}

private func decode<T: Decodable>(_ type: T.Type = T.self, from json: String) -> T {
    try! JSONDecoder().decode(T.self, from: Data(json.utf8))
}

private func tokenPair() -> TokenPair {
    decode(from: """
    {
      "access_token": "access",
      "refresh_token": "refresh",
      "token_type": "bearer",
      "expires_in": 1800
    }
    """)
}

private func apiErrorBody(message: String) -> APIErrorBody {
    decode(from: """
    {
      "code": "temporary_failure",
      "message": "\(message)"
    }
    """)
}

private func me(
    currentHouseholdJSON: String? = householdJSON(),
    householdsJSON: [String] = [householdJSON()]
) -> Me {
    let currentHousehold = currentHouseholdJSON ?? "null"
    let households = householdsJSON.joined(separator: ",")
    return decode(from: """
    {
      "user": {
        "id": "11111111-1111-1111-1111-111111111111",
        "username": "alice",
        "email": "alice@example.com"
      },
      "current_household": \(currentHousehold),
      "households": [\(households)],
      "public_base_url": "https://quartermaster.example.com"
    }
    """)
}

private func defaultMe() -> Me {
    me()
}

private func householdJSON() -> String {
    """
    {
      "id": "66666666-6666-6666-6666-666666666666",
      "name": "Home",
      "timezone": "UTC",
      "role": "admin",
      "joined_at": "2026-04-22T12:00:00Z"
    }
    """
}

private func reminder(
    id: String = "55555555-5555-5555-5555-555555555555",
    expiresOn: String? = nil,
    householdFireLocalAt: String = "2026-04-23T09:00:00Z"
) -> Reminder {
    decode(from: """
    {
      "id": "\(id)",
      "kind": "expiry",
      "title": "Use flour soon",
      "body": "Pantry flour expires tomorrow.",
      "fire_at": "2026-04-23T09:00:00Z",
      "household_timezone": "UTC",
      "household_fire_local_at": "\(householdFireLocalAt)",
      "batch_id": "33333333-3333-3333-3333-333333333333",
      "product_id": "44444444-4444-4444-4444-444444444444",
      "location_id": "22222222-2222-2222-2222-222222222222",
      "expires_on": \(jsonString(expiresOn)),
      "presented_on_device_at": null,
      "opened_on_device_at": null
    }
    """)
}

private func reminderListResponse(_ items: [Reminder]) -> ReminderListResponse {
    let encodedItems = items.map { item in
        """
        {
          "id": "\(item.id)",
          "kind": "expiry",
          "title": "\(item.title)",
          "body": "\(item.body)",
          "fire_at": "\(item.fireAt)",
          "household_timezone": "\(item.householdTimezone)",
          "household_fire_local_at": "\(item.householdFireLocalAt)",
          "batch_id": "\(item.batchID)",
          "product_id": "\(item.productID)",
          "location_id": "\(item.locationID)",
          "presented_on_device_at": \(jsonString(item.presentedOnDeviceAt)),
          "opened_on_device_at": \(jsonString(item.openedOnDeviceAt)),
          "expires_on": \(jsonString(item.expiresOn))
        }
        """
    }.joined(separator: ",")

    return decode(from: """
    {
      "items": [\(encodedItems)],
      "next_after_fire_at": null,
      "next_after_id": null
    }
    """)
}

private func jsonString(_ value: String?) -> String {
    guard let value else { return "null" }
    return "\"\(value)\""
}
