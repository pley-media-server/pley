package app.pley.android.testing.player.session

import app.pley.android.model.MediaStream
import app.pley.android.player.EngineSource
import app.pley.android.player.PlayerEngine
import app.pley.android.player.session.AudioOut
import app.pley.android.player.session.EngineError
import app.pley.android.player.session.EngineLink
import app.pley.android.player.session.EngineStatus
import app.pley.android.player.session.EngineTracks
import app.pley.android.player.session.FrameCounts

/**
 * An engine without Media3, for the session's tests: it plays along
 * [clock] (a test scheduler's time) once [ready], and records what it was
 * told. Tests make things happen to it: [ready], [stall], [starve],
 * [outOfReach], [end], [fail], [tracks].
 */
class FakeEngineLink(private val clock: () -> Long = { 0L }) : EngineLink {
    /** One prepare: what was loaded, from where. */
    data class Prepared(val source: EngineSource, val startMs: Long, val playWhenReady: Boolean)

    override val engine: PlayerEngine? = null

    val prepared = mutableListOf<Prepared>()
    val seeks = mutableListOf<Long>()

    /** Audio choices: the ordinal in the file and the language. */
    val audioChoices = mutableListOf<Pair<Int, String?>>()

    /** Subtitle choices: the ordinal in the file (`null`: off) and the language. */
    val textChoices = mutableListOf<Pair<Int?, String?>>()
    var released = false
        private set

    /** What [frameCounts] answers. */
    var frames: FrameCounts? = null

    /** What [bandwidthKbps] answers: the network's speed as measured, `null` while unknown. */
    override var bandwidthKbps: Long? = null

    /** What [stats] answers. */
    var lines: List<String> = listOf("Video decoder: fake")

    /** What [dolbyVisionShown] answers: `null`, the engine can't tell. */
    var dolbyVision: Boolean? = null

    /** How the sound goes out: tests say when it passes through or is tunneled. */
    override var audioOut: AudioOut = AudioOut.Decoded

    /** The speed and the audio delay asked for last. */
    var speed = 1f
        private set
    var audioDelayMs = 0L
        private set

    private var listener: EngineLink.Listener? = null
    private var basePosition = 0L
    private var since = 0L
    private var frozen = false

    override var durationMs: Long = 0
    override var status: EngineStatus = EngineStatus.Idle
        private set
    override var playWhenReady: Boolean = false
        private set
    override var reconnecting: Boolean = false
        private set

    /** Moving: ready, meant to play, not stalled. */
    private val moving: Boolean get() = status == EngineStatus.Ready && playWhenReady && !frozen

    override val positionMs: Long
        get() {
            val pos = if (moving) basePosition + (clock() - since) else basePosition
            return if (durationMs > 0) pos.coerceAtMost(durationMs) else pos
        }

    private var starving = false

    /** As far as it has loaded: 30 s ahead, nothing while [starve]d. */
    override val bufferedMs: Long get() = if (starving) basePosition else positionMs + AHEAD_MS

    /** The source loaded last. */
    val source: EngineSource? get() = prepared.lastOrNull()?.source

    override fun setListener(listener: EngineLink.Listener?) {
        this.listener = listener
    }

    override suspend fun dolbyVisionShown(video: MediaStream?, durationMs: Long?): Boolean? = dolbyVision

    override fun prepare(source: EngineSource, startMs: Long, playWhenReady: Boolean) {
        prepared += Prepared(source, startMs, playWhenReady)
        settle(startMs)
        frozen = false
        starving = false
        reconnecting = false
        this.playWhenReady = playWhenReady
        status = EngineStatus.Buffering
        listener?.onStatus(EngineStatus.Buffering)
    }

    /** The media is loaded: it plays from here when meant to. */
    fun ready() {
        settle(positionMs)
        starving = false
        reconnecting = false
        status = EngineStatus.Ready
        listener?.onStatus(EngineStatus.Ready)
        if (playWhenReady) listener?.onPlaying(true)
    }

    /** It stands still while meant to play, with data: a decoder that doesn't keep up. */
    fun stall() {
        settle(positionMs)
        frozen = true
    }

    /** It waits for data that doesn't come. */
    fun starve() {
        settle(positionMs)
        starving = true
        status = EngineStatus.Buffering
        listener?.onStatus(EngineStatus.Buffering)
    }

    /** The server goes out of reach: it waits for data, trying again, until [ready] or [fail]. */
    fun outOfReach() {
        starve()
        reconnecting = true
    }

    /** The end of the media. */
    fun end() {
        settle(if (durationMs > 0) durationMs else positionMs)
        status = EngineStatus.Ended
        listener?.onStatus(EngineStatus.Ended)
    }

    /** It stops with [error]. */
    fun fail(error: EngineError) {
        settle(positionMs)
        reconnecting = false
        status = EngineStatus.Idle
        listener?.onError(error)
    }

    /** The media's tracks are known. */
    fun tracks(tracks: EngineTracks) {
        listener?.onTracks(tracks)
    }

    override fun play() {
        if (playWhenReady) return
        settle(positionMs)
        playWhenReady = true
        listener?.onPlayWhenReady(true)
        if (status == EngineStatus.Ready) listener?.onPlaying(true)
    }

    override fun pause() {
        if (!playWhenReady) return
        settle(positionMs)
        playWhenReady = false
        if (status == EngineStatus.Ready) listener?.onPlaying(false)
        listener?.onPlayWhenReady(false)
    }

    override fun seekTo(positionMs: Long) {
        seeks += positionMs
        settle(positionMs)
        frozen = false
    }

    override fun selectAudio(ordinal: Int, language: String?) {
        audioChoices += ordinal to language
    }

    override fun selectText(ordinal: Int?, language: String?) {
        textChoices += ordinal to language
    }

    override fun frameCounts(): FrameCounts? = frames

    override fun setSpeed(speed: Float) {
        this.speed = speed
    }

    override fun setAudioDelay(delayMs: Long) {
        audioDelayMs = delayMs
    }

    override fun stats(): List<String> = lines

    override fun release() {
        released = true
        listener = null
    }

    private fun settle(position: Long) {
        basePosition = position
        since = clock()
    }

    private companion object {
        const val AHEAD_MS = 30_000L
    }
}
