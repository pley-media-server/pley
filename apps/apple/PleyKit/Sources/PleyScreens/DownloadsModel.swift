import Combine
import Foundation
import PleyData
import PleyModel
import PleyPlayer

/// One download in Me → Downloads, every word made (Android's `DownloadsScreen` rows).
public struct DownloadRow: Identifiable, Hashable, Sendable {
    public let entry: DownloadEntry
    /// The movie, or the episode's show.
    public let title: String
    /// An episode's "S1 E2 · Title"; a movie's year.
    public let subtitle: String
    /// Where it is: "1.2 GB", "Downloading · 40% of 1.2 GB", "Waiting for Wi-Fi"…
    public let status: String
    /// How far it is, 0 to 1, while it downloads or the server prepares it; `nil` otherwise.
    public let progress: Double?
    /// "Arrival, 1.2 GB": what VoiceOver reads.
    public let accessibilityLabel: String

    public var id: Int { entry.itemId }
    public var canPlay: Bool { entry.state == .done }
    public var failed: Bool { entry.state == .failed }

    public init(_ entry: DownloadEntry) {
        self.entry = entry
        let episode = entry.kind == .episode
        title = episode ? entry.showTitle ?? entry.title : entry.title
        subtitle = episode
            ? [Words.episodeLabel(season: entry.seasonIndex, episode: entry.episodeIndex), entry.title]
                .filter { !$0.isEmpty }.joined(separator: " · ")
            : entry.year.map(String.init) ?? ""
        status = DownloadWords.status(entry)
        switch entry.state {
        case .downloading, .queued, .waiting:
            progress = entry.converting ?? (entry.sizeBytes > 0 ? entry.fraction : nil)
        default:
            progress = nil
        }
        accessibilityLabel = [title, subtitle, status].filter { !$0.isEmpty }.joined(separator: ", ")
    }
}

/// The downloads' words, the Android app's.
public enum DownloadWords {
    public static let emptyTitle = "Nothing downloaded yet"
    public static let emptyMessage = "Download a movie or episode from its page to watch it offline."
    public static let started = "Downloading. It's in Downloads, under Me."
    public static let download = "Download"
    public static let downloaded = "Downloaded"
    public static let downloadSeason = "Download season"
    public static let play = "Play"
    public static let tryAgain = "Try again"
    public static let delete = "Delete download"
    public static let cancel = "Cancel download"
    public static let goToPage = "Go to the page"
    public static let meEntry = "Downloads"
    public static let meHint = "Watch without your server"
    public static let qualityHint = "Original downloads the file unchanged when this device can play it. "
        + "Lower qualities take less space; the server converts them first."
    public static let mobileData = "Download on mobile data"
    public static let mobileDataHint = "Otherwise downloads wait for Wi-Fi"
    public static let deleteAll = "Delete all downloads"
    public static let deleteAllTitle = "Delete all downloads?"
    public static let deleteAllMessage = "This deletes every download on this device, for all servers and profiles."
    public static let deleteAllConfirm = "Delete all"
    public static let keep = "Keep them"
    /// The offline banner.
    public static let offline = "Showing what Pley had saved. Reconnecting…"

    /// "1.2 GB on this device · 20 GB free".
    public static func storage(_ storage: DownloadStorage) -> String {
        "\(Words.bytes(Int(storage.usedBytes))) on this device · \(Words.bytes(Int(storage.freeBytes))) free"
    }

    /// Where `entry` is, in words.
    public static func status(_ entry: DownloadEntry) -> String {
        switch entry.state {
        case .done:
            return Words.bytes(Int(entry.sizeBytes))
        case .failed:
            return "Didn't download: \(entry.error ?? "something went wrong")"
        case .waiting:
            return entry.serverAway ? "Waiting for your server" : "Waiting for Wi-Fi"
        case .queued, .downloading:
            if let converting = entry.converting {
                return "Preparing on the server · \(Int((converting * 100).rounded(.down)))%"
            }
            guard entry.state == .downloading, entry.sizeBytes > 0 else { return "Waiting to download" }
            return "Downloading · \(Int((entry.fraction * 100).rounded(.down)))% of \(Words.bytes(Int(entry.sizeBytes)))"
        }
    }
}

/// Me → Downloads and Settings → Downloads: the signed-in profile's
/// downloads, what they take, and the downloads' settings.
@MainActor
public final class DownloadsModel: ObservableObject {
    @Published public private(set) var rows: [DownloadRow] = []
    /// "1.2 GB on this device · 20 GB free".
    @Published public private(set) var storage = ""

    public let app: AppModel
    private var listener: AnyCancellable?

    public init(app: AppModel) {
        self.app = app
        listener = app.downloads.changed.sink { [weak self] in
            // Told before the lists change: read once they have.
            Task { @MainActor in self?.refresh() }
        }
        refresh()
    }

    /// Whether this device downloads (not Apple TV).
    public var available: Bool { app.downloads.available }

    public func play(_ row: DownloadRow) async {
        guard row.canPlay else { return }
        await app.play(PlayRequest(itemId: row.entry.itemId))
    }

    public func remove(_ row: DownloadRow) {
        app.downloads.remove(row.entry.itemId)
    }

    public func retry(_ row: DownloadRow) {
        app.downloads.retry(row.entry.itemId)
    }

    /// Settings → Downloads → Delete all downloads: every server's and profile's.
    public func removeAll() {
        app.downloads.removeAll()
    }

    /// Settings → Downloads → Quality.
    public var quality: DownloadQuality {
        get { app.settings.downloadQuality }
        set {
            objectWillChange.send()
            app.settings.downloadQuality = newValue
        }
    }

    /// Settings → Downloads → Download on mobile data.
    public var mobileData: Bool {
        get { app.settings.downloadOnMobileData }
        set {
            objectWillChange.send()
            app.settings.downloadOnMobileData = newValue
            // A download waiting for Wi-Fi may go now.
            app.downloads.resume()
        }
    }

    private func refresh() {
        let made = app.downloads.entries.map(DownloadRow.init)
        if made != rows { rows = made }
        let words = DownloadWords.storage(app.downloads.storage)
        if words != storage { storage = words }
    }
}
