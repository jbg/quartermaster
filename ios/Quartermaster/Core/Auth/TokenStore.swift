import Foundation
import Security

/// Thin wrapper around Keychain services that stores the access + refresh
/// token pair. The store is an actor-isolated class so the API client can
/// safely read/write from background tasks.
actor TokenStore: AppStateTokenStore {
    private let service = "com.quartermaster.app.tokens"
    private let accessAccount = "access"
    private let refreshAccount = "refresh"

    var accessToken: String? { read(account: accessAccount) }
    var refreshToken: String? { read(account: refreshAccount) }

    func store(_ pair: TokenPair) {
        write(account: accessAccount, value: pair.accessToken)
        write(account: refreshAccount, value: pair.refreshToken)
    }

    func clear() {
        delete(account: accessAccount)
        delete(account: refreshAccount)
    }

    // MARK: - Keychain primitives

    private func baseQuery(account: String) -> [String: Any] {
        [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
        ]
    }

    private func read(account: String) -> String? {
        var query = baseQuery(account: account)
        query[kSecReturnData as String] = true
        query[kSecMatchLimit as String] = kSecMatchLimitOne

        var item: AnyObject?
        let status = SecItemCopyMatching(query as CFDictionary, &item)
        guard status == errSecSuccess, let data = item as? Data else {
            return nil
        }
        return String(data: data, encoding: .utf8)
    }

    private func write(account: String, value: String) {
        let data = Data(value.utf8)
        var query = baseQuery(account: account)
        query[kSecValueData as String] = data
        query[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly

        let status = SecItemAdd(query as CFDictionary, nil)
        if status == errSecDuplicateItem {
            let updateQuery = baseQuery(account: account)
            let attributes: [String: Any] = [
                kSecValueData as String: data,
                kSecAttrAccessible as String: kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly,
            ]
            SecItemUpdate(updateQuery as CFDictionary, attributes as CFDictionary)
        }
    }

    private func delete(account: String) {
        let query = baseQuery(account: account)
        SecItemDelete(query as CFDictionary)
    }
}
