package app.pley.android.player.music.library

import android.content.pm.PackageManager
import androidx.core.content.pm.PackageInfoCompat

/**
 * The apps besides the system that may browse and play Pley's music:
 * Android Auto (on the phone, for the car's screen) and the Google app
 * (its Assistant's "play … on Pley"). Each is checked by its package and
 * the certificate Google signs it with, so an app installed under one of
 * these names doesn't pass. The keys are the release ones Google's media
 * sample lists (android/uamp, allowed_media_browser_callers.xml).
 */
object MusicCallers {
    /** Android Auto: its release key and the one it rotated to. */
    const val ANDROID_AUTO = "com.google.android.projection.gearhead"
    const val GOOGLE = "com.google.android.googlequicksearchbox"

    private val KEYS: Map<String, List<String>> = mapOf(
        ANDROID_AUTO to listOf(
            "fdb00c43dbde8b51cb312aa81d3b5fa17713adb94b28f598d77f8eb89daceedf",
            "1ca8dcc0bed3cbd872d2cb791200c0292ca9975768a82d676b8b424fb65b5295",
        ),
        GOOGLE to listOf("f0fd6c5b410f25cb25c3b53346c8972fae30f8ee7411df910480ad6b2d60db83"),
    )

    /**
     * Whether [packageName] is one of these apps, signed by Google: with any of its keys, each asked about alone,
     * as hasSignatures given several wants every one of them (an Android Auto signed before its key rotated has the
     * first alone).
     */
    fun allowed(packageManager: PackageManager, packageName: String): Boolean {
        val keys = KEYS[packageName] ?: return false
        return keys.any { key ->
            try {
                PackageInfoCompat.hasSignatures(packageManager, packageName, mapOf(hex(key) to PackageManager.CERT_INPUT_SHA256), false)
            } catch (_: PackageManager.NameNotFoundException) {
                false
            }
        }
    }

    private fun hex(text: String): ByteArray = ByteArray(text.length / 2) { text.substring(it * 2, it * 2 + 2).toInt(16).toByte() }
}
