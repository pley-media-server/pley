package app.pley.android.common

/**
 * An error whose words are already written for people: one of the app's
 * ([text], in every language), or the server's own sentence (its message,
 * as it is). [errorText] shows it as it is.
 */
interface UserFacing {
    /** The app's words for it; `null` when its message is the words, as it is (the server's). */
    val text: UiText? get() = null
}

/**
 * What a stub of the native app answers: that part isn't built yet. It
 * says so in the app rather than failing quietly.
 */
class NotBuiltYet(what: String) : UnsupportedOperationException("$what isn't built yet"), UserFacing

/**
 * Why [error] happened, in a few words for people, to put in the language
 * shown where it shows: "no answer in time". Any other network failure is
 * one sentence: OkHttp's own words ("unexpected end of stream on
 * http://…") stay in the error, for its details.
 */
fun errorText(error: Throwable): UiText = when (error) {
    is java.net.ConnectException, is java.net.NoRouteToHostException -> UiText.res(R.string.error_nothing_answers)
    is java.net.SocketTimeoutException -> UiText.res(R.string.error_no_answer)
    is java.net.UnknownHostException -> UiText.res(R.string.error_unknown_host)
    is javax.net.ssl.SSLException -> UiText.res(R.string.error_secure_connection)
    is UserFacing -> error.text ?: UiText.raw(error.message ?: error.javaClass.simpleName)
    is org.json.JSONException -> UiText.res(R.string.error_not_pley)
    is java.io.IOException -> UiText.res(R.string.error_connection_failed)
    else -> UiText.raw(error.message ?: error.javaClass.simpleName)
}

/** Why [error] happened, as a sentence for people: "No answer in time." */
fun errorSentence(error: Throwable): UiText = UiText.Sentence(errorText(error))

/*
 * describe and sentenceOf are errorText and errorSentence in English, for
 * code that still keeps the words of an error as a String; they go once
 * nothing calls them. An error of the app's own words (UserFacing.text)
 * gives its message, which is them in the language shown.
 */

/** Why [error] happened, as a sentence for people: "No answer in time." See [errorSentence]. */
fun sentenceOf(error: Throwable): String = asSentence(describe(error))

/** [text] as a sentence: a capital first, a full stop last (the app's own texts come in lower case, without one). */
fun asSentence(text: String): String {
    val capital = text.trim().replaceFirstChar { it.uppercase() }
    return if (capital.isEmpty() || capital.last() in ".…!?") capital else "$capital."
}

/** Why a request failed, in a few words for people. See [errorText]. */
fun describe(error: Throwable): String = when (error) {
    is java.net.ConnectException, is java.net.NoRouteToHostException -> "nothing answers there"
    is java.net.SocketTimeoutException -> "no answer in time"
    is java.net.UnknownHostException -> "that address isn't known on this network"
    is javax.net.ssl.SSLException -> "its secure connection failed"
    is UserFacing -> error.message ?: error.javaClass.simpleName
    is org.json.JSONException -> "that isn't a Pley server"
    is java.io.IOException -> CONNECTION_FAILED
    else -> error.message ?: error.javaClass.simpleName
}

/** What [describe] says of a network failure it has no words of its own for. */
const val CONNECTION_FAILED = "the connection to the server failed"
