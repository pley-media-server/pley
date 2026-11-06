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
    public static var emptyTitle: String { localized("Nothing downloaded yet") }
    public static var emptyMessage: String {
        localized("Download a movie or episode from its page to watch it offline.")
    }
    public static var started: String { localized("Downloading. It's in Downloads, under Me.") }
    public static var download: String { localized("Download") }
    public static var downloaded: String { localized("Downloaded") }
    public static var downloadSeason: String { localized("Download season") }
    public static var play: String { localized("Play") }
    public static var tryAgain: String { localized("Try again") }
    public static var delete: String { localized("Delete download") }
    public static var cancel: String { localized("Cancel download") }
    public static var goToPage: String { localized("Go to the page") }
    public static var meEntry: String { localized("Downloads") }
    public static var meHint: String { localized("Watch without your server") }
    public static var qualityHint: String {
        localized("Original downloads the file unchanged when this device can play it. Lower qualities take less space; the server converts them first.")
    }
    public static var mobileData: String { localized("Download on mobile data") }
    public static var mobileDataHint: String { localized("Otherwise downloads wait for Wi-Fi") }
    public static var deleteAll: String { localized("Delete all downloads") }
    public static var deleteAllTitle: String { localized("Delete all downloads?") }
    public static var deleteAllMessage: String {
        localized("This deletes every download on this device, for all servers and profiles.")
    }
    public static var deleteAllConfirm: String { localized("Delete all") }
    public static var keep: String { localized("Keep them") }
    /// The offline banner.
    public static var offline: String { localized("Pley is offline") }
    /// The offline banner's way to the downloads, which play without the server.
    public static var offlineAction: String { localized("Downloads") }

    /// "1.2 GB on this device · 20 GB free".
    public static func storage(_ storage: DownloadStorage) -> String {
        let used = Words.bytes(Int(storage.usedBytes)), free = Words.bytes(Int(storage.freeBytes))
        return localized("\(used) on this device · \(free) free")
    }

    /// Where `entry` is, in words.
    public static func status(_ entry: DownloadEntry) -> String {
        switch entry.state {
        case .done:
            return Words.bytes(Int(entry.sizeBytes))
        case .failed:
            let error = entry.error ?? localized("something went wrong")
            return localized("Didn't download: \(error)")
        case .waiting:
            return entry.serverAway ? localized("Waiting for your server") : localized("Waiting for Wi-Fi")
        case .queued, .downloading:
            if let converting = entry.converting {
                let percent = Int((converting * 100).rounded(.down))
                return localized("Preparing on the server · \(percent)%")
            }
            guard entry.state == .downloading, entry.sizeBytes > 0 else { return localized("Waiting to download") }
            let percent = Int((entry.fraction * 100).rounded(.down)), size = Words.bytes(Int(entry.sizeBytes))
            return localized("Downloading · \(percent)% of \(size)")
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
