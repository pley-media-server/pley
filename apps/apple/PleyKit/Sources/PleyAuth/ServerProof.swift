import CryptoKit
import Foundation
import PleyCommon
import PleyModel
import PleyNetwork

/// What tells the server from another device at its address later
/// (docs/API.md, "Telling servers apart"): its id, and the key of this
/// sign-in with which it proves it is the one signed in to. Its
/// `description` never shows the key.
public struct ServerProof: Codable, Sendable, Hashable, CustomStringConvertible, CustomDebugStringConvertible {
    public let serverId: String
    /// Digits, as the server wrote them.
    public let proofId: String
    /// 32 bytes in base64url without padding.
    public let proofKey: String

    /// The proof written as it should be, or `nil`: a server id, a
    /// `proof_id` of digits without a leading zero, and a key of 32 bytes
    /// in base64url without padding.
    public init?(serverId: String?, proofId: String?, proofKey: String?) {
        guard let serverId, !serverId.isEmpty, let proofId, Self.isProofId(proofId),
              let proofKey, Self.keyBytes(proofKey) != nil
        else { return nil }
        self.serverId = serverId
        self.proofId = proofId
        self.proofKey = proofKey
    }

    /// The proof in a token answer, or `nil` when it has none or one written wrong.
    public init?(_ answer: TokenResponse) {
        self.init(serverId: answer.serverId, proofId: answer.proofId, proofKey: answer.proofKey)
    }

    private enum CodingKeys: String, CodingKey {
        case serverId, proofId, proofKey
    }

    /// A kept proof written wrong reads as an error: `StoredTokens` keeps none then.
    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        guard let proof = ServerProof(serverId: try c.decode(String.self, forKey: .serverId),
                                      proofId: try c.decode(String.self, forKey: .proofId),
                                      proofKey: try c.decode(String.self, forKey: .proofKey))
        else { throw DecodingError.dataCorruptedError(forKey: .proofKey, in: c, debugDescription: "not a proof") }
        self = proof
    }

    public var description: String { "ServerProof(serverId: \(serverId), proofId: \(proofId))" }
    public var debugDescription: String { description }

    /// The key's bytes.
    var key: SymmetricKey? { Self.keyBytes(proofKey).map { SymmetricKey(data: $0) } }

    /// `1` to `9999999999999999999`, as the server writes a grant's id.
    static func isProofId(_ text: String) -> Bool {
        guard let first = text.first, first != "0", text.count <= 19 else { return false }
        return text.allSatisfy { $0.isASCII && $0.isNumber }
    }

    /// The 32 bytes of a key written in base64url without padding; `nil` for anything else.
    static func keyBytes(_ text: String) -> Data? {
        guard text.count == 43, text.allSatisfy({ $0.isASCII && ($0.isLetter || $0.isNumber || $0 == "-" || $0 == "_") })
        else { return nil }
        let standard = text.replacingOccurrences(of: "-", with: "+").replacingOccurrences(of: "_", with: "/") + "="
        guard let bytes = Data(base64Encoded: standard), bytes.count == 32 else { return nil }
        return bytes
    }
}

/// Checking a server's proofs.
public enum ServerProofs {
    /// How long a proof counts for an access token, unless the network
    /// changes or a request to the server gets no answer first.
    public static let lasting: TimeInterval = 30 * 60

    /// The challenge's random bytes: 16 to 64 go.
    static let challengeBytes = 16

    /// `count` random bytes from the system.
    public static func randomBytes(_ count: Int) -> Data {
        var generator = SystemRandomNumberGenerator()
        return Data((0..<count).map { _ in UInt8.random(in: .min ... .max, using: &generator) })
    }

    /// The proof of `serverId`, asked at `address`, for `challenge` under
    /// `key`: HMAC-SHA256 of `pley-server-proof\n<id>\n<address>\n<challenge>`,
    /// in base64url without padding.
    public static func proof(key: SymmetricKey, serverId: String, address: String, challenge: String) -> String {
        let text = "pley-server-proof\n\(serverId)\n\(address)\n\(challenge)"
        return base64url(Data(HMAC<SHA256>.authenticationCode(for: Data(text.utf8), using: key)))
    }

    /// Asks the server at `issuer` to prove it holds `proof`'s key, with a
    /// challenge of `random`'s bytes. Throws the network's error when
    /// nothing answers, `.unreachable` when the server or a proxy before
    /// it fails for now (a 5xx), and `.unreachable(anotherDevice)` when
    /// what answers can't prove it: a wrong id or proof, a redirect, a 403
    /// or any other answer.
    static func prove(_ proof: ServerProof, issuer: String, endpoints: EndpointClient,
                      random: @Sendable (Int) -> Data) async throws {
        let notIt = PleyError.unreachable(PleyError.anotherDevice)
        guard let key = proof.key else { throw notIt }
        let challenge = base64url(random(challengeBytes))
        // The address as the server reads it from the request: the proof covers it.
        guard let address = Addresses.origin(issuer),
              var components = URLComponents(string: address + "/api/server")
        else { throw notIt }
        components.queryItems = [URLQueryItem(name: "challenge", value: challenge),
                                 URLQueryItem(name: "proof_id", value: proof.proofId)]
        guard let url = components.url else { throw notIt }
        let answer = try await endpoints.answer(url)
        if answer.status >= 500 { throw PleyError.unreachable(localized("the server couldn't answer (HTTP \(String(answer.status)))")) }
        guard answer.status == 200,
              let body = try? PleyJSON.decoder.decode(ProofAnswer.self, from: answer.body),
              body.id == proof.serverId, let given = body.proof
        else { throw notIt }
        let expected = Self.proof(key: key, serverId: proof.serverId, address: address, challenge: challenge)
        guard constantTimeEqual(Data(given.utf8), Data(expected.utf8)) else { throw notIt }
    }

    /// `/api/server`'s answer, as far as a proof needs it.
    private struct ProofAnswer: Decodable {
        let id: String?
        let proof: String?
    }

    /// Whether `a` and `b` are the same, taking as long whatever their bytes.
    static func constantTimeEqual(_ a: Data, _ b: Data) -> Bool {
        guard a.count == b.count else { return false }
        var difference: UInt8 = 0
        for (x, y) in zip(a, b) { difference |= x ^ y }
        return difference == 0
    }

    static func base64url(_ data: Data) -> String {
        data.base64EncodedString().replacingOccurrences(of: "+", with: "-").replacingOccurrences(of: "/", with: "_")
            .replacingOccurrences(of: "=", with: "")
    }
}
