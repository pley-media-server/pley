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
    /// Credits ending this close to the end of their file run to the end, as
    /// the server counts them: only a studio's logo, a channel's promo or
    /// black is that short.
    public static let creditsTailMs = 15_000

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

    /// Whether `marker` is credits that run to the end of their file
    /// (`durationMs`, 0 when not known). Only those bring up Up Next: its
    /// countdown would skip what follows other credits (an after-credits
    /// scene, the last minutes of a show whose music every episode shares),
    /// so they only offer Skip Credits and Up Next waits for the end.
    public static func reachesEnd(_ marker: Marker, durationMs: Int) -> Bool {
        marker.kind == .credits && durationMs > 0 && durationMs - marker.endMs <= creditsTailMs
    }

    /// Identifies a segment, to tell entering it again apart from entering another.
    public static func key(_ marker: Marker) -> String { "\(marker.fileId):\(marker.kind.rawValue):\(marker.startMs)" }

    /// The button's words for `marker`.
    public static func label(_ marker: Marker) -> String {
        marker.kind == .intro ? localized("Skip Intro") : localized("Skip Credits")
    }
}

/// When Skip shows. It shows by itself for its first 8 seconds in a
/// segment, on the clock from when it appears; after that, until a second
/// before the segment ends, only with the player's controls. Entering a
/// segment again, or another one, starts a fresh 8 seconds; seeking inside
/// the same one doesn't. While the Up Next card covers it, Skip hasn't
/// appeared: after Cancel, its 8 seconds start then.
public struct SkipWindow: Sendable {
    /// How long Skip shows by itself.
    public static let aloneMs: Int64 = 8_000

    private var key: String?
    private var since: Int64 = 0

    public init() {}

    /// The segment whose Skip is offered at `now` (`Markers.key`; `nil`
    /// when none is, or the Up Next card covers it). Another key than the
    /// last starts its window.
    public mutating func offered(_ key: String?, at now: Int64) {
        guard key != self.key else { return }
        self.key = key
        since = now
    }

    /// Whether Skip shows by itself at `now`, without the controls.
    public func alone(at now: Int64) -> Bool {
        key != nil && now - since < Self.aloneMs
    }

    /// A new item, an end or a failure: nothing is offered.
    public mutating func reset() { key = nil }
}

/// Entering credits brings up Up Next, once per segment: seeking back into
/// them later lets them play. The player passes only credits that run to
/// the end (`Markers.reachesEnd`).
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
