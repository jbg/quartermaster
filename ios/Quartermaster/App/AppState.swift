import Foundation
import Observation
import UIKit
import UserNotifications

protocol AppStateTokenStore: Actor {
  var accessToken: String? { get async }
  var refreshToken: String? { get async }
  func store(_ pair: TokenPair)
  func clear()
}

protocol AppStateAPI: Actor {
  func register(username: String, password: String, email: String?, inviteCode: String?)
    async throws -> TokenPair
  func login(username: String, password: String) async throws -> TokenPair
  func logout() async throws
  func me() async throws -> Me
  func switchHousehold(householdID: String) async throws -> Me
  func createHousehold(name: String, timezone: String) async throws -> Me
  func currentHousehold() async throws -> HouseholdDetail
  func updateCurrentHousehold(name: String, timezone: String) async throws -> HouseholdDetail
  func householdMembers() async throws -> [Member]
  func removeHouseholdMember(userID: String) async throws
  func householdInvites() async throws -> [Invite]
  func createInvite(expiresAt: String, maxUses: Int, role: MembershipRole) async throws -> Invite
  func revokeInvite(id: String) async throws
  func redeemInvite(code: String) async throws
  func locations() async throws -> [Location]
  func createLocation(name: String, kind: String, sortOrder: Int?) async throws -> Location
  func updateLocation(id: String, name: String, kind: String, sortOrder: Int) async throws
    -> Location
  func deleteLocation(id: String) async throws
  func units() async throws -> [Unit]
  func searchProducts(query: String, limit: Int, includeDeleted: Bool) async throws -> [Product]
  func lookupBarcode(_ barcode: String) async throws -> BarcodeLookupResponse
  func createProduct(_ request: CreateProductRequest) async throws -> Product
  func getProduct(id: String) async throws -> Product
  func updateProduct(id: String, request: UpdateProductRequest) async throws -> Product
  func deleteProduct(id: String) async throws
  func refreshProduct(id: String) async throws -> Product
  func restoreProduct(id: String) async throws -> Product
  func listStock(
    locationID: String?, productID: String?, expiringBefore: String?, includeDepleted: Bool
  ) async throws -> [StockBatch]
  func getStock(id: String) async throws -> StockBatch
  func createStock(_ request: CreateStockRequest) async throws -> StockBatch
  func updateStock(id: String, request: UpdateStockRequest) async throws -> StockBatch
  func deleteStock(id: String) async throws
  func consumeStock(_ request: ConsumeRequest) async throws -> ConsumeResponse
  func listStockEvents(beforeCreatedAt: String?, beforeID: String?, limit: Int) async throws
    -> StockEventListResponse
  func listBatchEvents(id: String, beforeCreatedAt: String?, beforeID: String?, limit: Int)
    async throws -> StockEventListResponse
  func restoreStock(id: String) async throws -> StockBatch
  func restoreManyStock(ids: [String]) async throws -> RestoreManyResponse
  func listReminders(afterFireAt: String?, afterID: String?, limit: Int) async throws
    -> ReminderListResponse
  func presentReminder(id: String) async throws
  func ackReminder(id: String) async throws
  func openReminder(id: String) async throws
  func registerDevice(
    deviceID: String,
    pushToken: String?,
    pushAuthorization: PushAuthorizationStatus,
    appVersion: String?
  ) async throws
}

extension AppStateAPI {
  func searchProducts(query: String) async throws -> [Product] {
    try await searchProducts(query: query, limit: 20, includeDeleted: false)
  }

  func searchProducts(query: String, includeDeleted: Bool) async throws -> [Product] {
    try await searchProducts(query: query, limit: 20, includeDeleted: includeDeleted)
  }

  func listStock() async throws -> [StockBatch] {
    try await listStock(
      locationID: nil, productID: nil, expiringBefore: nil, includeDepleted: false)
  }

  func listStock(includeDepleted: Bool) async throws -> [StockBatch] {
    try await listStock(
      locationID: nil, productID: nil, expiringBefore: nil, includeDepleted: includeDepleted)
  }

  func listStock(locationID: String?, productID: String?) async throws -> [StockBatch] {
    try await listStock(
      locationID: locationID, productID: productID, expiringBefore: nil, includeDepleted: false)
  }

  func listStock(locationID: String?, productID: String?, includeDepleted: Bool) async throws
    -> [StockBatch]
  {
    try await listStock(
      locationID: locationID,
      productID: productID,
      expiringBefore: nil,
      includeDepleted: includeDepleted,
    )
  }

  func createLocation(name: String, kind: String) async throws -> Location {
    try await createLocation(name: name, kind: kind, sortOrder: nil)
  }

  func listStockEvents(limit: Int = 50) async throws -> StockEventListResponse {
    try await listStockEvents(beforeCreatedAt: nil, beforeID: nil, limit: limit)
  }

  func listBatchEvents(id: String, limit: Int = 50) async throws -> StockEventListResponse {
    try await listBatchEvents(id: id, beforeCreatedAt: nil, beforeID: nil, limit: limit)
  }

  func listReminders(limit: Int = 50) async throws -> ReminderListResponse {
    try await listReminders(afterFireAt: nil, afterID: nil, limit: limit)
  }

}

struct AppStateNotifications {
  var currentAuthorization: @MainActor @Sendable () async -> PushAuthorizationStatus
  var requestAuthorization: @MainActor @Sendable () async throws -> Bool
  var registerForRemoteNotifications: @MainActor @Sendable () -> Void

  static let live = Self(
    currentAuthorization: {
      let settings = await UNUserNotificationCenter.current().notificationSettings()
      return AppState.mapAuthorization(settings.authorizationStatus)
    },
    requestAuthorization: {
      try await UNUserNotificationCenter.current()
        .requestAuthorization(options: [.badge, .sound, .alert])
    },
    registerForRemoteNotifications: {
      UIApplication.shared.registerForRemoteNotifications()
    }
  )
}

@Observable
@MainActor
final class AppState {
  enum ReminderSyncMode: Equatable {
    case initialLoad
    case userInitiated
    case silent
  }

  enum HouseholdScopedForbiddenResolution: Equatable {
    case retry
    case fallbackToNoHousehold
    case failed(String)
  }

  enum Phase: Equatable {
    case launching
    case launchFailed(String)
    case unauthenticated
    case authenticated(Me)
  }

  var phase: Phase = .launching
  var serverURL: URL = ServerConfig.defaultURL
  var lastError: String?
  var units: [Unit] = []
  var reminders: [Reminder] = []
  var activeReminder: Reminder?
  var isLoadingReminders = false
  var reminderInboxError: String?
  /// Deep-link target set by history → "Open in Inventory". MainTabView
  /// observes this to switch tabs; InventoryView observes it to present
  /// the batches sheet for the named product+location, then clears it.
  var pendingInventoryTarget: InventoryTarget?
  var pendingInviteContext: InviteContext?

  private let tokenStore: any AppStateTokenStore
  private let apiFactory: ((URL) -> any AppStateAPI)?
  private let notifications: AppStateNotifications
  private(set) var api: any AppStateAPI
  private var queuedReminders: [Reminder] = []
  private var reminderActionInFlightIDs = Set<String>()
  private var pushToken: String?
  private var pushAuthorization: PushAuthorizationStatus = .notDetermined
  private var isSyncingReminders = false
  private var hasLoadedReminderInbox = false

  init() {
    let tokenStore = TokenStore()
    self.tokenStore = tokenStore
    self.apiFactory = { baseURL in
      APIClient(baseURL: baseURL, tokenStore: tokenStore)
    }
    self.notifications = .live
    self.api = APIClient(baseURL: ServerConfig.defaultURL, tokenStore: tokenStore)
  }

  init(
    tokenStore: any AppStateTokenStore,
    api: any AppStateAPI,
    notifications: AppStateNotifications
  ) {
    self.tokenStore = tokenStore
    self.api = api
    self.apiFactory = nil
    self.notifications = notifications
  }

  func bootstrap() async {
    if await tokenStore.accessToken != nil {
      await refreshMe()
    } else {
      phase = .unauthenticated
    }
  }

  func refreshMe() async {
    do {
      let me = try await api.me()
      applyAuthenticated(me)
    } catch {
      let message = userMessage(for: error)
      if let apiError = error as? APIError, case .unauthorized = apiError {
        await tokenStore.clear()
        phase = .unauthenticated
        return
      }
      lastError = message
      if case .launching = phase {
        phase = .launchFailed(message)
      }
    }
  }

  func applyAuthenticated(_ me: Me) {
    phase = .authenticated(me)
    Task {
      await loadUnits()
      await refreshNotificationAuthorization()
      await requestNotificationAuthorizationIfNeeded()
      await registerCurrentDevice()
      await syncDueReminders(mode: .initialLoad)
    }
    lastError = nil
  }

  var me: Me? {
    if case .authenticated(let me) = phase {
      return me
    }
    return nil
  }

  func register(username: String, password: String, email: String?, inviteCode: String? = nil) async
  {
    lastError = nil
    do {
      let pair = try await api.register(
        username: username,
        password: password,
        email: email,
        inviteCode: inviteCode,
      )
      await tokenStore.store(pair)
      await refreshMe()
    } catch {
      lastError = userMessage(for: error)
    }
  }

  func login(username: String, password: String) async {
    lastError = nil
    do {
      let pair = try await api.login(username: username, password: password)
      await tokenStore.store(pair)
      await refreshMe()
    } catch {
      lastError = userMessage(for: error)
    }
  }

  func logout() async {
    _ = try? await api.logout()
    await tokenStore.clear()
    units = []
    clearReminderState(clearLoadingState: true)
    phase = .unauthenticated
  }

  func updateServerURL(_ url: URL) {
    serverURL = url
    if let apiFactory {
      api = apiFactory(url)
    }
  }

  func takePendingInviteContext() -> InviteContext? {
    defer { pendingInviteContext = nil }
    return pendingInviteContext
  }

  func handleIncomingURL(_ url: URL) {
    handleIncomingJoinURL(url)
  }

  func handleIncomingUserActivity(_ userActivity: NSUserActivity) {
    guard let url = userActivity.webpageURL else { return }
    handleIncomingJoinURL(url)
  }

  private func handleIncomingJoinURL(_ url: URL) {
    guard let components = URLComponents(url: url, resolvingAgainstBaseURL: false) else {
      return
    }

    let isJoinLink =
      (components.scheme == "quartermaster" && components.host == "join")
      || components.path == "/join"
    guard isJoinLink else { return }

    let items = Dictionary(
      uniqueKeysWithValues: (components.queryItems ?? []).map { ($0.name, $0.value ?? "") }
    )
    let inviteCode = items["invite"]?.trimmingCharacters(in: .whitespacesAndNewlines)
    let incomingServer = items["server"].flatMap { raw -> URL? in
      guard let url = URL(string: raw) else { return nil }
      guard ["http", "https"].contains(url.scheme?.lowercased() ?? "") else { return nil }
      return url
    }

    if case .unauthenticated = phase, let incomingServer {
      updateServerURL(incomingServer)
    }

    pendingInviteContext = InviteContext(
      inviteCode: inviteCode?.isEmpty == false ? inviteCode : nil,
      serverURL: incomingServer
    )
  }

  func unitsFor(family: ProductFamily) -> [Unit] {
    units.filter { $0.family == family }
  }

  func switchHousehold(to householdID: String) async throws -> Me {
    let updatedMe = try await api.switchHousehold(householdID: householdID)
    applyAuthenticated(updatedMe)
    return updatedMe
  }

  var householdTimeZoneID: String? {
    me?.currentHouseholdSummary?.timezone
  }

  var householdTimeZone: TimeZone? {
    householdTimeZoneID.flatMap(TimeZone.init(identifier:))
  }

  var deviceTimeZone: TimeZone {
    .autoupdatingCurrent
  }

  var timezonesDiffer: Bool {
    guard let householdTimeZoneID else { return false }
    return householdTimeZoneID != deviceTimeZone.identifier
  }

  func loadReminderInbox(limit: Int = 50) async {
    await syncDueReminders(limit: limit, mode: .initialLoad)
  }

  func refreshRemindersAfterUserAction(limit: Int = 50) async {
    await syncDueReminders(limit: limit, mode: .userInitiated)
  }

  func refreshRemindersSilently(limit: Int = 20) async {
    await syncDueReminders(limit: limit, mode: .silent)
  }

  func refreshRemindersAfterInventoryMutation(limit: Int = 50) async {
    await syncDueReminders(limit: limit, mode: .silent)
  }

  func syncDueReminders(limit: Int = 20, mode: ReminderSyncMode = .silent) async {
    guard !isSyncingReminders else { return }
    guard let me, me.currentHouseholdSummary != nil else {
      clearReminderState(clearLoadingState: true)
      return
    }

    if mode != .silent && !hasLoadedReminderInbox {
      isLoadingReminders = true
      reminderInboxError = nil
    }

    isSyncingReminders = true
    defer {
      isSyncingReminders = false
      if mode != .silent {
        isLoadingReminders = false
      }
    }

    var requestMode = mode
    while true {
      do {
        let response = try await api.listReminders(limit: limit)
        hasLoadedReminderInbox = true
        reminderInboxError = nil
        let sortedItems = Self.sortedReminders(response.items)
        applyReminderSnapshot(sortedItems)
        let existingIDs = Set(queuedReminders.map(\.id) + (activeReminder.map { [$0.id] } ?? []))
        for reminder in sortedItems
        where reminder.presentedOnDeviceAt == nil
          && !existingIDs.contains(reminder.id)
          && !reminderActionInFlightIDs.contains(reminder.id)
        {
          try? await api.presentReminder(id: reminder.id)
          queuedReminders.append(reminder)
        }
        presentNextReminderIfNeeded()
        return
      } catch let apiError as APIError {
        if case .unauthorized = apiError {
          await tokenStore.clear()
          units = []
          clearReminderState(clearLoadingState: true)
          phase = .unauthenticated
          return
        }
        if case .server(status: 403, _) = apiError {
          switch await resolveHouseholdScopedForbidden() {
          case .retry:
            requestMode = requestMode == .silent ? .silent : .userInitiated
            continue
          case .fallbackToNoHousehold:
            clearReminderState(clearLoadingState: true)
          case .failed(let message):
            if requestMode != .silent {
              reminderInboxError = message
            }
          }
          return
        }
        if requestMode != .silent {
          reminderInboxError = apiError.userFacingMessage
        }
        return
      } catch {
        if requestMode != .silent {
          reminderInboxError = error.localizedDescription
        }
        return
      }
    }
  }

  func dismissActiveReminder() {
    activeReminder = nil
    presentNextReminderIfNeeded()
  }

  func acknowledgeReminder(id: String) async {
    guard !reminderActionInFlightIDs.contains(id) else { return }
    let previousReminders = reminders
    let previousQueued = queuedReminders
    let previousActive = activeReminder
    reminderActionInFlightIDs.insert(id)
    reminders.removeAll { $0.id == id }
    queuedReminders.removeAll { $0.id == id }
    if activeReminder?.id == id {
      activeReminder = nil
    }
    presentNextReminderIfNeeded()
    defer { reminderActionInFlightIDs.remove(id) }
    do {
      try await api.ackReminder(id: id)
      await refreshRemindersSilently(limit: 50)
    } catch let err as APIError {
      reminders = previousReminders
      queuedReminders = previousQueued
      activeReminder = previousActive
      if case .server(status: 403, _) = err {
        switch await resolveHouseholdScopedForbidden() {
        case .retry:
          reminderActionInFlightIDs.remove(id)
          await acknowledgeReminder(id: id)
          return
        case .fallbackToNoHousehold:
          clearReminderState(clearLoadingState: true)
          return
        case .failed(let message):
          lastError = message
          return
        }
      }
      if case .unauthorized = err {
        await tokenStore.clear()
        clearReminderState(clearLoadingState: true)
        phase = .unauthenticated
        return
      }
      lastError = err.userFacingMessage
    } catch {
      reminders = previousReminders
      queuedReminders = previousQueued
      activeReminder = previousActive
      lastError = error.localizedDescription
    }
  }

  func openActiveReminder() {
    guard let reminder = activeReminder else { return }
    Task { await openReminder(reminder) }
  }

  func redeemInvite(_ code: String) async throws -> Me {
    try await api.redeemInvite(code: code)
    let me = try await authenticatedMe()
    applyAuthenticated(me)
    return me
  }

  func createHousehold(named name: String, timezone: String) async throws -> Me {
    let updatedMe = try await api.createHousehold(name: name, timezone: timezone)
    applyAuthenticated(updatedMe)
    return updatedMe
  }

  func resolveHouseholdScopedForbidden() async -> HouseholdScopedForbiddenResolution {
    do {
      let me = try await authenticatedMe()
      applyAuthenticated(me)
      return me.currentHouseholdSummary != nil ? .retry : .fallbackToNoHousehold
    } catch let apiError as APIError {
      if case .unauthorized = apiError {
        await tokenStore.clear()
        units = []
        phase = .unauthenticated
        return .fallbackToNoHousehold
      }
      let message = apiError.userFacingMessage
      lastError = message
      return .failed(message)
    } catch {
      let message = error.localizedDescription
      lastError = message
      return .failed(message)
    }
  }

  private func loadUnits() async {
    if !units.isEmpty { return }
    if let fresh = try? await api.units() {
      units = fresh
    }
  }

  func registerCurrentDevice() async {
    guard me != nil else { return }
    do {
      try await api.registerDevice(
        deviceID: Self.stableDeviceID,
        pushToken: pushToken,
        pushAuthorization: pushAuthorization,
        appVersion: Self.appVersion,
      )
    } catch {
      // Best effort. Device registration should not block the app.
    }
  }

  func refreshNotificationAuthorization() async {
    pushAuthorization = await notifications.currentAuthorization()
    if pushAuthorization == .authorized || pushAuthorization == .provisional {
      notifications.registerForRemoteNotifications()
    }
  }

  func requestNotificationAuthorizationIfNeeded() async {
    pushAuthorization = await notifications.currentAuthorization()
    guard pushAuthorization == .notDetermined else { return }
    do {
      let granted = try await notifications.requestAuthorization()
      pushAuthorization = granted ? .authorized : .denied
      await registerCurrentDevice()
      if granted {
        notifications.registerForRemoteNotifications()
      }
    } catch {
      lastError = error.localizedDescription
    }
  }

  func updatePushToken(_ tokenData: Data) async {
    let token = tokenData.map { String(format: "%02x", $0) }.joined()
    guard pushToken != token else { return }
    pushToken = token
    await registerCurrentDevice()
  }

  func handlePushRegistrationFailure(_ error: Error) {
    lastError = error.localizedDescription
  }

  func handleRemoteNotification(_ payload: ReminderPushPayload, opened: Bool) async {
    if opened {
      await openReminder(
        id: payload.reminderID,
        fallbackTarget: InventoryTarget(
          productID: payload.productID,
          locationID: payload.locationID,
          highlightBatchID: payload.batchID,
        ),
        surfacesErrors: false
      )
    }
    await refreshRemindersSilently(limit: 50)
  }

  func householdDayDifference(for isoDate: String?) -> Int? {
    guard let isoDate else { return nil }
    let formatter = Self.yyyymmdd
    guard let date = formatter.date(from: isoDate) else { return nil }
    var calendar = Calendar(identifier: .gregorian)
    calendar.timeZone = householdTimeZone ?? deviceTimeZone
    let todayComponents = calendar.dateComponents([.year, .month, .day], from: .now)
    let expiryComponents = calendar.dateComponents([.year, .month, .day], from: date)
    guard
      let today = calendar.date(from: todayComponents),
      let expiry = calendar.date(from: expiryComponents)
    else { return nil }
    return calendar.dateComponents([.day], from: today, to: expiry).day
  }

  func displayDate(for isoDate: String?) -> String? {
    guard let isoDate, let date = Self.yyyymmdd.date(from: isoDate) else { return nil }
    return Self.displayDateFormatter.string(from: date)
  }

  func reminderUrgencyText(for reminder: Reminder) -> String? {
    reminder.displayUrgency
  }

  private func presentNextReminderIfNeeded() {
    guard activeReminder == nil, !queuedReminders.isEmpty else { return }
    activeReminder = queuedReminders.removeFirst()
  }

  private func openReminder(_ reminder: Reminder) async {
    await openReminder(
      id: reminder.id,
      fallbackTarget: InventoryTarget(
        productID: reminder.productID,
        locationID: reminder.locationID,
        highlightBatchID: reminder.batchID,
      ),
      surfacesErrors: true
    )
  }

  func openReminderFromInbox(_ reminder: Reminder) {
    Task { await openReminder(reminder) }
  }

  func isReminderActionInFlight(id: String) -> Bool {
    reminderActionInFlightIDs.contains(id)
  }

  private func openReminder(
    id: String,
    fallbackTarget: InventoryTarget,
    surfacesErrors: Bool
  ) async {
    guard !reminderActionInFlightIDs.contains(id) else { return }
    reminderActionInFlightIDs.insert(id)
    defer { reminderActionInFlightIDs.remove(id) }
    do {
      try await api.openReminder(id: id)
      pendingInventoryTarget = fallbackTarget
      if activeReminder?.id == id {
        activeReminder = nil
        presentNextReminderIfNeeded()
      }
      queuedReminders.removeAll { $0.id == id }
      await refreshRemindersSilently(limit: 50)
    } catch let err as APIError {
      if case .server(status: 403, _) = err {
        switch await resolveHouseholdScopedForbidden() {
        case .retry:
          reminderActionInFlightIDs.remove(id)
          await openReminder(id: id, fallbackTarget: fallbackTarget, surfacesErrors: surfacesErrors)
          return
        case .fallbackToNoHousehold:
          clearReminderState(clearLoadingState: true)
          return
        case .failed(let message):
          if surfacesErrors {
            lastError = message
          }
          return
        }
      }
      if case .unauthorized = err {
        await tokenStore.clear()
        clearReminderState(clearLoadingState: true)
        phase = .unauthenticated
        return
      }
      pendingInventoryTarget = fallbackTarget
      if surfacesErrors {
        lastError = err.userFacingMessage
      }
      await refreshRemindersSilently(limit: 50)
    } catch {
      pendingInventoryTarget = fallbackTarget
      if surfacesErrors {
        lastError = error.localizedDescription
      }
      await refreshRemindersSilently(limit: 50)
    }
  }

  private func applyReminderSnapshot(_ snapshot: [Reminder]) {
    reminders = Self.sortedReminders(snapshot)
    let validIDs = Set(snapshot.map(\.id))
    queuedReminders.removeAll { !validIDs.contains($0.id) }
    if let activeReminder, !validIDs.contains(activeReminder.id) {
      self.activeReminder = nil
    }
  }

  private static func sortedReminders(_ reminders: [Reminder]) -> [Reminder] {
    reminders.sorted { lhs, rhs in
      let lhsExpires = lhs.expiresOn ?? ""
      let rhsExpires = rhs.expiresOn ?? ""
      if lhsExpires != rhsExpires {
        if lhsExpires.isEmpty { return false }
        if rhsExpires.isEmpty { return true }
        return lhsExpires < rhsExpires
      }
      if lhs.householdFireLocalAt != rhs.householdFireLocalAt {
        return lhs.householdFireLocalAt < rhs.householdFireLocalAt
      }
      return lhs.id < rhs.id
    }
  }

  private func clearReminderState(clearLoadingState: Bool) {
    reminders = []
    queuedReminders = []
    activeReminder = nil
    reminderActionInFlightIDs.removeAll()
    reminderInboxError = nil
    hasLoadedReminderInbox = false
    if clearLoadingState {
      isLoadingReminders = false
    }
  }

  private func authenticatedMe() async throws -> Me {
    try await api.me()
  }

  private func userMessage(for error: Error) -> String {
    if let apiError = error as? APIError {
      return apiError.userFacingMessage
    }
    return error.localizedDescription
  }

  nonisolated fileprivate static func mapAuthorization(_ status: UNAuthorizationStatus)
    -> PushAuthorizationStatus
  {
    switch status {
    case .authorized:
      return .authorized
    case .provisional, .ephemeral:
      return .provisional
    case .denied:
      return .denied
    case .notDetermined:
      return .notDetermined
    @unknown default:
      return .notDetermined
    }
  }

  private static let yyyymmdd: DateFormatter = {
    let f = DateFormatter()
    f.dateFormat = "yyyy-MM-dd"
    f.timeZone = .init(secondsFromGMT: 0)
    f.locale = .init(identifier: "en_US_POSIX")
    return f
  }()

  private static let displayDateFormatter: DateFormatter = {
    let f = DateFormatter()
    f.dateStyle = .medium
    f.timeStyle = .none
    f.timeZone = .init(secondsFromGMT: 0)
    return f
  }()

  private static let stableDeviceID: String = {
    let key = "quartermaster.device_id"
    if let existing = UserDefaults.standard.string(forKey: key) {
      return existing
    }
    let created = UUID().uuidString.lowercased()
    UserDefaults.standard.set(created, forKey: key)
    return created
  }()

  private static let appVersion: String? = {
    let short = Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String
    let build = Bundle.main.object(forInfoDictionaryKey: "CFBundleVersion") as? String
    switch (short, build) {
    case (let short?, let build?) where !short.isEmpty && !build.isEmpty:
      return "\(short) (\(build))"
    case (let short?, _):
      return short
    case (_, let build?):
      return build
    default:
      return nil
    }
  }()
}

struct InventoryTarget: Equatable, Hashable, Sendable {
  let productID: String
  let locationID: String
  /// When set, the batches sheet should scroll to and briefly highlight
  /// this batch — so "Open in Inventory" from a history row makes it
  /// obvious which batch the user came from when there are several.
  var highlightBatchID: String?
}

struct InviteContext: Equatable, Sendable {
  let inviteCode: String?
  let serverURL: URL?
}

struct ReminderPushPayload: Equatable, Sendable {
  let reminderID: String
  let batchID: String
  let productID: String
  let locationID: String

  init?(userInfo: [AnyHashable: Any]) {
    guard
      let reminderID = userInfo["reminder_id"] as? String,
      let batchID = userInfo["batch_id"] as? String,
      let productID = userInfo["product_id"] as? String,
      let locationID = userInfo["location_id"] as? String
    else {
      return nil
    }
    self.reminderID = reminderID
    self.batchID = batchID
    self.productID = productID
    self.locationID = locationID
  }
}
