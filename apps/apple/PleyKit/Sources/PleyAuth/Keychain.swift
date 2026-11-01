import Foundation
import PleyCommon
import Security

/// A Keychain error, with the Security framework's status. Trying again
/// rarely helps, so its words say what failed rather than "try again".
public struct KeychainError: Error, Sendable, Equatable, CustomStringConvertible {
    public let status: OSStatus

    public init(status: OSStatus) {
        self.status = status
        Log.auth.error("Keychain error \(status, privacy: .public)")
    }

    public var description: String { "Keychain error \(status)" }
}

extension KeychainError: LocalizedError {
    public var errorDescription: String? {
        switch status {
        // A build that isn't signed has no Keychain of its own.
        case errSecMissingEntitlement: "this build of Pley can't use the Keychain: it isn't signed (error \(status))"
        case errSecInteractionNotAllowed: "the Keychain can't be read until this device is unlocked (error \(status))"
        default: "this device's Keychain can't be used (error \(status))"
        }
    }
}

/// One generic-password item of the app's Keychain: `service` and
/// `account` name it, and it holds bytes.
///
/// Items are readable after the first unlock, on this device only: never
/// synchronised to iCloud and never restored to another device, so tokens
/// and server lists stay where they were made. No access group is named, so
/// items go to the app's own group, `$(AppIdentifierPrefix)` and the bundle
/// ID; a Top Shelf extension later shares that group by name.
///
/// On the Mac, items go to the data protection keychain. A build signed to
/// run locally without a team can't use it (`errSecMissingEntitlement`), and
/// uses the login keychain instead.
public struct KeychainItem: Sendable, Hashable {
    public let service: String
    public let account: String

    public init(service: String, account: String) {
        self.service = service
        self.account = account
    }

    /// The bytes kept, or `nil` when there is no item.
    public func read() throws -> Data? {
        var query = query()
        query[kSecReturnData as String] = true
        query[kSecMatchLimit as String] = kSecMatchLimitOne
        var result: CFTypeRef?
        let status = SecItemCopyMatching(query as CFDictionary, &result)
        if status == errSecItemNotFound { return nil }
        guard status == errSecSuccess else { throw KeychainError(status: status) }
        return result as? Data
    }

    /// Keeps `data`, replacing what was there.
    public func write(_ data: Data) throws {
        let query = query()
        let update: [String: Any] = [
            kSecValueData as String: data,
            kSecAttrAccessible as String: kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly,
        ]
        var status = SecItemUpdate(query as CFDictionary, update as CFDictionary)
        if status == errSecItemNotFound {
            status = SecItemAdd(query.merging(update) { $1 } as CFDictionary, nil)
        }
        guard status == errSecSuccess else { throw KeychainError(status: status) }
    }

    /// Removes the item; nothing happens when there is none.
    public func delete() throws {
        let status = SecItemDelete(query() as CFDictionary)
        guard status == errSecSuccess || status == errSecItemNotFound else { throw KeychainError(status: status) }
    }

    /// Whether this process can keep Keychain items at all: false in a test
    /// runner without Keychain access (`errSecMissingEntitlement`,
    /// `errSecNotAvailable`). Tries a throwaway item and removes it.
    public static func isAvailable() -> Bool {
        let probe = KeychainItem(service: "app.pley.apple.probe", account: UUID().uuidString)
        defer { try? probe.delete() }
        do {
            try probe.write(Data([1]))
            return try probe.read() == Data([1])
        } catch {
            return false
        }
    }

    private func query() -> [String: Any] {
        var query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
            kSecAttrSynchronizable as String: false,
        ]
        #if os(macOS)
        if Self.dataProtection { query[kSecUseDataProtectionKeychain as String] = true }
        #endif
        return query
    }

    #if os(macOS)
    /// Whether this build may use the data protection keychain. Asked once
    /// by adding a throwaway item there: a search answers "not found"
    /// rather than `errSecMissingEntitlement`, so only a write tells.
    private static let dataProtection: Bool = {
        let probe: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: "app.pley.apple.probe",
            kSecAttrAccount as String: UUID().uuidString,
            kSecUseDataProtectionKeychain as String: true,
        ]
        var add = probe
        add[kSecValueData as String] = Data([1])
        add[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
        let status = SecItemAdd(add as CFDictionary, nil)
        SecItemDelete(probe as CFDictionary)
        return status != errSecMissingEntitlement
    }()
    #endif
}
