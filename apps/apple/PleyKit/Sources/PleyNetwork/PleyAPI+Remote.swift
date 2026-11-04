import Foundation
import PleyModel

/// A stop's optional body.
private struct StopBody: Encodable, Sendable {
    var message: String?
}

/// Remote control (docs/API.md, "Remote control", "Casting" and "Stopping a
/// stream"): the profile's other devices, what they play, and commands to
/// them.
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
}

extension PleyError {
    /// The server's `409 not_controllable`: the player doesn't take that
    /// command from a remote.
    public var notControllable: Bool {
        if case .api(409, "not_controllable", _) = self { return true }
        return false
    }
}
