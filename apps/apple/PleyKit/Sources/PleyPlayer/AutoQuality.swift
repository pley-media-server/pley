import Foundation

/// Why Automatic plays below the original.
public enum LoweredFor: Sendable, Equatable {
    /// The video didn't arrive fast enough.
    case network
    /// The device dropped frames: it fell behind decoding.
    case device
}

/// Where Automatic's start comes from (docs/QUALITY.md, "Automatic").
public enum StartCeiling: Sendable, Equatable {
    /// The server couldn't say (out of reach, too slow, or older): the device's own memory.
    case device
    /// The server's word, which takes the place of the device's memory: at most `kbps`, `nil` for no limit.
    case server(kbps: Int?)
}

/// What Automatic plays a file at: the file's offered qualities
/// (`Qualities.forFile`), what the original plays at (`originalKbps`: the
/// file's bitrate, or the server's quality when it converts the original),
/// its lines and the server's own quality for it (`cap`: the highest
/// Automatic ever goes below the original).
public struct AutoFile: Sendable, Equatable {
    public var offered: [QualityPreset]
    public var originalKbps: Int?
    public var lines: Int?
    public var cap: QualityPreset

    public init(offered: [QualityPreset], originalKbps: Int?, lines: Int?, cap: QualityPreset) {
        self.offered = offered
        self.originalKbps = originalKbps
        self.lines = lines
        self.cap = cap
    }
}

/// One look at the engine, every half second.
public struct AutoSample: Sendable, Equatable {
    public var atMs: Int64
    /// The video is meant to play and shows: not paused, not ended, not
    /// waiting for a server out of reach, not in the background.
    public var watching: Bool
    /// The engine waits for data.
    public var buffering: Bool
    public var positionMs: Int
    /// How far the media is loaded.
    public var bufferedMs: Int
    /// 0 while unknown.
    public var durationMs: Int
    /// The network's speed as the engine measured it from what it downloaded; `nil` when it can't tell.
    public var bandwidthKbps: Int?
    /// The stream comes in several sizes AVPlayer moves between as the
    /// network allows (`PlaybackInfo.adaptive`): the network's play issues
    /// are its own, and only hold a step back up off.
    public var sizes: Bool

    public init(atMs: Int64, watching: Bool, buffering: Bool, positionMs: Int, bufferedMs: Int, durationMs: Int,
                bandwidthKbps: Int?, sizes: Bool = false) {
        self.atMs = atMs
        self.watching = watching
        self.buffering = buffering
        self.positionMs = positionMs
        self.bufferedMs = bufferedMs
        self.durationMs = durationMs
        self.bandwidthKbps = bandwidthKbps
        self.sizes = sizes
    }
}

/// A change Automatic asks for: play `to` from where it is.
public struct AutoStep: Sendable, Equatable {
    public var to: QualityPreset
    /// Why it went down; `nil` for a step back up.
    public var reason: LoweredFor?

    public var up: Bool { reason == nil }
}

/// Automatic quality's decisions (docs/QUALITY.md), apart from the player
/// so they can be tested on their own: the original unless it doesn't play
/// well, then the quality that does. A port of the Android app's
/// `AutoQuality`, with the same constants.
///
/// - `begin` says where a file starts: the original, or at most the
///   server's ceiling for this network (`StartCeiling.server`); when the
///   server couldn't say, the quality the network carried when it was
///   found too slow on this device in the last 30 minutes.
/// - `sample` watches rebuffering and delivery while the video plays, and
///   asks to step down on a play issue, or back up after calm minutes. The
///   original as the file is moves only for a wait of 2 s or more on a
///   network not shown to carry it; a start below it for its ceiling goes
///   as high as the network carries after 20 calm seconds.
/// - It settles: a step up that a step down undoes within 10 minutes isn't
///   tried again during the title, and each one doubles the calm the next
///   step up waits for (3, 6, then 12 minutes).
/// - `deviceFellBehind` is dropped frames at a transcode; at the original,
///   the recovery ladder switches to the server's quality and tells
///   `deviceFellBehindAtOriginal`.
/// - `applied` is called once the player plays what was asked.
///
/// Seeks, pauses, the buffering after a start or a seek, and a new session
/// (`jumped`) are never play issues. Not thread-safe: the controller calls
/// it on the main actor.
public final class AutoQuality {
    /// A wait this long is a play issue…
    public static let longWaitMs: Int64 = 2_000
    /// …and so are two shorter ones within a minute.
    public static let shortWaits = 2
    public static let shortWaitsWindowMs: Int64 = 60_000
    /// At the original as the file is, a long wait counts on a network not shown to carry this times the file
    /// (video arriving: over 2 minutes)…
    public static let originalNetwork = 1.5
    public static let originalNetworkMs: Int64 = 120_000
    /// …or, where nothing showed its speed, at the second such wait within 5 minutes.
    public static let originalWaits = 2
    public static let originalWaitsWindowMs: Int64 = 300_000
    /// A start below the original for its ceiling goes as high as the network carries after this much calm.
    public static let ceilingStartCalmMs: Int64 = 20_000
    /// Delivery is measured over this long…
    public static let deliveryWindowMs: Int64 = 20_000
    public static let minDeliverySpanMs: Int64 = 5_000
    /// …and is a play issue under this for `lowForMs`, with less than `lowAheadMs` ahead and falling.
    public static let lowDelivery = 1.1
    public static let lowForMs: Int64 = 15_000
    public static let lowAheadMs = 10_000
    /// What ahead must have lost over the window to be falling.
    public static let fallingMs = 1_000
    public static let endMarginMs = 1_000
    /// The share of what the network carried that a step down aims for.
    public static let safety = 0.75
    /// The least time between two changes.
    public static let changeGapMs: Int64 = 20_000
    /// Watched without a play issue this long, it may step back up…
    public static let calmMs: Int64 = 180_000
    /// …but not this soon after a step down…
    public static let afterDownMs: Int64 = 300_000
    /// …and when the network carries this times the next quality up.
    public static let headroom = 1.6
    /// A step up that a step down undoes this soon was one too far: it isn't tried again this title.
    public static let settleMs: Int64 = 10 * 60_000
    /// Each step up undone doubles the calm the next one waits for, up to this.
    public static let maxCalmMs: Int64 = 4 * calmMs
    /// The measurements must cover the calm minutes, give or take this.
    public static let measureSlackMs: Int64 = 5_000
    /// How long the ceiling is remembered.
    public static let memoryMs: Int64 = 30 * 60_000

    private let memory: any CeilingMemory
    /// The remembered ceiling changed (kbps, `nil`: forgotten): the server is told (`NetworkStart.report`).
    private let onCeiling: (Int?) -> Void

    private var file: AutoFile?

    /// What plays: `Qualities.original` or one of the ladder's.
    public private(set) var playing: QualityPreset = Qualities.original
    /// Why it plays below the original; `nil` at the original.
    public private(set) var loweredFor: LoweredFor?

    /// The device fell behind here: steps back up stop below it.
    private var deviceTop: Int?
    private var originalBlocked = false
    private var originalConverted = false

    private var lastChangeAt: Int64?
    private var lastDownAt: Int64?

    // Rebuffering.
    private var settled = false
    private var waitFrom: Int64?
    private var waitCounted = false
    /// The wait under way lasted 2 s (`longWait`): what's left of it is no short wait.
    private var waitLong = false
    private var shortWaitTimes: [Int64] = []
    /// The wait under way came on a network nothing showed the speed of, and is among `originalWaitTimes`.
    private var waitUnmeasured = false
    /// When the original's long waits came on a network nothing showed the speed of.
    private var originalWaitTimes: [Int64] = []

    // Delivery: (time, buffered, playhead) over the last 20 s of watching.
    private var deliveries: [(at: Int64, buffered: Int, position: Int)] = []
    private var lowSince: Int64?
    /// What video arriving faster than it plays showed the network carries: (time, kbps), the last 2 minutes.
    private var filled: [(at: Int64, kbps: Int)] = []

    // Calm: watched time without an issue, and the network measured meanwhile.
    private var calm: Int64 = 0
    private var lastWatchedAt: Int64?
    private var bandwidths: [(at: Int64, kbps: Int)] = []
    private var lastBandwidth: Int?

    /// It began below the original for the ceiling it started at, and nothing went wrong since.
    private var ceilingStart = false

    // Settling: the last step up (to, when), until a step down or `settleMs` says whether it held…
    private var lastUp: (to: QualityPreset, at: Int64)?
    /// …the qualities a step down undid soon after a step up to them, not tried again this title…
    private var tooHigh: Set<String> = []
    /// …and the calm a step up waits for: `calmMs`, doubled by each step up undone, up to `maxCalmMs`.
    private var calmNeededMs = AutoQuality.calmMs

    public init(memory: any CeilingMemory, onCeiling: @escaping (Int?) -> Void = { _ in }) {
        self.memory = memory
        self.onCeiling = onCeiling
    }

    /// A new file in Automatic (a new title, or Automatic picked in the
    /// menu): it starts afresh, at the original, or at most at the ceiling
    /// when that is below the file's bitrate: the server's for this network
    /// when it said (`start`), else the one remembered here.
    @discardableResult
    public func begin(_ file: AutoFile, nowMs: Int64, start: StartCeiling = .device) -> QualityPreset {
        self.file = file
        deviceTop = nil
        originalBlocked = false
        originalConverted = false
        lastDownAt = nil
        lastChangeAt = nil
        resetSettling()
        resetWatch()
        resetCalm()
        let ceiling: Int? = if case .server(let kbps) = start { kbps } else { remembered(nowMs) }
        var begins: QualityPreset?
        if let ceiling, file.originalKbps.map({ ceiling < $0 }) ?? true {
            let ladder = ladder()
            begins = ladder.first { kbps($0) <= ceiling } ?? ladder.last
        }
        playing = begins ?? Qualities.original
        loweredFor = begins != nil ? .network : nil
        if begins != nil { lastChangeAt = nowMs }
        ceilingStart = begins != nil
        originalWaitTimes.removeAll()
        return playing
    }

    /// Automatic picked again in the menu: what was remembered of the network goes, here and on the server.
    public func forgetCeiling() {
        memory.forget()
        onCeiling(nil)
    }

    /// A file is in Automatic's hands (`begin`), until `stop`.
    public var active: Bool { file != nil }

    /// Another quality chosen: nothing is watched.
    public func stop() {
        file = nil
        playing = Qualities.original
        loweredFor = nil
        ceilingStart = false
        resetSettling()
        resetWatch()
        resetCalm()
    }

    /// The original plays at `kbps` now: the file as it is, or `converted`
    /// by the server at its own quality (then dropped frames step down from it).
    public func originalPlaysAt(_ kbps: Int?, converted: Bool = false) {
        guard var f = file else { return }
        originalConverted = converted
        if let kbps, kbps != f.originalKbps {
            f.originalKbps = kbps
            file = f
        }
    }

    /// A seek, a new session, a track switched: the waits and the delivery count from where it lands.
    public func jumped() { resetWatch() }

    /// The original as the file is (direct play or a remux): only a picture
    /// that stopped, a wait of 2 s or more the network is to blame for
    /// (`longWait`), moves it. Short waits and slow delivery don't: a big
    /// file's buffer holds only seconds of it, so they say little of the
    /// network, and a conversion is a worse picture for good.
    private var patient: Bool { playing.original && !originalConverted }

    /// Takes one look at the engine: a step to take, or `nil`.
    public func sample(_ s: AutoSample) -> AutoStep? {
        guard file != nil else { return nil }
        guard s.watching else {
            // Paused, in the background, or out of reach: nothing counts, the waits start over.
            waitFrom = nil
            waitCounted = false
            waitLong = false
            waitUnmeasured = false
            deliveries.removeAll()
            lowSince = nil
            lastWatchedAt = nil
            return nil
        }
        let watchedBefore = lastWatchedAt
        lastWatchedAt = s.atMs
        // The start's or the seek's buffering is over: from here, a wait is a play issue.
        if !s.buffering && !settled { settled = true }
        guard settled else { return nil }
        if let watchedBefore { calm += s.atMs - watchedBefore }
        if let bandwidth = s.bandwidthKbps, bandwidth > 0 { lastBandwidth = bandwidth }

        if let step = rebuffering(s) { return step }
        if let step = delivery(s) { return step }

        if let bandwidth = s.bandwidthKbps, bandwidth > 0 { bandwidths.append((s.atMs, bandwidth)) }
        bandwidths.removeAll { $0.at < s.atMs - calmNeededMs }
        return stepUp(s.atMs)
    }

    private func rebuffering(_ s: AutoSample) -> AutoStep? {
        if s.buffering {
            let from = waitFrom ?? s.atMs
            waitFrom = from
            if !waitCounted && s.atMs - from >= Self.longWaitMs && longWait(s.atMs) {
                // Asked again while it waits, until a step is taken (one may be too soon after the last).
                guard let step = issue(s.atMs, .network, sizes: s.sizes) else { return nil }
                waitCounted = true
                return step
            }
            return nil
        }
        guard let from = waitFrom else { return nil }
        waitFrom = nil
        let counted = waitCounted
        waitCounted = false
        // At the original, what loaded as it ended may show the network to blame after all.
        let blamed = !counted && s.atMs - from >= Self.longWaitMs && patient && longWait(s.atMs)
        let long = waitLong
        waitLong = false
        waitUnmeasured = false
        if blamed { return issue(s.atMs, .network, sizes: s.sizes) }
        if counted || long || s.atMs - from <= 0 || s.atMs - from >= Self.longWaitMs || patient { return nil }
        shortWaitTimes.append(s.atMs)
        shortWaitTimes.removeAll { $0 < s.atMs - Self.shortWaitsWindowMs }
        if shortWaitTimes.count >= Self.shortWaits {
            shortWaitTimes.removeAll()
            return issue(s.atMs, .network, sizes: s.sizes)
        }
        return nil
    }

    /// Whether the wait under way, 2 s long, is a play issue. At the
    /// original as the file is, only on a network not shown to carry
    /// `originalNetwork` times the file's bitrate (on one that does, the
    /// server is why: its disk, a remux catching up, which a conversion
    /// only slows down); where nothing showed its speed, the second such
    /// wait within 5 minutes.
    private func longWait(_ nowMs: Int64) -> Bool {
        waitLong = true
        if !patient { return true }
        let shown = networkShown(nowMs)
        if let shown, let original = file?.originalKbps {
            return Double(shown) < Double(original) * Self.originalNetwork
        }
        if !waitUnmeasured {
            waitUnmeasured = true
            originalWaitTimes.removeAll { $0 <= nowMs - Self.originalWaitsWindowMs }
            originalWaitTimes.append(nowMs)
        }
        return originalWaitTimes.count >= Self.originalWaits
    }

    /// The most the network was shown to carry, in kbps: the engine's
    /// measure of its last downloads, or video arriving faster than it
    /// plays in the last 2 minutes. `nil` when nothing showed.
    private func networkShown(_ nowMs: Int64) -> Int? {
        let arrived = filled.filter { $0.at > nowMs - Self.originalNetworkMs }.map(\.kbps).max()
        return [lastBandwidth, arrived].compactMap { $0 }.max()
    }

    private func delivery(_ s: AutoSample) -> AutoStep? {
        deliveries.append((s.atMs, s.bufferedMs, s.positionMs))
        while deliveries.count > 2 && deliveries[1].at <= s.atMs - Self.deliveryWindowMs { deliveries.removeFirst() }
        let rate = deliveryRate()
        // Filling faster than it plays: the network carries at least that much.
        if let rate, rate > 1, let playing = playingKbps() { filled.append((s.atMs, Int(Double(playing) * rate))) }
        filled.removeAll { $0.at <= s.atMs - Self.originalNetworkMs }
        let first = deliveries[0]
        let ahead = s.bufferedMs - s.positionMs
        let aheadBefore = first.buffered - first.position
        // Loaded to the end: nothing more can arrive, and nothing is late.
        let toTheEnd = s.durationMs > 0 && s.bufferedMs >= s.durationMs - Self.endMarginMs
        let low = rate.map { $0 < Self.lowDelivery } ?? false && !toTheEnd && !patient && ahead < Self.lowAheadMs
            && ahead < aheadBefore - Self.fallingMs
        guard low else {
            lowSince = nil
            return nil
        }
        let since = lowSince ?? s.atMs
        lowSince = since
        if s.atMs - since < Self.lowForMs { return nil }
        lowSince = nil
        return issue(s.atMs, .network, sizes: s.sizes)
    }

    /// Seconds of video that arrived per second, over the last 20 s; `nil` before 5 s of them.
    private func deliveryRate() -> Double? {
        guard deliveries.count >= 2, let from = deliveries.first, let to = deliveries.last else { return nil }
        let span = to.at - from.at
        if span < Self.minDeliverySpanMs { return nil }
        return Double(max(to.buffered - from.buffered, 0)) / Double(span)
    }

    /// The device dropped frames at a transcode: a step down, to the next
    /// quality of the ladder (fewer lines first), and no step back up to it.
    public func deviceFellBehind(nowMs: Int64) -> AutoStep? {
        // The original as it is: the recovery ladder's switch, as for every quality.
        if file == nil || (playing.original && !originalBlocked && !originalConverted) { return nil }
        return issue(nowMs, .device)
    }

    /// The device dropped frames at the original, and the recovery ladder
    /// switched to the server's quality (`convertedKbps`): that counts as
    /// this file's step down, and Automatic never steps back up to the original.
    public func deviceFellBehindAtOriginal(nowMs: Int64, convertedKbps: Int?) {
        guard file != nil else { return }
        wentDown(nowMs)
        originalBlocked = true
        originalPlaysAt(convertedKbps, converted: true)
        loweredFor = .device
        lastChangeAt = nowMs
        lastDownAt = nowMs
        resetCalm()
    }

    /// The server can't convert as fast as the video plays: a play issue
    /// like the network's. Not at the original as the file is: a remux the
    /// server can't keep up with reads all a conversion would, and does less.
    public func serverTooSlow(nowMs: Int64) -> AutoStep? {
        file == nil || patient ? nil : issue(nowMs, .network)
    }

    /// The player plays `step` now.
    public func applied(_ step: AutoStep, nowMs: Int64) {
        if step.up { lastUp = (step.to, nowMs) } else { wentDown(nowMs) }
        playing = step.to
        lastChangeAt = nowMs
        ceilingStart = false
        if step.up {
            if step.to.original {
                loweredFor = nil
                memory.forget()
                onCeiling(nil)
            } else {
                memory.write(kbps: kbps(step.to), atMs: nowMs)
                onCeiling(kbps(step.to))
            }
        } else {
            lastDownAt = nowMs
            loweredFor = step.reason
        }
        resetWatch()
        resetCalm()
    }

    /// A step down from what plays: from where a step up went less than
    /// `settleMs` ago, that step was one too far. It isn't tried again this
    /// title, and the calm before any other step up doubles.
    private func wentDown(_ nowMs: Int64) {
        guard let (to, at) = lastUp else { return }
        lastUp = nil
        if to.id != playing.id || nowMs - at >= Self.settleMs { return }
        tooHigh.insert(to.id)
        calmNeededMs = min(Self.maxCalmMs, calmNeededMs * 2)
    }

    /// A play issue: the calm starts over, and a step down, but in `sizes` for the network.
    private func issue(_ nowMs: Int64, _ reason: LoweredFor, sizes: Bool = false) -> AutoStep? {
        resetCalm()
        ceilingStart = false
        if sizes && reason == .network { return nil }
        if let last = lastChangeAt, nowMs - last < Self.changeGapMs { return nil }
        let ladder = ladder()
        let current = playingKbps()
        var target: QualityPreset?
        if reason == .device {
            let next: QualityPreset?
            if playing.original {
                next = ladder.first { current == nil || kbps($0) < current! }
            } else {
                let at = ladder.firstIndex(of: playing).map { $0 + 1 } ?? 0
                next = at < ladder.count ? ladder[at] : nil
            }
            if let next { deviceTop = ladder.firstIndex(of: next) }
            target = next
        } else {
            // Too early to measure delivery: the network's speed for this bitrate, else as much as plays.
            let rate = deliveryRate()
                ?? current.flatMap { c in lastBandwidth.map { min(Double($0) / Double(c), 1) } }
                ?? 1
            let ceiling = current.map { Double($0) * rate * Self.safety }
            let lower = ladder.filter { current == nil || kbps($0) < current! }
            let pick = lower.first { ceiling == nil || Double(kbps($0)) <= ceiling! } ?? lower.last
            if let pick {
                let kept = max(min(ceiling.map { Int($0) } ?? kbps(pick), kbps(pick)), kbps(Qualities.lowest))
                memory.write(kbps: kept, atMs: nowMs)
                onCeiling(kept)
            }
            target = pick
        }
        return target.map { AutoStep(to: $0, reason: reason) }
    }

    /// A quality up after calm minutes, when the network carries it; from a
    /// start below the original for its ceiling, nothing gone wrong since,
    /// as high as the network carries after `ceilingStartCalmMs`.
    private func stepUp(_ nowMs: Int64) -> AutoStep? {
        let needed = ceilingStart ? Self.ceilingStartCalmMs : calmNeededMs
        if playing.original || calm < needed { return nil }
        if let down = lastDownAt, nowMs - down < Self.afterDownMs { return nil }
        if let change = lastChangeAt, nowMs - change < needed { return nil }
        // A network it can't measure: it stays where it went down to.
        guard let oldest = bandwidths.first, oldest.at <= nowMs - needed + Self.measureSlackMs,
              let measured = bandwidths.filter({ $0.at >= nowMs - needed }).map(\.kbps).min()
        else { return nil }
        let ladder = ladder()
        var at = max(ladder.firstIndex(of: playing) ?? 0, 0)
        var to: QualityPreset?
        while at >= 0 {
            let next = at == 0 ? Qualities.original : ladder[at - 1]
            // Settled: the quality above didn't hold this title.
            if tooHigh.contains(next.id) { break }
            if next.original && (originalBlocked || deviceTop != nil) { break }
            if !next.original, let top = deviceTop, let index = ladder.firstIndex(of: next), index < top { break }
            let need: Int
            if next.original {
                guard let original = file?.originalKbps else { break }
                need = original
            } else {
                need = kbps(next)
            }
            if Double(measured) < Double(need) * Self.headroom { break }
            to = next
            if !ceilingStart { break }
            at -= 1
        }
        return to.map { AutoStep(to: $0, reason: nil) }
    }

    /// The bitrate playing now, in kbps; `nil` for an original of unknown bitrate.
    private func playingKbps() -> Int? {
        playing.original ? file?.originalKbps ?? file.map { kbps($0.cap) } : kbps(playing)
    }

    /// The qualities Automatic plays below the original, highest first: the
    /// file's offered transcodes up to the server's own quality for it, by
    /// the lines they give this file, then by bitrate.
    public func ladder() -> [QualityPreset] {
        guard let f = file else { return [] }
        func lines(_ q: QualityPreset) -> Int { min(q.maxHeight ?? .max, f.lines ?? .max) }
        let (capLines, capKbps) = (lines(f.cap), kbps(f.cap))
        return f.offered
            .filter { $0.maxBitrateKbps != nil && kbps($0) <= capKbps && lines($0) <= capLines }
            .enumerated()
            .sorted { a, b in
                let (la, lb) = (lines(a.element), lines(b.element))
                if la != lb { return la > lb }
                let (ka, kb) = (kbps(a.element), kbps(b.element))
                if ka != kb { return ka > kb }
                return a.offset < b.offset
            }
            .map(\.element)
    }

    private func remembered(_ nowMs: Int64) -> Int? {
        guard let (kbps, at) = memory.read() else { return nil }
        if at > nowMs || nowMs - at > Self.memoryMs {
            memory.forget()
            return nil
        }
        return kbps
    }

    private func resetWatch() {
        settled = false
        waitFrom = nil
        waitCounted = false
        waitLong = false
        waitUnmeasured = false
        shortWaitTimes.removeAll()
        deliveries.removeAll()
        lowSince = nil
        lastWatchedAt = nil
    }

    private func resetSettling() {
        lastUp = nil
        tooHigh.removeAll()
        calmNeededMs = Self.calmMs
    }

    private func resetCalm() {
        calm = 0
        bandwidths.removeAll()
    }

    private func kbps(_ q: QualityPreset) -> Int { q.maxBitrateKbps ?? 0 }
}
