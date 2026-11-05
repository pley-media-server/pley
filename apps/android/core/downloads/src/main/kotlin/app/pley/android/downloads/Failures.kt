package app.pley.android.downloads

import app.pley.android.common.UiText
import app.pley.android.network.ApiException
import kotlinx.serialization.Serializable

/**
 * Why a download failed, kept with it in the index ([Saved.failure]) and
 * worded where it shows ([errorText]), in the language shown then.
 */
@Serializable
internal enum class Failure {
    /** Marked whole, it has no file: its folder was removed outside the app. */
    Missing,

    /** Not enough room on the device for the rest of it. */
    NoSpace,

    /** A smaller file was asked for, and the server can't make one. */
    NoConversions,

    /** The server couldn't make its file for this device ([Saved.error]: the server's words, when it gave some). */
    ConversionFailed,

    /** The server wants the profile to sign in again (401, 403). */
    SignIn,

    /** The file is no longer on the server (404, 410). */
    Gone,

    /** The server couldn't send the file ([Saved.status]: its answer). */
    ServerFailed,
}

/** A download that can't go on by itself: [failure] says why, with the server's [detail] when it gave some. */
internal class DownloadFailure(val failure: Failure, val detail: String? = null) : Exception(failure.name)

/** [this] with why [e], an answer of the server's, failed it. */
internal fun Saved.failedWith(e: ApiException): Saved = when (e.status) {
    401, 403 -> copy(failure = Failure.SignIn, error = null, status = null)
    404, 410 -> copy(failure = Failure.Gone, error = null, status = null)
    else -> copy(failure = Failure.ServerFailed, error = null, status = e.status)
}

/**
 * Why [this] failed, as a sentence without its full stop: "This file is
 * no longer on the server". One an older version kept in words reads as
 * it was written; `null` when it didn't fail.
 */
internal fun Saved.errorText(): UiText? = when (failure) {
    null -> error?.let { UiText.raw(it) }
    Failure.Missing -> UiText.res(R.string.downloads_error_missing)
    Failure.NoSpace -> UiText.res(R.string.downloads_error_no_space)
    Failure.NoConversions -> UiText.res(R.string.downloads_error_no_conversions)
    Failure.ConversionFailed -> error?.let { UiText.res(R.string.downloads_error_conversion_said, it) }
        ?: UiText.res(R.string.downloads_error_conversion)
    Failure.SignIn -> UiText.res(R.string.downloads_error_sign_in)
    Failure.Gone -> UiText.res(R.string.downloads_error_gone)
    Failure.ServerFailed -> UiText.res(R.string.downloads_error_server, status ?: 0)
}
