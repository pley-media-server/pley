import Foundation
import PleyModel
@testable import PleyPlayer
import Testing

/// A file of `bitrate` bits a second and `width`×`height`, for the menu.
func mediaFile(bitrate: Int?, width: Int? = nil, height: Int?) -> MediaFile {
    var file = MediaFile(id: 1, path: "a")
    file.bitrate = bitrate
    file.width = width
    file.height = height
    return file
}

/// The menu of docs/QUALITY.md, the cases of the Android app's QualityTest
/// (and apps/web/src/player/quality.test.ts).
@Suite("Qualities")
struct QualityTests {
    private func ids(_ bitrate: Int?, _ height: Int?) -> [String] {
        Qualities.forFile(mediaFile(bitrate: bitrate, height: height)).map(\.id)
    }

    @Test func findsThemByIdFallingBackToTheOriginal() {
        #expect(Qualities.byId("4m").maxBitrateKbps == 4_000)
        #expect(Qualities.byId("4m").maxHeight == 720)
        #expect(Qualities.byId("nope") == Qualities.original)
        #expect(Qualities.byId(nil).id == "original")
        #expect(Qualities.byId("auto") == Qualities.automatic)
        // People say "Original" for the original.
        #expect(Qualities.byIdOrLabel("Original") == Qualities.original)
        #expect(Qualities.byIdOrLabel("auto") == Qualities.automatic)
    }

    @Test func aServerThatConvertsNoVideoIsOfferedTheOriginalAlone() {
        let film = mediaFile(bitrate: 60_000_000, height: 2160)
        #expect(Qualities.offered(film, originalOnly: true) == [Qualities.original])
        #expect(Qualities.offered(nil, originalOnly: true) == [Qualities.original])
        #expect(Qualities.offered(film, originalOnly: false) == Qualities.forFile(film))
    }

    @Test func theMenusOrderIsTheSameOnEveryApp() {
        #expect(Qualities.all.map(\.id) == ["original", "auto", "30m", "15m", "20m", "12m", "8m", "4m", "2m"])
        // 15 Mbps 4K under 30 Mbps 4K, then 1080p and below.
        #expect(Qualities.transcodes.map(\.maxHeight) == [2160, 2160, 1080, 1080, 1080, 720, 480])
        #expect(Qualities.lowest == Qualities.byId("2m"))
    }

    @Test func automaticIsAChoiceThatNeverPlaysAsItself() {
        #expect(Qualities.automatic.automatic)
        #expect(!Qualities.automatic.original)
        #expect(!Qualities.original.automatic)
        #expect(Qualities.automatic.maxBitrateKbps == nil)
    }

    @Test func transcodesEveryQualityButTheOriginalAndAutomatic() {
        #expect(Qualities.byId("original").mode == .auto)
        #expect(Qualities.byId("auto").mode == .auto)
        #expect(Qualities.byId("8m").mode == .transcode)
    }

    @Test func offersOnlyTheQualitiesThatMakeTheFileSmaller() {
        // 1080p at 9 Mbps: 8 Mbps is barely lighter, and plays no better.
        #expect(ids(9_000_000, 1080) == ["original", "auto", "4m", "2m"])
        // 1080p at 10 Mbps: 8 Mbps is 80 %.
        #expect(ids(10_000_000, 1080) == ["original", "auto", "8m", "4m", "2m"])
        #expect(ids(3_000_000, 720) == ["original", "auto", "2m"])
        #expect(ids(1_500_000, 480) == ["original", "auto"])
        // A 4K file at a low bitrate still gets smaller at 1080p.
        #expect(ids(6_000_000, 2160) == ["original", "auto", "20m", "12m", "8m", "4m", "2m"])
        // A 4K remux: 4K at 30 Mbps too.
        #expect(ids(60_000_000, 2160) == ["original", "auto", "30m", "15m", "20m", "12m", "8m", "4m", "2m"])
        // 4K at 20 Mbps: 4K at 15 is 75 %, 4K at 30 isn't smaller.
        #expect(ids(20_000_000, 2160) == ["original", "auto", "15m", "20m", "12m", "8m", "4m", "2m"])
        // A 720p file at 12 Mbps: 8 Mbps and lower.
        #expect(ids(12_000_000, 720) == ["original", "auto", "8m", "4m", "2m"])
        #expect(ids(nil, nil).count == Qualities.all.count)
        #expect(Qualities.forFile(nil).count == Qualities.all.count)
    }

    @Test func aFilesLinesAreTheServersSoAScopeFilmIs1080p() {
        #expect(Qualities.linesClassOf(mediaFile(bitrate: nil, width: 1920, height: 800)) == 1080)
        #expect(Qualities.linesClassOf(mediaFile(bitrate: nil, width: 3840, height: 1600)) == 2160)
        #expect(Qualities.linesClassOf(mediaFile(bitrate: nil, height: 720)) == 720)
        #expect(Qualities.linesClassOf(mediaFile(bitrate: nil, height: nil)) == nil)
        // 1920×800 at 30 Mbps: 20 Mbps 1080p has no fewer lines, but is 67 % of the bitrate; 15 Mbps 4K is
        // 15 Mbps 1080p here, and goes under it.
        let scope = mediaFile(bitrate: 30_000_000, width: 1920, height: 800)
        #expect(Qualities.forFile(scope).map(\.id) == ["original", "auto", "20m", "15m", "12m", "8m", "4m", "2m"])
    }

    @Test func qualitiesThatComeToTheSameBitrateAndLinesShowOnce() {
        #expect(Qualities.linesFor(Qualities.byId("15m"), fileLines: 1080) == 1080)
        #expect(Qualities.linesFor(Qualities.byId("4m"), fileLines: 1080) == 720)
        #expect(Qualities.linesFor(Qualities.original, fileLines: 1080) == 1080)
        let offered = Qualities.forFile(mediaFile(bitrate: 80_000_000, height: 1080))
        let keys = offered.filter { $0.maxBitrateKbps != nil }
            .map { "\($0.maxBitrateKbps!)/\(Qualities.linesFor($0, fileLines: 1080)!)" }
        #expect(Set(keys).count == keys.count)
    }

    @Test func nothingChosenIsAutomaticAndAQualityTheFileIsntOfferedPlaysTheOriginal() {
        let file = mediaFile(bitrate: 5_000_000, height: 1080)
        #expect(Qualities.choose(file, remembered: "4m").id == "4m")
        #expect(Qualities.choose(file, remembered: "8m") == Qualities.original)
        #expect(Qualities.choose(file, remembered: nil) == Qualities.automatic)
        #expect(Qualities.choose(file, remembered: "auto") == Qualities.automatic)
        // The original chosen before stays chosen.
        #expect(Qualities.choose(file, remembered: "original") == Qualities.original)
        #expect(Qualities.choose(file, remembered: "Original") == Qualities.original)
    }

    @Test func namesAQualityTallerThanTheFileByTheFilesLines() {
        #expect(Qualities.label(Qualities.byId("8m")) == "8 Mbps 1080p")
        #expect(Qualities.label(Qualities.byId("30m")) == "30 Mbps 4K")
        #expect(Qualities.label(Qualities.byId("8m"), fileLines: 720) == "8 Mbps 720p")
        #expect(Qualities.label(Qualities.byId("4m"), fileLines: 1080) == "4 Mbps 720p")
        #expect(Qualities.label(Qualities.original, fileLines: 720) == "Original")
        #expect(Qualities.label(Qualities.automatic) == "Automatic")
        #expect(Words.bitrate(8_200_000) == "8.2 Mbps")
        #expect(Words.bitrate(12_000_000) == "12 Mbps")
        #expect(Words.bitrate(640_000) == "640 kbps")
    }
}

/// `StruggleMeter` in a box, so `#expect` can feed it.
private final class Meter {
    private var meter = StruggleMeter()

    @discardableResult
    func sample(atMs: Int64, _ counts: FrameCounts) -> Bool { meter.sample(atMs: atMs, counts) }
    func reset() { meter.reset() }
}

/// The decoder's dropped frames, counted from AVPlayer's access log.
@Suite("Dropped frames")
struct StruggleMeterTests {
    @Test func over1PercentDroppedIn10SecondsStrugglesAndUnderDoesnt() {
        let m = Meter()
        #expect(!m.sample(atMs: 0, FrameCounts(expected: 0, dropped: 0)))
        #expect(!m.sample(atMs: 5_000, FrameCounts(expected: 120, dropped: 2)))
        // 240 frames over 10 s, 2 dropped: under 1 %.
        #expect(!m.sample(atMs: 10_000, FrameCounts(expected: 240, dropped: 2)))
        // The next 10 s: 240 frames, 5 dropped.
        #expect(!m.sample(atMs: 15_000, FrameCounts(expected: 360, dropped: 4)))
        #expect(m.sample(atMs: 20_000, FrameCounts(expected: 480, dropped: 7)))
    }

    @Test func needs10SecondsOfSamples() {
        let m = Meter()
        m.sample(atMs: 0, FrameCounts(expected: 0, dropped: 0))
        #expect(!m.sample(atMs: 4_000, FrameCounts(expected: 100, dropped: 5)))
        #expect(!m.sample(atMs: 8_000, FrameCounts(expected: 200, dropped: 9)))
        #expect(m.sample(atMs: 10_000, FrameCounts(expected: 250, dropped: 9)))
    }

    @Test func tenFramesDroppedBetweenTwoSamplesStruggleAtOnce() {
        let m = Meter()
        m.sample(atMs: 0, FrameCounts(expected: 0, dropped: 3))
        #expect(m.sample(atMs: 2_000, FrameCounts(expected: 48, dropped: 13)))
        // Nine is no burst.
        let n = Meter()
        n.sample(atMs: 0, FrameCounts(expected: 0, dropped: 0))
        #expect(!n.sample(atMs: 2_000, FrameCounts(expected: 48, dropped: 9)))
    }

    @Test func countsThatStartOverWithANewMediaStartTheWindowOver() {
        let m = Meter()
        m.sample(atMs: 0, FrameCounts(expected: 1_000, dropped: 100))
        #expect(!m.sample(atMs: 10_000, FrameCounts(expected: 10, dropped: 0)))
        #expect(!m.sample(atMs: 15_000, FrameCounts(expected: 120, dropped: 0)))
        m.reset()
        #expect(!m.sample(atMs: 20_000, FrameCounts(expected: 240, dropped: 50)))
    }

    @Test func expectedFramesAreTheTimeWatchedAtTheVideosRate() {
        var sample = EngineSample()
        sample.watchedSeconds = 10
        sample.droppedFrames = 3
        #expect(FrameCounts(sample, frameRate: 23.976) == FrameCounts(expected: 239, dropped: 3))
    }
}

/// What is remembered between playbacks: the quality picked per kind of
/// network, and Automatic's ceiling per server (the Android app's
/// PreferencesCeilingsTest).
@Suite("Remembered qualities and ceilings")
struct MemoryTests {
    private func defaults() -> (UserDefaults, String) {
        let name = "pley-tests-\(UUID().uuidString)"
        return (UserDefaults(suiteName: name)!, name)
    }

    @Test func eachServerKeepsItsOwnCeilingReadAgainByTheNextRun() {
        let (store, name) = defaults()
        defer { store.removePersistentDomain(forName: name) }
        DefaultsCeiling(origin: "http://a:7539", defaults: store).write(kbps: 4_000, atMs: 123)
        let again = DefaultsCeiling(origin: "http://a:7539", defaults: UserDefaults(suiteName: name)!)
        #expect(again.read()?.kbps == 4_000)
        #expect(again.read()?.atMs == 123)
        #expect(DefaultsCeiling(origin: "http://b:7539", defaults: store).read() == nil)
        again.forget()
        #expect(DefaultsCeiling(origin: "http://a:7539", defaults: store).read() == nil)
    }

    @Test func aQualityIsRememberedPerKindOfNetwork() {
        let (store, name) = defaults()
        defer { store.removePersistentDomain(forName: name) }
        nonisolated(unsafe) var cellular = false
        let memory = DefaultsQuality(defaults: store, cellular: { cellular })
        #expect(memory.remembered() == nil)
        memory.remember("original")
        cellular = true
        #expect(memory.remembered() == nil)
        memory.remember("4m")
        #expect(memory.remembered() == "4m")
        cellular = false
        #expect(memory.remembered() == "original")
    }
}
