import AVFoundation
import AVKit
import PleyPlayer
import SwiftUI

/// The live player's picture: an `AVPlayerLayer` showing the engine's
/// AVPlayer, the picture fitted with bars. The app draws its own controls
/// over it (live has no transport, so AVPlayerViewController's would
/// offer what live can't do). `ready` is told once the first frame shows.
///
/// The layer's KVO watch is invalidated in `dismantle…`, before SwiftUI
/// lets the view go.
#if os(macOS)
public struct LiveVideoSurface: NSViewRepresentable {
    let player: AVPlayer?
    let ready: @MainActor (Bool) -> Void

    public init(player: AVPlayer?, ready: @escaping @MainActor (Bool) -> Void = { _ in }) {
        self.player = player
        self.ready = ready
    }

    public func makeNSView(context: Context) -> LiveLayerView {
        let view = LiveLayerView()
        view.show(player, ready: ready)
        return view
    }

    public func updateNSView(_ view: LiveLayerView, context: Context) {
        view.show(player, ready: ready)
    }

    public static func dismantleNSView(_ view: LiveLayerView, coordinator: ()) {
        view.letGo()
    }
}

/// An NSView backed by an `AVPlayerLayer`.
public final class LiveLayerView: NSView {
    private let playerLayer = AVPlayerLayer()
    private var readyWatch: NSKeyValueObservation?

    init() {
        super.init(frame: .zero)
        wantsLayer = true
        layer = playerLayer
        playerLayer.videoGravity = .resizeAspect
        playerLayer.backgroundColor = NSColor.black.cgColor
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("init(coder:) is not used") }

    func show(_ player: AVPlayer?, ready: @escaping @MainActor (Bool) -> Void) {
        guard playerLayer.player !== player else { return }
        readyWatch?.invalidate()
        playerLayer.player = player
        readyWatch = playerLayer.observe(\.isReadyForDisplay, options: [.initial, .new]) { layer, _ in
            let shown = layer.isReadyForDisplay
            Task { @MainActor in ready(shown) }
        }
    }

    func letGo() {
        readyWatch?.invalidate()
        readyWatch = nil
        playerLayer.player = nil
    }
}
#else
public struct LiveVideoSurface: UIViewRepresentable {
    let player: AVPlayer?
    let ready: @MainActor (Bool) -> Void

    public init(player: AVPlayer?, ready: @escaping @MainActor (Bool) -> Void = { _ in }) {
        self.player = player
        self.ready = ready
    }

    public func makeUIView(context: Context) -> LiveLayerView {
        let view = LiveLayerView()
        view.show(player, ready: ready)
        return view
    }

    public func updateUIView(_ view: LiveLayerView, context: Context) {
        view.show(player, ready: ready)
    }

    public static func dismantleUIView(_ view: LiveLayerView, coordinator: ()) {
        view.letGo()
    }
}

/// A UIView whose layer is an `AVPlayerLayer`.
public final class LiveLayerView: UIView {
    override public static var layerClass: AnyClass { AVPlayerLayer.self }
    // Always one: `layerClass` says so.
    private var playerLayer: AVPlayerLayer { layer as! AVPlayerLayer }
    private var readyWatch: NSKeyValueObservation?

    init() {
        super.init(frame: .zero)
        playerLayer.videoGravity = .resizeAspect
        backgroundColor = .black
        // The app's own controls say what plays.
        accessibilityElementsHidden = true
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("init(coder:) is not used") }

    func show(_ player: AVPlayer?, ready: @escaping @MainActor (Bool) -> Void) {
        guard playerLayer.player !== player else { return }
        readyWatch?.invalidate()
        playerLayer.player = player
        readyWatch = playerLayer.observe(\.isReadyForDisplay, options: [.initial, .new]) { layer, _ in
            let shown = layer.isReadyForDisplay
            Task { @MainActor in ready(shown) }
        }
    }

    func letGo() {
        readyWatch?.invalidate()
        readyWatch = nil
        playerLayer.player = nil
    }
}
#endif

#if os(iOS) || os(macOS)
/// AirPlay's button (AVRoutePickerView): a live channel can play on an
/// Apple TV or a speaker, as a video can, and music on a speaker. Over
/// the picture it is white and lists TVs first; in Now Playing it takes
/// the text's colour and lists speakers first.
public struct RoutePicker: View {
    let video: Bool
    let id: String

    public init(video: Bool = true, id: String = "live.airplay") {
        self.video = video
        self.id = id
    }

    public var body: some View {
        RoutePickerView(video: video)
            .frame(width: 44, height: 44)
            .accessibilityLabel("AirPlay")
            .accessibilityIdentifier(id)
    }
}

#if os(iOS)
private struct RoutePickerView: UIViewRepresentable {
    let video: Bool

    func makeUIView(context: Context) -> AVRoutePickerView {
        let view = AVRoutePickerView()
        view.tintColor = video ? .white : .label
        view.activeTintColor = .systemBlue
        view.prioritizesVideoDevices = video
        return view
    }

    func updateUIView(_ view: AVRoutePickerView, context: Context) {}
}
#else
private struct RoutePickerView: NSViewRepresentable {
    let video: Bool

    func makeNSView(context: Context) -> AVRoutePickerView {
        let view = AVRoutePickerView()
        view.isRoutePickerButtonBordered = false
        return view
    }

    func updateNSView(_ view: AVRoutePickerView, context: Context) {}
}
#endif
#endif

extension AVLiveEngine {
    /// The AVPlayer of `engine`, when it is one (a fake in tests isn't).
    public static func player(of engine: (any LiveEngine)?) -> AVPlayer? {
        (engine as? AVLiveEngine)?.player
    }
}
