import Foundation
import Network
import os

/// A Pley server for the tests: real HTTP/1.1 on 127.0.0.1, so addresses,
/// redirects, metrics and headers are tested over a socket as they are
/// with a home server. A second one is a genuinely other origin.
///
/// It answers from the Android app's fixtures and keeps a small household
/// (`Household.kt`'s profiles): Alex is current; Sam has a PIN; Kids;
/// Robin needs a password; Guest is locked. Tokens it gives are checked
/// like the server's, with `WWW-Authenticate` on a refusal.
///
/// Every request is kept in `journal`, with any token recorded as which
/// one it was, never as the token, so test output prints none.
public final class FixtureServer: Sendable {
    /// One request as it came.
    public struct Recorded: Sendable, Hashable {
        public let method: String
        /// Without the query.
        public let path: String
        public let query: [String: String]
        /// Names in lower case. `authorization` is `Bearer #<n>` for the
        /// fixture's access token number n, or `Bearer (unknown)`.
        public let headers: [String: String]
        /// A form's `grant_type`, for the token endpoint.
        public let grantType: String?
        /// The form's names (values left out: they are codes and tokens).
        public let formKeys: [String]

        /// The access token number the request carried, if one of the fixture's.
        public var token: Int? {
            guard let auth = headers["authorization"], auth.hasPrefix("Bearer #") else { return nil }
            return Int(auth.dropFirst(8))
        }
    }

    /// A pair of tokens the fixture gave.
    public struct Tokens: Sendable, Hashable, CustomStringConvertible {
        public let accessToken: String
        public let refreshToken: String
        public var description: String { "FixtureServer.Tokens(hidden)" }
    }

    /// A profile of the household.
    public struct Profile: Sendable, Hashable {
        public let id: Int
        public let username: String
        public let name: String
        public let avatarColor: String?
        public let hasAvatar: Bool
        public let pin: String?
        /// `none`, `pin`, `password` or `locked`.
        public let needs: String
    }

    public static let household: [Profile] = [
        Profile(id: 2, username: "alex", name: "Alex", avatarColor: "#38bdf8", hasAvatar: true, pin: nil, needs: "none"),
        Profile(id: 3, username: "sam", name: "Sam", avatarColor: "#a3e635", hasAvatar: false, pin: "1234", needs: "pin"),
        Profile(id: 4, username: "kids", name: "Kids", avatarColor: "#f472b6", hasAvatar: false, pin: nil, needs: "none"),
        Profile(id: 1, username: "robin", name: "Robin", avatarColor: nil, hasAvatar: true, pin: nil, needs: "password"),
        Profile(id: 5, username: "guest", name: "Guest", avatarColor: "#94a3b8", hasAvatar: false, pin: nil, needs: "locked"),
    ]

    public static let allScopes = "library play playlists profiles"

    /// An answer of the knobs' own.
    struct Answer: Sendable {
        var status: Int
        var headers: [String: String]
        var body: Data
        var contentLength = true
    }

    private struct Access {
        let id: Int
        let profile: Int
        let scope: String
        let grant: Int
        var valid = true
    }

    private struct Refresh {
        let profile: Int
        let scope: String
        let grant: Int
        var used = false
    }

    private struct State {
        var origin = ""
        var listener: Listener?
        var journal: [Recorded] = []
        var accessTokens: [String: Access] = [:]
        var refreshTokens: [String: Refresh] = [:]
        var endedGrants: Set<Int> = []
        var nextToken = 1
        var nextGrant = 1
        var answers: [String: Answer] = [:]
        var widerScope = false
        var refuseEveryToken = false
        var issuerOverride: String?
        var deviceApproved = false
        var deviceDenied = false
    }

    /// NWListener is Sendable from iOS 17 only; it is safe to share, and only
    /// started and cancelled through this.
    private final class Listener: @unchecked Sendable {
        let listener: NWListener
        init(_ listener: NWListener) { self.listener = listener }
    }

    private let state = OSAllocatedUnfairLock(initialState: State())
    private let queue = DispatchQueue(label: "app.pley.apple.fixture")

    public init() {}

    // MARK: Running

    /// Starts listening on 127.0.0.1, on a free port; returns the origin,
    /// `http://127.0.0.1:<port>`. With `bonjour`, it also announces itself
    /// as a `_pley._tcp` service of that instance name and TXT record, on
    /// this Mac only (the loopback interface).
    @discardableResult
    public func start(bonjour: (instance: String, txt: [String: String])? = nil) async throws -> String {
        let parameters = NWParameters.tcp
        parameters.requiredLocalEndpoint = .hostPort(host: "127.0.0.1", port: .any)
        parameters.allowLocalEndpointReuse = true
        if bonjour != nil { parameters.requiredInterfaceType = .loopback }
        let box = Listener(try NWListener(using: parameters))
        let listener = box.listener
        if let bonjour {
            listener.service = NWListener.Service(name: bonjour.instance, type: "_pley._tcp", domain: "local.",
                                                  txtRecord: NWTXTRecord(bonjour.txt))
        }
        listener.newConnectionHandler = { [weak self] connection in
            guard let self else { connection.cancel(); return }
            self.serve(connection)
        }
        let port: UInt16 = try await withCheckedThrowingContinuation { continuation in
            let once = OSAllocatedUnfairLock(initialState: false)
            listener.stateUpdateHandler = { update in
                let resume: Result<UInt16, any Error>?
                switch update {
                case .ready: resume = .success(box.listener.port?.rawValue ?? 0)
                case .failed(let error): resume = .failure(error)
                case .cancelled: resume = .failure(CancellationError())
                default: resume = nil
                }
                guard let resume, once.withLock({ done in defer { done = true }; return !done }) else { return }
                continuation.resume(with: resume)
            }
            listener.start(queue: queue)
        }
        let origin = "http://127.0.0.1:\(port)"
        state.withLock {
            $0.listener = box
            $0.origin = origin
        }
        return origin
    }

    public func stop() {
        let box = state.withLock { s -> Listener? in
            defer { s.listener = nil }
            return s.listener
        }
        box?.listener.cancel()
    }

    deinit {
        state.withLock { $0.listener }?.listener.cancel()
    }

    public var origin: String { state.withLock { $0.origin } }

    public var journal: [Recorded] { state.withLock { $0.journal } }

    /// The requests for `path`.
    public func requests(_ path: String) -> [Recorded] { journal.filter { $0.path == path } }

    // MARK: Knobs

    /// Gives a new pair of tokens for `profile`, as a sign-in would.
    public func issueTokens(profile: Int = 2, scope: String = FixtureServer.allScopes) -> Tokens {
        state.withLock { s in
            let grant = s.nextGrant
            s.nextGrant += 1
            return Self.issue(&s, profile: profile, scope: scope, grant: grant)
        }
    }

    /// Ends every access token given so far, as if they expired; refresh
    /// tokens still work.
    public func expireAccessTokens() {
        state.withLock { s in
            for key in s.accessTokens.keys { s.accessTokens[key]?.valid = false }
        }
    }

    /// Refuses every access token, new ones too, as a server that ended the sign-in.
    public func refuseEveryToken(_ refuse: Bool = true) {
        state.withLock { $0.refuseEveryToken = refuse }
    }

    /// Token answers allow more than was asked.
    public func setWiderScope(_ wider: Bool = true) {
        state.withLock { $0.widerScope = wider }
    }

    /// The metadata names this issuer instead of the fixture's own origin.
    public func setIssuerOverride(_ issuer: String?) {
        state.withLock { $0.issuerOverride = issuer }
    }

    /// The person allowed the device code.
    public func approveDevice() {
        state.withLock { $0.deviceApproved = true }
    }

    /// The person refused the device code.
    public func denyDevice() {
        state.withLock { $0.deviceDenied = true }
    }

    /// Answers `path` with a redirect (`302`) to `location`.
    public func redirect(path: String, to location: String, status: Int = 302) {
        answer(path: path, status: status, headers: ["Location": location], body: Data())
    }

    /// Answers `path` with this instead of the route's own answer. Without
    /// `contentLength`, the body ends when the connection closes.
    public func answer(path: String, status: Int, headers: [String: String] = [:], body: Data,
                       contentLength: Bool = true) {
        state.withLock {
            $0.answers[path] = Answer(status: status, headers: headers, body: body, contentLength: contentLength)
        }
    }

    public func answer(path: String, status: Int, headers: [String: String] = [:], json: String) {
        var headers = headers
        headers["Content-Type"] = headers["Content-Type"] ?? "application/json"
        answer(path: path, status: status, headers: headers, body: Data(json.utf8))
    }

    // MARK: HTTP

    private struct Request {
        let method: String
        let path: String
        let query: [String: String]
        let headers: [String: String]
        let body: Data

        var form: [String: String] { FixtureServer.parseQuery(String(decoding: body, as: UTF8.self)) }
    }

    private static let maxHeader = 32 << 10
    private static let maxBody = 1 << 20

    private func serve(_ connection: NWConnection) {
        connection.start(queue: queue)
        receive(connection, buffer: Data())
    }

    private func receive(_ connection: NWConnection, buffer: Data) {
        connection.receive(minimumIncompleteLength: 1, maximumLength: 64 << 10) { [weak self] data, _, complete, error in
            guard let self else { connection.cancel(); return }
            var buffer = buffer
            if let data { buffer.append(data) }
            switch Self.parse(buffer) {
            case .some(.some(let request)):
                let answer = self.handle(request)
                self.send(answer, on: connection)
            case .some(.none):
                self.send(Answer(status: 400, headers: [:], body: Data()), on: connection)
            case .none:
                if complete || error != nil {
                    connection.cancel()
                } else {
                    self.receive(connection, buffer: buffer)
                }
            }
        }
    }

    /// `nil`: more to come; `.some(nil)`: a request that can't be read.
    private static func parse(_ buffer: Data) -> Request?? {
        // Not HTTP (a TLS hello from an https try): refused at once.
        if let first = buffer.first, !(UInt8(ascii: "A")...UInt8(ascii: "Z")).contains(first) { return .some(nil) }
        guard let end = buffer.range(of: Data("\r\n\r\n".utf8)) else {
            return buffer.count > maxHeader ? .some(nil) : nil
        }
        guard end.lowerBound <= maxHeader else { return .some(nil) }
        let head = String(decoding: buffer[..<end.lowerBound], as: UTF8.self)
        var lines = head.components(separatedBy: "\r\n")
        let requestLine = lines.removeFirst().split(separator: " ")
        guard requestLine.count == 3 else { return .some(nil) }
        var headers: [String: String] = [:]
        for line in lines {
            guard let colon = line.firstIndex(of: ":") else { return .some(nil) }
            let name = line[..<colon].trimmingCharacters(in: .whitespaces).lowercased()
            let value = line[line.index(after: colon)...].trimmingCharacters(in: .whitespaces)
            headers[name] = headers[name].map { "\($0), \(value)" } ?? value
        }
        let length = Int(headers["content-length"] ?? "0") ?? -1
        guard (0...maxBody).contains(length) else { return .some(nil) }
        let bodyStart = end.upperBound
        guard buffer.count - (bodyStart - buffer.startIndex) >= length else { return nil }
        let body = buffer[bodyStart..<(bodyStart + length)]
        let target = String(requestLine[1])
        let pathEnd = target.firstIndex(of: "?") ?? target.endIndex
        let path = String(target[..<pathEnd]).removingPercentEncoding ?? String(target[..<pathEnd])
        let query = pathEnd < target.endIndex ? parseQuery(String(target[target.index(after: pathEnd)...])) : [:]
        return .some(Request(method: String(requestLine[0]), path: path, query: query, headers: headers,
                             body: Data(body)))
    }

    static func parseQuery(_ text: String) -> [String: String] {
        var out: [String: String] = [:]
        for pair in text.split(separator: "&") {
            let parts = pair.split(separator: "=", maxSplits: 1, omittingEmptySubsequences: false)
            func decode(_ s: Substring) -> String {
                s.replacingOccurrences(of: "+", with: " ").removingPercentEncoding ?? String(s)
            }
            let name = decode(parts[0])
            if out[name] == nil { out[name] = parts.count > 1 ? decode(parts[1]) : "" }
        }
        return out
    }

    private func send(_ answer: Answer, on connection: NWConnection) {
        var head = "HTTP/1.1 \(answer.status) \(Self.reason(answer.status))\r\n"
        var headers = answer.headers
        if answer.contentLength { headers["Content-Length"] = String(answer.body.count) }
        headers["Connection"] = "close"
        for (name, value) in headers.sorted(by: { $0.key < $1.key }) { head += "\(name): \(value)\r\n" }
        head += "\r\n"
        var bytes = Data(head.utf8)
        bytes.append(answer.body)
        connection.send(content: bytes, contentContext: .finalMessage, isComplete: true,
                        completion: .contentProcessed { _ in connection.cancel() })
    }

    private static func reason(_ status: Int) -> String {
        switch status {
        case 200: "OK"
        case 204: "No Content"
        case 302: "Found"
        case 303: "See Other"
        case 307: "Temporary Redirect"
        case 400: "Bad Request"
        case 401: "Unauthorized"
        case 403: "Forbidden"
        case 404: "Not Found"
        case 410: "Gone"
        case 501: "Not Implemented"
        default: "Status"
        }
    }

    // MARK: Routes

    private func handle(_ request: Request) -> Answer {
        let form = request.headers["content-type"]?.hasPrefix("application/x-www-form-urlencoded") == true
            ? request.form : [:]
        return state.withLock { s in
            var headers = request.headers
            if let auth = headers["authorization"] {
                let token = auth.hasPrefix("Bearer ") ? String(auth.dropFirst(7)) : ""
                headers["authorization"] = s.accessTokens[token].map { "Bearer #\($0.id)" } ?? "Bearer (unknown)"
            }
            s.journal.append(Recorded(method: request.method, path: request.path, query: request.query,
                                      headers: headers, grantType: form["grant_type"], formKeys: form.keys.sorted()))
            if let answer = s.answers[request.path] { return answer }
            return Self.route(&s, request, form: form)
        }
    }

    private static func route(_ s: inout State, _ request: Request, form: [String: String]) -> Answer {
        let path = request.path
        let get = request.method == "GET"
        switch path {
        case "/api/server" where get:
            return fixture("server/info.json")
        case "/.well-known/oauth-authorization-server" where get:
            return json(200, metadata(issuer: s.issuerOverride ?? s.origin, origin: s.origin))
        case "/api/auth/me" where get:
            return withToken(&s, request) { _, access in json(200, me(profile: access.profile, scope: access.scope)) }
        case "/api/oauth/profiles" where get:
            return withToken(&s, request, scope: "profiles") { _, access in
                json(200, household.map { profileJSON($0, current: $0.id == access.profile) })
            }
        case "/api/hubs/home" where get:
            return withToken(&s, request, scope: "library") { _, _ in fixture("hubs/home.json") }
        case "/api/oauth/token" where request.method == "POST":
            return token(&s, form)
        case "/api/oauth/revoke" where request.method == "POST":
            if let token = form["token"] { end(&s, token: token) }
            return Answer(status: 200, headers: [:], body: Data())
        case "/oauth/authorize", "/api/oauth/device":
            return notBuilt()
        default:
            break
        }
        let parts = path.split(separator: "/").map(String.init)
        if get, parts.count == 5, parts[0] == "api", parts[1] == "items", Int(parts[2]) != nil, parts[3] == "images",
           ["poster", "backdrop", "thumb"].contains(parts[4]) {
            return withToken(&s, request, scope: "library") { _, _ in
                let width = min(max(Int(request.query["w"] ?? "") ?? 200, 1), 2000)
                let height = parts[4] == "poster" ? width * 3 / 2 : width * 9 / 16
                return image(width: width, height: max(height, 1), type: "public.png", contentType: "image/png")
            }
        }
        if get, parts.count == 4, parts[0] == "api", parts[1] == "users", let id = Int(parts[2]), parts[3] == "avatar" {
            return withToken(&s, request) { _, _ in
                guard household.contains(where: { $0.id == id && $0.hasAvatar }) else { return notFound(path) }
                return image(width: 256, height: 256, type: "public.jpeg", contentType: "image/jpeg")
            }
        }
        return notFound(path)
    }

    private static func withToken(_ s: inout State, _ request: Request, scope: String? = nil,
                                  _ body: (inout State, Access) -> Answer) -> Answer {
        let auth = request.headers["authorization"] ?? ""
        let token = auth.hasPrefix("Bearer ") ? String(auth.dropFirst(7)) : ""
        guard !s.refuseEveryToken, let access = s.accessTokens[token], access.valid,
              !s.endedGrants.contains(access.grant)
        else {
            return json(401, ["code": "unauthorized", "message": "sign in again"],
                        headers: ["WWW-Authenticate": #"Bearer error="invalid_token""#])
        }
        if let scope, !access.scope.split(separator: " ").contains(Substring(scope)) {
            return json(403, ["code": "insufficient_scope", "message": "this app wasn't allowed that"],
                        headers: ["WWW-Authenticate": #"Bearer error="insufficient_scope", scope="\#(scope)""#])
        }
        return body(&s, access)
    }

    private static func token(_ s: inout State, _ form: [String: String]) -> Answer {
        switch form["grant_type"] {
        case "refresh_token":
            guard let presented = form["refresh_token"], let refresh = s.refreshTokens[presented],
                  !s.endedGrants.contains(refresh.grant)
            else { return oauthError("invalid_grant", "the sign-in has ended") }
            if refresh.used {
                // A refresh token used twice: someone else has it. The whole grant ends.
                s.endedGrants.insert(refresh.grant)
                return oauthError("invalid_grant", "the sign-in has ended")
            }
            s.refreshTokens[presented]?.used = true
            let tokens = issue(&s, profile: refresh.profile, scope: refresh.scope, grant: refresh.grant)
            return tokenAnswer(s, tokens, scope: refresh.scope)
        case nil:
            return oauthError("invalid_request", "grant_type is missing")
        default:
            return Answer(status: 501, headers: ["Content-Type": "application/json"],
                          body: Data(#"{"error":"unsupported_grant_type","error_description":"not in the fixture yet"}"#.utf8))
        }
    }

    private static func issue(_ s: inout State, profile: Int, scope: String, grant: Int) -> Tokens {
        let n = s.nextToken
        s.nextToken += 1
        let random = (0..<4).map { _ in String(UInt64.random(in: 0...UInt64.max), radix: 16) }.joined()
        let access = "pley_at_\(n)_\(random)"
        let refresh = "pley_rt_\(n)_\(random)"
        s.accessTokens[access] = Access(id: n, profile: profile, scope: scope, grant: grant)
        s.refreshTokens[refresh] = Refresh(profile: profile, scope: scope, grant: grant)
        return Tokens(accessToken: access, refreshToken: refresh)
    }

    private static func end(_ s: inout State, token: String) {
        if let grant = s.accessTokens[token]?.grant ?? s.refreshTokens[token]?.grant { s.endedGrants.insert(grant) }
    }

    private static func tokenAnswer(_ s: State, _ tokens: Tokens, scope: String) -> Answer {
        json(200, ["access_token": tokens.accessToken, "token_type": "Bearer", "expires_in": 3600,
                   "refresh_token": tokens.refreshToken, "scope": s.widerScope ? scope + " admin" : scope],
             headers: ["Cache-Control": "no-store", "Pragma": "no-cache"])
    }

    private static func oauthError(_ error: String, _ description: String, status: Int = 400) -> Answer {
        json(status, ["error": error, "error_description": description],
             headers: ["Cache-Control": "no-store", "Pragma": "no-cache"])
    }

    private static func metadata(issuer: String, origin: String) -> [String: Any] {
        [
            "issuer": issuer,
            "authorization_endpoint": "\(origin)/oauth/authorize",
            "token_endpoint": "\(origin)/api/oauth/token",
            "revocation_endpoint": "\(origin)/api/oauth/revoke",
            "device_authorization_endpoint": "\(origin)/api/oauth/device",
            "response_types_supported": ["code"],
            "response_modes_supported": ["query"],
            "grant_types_supported": ["authorization_code", "refresh_token",
                                      "urn:pley:params:oauth:grant-type:switch-profile",
                                      "urn:ietf:params:oauth:grant-type:device_code"],
            "token_endpoint_auth_methods_supported": ["none"],
            "revocation_endpoint_auth_methods_supported": ["none"],
            "code_challenge_methods_supported": ["S256"],
            "scopes_supported": ["library", "play", "playlists", "profiles"],
            "authorization_response_iss_parameter_supported": true,
            "client_id_metadata_document_supported": false,
        ]
    }

    private static func me(profile: Int, scope: String) -> Any {
        let p = household.first { $0.id == profile } ?? household[0]
        var user = (try? JSONSerialization.jsonObject(with: Fixtures.data("me/app_token.json"))) as? [String: Any] ?? [:]
        user["id"] = p.id
        user["username"] = p.username
        user["name"] = p.name
        user["display_name"] = p.name
        user["managed"] = p.id != 2 && p.id != 1
        user["has_pin"] = p.pin != nil
        user["avatar_url"] = p.hasAvatar ? "/api/users/\(p.id)/avatar?v=1761100000" : NSNull()
        user["avatar_color"] = p.avatarColor ?? NSNull()
        user["scope"] = scope
        return user
    }

    private static func profileJSON(_ p: Profile, current: Bool) -> [String: Any] {
        var out: [String: Any] = ["id": p.id, "name": p.name, "current": current, "needs": p.needs]
        out["avatar_url"] = p.hasAvatar ? "/api/users/\(p.id)/avatar?v=1761100000" : NSNull()
        out["avatar_color"] = p.avatarColor ?? NSNull()
        return out
    }

    private static func fixture(_ path: String) -> Answer {
        guard let data = try? Fixtures.data(path) else { return notFound(path) }
        return Answer(status: 200, headers: ["Content-Type": "application/json"], body: data)
    }

    private static func notFound(_ path: String) -> Answer {
        json(404, ["code": "not_found", "message": "\(path) not found"])
    }

    private static func notBuilt() -> Answer {
        json(501, ["code": "not_implemented", "message": "not in the fixture yet"])
    }

    private static func json(_ status: Int, _ object: Any, headers: [String: String] = [:]) -> Answer {
        var headers = headers
        headers["Content-Type"] = "application/json"
        let body = (try? JSONSerialization.data(withJSONObject: object, options: [.sortedKeys])) ?? Data()
        return Answer(status: status, headers: headers, body: body)
    }

    /// A picture of one colour, made here: no file to keep.
    private static func image(width: Int, height: Int, type: String, contentType: String) -> Answer {
        let data = FixtureImages.make(width: width, height: height, type: type)
        return Answer(status: 200, headers: ["Content-Type": contentType], body: data)
    }
}
