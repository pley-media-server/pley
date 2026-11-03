package app.pley.android.player.engine

import android.annotation.SuppressLint
import android.content.Context
import android.hardware.display.DisplayManager
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import android.util.Log
import android.view.Display
import android.view.Window
import androidx.annotation.OptIn
import androidx.media3.common.AudioAttributes
import androidx.media3.common.C
import androidx.media3.common.MimeTypes
import androidx.media3.common.PlaybackException
import androidx.media3.common.Player
import androidx.media3.common.TrackSelectionParameters.AudioOffloadPreferences
import androidx.media3.common.Tracks
import androidx.media3.common.util.Clock
import androidx.media3.common.util.UnstableApi
import androidx.media3.exoplayer.ExoPlayer
import androidx.media3.exoplayer.SeekParameters
import androidx.media3.exoplayer.analytics.AnalyticsListener
import androidx.media3.exoplayer.audio.AudioSink
import androidx.media3.exoplayer.source.LoadEventInfo
import androidx.media3.exoplayer.source.MediaLoadData
import androidx.media3.exoplayer.trackselection.AdaptiveTrackSelection
import androidx.media3.exoplayer.trackselection.DefaultTrackSelector
import androidx.media3.exoplayer.trackselection.ExoTrackSelection
import app.pley.android.caps.AndroidDisplayInfo
import app.pley.android.caps.CapsDetails
import app.pley.android.caps.CapsGraph
import app.pley.android.caps.CapsSnapshot
import app.pley.android.caps.DecoderPolicy
import app.pley.android.caps.DeviceCapabilities
import app.pley.android.caps.DisplayState
import app.pley.android.common.DeviceProfile
import app.pley.android.model.DynamicRange
import app.pley.android.model.MediaStream
import app.pley.android.network.NetworkGraph
import app.pley.android.player.EngineSource
import app.pley.android.player.PlayerEngine
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import okhttp3.OkHttpClient
import java.io.IOException

/** The playback settings the engine follows (Settings → Playback and Advanced). */
data class EngineSettings(
    val refreshRate: RefreshRateMatching = RefreshRateMatching.Auto,
    /** Advanced: tunneled playback on a model not on the allowlist. */
    val tunneling: Boolean = false,
)

/**
 * Engines on Media3 for this device: its caps decide the decoders, the
 * TV's display modes and Dolby Vision; [mediaClient] gives each server's
 * media client (Bearer to that server only). Engines are made and used on
 * the main thread.
 */
class Media3EngineFactory(
    private val capabilities: DeviceCapabilities,
    private val mediaClient: (origin: String) -> OkHttpClient,
    private val settings: () -> EngineSettings = { EngineSettings() },
) : PlayerEngine.Factory {
    override fun create(context: Context, profile: DeviceProfile, music: Boolean): PlayerEngine = Media3Engine(
        context = context,
        profile = profile,
        music = music,
        caps = { capabilities.snapshot.value },
        mediaClient = mediaClient,
        settings = settings,
        frameRateMemory = PreferencesFrameRateMemory(context),
        focusPort = if (music) null else AndroidFocusPort(context),
    )

    companion object {
        fun of(caps: CapsGraph, network: NetworkGraph, settings: () -> EngineSettings = { EngineSettings() }): Media3EngineFactory =
            Media3EngineFactory(caps.capabilities, network::media, settings)
    }
}

/**
 * [PlayerEngine] on ExoPlayer, as pley plays (P3-ENGINE):
 * - decoders by the caps' policy (TVs hardware only), falling back to the
 *   next decoder when one fails, asynchronous queueing on Fire TV;
 * - tracks never beyond what the device plays (the session asks the server
 *   instead), selections redone when the audio output changes, Dolby Vision
 *   preferred on Dolby Vision screens, tunneling only where allowed;
 * - the sound decoded here and tunneling off while the speed or the audio
 *   delay needs it ([setSoundNeeds]);
 * - buffers by memory tier, starting sooner from a home server;
 * - on TVs, the display mode matched to the video's frame rate before it
 *   plays; on phones, Media3's seamless-only rate hints;
 * - audio focus that pauses rather than ducks passthrough, pauses when
 *   headphones or HDMI go, and an audio output started again after it
 *   fails;
 * - loads retried while the server is out of reach (MediaRetries), and
 *   then [EngineState.reconnecting];
 * - stats, a struggling decoder, and tracks the device can't play, in
 *   [state].
 * Music engines have no video renderer, let Media3 handle focus and
 * offload audio with the screen off.
 */
@OptIn(markerClass = [UnstableApi::class])
class Media3Engine(
    context: Context,
    private val profile: DeviceProfile,
    private val music: Boolean,
    private val caps: () -> CapsSnapshot?,
    mediaClient: (origin: String) -> OkHttpClient,
    private val settings: () -> EngineSettings,
    private val frameRateMemory: FrameRateMemory,
    focusPort: FocusPort?,
    private val sdk: Int = Build.VERSION.SDK_INT,
    private val now: () -> Long = SystemClock::elapsedRealtime,
    private val hostFor: (Window) -> DisplayModeHost = ::WindowModeHost,
    /** Media3's clock; tests pass one that moves on its own. */
    playerClock: Clock = Clock.DEFAULT,
) : PlayerEngine {
    private val app = context.applicationContext
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    private val main = Handler(Looper.getMainLooper())
    private val mutableState = MutableStateFlow(EngineState())
    override val state: StateFlow<EngineState> = mutableState.asStateFlow()

    private val details: CapsDetails? get() = caps()?.details

    /** Whether the video being played comes out as Dolby Vision ([ModeChoice.dolbyVisionShown]); read on Media3's thread. */
    @Volatile
    private var dolbyVisionShown: Boolean? = null

    /** Dolby Vision profile 7 plays as its base layer ([EngineSource.profile7BaseLayer]); read on Media3's thread. */
    @Volatile
    private var profile7BaseLayer = false

    /** The video decoder started last, with the MIME type it was started for: a decoder can take more than one. */
    @Volatile
    private var videoCodec: Pair<String, String>? = null

    private val rules = DecoderRules(
        policy = { details?.policy ?: DecoderPolicy(tv = profile.isTv) },
        dolbyVisionShown = { dolbyVisionShown },
        profile7BaseLayer = { profile7BaseLayer },
    )

    private val loadControl = PleyLoadControl(BufferPlan.of(profile))
    private val audioDelay = AudioDelay()
    private val audioDecoding = AudioDecoding()
    private val trackSelector = DefaultTrackSelector(app, sizeSwitching())
    private val loading = MediaLoading(app, mediaClient, now)

    override val player: ExoPlayer = ExoPlayer.Builder(app)
        .setRenderersFactory(
            PleyRenderersFactory(
                app,
                rules,
                asyncQueueing = details?.forceAsyncQueueing ?: profile.fireTv,
                music = music,
                audioDelay = if (music) null else audioDelay,
                audioDecoding = if (music) null else audioDecoding,
                onVideoCodec = { name, mimeType -> videoCodec = name to mimeType },
            ),
        )
        .setTrackSelector(trackSelector)
        .setLoadControl(loadControl)
        .setMediaSourceFactory(loading.sources)
        .setAudioAttributes(audioAttributes(music), music)
        .setHandleAudioBecomingNoisy(true)
        .setVideoChangeFrameRateStrategy(frameRateStrategy())
        .setSeekBackIncrementMs(SEEK_STEP_MS)
        .setSeekForwardIncrementMs(SEEK_STEP_MS)
        .withClock(playerClock)
        .apply { if (music) setWakeMode(C.WAKE_MODE_NETWORK) }
        .build()

    private val focus: AudioFocusRules? = focusPort?.let { port ->
        AudioFocusRules(
            port,
            object : FocusTarget {
                override fun pause() = pauseOurselves()

                override fun play() = player.play()

                override fun setVolume(volume: Float) {
                    player.volume = volume
                }
            },
        )
    }

    private val hdmi: HdmiUnplugWatcher? = if (music) null else HdmiUnplugWatcher(app) {
        if (switchQuiet()) {
            Log.i(TAG, "HDMI output went with the display mode change: plays on")
        } else {
            Log.i(TAG, "HDMI output went: pause")
            player.pause()
        }
    }
    private val restarts = AudioRestarts(now)
    private val struggle = StruggleDetector()
    private val reconnect = ReconnectWatcher()

    /** While reconnecting, every half second: whether media came in. */
    private val reconnectCheck = Runnable {
        reconnect.buffered(player.bufferedPosition)
        publishReconnecting()
    }
    private val analytics = PlaybackAnalytics(now, ::isHardware, ::decoderMime) { stats -> mutableState.update { it.copy(stats = stats) } }

    private var window: Window? = null
    private var frameRate: FrameRateController? = null
    private var loaded: EngineSource? = null
    private var matchJob: Job? = null
    private var matchPending = false
    private var request: MatchRequest? = null
    private var wantPlay = false
    private var ownPauses = 0
    private var released = false

    /** Tunneled playback is held off for the speed or the audio delay ([setSoundNeeds]). */
    private var untunneled = false

    /** The player's playWhenReady as last reported: Media3 also reports a new reason alone, paused already. */
    private var reportedPlayWhenReady = false

    /**
     * Until when an HDMI output that goes, and Media3's "becoming noisy"
     * with it, are the display mode change's doing, not the person's: a
     * Fire TV drops the HDMI audio for about a second as the mode changes,
     * and says so up to a second later. Taken for the person's pause, it
     * kept a resume paused once the mode had settled.
     */
    private var switchQuietUntil = 0L

    private fun switchQuiet(): Boolean = matchPending || now() < switchQuietUntil

    /**
     * A file played as it is that starts part way (a resume) starts at the
     * key frame before its place (Media3's PREVIOUS_SYNC, once the file's
     * index is read), so no frame is decoded only to be thrown away; seeks
     * are exact again once its first frame shows. On the Google TV Streamer
     * a 1080p HEVC resume went from 3.75 s to 2.3 s from the press to the
     * first frame, a 4K one from 4.4 s to 3.6 s. Not for HLS: the server's
     * segments start where the playback does, and moving into the one
     * being fetched made a 4K remux start later.
     */
    private var snapToKeyFrame = false

    private val tick = object : Runnable {
        override fun run() {
            sample()
            main.postDelayed(this, TICK_MS)
        }
    }

    init {
        player.addListener(Listener())
        player.addAnalyticsListener(analytics)
        player.addAnalyticsListener(AudioOutput())
        player.addAnalyticsListener(Loads())
        applyTrackParameters()
        hdmi?.start()
    }

    override fun prepare(source: EngineSource, startMs: Long, playWhenReady: Boolean) {
        check(!released) { "The engine was released" }
        matchJob?.cancel()
        wantPlay = playWhenReady
        loadControl.home = BufferPlan.isHome(source.url)
        loadControl.live = source.live
        applyTrackParameters()
        player.setVideoChangeFrameRateStrategy(frameRateStrategy())
        struggle.reset()
        reconnect.reset()
        main.removeCallbacks(reconnectCheck)
        analytics.onPrepare(frameRate?.requestedModeId?.takeIf { it != 0 })
        mutableState.update { EngineState(stats = it.stats) }
        val asked = matchRequest(source)
        request = asked
        loaded = source
        dolbyVisionShown = source.dolbyVision ?: details?.let { ModeChoice.dolbyVisionShown(it, asked, sdk) }
        profile7BaseLayer = source.profile7BaseLayer
        matchPending = asked != null
        Log.i(TAG, "prepare at $startMs ms: play $playWhenReady, display match ${if (asked != null) "asked" else "none"}, window ${window != null}")
        snapToKeyFrame = startMs > 0 && !music && !source.hls
        player.setSeekParameters(SeekParameters.EXACT)
        // A live channel starts at its default position: the live edge, less the playlist's offset.
        if (source.live) player.setMediaSource(loading.mediaSource(source)) else player.setMediaSource(loading.mediaSource(source), startMs.coerceAtLeast(0))
        setPlayWhenReady(playWhenReady && !matchPending)
        player.prepare()
        if (matchPending && window != null) startMatch()
    }

    /**
     * The mode [video] would play in, as the display match will choose it
     * (the live display, and Auto given up on it), and whether the screen
     * shows Dolby Vision there, with the HDR types seen turned off on it.
     */
    override suspend fun dolbyVisionShown(video: MediaStream?, durationMs: Long?): Boolean? = withContext(Dispatchers.Main.immediate) {
        val caps = details
        if (caps == null || music) return@withContext null
        val controller = frameRate
        val key = controller?.displayKey ?: displayKey(app.getSystemService(DisplayManager::class.java), Display.DEFAULT_DISPLAY)
        val live = controller?.read() ?: AndroidDisplayInfo(app) { Display.DEFAULT_DISPLAY }.read()
        val gaveUp = frameRateMemory.timeouts(key) >= FrameRateController.GIVE_UP_AFTER
        val asked = ModeChoice.matchFor(profile.isTv, video, durationMs, caps, settings().refreshRate, gaveUp)
        ModeChoice.dolbyVisionShown(caps, asked, sdk, live ?: caps.input?.display, turnedOff(key, live))
    }

    /** The HDR types turned off on the display [key] names: what [live] tells now, and what was seen before. */
    private suspend fun turnedOff(key: String, live: DisplayState?): Set<Int> {
        val remembered = frameRateMemory.turnedOffHdrTypes(key)
        val seen = live?.turnedOffHdrTypes(remembered) ?: return remembered
        if (seen != remembered) frameRateMemory.setTurnedOffHdrTypes(key, seen)
        return seen
    }

    override fun setAudioDelay(delayMs: Long) {
        audioDelay.us = delayMs * 1_000
    }

    /**
     * Tunneling is a track selection parameter: Media3 sets the renderers up
     * again when it changes. Passthrough is the audio output's choice when
     * the renderer starts, so the media is loaded again where it is, as for
     * [recheckDolbyVision]: to decode a sound passed through, and back at
     * 1× to pass through again one the output took as PCM only for the
     * speed (whether it was loaded so, or loaded again).
     */
    override fun setSoundNeeds(decoded: Boolean, untunneled: Boolean, reload: Boolean) {
        if (music) return
        val tunnelingChanged = untunneled != this.untunneled
        val decodingChanged = decoded != audioDecoding.on
        this.untunneled = untunneled
        audioDecoding.on = decoded
        val passesAgain = !decoded && player.audioFormat?.let(audioDecoding::turnedDown) == true
        if (!decoded) {
            audioDecoding.forget()
            mutableState.update { it.copy(soundUndecodable = false) }
        }
        if (!reload) return
        if (tunnelingChanged) applyTrackParameters()
        val source = loaded?.takeIf { !it.live }
        if (!decodingChanged || source == null) return
        if (decoded) {
            // No sound yet: the output takes it as asked when it starts (onAudioTrackInitialized).
            val format = player.audioFormat ?: return
            if (!state.value.stats.passthrough) return
            if (!DecodingAudioSink.hasDecoder(app, format)) {
                mutableState.update { it.copy(soundUndecodable = true) }
                return
            }
        } else if (!passesAgain) {
            return
        }
        Log.i(TAG, "loads again where it is to ${if (decoded) "decode the sound" else "pass the sound through"}")
        player.setMediaSource(loading.mediaSource(source), player.currentPosition)
        player.prepare()
    }

    override fun attachWindow(window: Window) {
        if (released || this.window === window) return
        this.window = window
        Log.i(TAG, "window attached, display match pending $matchPending")
        if (profile.isTv && !music) frameRate = FrameRateController(hostFor(window), frameRateMemory, sdk)
        if (matchPending) startMatch()
    }

    override fun detachWindow() {
        if (window != null) Log.i(TAG, "window detached, display match pending $matchPending")
        matchJob?.cancel()
        matchJob = null
        frameRate?.clear()
        frameRate = null
        window = null
        mutableState.update { it.copy(switchingDisplayMode = false, switchingToHz = null) }
    }

    override fun release() {
        if (released) return
        released = true
        detachWindow()
        scope.cancel()
        main.removeCallbacks(tick)
        main.removeCallbacks(reconnectCheck)
        hdmi?.stop()
        focus?.release()
        player.release()
    }

    /**
     * Media3's own refresh-rate switching: TVs match the display mode
     * themselves (FrameRateController); phones let Media3 switch when it
     * is seamless, unless the setting is Off. Read again for each video.
     */
    private fun frameRateStrategy(): Int = when {
        profile.isTv || settings().refreshRate == RefreshRateMatching.Off -> C.VIDEO_CHANGE_FRAME_RATE_STRATEGY_OFF
        else -> C.VIDEO_CHANGE_FRAME_RATE_STRATEGY_ONLY_IF_SEAMLESS
    }

    /** The display match a video on a TV needs; `null` when nothing is to match. */
    private fun matchRequest(source: EngineSource): MatchRequest? =
        if (music) null else ModeChoice.matchFor(profile.isTv, source.video, source.durationMs, details, settings().refreshRate)

    private fun startMatch() {
        val controller = frameRate
        val asked = request
        if (controller == null || asked == null) {
            // Not a TV window: nothing to wait for.
            matchPending = false
            setPlayWhenReady(wantPlay)
            return
        }
        matchJob?.cancel()
        matchJob = scope.launch {
            val outcome = controller.match(asked) { hz -> mutableState.update { it.copy(switchingDisplayMode = true, switchingToHz = hz) } }
            Log.i(TAG, "display match: ${outcome.decision}, switched ${outcome.switched}, timed out ${outcome.timedOut}; play $wantPlay")
            if (outcome.switched || outcome.timedOut) switchQuietUntil = now() + SWITCH_QUIET_MS
            matchPending = false
            mutableState.update {
                it.copy(switchingDisplayMode = false, switchingToHz = null, frameRateHint = it.frameRateHint || outcome.showHint)
            }
            analytics.onDisplayMode(controller.requestedModeId.takeIf { it != 0 })
            // The screen's HDR types and the receiver's formats may have changed with the mode.
            if (outcome.switched) applyTrackParameters()
            recheckDolbyVision(controller)
            setPlayWhenReady(wantPlay)
        }
    }

    /**
     * What the screen shows in the mode the video plays in, now that it is
     * set: Dolby Vision turned off in the system's settings shows only in a
     * mode that lists it. Remembered for the next video; this one, should
     * its Dolby Vision decoder have started, loads again on its base layer's.
     */
    private suspend fun recheckDolbyVision(controller: FrameRateController) {
        val caps = details ?: return
        val live = controller.read() ?: return
        val turnedOff = turnedOff(controller.displayKey, live)
        if (dolbyVisionShown != true || ModeChoice.dolbyVisionShown(caps, null, sdk, live, turnedOff) != false) return
        dolbyVisionShown = false
        val source = loaded ?: return
        if (source.live || player.videoFormat?.sampleMimeType != MimeTypes.VIDEO_DOLBY_VISION) return
        Log.i(TAG, "Dolby Vision isn't shown in this mode: loads again on the base layer's decoder")
        player.setMediaSource(loading.mediaSource(source), player.currentPosition)
        player.prepare()
    }

    private fun applyTrackParameters() {
        val caps = details
        val dolbyVisionScreen = caps?.input != null && DynamicRange.DolbyVision in caps.screenHdrTypes && caps.dolbyVisionProfiles.isNotEmpty()
        val builder = trackSelector.buildUponParameters()
            .setExceedRendererCapabilitiesIfNecessary(false)
            .setAllowInvalidateSelectionsOnRendererCapabilitiesChange(true)
            .setTunnelingEnabled(!music && !untunneled && (caps?.tunnelingAllowed == true || settings().tunneling))
            .setPreferredVideoMimeTypes(*if (dolbyVisionScreen) arrayOf(MimeTypes.VIDEO_DOLBY_VISION) else emptyArray())
        if (music) {
            builder.setTrackTypeDisabled(C.TRACK_TYPE_VIDEO, true)
                .setAudioOffloadPreferences(
                    AudioOffloadPreferences.Builder()
                        .setAudioOffloadMode(AudioOffloadPreferences.AUDIO_OFFLOAD_MODE_ENABLED)
                        .setIsGaplessSupportRequired(true)
                        .build(),
                )
        }
        trackSelector.setParameters(builder)
    }

    private fun setPlayWhenReady(play: Boolean) {
        if (play) {
            player.playWhenReady = true
        } else {
            pauseOurselves()
        }
    }

    /**
     * Pauses for the engine's own reasons (a display mode change, audio
     * focus): counted, so the listener tells them from the person's pauses
     * even though Media3 reports a change made inside a listener after it
     * returns.
     */
    private fun pauseOurselves() {
        if (!player.playWhenReady) return
        ownPauses++
        player.playWhenReady = false
    }

    /** Once a second while playing: the decoder's counts, and whether it struggles. */
    private fun sample() {
        val counters = player.videoDecoderCounters ?: return
        analytics.onCounters(counters)
        val stats = analytics.stats
        val struggling = struggle.add(now(), stats.renderedFrames, stats.droppedFrames, stats.maxConsecutiveDropped)
        if (struggling != state.value.struggling) mutableState.update { it.copy(struggling = struggling) }
    }

    private fun isHardware(name: String): Boolean? = details?.input?.decoders?.firstOrNull { it.name == name }?.hardware

    private fun decoderMime(name: String): String? =
        videoCodec?.takeIf { it.first == name }?.second ?: details?.input?.decoders?.firstOrNull { it.name == name }?.mime

    /** [EngineState.reconnecting] as the watcher has it, looked at again while it lasts. */
    private fun publishReconnecting() {
        val reconnecting = reconnect.reconnecting
        main.removeCallbacks(reconnectCheck)
        if (reconnecting) main.postDelayed(reconnectCheck, RECONNECT_CHECK_MS)
        if (reconnecting != state.value.reconnecting) mutableState.update { it.copy(reconnecting = reconnecting) }
    }

    private inner class Listener : Player.Listener {
        override fun onPlayWhenReadyChanged(playWhenReady: Boolean, reason: Int) {
            Log.i(TAG, "play when ready $playWhenReady (reason $reason, own pauses $ownPauses, display match pending $matchPending)")
            val changed = playWhenReady != reportedPlayWhenReady
            reportedPlayWhenReady = playWhenReady
            // A new reason alone (paused already, now "becoming noisy") is no new pause.
            if (!changed) return
            if (!playWhenReady && ownPauses > 0) {
                ownPauses--
                return
            }
            if (!playWhenReady && reason == Player.PLAY_WHEN_READY_CHANGE_REASON_AUDIO_BECOMING_NOISY && switchQuiet()) {
                Log.i(TAG, "becoming noisy with the display mode change: plays on")
                // While the mode changes the player is held anyway; it plays as the mode settles.
                if (!matchPending) player.play()
                return
            }
            if (playWhenReady) {
                when {
                    // Asked to play while the display changes: it plays once the mode settles.
                    matchPending -> {
                        wantPlay = true
                        pauseOurselves()
                    }
                    focus != null && !focus.beforePlay() -> {
                        Log.i(TAG, "audio focus refused: pause")
                        pauseOurselves()
                    }
                }
            } else {
                wantPlay = false
                focus?.onPausedByPerson()
            }
        }

        override fun onIsPlayingChanged(isPlaying: Boolean) {
            Log.i(TAG, if (isPlaying) "playing" else "not playing")
            main.removeCallbacks(tick)
            if (isPlaying) main.postDelayed(tick, TICK_MS)
        }

        override fun onPositionDiscontinuity(oldPosition: Player.PositionInfo, newPosition: Player.PositionInfo, reason: Int) {
            if (reason == Player.DISCONTINUITY_REASON_SEEK) {
                struggle.reset()
                reconnect.sought()
            }
        }

        override fun onRenderedFirstFrame() {
            player.setSeekParameters(SeekParameters.EXACT)
        }

        override fun onTracksChanged(tracks: Tracks) {
            if (tracks.isEmpty) return
            if (snapToKeyFrame) {
                snapToKeyFrame = false
                player.setSeekParameters(SeekParameters.PREVIOUS_SYNC)
                player.seekTo(player.currentPosition)
            }
            // Music plays no video: an album's cover track isn't missing anything.
            val groups = TrackMapper.groupsOf(tracks).filter { !music || it.type != C.TRACK_TYPE_VIDEO }
            val help = UnsupportedTrackWatcher.check(groups)
            if (help != state.value.needsServerHelp) mutableState.update { it.copy(needsServerHelp = help) }
        }

        override fun onPlayerError(error: PlaybackException) {
            reconnect.reset()
            publishReconnecting()
            if (restarts.shouldRestart(error)) {
                mutableState.update { it.copy(audioRestarts = it.audioRestarts + 1) }
                // After an error the player keeps its position: preparing again resumes there.
                player.prepare()
            }
        }
    }

    /** Tells the focus rules when encoded audio passes through, and the session when it does though it was to be decoded. */
    private inner class AudioOutput : AnalyticsListener {
        override fun onAudioTrackInitialized(eventTime: AnalyticsListener.EventTime, audioTrackConfig: AudioSink.AudioTrackConfig) {
            val passthrough = PlaybackAnalytics.isPassthrough(audioTrackConfig.encoding)
            focus?.passthrough = passthrough
            // DecodingAudioSink lets it through only when no decoder here takes it.
            val undecodable = passthrough && audioDecoding.on && player.audioFormat?.let { !DecodingAudioSink.hasDecoder(app, it) } == true
            if (undecodable) mutableState.update { it.copy(soundUndecodable = true) }
        }
    }

    /** Whether loads reach the server: [EngineState.reconnecting]. */
    private inner class Loads : AnalyticsListener {
        override fun onLoadError(
            eventTime: AnalyticsListener.EventTime,
            loadEventInfo: LoadEventInfo,
            mediaLoadData: MediaLoadData,
            error: IOException,
            wasCanceled: Boolean,
        ) {
            reconnect.failed(error, gaveUp = wasCanceled, bufferedMs = player.bufferedPosition)
            publishReconnecting()
        }

        override fun onLoadCompleted(eventTime: AnalyticsListener.EventTime, loadEventInfo: LoadEventInfo, mediaLoadData: MediaLoadData) {
            if (!reconnect.reconnecting) return
            reconnect.completed()
            publishReconnecting()
        }
    }

    companion object {
        const val SEEK_STEP_MS = 10_000L
        const val TICK_MS = 1_000L
        const val RECONNECT_CHECK_MS = 500L

        /** A few lines a playback (prepare, display match, play and pause with their reasons), for devices' bugs. */
        private const val TAG = "pley.engine"

        /** After a display mode change, how long an HDMI output going is still taken for its doing. */
        const val SWITCH_QUIET_MS = 4_000L

        /**
         * How much is buffered before Media3 moves up a size of a stream
         * in several sizes (its default is 10 s): only with room to spare,
         * so it doesn't swing between two sizes on a network near the line
         * (docs/QUALITY.md).
         */
        const val MIN_BUFFER_FOR_LARGER_SIZE_MS = 20_000

        /** How Media3 moves between a stream's sizes: up only with [MIN_BUFFER_FOR_LARGER_SIZE_MS] buffered, the rest as Media3's defaults. */
        fun sizeSwitching(): ExoTrackSelection.Factory = AdaptiveTrackSelection.Factory(
            MIN_BUFFER_FOR_LARGER_SIZE_MS,
            AdaptiveTrackSelection.DEFAULT_MAX_DURATION_FOR_QUALITY_DECREASE_MS,
            AdaptiveTrackSelection.DEFAULT_MIN_DURATION_TO_RETAIN_AFTER_DISCARD_MS,
            AdaptiveTrackSelection.DEFAULT_BANDWIDTH_FRACTION,
        )

        /** Media3 marks setClock for tests; the app always passes Clock.DEFAULT, tests a clock that moves by itself. */
        @SuppressLint("VisibleForTests")
        private fun ExoPlayer.Builder.withClock(clock: Clock): ExoPlayer.Builder =
            if (clock === Clock.DEFAULT) this else setClock(clock)

        fun audioAttributes(music: Boolean): AudioAttributes = AudioAttributes.Builder()
            .setUsage(C.USAGE_MEDIA)
            .setContentType(if (music) C.AUDIO_CONTENT_TYPE_MUSIC else C.AUDIO_CONTENT_TYPE_MOVIE)
            .build()
    }
}
