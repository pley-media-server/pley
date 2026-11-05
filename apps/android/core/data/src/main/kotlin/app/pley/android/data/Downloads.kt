package app.pley.android.data

import app.pley.android.common.UiText
import app.pley.android.model.ItemDetail
import app.pley.android.model.ItemKind
import app.pley.android.model.ItemSummary
import app.pley.android.model.ProgressReport
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

/** A downloaded file on this device, as the player opens it. */
data class LocalFile(
    /** The video: `file:///…/media.mkv`. */
    val uri: String,
    /** Subtitles saved beside it, by the stream's index: `file:///…/subs/3.vtt`. */
    val subtitles: Map<Int, String> = emptyMap(),
    /** Made by the server for this device: it holds the subtitle tracks a conversion keeps, in their order. */
    val converted: Boolean = false,
)

/**
 * What the player takes from downloads, for the signed-in server and
 * profile: the file to play instead of the server's, the item's details
 * while the server is out of reach, and progress that waits until the
 * server can be told.
 */
interface LocalMedia {
    /** The downloaded copy of file [fileId], whole; `null` when there is none. */
    fun file(fileId: Long): LocalFile? = null

    /** The details saved with a download of [itemId]; `null` when there is none. */
    suspend fun detail(itemId: Long): ItemDetail? = null

    /**
     * [report] of [itemId] played from a download: sent now when the
     * server can be reached, else kept and sent (with its age) when it can.
     * It never fails: what can't be sent is kept.
     */
    suspend fun report(itemId: Long, report: ProgressReport) {}

    /**
     * The episode to play after [itemId] while the server is away: the
     * next of its show downloaded whole, by season and number, with the
     * picture saved beside it; `null` when there is none.
     */
    suspend fun nextAfter(itemId: Long): ItemSummary? = null

    companion object {
        /** No downloads: TVs, tests. */
        val None: LocalMedia = object : LocalMedia {}
    }
}

/** Where a download is. */
enum class DownloadState {
    /** Waiting its turn. */
    Queued,

    Downloading,

    /**
     * Waiting for a network it may use (Wi-Fi, unless mobile data is
     * allowed), or for the server to answer ([DownloadEntry.serverAway]).
     */
    Waiting,

    /** Stopped by an error: [DownloadEntry.error] says which; retried by hand. */
    Failed,

    /** Whole, and playable without the server. */
    Done,
}

/** A download of a movie or an episode, for lists and the item page. */
data class DownloadEntry(
    val itemId: Long,
    val fileId: Long,
    val kind: ItemKind,
    val title: String,
    /** An episode's show and its numbers, for grouping. */
    val showId: Long? = null,
    val showTitle: String? = null,
    val seasonIndex: Int? = null,
    val episodeIndex: Int? = null,
    val year: Int? = null,
    val durationMs: Long? = null,
    val sizeBytes: Long,
    val downloadedBytes: Long = 0,
    val state: DownloadState,
    /** The picture saved with it (a still, else the poster): `file:///…`. */
    val imageUri: String? = null,
    /** When it was asked for, unix ms: newest first. */
    val addedAt: Long,
    /** Where it was left last, played here: for Resume. */
    val positionMs: Long = 0,
    val watched: Boolean = false,
    /** Why it failed, one short sentence, put into words where it shows. */
    val error: UiText? = null,
    /** While the server makes this device's file: how far it has come, 0 to 1. */
    val converting: Float? = null,
    /** Waiting because the server didn't answer (away from home, or its transcoder down), not for Wi-Fi. */
    val serverAway: Boolean = false,
) {
    /** How much is down, 0 to 1. */
    val fraction: Float get() = if (sizeBytes <= 0) 0f else (downloadedBytes.toFloat() / sizeBytes).coerceIn(0f, 1f)
}

/** A download made smaller by the server: its picture's height at most, and its bitrate. */
data class DownloadTarget(val maxHeight: Int, val maxBitrateKbps: Int)

/** How much the downloads take, and what is left where they go. */
data class DownloadStorage(val usedBytes: Long = 0, val freeBytes: Long = 0)

/**
 * Downloads, for watching away from the server: the original file of a
 * movie or an episode, with its details, pictures and subtitles. They
 * belong to the server and profile they were made with; [entries] lists
 * the signed-in one's.
 */
interface Downloads : LocalMedia {
    /** The signed-in profile's downloads, newest first. */
    val entries: StateFlow<List<DownloadEntry>>

    val storage: StateFlow<DownloadStorage>

    /**
     * Downloads the movies and episodes [itemIds] (a season's, a show's),
     * in that order. Throws what the server or the network does when the
     * items can't be read, so the screen can say so.
     */
    suspend fun download(itemIds: List<Long>) {}

    /** Stops and deletes the download of [itemId], or deletes it when done. */
    fun remove(itemId: Long) {}

    /** Deletes every download of every server and profile. */
    fun removeAll() {}

    /** Tries a failed download again. */
    fun retry(itemId: Long) {}

    /** Starts downloads still waiting: the app came to the foreground, or a network did. */
    fun resume() {}

    companion object {
        /** No downloads (TVs, tests): nothing listed, nothing done. */
        val None: Downloads = object : Downloads {
            override val entries: StateFlow<List<DownloadEntry>> = MutableStateFlow<List<DownloadEntry>>(emptyList()).asStateFlow()
            override val storage: StateFlow<DownloadStorage> = MutableStateFlow(DownloadStorage()).asStateFlow()
        }
    }
}
