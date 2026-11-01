import Foundation
import PleyCaps
import PleyModel
import PleyNetwork
@testable import PleyPlayer
import PleyTesting
import Testing

private func fixtureItem(_ path: String) throws -> ItemDetail {
    try PleyJSON.decoder.decode(ItemDetail.self, from: Fixtures.data(path))
}

@Suite("The play request follows Android's rules")
struct PlanTests {
    @Test func whereItStarts() throws {
        let movie = try fixtureItem("items/movie_detail.json")
        #expect(PlaybackPlan.startMs(movie, PlayRequest(itemId: 101)) == 2_412_000)
        #expect(PlaybackPlan.startMs(movie, PlayRequest(itemId: 101, fromStart: true)) == 0)
        #expect(PlaybackPlan.startMs(movie, PlayRequest(itemId: 101, startMs: 60_000)) == 60_000)
        let watched = try fixtureItem("items/episode_detail.json")
        #expect(PlaybackPlan.startMs(watched, PlayRequest(itemId: 203)) == 0)
    }

    @Test func theFileThatPlaysBestHere() throws {
        let movie = try fixtureItem("items/movie_detail.json")
        // The 4K file is Dolby Vision 8.1: as it is on a DV screen, so it wins by its height.
        let dv = CapsBuilder.build(try recordedProbe("tv_4k_dv")).report
        #expect(PlaybackPlan.file(movie, requested: nil, caps: dv)?.id == 501)
        // The Apple TV HD decodes no HEVC: the 1080p H.264 version plays as it is.
        let hd = CapsBuilder.build(try recordedProbe("tv_hd")).report
        #expect(PlaybackPlan.file(movie, requested: nil, caps: hd)?.id == 502)
        #expect(CapsPolicy.decide(movie.files[0], hd) == .serverToneMaps)
        // Asked for: that one, whatever plays best.
        #expect(PlaybackPlan.file(movie, requested: 501, caps: hd)?.id == 501)
        #expect(PlaybackPlan.file(movie, requested: nil, caps: nil)?.id == 501)
    }

    @Test func theBodyAsksForKeyedURLsAndNamesTracksOnlyWhenNeeded() throws {
        let movie = try fixtureItem("items/movie_detail.json")
        let caps = CapsBuilder.build(try recordedProbe("tv_4k_dv")).report
        let file = movie.files[0]
        let plain = PlaybackPlan.request(itemId: 101, .init(file: file, startMs: 5_000, streaming: true),
                                         clientId: "apple-1", caps: caps)
        #expect(plain.keyedUrls == true)
        #expect(plain.fileId == 501)
        #expect(plain.startMs == 5_000)
        #expect(plain.mode == .auto)
        #expect(plain.clientId == "apple-1")
        #expect(plain.audioIndex == nil)
        #expect(plain.subtitleIndex == nil)
        #expect(plain.keepPrevious == nil)
        #expect(plain.caps == caps)

        // A picked track goes when the session streams.
        let commentary = file.streams(.audio)[1]
        let picked = PlaybackPlan.request(itemId: 101, .init(file: file, startMs: 0, audio: commentary, streaming: true),
                                          clientId: "apple-1", caps: caps)
        #expect(picked.audioIndex == 2)
        // TrueHD doesn't play here: named even for a file as it is.
        let trueHD = file.streams(.audio)[0]
        #expect(!PlaybackPlan.audioPlaysHere(trueHD, caps: caps))
        let direct = PlaybackPlan.request(itemId: 101, .init(file: file, startMs: 0, audio: trueHD), clientId: "a",
                                          caps: caps)
        #expect(direct.audioIndex == 1)
        // The details' own AAC on the MP4, played as it is: left to the server.
        let mp4 = movie.files[1]
        let aac = PlaybackPlan.request(itemId: 101, .init(file: mp4, startMs: 0, audio: mp4.streams(.audio)[0]),
                                       clientId: "a", caps: caps)
        #expect(aac.audioIndex == nil)

        // Pictures are burned in only into a stream, and never when side-loaded.
        let pgs = file.streams(.subtitle)[0]
        #expect(PlaybackPlan.request(itemId: 101, .init(file: file, startMs: 0, subtitle: pgs, streaming: true),
                                     clientId: "a", caps: caps).subtitleIndex == 3)
        #expect(PlaybackPlan.request(itemId: 101, .init(file: file, startMs: 0, subtitle: pgs, streaming: true,
                                                        sideload: true), clientId: "a", caps: caps).subtitleIndex == nil)
        #expect(PlaybackPlan.request(itemId: 101, .init(file: file, startMs: 0, subtitle: file.streams(.subtitle)[1],
                                                        streaming: true), clientId: "a", caps: caps).subtitleIndex == nil)
        #expect(PlaybackPlan.request(itemId: 101, .init(file: file, startMs: 0), clientId: "a", caps: caps,
                                     keepPrevious: true).keepPrevious == true)
    }

    @Test func whatStreamsBeforeAsking() throws {
        let movie = try fixtureItem("items/movie_detail.json")
        let caps = CapsBuilder.build(try recordedProbe("tv_4k_dv")).report
        #expect(PlaybackPlan.streams(mode: .auto, decision: .direct, file: movie.files[0], audio: nil, caps: caps))
        #expect(!PlaybackPlan.streams(mode: .auto, decision: .direct, file: movie.files[1], audio: nil, caps: caps))
        #expect(PlaybackPlan.streams(mode: .transcode, decision: .direct, file: movie.files[1], audio: nil, caps: caps))
    }

    @Test func aStreamWithTheWrongAudioIsAskedAgain() throws {
        let movie = try fixtureItem("items/movie_detail.json")
        let file = movie.files[0]
        var info = PlaybackInfo(sessionId: "a", method: .remux, url: "/api/media/k/master.m3u8", hls: true, fileId: 501)
        info.audioIndex = 1
        let choices = PlaybackPlan.Choices(file: file, startMs: 0, audio: file.streams(.audio)[1])
        // Sent as a file played as it is, with the track left to the server, which chose another.
        var sent = PlaybackPlan.request(itemId: 101, choices, clientId: "a", caps: nil)
        sent.audioIndex = nil
        #expect(PlaybackPlan.needsIndexes(info, sent: sent, choices))
        info.audioIndex = 2
        #expect(!PlaybackPlan.needsIndexes(info, sent: sent, choices))
        info.audioIndex = 1
        info.hls = false
        #expect(!PlaybackPlan.needsIndexes(info, sent: sent, choices))
    }

    @Test func theClientIdIsKeptPerInstall() {
        let defaults = UserDefaults(suiteName: "pley-test-\(UUID().uuidString)")!
        let first = ClientID.current(defaults)
        #expect(first.hasPrefix("apple-"))
        #expect(first.count == 42)
        #expect(ClientID.current(defaults) == first)
    }
}

@Suite("Reports go out on the web and Android players' cadence")
struct ScheduleTests {
    @Test func theCadence() {
        var schedule = ProgressSchedule()
        // Nothing before the first frame.
        #expect(schedule.due(at: 50_000) == nil)
        #expect(schedule.firstFrame(at: 0) == .playing)
        #expect(schedule.firstFrame(at: 1) == nil)
        #expect(schedule.due(at: 9_999) == nil)
        #expect(schedule.due(at: 10_000) == .playing)
        #expect(schedule.due(at: 15_000) == nil)
        #expect(schedule.due(at: 20_000) == .playing)
        // A pause reports at once, then every 30 s.
        #expect(schedule.wantsToPlay(false, at: 21_000) == .paused)
        #expect(schedule.wantsToPlay(false, at: 21_500) == nil)
        #expect(schedule.due(at: 50_999) == nil)
        #expect(schedule.due(at: 51_000) == .paused)
        #expect(schedule.due(at: 81_000) == .paused)
        // Playing again: 10 s from then.
        #expect(schedule.wantsToPlay(true, at: 82_000) == nil)
        #expect(schedule.due(at: 91_999) == nil)
        #expect(schedule.due(at: 92_000) == .playing)
    }

    @Test func aSeekReportsASecondLater() {
        var schedule = ProgressSchedule()
        _ = schedule.firstFrame(at: 0)
        schedule.sought(at: 3_000)
        schedule.sought(at: 3_400)
        #expect(schedule.due(at: 4_000) == nil)
        #expect(schedule.due(at: 4_400) == .playing)
        // The 10 s count from that report.
        #expect(schedule.due(at: 14_399) == nil)
        #expect(schedule.due(at: 14_400) == .playing)
        _ = schedule.wantsToPlay(false, at: 15_000)
        schedule.sought(at: 16_000)
        #expect(schedule.due(at: 17_000) == .paused)
    }

    @Test func aNewSessionReportsItsFirstFrameAgain() {
        var schedule = ProgressSchedule()
        _ = schedule.firstFrame(at: 0)
        _ = schedule.wantsToPlay(false, at: 1_000)
        schedule.restart()
        #expect(schedule.due(at: 100_000) == nil)
        #expect(schedule.firstFrame(at: 100_000) == .paused)
    }
}

@Suite("The recovery ladder")
struct RecoveryTests {
    @Test func directPlayFallsBackToARemuxThenATranscode() {
        var recovery = Recovery()
        let audio = EngineError.unsupported(detail: "AVFoundationErrorDomain -11829", audio: true, container: false)
        #expect(recovery.decide(audio, method: .direct, videoCodec: "h264") == .newSession(mode: .remux, reason: .unplayable))
        #expect(recovery.decide(audio, method: .remux, videoCodec: "h264")
            == .newSession(mode: .transcode, reason: .unplayable))
        #expect(recovery.decide(audio, method: .transcode, videoCodec: "h264") == .fail(.unplayable, detail: audio.detail))
        #expect(recovery.fallbackMode == .transcode)
    }

    @Test func aVideoThatDoesntDecodeGoesStraightToATranscode() {
        var recovery = Recovery()
        let video = EngineError.unsupported(detail: "decode", audio: false, container: false)
        #expect(recovery.decide(video, method: .direct, videoCodec: "h264")
            == .newSession(mode: .transcode, reason: .unplayable))
        var mpeg2 = Recovery()
        let container = EngineError.unsupported(detail: "format", audio: false, container: true)
        #expect(mpeg2.decide(container, method: .direct, videoCodec: "mpeg2video")
            == .newSession(mode: .transcode, reason: .unplayable))
    }

    @Test func expiredSessionsAreMadeAgainThreeTimes() {
        var recovery = Recovery()
        for _ in 0..<3 {
            #expect(recovery.decide(.http(status: 404, detail: "x"), method: .remux, videoCodec: "h264")
                == .newSession(mode: nil, reason: nil))
        }
        #expect(recovery.decide(.playlist(detail: "x"), method: .remux, videoCodec: "h264") == .fail(.stopped, detail: "x"))
        // 5 s of playback: the budget starts over.
        recovery.onPosition(10_000)
        recovery.onPosition(15_000)
        #expect(recovery.decide(.http(status: 410, detail: "x"), method: .remux, videoCodec: "h264")
            == .newSession(mode: nil, reason: nil))
        recovery.retry()
        #expect(recovery.recreate == 0)
    }

    @Test func networkErrorsAndOthers() {
        var recovery = Recovery()
        for _ in 0..<3 {
            #expect(recovery.decide(.network(detail: "n"), method: .direct, videoCodec: "h264")
                == .newSession(mode: nil, reason: nil))
        }
        #expect(recovery.decide(.network(detail: "n"), method: .direct, videoCodec: "h264") == .fail(.network, detail: "n"))
        #expect(recovery.decide(.http(status: 500, detail: "s"), method: .direct, videoCodec: "h264")
            == .fail(.network, detail: "s"))
        #expect(recovery.decide(.other(detail: "o"), method: .direct, videoCodec: "h264") == .fail(.engine, detail: "o"))
    }

    @Test func stalls() {
        var direct = Recovery()
        #expect(direct.onStall(method: .direct, starving: false) == .newSession(mode: .transcode, reason: .notSmooth))
        var stream = Recovery()
        #expect(stream.onStall(method: .transcode, starving: true) == .checkServer)
        #expect(stream.afterServerCheck(speed: 0.6, starving: true) == .suggestLowerQuality)
        #expect(stream.afterServerCheck(speed: 2, starving: true) == .newSession(mode: nil, reason: nil))
        #expect(stream.afterServerCheck(speed: nil, starving: false) == .newSession(mode: nil, reason: nil))
        #expect(stream.afterServerCheck(speed: nil, starving: false) == .fail(.stuck, detail: nil))
    }

    @Test func aRemuxWithoutPictureBecomesATranscodeOnce() {
        var recovery = Recovery()
        #expect(recovery.onNoVideo(method: .direct) == nil)
        #expect(recovery.onNoVideo(method: .remux) == .newSession(mode: .transcode, reason: .unplayable))
        #expect(recovery.onNoVideo(method: .remux) == nil)
        recovery.newItem()
        #expect(recovery.fallbackMode == nil)
        #expect(recovery.onNoVideo(method: .remux) != nil)
    }

    @Test func avFoundationsErrorsAreRead() {
        let notFound = NSError(domain: "CoreMediaErrorDomain", code: -12938)
        let wrapped = NSError(domain: AVErrorDomainName.name, code: -11800, userInfo: [NSUnderlyingErrorKey: notFound])
        #expect(AVPlayerEngine.classify(wrapped, httpStatus: nil, hls: true) == .http(status: 404, detail:
            "AVFoundationErrorDomain -11800 / CoreMediaErrorDomain -12938"))
        #expect(AVPlayerEngine.classify(NSError(domain: "x", code: 1), httpStatus: 410, hls: true)
            == .http(status: 410, detail: "x 1"))
        let offline = NSError(domain: NSURLErrorDomain, code: NSURLErrorNotConnectedToInternet)
        #expect(AVPlayerEngine.classify(offline, httpStatus: nil, hls: false) == .network(detail: "NSURLErrorDomain -1009"))
        let format = NSError(domain: AVErrorDomainName.name, code: -11828)
        #expect(AVPlayerEngine.classify(format, httpStatus: nil, hls: false)
            == .unsupported(detail: "AVFoundationErrorDomain -11828", audio: false, container: true))
        #expect(AVPlayerEngine.classify(format, httpStatus: nil, hls: true) == .playlist(detail: "AVFoundationErrorDomain -11828"))
        let decode = NSError(domain: AVErrorDomainName.name, code: -11821)
        #expect(AVPlayerEngine.classify(decode, httpStatus: nil, hls: true)
            == .unsupported(detail: "AVFoundationErrorDomain -11821", audio: false, container: false))
        #expect(AVPlayerEngine.httpStatus(in: "HTTP 410: Gone") == 410)
        #expect(AVPlayerEngine.httpStatus(in: "HTTP 200") == nil)
    }
}

private enum AVErrorDomainName {
    static let name = "AVFoundationErrorDomain"
}

private extension EngineError {
    var detail: String {
        switch self {
        case .unsupported(let detail, _, _), .http(_, let detail), .playlist(let detail), .network(let detail),
             .other(let detail):
            detail
        }
    }
}
