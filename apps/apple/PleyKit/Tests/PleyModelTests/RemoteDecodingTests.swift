import Foundation
import PleyModel
import Testing

private func decode<T: Decodable>(_ type: T.Type, _ json: String) throws -> T {
    try PleyJSON.decoder.decode(T.self, from: Data(json.utf8))
}

private func object<T: Encodable>(_ value: T) throws -> [String: Any] {
    try JSONSerialization.jsonObject(with: PleyJSON.encoder.encode(value)) as? [String: Any] ?? [:]
}

@Suite("Remote control reads and writes as the server does")
struct RemoteDecodingTests {
    @Test func theSessionsPlayingElsewhere() throws {
        let sessions = try decode([PlayingSession].self, #"""
        [{"session_id":"a1","kind":"item","device":"Living room TV",
          "item":{"id":101,"library_id":1,"kind":"movie","title":"Arrival"},"channel":null,
          "state":"paused","position_ms":61000,"duration_ms":6960000,"updated_at":1761300000,"age_ms":4200,
          "controls":["play","pause","seek","skip","rewind"]},
         {"session_id":"b2","kind":"live","device":null,"item":null,
          "channel":{"id":3,"name":"BBC One","number":"101","logo":"/api/live/channels/3/logo","now":"News"},
          "state":"playing","position_ms":0,"duration_ms":null,"updated_at":1761300010,"age_ms":0,
          "controls":["next","previous"]}]
        """#)
        let movie = sessions[0]
        #expect(movie.id == "a1")
        #expect(movie.kind == .item)
        #expect(movie.device == "Living room TV")
        #expect(movie.item?.title == "Arrival")
        #expect(movie.channel == nil)
        #expect(movie.state == .paused)
        #expect(movie.positionMs == 61_000 && movie.durationMs == 6_960_000)
        #expect(movie.updatedAt == 1_761_300_000 && movie.ageMs == 4_200)
        // A command this app doesn't know reads, and is never offered.
        #expect(movie.obeys(.seek) && !movie.obeys(.next))
        #expect(movie.controls.last == Control(rawValue: "rewind"))
        let live = sessions[1]
        #expect(live.kind == .live)
        #expect(live.device == nil && live.item == nil && live.durationMs == nil)
        #expect(live.channel == PlayingChannel(id: 3, name: "BBC One", number: "101",
                                               logo: "/api/live/channels/3/logo", now: "News"))
        #expect(live.controls == [.next, .previous])
    }

    @Test func aSessionReadsWithWhatIsLeftOut() throws {
        let bare = try decode(PlayingSession.self, #"{"session_id":"c3"}"#)
        #expect(bare == PlayingSession(sessionId: "c3"))
        // A title that can't be read leaves the session listed.
        let odd = try decode(PlayingSession.self, #"{"session_id":"d4","item":{"title":7},"state":"buffering"}"#)
        #expect(odd.item == nil && odd.state == .buffering)
    }

    @Test func aRemotesCommandAsAnEvent() {
        func event(_ json: String) -> ServerEvent? { ServerEvent.parse(Data(json.utf8)) }
        #expect(event(#"{"type":"player_command","session_id":"a1","command":"seek","position_ms":90000,"offset_ms":null}"#)
            == .playerCommand(sessionId: "a1", command: .seek(toMs: 90_000)))
        #expect(event(#"{"type":"player_command","session_id":"a1","command":"skip","position_ms":null,"offset_ms":-10000}"#)
            == .playerCommand(sessionId: "a1", command: .skip(byMs: -10_000)))
        #expect(event(#"{"type":"player_command","session_id":"a1","command":"pause","position_ms":null,"offset_ms":null}"#)
            == .playerCommand(sessionId: "a1", command: .pause))
        // Missing what it needs: read as unknown, so it does nothing.
        #expect(event(#"{"type":"player_command","command":"pause"}"#) == .unknown("player_command"))
        #expect(event(#"{"type":"player_command","session_id":"a1"}"#) == .unknown("player_command"))
    }

    @Test func aCommandsBodyCarriesOnlyWhatItNeeds() throws {
        #expect(try object(PlayerCommand.play) as? [String: String] == ["command": "play"])
        #expect(try object(PlayerCommand.seek(toMs: 5_000)) as? [String: AnyHashable]
            == ["command": "seek", "position_ms": 5_000])
        #expect(try object(PlayerCommand.skip(byMs: -10_000)) as? [String: AnyHashable]
            == ["command": "skip", "offset_ms": -10_000])
        #expect(try object(PlayerCommand.audio(2)) as? [String: AnyHashable]
            == ["command": "audio", "stream_index": 2])
        // No subtitles: -1.
        #expect(try object(PlayerCommand.subtitles(nil)) as? [String: AnyHashable]
            == ["command": "subtitles", "stream_index": -1])
        #expect(try object(PlayerCommand.subtitles(1000)) as? [String: AnyHashable]
            == ["command": "subtitles", "stream_index": 1000])
    }

    @Test func aTrackCommandAsAnEvent() {
        func event(_ json: String) -> ServerEvent? { ServerEvent.parse(Data(json.utf8)) }
        #expect(event(#"{"type":"player_command","session_id":"a1","command":"audio","position_ms":null,"offset_ms":null,"stream_index":2}"#)
            == .playerCommand(sessionId: "a1", command: .audio(2)))
        #expect(event(#"{"type":"player_command","session_id":"a1","command":"subtitles","position_ms":null,"offset_ms":null,"stream_index":-1}"#)
            == .playerCommand(sessionId: "a1", command: .subtitles(nil)))
        // An older server's event, without the field, reads as before.
        #expect(event(#"{"type":"player_command","session_id":"a1","command":"pause","position_ms":null,"offset_ms":null}"#)
            == .playerCommand(sessionId: "a1", command: .pause))
    }

    @Test func aSessionSaysItsDeviceAndItsTracks() throws {
        let session = try decode(PlayingSession.self, #"""
        {"session_id":"a1","kind":"item","device":"Living room TV","device_id":"app-12","device_kind":"tv",
         "item":{"id":101,"library_id":1,"kind":"movie","title":"Test Movie"},"state":"playing","position_ms":0,
         "controls":["play","pause","audio","subtitles"],"audio_index":2,"subtitle_index":-1,
         "tracks":{"audio":[{"index":1,"language":"eng","title":null,"codec":"aac","channels":2,"default":true,
                             "forced":false},
                            {"index":2,"language":"fre","title":"Commentary","codec":"eac3","channels":6,
                             "default":false,"forced":false}],
                   "subtitles":[{"index":3,"language":"eng","title":null,"codec":"subrip","channels":null,
                                 "default":false,"forced":true},
                                {"index":1000,"language":"fre","title":null,"codec":"subrip","channels":null,
                                 "default":false,"forced":false}]}}
        """#)
        #expect(session.deviceId == "app-12" && session.deviceKind == .tv)
        #expect(session.obeys(.audio) && session.obeys(.subtitles))
        #expect(session.audioIndex == 2 && session.subtitleIndex == SessionTracks.off)
        #expect(session.tracks.audio.map(\.index) == [1, 2])
        #expect(session.tracks.audio[0].default && session.tracks.subtitles[0].forced)
        // Labelled as the player's own menus label tracks; from 1000, beside the video.
        #expect(session.tracks.audio.map { Words.stream($0.stream(.audio)) }
            == ["English (AAC Stereo)", "French – Commentary (EAC3 5.1)"])
        #expect(session.tracks.subtitles.map { Words.stream($0.stream(.subtitle)) }
            == ["English (SRT, Forced)", "French (SRT, External)"])

        // Live TV has neither tracks nor indexes; an older server says nothing of them.
        let live = try decode(PlayingSession.self, #"""
        {"session_id":"b2","kind":"live","device_id":"web-9f2c","device_kind":"browser",
         "audio_index":null,"subtitle_index":null,"tracks":{"audio":[],"subtitles":[]}}
        """#)
        #expect(live.tracks == SessionTracks() && live.audioIndex == nil && live.subtitleIndex == nil)
        #expect(live.deviceKind == .browser)
        let older = try decode(PlayingSession.self, #"{"session_id":"c3","tracks":{"audio":7}}"#)
        #expect(older.deviceId.isEmpty && older.deviceKind == .other && older.tracks == SessionTracks())
    }

    @Test func theDevicesThatTakeCasts() throws {
        let devices = try decode([RemoteDevice].self, #"""
        [{"receiver_id":"apple-1","device_id":"app-12","name":"Living room TV","kind":"tv","casts":["item","live"]},
         {"receiver_id":"web.kitchen:1","device_id":"web-9f2c","name":null,"kind":"browser","casts":["item","radio"]}]
        """#)
        #expect(devices.map(\.id) == ["apple-1", "web.kitchen:1"])
        #expect(devices[0].deviceId == "app-12" && devices[0].name == "Living room TV" && devices[0].kind == .tv)
        #expect(devices[0].takes(.item) && devices[0].takes(.live))
        // A kind this app doesn't know reads, and takes nothing it plays.
        #expect(devices[1].name == nil && devices[1].takes(.item) && !devices[1].takes(.live))
        #expect(devices[1].casts.last == CastKind(rawValue: "radio"))
        #expect(try decode(RemoteDevice.self, #"{"receiver_id":"x"}"#) == RemoteDevice(receiverId: "x"))
    }

    @Test func reportsSayTheTracksPlaying() throws {
        let report = try object(ProgressReport(positionMs: 5_000, state: .playing, sessionId: "a1", audioIndex: 2,
                                               subtitleIndex: SessionTracks.off))
        #expect(report["audio_index"] as? Int == 2 && report["subtitle_index"] as? Int == -1)
        // Left out where nothing is said (music, a download).
        let music = try object(ProgressReport(positionMs: 5_000, sessionId: "a1"))
        #expect(music["audio_index"] == nil && music["subtitle_index"] == nil)
    }

    @Test func playRequestsSayWhatThePlayerObeysAndWhereItIs() throws {
        let body = try object(PlaybackRequest(itemId: 101, clientId: "apple-1", keyedUrls: true,
                                              controls: [.play, .pause, .seek, .skip], deviceName: "Alex's Mac"))
        #expect(body["controls"] as? [String] == ["play", "pause", "seek", "skip"])
        #expect(body["device_name"] as? String == "Alex's Mac")
        // Left out: the player obeys nothing, and the server names the device itself.
        let bare = try object(PlaybackRequest(itemId: 101))
        #expect(bare["controls"] == nil && bare["device_name"] == nil)
        let watch = try object(LiveWatchRequest(clientId: "apple-1", controls: [.next, .previous],
                                                deviceName: "Living room"))
        #expect(watch["controls"] as? [String] == ["next", "previous"])
        #expect(watch["device_name"] as? String == "Living room")
        #expect(Set(try object(LiveWatchRequest(clientId: "apple-1")).keys) == ["client_id", "keyed_urls"])
    }
}
