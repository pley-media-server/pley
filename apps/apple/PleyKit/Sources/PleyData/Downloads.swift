import Combine
import Foundation
import PleyAuth
import PleyModel
import PleyNetwork

/// A downloaded file on this device, as the player opens it.
public struct LocalFile: Hashable, Sendable {
    /// The video: `file:///…/media.mp4`.
    public var url: URL
    /// The text subtitles saved beside it as WebVTT, by the stream's index.
    public var subtitles: [Int: URL]
    /// Made by the server for this device, rather than the original.
    public var converted: Bool

    public init(url: URL, subtitles: [Int: URL] = [:], converted: Bool = false) {
        self.url = url
        self.subtitles = subtitles
        self.converted = converted
    }
}

/// What the player takes from downloads, for the server and profile
/// signed in: the file to play instead of the server's, the details while
/// the server is out of reach, where it was left, and the next episode.
@MainActor
public protocol LocalMedia: AnyObject {
    /// The download of file `fileId`, whole; `nil` when there is none.
    func file(_ fileId: Int) -> LocalFile?
    /// The details saved with a download of `itemId`.
    func detail(_ itemId: Int) async -> ItemDetail?
    /// Where `itemId` was left, played here: Resume works without the
    /// server. At 90 % or more it counts as watched, as on the server.
    func played(_ itemId: Int, positionMs: Int, durationMs: Int?)
    /// The episode after `itemId` downloaded whole (its show's next by
    /// season and number), with the picture saved beside it.
    func nextAfter(_ itemId: Int) async -> ItemSummary?
}

/// Where a download is (Android's states).
public enum DownloadState: String, Codable, Hashable, Sendable {
    /// Waiting its turn.
    case queued
    case downloading
    /// Waiting for a network it may use, or for the server (`serverAway`).
    case waiting
    /// Stopped by an error: `error` says which; tried again by hand.
    case failed
    /// Whole, and playable without the server.
    case done
}

/// A download of a movie or an episode, for the lists and the item page.
public struct DownloadEntry: Hashable, Sendable, Identifiable {
    public var itemId: Int
    public var fileId: Int
    public var kind: ItemKind
    public var title: String
    public var showId: Int?
    public var showTitle: String?
    public var seasonIndex: Int?
    public var episodeIndex: Int?
    public var year: Int?
    public var durationMs: Int?
    public var sizeBytes: Int64
    public var downloadedBytes: Int64
    public var state: DownloadState
    /// The picture saved with it (a still, else the poster).
    public var image: URL?
    /// When it was asked for: newest first.
    public var addedAt: Date
    public var positionMs: Int
    public var watched: Bool
    /// Why it failed, one short sentence.
    public var error: String?
    /// While the server makes this device's file: how far, 0 to 1.
    public var converting: Double?
    /// Waiting because the server didn't answer, not for Wi-Fi.
    public var serverAway: Bool

    public var id: Int { itemId }

    public init(itemId: Int, fileId: Int, kind: ItemKind, title: String, showId: Int? = nil, showTitle: String? = nil,
                seasonIndex: Int? = nil, episodeIndex: Int? = nil, year: Int? = nil, durationMs: Int? = nil,
                sizeBytes: Int64 = 0, downloadedBytes: Int64 = 0, state: DownloadState, image: URL? = nil,
                addedAt: Date = Date(), positionMs: Int = 0, watched: Bool = false, error: String? = nil,
                converting: Double? = nil, serverAway: Bool = false) {
        self.itemId = itemId
        self.fileId = fileId
        self.kind = kind
        self.title = title
        self.showId = showId
        self.showTitle = showTitle
        self.seasonIndex = seasonIndex
        self.episodeIndex = episodeIndex
        self.year = year
        self.durationMs = durationMs
        self.sizeBytes = sizeBytes
        self.downloadedBytes = downloadedBytes
        self.state = state
        self.image = image
        self.addedAt = addedAt
        self.positionMs = positionMs
        self.watched = watched
        self.error = error
        self.converting = converting
        self.serverAway = serverAway
    }

    /// How much is down, 0 to 1.
    public var fraction: Double {
        sizeBytes > 0 ? min(max(Double(downloadedBytes) / Double(sizeBytes), 0), 1) : 0
    }
}

/// How much the downloads take, and what is free where they go.
public struct DownloadStorage: Hashable, Sendable {
    public var usedBytes: Int64
    public var freeBytes: Int64

    public init(usedBytes: Int64 = 0, freeBytes: Int64 = 0) {
        self.usedBytes = usedBytes
        self.freeBytes = freeBytes
    }
}

/// Settings → Downloads → Quality: the original, or a smaller file the
/// server makes (Android's choices).
public enum DownloadQuality: String, CaseIterable, Hashable, Sendable {
    case original
    case hd1080 = "1080p"
    case hd720 = "720p"
    case sd480 = "480p"

    public var label: String {
        switch self {
        case .original: localized("Original")
        case .hd1080: "1080p · 8 Mbps"
        case .hd720: "720p · 4 Mbps"
        case .sd480: "480p · 1.5 Mbps"
        }
    }

    /// The height and bitrate asked for; `nil` for the original.
    public var target: (maxHeight: Int, maxBitrateKbps: Int)? {
        switch self {
        case .original: nil
        case .hd1080: (1080, 8_000)
        case .hd720: (720, 4_000)
        case .sd480: (480, 1_500)
        }
    }
}

/// An access token for a download's piece: one that lasts `lasting`
/// seconds more, and when it ends. Either returns only once the server has
/// proved itself (docs/API.md, "Telling servers apart").
public protocol DownloadTokens: Sendable {
    func accessToken(lasting: TimeInterval) async throws -> (token: String, expires: Date)
    /// Returns once the server has proved itself as an access token needs:
    /// before a piece goes on from its resume data with the token it began with.
    func proveServer() async throws
    /// A piece's token reached an address that isn't private over plain
    /// http: the sign-in ends on the device.
    func connectionLeaked() async
}

extension Tokens: DownloadTokens {}

/// The server and profile downloads are made for: the one signed in.
public struct DownloadScope: Sendable {
    public var origin: String
    public var profileId: Int
    public var api: PleyAPI
    public var tokens: (any DownloadTokens)?
    /// What this device plays, sent with a conversion's request.
    public var caps: @Sendable () async -> ClientCaps

    public init(origin: String, profileId: Int, api: PleyAPI, tokens: (any DownloadTokens)?,
                caps: @escaping @Sendable () async -> ClientCaps) {
        self.origin = origin
        self.profileId = profileId
        self.api = api
        self.tokens = tokens
        self.caps = caps
    }
}

/// The app's downloads, for the screens: the signed-in profile's list,
/// asking, deleting and trying again. iPhone, iPad and Mac have them;
/// Apple TV has none (`NoDownloads`).
@MainActor
public protocol DownloadsService: AnyObject {
    /// Whether this device downloads at all.
    var available: Bool { get }
    /// The signed-in profile's downloads, newest first.
    var entries: [DownloadEntry] { get }
    var storage: DownloadStorage { get }
    /// Says something changed (`entries`, `storage`).
    var changed: AnyPublisher<Void, Never> { get }
    /// The profile signed in now; `nil` when nobody is.
    func signedIn(_ scope: DownloadScope?)
    /// The signed-in profile's downloads, as the player takes them.
    var local: (any LocalMedia)? { get }
    /// Downloads the movies and episodes `itemIds`, in that order. Throws
    /// what the server does when the items can't be read.
    func download(_ itemIds: [Int]) async throws
    /// Stops and deletes the download of `itemId`.
    func remove(_ itemId: Int)
    func retry(_ itemId: Int)
    /// Deletes every download of every server and profile.
    func removeAll()
    /// Deletes the downloads of the server at `origin` (it was forgotten).
    func forgetServer(_ origin: String)
    /// The app came to the foreground, or a network or the server came back.
    func resume()
}

/// No downloads: Apple TV, and tests that don't need them.
@MainActor
public final class NoDownloads: DownloadsService {
    public init() {}
    public var available: Bool { false }
    public var entries: [DownloadEntry] { [] }
    public var storage: DownloadStorage { DownloadStorage() }
    public var changed: AnyPublisher<Void, Never> { Empty().eraseToAnyPublisher() }
    public func signedIn(_ scope: DownloadScope?) {}
    public var local: (any LocalMedia)? { nil }
    public func download(_ itemIds: [Int]) async throws {}
    public func remove(_ itemId: Int) {}
    public func retry(_ itemId: Int) {}
    public func removeAll() {}
    public func forgetServer(_ origin: String) {}
    public func resume() {}
}
