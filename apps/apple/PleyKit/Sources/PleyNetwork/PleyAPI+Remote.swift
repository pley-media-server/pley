import Foundation
import PleyModel

/// A stop's optional body.
private struct StopBody: Encodable, Sendable {
    var message: String?
}

/// The body of a pairing: none to have the device show its code, the code it shows to pair.
private struct PairingBody: Encodable, Sendable {
    var pin: String?
}

/// Remote control (docs/API.md, "Remote control", "Casting" and "Stopping a
/// stream"): what the profile's other devices play, commands to them, and
/// what they are sent to play.
extension PleyAPI {
    /// `GET /api/sessions`: the profile's own sessions whose players are
    /// there, oldest first, an administrator's too.
    public func playingSessions() async throws -> [PlayingSession] {
        try await client.get("/api/sessions", as: [PlayingSession].self)
    }

    /// `POST /api/sessions/{id}/command`: `202` once it is on its way. It
    /// refuses with `.api`: `403` for someone else's session, `404` for
    /// one gone, `409` `not_controllable` (`notControllable`) for a command
    /// the player didn't list, `429` past 10 a second.
    public func send(_ command: PlayerCommand, to sessionId: String) async throws {
        _ = try await client.send("POST", "/api/sessions/\(Self.segment(sessionId))/command", body: command,
                                  as: NoContent.self)
    }

    /// `DELETE /api/sessions/{id}`: stops a session from here, the
    /// profile's own on another device (or anyone's, for an administrator),
    /// with `message` (200 characters at most) for whoever watches.
    public func stopSession(_ sessionId: String, message: String? = nil) async throws {
        let message = message.map { String($0.trimmingCharacters(in: .whitespacesAndNewlines).prefix(200)) }
        _ = try await client.send("DELETE", "/api/sessions/\(Self.segment(sessionId))",
                                  body: message.map { StopBody(message: $0.isEmpty ? nil : $0) },
                                  as: NoContent.self)
    }

    /// `GET /api/remote/devices`: the profile's own devices that take
    /// casts, one each, by name, an administrator's too, and for a home
    /// member the Google Cast and AirPlay devices on the server's network
    /// (`nearby=1`).
    public func remoteDevices() async throws -> [RemoteDevice] {
        try await client.get("/api/remote/devices", query: [URLQueryItem(name: "nearby", value: "1")],
                             as: [RemoteDevice].self)
    }

    /// `POST /api/remote/devices/{id}/pairing` with no code: has the AirPlay
    /// device of `receiverId` show its code, and answers how many digits it
    /// has, where the server knows (4 on an Apple TV). It refuses with
    /// `.api`: `404` for a device gone, `409` `pairing_failed`, `429` while
    /// the device takes no other try yet. It waits `deviceTimeout` for the
    /// answer.
    public func showPairingCode(on receiverId: String) async throws -> Int? {
        try await client.send("POST", "/api/remote/devices/\(try Self.receiverPath(receiverId))/pairing",
                              body: PairingBody(), as: PairingCode.self, timeout: Self.deviceTimeout).digits
    }

    /// `POST /api/remote/devices/{id}/pairing` with `code`, the one the
    /// AirPlay device of `receiverId` shows: `204` once the server is paired
    /// with it, for every home member's casts. It refuses with `.api`: `403`
    /// `wrong_pin` (`wrongPairingCode`), `429` after too many, `404` and
    /// `409` as above. It waits `deviceTimeout` for the answer.
    public func pair(with receiverId: String, code: String) async throws {
        _ = try await client.send("POST", "/api/remote/devices/\(try Self.receiverPath(receiverId))/pairing",
                                  body: PairingBody(pin: code), as: NoContent.self, timeout: Self.deviceTimeout)
    }

    /// `POST /api/remote/devices/{id}/cast`: `202` once `target` is on its
    /// way to the device of `receiverId`, which plays it as its own Play
    /// would. It refuses with `.api`: `404` for a device gone (or never the
    /// profile's) or what the profile can't see, `409` `not_castable` for
    /// what the device doesn't take, `403` for a channel without Live TV,
    /// `429` past 10 a second; for a Google Cast or an AirPlay device,
    /// `409` `cast_failed`, `cast_unreachable`, or `pairing_needed`
    /// (`pairingNeeded`) for an AirPlay device that wants its code first.
    /// It waits `deviceTimeout` for the answer.
    public func cast(_ target: CastTarget, to receiverId: String) async throws {
        _ = try await client.send("POST", "/api/remote/devices/\(try Self.receiverPath(receiverId))/cast",
                                  body: target, as: NoContent.self, timeout: Self.deviceTimeout)
    }

    /// How long a cast, or a pairing with an AirPlay device, waits for its
    /// answer rather than the transport's 20 seconds. A cast to a Google Cast
    /// or an AirPlay device is answered once the device plays, within the
    /// server's 3 minutes for starting it there (the stream started, the
    /// device's player launched, the stream loaded, and tried again with the
    /// basic stream when it is refused); a pairing with the code can take it
    /// 50 seconds, and having the device show its code 20. Waiting less, they
    /// would fail while the server still works on them, and the server be
    /// doubted as if it hadn't answered.
    public static let deviceTimeout: TimeInterval = 210

    /// `receiverSegment`, else the server's own answer for a device it doesn't know.
    private static func receiverPath(_ id: String) throws -> String {
        guard let receiver = receiverSegment(id) else {
            throw PleyError.api(status: 404, code: "not_found", message: localized("no such device"))
        }
        return receiver
    }

    /// A receiver's id as a path's segment: as the server allows one, 1 to
    /// 128 letters, digits, `.`, `_`, `:` and `-`, but dots alone, which
    /// would name another path; `nil` for anything else.
    static func receiverSegment(_ id: String) -> String? {
        guard (1...128).contains(id.count), !id.allSatisfy({ $0 == "." }),
              id.allSatisfy({ $0.isASCII && ($0.isLetter || $0.isNumber || ".:_-".contains($0)) })
        else { return nil }
        return id
    }
}

extension PleyError {
    /// The server's `409 not_controllable`: the player doesn't take that
    /// command from a remote.
    public var notControllable: Bool {
        if case .api(409, "not_controllable", _) = self { return true }
        return false
    }

    /// The server's `409 pairing_needed`: the AirPlay device wants its code first.
    public var pairingNeeded: Bool {
        if case .api(409, "pairing_needed", _) = self { return true }
        return false
    }

    /// The server's `403 wrong_pin` from a pairing: not the code the device shows.
    public var wrongPairingCode: Bool {
        if case .api(403, "wrong_pin", _) = self { return true }
        return false
    }
}
