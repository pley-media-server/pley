import Foundation
import PleyCommon
import PleyModel
import PleyNetwork
@testable import PleySubtitles
import PleyTesting
import Testing

@Suite("WebVTT, text cues and subtitle sync")
struct TextSubtitlesTests {
    static let vtt = """
    WEBVTT
    X-TIMESTAMP-MAP=MPEGTS:0,LOCAL:00:00:00.000

    NOTE a comment --> that isn't a cue

    STYLE
    ::cue { color: yellow }

    1
    00:00:02.000 --> 00:00:05.000
    The <i>first</i> line &amp; <b>more</b>.

    00:08.500 --> 00:11.000 line:0 position:50%
    <v Narrator>At the <u>top</u>,</v>
    on <c.yellow>two</c> lines

    3
    00:00:10.000 --> 00:00:12.000 line:-1
    Overlapping &lt;3

    broken
    00:00:20.000 --> 00:00:19.000
    Ends before it starts.

    00:01:00.000 --> 00:01:02.5
    A minute in.
    """

    @Test func readsCuesTheirTimesTextAndPlace() throws {
        let cues = WebVTT.parse(Self.vtt)
        #expect(cues.map(\.startMs) == [2_000, 8_500, 10_000, 60_000])
        #expect(cues.map(\.endMs) == [5_000, 11_000, 12_000, 62_500])
        #expect(cues.map(\.plain) == ["The first line & more.", "At the top,\non two lines", "Overlapping <3",
                                      "A minute in."])
        #expect(cues.map(\.top) == [false, true, false, false])
    }

    @Test func keepsItalicBoldAndUnderline() throws {
        let cues = WebVTT.parse(Self.vtt)
        let first = cues[0].text
        let italic = first.runs.filter { $0.inlinePresentationIntent?.contains(.emphasized) == true }
            .map { String(first[$0.range].characters) }
        let bold = first.runs.filter { $0.inlinePresentationIntent?.contains(.stronglyEmphasized) == true }
            .map { String(first[$0.range].characters) }
        #expect(italic == ["first"])
        #expect(bold == ["more"])
        let second = cues[1].text
        let underlined = second.runs.filter { $0[SubtitleUnderline.self] == true }
            .map { String(second[$0.range].characters) }
        #expect(underlined == ["top"])
    }

    @Test func readsCarriageReturnsAndAByteOrderMark() {
        let cues = WebVTT.parse("\u{FEFF}WEBVTT\r\n\r\n00:00:01.000 --> 00:00:02.000\r\nHi\r\n")
        #expect(cues.map(\.plain) == ["Hi"])
        #expect(WebVTT.parse("").isEmpty)
        #expect(WebVTT.time("1:02:03.4") == 3_723_400)
        #expect(WebVTT.time("00:61.000") == nil)
        // From a file anyone can add: nothing reads past its parts, nothing
        // overflows.
        #expect(WebVTT.time("00:.") == nil)
        #expect(WebVTT.time("00:01.") == 1_000)
        #expect(WebVTT.time("99999999999999999:00.000") == nil)
        #expect(WebVTT.time("99999:59:59.999") == 99_999 * 3_600_000 + 3_599_999)
        #expect(WebVTT.parse("WEBVTT\n\n00:. --> 00:01.000\nboom\n").isEmpty)
    }

    @Test func findsTheCuesShowingAndWhenThatChanges() {
        let track = TextCueTrack(WebVTT.parse(Self.vtt))
        #expect(track.shown(at: 1_999) == .none)
        #expect(track.cuesAt(2_000).map(\.startMs) == [2_000])
        #expect(track.cuesAt(5_000).isEmpty)
        #expect(track.cuesAt(10_500).map(\.startMs) == [8_500, 10_000])
        #expect(track.nextChangeAfter(0) == 2_000)
        #expect(track.nextChangeAfter(3_000) == 5_000)
        #expect(track.nextChangeAfter(9_000) == 10_000)
        #expect(track.nextChangeAfter(10_500) == 11_000)
        #expect(track.nextChangeAfter(61_000) == 62_500)
        #expect(track.nextChangeAfter(62_500) == nil)
    }

    @Test func aLongCueEarlierStillShows() {
        let long = TextCue(startMs: 0, endMs: 100_000, text: "Long")
        let short = TextCue(startMs: 1_000, endMs: 2_000, text: "Short")
        let later = TextCue(startMs: 50_000, endMs: 51_000, text: "Later")
        let track = TextCueTrack([later, short, long])
        #expect(track.cuesAt(50_500).map(\.plain) == ["Long", "Later"])
        #expect(track.nextChangeAfter(3_000) == 50_000)
        #expect(track.nextChangeAfter(51_000) == 100_000)
    }

    @Test func aLaterAnswerOnlyReplacesWhenItReachesFurther() {
        let some = TextCueTrack([TextCue(startMs: 0, endMs: 1_000, text: "a")])
        let more = TextCueTrack([TextCue(startMs: 0, endMs: 1_000, text: "a"),
                                 TextCue(startMs: 2_000, endMs: 3_000, text: "b")])
        #expect(some.merge(more) === more)
        #expect(more.merge(some) === more)
    }

    @Test func syncStepsLimitsAndLabels() {
        #expect(SubtitleSync.clamp(149) == 100)
        #expect(SubtitleSync.clamp(-151) == -200)
        #expect(SubtitleSync.clamp(12_345) == 10_000)
        #expect(SubtitleSync.clamp(-20_000) == -10_000)
        #expect(SubtitleSync.label(500) == "+0.5 s")
        #expect(SubtitleSync.label(-1_200) == "\u{2212}1.2 s")
        #expect(SubtitleSync.label(0) == "0.0 s")
        // Later by 0.5 s: the cue made for 2 s shows at 2.5 s.
        let track = TextCueTrack([TextCue(startMs: 2_000, endMs: 3_000, text: "x")])
        #expect(SubtitleSync.trackTime(2_500, offsetMs: 500) == 2_000)
        #expect(SubtitleSync.untilChange(track, positionMs: 2_000, offsetMs: 500) == 500)
        #expect(SubtitleSync.untilChange(track, positionMs: 4_000, offsetMs: 500) == nil)
        #expect(SubtitleSync.key(origin: "http://nas.local:7712", fileId: 7, profileId: 2)
            == "http://nas.local:7712/api/files/7#profile=2")
    }

    @Test func offsetsAreRememberedPerKeyAndZeroForgets() throws {
        let suite = "pley-subtitle-offsets-\(UUID().uuidString)"
        let defaults = try #require(UserDefaults(suiteName: suite))
        defer { defaults.removePersistentDomain(forName: suite) }
        let offsets = DefaultsSubtitleOffsets(defaults: defaults)
        #expect(offsets.offset("a") == 0)
        offsets.remember("a", offsetMs: 500)
        offsets.remember("b", offsetMs: -300)
        #expect(DefaultsSubtitleOffsets(defaults: defaults).offset("a") == 500)
        #expect(offsets.offset("b") == -300)
        offsets.remember("a", offsetMs: 0)
        #expect(offsets.offset("a") == 0)
        #expect((defaults.dictionary(forKey: DefaultsSubtitleOffsets.key) ?? [:]).keys.sorted() == ["b"])
        let memory = MemorySubtitleOffsets()
        memory.remember("c", offsetMs: 100)
        #expect(memory.offset("c") == 100)
    }
}

@Suite("Fetching subtitle tracks")
struct SubtitleFetchTests {
    private func client(_ server: FixtureServer) -> APIClient {
        let transport = HTTPTransport(userAgent: "pleyApple/9.9.9 (macOS 15.2; Mac)")
        return APIClient(origin: server.origin, tokens: nil, transport: transport)
    }

    @Test func readsWebVTTAndWhetherItIsComplete() async throws {
        let server = FixtureServer()
        _ = try await server.start()
        defer { server.stop() }
        let path = SubtitleFetch.textPath(fileId: 7, index: 1000)
        server.serveFile(path: path, type: "text/vtt; charset=utf-8", body: Data(TextSubtitlesTests.vtt.utf8),
                         headers: ["X-Pley-Complete": "0"])
        let partial = try await SubtitleFetch.text(client(server), path: path)
        #expect(partial.track.cues.count == 4)
        #expect(!partial.complete)
        server.serveFile(path: path, type: "text/vtt", body: Data(TextSubtitlesTests.vtt.utf8))
        #expect(try await SubtitleFetch.text(client(server), path: path).complete)
        // Anything but WebVTT is refused.
        server.serveFile(path: path, type: "text/html", body: Data("<p>".utf8))
        await #expect(throws: PleyError.self) { _ = try await SubtitleFetch.text(client(server), path: path) }
        #expect(SubtitleFetch.isTrackPath("/api/files/7/subtitles/3.vtt"))
        #expect(!SubtitleFetch.isTrackPath("/api/files/7/../../users"))
        #expect(!SubtitleFetch.isTrackPath("/api/media/key/file.mp4"))
    }

    @Test func downloadsPicturesAndAsksOnlyForTheRestOfAPartialFile() async throws {
        let server = FixtureServer()
        _ = try await server.start()
        defer { server.stop() }
        let set = pgsDisplaySet(state: 0x80, objects: true, rle: rle2x4)
        let whole = mks("S_HDMV/PGS", [Block(time: 1000, data: set), Block(time: 3000, data: set)])
        let cut = Array(whole.prefix(whole.count - set.count / 2))
        let path = SubtitleFetch.picturesPath(fileId: 7, index: 3)
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent("pley-mks-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: folder) }
        let file = folder.appendingPathComponent("7-3.mks")

        server.serveFile(path: path, type: "video/x-matroska", body: Data(cut), headers: ["X-Pley-Complete": "0"])
        let first = try await SubtitleFetch.pictures(client(server), path: path, file: file)
        #expect(!first.complete)
        #expect(first.track.samples.map(\.timeMs) == [1000])

        server.serveFile(path: path, type: "video/x-matroska", body: Data(whole))
        let second = try await SubtitleFetch.pictures(client(server), path: path, file: file)
        #expect(second.complete)
        #expect(second.track.samples.map(\.timeMs) == [1000, 3000])
        #expect(try Data(contentsOf: file) == Data(whole))
        #expect(server.requests(path).map { $0.headers["range"] } == [nil, "bytes=\(cut.count)-"])

        // Nothing new: the file is as it was, and complete.
        let third = try await SubtitleFetch.pictures(client(server), path: path, file: file)
        #expect(third.complete)
        #expect(third.track.samples.count == 2)
    }

    @Test func aPictureDownloadIsBoundedAndChecked() async throws {
        let server = FixtureServer()
        _ = try await server.start()
        defer { server.stop() }
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent("pley-mks-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: folder) }
        let file = folder.appendingPathComponent("x.mks")
        server.serveFile(path: "/api/files/1/subtitles/3.mks", type: "video/x-matroska", body: Data(count: 2_048))
        await #expect(throws: PleyError.tooLarge) {
            _ = try await client(server).download("/api/files/1/subtitles/3.mks", to: file, maxBytes: 1_024,
                                                  accept: ["video/x-matroska"])
        }
        #expect(!FileManager.default.fileExists(atPath: file.path))
        server.serveFile(path: "/api/files/1/subtitles/4.mks", type: "text/html", body: Data(count: 16))
        await #expect(throws: PleyError.self) {
            _ = try await client(server).download("/api/files/1/subtitles/4.mks", to: file, maxBytes: 1_024,
                                                  accept: ["video/x-matroska"])
        }
        #expect(!FileManager.default.fileExists(atPath: file.path))
        // 404 for a text track.
        await #expect(throws: PleyError.self) {
            _ = try await SubtitleFetch.pictures(client(server), path: "/api/files/1/subtitles/9.mks", file: file)
        }
    }
}
