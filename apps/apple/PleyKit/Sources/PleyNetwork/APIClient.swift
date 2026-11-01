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

    /// Bytes of a type in `accept` (`image/png`…) and their type; anything
    /// else is refused.
    public func data(_ path: String, query: [URLQueryItem] = [], maxBytes: Int,
                     accept: Set<String>) async throws -> (Data, String) {
        let response = try await request("GET", path, query: query, body: nil, maxBytes: maxBytes,
                                         accept: accept.sorted().joined(separator: ", "))
        guard let type = response.contentType, accept.contains(type) else {
            throw PleyError.unreadable(status: response.status)
        }
        return (response.body, type)
    }

    /// `GET path` read as it arrives: each piece of the body's byte count
    /// goes to `read`, until it returns false or the body ends; nothing is
    /// kept (`HTTPTransport.stream`). With the token, refreshed once as
    /// every request is; an error status throws as it does.
    public func bytes(_ path: String, query: [URLQueryItem] = [], headers: [String: String] = [:],
                      read: @escaping @Sendable (Int) -> Bool) async throws {
        guard let url = url(path, query: query) else { throw PleyError.unreachable("that isn't an address on the server") }
        var request = URLRequest(url: url)
        for (name, value) in headers { request.setValue(value, forHTTPHeaderField: name) }
        let tokens = self.tokens.flatMap { t in
            Addresses.origin(url).map { $0 == Addresses.origin(t.issuer) } == true ? t : nil
        }
        let issuer = tokens?.issuer ?? origin
        guard let tokens else {
            try check(try await transport.stream(request, redirects: .api(issuer: issuer), read: read))
            return
        }
        let sent = try await tokens.accessToken()
        request.setValue("Bearer \(sent)", forHTTPHeaderField: "Authorization")
        let first = try await transport.stream(request, redirects: .api(issuer: issuer),
                                               onLeak: { await tokens.connectionLeaked() }, read: read)
        guard first.status == 401, first.header("WWW-Authenticate").map(BearerChallenge.isInvalidToken) ?? false else {
            return try check(first)
        }
        if await tokens.refusedAfterRefresh(sent) { throw PleyError.signedOut }
        try await tokens.refresh(after: sent)
        let fresh = try await tokens.accessToken()
        request.setValue("Bearer \(fresh)", forHTTPHeaderField: "Authorization")
        let second = try await transport.stream(request, redirects: .api(issuer: issuer),
                                                onLeak: { await tokens.connectionLeaked() }, read: read)
        if second.status == 401 { throw PleyError.signedOut }
        try check(second)
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

    private func request(_ method: String, _ path: String, query: [URLQueryItem], body: Data?, maxBytes: Int,
                         accept: String) async throws -> HTTPResponse {
        guard let url = url(path, query: query) else { throw PleyError.unreachable("that isn't an address on the server") }
        var request = URLRequest(url: url)
        request.httpMethod = method
        request.setValue(accept, forHTTPHeaderField: "Accept")
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
