import CryptoKit
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

    /// An item the fixture plays: a movie with one file, its media made by
    /// the test (`MediaMaker`) or none.
    public struct Playable: Sendable {
        public var itemId: Int
        public var fileId: Int
        public var title: String
        public var container: String
        public var durationMs: Int
        public var positionMs: Int
        public var watched: Bool
        public var media: Data

        public init(itemId: Int, fileId: Int, title: String = "Test Movie", container: String = "mp4",
                    durationMs: Int = 4_000, positionMs: Int = 0, watched: Bool = false, media: Data = Data()) {
            self.itemId = itemId
            self.fileId = fileId
            self.title = title
            self.container = container
            self.durationMs = durationMs
            self.positionMs = positionMs
            self.watched = watched
            self.media = media
        }
    }

    /// A progress report as it came: the item and the JSON body.
    public struct ProgressRecord: Sendable, Hashable {
        public let itemId: Int
        public let body: Data
    }

    public static let household: [Profile] = [
        Profile(id: 2, username: "alex", name: "Alex", avatarColor: "#38bdf8", hasAvatar: true, pin: nil, needs: "none"),
        Profile(id: 3, username: "sam", name: "Sam", avatarColor: "#a3e635", hasAvatar: false, pin: "1234", needs: "pin"),
        Profile(id: 4, username: "kids", name: "Kids", avatarColor: "#f472b6", hasAvatar: false, pin: nil, needs: "none"),
        Profile(id: 1, username: "robin", name: "Robin", avatarColor: nil, hasAvatar: true, pin: nil, needs: "password"),
        Profile(id: 5, username: "guest", name: "Guest", avatarColor: "#94a3b8", hasAvatar: false, pin: nil, needs: "locked"),
    ]

    public static let allScopes = "library play playlists profiles"

    /// The client IDs of Pley's Apple apps, which the fixture knows as the server does.
    public static let appleClients = ["ios", "tvos", "macos"].map {
        "https://pley-media-server.github.io/pley/oauth/\($0).json"
    }
    public static let redirectURI = "app.pley.apple:/oauth/callback"
    public static let deviceCodeGrant = "urn:ietf:params:oauth:grant-type:device_code"
    public static let switchProfileGrant = "urn:pley:params:oauth:grant-type:switch-profile"

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

    /// A code the sign-in page gave, waiting to be exchanged.
    private struct AuthCode {
        let clientID: String
        let challenge: String
        let scope: String
        var used = false
    }

    /// A TV's code, waiting for an answer.
    private struct Device {
        let clientID: String
        let scope: String
        var answered = false
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
        var deviceExpired = false
        var slowDowns = 0
        var authorizeError: String?
        var codes: [String: AuthCode] = [:]
        var devices: [String: Device] = [:]
        var wrongPins: [Int: Int] = [:]
        var lockedPins: Set<Int> = []
        var playables: [Int: Playable] = [:]
        var sessions: [String: PlaySession] = [:]
        var progress: [ProgressRecord] = []
        var playbackBodies: [Data] = []
        var streamsBodies: [Data] = []
        var playbackMethod = "direct"
        var playbackFails: Int?
    }

    /// A session the fixture started.
    private struct PlaySession {
        let itemId: Int
        let key: String
        let method: String
        var stopped: String??
        var expired = false
        var deleted = false
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

    /// The device codes given so far run out (`expired_token`).
    public func expireDevice() {
        state.withLock { $0.deviceExpired = true }
    }

    /// The next `count` polls of a device code answer `slow_down`.
    public func slowDown(_ count: Int = 1) {
        state.withLock { $0.slowDowns = count }
    }

    /// The sign-in page sends the browser back with this `error` (`access_denied`).
    public func setAuthorizeError(_ error: String?) {
        state.withLock { $0.authorizeError = error }
    }

    /// The PIN of `profile` is locked, as after 100 wrong ones.
    public func lockPin(of profile: Int) {
        state.withLock { _ = $0.lockedPins.insert(profile) }
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

    /// Plays `playable` from now on: its details, sessions and keyed media.
    public func addPlayable(_ playable: Playable) {
        state.withLock { $0.playables[playable.itemId] = playable }
    }

    /// The method new sessions answer with: `direct` (the default) gives the
    /// keyed file, `remux` and `transcode` a keyed playlist.
    public func setPlaybackMethod(_ method: String) {
        state.withLock { $0.playbackMethod = method }
    }

    /// `POST /api/playback` answers with this status from now on (`nil`: normally).
    public func failPlayback(status: Int?) {
        state.withLock { $0.playbackFails = status }
    }

    /// Stops session `id` from elsewhere: its status, reports and media answer `410` with `message`.
    public func stopSession(_ id: String, message: String? = nil) {
        state.withLock { $0.sessions[id]?.stopped = .some(message) }
    }

    /// Forgets session `id`, as the server does after 90 s without a report: `404`.
    public func expireSession(_ id: String) {
        state.withLock { $0.sessions[id]?.expired = true }
    }

    /// The progress reports received, in order.
    public var progressReports: [ProgressRecord] { state.withLock { $0.progress } }

    /// The bodies of `POST /api/playback`, in order.
    public var playbackRequests: [Data] { state.withLock { $0.playbackBodies } }

    /// The bodies of `PUT /api/items/{id}/streams`, in order.
    public var streamsChoices: [Data] { state.withLock { $0.streamsBodies } }

    /// The sessions started, in no order, and whether each is still open.
    public var openSessions: Set<String> {
        state.withLock { s in Set(s.sessions.filter { !$0.value.deleted && !$0.value.expired }.keys) }
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
        case 206: "Partial Content"
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
        case "/oauth/authorize" where get:
            return authorize(&s, request.query)
        case "/api/oauth/device" where request.method == "POST":
            return device(&s, form)
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
        if let answer = playback(&s, request, parts) { return answer }
        if get, parts.count == 4, parts[0] == "api", parts[1] == "users", let id = Int(parts[2]), parts[3] == "avatar" {
            return withToken(&s, request) { _, _ in
                guard household.contains(where: { $0.id == id && $0.hasAvatar }) else { return notFound(path) }
                return image(width: 256, height: 256, type: "public.jpeg", contentType: "image/jpeg")
            }
        }
        return notFound(path)
    }

    /// The routes of playing: an item's details, sessions, reports and keyed media.
    private static func playback(_ s: inout State, _ request: Request, _ parts: [String]) -> Answer? {
        let method = request.method
        guard parts.count >= 2, parts[0] == "api" else { return nil }
        switch (method, parts.count, parts[1]) {
        case ("GET", 3, "items"):
            guard let id = Int(parts[2]) else { return nil }
            return withToken(&s, request, scope: "library") { s, _ in
                guard let playable = s.playables[id] else { return notFound(request.path) }
                return json(200, detail(playable))
            }
        case ("GET", 4, "items") where parts[3] == "on-deck" || parts[3] == "next":
            return withToken(&s, request, scope: "library") { _, _ in json(200, ["item": NSNull()]) }
        case ("POST", 4, "items") where parts[3] == "progress":
            guard let id = Int(parts[2]) else { return nil }
            return withToken(&s, request, scope: "play") { s, _ in
                s.progress.append(ProgressRecord(itemId: id, body: request.body))
                let body = (try? JSONSerialization.jsonObject(with: request.body)) as? [String: Any] ?? [:]
                if let sid = body["session_id"] as? String, let session = s.sessions[sid] {
                    if case .some(let message) = session.stopped { return stoppedAnswer(message) }
                    if session.expired || session.deleted { return expiredAnswer() }
                }
                return Answer(status: 204, headers: [:], body: Data())
            }
        case ("PUT", 4, "items") where parts[3] == "streams":
            return withToken(&s, request, scope: "play") { s, _ in
                s.streamsBodies.append(request.body)
                return Answer(status: 204, headers: [:], body: Data())
            }
        case ("POST", 2, "playback"):
            return withToken(&s, request, scope: "play") { s, _ in startSession(&s, request.body) }
        case ("GET", 3, "playback") where parts[2] == "network":
            return withToken(&s, request, scope: "play") { _, _ in
                json(200, ["local": true, "ceiling_kbps": NSNull(), "source": NSNull(), "since": NSNull(),
                           "probe": false])
            }
        case ("GET", 3, "playback"), ("DELETE", 3, "playback"):
            let id = parts[2]
            return withToken(&s, request, scope: "play") { s, _ in
                guard let session = s.sessions[id] else { return expiredAnswer() }
                if method == "DELETE" {
                    s.sessions[id]?.deleted = true
                    return Answer(status: 204, headers: [:], body: Data())
                }
                if case .some(let message) = session.stopped { return stoppedAnswer(message) }
                if session.expired || session.deleted { return expiredAnswer() }
                return json(200, ["session_id": id, "method": session.method, "transcode": NSNull()])
            }
        case ("GET", 4, "media"):
            // The key is the session's own: no token.
            guard let session = s.sessions.values.first(where: { $0.key == parts[2] }) else { return expiredAnswer() }
            if case .some(let message) = session.stopped { return stoppedAnswer(message) }
            if session.expired || session.deleted { return expiredAnswer() }
            guard let playable = s.playables[session.itemId], parts[3].hasPrefix("file.") else {
                return notFound("/api/media/…")
            }
            return ranged(playable.media, range: request.headers["range"], type: "video/mp4")
        default:
            return nil
        }
    }

    private static func startSession(_ s: inout State, _ body: Data) -> Answer {
        s.playbackBodies.append(body)
        if let status = s.playbackFails { return json(status, ["code": "unavailable", "message": "no transcode free"]) }
        let asked = (try? JSONSerialization.jsonObject(with: body)) as? [String: Any] ?? [:]
        guard let itemId = asked["item_id"] as? Int, let playable = s.playables[itemId] else {
            return json(404, ["code": "not_found", "message": "item not found"])
        }
        let method = (asked["mode"] as? String).flatMap { ["remux", "transcode"].contains($0) ? $0 : nil }
            ?? s.playbackMethod
        let id = randomToken()
        let key = "pley_mk_" + randomToken()
        s.sessions[id] = PlaySession(itemId: itemId, key: key, method: method)
        let hls = method != "direct"
        return json(200, [
            "session_id": id, "method": method, "hls": hls, "file_id": playable.fileId,
            "url": hls ? "/api/media/\(key)/index.m3u8" : "/api/media/\(key)/file.\(playable.container)",
            "duration_ms": playable.durationMs, "audio_index": 1, "subtitle_index": NSNull(), "burn_subtitles": false,
            "audio_converted": false, "subtitles": [], "reasons": hls ? ["MKV not supported."] : [],
            "reason": NSNull(), "auto_quality": NSNull(), "video_range": NSNull(), "adaptive": false,
            "live_bitrate": false, "pts_offset_ms": hls ? 0 : NSNull(),
        ])
    }

    private static func detail(_ p: Playable) -> [String: Any] {
        [
            "id": p.itemId, "library_id": 1, "kind": "movie", "title": p.title, "sort_title": p.title, "year": 2020,
            "duration_ms": p.durationMs, "added_at": 1761300000,
            "user_state": ["position_ms": p.positionMs, "watched": p.watched, "play_count": p.watched ? 1 : 0],
            "files": [[
                "id": p.fileId, "path": "\(p.title).\(p.container)", "size": p.media.count, "container": p.container,
                "duration_ms": p.durationMs, "bitrate": 1_000_000, "width": 320, "height": 180, "video_codec": "h264",
                "audio_codec": NSNull(),
                "streams": [["index": 0, "kind": "video", "codec": "h264", "default": true, "profile": "High",
                             "width": 320, "height": 180, "frame_rate": 30.0, "bit_depth": 8, "level": 30,
                             "dynamic_range": "sdr"]],
                "selected_audio": NSNull(), "audio_picked": false, "selected_subtitle": NSNull(), "chapters": [],
            ]],
            "markers": [],
        ]
    }

    private static func stoppedAnswer(_ message: String?) -> Answer {
        json(410, ["code": "session_stopped", "error": "session_stopped", "message": message.map { $0 as Any } ?? NSNull()])
    }

    private static func expiredAnswer() -> Answer {
        json(404, ["code": "not_found", "message": "playback session expired"])
    }

    /// `data`, or the part `range` asks for (`bytes=a-b`, `bytes=a-`).
    private static func ranged(_ data: Data, range: String?, type: String) -> Answer {
        var headers = ["Content-Type": type, "Accept-Ranges": "bytes", "Cache-Control": "private, no-store"]
        guard let range, range.hasPrefix("bytes="), !data.isEmpty else {
            return Answer(status: 200, headers: headers, body: data)
        }
        let bounds = range.dropFirst(6).split(separator: "-", maxSplits: 1, omittingEmptySubsequences: false)
        let start = Int(bounds.first ?? "") ?? 0
        let end = min(bounds.count > 1 ? Int(bounds[1]) ?? data.count - 1 : data.count - 1, data.count - 1)
        guard start <= end, start < data.count else {
            return Answer(status: 416, headers: ["Content-Range": "bytes */\(data.count)"], body: Data())
        }
        headers["Content-Range"] = "bytes \(start)-\(end)/\(data.count)"
        return Answer(status: 206, headers: headers, body: data.subdata(in: start..<(end + 1)))
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
        case "authorization_code":
            return exchange(&s, form)
        case deviceCodeGrant:
            return pollDevice(&s, form)
        case switchProfileGrant:
            return switchProfile(&s, form)
        case nil:
            return oauthError("invalid_request", "grant_type is missing")
        default:
            return oauthError("unsupported_grant_type", "that grant isn't supported")
        }
    }

    /// `/oauth/authorize`: checks what the app sent as the server does, and
    /// sends the browser back at once (`303`) as if Alex allowed the app.
    private static func authorize(_ s: inout State, _ query: [String: String]) -> Answer {
        guard let client = query["client_id"], appleClients.contains(client),
              query["redirect_uri"] == redirectURI
        else { return json(400, ["code": "bad_request", "message": "unknown app or return address"]) }
        guard query["response_type"] == "code", query["code_challenge_method"] == "S256",
              let challenge = query["code_challenge"], challenge.count == 43,
              let state = query["state"], !state.isEmpty,
              let scope = query["scope"], !scope.isEmpty,
              Set(scope.split(separator: " ")).isSubset(of: Set(allScopes.split(separator: " ")))
        else { return json(400, ["code": "bad_request", "message": "a parameter is missing or wrong"]) }
        let issuer = s.issuerOverride ?? s.origin
        var back = URLComponents(string: redirectURI)!
        if let error = s.authorizeError {
            back.queryItems = [URLQueryItem(name: "error", value: error), URLQueryItem(name: "state", value: state),
                               URLQueryItem(name: "iss", value: issuer)]
        } else {
            let code = randomToken()
            s.codes[code] = AuthCode(clientID: client, challenge: challenge, scope: scope)
            back.queryItems = [URLQueryItem(name: "code", value: code), URLQueryItem(name: "state", value: state),
                               URLQueryItem(name: "iss", value: issuer)]
        }
        return Answer(status: 303, headers: ["Location": back.string ?? redirectURI], body: Data())
    }

    /// `grant_type=authorization_code`, with PKCE's S256 check.
    private static func exchange(_ s: inout State, _ form: [String: String]) -> Answer {
        guard let code = form["code"], let waiting = s.codes[code], !waiting.used,
              form["client_id"] == waiting.clientID, form["redirect_uri"].map({ $0 == redirectURI }) ?? true,
              let verifier = form["code_verifier"], challenge(verifier) == waiting.challenge
        else { return oauthError("invalid_grant", "the code doesn't work") }
        s.codes[code]?.used = true
        let grant = s.nextGrant
        s.nextGrant += 1
        let tokens = issue(&s, profile: 2, scope: waiting.scope, grant: grant)
        return tokenAnswer(s, tokens, scope: waiting.scope)
    }

    /// `POST /api/oauth/device`: a code for a TV. The Mac's app doesn't sign in this way.
    private static func device(_ s: inout State, _ form: [String: String]) -> Answer {
        guard let client = form["client_id"], appleClients.contains(client) else {
            return oauthError("invalid_client", "unknown app")
        }
        guard !client.hasSuffix("/macos.json") else {
            return oauthError("unauthorized_client", "this app can't sign in with a code")
        }
        let scope = form["scope"] ?? "library"
        let deviceCode = randomToken()
        s.devices[deviceCode] = Device(clientID: client, scope: scope)
        return json(200, ["device_code": deviceCode, "user_code": "BCDFGHJK",
                          "verification_uri": "\(s.origin)/link",
                          "verification_uri_complete": "\(s.origin)/link?code=BCDFGHJK",
                          "expires_in": 600, "interval": 5],
                    headers: ["Cache-Control": "no-store"])
    }

    /// `grant_type=…:device_code`: pending until `approveDevice()` or `denyDevice()`.
    private static func pollDevice(_ s: inout State, _ form: [String: String]) -> Answer {
        guard let code = form["device_code"], let device = s.devices[code], !device.answered,
              form["client_id"] == device.clientID
        else { return oauthError("invalid_grant", "the code doesn't work") }
        if s.slowDowns > 0 {
            s.slowDowns -= 1
            return oauthError("slow_down", "ask less often")
        }
        if s.deviceExpired {
            s.devices[code]?.answered = true
            return oauthError("expired_token", "the code ran out")
        }
        if s.deviceDenied {
            s.devices[code]?.answered = true
            return oauthError("access_denied", "the person refused")
        }
        guard s.deviceApproved else { return oauthError("authorization_pending", "nobody has answered yet") }
        s.devices[code]?.answered = true
        let grant = s.nextGrant
        s.nextGrant += 1
        let tokens = issue(&s, profile: 2, scope: device.scope, grant: grant)
        return tokenAnswer(s, tokens, scope: device.scope)
    }

    /// Pley's switch-profile grant, with the server's reasons. A refusal spends nothing.
    private static func switchProfile(_ s: inout State, _ form: [String: String]) -> Answer {
        guard let presented = form["refresh_token"], let refresh = s.refreshTokens[presented],
              !s.endedGrants.contains(refresh.grant)
        else { return oauthError("invalid_grant", "the sign-in has ended") }
        if refresh.used {
            s.endedGrants.insert(refresh.grant)
            return oauthError("invalid_grant", "the sign-in has ended")
        }
        guard refresh.scope.split(separator: " ").contains("profiles") else {
            return oauthError("unauthorized_client", "this app can't switch profiles")
        }
        guard let id = Int(form["profile"] ?? "") else { return oauthError("invalid_request", "profile is missing") }
        guard let profile = household.first(where: { $0.id == id }) else {
            return switchRefused("not_allowed", "you can't switch to this profile")
        }
        let pin = form["pin"]
        if id == refresh.profile && (pin == nil || profile.pin == nil) {
            return oauthError("invalid_request", "this is the profile in use")
        }
        switch profile.needs {
        case "locked": return switchRefused("not_allowed", "you can't switch to this profile")
        case "password": return switchRefused("browser", "this profile needs its password")
        default: break
        }
        if let expected = profile.pin {
            if s.lockedPins.contains(id) { return switchRefused("pin_locked", "this PIN is locked") }
            if s.wrongPins[id, default: 0] >= 5 { return switchRefused("throttled", "too many wrong PINs") }
            guard pin == expected else {
                s.wrongPins[id, default: 0] += 1
                return switchRefused("wrong_pin", "wrong PIN")
            }
        }
        // The old tokens stop at once; the grant goes on as the new profile.
        s.refreshTokens[presented]?.used = true
        for (token, access) in s.accessTokens where access.grant == refresh.grant { s.accessTokens[token]?.valid = false }
        let tokens = issue(&s, profile: id, scope: refresh.scope, grant: refresh.grant)
        return tokenAnswer(s, tokens, scope: refresh.scope)
    }

    private static func switchRefused(_ reason: String, _ description: String) -> Answer {
        json(400, ["error": "invalid_grant", "error_description": description, "reason": reason],
             headers: ["Cache-Control": "no-store"])
    }

    private static func challenge(_ verifier: String) -> String {
        Data(SHA256.hash(data: Data(verifier.utf8))).base64EncodedString()
            .replacingOccurrences(of: "+", with: "-").replacingOccurrences(of: "/", with: "_")
            .replacingOccurrences(of: "=", with: "")
    }

    private static func randomToken() -> String {
        (0..<4).map { _ in String(UInt64.random(in: 0...UInt64.max), radix: 16) }.joined()
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
