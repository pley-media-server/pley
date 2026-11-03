import os

/// The app's loggers, in the unified log under the subsystem `app.pley.apple`.
///
/// What may be logged: a request as its method and path only, never its
/// query, headers or body, and never a code, token, password or PIN.
/// Interpolated values keep the default `.private` privacy, so the stored
/// log shows them as `<private>`; only status codes and paths are marked
/// `.public`, and in the player's log a session's method, the first 8
/// characters of its id and AVFoundation's error domains and codes. A keyed
/// media address (`/api/media/{key}/…`) is never logged: its key stands
/// for the session.
public enum Log {
    public static let network = Logger(subsystem: subsystem, category: "network")
    public static let auth = Logger(subsystem: subsystem, category: "auth")
    public static let discovery = Logger(subsystem: subsystem, category: "discovery")
    public static let data = Logger(subsystem: subsystem, category: "data")
    public static let caps = Logger(subsystem: subsystem, category: "caps")
    public static let player = Logger(subsystem: subsystem, category: "player")
    public static let downloads = Logger(subsystem: subsystem, category: "downloads")

    static let subsystem = "app.pley.apple"

    /// A request's path as it may be logged: a keyed media address keeps
    /// only `/api/media/…`, since its key stands for the session.
    public static func path(_ path: String) -> String {
        path.lowercased().hasPrefix("/api/media/") ? "/api/media/…" : path
    }
}
