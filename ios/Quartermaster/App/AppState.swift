import Foundation
import Observation

@Observable
@MainActor
final class AppState {
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
    var activeReminder: Reminder?
    /// Deep-link target set by history → "Open in Inventory". MainTabView
    /// observes this to switch tabs; InventoryView observes it to present
    /// the batches sheet for the named product+location, then clears it.
    var pendingInventoryTarget: InventoryTarget?
    var pendingInviteContext: InviteContext?

    private let tokenStore = TokenStore()
    private(set) var api: APIClient
    private var queuedReminders: [Reminder] = []
    private var isSyncingReminders = false

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
            await syncDueReminders()
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
        queuedReminders = []
        activeReminder = nil
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

    func syncDueReminders(limit: Int = 20) async {
        guard !isSyncingReminders else { return }
        guard let me, me.householdId != nil else {
            queuedReminders = []
            activeReminder = nil
            return
        }

        isSyncingReminders = true
        defer { isSyncingReminders = false }

        do {
            let response = try await api.listReminders(limit: limit)
            let existingIDs = Set(queuedReminders.map(\.id) + (activeReminder.map { [$0.id] } ?? []))
            for reminder in response.items where !existingIDs.contains(reminder.id) {
                try? await api.ackReminder(id: reminder.id)
                queuedReminders.append(reminder)
            }
            presentNextReminderIfNeeded()
        } catch let apiError as APIError {
            if case .unauthorized = apiError {
                await tokenStore.clear()
                units = []
                queuedReminders = []
                activeReminder = nil
                phase = .unauthenticated
            }
        } catch {
            // Reminder polling is best-effort; ignore transient failures.
        }
    }

    func dismissActiveReminder() {
        activeReminder = nil
        presentNextReminderIfNeeded()
    }

    func openActiveReminder() {
        guard let reminder = activeReminder else { return }
        pendingInventoryTarget = InventoryTarget(
            productID: reminder.productID,
            locationID: reminder.locationID,
            highlightBatchID: reminder.batchID,
        )
        activeReminder = nil
        presentNextReminderIfNeeded()
    }

    func redeemInvite(_ code: String) async throws -> Me {
        try await api.redeemInvite(code: code)
        let me = try await authenticatedMe()
        applyAuthenticated(me)
        return me
    }

    func createHousehold(named name: String) async throws -> Me {
        let updatedMe = try await api.createHousehold(name: name)
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

    private func presentNextReminderIfNeeded() {
        guard activeReminder == nil, !queuedReminders.isEmpty else { return }
        activeReminder = queuedReminders.removeFirst()
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
