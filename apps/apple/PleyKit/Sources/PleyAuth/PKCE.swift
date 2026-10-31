import CryptoKit
import Foundation
import Security

/// Proof Key for Code Exchange (RFC 7636), with S256 only, and the `state`
/// that ties the browser's answer to its sign-in.
public enum PKCE {
    /// 48 random bytes in base64url: 64 characters of `A-Z a-z 0-9 - _`.
    public static func verifier() -> String {
        base64url(random(48))
    }

    /// base64url(SHA-256(verifier)): 43 characters.
    public static func challenge(for verifier: String) -> String {
        base64url(Data(SHA256.hash(data: Data(verifier.utf8))))
    }

    /// 32 random bytes in base64url.
    public static func state() -> String {
        base64url(random(32))
    }

    /// Without padding (RFC 7636 Appendix A).
    static func base64url(_ data: Data) -> String {
        data.base64EncodedString()
            .replacingOccurrences(of: "+", with: "-")
            .replacingOccurrences(of: "/", with: "_")
            .replacingOccurrences(of: "=", with: "")
    }

    private static func random(_ count: Int) -> Data {
        var bytes = [UInt8](repeating: 0, count: count)
        let status = SecRandomCopyBytes(kSecRandomDefault, count, &bytes)
        // The system's generator doesn't fail; if it ever did, no sign-in
        // may go ahead with predictable values.
        precondition(status == errSecSuccess, "no random bytes")
        return Data(bytes)
    }
}
