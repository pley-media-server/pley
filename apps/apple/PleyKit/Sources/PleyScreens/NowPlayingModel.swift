import Foundation
import PleyModel
import PleyPlayer

/// Now Playing's words and rows, from `AppModel.music`'s state: the mini
/// player (iPhone, iPad and Mac), the Now Playing sheet and window, and
/// Apple TV's Now Playing tab all show these.
public struct NowPlayingModel: Equatable, Sendable {
    /// One entry of Up next: its place in the queue, for `jump(to:)`.
    public struct Row: Identifiable, Hashable, Sendable {
        public var id: Int { position }
        public let position: Int
        public let itemId: Int
        public let title: String
        /// "Artist · Album".
        public let byline: String
        public let duration: String
        /// The track's own picture, else its album's.
        public let artPath: String?
        /// "Next: Second, Test Artist, 0:20".
        public let spoken: String
    }

    public let title: String
    public let artist: String?
    public let album: String?
    public let artistId: Int?
    public let albumId: Int?
    public let artPath: String?
    /// "Artist · Album", as the mini player says it.
    public let byline: String
    public let playing: Bool
    public let loading: Bool
    public let ended: Bool
    /// Why it doesn't play, as a sentence; `nil` while it does.
    public let error: String?
    public let shuffle: Bool
    public let hasNext: Bool
    /// The sleep timer: its choice, and its words beside "Sleep Timer"
    /// ("15 min left", "Pausing in 12 s").
    public let sleep: SleepChoice
    public let sleepSummary: String?
    /// "Pausing in 12 s" in the timer's last 30 s, with Keep listening.
    public let sleepNotice: String?
    public let upNext: [Row]
    /// What VoiceOver reads for the mini player: "Now playing, Song, Artist".
    public let spoken: String

    public init(_ state: MusicState) {
        let track = state.track
        title = track?.title ?? ""
        artist = state.artist
        album = state.album
        artistId = state.artistId
        albumId = state.albumId
        artPath = state.artPath
        byline = [state.artist, state.album].compactMap { $0 }.joined(separator: " · ")
        playing = state.playing
        loading = state.loading
        ended = state.ended
        error = state.error?.text
        shuffle = state.shuffle
        hasNext = state.hasNext
        sleep = state.sleep.choice
        sleepSummary = state.sleep.summary(item: "track")
        sleepNotice = state.sleep.notice
        upNext = state.upNext.map { position, item in
            let byline = [item.grandparentTitle, item.parentTitle].compactMap { $0 }.filter { !$0.isEmpty }
                .joined(separator: " · ")
            let duration = (item.durationMs ?? 0) > 0 ? Words.clock(ms: item.durationMs!) : ""
            let spoken = [item.title, item.grandparentTitle, Words.spokenDuration(ms: item.durationMs)]
                .compactMap { $0 }.filter { !$0.isEmpty }.joined(separator: ", ")
            return Row(position: position, itemId: item.id, title: item.title, byline: byline, duration: duration,
                       artPath: MusicState.artPath(item), spoken: spoken)
        }
        spoken = [localized("Now playing"), title, state.artist].compactMap { $0 }.filter { !$0.isEmpty }
            .joined(separator: ", ") + (state.error.map { ". " + $0.text } ?? "")
    }

    /// "1:05", the elapsed time.
    public static func elapsed(_ ms: Int) -> String { Words.clock(ms: max(ms, 0)) }

    /// "−2:10", the time left.
    public static func left(_ positionMs: Int, _ durationMs: Int) -> String {
        "−" + Words.clock(ms: max(durationMs - positionMs, 0))
    }

    /// "1 minute of 3 minutes", for VoiceOver's scrubber.
    public static func spokenTime(_ positionMs: Int, _ durationMs: Int) -> String {
        let at = positionMs < 1_000 ? localized("0 seconds") : Words.spokenDuration(ms: positionMs)
        let of = Words.spokenDuration(ms: durationMs)
        return of.isEmpty ? at : localized("\(at) of \(of)")
    }
}
