import Foundation
import Observation

@Observable
@MainActor
final class AppState {
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
    /// Deep-link target set by history → "Open in Inventory". MainTabView
    /// observes this to switch tabs; InventoryView observes it to present
    /// the batches sheet for the named product+location, then clears it.
    var pendingInventoryTarget: InventoryTarget?

    private let tokenStore = TokenStore()
    private(set) var api: APIClient

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
            phase = .authenticated(me)
            await loadUnits()
            lastError = nil
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
        phase = .unauthenticated
    }

    func updateServerURL(_ url: URL) {
        serverURL = url
        api = APIClient(baseURL: url, tokenStore: tokenStore)
    }

    func unitsFor(family: ProductFamily) -> [Unit] {
        units.filter { $0.family == family }
    }

    private func loadUnits() async {
        if !units.isEmpty { return }
        if let fresh = try? await api.units() {
            units = fresh
        }
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
