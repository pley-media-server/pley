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

    /// Item `id` as the administrator sees it: its JSON.
    static func item(_ id: Int) async -> [String: Any] {
        await call("GET", "/api/items/\(id)").1 as? [String: Any] ?? [:]
    }

    /// The top-level titles of library `id`, by title.
    static func titles(library id: Int) async -> [String: Int] {
        let (_, answer) = await call("GET", "/api/libraries/\(id)/items?limit=200")
        let items = (answer as? [String: Any])?["items"] as? [[String: Any]] ?? []
        return Dictionary(items.compactMap { item -> (String, Int)? in
            guard let id = item["id"] as? Int, let title = item["title"] as? String else { return nil }
            return (title, id)
        }, uniquingKeysWith: { a, _ in a })
    }

    /// Gives items `ids` the genre `genre`, as an administrator's edit does,
    /// so they are More Like This for each other.
    static func genre(_ genre: String, _ ids: [Int]) async {
        for id in ids {
            _ = await call("PATCH", "/api/items/\(id)", body: #"{"genres":["\#(genre)"]}"#)
        }
    }

    /// The first file of item `id`: the audio track it starts with, and
    /// whether the administrator picked it.
    static func audio(_ id: Int) async -> (selected: Int?, picked: Bool, french: Int?) {
        let file = (await item(id)["files"] as? [[String: Any]])?.first ?? [:]
        let streams = file["streams"] as? [[String: Any]] ?? []
        let french = streams.first { $0["kind"] as? String == "audio" && $0["language"] as? String == "fre" }
        return (file["selected_audio"] as? Int, file["audio_picked"] as? Bool ?? false, french?["index"] as? Int)
    }

    /// Item `id` as it was before the walk on another device: Automatic
    /// audio and no rating.
    static func resetChoices(_ id: Int) async {
        _ = await call("PUT", "/api/items/\(id)/streams", body: #"{"audio_index":null}"#)
        _ = await call("PUT", "/api/items/\(id)/rating", body: #"{"rating":null}"#)
    }

    /// The administrator's own rating of item `id`, 1 to 10.
    static func rating(_ id: Int) async -> Double? {
        (await item(id)["user_state"] as? [String: Any])?["rating"] as? Double
    }

    /// A title of library `id` with cast (from a metadata source), if any.
    static func titleWithCast(library id: Int) async -> String? {
        for (title, item) in await titles(library: id).sorted(by: { $0.key < $1.key }) {
            if !((await self.item(item)["cast"] as? [Any]) ?? []).isEmpty { return title }
        }
        return nil
    }

    /// The libraries back in the server's order.
    static func resetLibraryOrder() async {
        _ = await call("DELETE", "/api/me/libraries/order")
    }
}
