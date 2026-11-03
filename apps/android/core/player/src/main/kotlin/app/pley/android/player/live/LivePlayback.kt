package app.pley.android.player.live

import android.content.Context
import androidx.annotation.OptIn
import androidx.media3.common.C
import androidx.media3.common.MimeTypes
import androidx.media3.common.PlaybackException
import androidx.media3.common.Player
import androidx.media3.common.Tracks
import androidx.media3.common.util.UnstableApi
import androidx.media3.datasource.HttpDataSource
import app.pley.android.caps.CapsSnapshot
import app.pley.android.common.DeviceProfile
import app.pley.android.common.asSentence
import app.pley.android.common.sentenceOf
import app.pley.android.data.LiveTvRepository
import app.pley.android.model.ApiErrorBody
import app.pley.android.model.BusyConnection
import app.pley.android.model.LivePlan
import app.pley.android.model.PleyJson
import app.pley.android.model.ServerEvent
import app.pley.android.network.ApiException
import app.pley.android.network.OfflineException
import app.pley.android.player.EngineSource
import app.pley.android.player.PlayerEngine
import app.pley.android.player.session.clientIdOf
import kotlin.coroutines.cancellation.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.async
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.emptyFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

/**
 * Why a channel doesn't play, by what the server answered (docs/API.md,
 * "Watching"), with its own sentence: a `409` names the channels holding
 * every connection of the source, and lists them ([connections]: who
 * watches each, and which may be taken over). A connection taken over
 * from this device is told by the server's `live_taken_over` event, and
 * by a `410` after it, saying why (who took it, for what, only within the
 * home).
 */
data class LiveTrouble(val kind: Kind, val message: String, val connections: List<BusyConnection> = emptyList()) {
    enum class Kind {
        /** `409`: every connection of the channel's source is in use. */
        Busy,

        /** `410` or `live_taken_over`: someone took the channel's connection, or the server stopped it. */
        Stopped,

        /** `404`: the channel was hidden or its source removed. */
        Missing,

        /** `403`: Live TV isn't open to this profile. */
        NotAllowed,

        /** `503` or `504`: the provider sends nothing, or not in time. */
        Unavailable,

        /** The server is out of reach. */
        Offline,
        Other,
    }

    /**
     * Whether trying again may help: not for a channel that is gone. A
     * stopped one (an administrator, a server restarting) tunes again.
     */
    val canRetry: Boolean get() = kind != Kind.Missing

    companion object {
        /** The trouble [error] is: a refusal of the server's, a network failure, or else. */
        fun of(error: Throwable): LiveTrouble = when (error) {
            is ApiException -> LiveTrouble(kindOf(error.status), sentenceOf(error), error.connections)
            is OfflineException -> LiveTrouble(Kind.Offline, sentenceOf(error))
            else -> LiveTrouble(Kind.Other, sentenceOf(error))
        }

        /** The trouble of a stream that stopped: the server's answer to a playlist or segment, when it gave one. */
        @OptIn(markerClass = [UnstableApi::class])
        fun of(error: PlaybackException): LiveTrouble {
            var cause: Throwable? = error
            while (cause != null) {
                if (cause is HttpDataSource.InvalidResponseCodeException) {
                    val body = runCatching { PleyJson.decodeFromString(ApiErrorBody.serializer(), cause.responseBody.decodeToString()) }.getOrNull()
                    val said = body?.message?.takeIf { it.isNotBlank() }
                    return LiveTrouble(
                        kindOf(cause.responseCode),
                        said?.let(::sentence) ?: "The server answered ${cause.responseCode}.",
                        body?.connections.orEmpty(),
                    )
                }
                if (cause is HttpDataSource.HttpDataSourceException) return LiveTrouble(Kind.Offline, sentenceOf(OfflineException(cause)))
                cause = cause.cause
            }
            return LiveTrouble(Kind.Other, sentence(error.errorCodeName))
        }

        private fun kindOf(status: Int): Kind = when (status) {
            409 -> Kind.Busy
            410 -> Kind.Stopped
            404 -> Kind.Missing
            403 -> Kind.NotAllowed
            502, 503, 504 -> Kind.Unavailable
            else -> Kind.Other
        }

        private fun sentence(text: String): String = asSentence(text)
    }
}

/** Where watching a channel is. */
sealed interface LivePhase {
    /** Nothing is tuned: before the first channel, or let go in the background. */
    data object Idle : LivePhase

    /** Asking the server for the channel, then for its first segments. */
    data object Tuning : LivePhase

    /** The channel's picture (or sound) is out. */
    data object Playing : LivePhase

    data class Failed(val trouble: LiveTrouble) : LivePhase
}

/** What a live player shows. Live TV is watched as it airs: no pausing, no going back. */
data class LiveState(
    val channelId: Long? = null,
    val phase: LivePhase = LivePhase.Idle,
    /** Waiting for data while playing. */
    val buffering: Boolean = false,
    /** The channel has sound only (radio): no picture comes. */
    val audioOnly: Boolean = false,
)

/**
 * Watching Live TV on this device (docs/API.md, "Live TV"), shared by the
 * phone's and the TV's live players:
 *
 * - [tune] asks the server for the channel with this device's
 *   [clientId], which ends what it watched before (zapping never holds
 *   two of the provider's connections), lets the previous session go, and
 *   reads the channel's `stream.ts` (MPEG-TS as the provider sends it) as
 *   it asks, the server tuning for whichever comes first (after it, when
 *   a connection is taken over, which only the watch does), then asks what
 *   the channel is made into: sent as it is, or with sound the server
 *   converts that this device plays as it comes ([playsAsSent]), that
 *   plays from its first keyframe rather than after the first HLS segments
 *   (as the web's); otherwise, or when it fails, the answer's HLS from the
 *   live edge, on the app's Media3 engine;
 * - it is watched as it airs, as the web's: no pausing, no going back;
 * - a player failure tunes the channel again twice, after 1 s and 3 s
 *   (Samsung's), before it says why, unless the server said no ([RETRIES]);
 * - someone taking the channel's connection over (the server's
 *   `live_taken_over`, for the channel rather than the session:
 *   `stream.ts` has none of its own) stops it at once and says why, the
 *   same as the `410` that follows; nothing tunes it again by itself;
 * - [stop] lets the session and the engine go (the screen went to the
 *   background), [resume] tunes the channel again, [close] ends it all.
 *
 * Made and used on the main thread: [scope] is the screen's.
 */
@OptIn(markerClass = [UnstableApi::class])
class LivePlayback(
    private val live: LiveTvRepository,
    private val engines: () -> PlayerEngine,
    private val clientId: () -> String,
    private val scope: CoroutineScope,
    /** The server's events, to hear of the channel's connection being taken over. */
    events: Flow<ServerEvent> = emptyFlow(),
    /** What this device plays, for the sound the server converts ([playsAsSent]); `null` until probed. */
    private val caps: () -> CapsSnapshot? = { null },
) {
    private val mutableState = MutableStateFlow(LiveState())
    val state: StateFlow<LiveState> = mutableState.asStateFlow()

    private val mutableEngine = MutableStateFlow<PlayerEngine?>(null)

    /** The engine playing now, for the screen's video surface and media session; `null` before the first tune. */
    val engine: StateFlow<PlayerEngine?> = mutableEngine.asStateFlow()

    private var session: String? = null
    private var tuning: Job? = null
    private var closed = false
    private val listener = Listener()

    /** What plays is the channel's `stream.ts`: failing, it is watched again as HLS. */
    private var direct = false

    /** `stream.ts` is read, not yet played: the plan hasn't said the channel is sent as it is. */
    private var holding = false

    /** `stream.ts` failed while the tune still waited for the watch or its plan: the tune goes on to the HLS. */
    private var streamFailed = false

    /** A stall under way, said once it has lasted [STALL_SHOWN_MS]. */
    private var stall: Job? = null

    /** The times the channel was tuned again after a player failure, since it was tuned or retried. */
    private var retries = 0

    init {
        scope.launch {
            events.collect { event ->
                if (event is ServerEvent.LiveTakenOver) takenOver(event.channelId, event.message)
            }
        }
    }

    /**
     * Someone took [channelId]'s connection over: if it is what this device
     * watches (or tunes), it stops now and says [message]. Watching ended
     * at the server, so there is no session to let go, and the stream's end
     * that follows is no break to tune again after.
     */
    private fun takenOver(channelId: Long, message: String) {
        val now = state.value
        if (closed || now.channelId != channelId || (now.phase != LivePhase.Playing && now.phase != LivePhase.Tuning)) return
        tuning?.cancel()
        session = null
        direct = false
        holding = false
        fail(LiveTrouble(LiveTrouble.Kind.Stopped, asSentence(message)))
    }

    /** Starts watching [channelId] in place of what played. */
    fun tune(channelId: Long) {
        retries = 0
        tune(channelId, allowDirect = true)
    }

    private fun tune(channelId: Long, allowDirect: Boolean, takeOver: Long? = null) {
        if (closed) return
        tuning?.cancel()
        stall?.cancel()
        direct = false
        holding = false
        streamFailed = false
        letSessionGo()
        mutableEngine.value?.player?.let {
            it.stop()
            it.clearMediaItems()
        }
        mutableState.value = LiveState(channelId = channelId, phase = LivePhase.Tuning)
        tuning = scope.launch {
            val asked = async {
                try {
                    Result.success(live.watch(channelId, clientId(), takeOver))
                } catch (e: CancellationException) {
                    throw e
                } catch (e: Exception) {
                    Result.failure(e)
                }
            }
            // As the web's: stream.ts is read at once, under an id of its own (so it doesn't end the HLS
            // session the plan is asked of), and played once the plan says the channel is sent as it is.
            // As Samsung's, it is asked for with the channel rather than once the server answered: a
            // round trip sooner. After the answer when a connection is taken over: only the watch takes it.
            var stream = if (allowDirect && takeOver == null) readStream(channelId) else null
            val watching = asked.await().getOrElse { e ->
                fail(LiveTrouble.of(e))
                return@launch
            }
            session = watching.sessionId
            val hls = EngineSource(url = watching.url, hls = true, live = true)
            if (allowDirect && takeOver != null) stream = readStream(channelId)
            if (stream == null || streamFailed) {
                direct = false
                engine().prepare(hls, startMs = 0, playWhenReady = true)
                return@launch
            }
            val engine = engine()
            val plan = quietly { live.plan(watching.sessionId) }
            holding = false
            if (!streamFailed && plan != null && playsAsSent(plan, caps())) {
                engine.player.play()
                if (engine.player.playbackState == Player.STATE_READY) playing()
            } else {
                // Made into something else, or not known in time: its HLS.
                direct = false
                engine.prepare(hls, startMs = 0, playWhenReady = true)
            }
        }
    }

    /** The engine, made (and listened to) on the first tune. */
    private fun engine(): PlayerEngine = mutableEngine.value ?: engines().also {
        it.player.addListener(listener)
        mutableEngine.value = it
    }

    /** [channelId]'s `stream.ts`, read and held until the plan says it plays; `null` when it has no address. */
    private suspend fun readStream(channelId: Long): String? {
        val stream = quietly { live.streamUrl(channelId, clientId() + STREAM_CLIENT) } ?: return null
        direct = true
        holding = true
        engine().prepare(EngineSource(url = stream, hls = false, mimeType = MimeTypes.VIDEO_MP2T, live = true), startMs = 0, playWhenReady = false)
        return stream
    }

    /**
     * stream.ts stopped while it played. The server ended watching (its
     * connection taken over, the channel stopped): its `410` says why, as
     * the HLS would. Otherwise it broke off: watched again, as HLS (the
     * web's).
     */
    private fun brokeOff(channelId: Long) {
        direct = false
        holding = false
        stall?.cancel()
        val asked = session
        tuning?.cancel()
        tuning = scope.launch {
            val ended = asked?.let { s ->
                try {
                    live.plan(s)
                    null
                } catch (e: CancellationException) {
                    throw e
                } catch (e: ApiException) {
                    e.takeIf { it.status == 410 }
                } catch (_: Exception) {
                    null
                }
            }
            if (ended != null) {
                // Already gone at the server: nothing to let go.
                if (session == asked) session = null
                fail(LiveTrouble.of(ended))
            } else {
                tune(channelId, allowDirect = false)
            }
        }
    }

    private fun playing() {
        stall?.cancel()
        mutableState.update { if (it.phase is LivePhase.Tuning) it.copy(phase = LivePhase.Playing, buffering = false) else it.copy(buffering = false) }
    }

    /** [block]'s answer, or `null` when it fails: a plan not had is HLS played, as before there were plans. */
    private suspend fun <T> quietly(block: suspend () -> T): T? = try {
        block()
    } catch (e: CancellationException) {
        throw e
    } catch (_: Exception) {
        null
    }

    /**
     * Tunes the channel again: after a failure, or back from the background.
     * [takeOver]: every connection being in use, the channel whose
     * connection to take (one [LiveTrouble.connections] said may be).
     */
    fun retry(takeOver: Long? = null) {
        retries = 0
        state.value.channelId?.let { tune(it, allowDirect = true, takeOver = takeOver) }
    }

    /**
     * The player failed with [trouble]: tuned again after a while, the
     * server having let the channel go or the network blinked, as long as
     * [RETRIES] allow; then it says why.
     */
    private fun failedPlaying(channelId: Long, trouble: LiveTrouble) {
        if (trouble.kind !in RETRIED || retries >= RETRIES) {
            fail(trouble)
            return
        }
        retries++
        stall?.cancel()
        tuning?.cancel()
        mutableState.update { it.copy(phase = LivePhase.Tuning, buffering = false) }
        tuning = scope.launch {
            delay(if (retries == 1) FIRST_RETRY_MS else NEXT_RETRY_MS)
            tune(channelId, allowDirect = true)
        }
    }

    /** Back in front: the channel left in [stop] plays again, from the live edge. */
    fun resume() {
        if (state.value.phase == LivePhase.Idle) retry()
    }

    /**
     * Lets the session and the engine go, keeping which channel it was:
     * the screen is in the background. Nothing plays and no connection is
     * held until [resume].
     */
    fun stop() {
        tuning?.cancel()
        stall?.cancel()
        direct = false
        holding = false
        letSessionGo()
        mutableEngine.value?.let {
            it.player.removeListener(listener)
            it.release()
        }
        mutableEngine.value = null
        mutableState.update { LiveState(channelId = it.channelId) }
    }

    /** Ends watching for good. */
    fun close() {
        stop()
        closed = true
    }

    private fun letSessionGo() {
        session?.let(live::leave)
        session = null
    }

    private fun fail(trouble: LiveTrouble) {
        stall?.cancel()
        letSessionGo()
        mutableEngine.value?.player?.stop()
        mutableState.update { it.copy(phase = LivePhase.Failed(trouble), buffering = false) }
    }

    private inner class Listener : Player.Listener {
        override fun onPlaybackStateChanged(playbackState: Int) {
            when (playbackState) {
                // Held for the plan, it isn't playing yet.
                Player.STATE_READY -> if (!holding) playing()
                // Said once it lasts: a channel that stutters for a moment doesn't flicker the spinner (the web's).
                Player.STATE_BUFFERING -> if (state.value.phase is LivePhase.Playing && stall?.isActive != true) {
                    stall = scope.launch {
                        delay(STALL_SHOWN_MS)
                        mutableState.update { if (it.phase is LivePhase.Playing) it.copy(buffering = true) else it }
                    }
                }
                // stream.ts ended: the server let it go (taken over, stopped), or it broke off.
                Player.STATE_ENDED -> state.value.channelId?.let { if (direct && !holding) brokeOff(it) }
                else -> Unit
            }
        }

        override fun onPlayWhenReadyChanged(playWhenReady: Boolean, reason: Int) {
            // Live has no pause (headphones pulled out, a stray media key): it plays on, as a browser's does.
            if (!playWhenReady && !holding) mutableEngine.value?.player?.play()
        }

        override fun onTracksChanged(tracks: Tracks) {
            if (tracks.isEmpty) return
            mutableState.update { it.copy(audioOnly = !tracks.containsType(C.TRACK_TYPE_VIDEO)) }
        }

        override fun onPlayerError(error: PlaybackException) {
            val player = mutableEngine.value?.player ?: return
            val channelId = state.value.channelId
            if (direct && channelId != null) {
                if (state.value.phase is LivePhase.Playing) {
                    brokeOff(channelId)
                } else if (tuning?.isActive == true) {
                    // Failed before the watch or its plan answered (a busy channel's 409, a 404): the tune
                    // goes on, to the HLS with the session it gets or to why watching failed, rather than
                    // asking for the channel a second time.
                    streamFailed = true
                    direct = false
                    holding = false
                } else {
                    // stream.ts didn't play (its codecs, the connection): watched again, as HLS.
                    direct = false
                    holding = false
                    tune(channelId, allowDirect = false)
                }
                return
            }
            if (error.errorCode == PlaybackException.ERROR_CODE_BEHIND_LIVE_WINDOW) {
                // Fell behind what the server keeps (a long stall): on from the live edge.
                player.seekToDefaultPosition()
                player.prepare()
                return
            }
            val trouble = LiveTrouble.of(error)
            if (channelId != null) failedPlaying(channelId, trouble) else fail(trouble)
        }
    }

    companion object {
        /** A stall shows the spinner once it has lasted this long (the web's 0.7 s). */
        const val STALL_SHOWN_MS = 700L

        /** `stream.ts` is read under the device's id with this after it (the web's): the HLS session stays. */
        const val STREAM_CLIENT = "-stream"

        /** A player failure tunes the channel again this many times before it says why (Samsung's)... */
        const val RETRIES = 2

        /** …the first after this long… */
        const val FIRST_RETRY_MS = 1_000L

        /** …the next after this. */
        const val NEXT_RETRY_MS = 3_000L

        /**
         * Whether this device plays [plan]'s channel from `stream.ts` as the
         * provider sends it: its picture sent as it is (or none), and its
         * sound too ([LivePlan.direct]), or AC3, E-AC3 or MP2, which the
         * plan converts for players that can't, where this device decodes it
         * or sends it to its receiver as it is ([caps], as Samsung's
         * PlaysAsSent, whose TVs all decode them; the probe lists no MP2
         * decoders, so MP2 stays with the HLS). Before the device is probed,
         * only what is sent as it is: such a channel's HLS, made for every
         * player, starts later, but plays.
         */
        fun playsAsSent(plan: LivePlan, caps: CapsSnapshot?): Boolean {
            if (plan.direct) return true
            if (plan.video !in AS_SENT) return false
            val codec = plan.audioCodec?.lowercase()?.takeIf { it in CONVERTED_SOUND } ?: return false
            val report = caps?.report ?: return false
            return codec in report.audioCodecs + report.audioDecode.orEmpty() + report.audioPassthrough.orEmpty()
        }

        /** A plan's picture or sound as the provider sends it, or none of it. */
        private val AS_SENT = setOf("copy", "none")

        /** Sound the server converts for HLS that a device may play as it comes, in the server's names. */
        private val CONVERTED_SOUND = setOf("ac3", "eac3", "mp2")

        /** Failures tuning again may help: not the server's refusals (busy, stopped, gone, not allowed). */
        private val RETRIED = setOf(LiveTrouble.Kind.Other, LiveTrouble.Kind.Offline, LiveTrouble.Kind.Unavailable)

        /**
         * Watching on this device: [live]'s server, engines of [engines],
         * the device's own client id (the one its playbacks send), what it
         * plays as [caps] last probed it.
         */
        fun create(
            context: Context,
            live: LiveTvRepository,
            engines: PlayerEngine.Factory,
            profile: DeviceProfile,
            scope: CoroutineScope,
            events: Flow<ServerEvent> = emptyFlow(),
            caps: () -> CapsSnapshot? = { null },
        ): LivePlayback {
            val app = context.applicationContext
            return LivePlayback(live, { engines.create(app, profile, music = false) }, clientIdOf(app), scope, events, caps)
        }
    }
}
