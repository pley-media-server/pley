package app.pley.android.player.subtitles

import android.content.Context
import androidx.media3.common.text.Cue
import app.pley.android.common.AppLanguage
import java.util.Locale
import kotlin.math.abs
import kotlin.math.roundToLong
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/** A subtitle track's cues by time: text (the server's WebVTT) or pictures (the server's .mks). */
interface TimedCueTrack {
    /** The cues showing at [positionMs] of the track's own time. Pictures are decoded here. */
    suspend fun cuesAt(positionMs: Long): List<Cue>

    /** The next time after [positionMs] at which what shows changes; `null` when nothing changes any more. */
    fun nextChangeAfter(positionMs: Long): Long?
}

/**
 * Subtitles moved in time (design: the subtitle menu's "Sync"): in 0.1 s
 * steps from −10 s to +10 s; positive shows them later.
 */
object SubtitleSync {
    const val STEP_MS = 100L
    const val MAX_MS = 10_000L

    /** [offsetMs] rounded to a step and kept within ±10 s. */
    fun clamp(offsetMs: Long): Long = ((offsetMs / STEP_MS.toDouble()).roundToLong() * STEP_MS).coerceIn(-MAX_MS, MAX_MS)

    /** The track's own time when the playhead is at [positionMs]: a cue made for t shows at t + [offsetMs]. */
    fun trackTime(positionMs: Long, offsetMs: Long): Long = positionMs - offsetMs

    /** How long until what [track] shows changes, from the playhead at [positionMs]; `null` when nothing changes any more. */
    fun untilChange(track: TimedCueTrack, positionMs: Long, offsetMs: Long): Long? {
        val at = trackTime(positionMs, offsetMs)
        return track.nextChangeAfter(at)?.let { it - at }
    }

    /** "+0.5 s", "−1.2 s", "0.0 s", with [locale]'s decimal separator ("+0,5 s" in French). */
    fun label(offsetMs: Long, locale: Locale = AppLanguage.locale): String {
        val sign = when {
            offsetMs > 0 -> "+"
            offsetMs < 0 -> "−"
            else -> ""
        }
        return sign + String.format(locale, "%.1f s", abs(offsetMs) / 1000.0)
    }
}

/** Subtitle offsets, remembered per file and profile on this device (never on the server). */
interface SubtitleOffsets {
    /** The offset of the file under [key]; 0 when none was set. */
    suspend fun offset(key: String): Long

    /** Remembers [offsetMs] for [key]; 0 forgets it. */
    suspend fun remember(key: String, offsetMs: Long)

    companion object {
        /** Kept while the process lives: for tests and players without storage. */
        fun memory(): SubtitleOffsets = object : SubtitleOffsets {
            private val kept = HashMap<String, Long>()

            override suspend fun offset(key: String): Long = synchronized(kept) { kept[key] ?: 0 }

            override suspend fun remember(key: String, offsetMs: Long) {
                synchronized(kept) { if (offsetMs == 0L) kept.remove(key) else kept[key] = offsetMs }
            }
        }
    }
}

/** In a preferences file of the player's own ("subtitle_offsets"), an entry per file and profile whose offset isn't 0. */
class PreferencesSubtitleOffsets(context: Context, private val io: CoroutineDispatcher = Dispatchers.IO) : SubtitleOffsets {
    private val app = context.applicationContext
    private val prefs by lazy { app.getSharedPreferences(PREFS, Context.MODE_PRIVATE) }

    override suspend fun offset(key: String): Long = withContext(io) { prefs.getLong(key, 0) }

    override suspend fun remember(key: String, offsetMs: Long) = withContext(io) {
        val edit = prefs.edit()
        if (offsetMs == 0L) edit.remove(key) else edit.putLong(key, offsetMs)
        edit.apply()
    }

    private companion object {
        const val PREFS = "subtitle_offsets"
    }
}
