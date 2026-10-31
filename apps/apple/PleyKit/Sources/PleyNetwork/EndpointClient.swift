import Foundation
import PleyCommon
import PleyModel

/// The sign-in endpoints: the metadata, token, revocation and device
/// endpoints. Codes and tokens go to the address the metadata named or
/// nowhere, so no redirect is followed and no cookie is sent; plain http
/// follows Pley's rule.
public struct EndpointClient: Sendable {
    /// The most a form may be (docs/API.md, "Tokens").
    public static let maxForm = 16 << 10

    private let transport: HTTPTransport

    public init(transport: HTTPTransport) {
        self.transport = transport
    }

    /// Posts `form` and returns the answer's body. A non-2xx answer throws
    /// `.oauth` with the server's `{ error, error_description }`, or
    /// `.unreadable` without one; a redirect throws `.redirected`.
    public func postForm(_ url: URL, _ form: [(String, String)]) async throws -> Data {
        let body = Data(Self.encode(form).utf8)
        guard body.count <= Self.maxForm else { throw PleyError.tooLarge }
        var request = URLRequest(url: url)
        request.httpMethod = "POST"
        request.httpBody = body
        request.setValue("application/x-www-form-urlencoded", forHTTPHeaderField: "Content-Type")
        request.setValue("application/json", forHTTPHeaderField: "Accept")
        let response = try await transport.perform(request, redirects: .endpoints, maxBytes: BodyLimit.small)
        if (200..<300).contains(response.status) { return response.body }
        if (300..<400).contains(response.status) { throw PleyError.redirected(location: response.header("Location")) }
        guard let error = try? PleyJSON.decoder.decode(OAuthErrorBody.self, from: response.body), !error.error.isEmpty
        else { throw PleyError.unreadable(status: response.status) }
        throw PleyError.oauth(status: response.status, error)
    }

    /// GETs `url` (the metadata) and returns the body of a 2xx answer. A
    /// redirect throws `.redirected`; any other answer `.unreadable`.
    public func get(_ url: URL) async throws -> Data {
        var request = URLRequest(url: url)
        request.setValue("application/json", forHTTPHeaderField: "Accept")
        let response = try await transport.perform(request, redirects: .endpoints, maxBytes: BodyLimit.small)
        if (200..<300).contains(response.status) { return response.body }
        if (300..<400).contains(response.status) { throw PleyError.redirected(location: response.header("Location")) }
        throw PleyError.unreadable(status: response.status)
    }

    /// `application/x-www-form-urlencoded`, every character but RFC 3986's
    /// unreserved ones percent-encoded.
    public static func encode(_ form: [(String, String)]) -> String {
        form.map { "\(escape($0.0))=\(escape($0.1))" }.joined(separator: "&")
    }

    private static let unreserved = CharacterSet(
        charactersIn: "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~")

    private static func escape(_ text: String) -> String {
        text.addingPercentEncoding(withAllowedCharacters: unreserved) ?? ""
    }
}
