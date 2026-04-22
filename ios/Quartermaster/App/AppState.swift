import Foundation
import Observation
import UIKit
import UserNotifications

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

    private let tokenStore = TokenStore()
    private(set) var api: APIClient
    private var queuedReminders: [Reminder] = []
    private var reminderActionInFlightIDs = Set<String>()
    private var pushToken: String?
    private var pushAuthorization: PushAuthorizationStatus = .notDetermined
    private var isSyncingReminders = false
    private var hasLoadedReminderInbox = false

    init() {
        self.api = APIClient(baseURL: ServerConfig.defaultURL, tokenStore: tokenStore)
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

    func register(username: String, password: String, email: String?, inviteCode: String? = nil) async {
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
        api = APIClient(baseURL: url, tokenStore: tokenStore)
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

        let isJoinLink = (components.scheme == "quartermaster" && components.host == "join")
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
        me?.householdTimezone ?? me?.activeHousehold?.timezone
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
        guard let me, me.householdId != nil else {
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

        do {
            let response = try await api.listReminders(limit: limit)
            hasLoadedReminderInbox = true
            reminderInboxError = nil
            applyReminderSnapshot(response.items)
            let existingIDs = Set(queuedReminders.map(\.id) + (activeReminder.map { [$0.id] } ?? []))
            for reminder in response.items
            where reminder.presentedOnDeviceAt == nil
                && !existingIDs.contains(reminder.id)
                && !reminderActionInFlightIDs.contains(reminder.id)
            {
                try? await api.presentReminder(id: reminder.id)
                queuedReminders.append(reminder)
            }
            presentNextReminderIfNeeded()
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
                    await syncDueReminders(
                        limit: limit,
                        mode: mode == .silent ? .silent : .userInitiated
                    )
                case .fallbackToNoHousehold:
                    clearReminderState(clearLoadingState: true)
                case .failed(let message):
                    if mode != .silent {
                        reminderInboxError = message
                    }
                }
                return
            }
            if mode != .silent {
                reminderInboxError = apiError.userFacingMessage
            }
        } catch {
            if mode != .silent {
                reminderInboxError = error.localizedDescription
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
            return me.householdId != nil ? .retry : .fallbackToNoHousehold
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
        pushAuthorization = await notificationAuthorizationStatus()
        if pushAuthorization == .authorized || pushAuthorization == .provisional {
            UIApplication.shared.registerForRemoteNotifications()
        }
    }

    func requestNotificationAuthorizationIfNeeded() async {
        pushAuthorization = await notificationAuthorizationStatus()
        guard pushAuthorization == .notDetermined else { return }
        do {
            let granted = try await UNUserNotificationCenter.current()
                .requestAuthorization(options: [.badge, .sound, .alert])
            pushAuthorization = granted ? .authorized : .denied
            await registerCurrentDevice()
            if granted {
                UIApplication.shared.registerForRemoteNotifications()
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
        reminders = snapshot
        let validIDs = Set(snapshot.map(\.id))
        queuedReminders.removeAll { !validIDs.contains($0.id) }
        if let activeReminder, !validIDs.contains(activeReminder.id) {
            self.activeReminder = nil
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

    private func notificationAuthorizationStatus() async -> PushAuthorizationStatus {
        await withCheckedContinuation { continuation in
            UNUserNotificationCenter.current().getNotificationSettings { settings in
                continuation.resume(returning: Self.mapAuthorization(settings.authorizationStatus))
            }
        }
    }

    private func userMessage(for error: Error) -> String {
        if let apiError = error as? APIError {
            return apiError.userFacingMessage
        }
        return error.localizedDescription
    }

    nonisolated private static func mapAuthorization(_ status: UNAuthorizationStatus) -> PushAuthorizationStatus {
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
        case let (short?, build?) where !short.isEmpty && !build.isEmpty:
            return "\(short) (\(build))"
        case let (short?, _):
            return short
        case let (_, build?):
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
