package app.pley.android.downloads

import app.pley.android.common.sha1Hex
import app.pley.android.data.DownloadState
import app.pley.android.model.ItemKind
import java.io.File
import java.io.IOException
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.serialization.Serializable
import kotlinx.serialization.builtins.ListSerializer
import kotlinx.serialization.json.Json

/** A download, as the index keeps it. */
@Serializable
internal data class Saved(
    /** The server's origin, as it signs in ("http://nas:7539"). */
    val server: String,
    /** The profile that asked for it. */
    val userId: Long,
    val itemId: Long,
    val fileId: Long,
    val kind: ItemKind,
    val title: String,
    val showId: Long? = null,
    val showTitle: String? = null,
    val seasonIndex: Int? = null,
    val episodeIndex: Int? = null,
    val year: Int? = null,
    val durationMs: Long? = null,
    /** The file's container ("mkv"), its saved name's extension. */
    val container: String? = null,
    val sizeBytes: Long = 0,
    val downloadedBytes: Long = 0,
    val state: DownloadState = DownloadState.Queued,
    /** The details, picture and subtitles are saved. */
    val extrasSaved: Boolean = false,
    /** A picture is saved (image.jpg). */
    val image: Boolean = false,
    /** The subtitle tracks saved as WebVTT (subs/<index>.vtt). */
    val subtitles: List<Int> = emptyList(),
    /** Those of [subtitles] saved before the server had read them whole out of the file: asked for again. */
    val partialSubtitles: List<Int> = emptyList(),
    /** When it was asked for, unix ms. */
    val addedAt: Long,
    /** Where it was left, played here. */
    val positionMs: Long = 0,
    val watched: Boolean = false,
    /** Why it failed, worded where it shows ([errorText]). */
    val failure: Failure? = null,
    /** The server's own words of a [Failure.ConversionFailed]; without a [failure], why it failed as an older version wrote it. */
    val error: String? = null,
    /** The server's answer of a [Failure.ServerFailed]. */
    val status: Int? = null,
    /** The server makes the file for this device ([maxHeight] and [maxBitrateKbps] when smaller). */
    val convert: Boolean = false,
    val maxHeight: Int? = null,
    val maxBitrateKbps: Int? = null,
    /** The server's conversion, once asked for. */
    val conversion: String? = null,
    /** How far the server has come making it, 0 to 1, until it is whole. */
    val converting: Float? = null,
    /** Where the converted file comes from, once whole. */
    val mediaUrl: String? = null,
    /** Not to be looked at again before this (unix ms): its conversion is being made. */
    val notBefore: Long = 0,
    /** It waits because the server didn't answer, rather than for a network. */
    val unreachable: Boolean = false,
) {
    val key: Key get() = Key(server, userId, itemId)
}

/** A download's identity: an item of a server, for one of its profiles. */
internal data class Key(val server: String, val userId: Long, val itemId: Long)

/**
 * Where downloads are kept: a folder per server, profile and item under
 * [root] (the media, its details, a picture and subtitles), and the index
 * of them all, written whole to index.json (through a temporary file, so a
 * crash never leaves half of one). Changes of how much is down stay in
 * memory: the size of the partial file says it again after a restart.
 */
internal class DownloadStore(private val root: File) {
    private val state = MutableStateFlow<List<Saved>>(emptyList())

    /** Every download of every server and profile, oldest first. */
    val all: StateFlow<List<Saved>> = state.asStateFlow()

    private val index = File(root, INDEX)

    /**
     * Reads the index (off the main thread), once. A download cut short by
     * the process ending waits its turn again, from what its file holds.
     */
    fun load() {
        val read = try {
            if (index.isFile) StoreJson.decodeFromString(ListSerializer(Saved.serializer()), index.readText()) else emptyList()
        } catch (_: Exception) {
            emptyList()
        }
        state.value = read.map { s ->
            when (s.state) {
                DownloadState.Downloading, DownloadState.Waiting -> s.copy(state = DownloadState.Queued, downloadedBytes = partOf(s).length())
                DownloadState.Done -> if (mediaOf(s).isFile) s else s.copy(state = DownloadState.Failed, failure = Failure.Missing, error = null)
                else -> s
            }
        }
    }

    fun dirOf(s: Saved): File = File(root, "${hash(s.server)}/${s.userId}/${s.itemId}")

    fun mediaOf(s: Saved): File = File(dirOf(s), "media.${extension(s)}")

    /**
     * The file while it comes: the original's, or one per conversion (a
     * conversion made anew, after its file changed, never continues another).
     */
    fun partOf(s: Saved): File {
        val made = s.conversion?.takeIf { s.mediaUrl != null }?.let { ".$it" }.orEmpty()
        return File(dirOf(s), "media.${extension(s)}$made.part")
    }

    fun detailOf(s: Saved): File = File(dirOf(s), "detail.json")

    fun imageOf(s: Saved): File = File(dirOf(s), "image.jpg")

    fun subtitleOf(s: Saved, index: Int): File = File(dirOf(s), "subs/$index.vtt")

    fun get(key: Key): Saved? = state.value.firstOrNull { it.key == key }

    /** Adds [entries] that aren't there yet, at the end; returns those added. */
    @Synchronized
    fun add(entries: List<Saved>): List<Saved> {
        val known = state.value.map { it.key }.toSet()
        val added = entries.filter { it.key !in known }.distinctBy { it.key }
        if (added.isNotEmpty()) {
            state.update { it + added }
            save()
        }
        return added
    }

    /** Changes the download of [key] ([change] gives `null` to remove it); saved unless [persist] is false. */
    @Synchronized
    fun update(key: Key, persist: Boolean = true, change: (Saved) -> Saved?): Saved? {
        var result: Saved? = null
        state.update { list ->
            list.mapNotNull { s ->
                if (s.key != key) {
                    s
                } else {
                    change(s).also { result = it }
                }
            }
        }
        if (persist) save()
        return result
    }

    /** Removes the download of [key] and its folder (first: on a full device that is what frees room). */
    @Synchronized
    fun remove(key: Key) {
        val s = get(key) ?: return
        dirOf(s).deleteRecursively()
        update(key) { null }
    }

    /** Removes every download, and every file under the root. */
    @Synchronized
    fun clear() {
        state.value = emptyList()
        root.listFiles()?.forEach { it.deleteRecursively() }
        save()
    }

    /** The bytes the downloads take. */
    fun usedBytes(): Long = root.walkBottomUp().filter { it.isFile }.sumOf { it.length() }

    /** What is free where the downloads go. */
    fun freeBytes(): Long = root.apply { mkdirs() }.usableSpace

    /**
     * Writes the index. A device too full to take it keeps the one before:
     * what changed lives on in memory, and the next save writes it all.
     */
    private fun save() {
        val temp = File(root, "$INDEX.tmp")
        try {
            root.mkdirs()
            temp.writeText(StoreJson.encodeToString(ListSerializer(Saved.serializer()), state.value))
            if (!temp.renameTo(index)) {
                index.delete()
                temp.renameTo(index)
            }
        } catch (_: IOException) {
            temp.delete()
        }
    }

    private fun extension(s: Saved): String = s.container?.lowercase()?.filter { it.isLetterOrDigit() }?.ifEmpty { null } ?: "media"

    companion object {
        const val INDEX = "index.json"

        /** A folder name for [server]: its address could hold anything. */
        fun hash(server: String): String = sha1Hex(server).take(16)
    }
}

/** The index's JSON: what isn't known any more is skipped, defaults stay out. */
internal val StoreJson: Json = Json {
    ignoreUnknownKeys = true
    coerceInputValues = true
}
