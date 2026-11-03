import Foundation

/// The test server, as its administrator: what a person does in Pley on the
/// web during the walk (allowing the sign-in code). check-screens.sh starts
/// the server and passes what the tests need in the environment:
///
/// - `PLEY_ORIGIN`: the server's address.
/// - `PLEY_ADMIN_NAME`: the administrator's profile.
/// - `PLEY_SECRETS_FILE`: a file only its owner reads, with the
///   administrator's web sign-in (`token`) and the PINs of the
///   administrator (`admin_pin`) and the managed profile Kids (`kids_pin`).
///   They aren't in the environment, which other processes can read.
/// - `PLEY_SHOTS`: a folder for the screenshots, when set.
enum Admin {
    static var env: [String: String] { ProcessInfo.processInfo.environment }
    static var origin: String { env["PLEY_ORIGIN"] ?? "" }
    static var adminName: String { env["PLEY_ADMIN_NAME"] ?? "Admin" }
    static var adminPin: String { secrets["admin_pin"] ?? "" }
    static var kidsPin: String { secrets["kids_pin"] ?? "" }
    static var shots: String? { env["PLEY_SHOTS"].flatMap { $0.isEmpty ? nil : $0 } }
    private static var token: String { secrets["token"] ?? "" }

    /// The secrets file, read once.
    private static let secrets: [String: String] = {
        guard let path = ProcessInfo.processInfo.environment["PLEY_SECRETS_FILE"], !path.isEmpty,
              let data = FileManager.default.contents(atPath: path),
              let object = try? JSONSerialization.jsonObject(with: data) as? [String: String]
        else { return [:] }
        return object
    }()

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

    /// The administrator's subtitle mode, as `GET /api/auth/me` says it.
    static func subtitleMode() async -> String? {
        (await call("GET", "/api/auth/me").1 as? [String: Any])?["subtitle_mode"] as? String
    }

    /// The administrator's track defaults back to the server's: the file's
    /// audio, forced subtitles in the audio's language.
    static func resetTrackDefaults() async {
        _ = await call("PATCH", "/api/me",
                       body: #"{"audio_language":null,"subtitle_language":null,"subtitle_mode":"forced"}"#)
    }

    /// The libraries back in the server's order.
    static func resetLibraryOrder() async {
        _ = await call("DELETE", "/api/me/libraries/order")
    }

    /// The administrator's playlists: their JSON.
    static func playlists() async -> [[String: Any]] {
        ((await call("GET", "/api/playlists").1 as? [String: Any])?["playlists"] as? [[String: Any]]) ?? []
    }

    /// The administrator's playlist called `title`, if there is one.
    static func playlist(titled title: String) async -> [String: Any]? {
        await playlists().first { $0["title"] as? String == title }
    }

    /// Playlist `id`'s entries in order: each entry's id and its item's.
    static func entries(_ id: Int) async -> [(entry: Int, item: Int)] {
        let entries = (await call("GET", "/api/playlists/\(id)").1 as? [String: Any])?["entries"] as? [[String: Any]]
        return (entries ?? []).compactMap { entry in
            guard let id = entry["entry_id"] as? Int, let item = (entry["item"] as? [String: Any])?["id"] as? Int
            else { return nil }
            return (id, item)
        }
    }

    /// The ids of the administrator's signed-in Apple apps
    /// (`GET /api/users/{id}/apps`), to see a sign-out end its grant.
    static func appleGrants() async -> Set<Int> {
        guard let me = (await call("GET", "/api/auth/me").1 as? [String: Any])?["id"] as? Int else { return [] }
        let (_, answer) = await call("GET", "/api/users/\(me)/apps")
        return Set((answer as? [[String: Any]] ?? []).compactMap { grant in
            guard let id = grant["id"] as? Int, let client = grant["client_id"] as? String,
                  client.hasPrefix("https://pley-media-server.github.io/pley/oauth/") else { return nil }
            let apple = ["ios", "tvos", "macos"].contains { client.hasSuffix("/\($0).json") }
            return apple ? id : nil
        })
    }

    /// Deletes the administrator's playlists called any of `titles`, left
    /// by the walk on another device or an earlier run.
    static func deletePlaylists(titled titles: Set<String>) async {
        for playlist in await playlists() {
            if let title = playlist["title"] as? String, titles.contains(title), let id = playlist["id"] as? Int {
                _ = await call("DELETE", "/api/playlists/\(id)")
            }
        }
    }
}
