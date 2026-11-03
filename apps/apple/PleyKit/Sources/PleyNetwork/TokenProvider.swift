import Foundation

/// The sign-in a request to a server carries. PleyAuth's `Tokens` is the
/// real one: it keeps the tokens in the Keychain and refreshes them.
public protocol TokenProvider: Sendable {
    /// The origin the tokens belong to (the server's issuer). Requests
    /// anywhere else get no token.
    var issuer: String { get }

    /// The access token to send, refreshed when it is about to end (within
    /// 60 seconds).
    func accessToken() async throws -> String

    /// Refreshes after the server refused `refused`. One refresh at a time:
    /// callers that come while one runs share it, and nothing happens when
    /// someone already replaced `refused`.
    func refresh(after refused: String) async throws

    /// Whether the server refused `token` moments after the refresh that
    /// gave it, which means it no longer accepts this sign-in: it ends
    /// then, and this is true.
    func refusedAfterRefresh(_ token: String) async -> Bool

    /// A request with the token reached an address that isn't private over
    /// plain http: the sign-in ends on the device.
    func connectionLeaked() async

    /// A request to the server got no answer: the device may be on another
    /// network, so the next token waits until the server proves itself again.
    func doubt() async
}

extension TokenProvider {
    /// Tokens without proofs have nothing to doubt.
    public func doubt() async {}
}
