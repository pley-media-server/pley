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
///   Picture plays on when the app is left, or from its button
///   (`PictureInPictureKeeper` keeps the controller while the screen is away).
/// - Apple TV: AVKit applies the content's frame rate and dynamic range to
///   the TV when the person turned on Match Content
///   (`appliesPreferredDisplayCriteriaAutomatically`, from the master
///   playlist's `FRAME-RATE` and `VIDEO-RANGE`). The Quality, Audio,
///   Subtitles (with the subtitle sync, the subtitle style while text
///   subtitles show, and Find Subtitles… last), Speed
///   and Sleep Timer menus sit in the transport bar, and Details is a tab
///   of the info panel. Picture in Picture is the transport bar's own
///   button.
///   Skip Intro, Skip Credits, Up Next ("Play Next (8)", Cancel) and the
///   sleep timer's Keep Watching are the player's contextual actions, and
///   the chapters its navigation markers, with their pictures.
public struct PlayerSurface: UIViewControllerRepresentable {
    let player: AVPlayer?
    let state: PlayerState
    let onTap: () -> Void
    let onQuality: @MainActor (String) -> Void
    /// Apple TV: what the transport bar's menus and the contextual actions act on.
    let controller: PlaybackController?
    /// Apple TV: Find Subtitles… in the Subtitles menu opens the search.
    let onFindSubtitles: (@MainActor () -> Void)?

    public init(player: AVPlayer?, state: PlayerState, onTap: @escaping () -> Void = {},
                onQuality: @escaping @MainActor (String) -> Void = { _ in }, controller: PlaybackController? = nil,
                onFindSubtitles: (@MainActor () -> Void)? = nil) {
        self.player = player
        self.state = state
        self.onTap = onTap
        self.onQuality = onQuality
        self.controller = controller
        self.onFindSubtitles = onFindSubtitles
    }

    public func makeCoordinator() -> Coordinator { Coordinator() }

    public func makeUIViewController(context: Context) -> AVPlayerViewController {
        let keeper = PictureInPictureKeeper.shared
        // Back from Picture in Picture: the controller it played in, its subtitles inside.
        let kept = keeper.take(for: player)
        let controller = kept ?? AVPlayerViewController()
        controller.showsPlaybackControls = true
        keeper.adopt(controller)
        // The speed is the Speed menu's (`PlaybackController.setSpeed`): one control.
        controller.speeds = []
        #if os(iOS)
        // The app's Now Playing owns the lock screen and the remote commands
        // (`NowPlayingCenter`), so music, video and live never fight over them.
        // Apple TV's AVKit always shows its own, which tvOS can't turn off.
        controller.updatesNowPlayingInfoCenter = false
        let tap = UITapGestureRecognizer(target: context.coordinator, action: #selector(Coordinator.tapped))
        tap.cancelsTouchesInView = false
        tap.delegate = context.coordinator
        controller.view.addGestureRecognizer(tap)
        context.coordinator.tap = tap
        #else
        controller.appliesPreferredDisplayCriteriaAutomatically = true
        let details = UIHostingController(rootView: AnyView(PlayerDetailsView(state: state).padding(40)))
        details.title = "Details"
        details.preferredContentSize = CGSize(width: 0, height: 320)
        context.coordinator.details = details
        controller.customInfoViewControllers = [details]
        #endif
        if let overlay = controller.contentOverlayView {
            let subtitles = overlay.subviews.lazy.compactMap { $0 as? SubtitleView }.first
                ?? SubtitleView(frame: overlay.bounds)
            if subtitles.superview == nil { overlay.addSubview(subtitles) }
            context.coordinator.follow(controller, subtitles: subtitles)
        }
        if kept != nil {
            context.coordinator.watch(player)
            keeper.shown(controller)
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
        PictureInPictureKeeper.shared.shown(controller)
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
            actions.findSubtitles = onFindSubtitles
            var menus = [PlayerMenus.quality(state.qualities, actions)]
            if state.audio.count > 1 {
                menus.append(PlayerMenus.audio(state.audio, actions))
            }
            if !state.subtitles.isEmpty || PlayerMenus.findSubtitles(state, actions) != nil {
                menus.append(PlayerMenus.subtitles(state, actions))
            }
            menus.append(PlayerMenus.speed(state, actions))
            menus.append(PlayerMenus.sleepTimer(state, actions))
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
        coordinator.stopFollowing()
        // In Picture in Picture: it plays on, kept, until it is restored or closed.
        if PictureInPictureKeeper.shared.keeps(controller) {
            #if os(iOS)
            if let tap = coordinator.tap { controller.view.removeGestureRecognizer(tap) }
            #endif
            return
        }
        controller.player = nil
    }

    #if os(tvOS)
    /// What the Subtitles menu shows, so it is made again only when that changes.
    struct SubtitleMenuKey: Equatable {
        var choices: [MenuChoice] = []
        var offsetMs = 0
        var canSync = false
        var canFind = false
        var speed: Double = 1
        var sleep: [MenuChoice] = []
        var sleepSummary: String?
        var style: [SubtitleStyleMenu] = []

        init() {}

        init(_ state: PlayerState) {
            choices = state.subtitles
            offsetMs = state.subtitleOffsetMs
            canSync = state.canSyncSubtitles
            canFind = state.canFindSubtitles
            speed = state.speed
            sleep = state.sleepChoices
            // Minutes left: the menu's second line follows them, not the notice's seconds.
            sleepSummary = state.sleep.secondsLeft == nil ? state.sleepSummary : "notice"
            style = state.subtitleStyleMenus
        }
    }

    /// The sleep timer's Keep Watching in its last 30 s, Skip Intro or
    /// Skip Credits, or Up Next's Play Next and Cancel; and what they are,
    /// so they are made again only when that changes.
    @MainActor
    static func contextualActions(_ state: PlayerState,
                                  _ playback: PlaybackController?) -> (key: String, list: [UIAction]) {
        if state.sleep.secondsLeft != nil {
            return ("sleep", [UIAction(title: "Keep Watching", image: UIImage(systemName: "moon.zzz")) { _ in
                playback?.keepWatching()
            }])
        }
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
        /// The tap on the video, taken off a controller Picture in Picture keeps.
        weak var tap: UITapGestureRecognizer?
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
                @Sendable [weak subtitles, weak controller] _, _ in
                Task { @MainActor in
                    guard let subtitles, let controller, let view = controller.viewIfLoaded else { return }
                    subtitles.video = subtitles.convert(controller.videoBounds, from: view)
                }
            }
            #endif
        }

        /// Ends the watch on `videoBounds` while the controller still has its
        /// player layer, which AVKit watches for it.
        @MainActor
        func stopFollowing() {
            boundsWatch?.invalidate()
            boundsWatch = nil
        }

        /// Follows the player's item, so each new one (a new session) is described too.
        @MainActor
        func watch(_ player: AVPlayer?) {
            itemWatch?.invalidate()
            itemWatch = nil
            self.player = player
            guard let player else { return }
            itemWatch = player.observe(\.currentItem, options: [.new]) { @Sendable [weak self] _, _ in
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
