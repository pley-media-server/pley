package app.pley.android.data.drive

import android.content.Context

/** Where a video stopped and how long it is, as this device keeps it: zeros when it doesn't (its length unknown: 0). */
data class DriveKept(val positionMs: Long = 0, val durationMs: Long = 0)

/**
 * Where each video on a drive stopped, kept on this device by its path as
 * "position,length", as the Samsung app keeps it (`local.resume.<id>`).
 * Played past 95 %, it was watched: it is forgotten, and starts over next
 * time.
 */
class DriveResume(
    private val read: (key: String) -> String?,
    private val write: (key: String, value: String?) -> Unit,
) {
    /** Where [path] stopped last; zeros when it wasn't kept, or was watched. */
    fun kept(path: String): DriveKept {
        val parts = read(keyOf(path))?.split(',') ?: return DriveKept()
        val position = parts[0].toLongOrNull()?.coerceAtLeast(0) ?: return DriveKept()
        return DriveKept(position, parts.getOrNull(1)?.toLongOrNull()?.coerceAtLeast(0) ?: 0)
    }

    /** [path] played to [positionMs] of [durationMs]: kept, or forgotten once it was watched. */
    fun keep(path: String, positionMs: Long, durationMs: Long?) {
        val key = keyOf(path)
        when {
            durationMs != null && durationMs > 0 && positionMs >= durationMs * WATCHED -> write(key, null)
            positionMs > 0 -> write(key, "$positionMs,${durationMs?.coerceAtLeast(0) ?: 0}")
        }
    }

    companion object {
        /** Played this far through, it was watched. */
        const val WATCHED = 0.95

        /**
         * A stable id for the file at [path] (FNV-1a over its characters, as
         * the Samsung app's): its resume point and subtitle offset are kept
         * by it, and it is the file's id in the player.
         */
        fun fileIdOf(path: String): Long {
            var hash = 14695981039346656037uL
            for (c in path) {
                hash = hash xor c.code.toULong()
                hash *= 1099511628211uL
            }
            return (hash and 0x3fffffffffffffffuL).toLong()
        }

        private fun keyOf(path: String): String = "local.resume.${fileIdOf(path)}"

        /** Kept in memory only: tests, and devices without drives. */
        fun memory(): DriveResume {
            val values = HashMap<String, String>()
            return DriveResume({ synchronized(values) { values[it] } }) { key, value ->
                synchronized(values) { if (value == null) values.remove(key) else values[key] = value }
            }
        }

        /** In the app's own preferences ("drive_resume"); read off the main thread the first time. */
        fun preferences(context: Context): DriveResume {
            val prefs by lazy { context.applicationContext.getSharedPreferences(PREFERENCES, Context.MODE_PRIVATE) }
            return DriveResume({ prefs.getString(it, null) }) { key, value ->
                prefs.edit().apply { if (value == null) remove(key) else putString(key, value) }.apply()
            }
        }

        private const val PREFERENCES = "drive_resume"
    }
}
