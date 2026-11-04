import Foundation
import PleyModel

/// Remote control, as the server does it (docs/API.md, "Remote control"
/// and "Casting"): the profile's sessions whose players are there
/// (`GET /api/sessions`), each with its device and its file's tracks;
/// commands to them, checked and passed on as `player_command` on the
/// event streams; a stop from another device (`DELETE /api/sessions/{id}`);
/// and the devices that take casts (`GET /api/remote/devices`), which a
/// test adds. Sessions starting and ending, reports that pause, play, jump
/// or change track, and receivers a test adds or takes away send
/// `sessions_changed`.
///
/// A device is a sign-in, as on the server: what one token plays shares
/// `app-<grant>`.
extension FixtureServer {
    struct Remote {
        /// Sessions started, titles' and live ones', for their order.
        var started = 0
        var commands: [CommandRecord] = []
        /// Commands answer this status and code from now on (`nil`: as the server would).
        var refusal: (status: Int, code: String)?
        /// When each title's session last reported, for its jumps.
        var reportedAt: [String: Date] = [:]
        /// The devices that take casts, by receiver id.
        var receivers: [String: FixtureReceiver] = [:]
        /// Receivers named so far, for their order: the newest of a device is its own.
        var named = 0
    }

    /// A device that takes casts.
    struct FixtureReceiver: Sendable {
        var receiverId: String
        var deviceId: String
        var name: String?
        var kind: String
        var casts: [String]
        var order: Int
    }

    /// A command as it came: the session and the JSON body.
    public struct CommandRecord: Sendable, Hashable {
        public let sessionId: String
        public let body: Data
    }

    static let sessionsChanged = #"{"type":"sessions_changed"}"#

    /// The furthest a `skip` goes either way, and a `seek` when the length isn't known.
    static let maxSkipMs = 600_000
    static let maxUnknownSeekMs = 86_400_000

    // MARK: Knobs

    /// The commands received, in order, whether or not they were refused.
    public var sessionCommands: [CommandRecord] { state.withLock { $0.remote.commands } }

    /// Commands answer `status` with `code` from now on; `nil`: as the server would.
    public func refuseCommands(status: Int?, code: String = "not_controllable") {
        state.withLock { s in s.remote.refusal = status.map { ($0, code) } }
    }

    /// A title's session `id` reports `state` at `positionMs`, as its player
    /// on another device would; with `audioIndex` and `subtitleIndex`, the
    /// tracks it plays.
    public func reportSession(_ id: String, state playing: String, positionMs: Int, audioIndex: Int? = nil,
                              subtitleIndex: Int? = nil) {
        let events = state.withLock { s -> [String] in
            var report: [String: Any] = ["state": playing, "position_ms": positionMs]
            if let audioIndex { report["audio_index"] = audioIndex }
            if let subtitleIndex { report["subtitle_index"] = subtitleIndex }
            Self.reported(&s, id, report)
            defer { s.outbox = [] }
            return s.outbox
        }
        for event in events { pushEvent(event) }
    }

    /// Another of the profile's devices takes casts: receiver `id` of the
    /// device `deviceId`, named `name`, of `kind`, taking `casts`. The
    /// remotes hear of it (`sessions_changed`).
    public func addReceiver(_ id: String, deviceId: String, name: String?, kind: String = "tv",
                            casts: [String] = ["item", "live"]) {
        state.withLock { s in
            s.remote.named += 1
            s.remote.receivers[id] = FixtureReceiver(receiverId: id, deviceId: deviceId, name: name, kind: kind,
                                                     casts: casts, order: s.remote.named)
        }
        pushEvent(Self.sessionsChanged)
    }

    /// Receiver `id` goes, as its app's stream closes.
    public func removeReceiver(_ id: String) {
        state.withLock { s in s.remote.receivers[id] = nil }
        pushEvent(Self.sessionsChanged)
    }

    /// Item `itemId` plays on another of the profile's devices, `deviceId`,
    /// whose player obeys `controls`: its session's id. It starts as the
    /// server starts a session, with the title's tracks.
    @discardableResult
    public func playElsewhere(_ itemId: Int, deviceId: String, device: String?, kind: String = "tv",
                              controls: [String] = ["play", "pause", "seek", "skip"], positionMs: Int = 0) -> String {
        let id = Self.randomToken()
        let events = state.withLock { s -> [String] in
            guard let playable = s.playables[itemId] else { return [] }
            s.remote.started += 1
            s.sessions[id] = PlaySession(itemId: itemId, key: "pley_mk_" + Self.randomToken(), method: "direct",
                                         controls: controls, device: device, positionMs: positionMs,
                                         durationMs: playable.durationMs, order: s.remote.started,
                                         deviceId: deviceId, deviceKind: kind,
                                         audioIndex: playable.selectedAudio ?? playable.audio.first?.index,
                                         subtitleIndex: playable.selectedSubtitle ?? -1)
            return [Self.sessionsChanged]
        }
        for event in events { pushEvent(event) }
        return id
    }

    // MARK: Routes

    static func remote(_ s: inout State, _ request: Request, _ parts: [String]) -> Answer? {
        guard parts.count >= 2, parts[0] == "api" else { return nil }
        if parts.count >= 3, parts[1] == "remote", parts[2] == "devices" {
            switch (request.method, parts.count) {
            case ("GET", 3):
                return withToken(&s, request, scope: "play") { s, _ in json(200, devices(s)) }
            default:
                return nil
            }
        }
        guard parts[1] == "sessions" else { return nil }
        switch (request.method, parts.count) {
        case ("GET", 2):
            return withToken(&s, request, scope: "play") { s, _ in json(200, playing(s)) }
        case ("POST", 4) where parts[3] == "command":
            return withToken(&s, request, scope: "play") { s, _ in command(&s, parts[2], request.body) }
        case ("DELETE", 3):
            return withToken(&s, request, scope: "play") { s, _ in stop(&s, parts[2], request.body) }
        default:
            return nil
        }
    }

    /// The device a token's sign-in is, as the server names one of its apps.
    static func deviceId(_ access: Access) -> String { "app-\(access.grant)" }

    /// A title's report: its place, state and tracks, and `sessions_changed` when it paused, played, jumped
    /// or changed track.
    static func reported(_ s: inout State, _ id: String, _ body: [String: Any]) {
        guard let session = s.sessions[id] else { return }
        let now = Date()
        let position = body["position_ms"] as? Int ?? session.positionMs
        let state = body["state"] as? String ?? session.state
        let elapsed = s.remote.reportedAt[id].map { Int(now.timeIntervalSince($0) * 1000) } ?? 0
        let expected = session.positionMs + (session.state == "playing" ? elapsed : 0)
        let jumped = abs(position - expected) > 5_000 + elapsed
        s.remote.reportedAt[id] = now
        guard state != "stopped" else { return }
        let audio = body["audio_index"] as? Int ?? session.audioIndex
        let subtitles = body["subtitle_index"] as? Int ?? session.subtitleIndex
        s.sessions[id]?.positionMs = position
        s.sessions[id]?.state = state
        s.sessions[id]?.audioIndex = audio
        s.sessions[id]?.subtitleIndex = subtitles
        if state != session.state || jumped || audio != session.audioIndex || subtitles != session.subtitleIndex {
            s.outbox.append(sessionsChanged)
        }
    }

    private static func playing(_ s: State) -> [[String: Any]] {
        var out: [(order: Int, json: [String: Any])] = []
        for (id, session) in s.sessions where !session.deleted && !session.expired && session.stopped == nil {
            guard let playable = s.playables[session.itemId] else { continue }
            var json: [String: Any] = [
                "session_id": id, "kind": "item", "device": session.device ?? "Pley for Mac",
                "item": summary(playable), "channel": NSNull(), "state": session.state,
                "position_ms": session.positionMs,
                "duration_ms": session.durationMs > 0 ? session.durationMs : NSNull(),
                "updated_at": Int(Date().timeIntervalSince1970), "age_ms": 0, "controls": session.controls,
            ]
            json["device_id"] = session.deviceId
            json["device_kind"] = session.deviceKind
            json["audio_index"] = session.audioIndex.map { $0 as Any } ?? NSNull()
            json["subtitle_index"] = session.subtitleIndex.map { $0 as Any } ?? NSNull()
            let tracks: [String: Any] = ["audio": playable.audio.map(track),
                                         "subtitles": playable.subtitles.map(track)]
            json["tracks"] = tracks
            out.append((session.order, json))
        }
        for (id, session) in s.live.sessions where !session.deleted && session.stopped == nil {
            guard let channel = s.live.channels.first(where: { $0.id == session.channelId }) else { continue }
            let full = channelJSON(s, channel)
            let now = (full["now"] as? [String: Any])?["title"] ?? NSNull()
            var json: [String: Any] = [
                "session_id": id, "kind": "live", "device": session.device ?? "Pley for Mac", "item": NSNull(),
                "channel": ["id": channel.id, "name": channel.name, "number": channel.number ?? NSNull(),
                            "logo": full["logo"] ?? NSNull(), "now": now],
                "state": "playing", "position_ms": 0, "duration_ms": NSNull(),
                "updated_at": Int(Date().timeIntervalSince1970), "age_ms": 0, "controls": session.controls,
            ]
            json["device_id"] = session.deviceId
            json["device_kind"] = "other"
            json["audio_index"] = NSNull()
            json["subtitle_index"] = NSNull()
            let tracks: [String: Any] = ["audio": [Any](), "subtitles": [Any]()]
            json["tracks"] = tracks
            out.append((session.order, json))
        }
        return out.sorted { $0.order < $1.order }.map(\.json)
    }

    /// A stream of the file as a session lists it for the remotes.
    private static func track(_ stream: MediaStream) -> [String: Any] {
        var json: [String: Any] = ["index": stream.index, "codec": stream.codec, "default": stream.default,
                                   "forced": stream.forced]
        json["language"] = stream.language.map { $0 as Any } ?? NSNull()
        json["title"] = stream.title.map { $0 as Any } ?? NSNull()
        json["channels"] = stream.channels.map { $0 as Any } ?? NSNull()
        return json
    }

    /// The devices that take casts: one per device, its newest receiver, by name.
    private static func devices(_ s: State) -> [[String: Any]] {
        var newest: [String: FixtureReceiver] = [:]
        for receiver in s.remote.receivers.values where (newest[receiver.deviceId]?.order ?? 0) < receiver.order {
            newest[receiver.deviceId] = receiver
        }
        let sorted = newest.values.sorted { ($0.name ?? "", $0.receiverId) < ($1.name ?? "", $1.receiverId) }
        return sorted.map { receiver -> [String: Any] in
            var json: [String: Any] = ["receiver_id": receiver.receiverId, "device_id": receiver.deviceId,
                                       "kind": receiver.kind, "casts": receiver.casts]
            json["name"] = receiver.name.map { $0 as Any } ?? NSNull()
            return json
        }
    }

    /// Checks a command as the server does and passes it on to the event streams.
    private static func command(_ s: inout State, _ id: String, _ body: Data) -> Answer {
        s.remote.commands.append(CommandRecord(sessionId: id, body: body))
        if let refusal = s.remote.refusal {
            return json(refusal.status, ["code": refusal.code, "message": "refused"])
        }
        let asked = (try? JSONSerialization.jsonObject(with: body)) as? [String: Any] ?? [:]
        let controls: [String]
        let durationMs: Int
        // The title's tracks; none on Live TV.
        let playable: Playable?
        if let session = s.sessions[id], !session.deleted, !session.expired, session.stopped == nil {
            controls = session.controls
            durationMs = session.durationMs
            playable = s.playables[session.itemId]
        } else if let session = s.live.sessions[id], !session.deleted, session.stopped == nil {
            controls = session.controls
            durationMs = 0
            playable = nil
        } else {
            return json(404, ["code": "not_found", "message": "no such session playing"])
        }
        guard let command = asked["command"] as? String,
              ["play", "pause", "seek", "skip", "next", "previous", "audio", "subtitles"].contains(command)
        else { return json(400, ["code": "bad_request", "message": "unknown command"]) }
        var position: Int?
        var offset: Int?
        var stream: Int?
        if command == "seek" {
            guard let at = asked["position_ms"] as? Int, at >= 0,
                  at <= (durationMs > 0 ? durationMs : maxUnknownSeekMs)
            else { return json(400, ["code": "bad_request", "message": "position_ms out of range"]) }
            position = at
        }
        if command == "skip" {
            guard let by = asked["offset_ms"] as? Int, (-maxSkipMs...maxSkipMs).contains(by) else {
                return json(400, ["code": "bad_request", "message": "offset_ms out of range"])
            }
            offset = by
        }
        if command == "audio" || command == "subtitles" {
            let tracks = command == "audio" ? playable?.audio : playable?.subtitles
            guard let tracks, let index = asked["stream_index"] as? Int,
                  tracks.contains(where: { $0.index == index }) || (command == "subtitles" && index == -1)
            else { return json(400, ["code": "bad_request", "message": "stream_index isn't one of its tracks"]) }
            stream = index
        }
        guard controls.contains(command) else {
            return json(409, ["code": "not_controllable", "message": "this player doesn't take \(command) from a remote"])
        }
        let event: [String: Any] = ["type": "player_command", "session_id": id, "command": command,
                                    "position_ms": position ?? NSNull(), "offset_ms": offset ?? NSNull(),
                                    "stream_index": stream ?? NSNull()]
        if let data = try? JSONSerialization.data(withJSONObject: event, options: [.sortedKeys]) {
            s.outbox.append(String(decoding: data, as: UTF8.self))
        }
        return Answer(status: 202, headers: [:], body: Data())
    }

    /// Stops a session from another device: its player hears `session_stopped`, and its next requests `410`.
    private static func stop(_ s: inout State, _ id: String, _ body: Data) -> Answer {
        let asked = (try? JSONSerialization.jsonObject(with: body)) as? [String: Any] ?? [:]
        let message = asked["message"] as? String
        if s.sessions[id] != nil {
            s.sessions[id]?.stopped = .some(message)
        } else if s.live.sessions[id] != nil {
            s.live.sessions[id]?.stopped = message ?? "Live TV was stopped from another device."
        } else {
            return json(404, ["code": "not_found", "message": "no such session"])
        }
        let event: [String: Any] = ["type": "session_stopped", "session_id": id, "message": message ?? NSNull(),
                                    "by_admin": false]
        if let data = try? JSONSerialization.data(withJSONObject: event, options: [.sortedKeys]) {
            s.outbox.append(String(decoding: data, as: UTF8.self))
        }
        s.outbox.append(sessionsChanged)
        return Answer(status: 204, headers: [:], body: Data())
    }
}
