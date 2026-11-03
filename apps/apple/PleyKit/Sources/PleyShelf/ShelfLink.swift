import CryptoKit
import Foundation

/// A Top Shelf entry's link into the app:
/// `app.pley.apple:/shelf/{open|play}/{itemId}?owner=<owner>&sig=<signature>`.
///
/// The signature is HMAC-SHA256 of the action, the item and the owner with
/// the link key, its first 16 bytes in hex. Only the app and its extension
/// can read the key (in their app group's container), so another app can't
/// make a link that opens or plays something in Pley, as the Android app's
/// signed Watch Next intents.
public struct ShelfLink: Hashable, Sendable {
    public enum Action: String, Hashable, Sendable, CaseIterable {
        /// The title's page.
        case open
        /// Plays the title.
        case play
    }

    /// The URL scheme, the same whatever the bundle ID.
    public static let scheme = "app.pley.apple"
    /// The bytes of the signature kept.
    static let signatureBytes = 16
    /// Longer links aren't read.
    static let maxLength = 512

    public let action: Action
    public let itemId: Int
    public let owner: String

    public init(action: Action, itemId: Int, owner: String) {
        self.action = action
        self.itemId = itemId
        self.owner = owner
    }

    /// The link, signed with `key`.
    public func url(key: SymmetricKey) -> URL {
        var components = URLComponents()
        components.scheme = Self.scheme
        components.path = "/shelf/\(action.rawValue)/\(itemId)"
        components.queryItems = [URLQueryItem(name: "owner", value: owner),
                                 URLQueryItem(name: "sig", value: Self.hex(signature(key: key)))]
        // A path and a query: always a URL.
        return components.url!
    }

    /// The link `url` says, when it is one and `key` signed it; `nil` otherwise.
    public static func verified(_ url: URL, key: SymmetricKey) -> ShelfLink? {
        guard let link = parse(url),
              let given = URLComponents(url: url, resolvingAgainstBaseURL: false)?.queryItems?
                .first(where: { $0.name == "sig" })?.value,
              let bytes = bytes(hex: given), bytes.count == signatureBytes
        else { return nil }
        let expected = link.signature(key: key)
        guard expected.count == bytes.count else { return nil }
        // Every byte compared, whatever differs first, so the time taken tells nothing.
        var difference: UInt8 = 0
        for (a, b) in zip(expected, bytes) { difference |= a ^ b }
        return difference == 0 ? link : nil
    }

    /// What `url` says, signed or not: `nil` when it isn't a shelf link.
    public static func parse(_ url: URL) -> ShelfLink? {
        guard url.absoluteString.count <= maxLength, url.scheme == scheme,
              let components = URLComponents(url: url, resolvingAgainstBaseURL: false)
        else { return nil }
        let parts = components.path.split(separator: "/", omittingEmptySubsequences: true)
        guard parts.count == 3, parts[0] == "shelf", let action = Action(rawValue: String(parts[1])),
              let itemId = Int(parts[2]), itemId > 0,
              let owner = components.queryItems?.first(where: { $0.name == "owner" })?.value,
              !owner.isEmpty, owner.count <= 64
        else { return nil }
        return ShelfLink(action: action, itemId: itemId, owner: owner)
    }

    private var message: Data { Data("\(action.rawValue)\n\(itemId)\n\(owner)".utf8) }

    private func signature(key: SymmetricKey) -> Data {
        Data(HMAC<SHA256>.authenticationCode(for: message, using: key)).prefix(Self.signatureBytes)
    }

    private static func hex(_ data: Data) -> String {
        data.map { String(format: "%02x", $0) }.joined()
    }

    private static func bytes(hex: String) -> Data? {
        guard hex.count % 2 == 0, hex.count <= 128 else { return nil }
        var data = Data()
        var index = hex.startIndex
        while index < hex.endIndex {
            let next = hex.index(index, offsetBy: 2)
            guard let byte = UInt8(hex[index..<next], radix: 16) else { return nil }
            data.append(byte)
            index = next
        }
        return data
    }
}
