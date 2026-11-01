#if os(iOS) || os(tvOS)
import AVKit
import PleyPlayer
import SwiftUI
import UIKit

/// The system's player, `AVPlayerViewController`, showing the controller's
/// AVPlayer with AVKit's own controls. Its item carries the title (and an
/// episode's label) for the system's info.
///
/// - iPhone and iPad: a tap on the video is told (`onTap`), so the app's
///   own buttons over it show with the system's controls. Picture in
///   Picture is off.
/// - Apple TV: AVKit applies the content's frame rate and dynamic range to
///   the TV when the person turned on Match Content
///   (`appliesPreferredDisplayCriteriaAutomatically`, from the master
///   playlist's `FRAME-RATE` and `VIDEO-RANGE`). The Quality menu sits in
///   the transport bar, and Details is a tab of the info panel.
public struct PlayerSurface: UIViewControllerRepresentable {
    let player: AVPlayer?
    let state: PlayerState
    let onTap: () -> Void
    let onQuality: (String) -> Void

    public init(player: AVPlayer?, state: PlayerState, onTap: @escaping () -> Void = {},
                onQuality: @escaping (String) -> Void = { _ in }) {
        self.player = player
        self.state = state
        self.onTap = onTap
        self.onQuality = onQuality
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
        #if os(tvOS)
        coordinator.details?.rootView = AnyView(PlayerDetailsView(state: state).padding(40))
        if coordinator.qualities != state.qualities {
            coordinator.qualities = state.qualities
            controller.transportBarCustomMenuItems = [Self.qualityMenu(state.qualities, onQuality)]
        }
        #endif
    }

    public static func dismantleUIViewController(_ controller: AVPlayerViewController, coordinator: Coordinator) {
        coordinator.watch(nil)
        controller.player = nil
    }

    #if os(tvOS)
    /// The transport bar's Quality menu: each choice with its second line,
    /// the one playing checked.
    static func qualityMenu(_ choices: [MenuChoice], _ select: @escaping (String) -> Void) -> UIMenu {
        let actions = choices.map { choice in
            let action = UIAction(title: choice.label, state: choice.selected ? .on : .off) { _ in select(choice.id) }
            action.subtitle = choice.detail
            return action
        }
        return UIMenu(title: "Quality", image: UIImage(systemName: "slider.horizontal.3"), children: actions)
    }
    #endif

    public final class Coordinator: NSObject, UIGestureRecognizerDelegate {
        var onTap: () -> Void = {}
        var title = ""
        var subtitle: String?
        #if os(tvOS)
        var details: UIHostingController<AnyView>?
        var qualities: [MenuChoice] = []
        #endif
        private weak var player: AVPlayer?
        private var itemWatch: NSKeyValueObservation?
        private weak var described: AVPlayerItem?
        private var describedAs: (String, String?)?

        @objc func tapped() { onTap() }

        public func gestureRecognizer(_ gestureRecognizer: UIGestureRecognizer,
                                      shouldRecognizeSimultaneouslyWith other: UIGestureRecognizer) -> Bool { true }

        /// Follows the player's item, so each new one (a new session) is described too.
        @MainActor
        func watch(_ player: AVPlayer?) {
            itemWatch?.invalidate()
            itemWatch = nil
            self.player = player
            guard let player else { return }
            itemWatch = player.observe(\.currentItem, options: [.new]) { [weak self] _, _ in
                Task { @MainActor in self?.describeItem() }
            }
        }

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
