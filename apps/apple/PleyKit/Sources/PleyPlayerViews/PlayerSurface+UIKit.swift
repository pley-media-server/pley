#if os(iOS) || os(tvOS)
import AVKit
import PleyPlayer
import PleySubtitles
import SwiftUI
import UIKit

/// The system's player, `AVPlayerViewController`, showing the controller's
/// AVPlayer with AVKit's own controls. Its item carries the title (and an
/// episode's label) for the system's info. The subtitles are drawn over
/// the picture, in its content overlay (`SubtitleView`), placed in the
/// video's bounds (on Apple TV, the picture's size fitted to the screen).
///
/// - iPhone and iPad: a tap on the video is told (`onTap`), so the app's
///   own buttons over it show with the system's controls. Picture in
///   Picture is off.
/// - Apple TV: AVKit applies the content's frame rate and dynamic range to
///   the TV when the person turned on Match Content
///   (`appliesPreferredDisplayCriteriaAutomatically`, from the master
///   playlist's `FRAME-RATE` and `VIDEO-RANGE`). The Quality, Audio and
///   Subtitles menus (with the subtitle sync) sit in the transport bar, and
///   Details is a tab of the info panel.
///   Skip Intro, Skip Credits and Up Next ("Play Next (8)", Cancel) are the
///   player's contextual actions, and the chapters its navigation markers,
///   with their pictures.
public struct PlayerSurface: UIViewControllerRepresentable {
    let player: AVPlayer?
    let state: PlayerState
    let onTap: () -> Void
    let onQuality: @MainActor (String) -> Void
    /// Apple TV: what the transport bar's menus and the contextual actions act on.
    let controller: PlaybackController?

    public init(player: AVPlayer?, state: PlayerState, onTap: @escaping () -> Void = {},
                onQuality: @escaping @MainActor (String) -> Void = { _ in }, controller: PlaybackController? = nil) {
        self.player = player
        self.state = state
        self.onTap = onTap
        self.onQuality = onQuality
        self.controller = controller
    }

    public func makeCoordinator() -> Coordinator { Coordinator() }

    public func makeUIViewController(context: Context) -> AVPlayerViewController {
        let controller = AVPlayerViewController()
        controller.showsPlaybackControls = true
        #if os(iOS)
        controller.allowsPictureInPicturePlayback = false
        controller.canStartPictureInPictureAutomaticallyFromInline = false
        let tap = UITapGestureRecognizer(target: context.coordinator, action: #selector(Coordinator.tapped))
        tap.cancelsTouchesInView = false
        tap.delegate = context.coordinator
        controller.view.addGestureRecognizer(tap)
        #else
        controller.appliesPreferredDisplayCriteriaAutomatically = true
        let details = UIHostingController(rootView: AnyView(PlayerDetailsView(state: state).padding(40)))
        details.title = "Details"
        details.preferredContentSize = CGSize(width: 0, height: 320)
        context.coordinator.details = details
        controller.customInfoViewControllers = [details]
        #endif
        if let overlay = controller.contentOverlayView {
            let subtitles = SubtitleView(frame: overlay.bounds)
            overlay.addSubview(subtitles)
            context.coordinator.follow(controller, subtitles: subtitles)
        }
        return controller
    }

    public func updateUIViewController(_ controller: AVPlayerViewController, context: Context) {
        let coordinator = context.coordinator
        coordinator.onTap = onTap
        coordinator.title = state.title
        coordinator.subtitle = state.subtitle
        if controller.player !== player {
            controller.player = player
            coordinator.watch(player)
        }
        coordinator.describeItem()
        coordinator.subtitles?.shown = state.cues
        #if os(tvOS)
        // Apple TV's player doesn't say where the video shows: the picture, fitted to the screen.
        coordinator.subtitles?.aspect = player?.currentItem?.presentationSize ?? .zero
        #endif
        #if os(tvOS)
        coordinator.details?.rootView = AnyView(PlayerDetailsView(state: state).padding(40))
        let playback = self.controller
        let subtitleMenu = SubtitleMenuKey(state)
        if coordinator.qualities != state.qualities || coordinator.audio != state.audio
            || coordinator.subtitleMenu != subtitleMenu {
            coordinator.qualities = state.qualities
            coordinator.audio = state.audio
            coordinator.subtitleMenu = subtitleMenu
            var actions = PlayerMenus.Actions(playback)
            actions.quality = onQuality
            var menus = [PlayerMenus.quality(state.qualities, actions)]
            if state.audio.count > 1 {
                menus.append(PlayerMenus.audio(state.audio, actions))
            }
            if !state.subtitles.isEmpty { menus.append(PlayerMenus.subtitles(state, actions)) }
            controller.transportBarCustomMenuItems = menus
        }
        let actions = Self.contextualActions(state, playback)
        if coordinator.actions != actions.key {
            coordinator.actions = actions.key
            controller.contextualActions = actions.list
        }
        coordinator.chapters = state.chapters
        coordinator.durationMs = state.durationMs
        coordinator.playback = playback
        coordinator.markChapters()
        #endif
    }

    public static func dismantleUIViewController(_ controller: AVPlayerViewController, coordinator: Coordinator) {
        coordinator.watch(nil)
        controller.player = nil
    }

    #if os(tvOS)
    /// What the Subtitles menu shows, so it is made again only when that changes.
    struct SubtitleMenuKey: Equatable {
        var choices: [MenuChoice] = []
        var offsetMs = 0
        var canSync = false

        init() {}

        init(_ state: PlayerState) {
            choices = state.subtitles
            offsetMs = state.subtitleOffsetMs
            canSync = state.canSyncSubtitles
        }
    }

    /// Skip Intro or Skip Credits, or Up Next's Play Next and Cancel; and
    /// what they are, so they are made again only when that changes.
    @MainActor
    static func contextualActions(_ state: PlayerState,
                                  _ playback: PlaybackController?) -> (key: String, list: [UIAction]) {
        if let card = state.upNext {
            return ("next \(card.itemId) \(card.playLabel)", [
                UIAction(title: card.playLabel, image: UIImage(systemName: "forward.end")) { _ in playback?.playNext() },
                UIAction(title: "Cancel") { _ in playback?.cancelUpNext() },
            ])
        }
        if let label = state.skipLabel {
            return ("skip \(label)", [UIAction(title: label) { _ in playback?.skip() }])
        }
        return ("", [])
    }
    #endif

    public final class Coordinator: NSObject, UIGestureRecognizerDelegate {
        var onTap: () -> Void = {}
        var title = ""
        var subtitle: String?
        #if os(tvOS)
        var details: UIHostingController<AnyView>?
        var qualities: [MenuChoice] = []
        var audio: [MenuChoice] = []
        var subtitleMenu = SubtitleMenuKey()
        var actions = ""
        var chapters: [ChapterChoice] = []
        var durationMs = 0
        weak var playback: PlaybackController?
        /// The item the chapter markers were given to, and for which chapters.
        private weak var marked: AVPlayerItem?
        private var markedFor: [ChapterChoice] = []
        private var pictures: [Int: Data] = [:]
        private var picturesTask: Task<Void, Never>?
        #endif
        /// The subtitles over the video, and the watch on where the video shows (iPhone and iPad).
        private(set) var subtitles: SubtitleView?
        private var boundsWatch: NSKeyValueObservation?
        private weak var player: AVPlayer?
        private var itemWatch: NSKeyValueObservation?
        private weak var described: AVPlayerItem?
        private var describedAs: (String, String?)?

        @objc func tapped() { onTap() }

        public func gestureRecognizer(_ gestureRecognizer: UIGestureRecognizer,
                                      shouldRecognizeSimultaneouslyWith other: UIGestureRecognizer) -> Bool { true }

        /// Keeps `subtitles` on the video: in its bounds, as the view and the video change size.
        @MainActor
        func follow(_ controller: AVPlayerViewController, subtitles: SubtitleView) {
            self.subtitles = subtitles
            #if os(iOS)
            boundsWatch = controller.observe(\.videoBounds, options: [.initial, .new]) {
                [weak subtitles, weak controller] _, _ in
                Task { @MainActor in
                    guard let subtitles, let controller, let view = controller.viewIfLoaded else { return }
                    subtitles.video = subtitles.convert(controller.videoBounds, from: view)
                }
            }
            #endif
        }

        /// Follows the player's item, so each new one (a new session) is described too.
        @MainActor
        func watch(_ player: AVPlayer?) {
            itemWatch?.invalidate()
            itemWatch = nil
            self.player = player
            guard let player else { return }
            itemWatch = player.observe(\.currentItem, options: [.new]) { [weak self] _, _ in
                Task { @MainActor in
                    self?.describeItem()
                    #if os(tvOS)
                    self?.markChapters()
                    #endif
                }
            }
        }

        #if os(tvOS)
        /// The chapters as the player's navigation markers, on each new item
        /// (a new session) and when they change; their pictures (JPEG, read
        /// with the token) are added as they come, and left out when missing.
        @MainActor
        func markChapters() {
            guard let item = player?.currentItem else { return }
            let titles = chapters.map { ChapterChoice(index: $0.index, title: $0.title, startMs: $0.startMs) }
            if marked === item, markedFor == titles { return }
            if markedFor != titles {
                picturesTask?.cancel()
                pictures = [:]
                if let playback, !titles.isEmpty {
                    let indexes = titles.map(\.index)
                    picturesTask = Task { [weak self] in
                        for index in indexes {
                            guard let data = await playback.chapterPicture(index), !Task.isCancelled else { continue }
                            self?.pictures[index] = data
                        }
                        guard !Task.isCancelled, let self, let item = self.player?.currentItem else { return }
                        item.navigationMarkerGroups = self.markerGroups()
                    }
                }
            }
            marked = item
            markedFor = titles
            item.navigationMarkerGroups = markerGroups()
        }

        private func markerGroups() -> [AVNavigationMarkersGroup] {
            guard !markedFor.isEmpty else { return [] }
            let markers = markedFor.enumerated().map { n, chapter in
                let end = n + 1 < markedFor.count ? markedFor[n + 1].startMs : max(durationMs, chapter.startMs + 1)
                var items = [Self.metadata(.commonIdentifierTitle, chapter.title)]
                if let data = pictures[chapter.index] {
                    let artwork = AVMutableMetadataItem()
                    artwork.identifier = .commonIdentifierArtwork
                    artwork.value = data as NSData
                    artwork.dataType = kCMMetadataBaseDataType_JPEG as String
                    artwork.extendedLanguageTag = "und"
                    items.append(artwork.copy() as! AVMetadataItem)
                }
                let range = CMTimeRange(start: CMTime(value: CMTimeValue(chapter.startMs), timescale: 1000),
                                        end: CMTime(value: CMTimeValue(end), timescale: 1000))
                return AVTimedMetadataGroup(items: items, timeRange: range)
            }
            return [AVNavigationMarkersGroup(title: nil, timedNavigationMarkers: markers)]
        }
        #endif

        /// The title and the episode's label, for the system's info panel.
        @MainActor
        func describeItem() {
            guard let item = player?.currentItem else { return }
            if described === item, let describedAs, describedAs == (title, subtitle) { return }
            described = item
            describedAs = (title, subtitle)
            var metadata = [Self.metadata(.commonIdentifierTitle, title)]
            if let subtitle, !subtitle.isEmpty {
                metadata.append(Self.metadata(.iTunesMetadataTrackSubTitle, subtitle))
            }
            item.externalMetadata = metadata
        }

        private static func metadata(_ identifier: AVMetadataIdentifier, _ value: String) -> AVMetadataItem {
            let item = AVMutableMetadataItem()
            item.identifier = identifier
            item.value = value as NSString
            item.extendedLanguageTag = "und"
            return item.copy() as! AVMetadataItem
        }
    }
}
#endif
