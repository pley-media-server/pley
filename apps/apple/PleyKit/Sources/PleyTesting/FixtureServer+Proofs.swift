import CryptoKit
import Foundation

/// The server's proofs (docs/API.md, "Telling servers apart"): token
/// answers carry the fixture's id and a key for each grant, the same at
/// every refresh and switch, and `GET /api/server?challenge=&proof_id=`
/// answers the proof for the address the fixture is at. A test can make
/// what answers fail the proof the ways another device would.
extension FixtureServer {
    /// How `/api/server` answers a challenge.
    public enum ProofAnswer: Sendable, Equatable {
        /// The right proof.
        case right
        /// Another server's id.
        case wrongId
        /// A proof under another key.
        case wrongProof
        /// A redirect to the same address.
        case redirect
        /// `403`, as the server answers a client on the internet at a plain-http address.
        case forbidden
        /// `503`: the server, or a proxy before it, out of reach for now.
        case unavailable
    }

    struct Proofs {
        var answer = ProofAnswer.right
        /// Whether token answers carry keys; false is a server from before them.
        var issued = true
        /// Refreshes and switches carry another grant's key, which the app must not take.
        var otherKeyLater = false
    }

    /// The fixture's id, as `/api/server` and the token answers say it.
    public static let serverId = "0123456789abcdef0123456789abcdef01234567"

    /// What the grants' keys come from, as a server's `secret.key`: fixed,
    /// as a real server keeps its key across restarts.
    private static let proofRoot = SymmetricKey(data: SHA256.hash(data: Data("pley fixture server".utf8)))

    // MARK: Knobs

    /// `/api/server` answers challenges so from now on.
    public func answerProofs(_ answer: ProofAnswer) {
        state.withLock { $0.proofs.answer = answer }
    }

    /// Token answers carry keys (the default), or none, as a server before them.
    public func issueProofKeys(_ issued: Bool) {
        state.withLock { $0.proofs.issued = issued }
    }

    /// Refreshes and switches answer with another grant's key from now on.
    public func giveOtherKeysLater(_ other: Bool = true) {
        state.withLock { $0.proofs.otherKeyLater = other }
    }

    /// The challenges `/api/server` was asked, oldest first.
    public var proofRequests: [Recorded] {
        journal.filter { $0.path == "/api/server" && $0.query["challenge"] != nil }
    }

    /// The `proof_id` and key a token answer carries for `grant`.
    public static func proofKey(grant: Int) -> (id: String, key: String) {
        (String(grant), base64url(Data(HMAC<SHA256>.authenticationCode(for: Data("grant \(grant)".utf8),
                                                                          using: proofRoot))))
    }

    // MARK: Routes

    /// The token answer's proof fields for `grant` (`later`: a refresh or switch).
    static func proofFields(_ s: State, grant: Int, later: Bool) -> [String: Any] {
        guard s.proofs.issued else { return [:] }
        let (id, key) = proofKey(grant: later && s.proofs.otherKeyLater ? grant + 1000 : grant)
        return ["server_id": serverId, "proof_id": id, "proof_key": key]
    }

    /// `GET /api/server`: the server, with its id, and its proof when asked.
    static func serverInfo(_ s: State, _ request: Request) -> Answer {
        guard let data = try? Fixtures.data("server/info.json"),
              var info = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any]
        else { return notFound(request.path) }
        info["id"] = serverId
        if let language = s.account.serverLanguage { info["language"] = language }
        let challenge = request.query["challenge"]
        let proofId = request.query["proof_id"]
        if challenge == nil && proofId == nil { return json(200, info) }
        guard let challenge, let proofId, (22...86).contains(challenge.count),
              challenge.allSatisfy({ $0.isASCII && ($0.isLetter || $0.isNumber || $0 == "-" || $0 == "_") }),
              let grant = Int(proofId), grant > 0, String(grant) == proofId
        else { return json(400, ["code": "bad_request", "message": "challenge and proof_id go together"]) }
        switch s.proofs.answer {
        case .redirect:
            return Answer(status: 302, headers: ["Location": "\(s.origin)/api/server?\(request.queryText)"], body: Data())
        case .forbidden:
            return json(403, ["code": "forbidden", "message": "no proof for this address"])
        case .unavailable:
            return json(503, ["code": "unavailable", "message": "starting"])
        case .right, .wrongId, .wrongProof:
            break
        }
        let keyGrant = s.proofs.answer == .wrongProof ? grant + 999 : grant
        let key = SymmetricKey(data: Data(base64url: proofKey(grant: keyGrant).key) ?? Data())
        let text = "pley-server-proof\n\(serverId)\n\(s.origin)\n\(challenge)"
        info["proof"] = base64url(Data(HMAC<SHA256>.authenticationCode(for: Data(text.utf8), using: key)))
        if s.proofs.answer == .wrongId { info["id"] = "fedcba9876543210fedcba9876543210fedcba98" }
        info["address"] = s.origin
        return json(200, info, headers: ["Cache-Control": "no-store"])
    }

    static func base64url(_ data: Data) -> String {
        data.base64EncodedString().replacingOccurrences(of: "+", with: "-").replacingOccurrences(of: "/", with: "_")
            .replacingOccurrences(of: "=", with: "")
    }
}

extension FixtureServer.Request {
    /// The query as it came, for a redirect to send it on.
    var queryText: String {
        query.map { "\($0.key)=\($0.value)" }.sorted().joined(separator: "&")
    }
}

extension Data {
    /// Bytes from base64url without padding.
    init?(base64url text: String) {
        var standard = text.replacingOccurrences(of: "-", with: "+").replacingOccurrences(of: "_", with: "/")
        while standard.count % 4 != 0 { standard += "=" }
        self.init(base64Encoded: standard)
    }
}
