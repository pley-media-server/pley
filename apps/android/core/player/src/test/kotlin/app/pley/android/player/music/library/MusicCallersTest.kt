package app.pley.android.player.music.library

import android.content.pm.ApplicationInfo
import android.content.pm.PackageInfo
import android.content.pm.PackageManager
import android.content.pm.Signature
import androidx.test.core.app.ApplicationProvider
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import java.security.MessageDigest

/**
 * Who the music service lets in besides the app: Android Auto and the Google app only as the app that asks and signed
 * by Google, the system only by a system uid or a system app's own name. A test certificate stands in for Google's.
 */
@RunWith(RobolectricTestRunner::class)
class MusicCallersTest {
    private val packages: PackageManager = ApplicationProvider.getApplicationContext<android.content.Context>().packageManager
    private val googles = Signature(byteArrayOf(1, 2, 3, 4))
    private val keys = mapOf(MusicCallers.ANDROID_AUTO to listOf("00".repeat(32), sha256(googles)))

    @Before
    fun install() {
        install(MusicCallers.ANDROID_AUTO, AUTO_UID, googles)
        install(OTHER, OTHER_UID, Signature(byteArrayOf(9, 9)))
        install(SYSTEM_UI, SYSTEM_UI_UID, Signature(byteArrayOf(7)), ApplicationInfo.FLAG_SYSTEM)
        install(NOTIFICATIONS, NOTIFICATIONS_UID, Signature(byteArrayOf(8)))
    }

    // Before Android 9, hasSignatures reads the package's signatures, which Robolectric keeps; from 9 it asks the
    // system, which Robolectric doesn't answer.
    @Test
    @Config(sdk = [27])
    fun androidAutoAskingAsItselfIsACarWithAnyOneOfItsKeys() {
        assertTrue(MusicCallers.isCar(packages, OWN, MusicCallers.ANDROID_AUTO, AUTO_UID, keys))
    }

    @Test
    @Config(sdk = [27])
    fun anAppGivingAndroidAutosNameIsNoCar() {
        // Media3 takes the name unchecked from an app this one can't see: the name isn't the asking uid's.
        assertFalse(MusicCallers.isCar(packages, OWN, MusicCallers.ANDROID_AUTO, OTHER_UID, keys))
    }

    @Test
    @Config(sdk = [27])
    fun anAppInstalledUnderAndroidAutosNameButNotSignedByGoogleIsNoCar() {
        install(MusicCallers.ANDROID_AUTO, AUTO_UID, Signature(byteArrayOf(5, 6)))
        assertFalse(MusicCallers.isCar(packages, OWN, MusicCallers.ANDROID_AUTO, AUTO_UID, keys))
    }

    @Test
    fun anotherAppOrNoneIsNoCar() {
        assertFalse(MusicCallers.isCar(packages, OWN, OTHER, OTHER_UID, keys))
        assertFalse(MusicCallers.isCar(packages, OWN, "com.example.missing", OTHER_UID, keys))
        // The app's own name is never a car's, whoever asks.
        assertFalse(MusicCallers.isCar(packages, OWN, OWN, OTHER_UID, keys))
    }

    @Test
    fun aSystemUidIsTheSystems() {
        assertTrue(MusicCallers.isSystem(packages, "android.media.session.MediaController", 1002))
    }

    @Test
    fun aSystemAppAskingAsItselfIsTheSystems() {
        assertTrue(MusicCallers.isSystem(packages, SYSTEM_UI, SYSTEM_UI_UID))
    }

    @Test
    fun aSystemAppAskingFromTheFirstUserIsTheSystemsWherePleyRunsInAnother() {
        // Pley in a work profile (user 10) sees System UI under user 10's uid; its media controls ask from user 0.
        install(SYSTEM_UI, 10 * 100_000 + SYSTEM_UI_UID, Signature(byteArrayOf(7)), ApplicationInfo.FLAG_SYSTEM)
        assertTrue(MusicCallers.isSystem(packages, SYSTEM_UI, SYSTEM_UI_UID))
        assertTrue(MusicCallers.isSystem(packages, "android", 10 * 100_000 + 1000))
    }

    @Test
    fun anAppGivingASystemAppsNameIsNotTheSystems() {
        assertFalse(MusicCallers.isSystem(packages, SYSTEM_UI, NOTIFICATIONS_UID))
        assertFalse(MusicCallers.isSystem(packages, SYSTEM_UI, 10 * 100_000 + NOTIFICATIONS_UID))
    }

    @Test
    fun anAppMedia3TrustsForItsNotificationAccessIsNotTheSystems() {
        assertFalse(MusicCallers.isSystem(packages, NOTIFICATIONS, NOTIFICATIONS_UID))
        assertFalse(MusicCallers.isSystem(packages, "com.example.missing", NOTIFICATIONS_UID))
    }

    @Test
    fun onlyAnAppInstalledAsPartOfTheSystemIsASystemApp() {
        assertTrue(MusicCallers.systemApp(packages, SYSTEM_UI))
        assertFalse(MusicCallers.systemApp(packages, NOTIFICATIONS))
        assertFalse(MusicCallers.systemApp(packages, "com.example.missing"))
    }

    private fun install(name: String, uid: Int, signature: Signature, flags: Int = 0) {
        val app = ApplicationInfo().apply {
            packageName = name
            this.uid = uid
            this.flags = flags
        }
        @Suppress("DEPRECATION")
        val info = PackageInfo().apply {
            packageName = name
            applicationInfo = app
            signatures = arrayOf(signature)
        }
        shadowOf(packages).installPackage(info)
    }

    private fun sha256(signature: Signature): String =
        MessageDigest.getInstance("SHA-256").digest(signature.toByteArray()).joinToString("") { "%02x".format(it) }

    private companion object {
        const val OWN = "app.pley.android"
        const val OTHER = "com.example.other"
        const val SYSTEM_UI = "com.android.systemui"
        const val NOTIFICATIONS = "com.example.watch"
        const val AUTO_UID = 10_101
        const val OTHER_UID = 10_102
        const val SYSTEM_UI_UID = 10_103
        const val NOTIFICATIONS_UID = 10_104
    }
}
