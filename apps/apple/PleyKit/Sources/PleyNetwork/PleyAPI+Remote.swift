import Foundation
import PleyModel

/// A stop's optional body.
private struct StopBody: Encodable, Sendable {
    var message: String?
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
    /// casts, one each, by name, an administrator's too.
    public func remoteDevices() async throws -> [RemoteDevice] {
        try await client.get("/api/remote/devices", as: [RemoteDevice].self)
    }

    /// `POST /api/remote/devices/{id}/cast`: `202` once `target` is on its
    /// way to the device of `receiverId`, which plays it as its own Play
    /// would. It refuses with `.api`: `404` for a device gone (or never the
    /// profile's) or what the profile can't see, `409` `not_castable` for
    /// what the device doesn't take, `403` for a channel without Live TV,
    /// `429` past 10 a second.
    public func cast(_ target: CastTarget, to receiverId: String) async throws {
        guard let receiver = Self.receiverSegment(receiverId) else {
            throw PleyError.api(status: 404, code: "not_found", message: localized("no such device"))
        }
        _ = try await client.send("POST", "/api/remote/devices/\(receiver)/cast", body: target, as: NoContent.self)
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
}
