import Foundation
import Network

/// Whether an address is this device or on a private network: where codes
/// and tokens may travel over plain http (SECURITY.md). Private means
/// loopback (127/8, ::1), RFC 1918, link-local (169.254/16, fe80::/10),
/// CGNAT (100.64/10, Tailscale's), IPv6 unique local (fc00::/7), the
/// deprecated IPv6 site-local fec0::/10, and an IPv4-mapped IPv6 address
/// whose IPv4 part is private: the rule of the Android app's
/// `Transport.isPrivate`.
public enum PrivateAddress {
    public static func isPrivate(_ address: any IPAddress) -> Bool {
        let bytes = [UInt8](address.rawValue)
        switch bytes.count {
        case 4:
            return isPrivateV4(bytes)
        case 16:
            if bytes[0..<10].allSatisfy({ $0 == 0 }) && bytes[10] == 0xff && bytes[11] == 0xff {
                return isPrivateV4(Array(bytes[12..<16]))
            }
            if bytes[0..<15].allSatisfy({ $0 == 0 }) && bytes[15] == 1 { return true }
            if bytes[0] & 0xfe == 0xfc { return true }
            // fe80::/10 (link-local) and fec0::/10 (site-local).
            return bytes[0] == 0xfe && bytes[1] & 0x80 == 0x80
        default:
            return false
        }
    }

    /// Whether `literal`, an IP address as text (`192.168.1.2`, `fd12::1`,
    /// `[fe80::1%en0]`), is private; `nil` when it isn't an IP address but
    /// a name. Brackets and a `%zone` are ignored.
    public static func isPrivate(literal: String) -> Bool? {
        var text = Substring(literal)
        if text.hasPrefix("[") && text.hasSuffix("]") { text = text.dropFirst().dropLast() }
        if let zone = text.firstIndex(of: "%") { text = text[..<zone] }
        if text.contains(":") {
            return IPv6Address(String(text)).map { isPrivate($0) }
        }
        // Network's parser also takes inet_aton's short forms ("127.1"),
        // as the system's resolver does; only four dotted numbers count here.
        guard text.split(separator: ".", omittingEmptySubsequences: false).count == 4,
              let v4 = IPv4Address(String(text))
        else { return nil }
        return isPrivate(v4)
    }

    private static func isPrivateV4(_ b: [UInt8]) -> Bool {
        b[0] == 127 || b[0] == 10 || (b[0] == 172 && b[1] & 0xf0 == 16) || (b[0] == 192 && b[1] == 168)
            || (b[0] == 169 && b[1] == 254) || (b[0] == 100 && b[1] & 0xc0 == 64)
    }
}
