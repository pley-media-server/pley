import Foundation
import PleyModel
@testable import PleyPlayer
import Testing

/// A playback simulated half a second at a time, for `AutoQuality` alone
/// (the Android app's AutoQualityTest, case by case).
private final class Sim {
    static let tick: Int64 = 500

    let memory = MemoryCeiling()
    /// The ceilings told to the server (NetworkStart.report).
    var reported: [Int?] = []
    lazy var auto = AutoQuality(memory: memory) { [unowned self] in self.reported.append($0) }

    /// The clock; far from 0, so a memory from "before" is in the past.
    var now: Int64 = 10_000_000
    var position = 0
    var buffered = 30_000
    var duration = 3_600_000

    /// 1080p at 10 Mbps: offered 8, 4 and 2 Mbps; the server's quality is 12 Mbps 1080p.
    let hd = mediaFile(bitrate: 10_000_000, width: 1920, height: 1080)
    /// A 4K remux at 60 Mbps: offered everything; the server's quality is 30 Mbps 4K.
    let uhd = mediaFile(bitrate: 60_000_000, width: 3840, height: 2160)
    /// 1080p at 18 Mbps: offered 12, 8, 4 and 2 Mbps; the server's quality is 20 Mbps 1080p.
    let fhd = mediaFile(bitrate: 18_000_000, width: 1920, height: 1080)

    func autoFile(_ f: MediaFile) -> AutoFile {
        AutoFile(offered: Qualities.forFile(f), originalKbps: Qualities.fileKbps(f), lines: Qualities.linesOf(f),
                 cap: Qualities.closest(f))
    }

    @discardableResult
    func begin(_ f: MediaFile) -> QualityPreset { auto.begin(autoFile(f), nowMs: now) }

    /// Plays for `ms`: the playhead moves unless `waiting`, and video
    /// arrives at `rate` seconds a second. The first step asked for, or `nil`.
    @discardableResult
    func play(_ ms: Int64, rate: Double = 1, waiting: Bool = false, watching: Bool = true, bandwidth: Int? = nil,
              apply: Bool = true, sizes: Bool = false) -> AutoStep? {
        var left = ms
        while left > 0 {
            now += Self.tick
            left -= Self.tick
            if watching && !waiting { position += Int(Self.tick) }
            buffered = max(min(buffered + Int(rate * Double(Self.tick)), duration), position)
            let step = auto.sample(AutoSample(atMs: now, watching: watching, buffering: waiting, positionMs: position,
                                              bufferedMs: buffered, durationMs: duration, bandwidthKbps: bandwidth,
                                              sizes: sizes))
            if let step {
                if apply { applied(step) }
                return step
            }
        }
        return nil
    }

    /// The player plays `step` now: a new session, which buffers before it plays.
    func applied(_ step: AutoStep) {
        auto.applied(step, nowMs: now)
        settle()
    }

    /// A start, a seek, or a new session: a moment of buffering, which never counts, then `ahead` loaded.
    func settle(ahead: Int = 30_000) {
        auto.jumped()
        #expect(play(3_000, waiting: true) == nil)
        buffered = position + ahead
        #expect(play(Self.tick) == nil)
    }

    @discardableResult
    func started(_ f: MediaFile, ahead: Int = 30_000) -> QualityPreset {
        let q = begin(f)
        settle(ahead: ahead)
        return q
    }

    /// Begun below the original, at most at `kbps` as the server knows the network, 20 s ago, and settled.
    @discardableResult
    func startedBelow(_ f: MediaFile, _ kbps: Int, ahead: Int = 30_000) -> QualityPreset {
        let q = auto.begin(autoFile(f), nowMs: now, start: .server(kbps: kbps))
        now += AutoQuality.changeGapMs
        settle(ahead: ahead)
        return q
    }

    func ids(_ list: [QualityPreset]) -> [String] { list.map(\.id) }

    /// A wait after 20 s of video arriving at `rate`, on a network measured at `bandwidth`: the step it takes.
    func falters(_ rate: Double, bandwidth: Int? = nil) throws -> AutoStep {
        #expect(play(20_000, rate: rate, bandwidth: bandwidth) == nil)
        return try #require(play(3_000, rate: rate, waiting: true))
    }

    /// Calm with a fast network: no step for `quietMs`, then `to` within the next minute.
    func upAfter(_ quietMs: Int64, _ to: String) throws {
        #expect(play(quietMs, bandwidth: 50_000) == nil)
        let up = try #require(play(60_000, bandwidth: 50_000))
        #expect(up.up)
        #expect(up.to.id == to)
    }

    func downTo4m() throws {
        started(hd)
        #expect(play(20_000, rate: 0.6, bandwidth: 6_000) == nil)
        #expect(play(3_000, rate: 0.6, waiting: true)?.to.id == "4m")
    }
}

@Suite("Automatic's decisions")
struct AutoQualityTests {
    // MARK: The ladder and the start

    @Test func startsAtTheOriginalWithTheFilesQualitiesUpToTheServersBelowIt() {
        let h = Sim()
        #expect(h.begin(h.uhd) == Qualities.original)
        #expect(h.auto.loweredFor == nil)
        // 4K first, then 1080p and below, as the menu says them.
        #expect(h.ids(h.auto.ladder()) == ["30m", "15m", "20m", "12m", "8m", "4m", "2m"])
        h.begin(h.hd)
        #expect(h.ids(h.auto.ladder()) == ["8m", "4m", "2m"])
        // 1080p at 40 Mbps: the server converts it at 20 Mbps 1080p, so nothing above.
        h.begin(mediaFile(bitrate: 40_000_000, width: 1920, height: 1080))
        #expect(h.ids(h.auto.ladder()) == ["20m", "15m", "12m", "8m", "4m", "2m"])
    }

    @Test func theServersQualityForAFileFollowsTheServersRule() {
        let h = Sim()
        #expect(Qualities.closest(h.uhd).id == "30m")
        #expect(Qualities.closest(h.hd).id == "12m")
        #expect(Qualities.closest(mediaFile(bitrate: 40_000_000, width: 1920, height: 1080)).id == "20m")
        // 4K at 6 Mbps: 4K's lowest.
        #expect(Qualities.closest(mediaFile(bitrate: 6_000_000, width: 3840, height: 2160)).id == "15m")
        #expect(Qualities.closest(mediaFile(bitrate: 3_000_000, width: 1280, height: 720)).id == "4m")
        #expect(Qualities.closest(nil).id == "20m")
    }

    @Test func theBufferingOfAStartOrASeekAndWaitsWhilePausedAreNoPlayIssue() {
        let h = Sim()
        h.begin(h.hd)
        #expect(h.play(10_000, waiting: true) == nil)
        #expect(h.play(5_000) == nil)
        // Paused, the video waits for nothing.
        #expect(h.play(10_000, waiting: true, watching: false) == nil)
        h.auto.jumped()
        #expect(h.play(6_000, waiting: true) == nil)
        #expect(h.play(5_000) == nil)
        #expect(h.auto.playing == Qualities.original)
    }

    // MARK: The triggers

    @Test func aWaitOf2SecondsStepsDown() throws {
        let h = Sim()
        h.started(h.hd)
        #expect(h.play(20_000, bandwidth: 8_000) == nil)
        #expect(h.play(1_500, waiting: true) == nil)
        let step = try #require(h.play(1_000, waiting: true))
        #expect(step.reason == .network)
        #expect(!step.up)
        #expect(h.auto.loweredFor == .network)
    }

    @Test func twoShortWaitsWithinAMinuteStepDownFurtherApartTheyDont() {
        let h = Sim()
        #expect(h.startedBelow(h.hd, 8_000).id == "8m")
        #expect(h.play(10_000) == nil)
        #expect(h.play(1_000, waiting: true) == nil)
        #expect(h.play(70_000) == nil)
        #expect(h.play(1_000, waiting: true) == nil)
        #expect(h.play(Sim.tick) == nil)
        #expect(h.play(40_000) == nil)
        #expect(h.play(1_000, waiting: true) == nil)
        #expect(h.play(Sim.tick)?.to.id == "4m")
    }

    @Test func videoArrivingSlowerThanItPlaysWithLittleAheadAndFallingStepsDownBeforeItWaits() throws {
        let h = Sim()
        h.startedBelow(h.hd, 8_000, ahead: 12_000)
        // 0.5 s a second: ahead falls under 10 s after 4 s, and it is a play issue 15 s later.
        #expect(h.play(18_000, rate: 0.5) == nil)
        let step = try #require(h.play(3_000, rate: 0.5))
        #expect(step.reason == .network)
        // 8 Mbps × 0.5 × 0.75 = 3 Mbps: 2 Mbps 480p.
        #expect(step.to.id == "2m")
    }

    @Test func slowDeliveryWithPlentyAheadSteadyOrLoadedToTheEndIsNoPlayIssue() {
        let h = Sim()
        // 30 s ahead: it has time.
        h.startedBelow(h.hd, 8_000)
        #expect(h.play(30_000, rate: 0.8) == nil)
        // 5 s ahead and steady (a buffer full by its bytes).
        h.startedBelow(h.hd, 8_000, ahead: 5_000)
        #expect(h.play(40_000, rate: 1.0) == nil)
        // Loaded to the end: nothing more arrives, nothing is late.
        h.startedBelow(h.hd, 8_000, ahead: 8_000)
        h.duration = h.buffered
        #expect(h.play(7_000, rate: 0.0) == nil)
    }

    @Test func theOriginalAsTheFileIsMovesOnlyForAWaitOf2Seconds() {
        let h = Sim()
        h.started(h.uhd, ahead: 12_000)
        // A big file's few seconds ahead, arriving no faster than they play, on a network measured at 40 Mbps.
        #expect(h.play(40_000, rate: 0.5, bandwidth: 40_000) == nil)
        for _ in 0..<3 {
            #expect(h.play(1_000, waiting: true) == nil)
            #expect(h.play(10_000) == nil)
        }
        #expect(h.auto.playing == Qualities.original)
        #expect(h.play(3_000, waiting: true)?.reason == .network)
    }

    @Test func theOriginalWaitsOnANetworkCarrying1Point5TimesTheFileTheServerIsWhy() {
        let h = Sim()
        h.started(h.uhd)
        for _ in 0..<5 {
            #expect(h.play(20_000, bandwidth: 90_000) == nil)
            #expect(h.play(8_000, waiting: true) == nil)
            // What ended the wait came as fast.
            #expect(h.play(Sim.tick, bandwidth: 90_000) == nil)
        }
        #expect(h.auto.playing == Qualities.original)
        // The network slower since: the next wait moves it, from what arrived (60 × 0.9 × 0.75 = 40.5).
        #expect(h.play(20_000, rate: 0.9, bandwidth: 50_000) == nil)
        #expect(h.play(3_000, rate: 0.9, waiting: true)?.to.id == "30m")
    }

    @Test func theOriginalOnANetworkNothingMeasuredMovesAtTheSecondLongWaitWithin5Minutes() {
        let h = Sim()
        h.started(h.uhd)
        #expect(h.play(10_000) == nil)
        #expect(h.play(6_000, waiting: true) == nil)
        #expect(h.play(5 * 60_000) == nil)
        #expect(h.play(6_000, waiting: true) == nil)
        #expect(h.auto.playing == Qualities.original)
        #expect(h.play(4 * 60_000) == nil)
        #expect(h.play(3_000, waiting: true)?.reason == .network)
    }

    @Test func theOriginalMovesWhenTheDownloadThatEndsAWaitShowsTheNetworkToBlame() {
        let h = Sim()
        h.started(h.uhd)
        #expect(h.play(20_000, bandwidth: 200_000) == nil)
        #expect(h.play(3_000, waiting: true) == nil)
        // The segment it waited for came at 20 Mbps.
        #expect(h.play(Sim.tick, bandwidth: 20_000)?.reason == .network)
    }

    @Test func anOriginalTheServerConvertsMovesForShortWaitsToo() {
        let h = Sim()
        h.started(h.hd)
        h.auto.originalPlaysAt(12_000, converted: true)
        #expect(h.play(10_000) == nil)
        #expect(h.play(1_000, waiting: true) == nil)
        #expect(h.play(10_000) == nil)
        #expect(h.play(1_000, waiting: true) == nil)
        #expect(h.play(Sim.tick)?.to.id == "8m")
    }

    @Test func inSeveralSizesTheNetworkIsAVPlayersAndCalmMinutesStepBackUpToTheOriginal() throws {
        let h = Sim()
        #expect(h.startedBelow(h.uhd, 30_000).id == "30m")
        // A wait and slow delivery: AVPlayer moves between the sizes for them.
        #expect(h.play(3_000, waiting: true, sizes: true) == nil)
        #expect(h.play(10_000, sizes: true) == nil)
        #expect(h.auto.playing.id == "30m")
        // 3 calm minutes, the network carrying 1.6 times the original's 60 Mbps.
        #expect(h.play(165_000, bandwidth: 200_000, sizes: true) == nil)
        let up = try #require(h.play(20_000, bandwidth: 200_000, sizes: true))
        #expect(up.to == Qualities.original)
        #expect(up.up)
        #expect(h.memory.read() == nil)
    }

    // MARK: The formula

    @Test func theCeilingIsTheBitratePlayingTimesTheDeliverySeenTimes075() {
        let h = Sim()
        h.started(h.hd)
        // 0.6 s a second for 20 s, then a wait: 10 Mbps × 0.6 × 0.75 = 4.5 Mbps.
        #expect(h.play(20_000, rate: 0.6, bandwidth: 6_000) == nil)
        #expect(h.play(3_000, rate: 0.6, waiting: true)?.to.id == "4m")
        #expect(h.memory.read()?.kbps == 4_000)
    }

    @Test func itPlaysTheHighestOfTheMenuUnderTheCeiling4KFirst() {
        let h = Sim()
        h.started(h.uhd)
        // 60 Mbps × 0.5 × 0.75 = 22.5 Mbps: 15 Mbps 4K, above 20 Mbps 1080p in the menu.
        #expect(h.play(20_000, rate: 0.5, bandwidth: 30_000) == nil)
        #expect(h.play(3_000, rate: 0.5, waiting: true)?.to.id == "15m")
    }

    @Test func theStepsFollowTheMenuTheFileIsOffered() {
        let h = Sim()
        // 4K at 18 Mbps: 15 Mbps 4K isn't offered (83 %), and 20 Mbps is above the server's 15 Mbps 4K.
        h.started(mediaFile(bitrate: 18_000_000, width: 3840, height: 2160))
        #expect(h.ids(h.auto.ladder()) == ["12m", "8m", "4m", "2m"])
        #expect(h.play(20_000, bandwidth: 18_000) == nil)
        #expect(h.play(3_000, waiting: true)?.to.id == "12m")
    }

    @Test func neverAboveTheServersQualityForTheFile() {
        let h = Sim()
        h.started(h.uhd)
        // 60 Mbps × 1.2 × 0.75 = 54 Mbps: 30 Mbps 4K, the server's own.
        #expect(h.play(20_000, rate: 1.2) == nil)
        #expect(h.play(3_000, rate: 1.2, waiting: true)?.to.id == "30m")
    }

    @Test func neverBelow2Mbps480p() {
        let h = Sim()
        h.started(h.hd)
        #expect(h.play(20_000, rate: 0.1, bandwidth: 1_000) == nil)
        #expect(h.play(3_000, rate: 0.1, waiting: true)?.to.id == "2m")
        // Already there: nothing lower, the recovery ladder takes over.
        h.now += AutoQuality.changeGapMs
        #expect(h.play(20_000, waiting: true) == nil)
        #expect(h.auto.playing.id == "2m")
    }

    @Test func aStepDownIsOneQualityAtLeastEvenOnAFastNetwork() {
        let h = Sim()
        h.started(h.hd)
        #expect(h.play(20_000, rate: 1.0) == nil)
        #expect(h.play(3_000, rate: 3.0, waiting: true)?.to.id == "8m")
        #expect(h.play(30_000, rate: 3.0) == nil)
        #expect(h.play(3_000, rate: 3.0, waiting: true)?.to.id == "4m")
    }

    @Test func changesAre20SecondsApartAtLeast() {
        let h = Sim()
        h.started(h.hd)
        #expect(h.play(20_000, rate: 1.2) == nil)
        #expect(h.play(3_000, rate: 1.2, waiting: true)?.to.id == "8m")
        // A wait right after the change: too soon, it waits on.
        #expect(h.play(5_000) == nil)
        #expect(h.play(10_000, waiting: true) == nil)
        // 20 s after the change: the same wait steps down now.
        #expect(h.play(10_000, waiting: true)?.to.id == "4m")
    }

    // MARK: The device

    @Test func droppedFramesAtATranscodeGoToFewerLinesAndNeverBackUpToIt() throws {
        let h = Sim()
        h.started(h.uhd)
        #expect(h.play(20_000, rate: 0.5, bandwidth: 30_000) == nil)
        #expect(h.play(3_000, rate: 0.5, waiting: true)?.to.id == "15m")
        h.now += AutoQuality.changeGapMs
        let step = try #require(h.auto.deviceFellBehind(nowMs: h.now))
        #expect(step.to.id == "20m")
        #expect(step.reason == .device)
        h.applied(step)
        #expect(h.auto.loweredFor == .device)
        // Calm and a fast network: 1080p stays, 4K made the device fall behind.
        #expect(h.play(10 * 60_000, bandwidth: 200_000) == nil)
        #expect(h.auto.playing.id == "20m")
    }

    @Test func droppedFramesAtTheOriginalAreTheRecoveryLaddersAndTheOriginalStaysBehind() {
        let h = Sim()
        h.started(h.hd)
        #expect(h.auto.deviceFellBehind(nowMs: h.now) == nil)
        h.auto.deviceFellBehindAtOriginal(nowMs: h.now, convertedKbps: 12_000)
        #expect(h.auto.loweredFor == .device)
        h.settle()
        #expect(h.play(10 * 60_000, bandwidth: 200_000) == nil)
        #expect(h.auto.playing == Qualities.original)
        // Dropped frames again, at the server's conversion: one step lower.
        #expect(h.auto.deviceFellBehind(nowMs: h.now)?.to.id == "8m")
    }

    @Test func aServerTooSlowToConvertIsAPlayIssue() {
        let h = Sim()
        h.started(h.hd)
        h.auto.originalPlaysAt(12_000, converted: true)
        #expect(h.auto.serverTooSlow(nowMs: h.now)?.to.id == "8m")
    }

    @Test func aRemuxTheServerCantKeepUpWithStays() {
        let h = Sim()
        h.started(h.uhd)
        #expect(h.auto.serverTooSlow(nowMs: h.now) == nil)
        #expect(h.auto.playing == Qualities.original)
    }

    // MARK: Stepping back up

    @Test func stepsBackUpOneAtATimeAfterCalmMinutesWith1Point6TimesTheNextQuality() throws {
        let h = Sim()
        try h.downTo4m()
        // Calm for 3 minutes, but not 5 minutes after the step down yet.
        #expect(h.play(4 * 60_000 + 50_000, bandwidth: 20_000) == nil)
        let up = try #require(h.play(60_000, bandwidth: 20_000))
        #expect(up.to.id == "8m")
        #expect(up.up)
        #expect(up.reason == nil)
        #expect(h.memory.read()?.kbps == 8_000)
        // The next one 3 minutes later at least: the original, at 1.6 × 10 Mbps.
        #expect(h.play(2 * 60_000 + 50_000, bandwidth: 20_000) == nil)
        let original = try #require(h.play(60_000, bandwidth: 20_000))
        #expect(original.to == Qualities.original)
        #expect(h.auto.loweredFor == nil)
        // Back at the original, the ceiling is forgotten.
        #expect(h.memory.read() == nil)
    }

    @Test func aStartBelowTheOriginalForItsCeilingGoesAsHighAsTheNetworkCarriesAfter20CalmSeconds() throws {
        let h = Sim()
        #expect(h.startedBelow(h.uhd, 15_000).id == "15m")
        // 1.6 × 30 Mbps is 48, 1.6 × 60 is 96: the server's 4K, not the original.
        #expect(h.play(19_000, bandwidth: 60_000) == nil)
        #expect(h.play(2_000, bandwidth: 60_000)?.to.id == "30m")
        // From there, as after any step: 3 calm minutes, a quality at a time.
        #expect(h.play(2 * 60_000 + 50_000, bandwidth: 200_000) == nil)
        #expect(h.play(60_000, bandwidth: 200_000)?.to == Qualities.original)
    }

    @Test func aStartBelowTheOriginalAPlayIssueFirstStepsUpAsAfterAStepDown() {
        let h = Sim()
        #expect(h.startedBelow(h.hd, 4_000).id == "4m")
        #expect(h.play(3_000, waiting: true, sizes: true) == nil)
        #expect(h.play(2 * 60_000 + 50_000, bandwidth: 50_000, sizes: true) == nil)
        #expect(h.play(60_000, bandwidth: 50_000, sizes: true)?.to.id == "8m")
    }

    @Test func itStaysDownWithoutTheHeadroomAMeasureOrCalm() throws {
        let h = Sim()
        try h.downTo4m()
        // 1.6 × 8 Mbps is 12.8: 12 isn't enough.
        #expect(h.play(10 * 60_000, bandwidth: 12_000) == nil)
        // A network it can't measure.
        #expect(h.play(10 * 60_000, bandwidth: nil) == nil)
        // A dip during the calm minutes.
        #expect(h.play(2 * 60_000, bandwidth: 50_000) == nil)
        #expect(h.play(5_000, bandwidth: 9_000) == nil)
        #expect(h.play(2 * 60_000, bandwidth: 50_000) == nil)
        #expect(h.auto.playing.id == "4m")
    }

    @Test func pausesDontCountAsCalm() throws {
        let h = Sim()
        try h.downTo4m()
        #expect(h.play(10 * 60_000, watching: false, bandwidth: 50_000) == nil)
        #expect(h.play(2 * 60_000, bandwidth: 50_000) == nil)
        #expect(h.auto.playing.id == "4m")
        #expect(h.play(2 * 60_000, bandwidth: 50_000)?.to.id == "8m")
    }

    @Test func aPlayIssueStartsTheCalmOver() throws {
        let h = Sim()
        try h.downTo4m()
        #expect(h.play(6 * 60_000, bandwidth: 5_000) == nil)
        // Two short waits: back to 2 Mbps; the calm counts from there.
        #expect(h.play(1_000, waiting: true, bandwidth: 50_000) == nil)
        #expect(h.play(10_000, bandwidth: 50_000) == nil)
        #expect(h.play(1_000, waiting: true, bandwidth: 50_000) == nil)
        #expect(h.play(Sim.tick, bandwidth: 50_000)?.to.id == "2m")
        #expect(h.play(4 * 60_000, bandwidth: 50_000) == nil)
        #expect(h.play(2 * 60_000, bandwidth: 50_000)?.to.id == "4m")
    }

    // MARK: It settles

    @Test func settlesAStepUpUndoneWithin10MinutesIsntTriedAgainDuringTheTitle() throws {
        let h = Sim()
        try h.downTo4m()
        try h.upAfter(4 * 60_000 + 50_000, "8m")
        // Two minutes on, the network can't carry 8 Mbps after all (8 × 0.8 × 0.75 = 4.8).
        #expect(h.play(100_000, bandwidth: 50_000) == nil)
        #expect(try h.falters(0.8).to.id == "4m")
        let kept = h.memory.read()
        #expect(kept?.kbps == 4_000)
        // However fast the network says it is, it stays where it held, for the rest of the title.
        #expect(h.play(60 * 60_000, bandwidth: 80_000) == nil)
        #expect(h.auto.playing.id == "4m")
        #expect(h.memory.read()?.kbps == kept?.kbps)
        #expect(h.memory.read()?.atMs == kept?.atMs)
    }

    @Test func aStepDownMoreThan10MinutesAfterAStepUpLeavesThatQualityOpen() throws {
        let h = Sim()
        try h.downTo4m()
        try h.upAfter(4 * 60_000 + 50_000, "8m")
        #expect(h.play(10 * 60_000, bandwidth: 9_000) == nil)
        #expect(try h.falters(0.8).to.id == "4m")
        // 5 minutes after the step down, as before.
        try h.upAfter(4 * 60_000 + 50_000, "8m")
    }

    @Test func eachStepUpUndoneDoublesTheCalmTheNextOneWaitsFor() throws {
        let h = Sim()
        h.started(h.fhd)
        #expect(h.ids(h.auto.ladder()) == ["12m", "8m", "4m", "2m"])
        // 18 × 0.3 × 0.75 = 4.05.
        #expect(try h.falters(0.3, bandwidth: 6_000).to.id == "4m")
        try h.upAfter(4 * 60_000 + 50_000, "8m")
        try h.upAfter(2 * 60_000 + 50_000, "12m")
        // 12 Mbps doesn't hold (12 × 0.9 × 0.75 = 8.1): back to 8, and 12 is off for the title.
        #expect(try h.falters(0.9).to.id == "8m")
        // Later 8 Mbps falters too, more than 10 minutes after it was reached: 4, with 8 still open.
        #expect(h.play(10 * 60_000, bandwidth: 50_000) == nil)
        #expect(try h.falters(0.8).to.id == "4m")
        // 5 minutes after the step down 3 calm minutes would have done; now it takes 6.
        try h.upAfter(5 * 60_000 + 50_000, "8m")
        #expect(h.play(30 * 60_000, bandwidth: 50_000) == nil)
        #expect(h.auto.playing.id == "8m")
    }

    @Test func theCalmDoublesTo12MinutesAtMost() throws {
        let h = Sim()
        // A 4K remux at 60 Mbps: 60 × 0.3 × 0.75 = 13.5, so 12 Mbps 1080p.
        h.started(h.uhd)
        #expect(try h.falters(0.3, bandwidth: 18_000).to.id == "12m")
        // 20 Mbps 1080p doesn't hold (20 × 0.9 × 0.75 = 13.5): the calm is 6 minutes.
        try h.upAfter(4 * 60_000 + 50_000, "20m")
        #expect(try h.falters(0.9).to.id == "12m")
        #expect(try h.falters(0.9).to.id == "8m")
        try h.upAfter(5 * 60_000 + 50_000, "12m")
        // 12 doesn't hold either: 12 minutes.
        #expect(try h.falters(0.9).to.id == "8m")
        #expect(try h.falters(0.8).to.id == "4m")
        try h.upAfter(11 * 60_000 + 50_000, "8m")
        // Nor 8: still 12 minutes, not 24.
        #expect(try h.falters(0.9).to.id == "4m")
        #expect(try h.falters(0.8).to.id == "2m")
        try h.upAfter(11 * 60_000 + 50_000, "4m")
    }

    @Test func aStepBackUpToTheOriginalUndoneKeepsTheQualityBelow() throws {
        let h = Sim()
        try h.downTo4m()
        try h.upAfter(4 * 60_000 + 50_000, "8m")
        let original = try #require(h.play(4 * 60_000, bandwidth: 20_000))
        #expect(original.to.original)
        #expect(h.memory.read() == nil)
        // On a network measured at 12 Mbps, 10 × 1.2 × 0.75 = 9: 8 Mbps, and the original is off for the title.
        #expect(try h.falters(1.2, bandwidth: 12_000).to.id == "8m")
        #expect(h.memory.read()?.kbps == 8_000)
        #expect(h.play(30 * 60_000, bandwidth: 80_000) == nil)
        #expect(h.auto.playing.id == "8m")
    }

    @Test func aNewTitleTriesEveryQualityAgainFromWhereTheNetworkHeld() throws {
        let h = Sim()
        try h.downTo4m()
        try h.upAfter(4 * 60_000 + 50_000, "8m")
        #expect(try h.falters(0.8).to.id == "4m")
        // The next title: it starts where the network held, and goes as high as the network carries after 20 calm seconds.
        #expect(h.begin(h.hd).id == "4m")
        h.settle()
        #expect(h.play(19_000, bandwidth: 50_000) == nil)
        #expect(h.play(2_000, bandwidth: 50_000)?.to == Qualities.original)
    }

    @Test func automaticPickedAgainStartsAfresh() throws {
        let h = Sim()
        try h.downTo4m()
        try h.upAfter(4 * 60_000 + 50_000, "8m")
        #expect(try h.falters(0.8).to.id == "4m")
        h.auto.stop()
        // Automatic picked again: afresh, at the original.
        h.auto.forgetCeiling()
        #expect(h.begin(h.hd) == Qualities.original)
        h.settle()
        #expect(try h.falters(0.6, bandwidth: 6_000).to.id == "4m")
        try h.upAfter(4 * 60_000 + 50_000, "8m")
    }

    // MARK: The memory

    @Test func theNextTitleStartsAtTheRememberedCeilingFor30Minutes() throws {
        let h = Sim()
        try h.downTo4m()
        #expect(h.memory.read()?.kbps == 4_000)
        h.now += 60_000
        // A new title: at most what the network carried.
        #expect(h.begin(h.uhd).id == "4m")
        #expect(h.auto.loweredFor == .network)
        // A file below the ceiling plays as it is.
        #expect(h.begin(mediaFile(bitrate: 3_000_000, width: 1280, height: 720)) == Qualities.original)
        // 30 minutes on, forgotten.
        h.now += AutoQuality.memoryMs
        #expect(h.begin(h.uhd) == Qualities.original)
        #expect(h.memory.read() == nil)
    }

    @Test func theServersCeilingForTheNetworkTakesThePlaceOfTheRememberedOne() {
        let h = Sim()
        h.memory.write(kbps: 2_000, atMs: h.now)
        // The server knows no limit here: the original, whatever this device remembers.
        #expect(h.auto.begin(h.autoFile(h.hd), nowMs: h.now, start: .server(kbps: nil)) == Qualities.original)
        #expect(h.auto.loweredFor == nil)
        // The server's ceiling, as the remembered one works.
        #expect(h.auto.begin(h.autoFile(h.uhd), nowMs: h.now, start: .server(kbps: 5_000)).id == "4m")
        #expect(h.auto.loweredFor == .network)
        #expect(h.auto.begin(h.autoFile(mediaFile(bitrate: 3_000_000, width: 1280, height: 720)), nowMs: h.now,
                             start: .server(kbps: 5_000)) == Qualities.original)
        // The server couldn't say: this device's memory.
        #expect(h.auto.begin(h.autoFile(h.hd), nowMs: h.now, start: .device).id == "2m")
        #expect(h.reported.isEmpty)
    }

    @Test func everyChangeOfTheRememberedCeilingIsToldToTheServer() throws {
        let h = Sim()
        try h.downTo4m()
        #expect(h.reported == [h.memory.read()?.kbps])
        // A step up to a quality is its bitrate; to the original, forgotten.
        let up = try #require(h.play(6 * 60_000, bandwidth: 20_000))
        #expect(up.to.id == "8m")
        #expect(h.reported.last == 8_000)
        let original = try #require(h.play(4 * 60_000, bandwidth: 20_000))
        #expect(original.to.original)
        #expect(h.reported.last == .some(nil))
        // Automatic picked again.
        h.reported.removeAll()
        h.auto.forgetCeiling()
        #expect(h.reported == [nil])
        #expect(h.memory.read() == nil)
    }

    @Test func aCeilingFromTheFutureIsFromBeforeTheDeviceRestartedAndForgotten() {
        let h = Sim()
        h.memory.write(kbps: 4_000, atMs: h.now + 60_000)
        #expect(h.begin(h.hd) == Qualities.original)
        #expect(h.memory.read() == nil)
    }

    @Test func aCeilingUnderEveryQualityStartsAtTheLowest() {
        let h = Sim()
        h.memory.write(kbps: 1_000, atMs: h.now)
        #expect(h.begin(h.uhd).id == "2m")
    }

    @Test func stoppedItWatchesNothing() {
        let h = Sim()
        h.started(h.hd)
        h.auto.stop()
        #expect(!h.auto.active)
        #expect(h.play(10_000, waiting: true) == nil)
        #expect(h.auto.deviceFellBehind(nowMs: h.now) == nil)
        #expect(h.auto.serverTooSlow(nowMs: h.now) == nil)
    }
}
