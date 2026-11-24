package app.pley.android.player

import android.content.Context
import app.pley.android.caps.CapsGraph
import app.pley.android.caps.CapsSettings
import app.pley.android.caps.ModeSwitching
import app.pley.android.caps.Override
import app.pley.android.caps.PassthroughFormat
import app.pley.android.common.DeviceProfile
import app.pley.android.data.DataGraph
import app.pley.android.data.FrameRateMatching
import app.pley.android.data.LiveTvRepository
import app.pley.android.data.LocalMedia
import app.pley.android.data.Passthrough
import app.pley.android.data.Settings
import app.pley.android.data.drive.Drives
import app.pley.android.network.NetworkGraph
import app.pley.android.player.engine.EngineSettings
import app.pley.android.player.engine.Media3EngineFactory
import app.pley.android.player.engine.RefreshRateMatching
import app.pley.android.player.live.LivePlayback
import app.pley.android.player.music.ServiceMusicControl
import app.pley.android.player.music.library.MusicLibraryGraph
import app.pley.android.player.session.SessionPlaybackServices
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.launch
import kotlinx.coroutines.withTimeoutOrNull

/**
 * Implemented by the app's Application: what Android starts by itself (the
 * music service, the pictures it hands cars) reaches the player through it.
 */
interface PlayerGraphOwner {
    val playerGraph: PlayerGraph
}

/** Playback for the rest of the app: engines, sessions and music. */
interface PlayerGraph {
    val engines: PlayerEngine.Factory
    val services: PlaybackServices
    val music: MusicControl

    /** Music as cars and voice assistants browse it (MusicService); none unless the graph has it. */
    val musicLibrary: MusicLibraryGraph? get() = null

    /**
     * Ends what plays here, video, music and Live TV, and waits (up to
     * [END_WAIT_MS]) until each one's last reports went out: `stopped` where
     * it was, and its session's end. Their requests carry the tokens in use
     * as they go, so what hasn't gone out by then is dropped: a profile
     * switch made after this sends nothing of theirs as the next profile.
     * Main thread.
     */
    suspend fun endAll() {}

    companion object {
        /** How long [endAll] waits for what played to say it ended. */
        const val END_WAIT_MS = 5_000L

        /**
         * Media3 engines by device, following the app's settings (refresh
         * rate, tunneling, passthrough), server sessions over them for the
         * server and profile signed in to now, files played from [local]'s
         * downloads and from [drives], and music in the background.
         */
        fun create(
            context: Context,
            data: DataGraph,
            network: NetworkGraph,
            caps: CapsGraph,
            profile: DeviceProfile,
            local: () -> LocalMedia = { LocalMedia.None },
            drives: () -> Drives = { Drives.None },
        ): PlayerGraph {
            val settings = FollowedSettings(data, caps, profile)
            val engines = Media3EngineFactory.of(caps, network) { settings.engine }
            val services = SessionPlaybackServices.create(context, data, network, caps, profile, engines, local, drives)
            return AppPlayer(
                engines,
                services,
                { data.liveTv },
                library = {
                    // The car's Live TV plays as radio, watched as the phone's Live TV player watches.
                    MusicLibraryGraph(context, data, network) { scope ->
                        LivePlayback.create(
                            context,
                            data.liveTv,
                            engines,
                            profile,
                            scope,
                            data.liveUpdates.events,
                            data.liveUpdates::playing,
                            data.remote::here,
                            radio = true,
                        ) { caps.capabilities.snapshot.value }
                    }
                },
            ) {
                // The library above names every track's cover for the session, which reads it itself.
                ServiceMusicControl.create(context, data, network, caps, profile, engines, libraryCovers = true)
            }
        }
    }
}

internal class AppPlayer(
    override val engines: PlayerEngine.Factory,
    override val services: PlaybackServices,
    private val liveTv: () -> LiveTvRepository,
    /** Closes the player windows open ([PlayerWindows.closeAll]). */
    private val closeWindows: () -> Unit = PlayerWindows::closeAll,
    /** Waits until they have gone ([PlayerWindows.gone]). */
    private val windowsGone: suspend () -> Unit = PlayerWindows::gone,
    library: () -> MusicLibraryGraph? = { null },
    music: () -> MusicControl,
) : PlayerGraph {
    private val musicMade = lazy(music)

    override val musicLibrary: MusicLibraryGraph? by lazy(library)

    override val music: MusicControl by musicMade

    override suspend fun endAll() {
        // Each ends at once, its last reports queued while the tokens are still its profile's; music only if it
        // was ever made.
        services.closeAll()
        val made = if (musicMade.isInitialized()) music else null
        made?.stop()
        // Live TV's window leaves its channel as it stops.
        closeWindows()
        // All waited for at once: one slow to go out doesn't keep the others from being waited for.
        withTimeoutOrNull(PlayerGraph.END_WAIT_MS) {
            coroutineScope {
                launch { services.flush() }
                launch { made?.flush() }
                launch {
                    windowsGone()
                    liveTv().left()
                }
            }
        }
        // What hasn't gone out by now would go with the next profile's tokens.
        services.drop()
        made?.drop()
        liveTv().drop()
    }
}

/**
 * The settings the engines and the capability probe follow (Settings →
 * Playback, Audio and Advanced), kept up to date from the app's settings.
 */
private class FollowedSettings(data: DataGraph, caps: CapsGraph, profile: DeviceProfile) {
    @Volatile
    var engine: EngineSettings = EngineSettings()
        private set

    init {
        CoroutineScope(SupervisorJob() + Dispatchers.Default).launch {
            try {
                data.settings.settings.collect { s ->
                    engine = engineSettings(s, profile.isTv)
                    caps.capabilities.updateSettings(capsSettings(s, profile.isTv))
                }
            } catch (e: CancellationException) {
                throw e
            } catch (_: Exception) {
                // The settings can't be read: the defaults stay.
            }
        }
    }
}

/** The engine's view of [settings] on a phone ([tv] false) or a TV. */
internal fun engineSettings(settings: Settings, tv: Boolean): EngineSettings = EngineSettings(
    refreshRate = when (settings.frameRateMatching(tv)) {
        FrameRateMatching.Off -> RefreshRateMatching.Off
        FrameRateMatching.Always -> RefreshRateMatching.Always
        // Phones switch only seamlessly, which Media3 decides by itself.
        FrameRateMatching.Auto, FrameRateMatching.Seamless -> RefreshRateMatching.Auto
    },
    tunneling = tv && settings.tunneling,
)

/**
 * The capability probe's view of [settings] on a phone ([tv] false) or a TV:
 * passthrough per format, the display modes a TV switches to, software
 * video in debug builds.
 */
internal fun capsSettings(settings: Settings, tv: Boolean): CapsSettings = CapsSettings(
    passthrough = PASSTHROUGH.mapNotNull { (key, format) ->
        when (settings.passthrough(key)) {
            Passthrough.On -> format to Override.On
            Passthrough.Off -> format to Override.Off
            Passthrough.Auto -> null
        }
    }.toMap(),
    allowSoftwareDecoders = settings.allowSoftwareVideo,
    modeSwitching = when (settings.frameRateMatching(tv)) {
        FrameRateMatching.Off -> ModeSwitching.Off
        FrameRateMatching.Always -> ModeSwitching.Always
        FrameRateMatching.Auto, FrameRateMatching.Seamless -> ModeSwitching.Auto
    },
)

/** Settings' passthrough keys and the probe's formats. */
private val PASSTHROUGH = listOf(
    "ac3" to PassthroughFormat.DolbyDigital,
    "eac3" to PassthroughFormat.DolbyDigitalPlus,
    "eac3-joc" to PassthroughFormat.Atmos,
    "truehd" to PassthroughFormat.TrueHd,
    "dts" to PassthroughFormat.Dts,
    "dts-hd" to PassthroughFormat.DtsHd,
)
