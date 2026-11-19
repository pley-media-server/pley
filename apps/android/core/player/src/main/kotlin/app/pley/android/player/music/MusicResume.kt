package app.pley.android.player.music

import app.pley.android.data.ProfileScope
import app.pley.android.model.ItemKind
import app.pley.android.model.ItemSummary
import app.pley.android.model.PleyJson
import app.pley.android.player.MusicQueueState
import app.pley.android.player.MusicRepeat
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.launch
import kotlinx.serialization.builtins.ListSerializer

/** What music was playing: the queue's [tracks] in the order they played, which one and where in it, and what repeats. */
data class SavedMusic(
    val tracks: List<ItemSummary>,
    val index: Int,
    val positionMs: Long,
    val repeat: MusicRepeat = MusicRepeat.Off,
    /** Shuffled, where each of [tracks] comes in the order the queue was given (Shuffle off plays that); `null` otherwise. */
    val given: List<Int>? = null,
)

/** Makes [saved] the queue that waits for Play, as it was: in the order it played, shuffled or not, with what repeats. */
fun MusicPlayback.cue(saved: SavedMusic) {
    setRepeat(saved.repeat)
    cue(saved.tracks, saved.index, startMs = saved.positionMs, given = saved.given)
}

/**
 * What music was playing, kept on the device so Play brings it back when
 * the app is no longer in memory: Android ends an app that has been paused
 * a while, and the Play of a car's steering wheel, of Android Auto or of
 * the phone's media controls then found nothing to play. The queue is
 * written when it changes and the place in it as it moves ([keep]); a
 * queue stopped is forgotten, as it is for whoever stopped it. A Live TV
 * channel played as radio in a car is kept too ([keepRadio]), until music
 * plays again: the last of the two is what a car is shown.
 *
 * It is kept with the saved things of the profile that played it:
 * [profile] is the one signed in now, and [read] and [write] reach a
 * profile's snapshots whichever is, so a write still on its way as the
 * profile changes goes to the profile it is for. The queue playing on
 * after a switch of profile is kept for neither (the profile that played
 * it keeps what it had), so another profile never gets it; one of other
 * tracks, started since, is the profile's signed in now. Signing out
 * deletes it. A long queue keeps [MAX_TRACKS] tracks around the one
 * playing. Main thread only; the writing itself is the store's, off it.
 */
class MusicResume(
    private val profile: () -> ProfileScope?,
    private val read: suspend (profile: ProfileScope, key: String) -> ByteArray?,
    private val write: suspend (profile: ProfileScope, key: String, bytes: ByteArray) -> Unit,
    private val scope: CoroutineScope,
) {
    /** The profile the queue kept is that of: the one signed in as it was first kept, until it is forgotten. */
    private var owner: ProfileScope? = null

    /** The whole queue as last kept for [owner]: after a switch of profile, a queue of other tracks is a new one. */
    private var ownerQueue: List<ItemSummary> = emptyList()

    /** The tracks written last, where in the queue they start, and the order they were given in, written with them. */
    private var kept: List<ItemSummary>? = null
    private var keptFrom = 0
    private var keptGiven: List<Int>? = null

    /** Whether what was kept has been forgotten in this run, and nothing kept since. */
    private var forgotten = false

    /** The radio's channel as last kept or read ([keepRadio]), and the profile it is that of; unknown until then. */
    private var radio: String? = UNREAD
    private var radioOf: ProfileScope? = null

    /** What waits to be written, the latest of each profile's key, in order. */
    private val pending = LinkedHashMap<Pair<ProfileScope, String>, ByteArray>()
    private var writer: Job? = null

    /** Keeps [queue] and where its track is, [positionMs]; an empty queue forgets what was kept. */
    fun keep(queue: MusicQueueState, positionMs: Long) {
        val tracks = queue.tracks
        if (tracks.isEmpty()) {
            forget()
            return
        }
        val now = profile() ?: return
        if (owner != now) {
            // Since a switch of profile: the last profile's queue playing on, its tracks in whatever order, is neither's to
            // keep; a queue of other tracks has been started by the profile signed in now.
            if (owner != null && trackIds(tracks) == trackIds(ownerQueue)) return
            owner = now
            kept = null
            keptGiven = null
        }
        ownerQueue = tracks
        forgotten = false
        val last = kept
        // Of a long queue, the tracks kept stay the same while its track is among them: only the place is written then.
        val stays = last != null && keptFrom + last.size <= tracks.size && queue.index in keptFrom until keptFrom + last.size &&
            tracks.subList(keptFrom, keptFrom + last.size) == last
        val from = when {
            tracks.size <= MAX_TRACKS -> 0
            stays -> keptFrom
            else -> (queue.index - BEHIND).coerceIn(0, tracks.size - MAX_TRACKS)
        }
        val window = if (tracks.size <= MAX_TRACKS) tracks else tracks.subList(from, from + MAX_TRACKS)
        if (window != kept || from != keptFrom) {
            kept = window.toList()
            keptFrom = from
            put(now, QUEUE, PleyJson.encodeToString(TRACKS, window).encodeToByteArray())
        }
        // Shuffled, the tracks kept in the order they were given, among themselves: Shuffle off plays them so once back.
        val given = if (queue.shuffle && queue.given.size == tracks.size) ranks(queue.given.subList(from, from + window.size)) else emptyList()
        if (given != keptGiven) {
            keptGiven = given
            put(now, GIVEN, given.joinToString(" ").encodeToByteArray())
        }
        put(now, PLACE, "${queue.index - from} ${positionMs.coerceAtLeast(0)} ${queue.repeat.name}".encodeToByteArray())
    }

    /**
     * Keeps which channel the car's radio plays ([mediaId], as the car's
     * library names it), so it is what waits for Play when a car next
     * connects; `null` once music plays again.
     */
    fun keepRadio(mediaId: String?) {
        val now = profile() ?: return
        if (now == radioOf && mediaId == radio) return
        radioOf = now
        radio = mediaId
        put(now, RADIO, (mediaId ?: "").encodeToByteArray())
    }

    /** The channel kept by [keepRadio] for the profile in use, or `null`: music played last, or nothing did. */
    suspend fun savedRadio(): String? {
        writer?.join()
        val now = profile() ?: return null
        val kept = try {
            read(now, RADIO)?.decodeToString()?.takeIf { it.isNotEmpty() && it.length <= MAX_RADIO_ID }
        } catch (e: CancellationException) {
            throw e
        } catch (_: Exception) {
            null
        }
        radio = kept
        radioOf = now
        return kept
    }

    /**
     * Forgets what was kept: the music was stopped. Stopped after a switch
     * of profile, the profile that played it keeps what it had.
     */
    fun forget() {
        val who = owner ?: profile()
        owner = null
        ownerQueue = emptyList()
        kept = null
        keptGiven = null
        if (forgotten) return
        forgotten = true
        if (who == null || who != profile()) return
        // Empty: the store reads that as nothing kept.
        put(who, QUEUE, ByteArray(0))
        put(who, GIVEN, ByteArray(0))
        put(who, PLACE, ByteArray(0))
    }

    /** What was kept for the profile in use; `null` when nothing was, or it can't be read. */
    suspend fun saved(): SavedMusic? {
        // What is still on its way to the store is what was kept last.
        writer?.join()
        val now = profile() ?: return null
        val tracks = try {
            read(now, QUEUE)?.let { PleyJson.decodeFromString(TRACKS, it.decodeToString()) }
        } catch (e: CancellationException) {
            throw e
        } catch (_: Exception) {
            null
        }
        if (tracks.isNullOrEmpty() || tracks.size > MAX_TRACKS || tracks.any { it.kind != ItemKind.Track }) return null
        val place = read(now, PLACE)?.decodeToString()?.split(' ').orEmpty()
        val index = place.getOrNull(0)?.toIntOrNull()?.takeIf { it in tracks.indices } ?: 0
        val length = tracks[index].durationMs?.takeIf { it > 0 }
        // A track left in its last seconds, or at its end, starts over: brought back there it would end at once.
        val position = place.getOrNull(1)?.toLongOrNull()?.takeIf { it > 0 && (length == null || it < length - NEAR_END_MS) } ?: 0
        val repeat = MusicRepeat.entries.firstOrNull { it.name == place.getOrNull(2) } ?: MusicRepeat.Off
        // Kept for these tracks only when it places each once; otherwise they come back in the order they played.
        val given = read(now, GIVEN)?.decodeToString()?.split(' ')?.map { it.toIntOrNull() ?: -1 }
            ?.takeIf { it.size == tracks.size && it.toSet() == tracks.indices.toSet() }
        return SavedMusic(tracks, index, position, repeat, given)
    }

    private fun trackIds(tracks: List<ItemSummary>): Set<Long> = tracks.mapTo(HashSet(tracks.size)) { it.id }

    /** Where each of [positions] comes among them, smallest first. */
    private fun ranks(positions: List<Int>): List<Int> {
        val ranks = IntArray(positions.size)
        positions.indices.sortedBy { positions[it] }.forEachIndexed { rank, i -> ranks[i] = rank }
        return ranks.toList()
    }

    private fun put(whose: ProfileScope, key: String, bytes: ByteArray) {
        val at = whose to key
        pending.remove(at)
        pending[at] = bytes
        if (writer?.isActive == true) return
        writer = scope.launch {
            while (true) {
                val next = pending.entries.firstOrNull() ?: break
                pending.remove(next.key)
                write(next.key.first, next.key.second, next.value)
            }
        }
    }

    companion object {
        /** The most tracks kept: a queue longer than this (an artist's every album, shuffled) keeps those around its track. */
        const val MAX_TRACKS = 500

        /** Of a long queue, the tracks kept before the one playing: Previous goes back that far. */
        private const val BEHIND = 50

        /** Left this close to its end, a track starts over when brought back. */
        private const val NEAR_END_MS = 5_000L

        /** The snapshots' keys: the queue's tracks, the order they were given in when shuffled, and "index position repeat". */
        const val QUEUE = "music:queue"
        const val GIVEN = "music:given"
        const val PLACE = "music:place"

        /** The snapshot's key for the radio's channel, and the longest id read back. */
        const val RADIO = "music:radio"
        private const val MAX_RADIO_ID = 200

        /** Not a channel's id: nothing was kept or read in this run yet, so the first [keepRadio] writes. */
        private const val UNREAD = "unread"

        private val TRACKS = ListSerializer(ItemSummary.serializer())
    }
}
