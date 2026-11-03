import Foundation

/// The administrator's sign-in failed.
public struct AdminPageError: Error, CustomStringConvertible {
    public let description: String
}

/// What the end-to-end checks (`pley-apple-check`, `pley-apple-play`) do
/// as the administrator, the way the web app does it: a browser's cookie,
/// its `Origin`, and `Sec-Fetch-Site`. The cookie is never printed.
public struct AdminPage: Sendable {
    public let origin: String
    let session: URLSession
    var cookie = ""
    /// The User-Agent the page sends.
    public var userAgent = "pley-apple-check"

    public init(origin: String) {
        self.origin = origin
        let configuration = URLSessionConfiguration.ephemeral
        configuration.httpCookieStorage = nil
        configuration.httpShouldSetCookies = false
        configuration.urlCache = nil
        session = URLSession(configuration: configuration)
    }

    /// `POST /api/auth/login` without `Origin`, so the token comes in the body.
    public mutating func login(username: String, password: String) async throws {
        let (status, body) = try await send("POST", "/api/auth/login",
                                            json: ["username": username, "password": password], asPage: false)
        guard status == 200, let token = body["token"] as? String, !token.isEmpty else {
            throw AdminPageError(description: "the administrator couldn't sign in (HTTP \(status))")
        }
        cookie = "pley_token=\(token)"
    }

    public func send(_ method: String, _ path: String, json: [String: Any]? = nil,
              asPage: Bool = true) async throws -> (Int, [String: Any]) {
        let (status, data) = try await raw(method, path, json: json, asPage: asPage)
        let object = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any] ?? [:]
        return (status, object)
    }

    public func raw(_ method: String, _ path: String, json: [String: Any]? = nil, asPage: Bool = true,
             bearer: String? = nil) async throws -> (Int, Data) {
        var request = URLRequest(url: URL(string: origin + path)!)
        request.httpMethod = method
        request.httpShouldHandleCookies = false
        request.setValue("application/json", forHTTPHeaderField: "Accept")
        request.setValue(userAgent, forHTTPHeaderField: "User-Agent")
        if let json {
            request.httpBody = try JSONSerialization.data(withJSONObject: json)
            request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        }
        if asPage && !cookie.isEmpty {
            request.setValue(cookie, forHTTPHeaderField: "Cookie")
            request.setValue(origin, forHTTPHeaderField: "Origin")
            request.setValue("same-origin", forHTTPHeaderField: "Sec-Fetch-Site")
        }
        if let bearer { request.setValue("Bearer \(bearer)", forHTTPHeaderField: "Authorization") }
        let (data, response) = try await session.data(for: request)
        return ((response as? HTTPURLResponse)?.statusCode ?? 0, data)
    }
}
