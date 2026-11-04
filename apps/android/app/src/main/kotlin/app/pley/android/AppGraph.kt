package app.pley.android

import android.app.Application
import android.os.Build
import androidx.annotation.OptIn
import androidx.media3.common.util.Log as Media3Log
import androidx.media3.common.util.UnstableApi
import app.pley.android.auth.AuthGraph
import app.pley.android.caps.CapsDecision
import app.pley.android.caps.CapsGraph
import app.pley.android.caps.CapsSnapshot
import app.pley.android.common.DeviceProfile
import app.pley.android.common.UserAgent
import app.pley.android.data.DataGraph
import app.pley.android.data.DownloadTarget
import app.pley.android.data.Downloads
import app.pley.android.data.drive.DriveResume
import app.pley.android.data.drive.Drives
import app.pley.android.data.drive.StorageDrives
import app.pley.android.downloads.AppDownloads
import app.pley.android.network.HttpRoot
import app.pley.android.network.NetworkGraph
import app.pley.android.player.PlayerGraph
import app.pley.android.player.session.Qualities
import app.pley.android.screens.PlayerLauncher
import app.pley.android.screens.ScreensGraph
import app.pley.android.screens.remote.CastReceiver
import app.pley.android.watchnext.TvProviderRow
import app.pley.android.watchnext.WatchNext
import app.pley.android.watchnext.WatchNextImages
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.first
import okhttp3.OkHttp
import okhttp3.OkHttpClient

/**
 * The app's parts and how they connect, by hand (no injection framework).
 * Everything is made the first time it's asked for, so starting the app
 * builds nothing: the first frame never waits for a client, the Keystore
 * or the capability probe.
 */
class AppGraph(private val app: Application) : ScreensGraph {
    override val device: DeviceProfile by lazy { DeviceProfile.of(app) }
    val userAgent: String by lazy { UserAgent.of(BuildConfig.VERSION_NAME, device = UserAgent.deviceName(app)) }
    val root: OkHttpClient by lazy {
        // OkHttp's own startup step, which its androidx.startup initializer would
        // run on the main thread as the app starts (the manifest leaves it out):
        // its Android platform, with the app's context, before the first client.
        OkHttp.initialize(app)
        HttpRoot.create(userAgent)
    }
    // Signing in and the servers' proofs go where the rest goes, the home network's way.
    override val auth: AuthGraph by lazy { AuthGraph.create(app, NetworkGraph.homeRouted(app, root), userAgent) }
    override val network: NetworkGraph by lazy { NetworkGraph.create(app, root, auth, device, app.cacheDir) }
    override val data: DataGraph by lazy { DataGraph.create(app, network, auth, device) }
    override val caps: CapsGraph by lazy { CapsGraph.create(app, device) }
    override val player: PlayerGraph by lazy {
        quietPlayerLogs()
        PlayerGraph.create(app, data, network, caps, device, local = { localMedia }, drives = { drives })
    }

    /**
     * Phones and tablets only: TVs stay where the server is, and download
     * nothing. Of several versions, one this device plays as it is; the
     * server makes one that doesn't, or a smaller one when Settings →
     * Downloads asks, from what this device plays.
     */
    val appDownloads: AppDownloads by lazy {
        AppDownloads.create(
            app,
            data,
            network,
            playsHere = { item, file ->
                val snapshot = capsNow()
                snapshot == null ||
                    (caps.policy.decide(item, file.id, snapshot) == CapsDecision.Direct && AppDownloads.soundPlays(file, snapshot.report))
            },
            caps = { capsNow()?.report },
            target = {
                Qualities.byIdOrLabel(data.settings.settings.first().downloadQuality)
                    .takeUnless { it.original }
                    ?.let { DownloadTarget(maxHeight = it.maxHeight ?: 1080, maxBitrateKbps = it.maxBitrateKbps ?: 8_000) }
            },
        )
    }

    /** This device's capabilities, probed if they weren't; `null` when they can't be. */
    private suspend fun capsNow(): CapsSnapshot? = try {
        caps.capabilities.warmUp()
    } catch (e: CancellationException) {
        throw e
    } catch (_: Exception) {
        null
    }
    override val downloads: Downloads get() = if (device.isTv) Downloads.None else appDownloads

    /** What the player plays from the device: the downloads, on phones and tablets. */
    private val localMedia: Downloads get() = downloads

    /**
     * TVs only: the USB drive plugged in, whose videos play without a
     * server, and where each stopped. Made when first asked for (the
     * rail, off the main thread).
     */
    override val drives: Drives by lazy { if (device.isTv) StorageDrives(app, DriveResume.preferences(app)) else Drives.None }
    override val playerLauncher: PlayerLauncher by lazy { PlayerLauncherImpl(app, { player }, device) }

    /** What the profile's remotes cast here, played on the main thread for the whole life of the app. */
    override val casts: CastReceiver by lazy {
        CastReceiver.of(data, playerLauncher, { player.music }, CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate))
    }

    /**
     * The home screen's Watch Next row: Android TV and Google TV only
     * (Android 8 and later); Fire TV's home screen has none, and phones no
     * such row.
     */
    val watchNext: WatchNext? by lazy {
        if (device.isTv && !device.fireTv && Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            WatchNext({ data }, { network }, TvProviderRow(app), WatchNextImages(app) { network.images })
        } else {
            null
        }
    }

    /**
     * Release builds log Media3's errors by their message alone: the causes
     * under them name what was being read, such as a video on a USB drive
     * by its file's name, which stays out of the device's log.
     */
    @OptIn(markerClass = [UnstableApi::class])
    private fun quietPlayerLogs() {
        if (!BuildConfig.DEBUG) Media3Log.setLogStackTraces(false)
    }
}
