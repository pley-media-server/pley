import Foundation
import PleyModel

/// Skip Intro and Skip Credits, as the web and Android apps do them (a
/// port of the Android app's `Markers`). Nothing is skipped by itself: the
/// button only offers it.
public enum Markers {
    /// The button goes this long before the segment ends.
    public static let tailMs = 1_000
    /// Skip Intro lands this long before the intro ends, so its last moment plays.
    public static let introLeadMs = 750

    /// The markers of the file playing (the item's first file when it isn't known yet).
    public static func forFile(_ item: ItemDetail, fileId: Int?) -> [Marker] {
        guard let id = fileId ?? item.files.first?.id else { return [] }
        return item.markers.filter { $0.fileId == id && ($0.kind == .intro || $0.kind == .credits) }
    }

    /// The marker the playhead is in: from its start until a second before its end.
    public static func active(_ markers: [Marker], positionMs: Int) -> Marker? {
        markers.first { positionMs >= $0.startMs && positionMs < $0.endMs - tailMs }
    }

    /// Where skipping `marker` lands: just before an intro ends (at most
    /// half the intro before), at the end of credits.
    public static func skipTarget(_ marker: Marker) -> Int {
        let lead = marker.kind == .intro ? min(introLeadMs, (marker.endMs - marker.startMs) / 2) : 0
        return marker.endMs - lead
    }

    /// Identifies a segment, to tell entering it again apart from entering another.
    public static func key(_ marker: Marker) -> String { "\(marker.fileId):\(marker.kind.rawValue):\(marker.startMs)" }

    /// The button's words for `marker`.
    public static func label(_ marker: Marker) -> String {
        marker.kind == .intro ? localized("Skip Intro") : localized("Skip Credits")
    }
}

/// Entering credits brings up Up Next, once per segment: seeking back into
/// them later lets them play.
public struct CreditsGate: Sendable {
    private var announced: Set<String> = []

    public init() {}

    /// The credits `marker` the playhead just entered for the first time; `nil` otherwise.
    public mutating func entered(_ marker: Marker?) -> Marker? {
        guard let marker, marker.kind == .credits else { return nil }
        return announced.insert(Markers.key(marker)).inserted ? marker : nil
    }

    /// A new item: every segment may be announced again.
    public mutating func reset() { announced.removeAll() }
}
