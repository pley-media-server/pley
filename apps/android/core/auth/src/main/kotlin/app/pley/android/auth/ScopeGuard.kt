package app.pley.android.auth

import app.pley.android.common.UiText
import okhttp3.OkHttpClient

/** Why a sign-in with more than the app asks for ended. */
val WIDER_SCOPE: UiText = UiText.res(R.string.auth_wider_scope)

/** A token answer with more rights than were asked for. Its grant was ended. */
class WiderScopeException : AuthException(WIDER_SCOPE)

/**
 * The app holds viewer tokens only. Every token answer (a code exchanged, a
 * refresh, a profile switch, a TV's code) may hold what was asked for and
 * no more; an answer with more is ended on the server at once and never
 * used. pley never widens a grant, so this would be a bug or not pley.
 */
internal object ScopeGuard {
    /** The scope names in [answer] that [asked] doesn't hold. */
    fun extra(answer: TokenResponse, asked: String): Set<String> = words(answer.scope) - words(asked)

    /**
     * Throws [WiderScopeException] when [answer] holds more than [asked],
     * after revoking what it gave ([fresh]: its tokens, as they would be
     * kept).
     */
    suspend fun check(http: OkHttpClient, answer: TokenResponse, asked: String, fresh: StoredTokens) {
        if (extra(answer, asked).isEmpty()) return
        Tokens.revoke(http, fresh)
        throw WiderScopeException()
    }

    private fun words(scope: String): Set<String> = scope.split(' ').filter { it.isNotEmpty() }.toSet()
}

/**
 * Keeps the tokens of a sign-in that just finished (a code exchanged in the
 * browser, or a TV's code), after the scope guard; the sign-in they replace
 * on that server (signing in again, or as a profile that needs its
 * password) is ended.
 */
internal suspend fun keepNewSignIn(
    http: OkHttpClient,
    tokens: Tokens,
    answer: TokenResponse,
    asked: String,
    fresh: StoredTokens,
): StoredTokens {
    ScopeGuard.check(http, answer, asked, fresh)
    val previous = tokens.install(fresh)
    // The sign-in before goes only to the server that proves it holds its key.
    if (previous != null && previous.refreshToken != fresh.refreshToken && tokens.proves(previous)) Tokens.revoke(http, previous)
    return fresh
}
