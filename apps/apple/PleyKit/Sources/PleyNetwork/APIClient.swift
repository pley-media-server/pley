import Foundation
import PleyCommon
import PleyModel

/// An answer without a body (`204`), for `send`.
public struct NoContent: Decodable, Sendable, Equatable {
    public init() {}
}

/// A server's API. Paths are always joined to the server's origin, never
/// taken from an answer as an address of their own. The token goes in
/// `Authorization: Bearer`, only to the token's issuer and never in a URL;
/// redirects are followed only within that origin.
///
/// On a `401` with `error="invalid_token"`, the tokens are refreshed once
/// and the request tried again; a second refusal means signed out.
public actor APIClient {
    public nonisolated let origin: String
    private let tokens: (any TokenProvider)?
    private nonisolated let transport: HTTPTransport

    public init(origin: String, tokens: (any TokenProvider)?, transport: HTTPTransport) {
        self.origin = origin
        self.tokens = tokens
        self.transport = transport
    }

    public func get<T: Decodable & Sendable>(_ path: String, query: [URLQueryItem] = [],
                                             maxBytes: Int = BodyLimit.json, as type: T.Type) async throws -> T {
        let response = try await request("GET", path, query: query, body: nil, maxBytes: maxBytes,
                                         accept: "application/json")
        return try decode(type, response)
    }

    public func send<B: Encodable & Sendable, T: Decodable & Sendable>(_ method: String, _ path: String, body: B?,
                                                                    as type: T.Type) async throws -> T {
        let data = try body.map { try PleyJSON.encoder.encode($0) }
        let response = try await request(method, path, query: [], body: data, maxBytes: BodyLimit.json,
                                         accept: "application/json")
        return try decode(type, response)
    }

    /// `get` with the tag of the answer shown (`If-None-Match`): `nil` when
    /// the server says it is unchanged (`304`), else the answer and its
    /// `ETag`.
    public func get<T: Decodable & Sendable>(_ path: String, query: [URLQueryItem] = [], ifNoneMatch tag: String?,
                                             as type: T.Type) async throws -> (value: T, tag: String?)? {
        let response = try await request("GET", path, query: query, body: nil, maxBytes: BodyLimit.json,
                                         accept: "application/json",
                                         headers: tag.map { ["If-None-Match": $0] } ?? [:], notModified: tag != nil)
        if response.status == 304 { return nil }
        return (try decode(type, response), response.header("ETag"))
    }

    /// `GET path` read as it arrives, each piece of the body handed to
    /// `read` until it returns false or the body ends (an event stream).
    /// `opened` is told the answer's status as soon as it comes. With the
    /// token, refreshed once as every request is; an error status throws
    /// as it does.
    public func stream(_ path: String, query: [URLQueryItem] = [], headers: [String: String] = [:],
                       opened: @escaping @Sendable (Int) -> Void = { _ in },
                       read: @escaping @Sendable (Data) -> Bool) async throws {
        try check(try await streamed(path, query: query, headers: headers, opened: opened, read: read))
    }

    /// Bytes of a type in `accept` (`image/png`…) and their type; anything
    /// else is refused.
    public func data(_ path: String, query: [URLQueryItem] = [], maxBytes: Int,
                     accept: Set<String>) async throws -> (Data, String) {
        let response = try await fetch(path, query: query, maxBytes: maxBytes, accept: accept)
        return (response.body, response.contentType ?? "")
    }

    /// `data`'s answer whole, with its headers (a subtitle track says
    /// whether it is complete in one): a type in `accept`, or refused.
    public func fetch(_ path: String, query: [URLQueryItem] = [], maxBytes: Int,
                      accept: Set<String>) async throws -> HTTPResponse {
        let response = try await request("GET", path, query: query, body: nil, maxBytes: maxBytes,
                                         accept: accept.sorted().joined(separator: ", "))
        guard let type = response.contentType, accept.contains(type) else {
            throw PleyError.unreadable(status: response.status)
        }
        return response
    }

    /// `GET path` read as it arrives: each piece of the body's byte count
    /// goes to `read`, until it returns false or the body ends; nothing is
    /// kept (`HTTPTransport.stream`). With the token, refreshed once as
    /// every request is; an error status throws as it does.
    public func bytes(_ path: String, query: [URLQueryItem] = [], headers: [String: String] = [:],
                      read: @escaping @Sendable (Int) -> Bool) async throws {
        try check(try await streamed(path, query: query, headers: headers) { read($0.count) })
    }

    /// What a download wrote (`download`).
    public struct Download: Sendable {
        /// `200` the whole file, `206` the part from the offset asked
        /// for, `416` nothing new past it (the file is as it was).
        public let status: Int
        /// Bytes added to the file by this answer.
        public let bytes: Int
        /// Header names in lower case.
        public let headers: [String: String]

        public func header(_ name: String) -> String? { headers[name.lowercased()] }
    }

    /// `GET path` written to `file` as it arrives, with the token, through
    /// the same plain http rule as every request: of a type in `accept`,
    /// and `maxBytes` at most in all. With `offset`, only what follows
    /// that many bytes of the file is asked for (`Range`): a `206` adds it
    /// to the file, a `200` (a server that sent the whole) replaces it. The
    /// file is left as it was when the download fails.
    public func download(_ path: String, query: [URLQueryItem] = [], to file: URL, maxBytes: Int, from offset: Int = 0,
                         accept: Set<String>) async throws -> Download {
        let part = file.appendingPathExtension("part")
        let fm = FileManager.default
        try? fm.removeItem(at: part)
        guard fm.createFile(atPath: part.path, contents: nil), let handle = try? FileHandle(forWritingTo: part) else {
            throw PleyError.unreachable("the download couldn't be saved")
        }
        let writer = PartWriter(handle: handle, limit: maxBytes - max(offset, 0))
        defer {
            writer.close()
            try? fm.removeItem(at: part)
        }
        var headers = ["Accept": accept.sorted().joined(separator: ", ")]
        if offset > 0 { headers["Range"] = "bytes=\(offset)-" }
        let response = try await streamed(path, query: query, headers: headers, read: writer.write)
        if response.status == 416 && offset > 0 { return Download(status: 416, bytes: 0, headers: response.headers) }
        try check(response)
        let type = response.header("Content-Type")?.split(separator: ";").first
            .map { $0.trimmingCharacters(in: .whitespaces).lowercased() }
        guard let type, accept.contains(type) else { throw PleyError.unreadable(status: response.status) }
        let (written, over, failed) = writer.result
        if over { throw PleyError.tooLarge }
        if failed { throw PleyError.unreachable("the download couldn't be saved") }
        writer.close()
        if response.status == 206 && offset > 0, fm.fileExists(atPath: file.path) {
            let out = try FileHandle(forWritingTo: file)
            defer { try? out.close() }
            try out.seekToEnd()
            try out.write(contentsOf: try Data(contentsOf: part, options: .alwaysMapped))
        } else {
            try? fm.removeItem(at: file)
            try fm.moveItem(at: part, to: file)
        }
        return Download(status: response.status, bytes: written, headers: response.headers)
    }

    /// `GET path` read as it arrives, with the token (refreshed once, as
    /// for every request); the answer, unchecked.
    private func streamed(_ path: String, query: [URLQueryItem], headers: [String: String],
                          opened: (@Sendable (Int) -> Void)? = nil,
                          read: @escaping @Sendable (Data) -> Bool) async throws -> StreamedResponse {
        guard let url = url(path, query: query) else { throw PleyError.unreachable("that isn't an address on the server") }
        var request = URLRequest(url: url)
        for (name, value) in headers { request.setValue(value, forHTTPHeaderField: name) }
        let tokens = self.tokens.flatMap { t in
            Addresses.origin(url).map { $0 == Addresses.origin(t.issuer) } == true ? t : nil
        }
        let issuer = tokens?.issuer ?? origin
        guard let tokens else {
            return try await transport.streamData(request, redirects: .api(issuer: issuer), opened: opened, read: read)
        }
        let sent = try await tokens.accessToken()
        request.setValue("Bearer \(sent)", forHTTPHeaderField: "Authorization")
        // A 401 asks again: the first answer's status isn't the stream's.
        let firstOpened: (@Sendable (Int) -> Void)?
        if let opened {
            firstOpened = { status in if status != 401 { opened(status) } }
        } else {
            firstOpened = nil
        }
        let first = try await transport.streamData(request, redirects: .api(issuer: issuer),
                                                   onLeak: { await tokens.connectionLeaked() },
                                                   opened: firstOpened, read: read)
        guard first.status == 401, first.header("WWW-Authenticate").map(BearerChallenge.isInvalidToken) ?? false else {
            return first
        }
        if await tokens.refusedAfterRefresh(sent) { throw PleyError.signedOut }
        try await tokens.refresh(after: sent)
        let fresh = try await tokens.accessToken()
        request.setValue("Bearer \(fresh)", forHTTPHeaderField: "Authorization")
        let second = try await transport.streamData(request, redirects: .api(issuer: issuer),
                                                    onLeak: { await tokens.connectionLeaked() }, opened: opened,
                                                    read: read)
        if second.status == 401 { throw PleyError.signedOut }
        return second
    }

    /// A player's address on this server (a keyed `/api/media/…`, which
    /// carries its own key), read whole as the player would: no token, and
    /// Pley's plain http rule before and after. For a stream's playlists,
    /// asked ahead of the player.
    public func playerData(_ url: URL, maxBytes: Int) async throws -> Data {
        guard Addresses.origin(url) == Addresses.origin(origin) else {
            throw PleyError.unreachable("that isn't an address on the server")
        }
        var request = URLRequest(url: url)
        request.setValue("*/*", forHTTPHeaderField: "Accept")
        let response = try await transport.perform(request, redirects: .api(issuer: origin), maxBytes: maxBytes)
        try check(response)
        return response.body
    }

    /// `playerData`, read as it arrives and thrown away (a stream's segment, asked ahead of the player).
    public func playerBytes(_ url: URL, read: @escaping @Sendable (Int) -> Bool) async throws {
        guard Addresses.origin(url) == Addresses.origin(origin) else {
            throw PleyError.unreachable("that isn't an address on the server")
        }
        try check(try await transport.stream(URLRequest(url: url), redirects: .api(issuer: origin), read: read))
    }

    /// `path` (with any query of its own, `/api/users/2/avatar?v=1`) on
    /// this server, plus `query`. `nil` for anything but a path on it.
    public nonisolated func url(_ path: String, query: [URLQueryItem] = []) -> URL? {
        guard path.hasPrefix("/"), !path.hasPrefix("//"), !path.contains("\\"), !path.contains("#"),
              !path.unicodeScalars.contains(where: { $0.properties.generalCategory == .control || $0 == " " }),
              var components = URLComponents(string: origin + path),
              Addresses.origin(origin + path) == Addresses.origin(origin)
        else { return nil }
        if !query.isEmpty { components.queryItems = (components.queryItems ?? []) + query }
        return components.url
    }

    /// Before a player opens `url` itself (AVPlayer, without the token):
    /// throws unless it is on this server's origin and, over plain http,
    /// every address its host stands for is private (`ConnectionGate`'s
    /// check before a request). What answers can't be checked after, as
    /// the player makes its own connections.
    public nonisolated func checkPlayerURL(_ url: URL) async throws {
        guard Addresses.origin(url) == Addresses.origin(origin), let host = url.host(percentEncoded: false),
              !host.isEmpty
        else { throw PleyError.unreachable("that isn't an address on the server") }
        if url.scheme?.lowercased() == "http" { try await transport.gate.check(host: host) }
    }

    /// With `notModified`, a `304` is an answer, not an error.
    private func request(_ method: String, _ path: String, query: [URLQueryItem], body: Data?, maxBytes: Int,
                         accept: String, headers: [String: String] = [:],
                         notModified: Bool = false) async throws -> HTTPResponse {
        guard let url = url(path, query: query) else { throw PleyError.unreachable("that isn't an address on the server") }
        var request = URLRequest(url: url)
        request.httpMethod = method
        request.setValue(accept, forHTTPHeaderField: "Accept")
        for (name, value) in headers { request.setValue(value, forHTTPHeaderField: name) }
        let check: (HTTPResponse) throws -> Void = { [self] response in
            if notModified && response.status == 304 { return }
            try self.check(response)
        }
        if let body {
            request.httpBody = body
            request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        }
        // The token only for its own server, compared as origins.
        let tokens = self.tokens.flatMap { t in
            Addresses.origin(url).map { $0 == Addresses.origin(t.issuer) } == true ? t : nil
        }
        let issuer = tokens?.issuer ?? origin
        guard let tokens else {
            let response = try await transport.perform(request, redirects: .api(issuer: issuer), maxBytes: maxBytes)
            try check(response)
            return response
        }

        let sent = try await tokens.accessToken()
        let first = try await send(request, token: sent, tokens: tokens, issuer: issuer, maxBytes: maxBytes)
        guard first.status == 401, invalidToken(first) else {
            try check(first)
            return first
        }
        if await tokens.refusedAfterRefresh(sent) { throw PleyError.signedOut }
        try await tokens.refresh(after: sent)
        let fresh = try await tokens.accessToken()
        let second = try await send(request, token: fresh, tokens: tokens, issuer: issuer, maxBytes: maxBytes)
        if second.status == 401 {
            // A token a refresh just gave refused too: the sign-in is over.
            if invalidToken(second) { _ = await tokens.refusedAfterRefresh(fresh) }
            throw PleyError.signedOut
        }
        try check(second)
        return second
    }

    private func send(_ request: URLRequest, token: String, tokens: any TokenProvider, issuer: String,
                      maxBytes: Int) async throws -> HTTPResponse {
        var request = request
        request.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
        return try await transport.perform(request, redirects: .api(issuer: issuer), maxBytes: maxBytes,
                                           onLeak: { await tokens.connectionLeaked() })
    }

    private func invalidToken(_ response: HTTPResponse) -> Bool {
        response.header("WWW-Authenticate").map(BearerChallenge.isInvalidToken) ?? false
    }

    /// Throws the error a non-2xx answer stands for.
    private func check(_ response: HTTPResponse) throws {
        try check(status: response.status, header: response.header, body: response.body)
    }

    private func check(_ response: StreamedResponse) throws {
        try check(status: response.status, header: response.header, body: response.errorBody)
    }

    private func check(status: Int, header: (String) -> String?, body data: Data) throws {
        if (200..<300).contains(status) { return }
        if (300..<400).contains(status) { throw PleyError.redirected(location: header("Location")) }
        let challenge = header("WWW-Authenticate")
        if status == 403, let challenge, BearerChallenge.isInsufficientScope(challenge) {
            throw PleyError.insufficientScope(scope: BearerChallenge.scope(challenge))
        }
        guard data.count <= BodyLimit.small,
              let body = try? PleyJSON.decoder.decode(APIErrorBody.self, from: data), !body.code.isEmpty
        else { throw PleyError.unreadable(status: status) }
        let message = body.message.map(plain)
        if status == 410 && body.code == "session_stopped" { throw PleyError.sessionStopped(message: message) }
        if status == 403 && body.code == "insufficient_scope" { throw PleyError.insufficientScope(scope: nil) }
        throw PleyError.api(status: status, code: body.code, message: message)
    }

    private func decode<T: Decodable>(_ type: T.Type, _ response: HTTPResponse) throws -> T {
        if type == NoContent.self, let empty = NoContent() as? T { return empty }
        do {
            return try PleyJSON.decoder.decode(type, from: response.body)
        } catch {
            throw PleyError.unreadable(status: response.status)
        }
    }
}

/// A download's bytes into its part file, from the transport's queue.
private final class PartWriter: @unchecked Sendable {
    // Unchecked Sendable: every mutable thing is behind the lock.
    private let lock = NSLock()
    private var handle: FileHandle?
    private let limit: Int
    private var written = 0
    private var over = false
    private var failed = false

    init(handle: FileHandle, limit: Int) {
        self.handle = handle
        self.limit = limit
    }

    /// Writes a piece; false (stop) once past the limit or when the file can't be written.
    func write(_ data: Data) -> Bool {
        lock.withLock {
            guard let handle, !over, !failed else { return false }
            if written + data.count > limit {
                over = true
                return false
            }
            do {
                try handle.write(contentsOf: data)
                written += data.count
                return true
            } catch {
                failed = true
                return false
            }
        }
    }

    var result: (written: Int, over: Bool, failed: Bool) { lock.withLock { (written, over, failed) } }

    func close() {
        lock.withLock {
            try? handle?.close()
            handle = nil
        }
    }
}
