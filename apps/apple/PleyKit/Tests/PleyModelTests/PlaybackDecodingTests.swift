import Foundation
import PleyModel
import PleyTesting
import Testing

private func decode<T: Decodable>(_ type: T.Type, fixture path: String) throws -> T {
    try PleyJSON.decoder.decode(T.self, from: Fixtures.data(path))
}

private func object<T: Encodable>(_ value: T) throws -> [String: Any] {
    try JSONSerialization.jsonObject(with: PleyJSON.encoder.encode(value)) as? [String: Any] ?? [:]
}

@Suite("Items and playback decode as the Android app reads them")
struct PlaybackDecodingTests {
    @Test func movieDetail() throws {
        let movie = try decode(ItemDetail.self, fixture: "items/movie_detail.json")
        #expect(movie.id == 101)
        #expect(movie.kind == .movie)
        #expect(movie.title == "Arrival")
        #expect(movie.userState.positionMs == 2_412_000)
        #expect(movie.genres == ["Drama", "Science Fiction", "Mystery"])
        #expect(movie.contentRating == "PG-13")
        #expect(movie.files.map(\.id) == [501, 502])
        let uhd = movie.files[0]
        #expect(uhd.container == "mkv")
        #expect(uhd.selectedAudio == 1)
        #expect(uhd.audioPicked)
        #expect(uhd.selectedSubtitle == 4)
        #expect(uhd.chapters.count == 3)
        #expect(uhd.chapters[2].title == "End Credits")
        #expect(uhd.chapters[0].thumbUrl == "/api/files/501/chapters/0.jpg?v=9c1d2e3f4a5b")
        let video = try #require(uhd.video)
        #expect(video.dynamicRange == .dolbyVision)
        #expect(video.dvProfile == 8)
        #expect(video.dvBlCompat == 1)
        #expect(video.level == 153)
        #expect(video.frameRate == 23.976)
        #expect(uhd.streams(.audio).map(\.title) == ["TrueHD Atmos 7.1", "Commentary"])
        let subtitles = uhd.streams(.subtitle)
        #expect(subtitles.map(\.index) == [3, 4, 1000])
        #expect(subtitles[0].textBased == false)
        #expect(subtitles[2].external)
        #expect(movie.files[1].edition == "Open Matte")
        #expect(!movie.files[1].audioPicked)
        #expect(movie.markers == [Marker(fileId: 501, kind: .credits, startMs: 6_702_000, endMs: 6_985_000,
                                         source: "chapter")])
        #expect(movie.extras.map(\.extraType) == ["trailer", "featurette"])
    }

    @Test func episodeDetail() throws {
        let episode = try decode(ItemDetail.self, fixture: "items/episode_detail.json")
        #expect(episode.kind == .episode)
        #expect(episode.summary.grandparentTitle == "Severance")
        #expect(episode.summary.parentIndex == 1)
        #expect(episode.summary.index == 1)
        #expect(episode.userState.watched)
        #expect(episode.files.first?.videoCodec == "h264")
    }

    @Test func nullsAndMissingFieldsReadAsDefaults() throws {
        let minimal = try decode(ItemDetail.self, fixture: "items/minimal_detail.json")
        #expect(minimal.files.isEmpty)
        #expect(minimal.markers.isEmpty)
        #expect(minimal.genres.isEmpty)
        let nulls = try decode(ItemDetail.self, fixture: "items/explicit_nulls.json")
        let file = try #require(nulls.files.first)
        #expect(file.id == 504)
        #expect(file.size == 0)
        #expect(file.streams.isEmpty)
        #expect(file.chapters.isEmpty)
        #expect(file.video == nil)
    }

    @Test func onDeckAndNext() throws {
        let deck = try decode(MaybeItem.self, fixture: "items/on_deck.json")
        #expect(deck.item?.id == 204)
        #expect(deck.item?.title == "Half Loop")
        #expect(try decode(MaybeItem.self, fixture: "items/next_none.json").item == nil)
    }

    @Test func playbackInfo() throws {
        let direct = try decode(PlaybackInfo.self, fixture: "playback/info_direct.json")
        #expect(direct.method == .direct)
        #expect(!direct.hls)
        #expect(direct.fileId == 601)
        #expect(direct.audioIndex == 1)
        #expect(direct.subtitles.map(\.url) == ["/api/files/601/subtitles/3.vtt", "/api/files/601/subtitles/4.vtt"])
        #expect(direct.reasons.isEmpty)
        #expect(direct.ptsOffsetMs == nil)
        #expect(!direct.adaptive)

        let transcode = try decode(PlaybackInfo.self, fixture: "playback/info_transcode.json")
        #expect(transcode.method == .transcode)
        #expect(transcode.hls)
        #expect(transcode.burnSubtitles)
        #expect(transcode.audioConverted)
        #expect(transcode.subtitles[0].url == nil)
        #expect(!transcode.subtitles[0].textBased)
        #expect(transcode.subtitles[2].external)
        #expect(transcode.reasons.count == 3)
        #expect(transcode.autoQuality == "15m")
    }

    @Test func sessionStatus() throws {
        let direct = try decode(SessionStatus.self, fixture: "playback/status_direct.json")
        #expect(direct.method == .direct)
        #expect(direct.transcode == nil)
        let transcode = try decode(SessionStatus.self, fixture: "playback/status_transcode.json")
        #expect(transcode.transcode?.videoOut == "H.264 3840×2160")
        #expect(transcode.transcode?.speed == 2.4)
        #expect(transcode.transcode?.decoder == "cuda")
        #expect(transcode.transcode?.subtitleBurn == true)
    }

    @Test func newerFieldsOfTheAnswerRead() throws {
        let info = try PleyJSON.decoder.decode(PlaybackInfo.self, from: Data("""
            {"session_id":"a1","method":"remux","url":"/api/media/pley_mk_x/master.m3u8","hls":true,"file_id":3,
             "video_range":"HDR10","adaptive":true,"live_bitrate":false,"pts_offset_ms":0,"method_next":"x"}
            """.utf8))
        #expect(info.method == .remux)
        #expect(info.videoRange == "HDR10")
        #expect(info.adaptive)
        #expect(info.ptsOffsetMs == 0)
        #expect(info.durationMs == 0)
    }
}

@Suite("Playback bodies are written as the server reads them")
struct PlaybackBodyTests {
    @Test func theRequestLeavesOutWhatIsntChosen() throws {
        let body = try object(PlaybackRequest(itemId: 101, fileId: 501, startMs: 2_412_000, clientId: "apple-1",
                                              keyedUrls: true, caps: ClientCaps(containers: ["mp4"])))
        #expect(Set(body.keys) == ["item_id", "file_id", "start_ms", "mode", "client_id", "keyed_urls", "caps"])
        #expect(body["mode"] as? String == "auto")
        #expect(body["keyed_urls"] as? Bool == true)
        let android = try decode(PlaybackRequest.self, fixture: "playback/request.json")
        #expect(android.maxBitrate == 20_000)
        #expect(android.audioIndex == 1)
        #expect(android.caps.containers.contains("mkv"))
    }

    @Test func progressReport() throws {
        let report = try decode(ProgressReport.self, fixture: "playback/progress_request.json")
        #expect(report.state == ProgressState(rawValue: "buffering"))
        #expect(report.sessionId == "6f1c2a9e4b7d4e0f9a3c5b8d7e6f1a2b")
        let late = try object(ProgressReport(positionMs: 5, durationMs: nil, state: .stopped, ageMs: 60_000))
        #expect(Set(late.keys) == ["position_ms", "state", "age_ms"])
    }

    @Test func streamsChoice() throws {
        let android = try JSONSerialization.jsonObject(with: Fixtures.data("playback/streams_request.json"))
            as? [String: Int]
        let picked = try object(StreamsChoice(fileId: 601, audioIndex: 2, subtitleIndex: 4))
        #expect(picked as? [String: Int] == android)
        // Subtitles alone: the audio is left out, which keeps the one chosen before.
        #expect(Set(try object(StreamsChoice(fileId: 601, subtitleIndex: 4)).keys) == ["file_id", "subtitle_index"])
        // Automatic: the audio picked before is forgotten.
        let automatic = try object(StreamsChoice(fileId: 601, automaticAudio: true))
        #expect(automatic["audio_index"] is NSNull)
    }

    @Test func networkReports() throws {
        let probe = try object(NetworkReport.probe(clientId: "apple-1", kbps: 52_000, complete: false))
        #expect(probe["probe_kbps"] as? Int == 52_000)
        #expect(probe["probe_complete"] as? Bool == false)
        let forget = try object(NetworkReport.ceiling(clientId: "apple-1", kbps: nil))
        #expect(forget["ceiling_kbps"] is NSNull)
        #expect(forget["client_id"] as? String == "apple-1")
        let answer = try PleyJSON.decoder.decode(NetworkAnswer.self, from: Data(#"{"local":true,"ceiling_kbps":null}"#.utf8))
        #expect(answer == NetworkAnswer(local: true))
    }
}
