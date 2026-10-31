import os

/// The app's loggers, in the unified log under the subsystem `app.pley.apple`.
///
/// What may be logged: a request as its method and path only, never its
/// query, headers or body, and never a code, token, password or PIN.
/// Interpolated values keep the default `.private` privacy, so the stored
/// log shows them as `<private>`; only status codes and paths are marked
/// `.public`.
public enum Log {
    public static let network = Logger(subsystem: subsystem, category: "network")
    public static let auth = Logger(subsystem: subsystem, category: "auth")
    public static let discovery = Logger(subsystem: subsystem, category: "discovery")
    public static let data = Logger(subsystem: subsystem, category: "data")

    static let subsystem = "app.pley.apple"
}
