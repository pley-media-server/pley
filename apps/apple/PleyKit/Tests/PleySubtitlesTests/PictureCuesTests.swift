import Foundation
@testable import PleySubtitles
import Testing

/// Picture subtitles as the player draws them: decoded by `PictureRenderer`,
/// away from the main actor, and looked up by `PictureCueTrack`.
@Suite("Picture subtitles: decoded away from the main actor")
struct PictureCuesTests {
    /// A picture from 1 s, cleared at 3 s; another from 5 s, for 1.5 s.
    let pgs = PictureTrack(codecId: "S_HDMV/PGS", codecPrivate: nil, samples: [
        PictureSample(timeMs: 1_000, durationMs: nil, data: pgsDisplaySet(state: 0x80, objects: true, rle: rle2x4)),
        PictureSample(timeMs: 3_000, durationMs: nil, data: pgsDisplaySet(state: 0, objects: false)),
        PictureSample(timeMs: 5_000, durationMs: 1_500,
                      data: pgsDisplaySet(state: 0x80, objects: true, x: 60, rle: rle2x4)),
    ])

    /// Two VobSub pictures, each stopping by its own command.
    let vobSub = PictureTrack(codecId: "S_VOBSUB", codecPrivate: PicturesTests.idx, samples: [
        PictureSample(timeMs: 1_000, durationMs: nil, data: PicturesTests.spu()),
        PictureSample(timeMs: 10_000, durationMs: nil, data: PicturesTests.spu()),
    ])

    /// Decodes what `cues` wants, as the player does.
    private func decodeWanted(_ cues: PictureCueTrack, _ renderer: PictureRenderer) async {
        for index in cues.wanted {
            cues.asking(index)
            cues.decoded(index, await renderer.decode(index))
        }
    }

    @Test func aPictureShowsOnceDecodedAndTheNextOneIsDecodedAhead() async throws {
        let cues = PictureCueTrack(pgs, id: "t")
        let renderer = try #require(PictureRenderer(pgs))
        #expect(cues.shown(at: 1_500) == .none)
        #expect(cues.wanted == [0, 1])
        #expect(cues.nextChangeAfter(1_500) == 3_000)
        await decodeWanted(cues, renderer)
        #expect(cues.wanted.isEmpty)
        guard case .picture(let shown) = cues.shown(at: 1_500) else {
            Issue.record("no picture")
            return
        }
        #expect(shown.id == "t:0")
        #expect(shown.frame.bitmaps.map { [$0.x, $0.y] } == [[100, 900]])
        // The clear was decoded ahead: it shows at once, and the next picture is wanted.
        #expect(cues.shown(at: 3_200) == .none)
        #expect(cues.wanted == [2])
    }

    @Test func onceDecodedTheTimelineIsTheDecodersOwn() async throws {
        for track in [pgs, vobSub] {
            let subs = try #require(PictureSubtitles.of(track))
            let cues = PictureCueTrack(track)
            let renderer = try #require(PictureRenderer(track))
            for ms in [500, 1_500, 3_200, 4_000, 5_100, 6_000, 6_600, 9_000, 10_500, 99_000] {
                _ = cues.shown(at: ms)
                await decodeWanted(cues, renderer)
                var index: Int?
                if case .picture(let shown) = cues.shown(at: ms) { index = Int(shown.id.split(separator: ":")[1]) }
                #expect(index == subs.frameAt(ms)?.index, "\(track.codecId) at \(ms)")
                #expect(cues.nextChangeAfter(ms) == subs.nextChange(ms), "\(track.codecId) after \(ms)")
            }
        }
    }

    @Test func aTrackReadFurtherKeepsWhatIsDecodedAndEndsItAtTheNextSample() async throws {
        let first = PictureTrack(codecId: pgs.codecId, codecPrivate: nil, samples: [pgs.samples[0]])
        let cues = PictureCueTrack(first)
        let renderer = try #require(PictureRenderer(first))
        _ = cues.shown(at: 1_500)
        await decodeWanted(cues, renderer)
        #expect(cues.nextChangeAfter(1_500) == nil)

        await renderer.grow(pgs)
        cues.grow(pgs)
        #expect(cues.count == 3)
        #expect(cues.shown(at: 1_500) != .none)
        #expect(cues.wanted == [1])
        #expect(cues.nextChangeAfter(1_500) == 3_000)
        // A shorter answer, come late, takes nothing away.
        cues.grow(first)
        #expect(cues.count == 3)
    }

    @Test func onlyTheSamplesNearThePlayheadStayDecoded() async throws {
        let cues = PictureCueTrack(pgs)
        let renderer = try #require(PictureRenderer(pgs))
        _ = cues.shown(at: 1_500)
        await decodeWanted(cues, renderer)
        _ = cues.shown(at: 5_100)
        await decodeWanted(cues, renderer)
        // Back to the start: the first picture was let go, the clear was kept.
        #expect(cues.shown(at: 1_500) == .none)
        #expect(cues.wanted == [0])
    }

    @Test func aDecodeCancelledBeforeItBeginsIsWantedAgain() async throws {
        let cues = PictureCueTrack(pgs)
        let renderer = try #require(PictureRenderer(pgs))
        _ = cues.shown(at: 1_500)
        cues.asking(0)
        #expect(!cues.wanted.contains(0))
        let answer = await Task { () -> DecodedPicture? in
            withUnsafeCurrentTask { $0?.cancel() }
            return await renderer.decode(0)
        }.value
        #expect(answer == nil)
        cues.decoded(0, answer)
        #expect(cues.wanted.contains(0))
    }

    @Test func aCodecThisAppDoesntDrawHasNoRenderer() {
        #expect(PictureRenderer(PictureTrack(codecId: "S_TEXT/ASS", codecPrivate: nil, samples: [])) == nil)
    }
}
