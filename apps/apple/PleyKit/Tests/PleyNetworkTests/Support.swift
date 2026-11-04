import Foundation
import PleyCommon
import PleyNetwork
import PleyTesting

let testAgent = "pleyApple/9.9.9 (macOS 15.2; Mac)"

/// A transport for the tests, with the system's resolver unless given one.
func transport(_ gate: ConnectionGate = ConnectionGate()) -> HTTPTransport {
    HTTPTransport(userAgent: testAgent, gate: gate)
}

/// A started fixture server; `stop()` it at the end.
func fixture() async throws -> FixtureServer {
    let server = FixtureServer()
    try await server.start()
    return server
}

/// Answers with fixed addresses, and counts the lookups.
struct FakeResolver: HostResolver {
    let answer: [String]
    func addresses(of host: String) async throws -> [String] { answer }
}

/// A resolver that must not be asked.
struct NoResolver: HostResolver {
    struct Asked: Error {}
    func addresses(of host: String) async throws -> [String] { throw Asked() }
}

/// A `TokenProvider` for the tests: tokens from the fixture server,
/// refreshed at its token endpoint (so the journal shows each refresh),
/// one refresh at a time.
actor FakeTokens: TokenProvider {
    nonisolated let issuer: String
    private var access: String
    private var refreshToken: String
    private let endpoints: EndpointClient
    private var running: Task<Void, any Error>?
    private var refreshed: Set<String> = []
    private(set) var refreshes = 0
    private(set) var leaked = false
    private(set) var ended = false
    /// What `refusedAfterRefresh` says for the token sent first.
    var refuseAtOnce = false

    init(server: FixtureServer, transport: HTTPTransport, scope: String = FixtureServer.allScopes) {
        issuer = server.origin
        let tokens = server.issueTokens(scope: scope)
        access = tokens.accessToken
        refreshToken = tokens.refreshToken
        endpoints = EndpointClient(transport: transport)
    }

    func setRefuseAtOnce(_ refuse: Bool) { refuseAtOnce = refuse }

    func accessToken() async throws -> String {
        if ended { throw PleyError.signedOut }
        return access
    }

    func refresh(after refused: String) async throws {
        if let running {
            try await running.value
            return
        }
        guard refused == access else { return }
        let task = Task { try await self.exchange() }
        running = task
        defer { running = nil }
        try await task.value
    }

    private func exchange() async throws {
        refreshes += 1
        let url = URL(string: issuer + "/api/oauth/token")!
        let data = try await endpoints.postForm(url, [("grant_type", "refresh_token"), ("refresh_token", refreshToken),
                                                      ("client_id", "https://pley-media-server.github.io/pley/oauth/ios.json")])
        let answer = try JSONSerialization.jsonObject(with: data) as? [String: Any]
        access = answer?["access_token"] as? String ?? ""
        refreshToken = answer?["refresh_token"] as? String ?? ""
        refreshed.insert(access)
    }

    func refusedAfterRefresh(_ token: String) async -> Bool {
        if refuseAtOnce || refreshed.contains(token) {
            ended = true
            return true
        }
        return false
    }

    func connectionLeaked() async {
        leaked = true
        ended = true
    }
}
