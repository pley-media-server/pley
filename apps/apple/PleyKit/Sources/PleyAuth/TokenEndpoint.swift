import Foundation
import PleyCommon
import PleyModel
import PleyNetwork

/// Reading token answers (RFC 6749 §5.1).
enum TokenAnswer {
    /// Pley's access tokens last an hour; an answer without `expires_in` is read as that.
    static let defaultExpiresIn = 3600

    /// The answer in `data`. Without a scope, the server gave what was
    /// asked (RFC 6749 §5.1).
    static func parse(_ data: Data, asked: String) throws -> TokenResponse {
        guard var answer = try? PleyJSON.decoder.decode(TokenResponse.self, from: data),
              !answer.accessToken.isEmpty, answer.tokenType.caseInsensitiveCompare("Bearer") == .orderedSame
        else { throw AuthError.unreadable }
        let expiresIn = answer.expiresIn ?? defaultExpiresIn
        guard expiresIn > 0 else { throw AuthError.unreadable }
        answer.expiresIn = expiresIn
        if answer.refreshToken?.isEmpty == true { answer.refreshToken = nil }
        if (answer.scope ?? "").isEmpty { answer.scope = asked }
        return answer
    }
}

extension TokenResponse {
    /// The answer as it is kept.
    func stored(like base: StoredTokens, now: Date, byRefresh: Bool) -> StoredTokens {
        stored(issuer: base.issuer, clientID: base.clientID, tokenEndpoint: base.tokenEndpoint,
               revocationEndpoint: base.revocationEndpoint, now: now, byRefresh: byRefresh)
    }

    func stored(issuer: String, clientID: String, tokenEndpoint: String, revocationEndpoint: String, now: Date,
                byRefresh: Bool) -> StoredTokens {
        StoredTokens(issuer: issuer, clientID: clientID, tokenEndpoint: tokenEndpoint,
                     revocationEndpoint: revocationEndpoint, accessToken: accessToken,
                     accessExpiresAt: now.addingTimeInterval(TimeInterval(expiresIn ?? TokenAnswer.defaultExpiresIn)),
                     refreshToken: refreshToken, scope: scope ?? "", issuedByRefreshAt: byRefresh ? now : nil)
    }
}

/// Ending a sign-in on its server (RFC 7009).
enum Revocation {
    /// Ends the grant of `tokens` on its server: with the refresh token,
    /// which ends the whole grant, else the access token. Best effort: when
    /// it fails the person can still sign the app out in the web app's
    /// Account → Apps.
    static func revoke(_ tokens: StoredTokens, endpoints: EndpointClient) async {
        guard let url = URL(string: tokens.revocationEndpoint) else { return }
        let token = tokens.refreshToken ?? tokens.accessToken
        do {
            _ = try await endpoints.postForm(url, [("token", token), ("client_id", tokens.clientID)])
        } catch {
            Log.auth.info("revoking failed")
        }
    }
}

/// The app holds viewer tokens only. Every token answer (a code exchanged,
/// a refresh, a profile switch, a TV's code) may hold what was asked for
/// and no more; an answer with more is ended on the server at once and
/// never used. Pley never widens a grant, so this would be a bug or not Pley.
enum ScopeGuard {
    /// The scope words in `scope` that `asked` doesn't hold.
    static func extra(_ scope: String?, asked: String) -> Set<String> {
        words(scope ?? "").subtracting(words(asked))
    }

    /// Throws `AuthError.widerScope` when `answer` holds more than `asked`,
    /// after revoking what it gave (`fresh`).
    static func check(_ answer: TokenResponse, asked: String, fresh: StoredTokens,
                      endpoints: EndpointClient) async throws {
        guard !extra(answer.scope, asked: asked).isEmpty else { return }
        await Revocation.revoke(fresh, endpoints: endpoints)
        throw AuthError.widerScope
    }

    private static func words(_ scope: String) -> Set<String> {
        Set(scope.split(separator: " ").map(String.init))
    }
}

/// Keeps the tokens of a sign-in that just finished (in the browser, or a
/// TV's code), after the scope guard. The sign-in they replace on that
/// server (signing in again) is ended.
func keepNewSignIn(_ answer: TokenResponse, asked: String, fresh: StoredTokens, store: any TokenStore,
                   endpoints: EndpointClient) async throws -> StoredTokens {
    try await ScopeGuard.check(answer, asked: asked, fresh: fresh, endpoints: endpoints)
    let previous = try? await store.load(issuer: fresh.issuer)
    try await store.save(fresh)
    if let previous, previous.refreshToken != fresh.refreshToken || previous.accessToken != fresh.accessToken {
        await Revocation.revoke(previous, endpoints: endpoints)
    }
    return fresh
}
