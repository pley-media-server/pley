import Foundation
import PleyModel
import PleyNetwork
@testable import PleyPlayer
import PleySubtitles
import PleyTesting
import Testing

/// A 90 s movie, as make-test-media.sh's file 3 has its tracks: English
/// SRT (2) and PGS (3) in the file, and an SRT beside it (1000).
@MainActor
private func subtitleRig(method: String = "direct", selected: Int? = 2,
                         offsets: any SubtitleOffsets = MemorySubtitleOffsets()) async throws -> Rig {
    let server = FixtureServer()
    try await server.start()
    var movie = FixtureServer.Playable(itemId: 51, fileId: 510, title: "Subtitled", durationMs: 90_000)
    movie.audio = [MediaStream(index: 1, kind: .audio, codec: "aac", language: "eng", default: true)]
    var srt = MediaStream(index: 2, kind: .subtitle, codec: "subrip", language: "eng")
    srt.textBased = true
    var pgs = MediaStream(index: 3, kind: .subtitle, codec: "hdmv_pgs_subtitle", language: "eng")
    pgs.textBased = false
    var beside = MediaStream(index: 1000, kind: .subtitle, codec: "subrip", language: "fre")
    beside.textBased = true
    beside.external = true
    movie.subtitles = [srt, pgs, beside]
    movie.selectedSubtitle = selected
    server.addPlayable(movie)
    server.setPlaybackMethod(method)
    server.serveFile(path: "/api/files/510/subtitles/2.vtt", type: "text/vtt", body: Data("""
    WEBVTT

    00:00:02.000 --> 00:00:05.000
    The first line.

    00:00:08.000 --> 00:00:11.000
    The second line.
    """.utf8))
    server.serveFile(path: "/api/files/510/subtitles/1000.vtt", type: "text/vtt", body: Data("""
    WEBVTT

    00:00:02.000 --> 00:00:05.000
    La première ligne.
    """.utf8))
    return try Rig(server: server, request: PlayRequest(itemId: 51), warmer: NoWarmer(), offsets: offsets)
}

/// The `.mks` of the PGS: a 4×2 picture from 1 s, cleared at 3 s.
private let pgsTrack = Data(SubtitleSamples.mks("S_HDMV/PGS", [
    SubtitleSamples.Block(time: 1_000, data: SubtitleSamples.pgsDisplaySet(state: 0x80, objects: true, x: 30, y: 40,
                                                                           rle: SubtitleSamples.rle2x4)),
    SubtitleSamples.Block(time: 3_000, data: SubtitleSamples.pgsDisplaySet(state: 0, objects: false)),
]))

@MainActor
private func started(_ rig: Rig) async throws {
    rig.controller.start()
    try await until("the media loaded") { rig.engine.loads.count == 1 }
    rig.engine.emit(.ready)
    try await until("the first frame") { rig.controller.state.phase == .playing }
}

/// Plays on at `positionMs`, 500 ms later.
@MainActor
private func at(_ rig: Rig, _ positionMs: Int) {
    rig.engine.playing(at: positionMs)
    rig.clock.advance(ms: 500)
    rig.controller.tick()
}

private func texts(_ cues: ShownSubtitles) -> [String] {
    if case .text(let shown) = cues { return shown.map(\.plain) }
    return []
}

private func bodies(_ server: FixtureServer) -> [[String: Any]] {
    server.streamsChoices.map { (try? JSONSerialization.jsonObject(with: $0)) as? [String: Any] ?? [:] }
}

@Suite("Subtitles in the controller")
@MainActor
struct SubtitleControllerTests {
    @Test func theProfilesSubtitlesShowFromTheServersWebVTT() async throws {
        let rig = try await subtitleRig()
        defer { rig.cleanUp() }
        try await started(rig)
        let state = rig.controller.state
        #expect(state.subtitles.map(\.id) == ["off", "2", "3", "1000"])
        #expect(state.subtitles.map(\.label) == ["Off", "English (SRT)", "English (PGS)", "French (SRT, External)"])
        #expect(state.subtitles.first(where: \.selected)?.id == "2")
        #expect(state.subtitleLabel == "English (SRT)")
        #expect(state.canSyncSubtitles)
        try await until("the first line") {
            at(rig, 2_500)
            return texts(rig.controller.state.cues) == ["The first line."]
        }
        at(rig, 6_000)
        #expect(rig.controller.state.cues == .none)
        at(rig, 9_000)
        #expect(texts(rig.controller.state.cues) == ["The second line."])
        // Nothing was asked of the server for them but their WebVTT.
        let sent = await rig.requests()
        #expect(sent.count == 1)
        #expect(sent.first?.subtitleIndex == nil)
        await rig.controller.close()
    }

    @Test func theSyncMovesThemAndIsRememberedForTheFileAndProfile() async throws {
        let offsets = MemorySubtitleOffsets()
        let rig = try await subtitleRig(offsets: offsets)
        defer { rig.cleanUp() }
        try await started(rig)
        try await until("the WebVTT") {
            at(rig, 2_500)
            return !rig.controller.state.cues.isEmpty
        }
        at(rig, 6_000)
        #expect(rig.controller.state.cues == .none)
        // 3 s later: the line made for 2 s to 5 s shows from 5 s to 8 s.
        rig.controller.setSubtitleOffset(3_040)
        #expect(rig.controller.state.subtitleOffsetMs == 3_000)
        #expect(rig.controller.state.subtitleOffsetLabel == "+3.0 s")
        #expect(texts(rig.controller.state.cues) == ["The first line."])
        let key = SubtitleSync.key(origin: rig.server.origin, fileId: 510, profileId: 2)
        #expect(offsets.offset(key) == 3_000)
        rig.controller.setSubtitleOffset(-30_000)
        #expect(rig.controller.state.subtitleOffsetMs == -10_000)
        await rig.controller.close()

        // The next time the file plays, it starts moved as it was left.
        let again = try Rig(server: rig.server, request: PlayRequest(itemId: 51), warmer: NoWarmer(), offsets: offsets)
        try await started(again)
        #expect(again.controller.state.subtitleOffsetMs == -10_000)
        await again.controller.close()
    }

    @Test func anotherTrackOrOffShowsAtOnceAndIsSaved() async throws {
        let rig = try await subtitleRig()
        defer { rig.cleanUp() }
        try await started(rig)
        rig.controller.selectSubtitle(1000)
        try await until("the file beside it") {
            at(rig, 2_500)
            return texts(rig.controller.state.cues) == ["La première ligne."]
        }
        #expect(rig.controller.state.subtitles.first(where: \.selected)?.id == "1000")
        rig.controller.selectSubtitle(nil)
        #expect(rig.controller.state.cues == .none)
        #expect(rig.controller.state.subtitleLabel == "Off")
        #expect(!rig.controller.state.canSyncSubtitles)
        try await until("both saved") { rig.server.streamsChoices.count == 2 }
        let saved = bodies(rig.server)
        // The subtitles, and the file; the audio left as the server chose it.
        #expect(saved.map { $0["subtitle_index"] as? Int } == [1000, nil])
        #expect(saved.allSatisfy { $0["audio_index"] == nil && $0["file_id"] as? Int == 510 })
        #expect(rig.server.playable(51)?.selectedSubtitle == nil)
        // The c key goes through them: the file's SRT after Off.
        rig.controller.cycleSubtitles()
        #expect(rig.controller.state.subtitles.first(where: \.selected)?.id == "2")
        #expect(rig.engine.loads.count == 1)
        await rig.controller.close()
    }

    @Test func picturesAreDrawnFromTheMksWhileTheVideoIsCopied() async throws {
        let rig = try await subtitleRig(method: "remux", selected: 3)
        defer { rig.cleanUp() }
        rig.server.serveFile(path: "/api/files/510/subtitles/3.mks", type: "video/x-matroska", body: pgsTrack)
        try await started(rig)
        try await until("the picture") {
            at(rig, 1_500)
            return rig.controller.state.cues != .none
        }
        guard case .picture(let shown) = rig.controller.state.cues else {
            Issue.record("no picture")
            return
        }
        #expect(shown.frame.planeWidth == 1920)
        #expect(shown.frame.bitmaps.map { [$0.x, $0.y, $0.width, $0.height] } == [[30, 40, 4, 2]])
        at(rig, 3_500)
        #expect(rig.controller.state.cues == .none)
        #expect(!rig.controller.state.subtitlesBurnedIn)
        #expect(rig.controller.state.canSyncSubtitles)
        #expect(rig.controller.state.subtitles.allSatisfy { $0.detail == nil })
        // The session copies the video: nothing to burn was asked for.
        let sent = await rig.requests()
        #expect(sent.map(\.subtitleIndex) == [nil])
        #expect(sent.first?.caps.pictureSubtitlesSideload == true)
        #expect(rig.controller.session?.info.method == .remux)
        // Closing deletes the download.
        await rig.controller.close()
        let pictures = rig.folder.appendingPathComponent("pictures")
        let left = try? FileManager.default.contentsOfDirectory(atPath: pictures.path)
        #expect((left ?? []).isEmpty)
    }

    @Test func withoutTheMksTheStreamStartsAgainWithThePicturesBurnedIn() async throws {
        let rig = try await subtitleRig(method: "remux", selected: 3)
        defer { rig.cleanUp() }
        rig.server.answer(path: "/api/files/510/subtitles/3.mks", status: 503,
                          json: #"{"code":"unavailable","message":"no transcoder"}"#)
        rig.controller.start()
        try await until("the media loaded") { rig.engine.loads.count == 1 }
        // Shown, it asks for the .mks at once, and the 503 starts the next session beside it.
        rig.engine.emit(.ready)
        try await until("the second session") { rig.engine.loads.count == 2 }
        let sent = await rig.requests()
        // Asked again with the track once the server said it streams, as any track to burn in is.
        #expect(sent.map(\.subtitleIndex) == [nil, nil, 3])
        #expect(sent.last?.keepPrevious == true)
        rig.engine.emit(.ready)
        try await until("the burned-in stream playing") { rig.controller.state.phase == .playing }
        #expect(rig.controller.state.subtitlesBurnedIn)
        #expect(!rig.controller.state.canSyncSubtitles)
        #expect(rig.controller.state.cues == .none)
        #expect(rig.controller.state.subtitles.first { $0.id == "3" }?.detail == "Burned into the video")
        // Text is never burned in: a stream without the pictures takes over, and the SRT shows over it.
        rig.controller.selectSubtitle(2)
        try await until("the third session") { rig.engine.loads.count == 3 }
        rig.engine.emit(.ready)
        try await until("the first line") {
            at(rig, 2_500)
            return texts(rig.controller.state.cues) == ["The first line."]
        }
        #expect(!rig.controller.state.subtitlesBurnedIn)
        #expect(await rig.requests().map(\.subtitleIndex) == [nil, nil, 3, nil])
        await rig.controller.close()
    }
}
