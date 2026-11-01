import Foundation

/// What someone asked to play: an item, maybe a version and a place to
/// start. The Mac's player window is opened with one.
public struct PlayRequest: Codable, Hashable, Sendable {
    public var itemId: Int
    /// The version (file) to play; `nil` lets the app pick the best one for this device.
    public var fileId: Int?
    /// Where to start; `nil` resumes where the profile left off, unless it was watched.
    public var startMs: Int?
    /// Play from the start, whatever the resume point.
    public var fromStart: Bool

    public init(itemId: Int, fileId: Int? = nil, startMs: Int? = nil, fromStart: Bool = false) {
        self.itemId = itemId
        self.fileId = fileId
        self.startMs = startMs
        self.fromStart = fromStart
    }
}
