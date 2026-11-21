package app.pley.android.player.music.library

import android.content.pm.ApplicationInfo
import android.content.pm.PackageManager
import android.os.Process
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
    fun allowed(packageManager: PackageManager, packageName: String): Boolean = allowed(packageManager, packageName, KEYS)

    /** [allowed], with the keys each app may be signed with ([KEYS]; a test's own in tests). */
    internal fun allowed(packageManager: PackageManager, packageName: String, keys: Map<String, List<String>>): Boolean {
        val wanted = keys[packageName] ?: return false
        return wanted.any { key ->
            try {
                PackageInfoCompat.hasSignatures(packageManager, packageName, mapOf(hex(key) to PackageManager.CERT_INPUT_SHA256), false)
            } catch (_: PackageManager.NameNotFoundException) {
                false
            }
        }
    }

    /**
     * Whether the caller named [name], asking from [uid], is Android Auto or the Google app: the name owned by that
     * uid, and the app under it signed by Google. Media3 takes a controller's name unchecked from an app this one can't
     * see (package visibility hides its packages), and from the system's controllers below Android 13. [own] is the
     * app's own name, never a car's.
     */
    fun isCar(packageManager: PackageManager, own: String, name: String, uid: Int): Boolean =
        isCar(packageManager, own, name, uid, KEYS)

    /** [isCar], with the keys each app may be signed with ([KEYS]; a test's own in tests). */
    internal fun isCar(packageManager: PackageManager, own: String, name: String, uid: Int, keys: Map<String, List<String>>): Boolean {
        if (name == own) return false
        val owner = runCatching { packageManager.getPackageUid(name, 0) }.getOrNull() ?: return false
        return appId(owner) == appId(uid) && allowed(packageManager, name, keys)
    }

    /**
     * Whether the caller named [name], asking from [uid], is the system's own (its media controls, Bluetooth, the lock
     * screen): a system uid, or a system app's under its own name. Media3 also trusts any app given access to
     * notifications (a watch's companion, an automation app), which may not drive the music.
     */
    fun isSystem(packageManager: PackageManager, name: String, uid: Int): Boolean {
        if (uid >= 0 && appId(uid) < Process.FIRST_APPLICATION_UID) return true
        val info = runCatching { packageManager.getApplicationInfo(name, 0) }.getOrNull() ?: return false
        return appId(info.uid) == appId(uid) && systemFlags(info)
    }

    /** Whether [name] is installed as part of the system: no other app can take a system app's name. */
    fun systemApp(packageManager: PackageManager, name: String): Boolean {
        val info = runCatching { packageManager.getApplicationInfo(name, 0) }.getOrNull() ?: return false
        return systemFlags(info)
    }

    /**
     * The app a [uid] is, whichever user it runs as: the system's media controls ask from the device's first user
     * while Pley may run in another (a work profile, a private space), where the same app has another uid. An app id
     * is one app's in every user, so no other app shares it.
     */
    private fun appId(uid: Int): Int = uid % PER_USER_RANGE

    /** The uids each user has (UserHandle.PER_USER_RANGE, which isn't public). */
    private const val PER_USER_RANGE = 100_000

    private fun systemFlags(info: ApplicationInfo): Boolean =
        info.flags and (ApplicationInfo.FLAG_SYSTEM or ApplicationInfo.FLAG_UPDATED_SYSTEM_APP) != 0

    private fun hex(text: String): ByteArray = ByteArray(text.length / 2) { text.substring(it * 2, it * 2 + 2).toInt(16).toByte() }
}
