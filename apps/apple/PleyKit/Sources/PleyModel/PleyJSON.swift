import Foundation

/// How the app reads the server's JSON and writes its own.
///
/// - Keys are snake_case on the wire and camelCase in Swift: `position_ms`
///   is `positionMs`, `poster_url` is `posterUrl`.
/// - Keys this app doesn't know are skipped, so a newer server can add
///   fields.
/// - A field left out, or `null`, reads as that property's default: `nil`,
///   `false`, `0` or an empty list. The server leaves optional fields out
///   of big listings (docs/API.md, "Conventions"), and the types decode
///   them by hand to say so.
public enum PleyJSON {
    /// Set up once and never changed, so every caller may share it.
    public static let decoder: JSONDecoder = {
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        return decoder
    }()

    public static let encoder: JSONEncoder = {
        let encoder = JSONEncoder()
        encoder.keyEncodingStrategy = .convertToSnakeCase
        encoder.outputFormatting = [.sortedKeys]
        return encoder
    }()
}

extension KeyedDecodingContainer {
    /// The value at `key`, or `fallback` when it is missing or `null`.
    func value<T: Decodable>(_ key: Key, or fallback: T) throws -> T {
        try decodeIfPresent(T.self, forKey: key) ?? fallback
    }
}
