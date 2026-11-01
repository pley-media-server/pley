import Foundation

/// The test server, as its administrator: what a person does in Pley on the
/// web during the walk (allowing the sign-in code). check-screens.sh starts
/// the server and passes what the tests need in the environment:
///
/// - `PLEY_ORIGIN`: the server's address.
/// - `PLEY_ADMIN_TOKEN`: the administrator's web sign-in.
/// - `PLEY_ADMIN_NAME`, `PLEY_ADMIN_PIN`: the administrator's profile and its PIN.
/// - `PLEY_KIDS_PIN`: the PIN of the managed profile Kids.
/// - `PLEY_SHOTS`: a folder for the screenshots, when set.
enum Admin {
    static var env: [String: String] { ProcessInfo.processInfo.environment }
    static var origin: String { env["PLEY_ORIGIN"] ?? "" }
    static var adminName: String { env["PLEY_ADMIN_NAME"] ?? "Admin" }
    static var adminPin: String { env["PLEY_ADMIN_PIN"] ?? "" }
    static var kidsPin: String { env["PLEY_KIDS_PIN"] ?? "" }
    static var shots: String? { env["PLEY_SHOTS"].flatMap { $0.isEmpty ? nil : $0 } }
    private static var token: String { env["PLEY_ADMIN_TOKEN"] ?? "" }

    /// Whether check-screens.sh set the walk up.
    static var ready: Bool { !origin.isEmpty && !token.isEmpty && !kidsPin.isEmpty && !adminPin.isEmpty }

    /// `method path` with the administrator's sign-in, as the web app sends
    /// it: the status and the JSON answer.
    static func call(_ method: String, _ path: String, body: String? = nil) async -> (Int, Any?) {
        guard let url = URL(string: origin + path) else { return (0, nil) }
        var request = URLRequest(url: url)
        request.httpMethod = method
        request.setValue("pley_token=\(token)", forHTTPHeaderField: "Cookie")
        request.setValue(origin, forHTTPHeaderField: "Origin")
        request.setValue("same-origin", forHTTPHeaderField: "Sec-Fetch-Site")
        request.setValue("application/json", forHTTPHeaderField: "Accept")
        if let body {
            request.httpBody = Data(body.utf8)
            request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        }
        guard let (data, response) = try? await URLSession.shared.data(for: request) else { return (0, nil) }
        return ((response as? HTTPURLResponse)?.statusCode ?? 0, try? JSONSerialization.jsonObject(with: data))
    }

    /// Allows the sign-in code the app shows, as the administrator does on
    /// the web's /link page.
    static func allow(code: String) async -> Bool {
        let (status, answer) = await call("POST", "/api/oauth/device/verify", body: #"{"user_code":"\#(code)"}"#)
        guard status == 200, let id = (answer as? [String: Any])?["id"] else { return false }
        return await call("POST", "/api/oauth/requests/\(id)", body: #"{"allow":true}"#).0 == 200
    }

    /// The libraries' ids by name, in the administrator's order.
    static func libraries() async -> [(name: String, id: Int)] {
        let (_, answer) = await call("GET", "/api/libraries")
        return (answer as? [[String: Any]] ?? []).compactMap { library in
            guard let id = library["id"] as? Int, let name = library["name"] as? String else { return nil }
            return (name, id)
        }
    }

    static func libraryIds() async -> [String: Int] {
        Dictionary(await libraries().map { ($0.name, $0.id) }, uniquingKeysWith: { a, _ in a })
    }

    /// Whether the administrator has watched item `id`.
    static func watched(_ id: Int) async -> Bool? {
        let (_, answer) = await call("GET", "/api/items/\(id)")
        return ((answer as? [String: Any])?["user_state"] as? [String: Any])?["watched"] as? Bool
    }

    /// The ids in the administrator's My List.
    static func watchlist() async -> [Int] {
        let (_, answer) = await call("GET", "/api/watchlist")
        return ((answer as? [String: Any])?["items"] as? [[String: Any]] ?? []).compactMap { $0["id"] as? Int }
    }

    /// The libraries back in the server's order.
    static func resetLibraryOrder() async {
        _ = await call("DELETE", "/api/me/libraries/order")
    }
}
