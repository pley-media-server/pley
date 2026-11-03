package app.pley.android.player

import app.pley.android.player.session.SavedPlaybackState

/**
 * What to play: an item of the signed-in server, and how. Small enough to
 * travel in an Intent; everything else is fetched or cached.
 */
data class PlayRequest(
    val itemId: Long,
    /** The version (file) to play; `null` lets the app pick the best one for this device. */
    val fileId: Long? = null,
    /** Where to start; `null` resumes where the profile left off, unless it was watched. */
    val startMs: Long? = null,
    val fromStart: Boolean = false,
    /**
     * The server's audio stream index, as the person picked it; `null`: the
     * server picks, in the remembered or default language, the track this
     * device plays best.
     */
    val audioIndex: Long? = null,
    /** The server's subtitle stream index; `null` keeps the remembered or default one. */
    val subtitleIndex: Long? = null,
    /** A playlist or collection to keep playing from, in order. */
    val queue: PlayQueue? = null,
    /**
     * A video on a drive plugged into the TV, by its path: it plays as it
     * is, with no server ([itemId] is [DRIVE_ITEM]).
     */
    val drivePath: String? = null,
) {
    companion object {
        /** The item a video on a drive plays as: no server's item is negative. */
        const val DRIVE_ITEM = -1L

        /** The video at [path] on a drive, from where it stopped. */
        fun drive(path: String): PlayRequest = PlayRequest(itemId = DRIVE_ITEM, drivePath = path)
    }
}

/** Where the next item comes from when this one ends. */
sealed interface PlayQueue {
    data class Playlist(val playlistId: Long) : PlayQueue

    data class Collection(val collectionId: Long) : PlayQueue
}

/**
 * Starts playbacks for the player screens. [prepare] runs when someone
 * presses play, before the player screen exists, so the server's answer,
 * the details and the decoders get ready while it opens.
 */
interface PlaybackServices {
    /** Starts preparing [request]; the key the player screen finds its controller by. */
    fun prepare(request: PlayRequest): String

    /** The controller [prepare] made for [key]; `null` once it was closed (or after process death). */
    fun controller(key: String): PlaybackController?

    /** Starts again what [saved] describes, after process death: a new key. */
    fun restore(saved: SavedPlaybackState): String = prepare(saved.toPlayRequest())
}
