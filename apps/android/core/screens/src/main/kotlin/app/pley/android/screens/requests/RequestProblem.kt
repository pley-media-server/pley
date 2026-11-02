package app.pley.android.screens.requests

import app.pley.android.data.RequestsState
import app.pley.android.data.mayAsk
import app.pley.android.model.MediaRequest
import app.pley.android.model.RequestScope
import app.pley.android.network.ApiException
import app.pley.android.network.OfflineException

/**
 * Why a request route refused, as the screens say it.
 * A 503 with pley's JSON is an answer, never the server being away: only
 * [Offline] is.
 */
sealed interface RequestProblem {
    /** 503 `unavailable`: "Requests are off". */
    data object Off : RequestProblem

    /** 403 `no_access`: "Requests aren't open to this account". */
    data object NoAccess : RequestProblem

    /** 403 `insufficient_scope`: this sign-in lacks [scope]; signing in again asks for it. */
    data class SignInAgain(val scope: RequestScope) : RequestProblem

    /** 503 `tmdb_key`: TMDB refused the server's key. No Retry: an administrator fixes it. */
    data object TmdbKey : RequestProblem

    /** 503 `tmdb_down`: "TMDB isn't answering", with Retry. */
    data object TmdbDown : RequestProblem

    /** 403 `restricted`: above the account's age limit; [text] says so. */
    data class Restricted(val text: String) : RequestProblem

    /** 409 `declined`: declined in the last 30 days; asking again works from [retryAt]. */
    data class Declined(val retryAt: Long?) : RequestProblem

    /**
     * 429 `too_many_requests`: past the limit until [retryAt]; `null`: what
     * was asked is more than the limit itself ("pick fewer").
     */
    data class Limit(val retryAt: Long?) : RequestProblem

    /** Another 409 (`in_library`, `conflict`): the server's [message], and the request as it is now, when it sent it. */
    data class Conflict(val message: String, val request: MediaRequest?) : RequestProblem

    /** 404: the title or the request isn't there (any more). */
    data object NotFound : RequestProblem

    /** The server can't be reached. */
    data class Offline(val error: Throwable) : RequestProblem

    /** Anything else: [error]'s sentence. */
    data class Other(val error: Throwable) : RequestProblem

    /** Trying again may help: a Retry shows. */
    val retry: Boolean get() = this is TmdbDown || this is Offline || this is Other

    companion object {
        /** Why Discover's pages can't show, from [gate]: signed in before requests, requests off, no access; `null` otherwise. */
        fun of(gate: RequestsState): RequestProblem? = when {
            gate is RequestsState.SignInAgain -> SignInAgain(RequestScope.Requests)
            gate == RequestsState.Hidden -> Off
            gate == RequestsState.NoAccess -> NoAccess
            gate is RequestsState.Open && !gate.mayAsk -> NoAccess
            else -> null
        }

        fun of(error: Throwable): RequestProblem {
            val api = error as? ApiException ?: return if (error is OfflineException) Offline(error) else Other(error)
            val details = api.details
            return when {
                api.needsScope -> SignInAgain(if (api.scope == RequestScope.Approvals.wire) RequestScope.Approvals else RequestScope.Requests)
                api.code == "unavailable" || (api.status == 503 && details?.setup == false) -> Off
                api.code == "tmdb_key" -> TmdbKey
                api.code == "tmdb_down" -> TmdbDown
                api.code == "no_access" -> NoAccess
                api.code == "restricted" -> Restricted(api.message.orEmpty())
                api.code == "declined" -> Declined(api.retryAt)
                api.status == 429 || api.code == "too_many_requests" -> Limit(api.retryAt)
                api.status == 409 -> Conflict(api.message.orEmpty(), details?.request)
                api.status == 404 -> NotFound
                // A 503 without a code of these: TMDB or the server busy, worth a Retry.
                else -> Other(error)
            }
        }
    }
}
