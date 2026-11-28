package app.pley.android.player.engine

import androidx.annotation.OptIn
import androidx.media3.common.C
import androidx.media3.common.Timeline
import androidx.media3.common.util.UnstableApi
import androidx.media3.common.util.Util
import androidx.media3.exoplayer.DefaultLoadControl
import androidx.media3.exoplayer.LoadControl
import androidx.media3.exoplayer.analytics.PlayerId
import androidx.media3.exoplayer.source.MediaSource
import androidx.media3.exoplayer.source.TrackGroupArray
import androidx.media3.exoplayer.trackselection.ExoTrackSelection
import androidx.media3.exoplayer.upstream.Allocator
import app.pley.android.common.DeviceProfile
import app.pley.android.common.Servers
import app.pley.android.common.Tier
import java.net.URI
import java.net.URISyntaxException

/**
 * How much the player buffers on this device. TVs hold no back buffer and
 * stop at a size their memory allows (48 MiB on 1 GB sticks, 64 MiB on
 * 1.5 GB ones, 96 MiB on T1, 120 MiB on T2), 15 to 30 s ahead on T0 and
 * 50 s elsewhere; phones buffer 15 to 50 s ahead and keep 10 s behind for
 * quick jumps back. Music, on any device, buffers up to 3 minutes ahead,
 * at least 50 s, within 16 MiB (8 MiB on a low-memory device or T0), and
 * keeps 10 s behind: a car's tunnel doesn't stop it, and the radio sleeps
 * between reads. Playback starts after 500 ms from a server on the home
 * network and 1 s from elsewhere; after a stall, 1.5 s and 2 s.
 */
data class BufferPlan(
    /** Bytes to stop at; [C.LENGTH_UNSET] lets Media3 size it from the tracks. */
    val targetBytes: Int,
    val minMs: Int,
    val maxMs: Int,
    val backBufferMs: Int,
    val startHomeMs: Int = START_HOME_MS,
    val startRemoteMs: Int = START_REMOTE_MS,
    val restartHomeMs: Int = RESTART_HOME_MS,
    val restartRemoteMs: Int = RESTART_REMOTE_MS,
) {
    /** What must be buffered before playing, or playing again after a stall. */
    fun startMs(home: Boolean, rebuffering: Boolean): Int = when {
        rebuffering && home -> restartHomeMs
        rebuffering -> restartRemoteMs
        home -> startHomeMs
        else -> startRemoteMs
    }

    companion object {
        const val MIB = 1024 * 1024
        const val START_HOME_MS = 500
        const val START_REMOTE_MS = 1_000
        const val RESTART_HOME_MS = 1_500
        const val RESTART_REMOTE_MS = 2_000

        /** 1 GB sticks report about 0.9 GiB; 1.5 GB devices about 1.4. */
        private const val ONE_GB_BELOW = 5L * (1L shl 30) / 4

        /** The plan of [profile]'s video, or with [music] of the music's player. */
        fun of(profile: DeviceProfile, music: Boolean = false): BufferPlan = when {
            music -> music(profile)
            profile.isTv -> tv(profile)
            else -> PHONE
        }

        /** Phones: Media3's sizes, 15 to 50 s ahead, 10 s behind. */
        val PHONE = BufferPlan(targetBytes = C.LENGTH_UNSET, minMs = 15_000, maxMs = 50_000, backBufferMs = 10_000)

        /**
         * Music: a few MiB a track, so more time than video in fewer bytes:
         * three minutes, enough to play on through a tunnel or a gap in the
         * signal, and no more, as music skipped on mobile data would have
         * been fetched for nothing. The bytes stop it first on a lossless
         * file.
         */
        private fun music(profile: DeviceProfile): BufferPlan = BufferPlan(
            targetBytes = if (profile.lowRam || profile.tier == Tier.T0) 8 * MIB else 16 * MIB,
            minMs = 50_000,
            maxMs = 180_000,
            backBufferMs = 10_000,
        )

        private fun tv(profile: DeviceProfile): BufferPlan = when (profile.tier) {
            Tier.T0 -> BufferPlan(
                targetBytes = if (profile.totalRamBytes in 1 until ONE_GB_BELOW) 48 * MIB else 64 * MIB,
                minMs = 15_000,
                maxMs = 30_000,
                backBufferMs = 0,
            )
            Tier.T1 -> BufferPlan(targetBytes = 96 * MIB, minMs = 50_000, maxMs = 50_000, backBufferMs = 0)
            Tier.T2 -> BufferPlan(targetBytes = 120 * MIB, minMs = 50_000, maxMs = 50_000, backBufferMs = 0)
        }

        /**
         * Whether [url] is a server on the home network: a private address
         * or a local name. Tailscale counts as away, as it often crosses the
         * internet.
         */
        fun isHome(url: String): Boolean {
            val host = try {
                URI(url).host
            } catch (_: URISyntaxException) {
                null
            } ?: return false
            val h = host.lowercase()
            if (h.endsWith(".ts.net")) return false
            val v4 = Regex("""^100\.(\d+)\.\d+\.\d+$""").find(h)
            if (v4 != null && v4.groupValues[1].toInt() in 64..127) return false
            return Servers.isLocalHost(h)
        }
    }
}

/**
 * Media3's DefaultLoadControl sized by [plan], deciding when to start from
 * where the server is ([home], set at each prepare): the home network's
 * thresholds are the delegate's, and away from home playback waits for a
 * little more. A live channel ([live]) starts on the small thresholds
 * wherever the server is (0.5 s, 1.5 s after a stall): it comes as it
 * airs, and a bigger start buffer is only a later picture.
 */
@OptIn(markerClass = [UnstableApi::class])
class PleyLoadControl(private val plan: BufferPlan) : LoadControl {
    @Volatile
    var home: Boolean = true

    @Volatile
    var live: Boolean = false

    private val delegate: DefaultLoadControl = DefaultLoadControl.Builder()
        .setBufferDurationsMs(plan.minMs, plan.maxMs, plan.startHomeMs, plan.restartHomeMs)
        .setTargetBufferBytes(plan.targetBytes)
        .setPrioritizeTimeOverSizeThresholds(false)
        .setBackBuffer(plan.backBufferMs, false)
        .build()

    override fun onPrepared(playerId: PlayerId) = delegate.onPrepared(playerId)

    override fun onTracksSelected(
        parameters: LoadControl.Parameters,
        trackGroups: TrackGroupArray,
        trackSelections: Array<out ExoTrackSelection?>,
    ) = delegate.onTracksSelected(parameters, trackGroups, trackSelections)

    override fun onStopped(playerId: PlayerId) = delegate.onStopped(playerId)

    override fun onReleased(playerId: PlayerId) = delegate.onReleased(playerId)

    override fun getAllocator(playerId: PlayerId): Allocator = delegate.getAllocator(playerId)

    override fun getBackBufferDurationUs(playerId: PlayerId): Long = delegate.getBackBufferDurationUs(playerId)

    override fun retainBackBufferFromKeyframe(playerId: PlayerId): Boolean = delegate.retainBackBufferFromKeyframe(playerId)

    override fun shouldContinueLoading(parameters: LoadControl.Parameters): Boolean = delegate.shouldContinueLoading(parameters)

    override fun shouldContinuePreloading(
        playerId: PlayerId,
        timeline: Timeline,
        mediaPeriodId: MediaSource.MediaPeriodId,
        bufferedDurationUs: Long,
    ): Boolean = delegate.shouldContinuePreloading(playerId, timeline, mediaPeriodId, bufferedDurationUs)

    override fun shouldStartPlayback(parameters: LoadControl.Parameters): Boolean {
        if (!delegate.shouldStartPlayback(parameters)) return false
        if (home || live) return true
        val needUs = plan.startMs(home = false, rebuffering = parameters.rebuffering) * 1_000L
        val bufferedUs = Util.getPlayoutDurationForMediaDuration(parameters.bufferedDurationUs, parameters.playbackSpeed)
        return bufferedUs >= needUs
    }
}
