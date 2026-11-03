package app.pley.android.player.session

import android.os.Bundle
import app.pley.android.player.PlayQueue
import app.pley.android.player.PlayRequest

/**
 * Where a playback was, to start it again after Android killed the app in
 * the background (the player screen keeps it in its saved instance state):
 * the item, its version, the position, the tracks, the quality and the queue,
 * or the video on a drive.
 */
data class SavedPlaybackState(
    val itemId: Long,
    val fileId: Long?,
    val positionMs: Long,
    val audioIndex: Long?,
    val subtitleIndex: Long?,
    /** The quality's id ("original", "8m"). */
    val quality: String,
    val queue: PlayQueue?,
    /** A video on a drive, by its path ([PlayRequest.drivePath]). */
    val drivePath: String? = null,
) {
    /** The request that plays it again from where it was. */
    fun toPlayRequest(): PlayRequest = PlayRequest(
        itemId = itemId,
        fileId = fileId,
        startMs = positionMs,
        audioIndex = audioIndex,
        subtitleIndex = subtitleIndex,
        queue = queue,
        drivePath = drivePath,
    )

    fun toBundle(): Bundle = Bundle().apply {
        putLong(ITEM, itemId)
        fileId?.let { putLong(FILE, it) }
        putLong(POSITION, positionMs)
        audioIndex?.let { putLong(AUDIO, it) }
        subtitleIndex?.let { putLong(SUBTITLE, it) }
        putString(QUALITY, quality)
        when (queue) {
            is PlayQueue.Playlist -> putLong(PLAYLIST, queue.playlistId)
            is PlayQueue.Collection -> putLong(COLLECTION, queue.collectionId)
            null -> Unit
        }
        drivePath?.let { putString(DRIVE, it) }
    }

    companion object {
        private const val ITEM = "pley.item"
        private const val FILE = "pley.file"
        private const val POSITION = "pley.position"
        private const val AUDIO = "pley.audio"
        private const val SUBTITLE = "pley.subtitle"
        private const val QUALITY = "pley.quality"
        private const val PLAYLIST = "pley.playlist"
        private const val COLLECTION = "pley.collection"
        private const val DRIVE = "pley.drive"

        /** What [toBundle] saved; `null` when [bundle] holds none. */
        fun fromBundle(bundle: Bundle?): SavedPlaybackState? {
            if (bundle == null || !bundle.containsKey(ITEM)) return null
            fun long(key: String) = if (bundle.containsKey(key)) bundle.getLong(key) else null
            return SavedPlaybackState(
                itemId = bundle.getLong(ITEM),
                fileId = long(FILE),
                positionMs = bundle.getLong(POSITION),
                audioIndex = long(AUDIO),
                subtitleIndex = long(SUBTITLE),
                quality = bundle.getString(QUALITY) ?: QualityPreset.ORIGINAL_ID,
                queue = long(PLAYLIST)?.let { PlayQueue.Playlist(it) } ?: long(COLLECTION)?.let { PlayQueue.Collection(it) },
                drivePath = bundle.getString(DRIVE),
            )
        }
    }
}
