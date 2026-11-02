import Foundation

/// Where a server's file for a download stands (docs/API.md, "Downloads").
public struct ConversionState: WireValue {
    public let rawValue: String
    public init(rawValue: String) { self.rawValue = rawValue }

    /// The device plays the original as it is: it downloads that.
    public static let direct = ConversionState(rawValue: "direct")
    public static let queued = ConversionState(rawValue: "queued")
    public static let running = ConversionState(rawValue: "running")
    /// The file is made: `url` downloads it.
    public static let done = ConversionState(rawValue: "done")
    public static let failed = ConversionState(rawValue: "failed")
}

/// The answer of `POST /api/conversions` and `GET /api/conversions/{id}`:
/// the file a download fetches, the original or one the server makes.
public struct DownloadInfo: Codable, Hashable, Sendable {
    /// The conversion; `nil` for the original.
    public var id: String?
    public var state: ConversionState
    /// 0 to 1 (1 for the original).
    public var progress: Double
    /// Bytes: the original's, or the converted file's once done.
    public var size: Int64?
    /// Why it failed; `nil` for apps (the server's log keeps it).
    public var error: String?
    /// A path on the server: the original's stream, or the converted file.
    public var url: String

    public init(id: String? = nil, state: ConversionState, progress: Double = 0, size: Int64? = nil,
                error: String? = nil, url: String) {
        self.id = id
        self.state = state
        self.progress = progress
        self.size = size
        self.error = error
        self.url = url
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decodeIfPresent(String.self, forKey: .id)
        state = try c.value(.state, or: .queued)
        progress = try c.value(.progress, or: 0)
        size = try c.decodeIfPresent(Int64.self, forKey: .size)
        error = try c.decodeIfPresent(String.self, forKey: .error)
        url = try c.value(.url, or: "")
    }
}
