@file:OptIn(ExperimentalSerializationApi::class)

package app.pley.android.network

import app.pley.android.auth.AuthException
import app.pley.android.model.PleyJson
import java.io.IOException
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.serialization.DeserializationStrategy
import kotlinx.serialization.ExperimentalSerializationApi
import kotlinx.serialization.SerializationException
import kotlinx.serialization.json.okio.decodeFromBufferedSource
import okhttp3.Call
import okhttp3.Callback
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.Response

/**
 * Sends [request] and reads its answer with [read], on OkHttp's own
 * threads (never the caller's: the main thread neither waits nor decodes).
 * Canceling the coroutine cancels the call, also while its body is read.
 * Failures come out as the app knows them (see [failure]).
 */
internal suspend fun <T> OkHttpClient.fetch(request: Request, read: (Response) -> T): T =
    suspendCancellableCoroutine { continuation ->
        val call = newCall(request)
        continuation.invokeOnCancellation { call.cancel() }
        call.enqueue(
            object : Callback {
                override fun onFailure(call: Call, e: IOException) {
                    // Ignored when the coroutine was canceled: it is what canceled the call.
                    continuation.resumeWith(Result.failure(failure(e)))
                }

                override fun onResponse(call: Call, response: Response) {
                    val result = try {
                        response.use { Result.success(read(it)) }
                    } catch (e: Throwable) {
                        Result.failure(failure(e))
                    }
                    continuation.resumeWith(result)
                }
            },
        )
    }

/**
 * An error as the app knows it: the server's refusals ([ApiException]) and
 * the sign-in module's ([AuthException]) as they are; an answer that can't
 * be read as [UnreadableAnswerException]; any other network failure as
 * [OfflineException].
 */
internal fun failure(error: Throwable): Throwable = when (error) {
    is ApiException, is AuthException, is OfflineException, is UnreadableAnswerException, is RedirectException -> error
    is SerializationException -> UnreadableAnswerException(0, error)
    is IOException -> OfflineException(error)
    else -> error
}

/** The answer's body as [T], after checking it is a success. */
internal fun <T> Response.decode(deserializer: DeserializationStrategy<T>): T {
    if (!isSuccessful) throw ApiException.from(this)
    return try {
        PleyJson.decodeFromBufferedSource(deserializer, body.source())
    } catch (e: SerializationException) {
        throw UnreadableAnswerException(code, e)
    } catch (e: IllegalArgumentException) {
        // kotlinx.serialization's other way of saying the JSON doesn't fit.
        throw UnreadableAnswerException(code, e)
    }
}

/** Checks the answer is a success; its body, if any, is left unread. */
internal fun Response.expectSuccess() {
    if (!isSuccessful) throw ApiException.from(this)
}
