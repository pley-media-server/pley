package app.pley.android.player.live

import android.content.Context
import android.os.SystemClock
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
import app.pley.android.common.UiText
import app.pley.android.common.errorSentence
import app.pley.android.data.LiveTvRepository
import app.pley.android.model.ApiErrorBody
import app.pley.android.model.BusyConnection
import app.pley.android.model.Control
import app.pley.android.model.LivePlan
import app.pley.android.model.PleyJson
import app.pley.android.model.ServerEvent
import app.pley.android.model.caps.ClientCaps
import app.pley.android.model.caps.VideoDecoderCaps
import app.pley.android.network.ApiException
import app.pley.android.network.OfflineException
import app.pley.android.player.EngineSource
import app.pley.android.player.PlayerEngine
import app.pley.android.player.R
import app.pley.android.player.session.clientIdOf
import kotlin.coroutines.cancellation.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
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
 * home). [text] is that sentence: the server's as it wrote it, or the
 * app's own, put into words where it shows.
 */
data class LiveTrouble(val kind: Kind, val text: UiText, val connections: List<BusyConnection> = emptyList()) {
    enum class Kind {
        /** `409`: every connection of the channel's source is in use. */
        Busy,

        /** `410`, `live_taken_over` or `session_stopped`: someone took the channel's connection, or the server stopped it. */
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
            is ApiException -> LiveTrouble(kindOf(error.status), errorSentence(error), error.connections)
            is OfflineException -> LiveTrouble(Kind.Offline, errorSentence(error))
            else -> LiveTrouble(Kind.Other, errorSentence(error))
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
                        said?.let(::sentence) ?: UiText.res(R.string.live_server_answered, cause.responseCode),
                        body?.connections.orEmpty(),
                    )
                }
                if (cause is HttpDataSource.HttpDataSourceException) return LiveTrouble(Kind.Offline, errorSentence(OfflineException(cause)))
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

        /** Words that aren't the app's (the server's, a code's name), as a sentence. */
        private fun sentence(text: String): UiText = UiText.Sentence(UiText.raw(text))
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
 *   the channel is made into: sent as it is, or made of video and sound
 *   this device plays as the provider sends them ([playsAsSent]), that
 *   plays from its first keyframe rather than after the first HLS segments
 *   (as the web's), and the server makes no transcode of it; otherwise, or
 *   when it fails, the answer's HLS from the live edge, on the app's
 *   Media3 engine;
 * - [details] says which it is and why, for the player's Playback details;
 * - it is watched as it airs, as the web's: no pausing, no going back;
 * - a player failure tunes the channel again twice, after 1 s and 3 s
 *   (Samsung's), before it says why, unless the server said no ([RETRIES]);
 * - a `stream.ts` that stands still for good while it comes (a provider
 *   whose stream starts its clock over) is read again, or watched as HLS
 *   if that didn't help (the web's);
 * - someone taking the channel's connection over (the server's
 *   `live_taken_over`, for the channel rather than the session:
 *   `stream.ts` has none of its own) stops it at once and says why, the
 *   same as the `410` that follows; nothing tunes it again by itself;
 * - the session stopped from elsewhere (the server's `session_stopped`:
 *   an administrator, or the profile on another device) stops it the
 *   same way, saying who and why;
 * - the profile's remote, on another device, changes channel: its next
 *   and previous are channel up and down, in the order the player [zap]s
 *   through, which a player declares only when it has one;
 * - while it has a session, the event stream stays open in the background
 *   too ([playing]);
 * - [stop] lets the session and the engine go (the screen went to the
 *   background), [resume] tunes the channel again unless the server
 *   stopped it, [close] ends it all.
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
    /** Told whether it has a session on the server now (LiveUpdates.playing): the event stream stays open meanwhile. */
    private val playing: (player: Any, playing: Boolean) -> Unit = { _, _ -> },
    /** Told which session it watches now, `null` once none (RemoteRepository.here): no remote shows it here. */
    private val here: (player: Any, sessionId: String?) -> Unit = { _, _ -> },
    /** What this device plays, for the video and sound it takes as the provider sends them ([playsAsSent]); `null` until probed. */
    private val caps: () -> CapsSnapshot? = { null },
    /**
     * A pause lets the channel go, as [stop], where a player on screen plays
     * on: the car's radio, whose Pause, another app's sound taking over, or
     * headphones pulled out stop it until Play ([resume]).
     */
    private val pausable: Boolean = false,
    /** A clock that only goes forward, in ms: how long ago `stream.ts` was read again. */
    private val now: () -> Long = SystemClock::elapsedRealtime,
) {
    private val mutableState = MutableStateFlow(LiveState())
    val state: StateFlow<LiveState> = mutableState.asStateFlow()

    private val mutableEngine = MutableStateFlow<PlayerEngine?>(null)

    /** The engine playing now, for the screen's video surface and media session; `null` before the first tune. */
    val engine: StateFlow<PlayerEngine?> = mutableEngine.asStateFlow()

    private var session: String? = null
        set(value) {
            if (value != field) plan = null
            field = value
            playing(this, value != null)
            here(this, value)
        }

    /** What the session's channel is made into, as the server last said; `null` until it did. */
    private var plan: LivePlan? = null

    /**
     * Channel up ([by] 1) or down (-1), in the order the player goes
     * through them: the remote's next and previous do the same. `null`, as
     * long as the player has no order, declares neither.
     */
    var zap: ((by: Int) -> Unit)? = null

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

    /** The watch for a `stream.ts` that stands still while it comes ([watchForStandstill]). */
    private var standstill: Job? = null

    /** When `stream.ts` was last read again ([now]'s time); `null`: not since the channel was tuned. */
    private var readAgainAt: Long? = null

    init {
        scope.launch {
            events.collect { event ->
                when (event) {
                    is ServerEvent.LiveTakenOver -> takenOver(event.channelId, event.message)
                    is ServerEvent.SessionStopped -> if (event.sessionId == session) stoppedElsewhere(event.message, event.byAdmin)
                    is ServerEvent.PlayerCommand -> if (event.sessionId == session && !closed) obey(event.command)
                    else -> Unit
                }
            }
        }
    }

    /**
     * The server stopped this device's session (an administrator, or the
     * profile on another device): it stops now and says so, as for a
     * connection taken over. Watching ended at the server: nothing to let go.
     */
    private fun stoppedElsewhere(message: String?, byAdmin: Boolean) {
        if (closed) return
        tuning?.cancel()
        session = null
        direct = false
        holding = false
        fail(LiveTrouble(LiveTrouble.Kind.Stopped, stoppedText(message, byAdmin)))
    }

    /** The remote's next and previous: channel up and down. The new channel's watch tells the server, and so the remote. */
    private fun obey(command: Control) {
        val step = zap ?: return
        when (command) {
            Control.Next -> step(1)
            Control.Previous -> step(-1)
            else -> Unit
        }
    }

    /** What the remote may ask of this player: channel up and down when it has an order to go through ([zap]). */
    private fun controls(): List<Control> = if (zap != null) ZAP_CONTROLS else emptyList()

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
        fail(LiveTrouble(LiveTrouble.Kind.Stopped, UiText.Sentence(UiText.raw(message))))
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
        standstill?.cancel()
        direct = false
        holding = false
        streamFailed = false
        readAgainAt = null
        letSessionGo()
        mutableEngine.value?.player?.let {
            it.stop()
            it.clearMediaItems()
        }
        mutableState.value = LiveState(channelId = channelId, phase = LivePhase.Tuning)
        tuning = scope.launch {
            val asked = async {
                try {
                    Result.success(live.watch(channelId, clientId(), takeOver, controls()))
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
            this@LivePlayback.plan = plan
            holding = false
            if (!streamFailed && plan != null && playsAsSent(plan, caps())) {
                engine.player.play()
                if (engine.player.playbackState == Player.STATE_READY) playing()
                watchForStandstill(channelId)
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
        engine().prepare(streamSource(stream), startMs = 0, playWhenReady = false)
        return stream
    }

    private fun streamSource(url: String) = EngineSource(url = url, hls = false, mimeType = MimeTypes.VIDEO_MP2T, live = true)

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
        standstill?.cancel()
        val asked = session
        tuning?.cancel()
        tuning = scope.launch {
            if (!goesOn(asked)) return@launch
            tune(channelId, allowDirect = false)
        }
    }

    /**
     * Whether the server still has [asked], the session watched: asked for
     * its plan, which also counts it as seen. Ended there (its `410`), the
     * channel stops and says why; any other answer, or none, and it goes on.
     */
    private suspend fun goesOn(asked: String?): Boolean {
        val ended = try {
            asked?.let { live.plan(it) }
            null
        } catch (e: CancellationException) {
            throw e
        } catch (e: ApiException) {
            e.takeIf { it.status == 410 }
        } catch (_: Exception) {
            null
        } ?: return true
        // Already gone at the server: nothing to let go.
        if (session == asked) session = null
        fail(LiveTrouble.of(ended))
        return false
    }

    /**
     * Watches the `stream.ts` that plays for standing still for good while
     * it comes, as the web's `whenStuck` does. A provider whose stream
     * starts its clock over (the server connected to it again, after it
     * ended or dropped the stream) sends sound and pictures timed before
     * what played: Media3 neither plays them nor fails, and would wait there
     * for its own ten minutes.
     *
     * Looked at every [STUCK_LOOK_MS]: its position no further and nothing
     * more buffered on [STUCK_AFTER] looks in a row, the stream having come
     * on [STUCK_COMING] of them and coming still, it is [stoodStill]. A
     * provider gone quiet or a stream that ended sends nothing, and is left
     * to the stall and to [brokeOff].
     *
     * Whether it comes is the bytes the engine [received][PlayerEngine.received]
     * since the last look, the web's own sign. Media3 reads on at the
     * stream's full rate while it stands still so (nothing it reads counts
     * as buffered ahead, so its buffer never fills), and says `isLoading`
     * for a provider gone quiet too: only the bytes tell the two apart.
     */
    private fun watchForStandstill(channelId: Long) {
        standstill?.cancel()
        val engine = engine()
        val player = engine.player
        standstill = scope.launch {
            var position = player.currentPosition
            var buffered = player.bufferedPosition
            var received = engine.received
            var movedBefore = false
            var played = false
            var still = 0
            var came = 0
            while (true) {
                delay(STUCK_LOOK_MS)
                val positionNow = player.currentPosition
                val bufferedNow = player.bufferedPosition
                val receivedNow = engine.received
                // Forward only: a stream read again starts at 0.
                val moved = positionNow > position
                val more = bufferedNow > buffered
                val coming = receivedNow > received
                position = positionNow
                buffered = bufferedNow
                received = receivedNow
                // Played: on from one look to the next, not the jump to where a stream starts.
                played = played || (moved && movedBefore)
                movedBefore = moved
                if (moved || more) {
                    still = 0
                    came = 0
                    continue
                }
                still++
                if (coming) came++
                if (still >= STUCK_AFTER && came >= STUCK_COMING && coming) break
            }
            stoodStill(channelId, played)
        }
    }

    /**
     * `stream.ts` stands still for good while it comes. One that [played]
     * is read again, on the same session: the server starts the new reader
     * at its last keyframe, and its clock starts with it. One that never
     * played wouldn't then either, and one that stands still again within
     * [READ_AGAIN_ONCE_IN_MS] (a provider whose clock starts over at every
     * break) keeps doing so: those are watched as HLS, as one that
     * [brokeOff], which the transcoder plays through the jump.
     */
    private fun stoodStill(channelId: Long, played: Boolean) {
        val last = readAgainAt
        if (!played || (last != null && now() - last < READ_AGAIN_ONCE_IN_MS)) {
            brokeOff(channelId)
            return
        }
        stall?.cancel()
        // The spinner, until it plays again.
        mutableState.update { it.copy(buffering = true) }
        val asked = session
        tuning?.cancel()
        tuning = scope.launch {
            // The session counts as watched while its stream is read: seen now, it stays while the stream is read again.
            if (!goesOn(asked)) return@launch
            val stream = quietly { live.streamUrl(channelId, clientId() + STREAM_CLIENT) }
            if (stream == null) {
                tune(channelId, allowDirect = false)
                return@launch
            }
            readAgainAt = now()
            engine().prepare(streamSource(stream), startMs = 0, playWhenReady = true)
            watchForStandstill(channelId)
        }
    }

    private fun playing() {
        stall?.cancel()
        mutableState.update { if (it.phase is LivePhase.Tuning) it.copy(phase = LivePhase.Playing, buffering = false) else it.copy(buffering = false) }
    }

    /**
     * How the channel plays now, for its Playback details: from `stream.ts`
     * or the server's HLS, with the session's plan asked again (it answers
     * at once and starts nothing; its bitrate is the provider's as it is
     * now) and, for a channel on HLS, why it isn't played as sent. A plan
     * that doesn't come leaves the last one known. `null` while no channel
     * plays.
     */
    suspend fun details(): LivePlaying? {
        val asked = session ?: return null
        if (state.value.phase != LivePhase.Playing) return null
        val now = quietly { live.plan(asked) }
        if (session != asked || state.value.phase != LivePhase.Playing) return null
        if (now != null) plan = now
        val known = plan
        val asSent = direct && !holding
        return LivePlaying(known, asSent, if (asSent || known == null) null else notAsSent(known, caps()))
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

    /** Back in front: the channel left in [stop] plays again, from the live edge; one the server stopped stays stopped. */
    fun resume() {
        if (state.value.phase == LivePhase.Idle) retry()
    }

    /**
     * Lets the session and the engine go, keeping which channel it was:
     * the screen is in the background. Nothing plays and no connection is
     * held until [resume]. A channel the server stopped keeps saying so:
     * it doesn't tune again by itself, only by Watch again ([retry]).
     */
    fun stop() {
        tuning?.cancel()
        stall?.cancel()
        standstill?.cancel()
        direct = false
        holding = false
        letSessionGo()
        mutableEngine.value?.let {
            it.player.removeListener(listener)
            it.release()
        }
        mutableEngine.value = null
        mutableState.update {
            val phase = it.phase
            val stopped = phase is LivePhase.Failed && phase.trouble.kind == LiveTrouble.Kind.Stopped
            LiveState(channelId = it.channelId, phase = if (stopped) phase else LivePhase.Idle)
        }
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
        standstill?.cancel()
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
            if (playWhenReady || holding) return
            // The radio lets the channel go: playing on would take the sound back from whatever took it. Not
            // inside the engine's own callback, which stop() releases: posted, as the scope's own dispatcher (the main
            // thread's, immediate) would run it here.
            if (pausable) {
                scope.launch(Dispatchers.Main) { if (!closed) stop() }
                return
            }
            // Live has no pause (headphones pulled out, a stray media key): it plays on, as a browser's does.
            mutableEngine.value?.player?.play()
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
        /** The remote's commands a player that goes through channels obeys. */
        private val ZAP_CONTROLS = listOf(Control.Next, Control.Previous)

        /** Who stopped a session, in the app's words, and their [message] (as they wrote it) when given. */
        fun stoppedText(message: String?, byAdmin: Boolean): UiText {
            val who = UiText.res(if (byAdmin) R.string.live_stopped_by_admin else R.string.live_stopped_elsewhere)
            return if (message.isNullOrBlank()) who else UiText.res(R.string.session_stopped_message, who, message.trim())
        }

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

        /** A `stream.ts` that plays is looked at this often (the web's)… */
        const val STUCK_LOOK_MS = 1_000L

        /** …and stands still for good on this many looks in a row… */
        const val STUCK_AFTER = 8

        /** …the stream having come on this many of them: not a provider gone quiet. */
        const val STUCK_COMING = 5

        /** `stream.ts` is read again once in this long at most (the web's 2 minutes): standing still again sooner, it is watched as HLS. */
        const val READ_AGAIN_ONCE_IN_MS = 120_000L

        /** Whether this device plays [plan]'s channel from `stream.ts` as the provider sends it ([notAsSent] says why not). */
        fun playsAsSent(plan: LivePlan, caps: CapsSnapshot?): Boolean = notAsSent(plan, caps) == null

        /**
         * Why this device doesn't play [plan]'s channel from `stream.ts` as
         * the provider sends it; `null` when it does:
         *
         * - its picture sent as it is (or none), and its sound too
         *   ([LivePlan.direct]), or AC3, E-AC3 or MP2, which the plan
         *   converts for players that can't, where this device decodes it
         *   or sends it to its receiver as it is ([caps], as Samsung's
         *   PlaysAsSent, whose TVs all decode them);
         * - while the server allows it ([LivePlan.directPlay]), whatever
         *   else it plays of the provider's: progressive video a decoder
         *   here takes at its bit depth, size and frame rate, and sound it
         *   decodes or sends on, among what Media3 reads from MPEG-TS
         *   ([STREAM_VIDEO], [STREAM_SOUND]). Interlaced video the server
         *   converts stays the server's: its deinterlacing setting decides.
         *
         * The probe lists no MP2 decoders, so MP2 stays with the HLS. Before
         * the device is probed, only what is sent as it is plays so: such a
         * channel's HLS, made for every player, starts later, but plays.
         */
        fun notAsSent(plan: LivePlan, caps: CapsSnapshot?): NotAsSent? {
            if (plan.direct) return null
            val report = caps?.report
            val sound = plan.audioCodec?.lowercase()
            val pictureAsSent = plan.video in AS_SENT
            if (pictureAsSent && sound in CONVERTED_SOUND && report != null && sound in sounds(report)) return null
            when (plan.directPlay) {
                true -> {}
                false -> return NotAsSent.Off
                // An older server: nothing to say of a setting it doesn't have.
                null -> return NotAsSent.Unknown
            }
            report ?: return NotAsSent.Unknown
            if (!pictureAsSent) {
                // Interlaced video is the server's while it makes it progressive.
                if (plan.interlaced && plan.deinterlace != false) return NotAsSent.Interlaced
                val video = plan.videoCodec?.lowercase() ?: return NotAsSent.Unknown
                if (!decodes(report, plan, video)) return NotAsSent.Video
                if (video !in STREAM_VIDEO) return NotAsSent.Unknown
            }
            if (plan.audio !in AS_SENT) {
                sound ?: return NotAsSent.Unknown
                if (sound !in sounds(report)) return NotAsSent.Audio
                if (sound !in STREAM_SOUND) return NotAsSent.Unknown
            }
            return null
        }

        /** The sound [report]'s device decodes or sends to its receiver as it is, in the server's names. */
        private fun sounds(report: ClientCaps): List<String> = report.audioCodecs + report.audioDecode.orEmpty() + report.audioPassthrough.orEmpty()

        /**
         * Whether a decoder of [report]'s takes [plan]'s video, of [codec]:
         * its bit depth (8 when the plan doesn't say), size and frame rate
         * within the limits the decoder gives, as the server checks a file's
         * (apps/server/src/playback.rs, `takes`). A report without decoders
         * goes by its list of codecs.
         */
        private fun decodes(report: ClientCaps, plan: LivePlan, codec: String): Boolean {
            // A report that only names codecs says nothing of bit depth: taken at 8 bits, as the LG app does.
            val decoders = report.video ?: return (plan.bitDepth ?: 8) <= 8 && report.videoCodecs.any { it.equals(codec, ignoreCase = true) }
            return decoders.any { it.codec.equals(codec, ignoreCase = true) && takes(it, plan) }
        }

        private fun takes(decoder: VideoDecoderCaps, plan: LivePlan): Boolean {
            val w = plan.width
            val h = plan.height
            val maxW = decoder.maxWidth
            val maxH = decoder.maxHeight
            // A decoder that takes 1920×1080 takes 1080×1920 too.
            if (w != null && h != null && maxW != null && maxH != null && !(w <= maxW && h <= maxH) && !(h <= maxW && w <= maxH)) return false
            val fps = plan.frameRate
            val maxFps = decoder.maxFps
            if (fps != null && maxFps != null && fps > maxFps + RATE_SLACK) return false
            val maxDepth = decoder.maxBitDepth
            return maxDepth == null || (plan.bitDepth ?: 8) <= maxDepth
        }

        /** A frame rate this far over a decoder's limit is still within it: 59.94 written as 59.940059. */
        private const val RATE_SLACK = 0.01

        /** A plan's picture or sound as the provider sends it, or none of it. */
        private val AS_SENT = setOf("copy", "none")

        /** Sound the server converts for HLS that a device may play as it comes, in the server's names. */
        private val CONVERTED_SOUND = setOf("ac3", "eac3", "mp2")

        /** The video Media3 reads from MPEG-TS (DefaultTsPayloadReaderFactory) that the probe lists decoders for, in the server's names. */
        private val STREAM_VIDEO = setOf("h264", "hevc", "mpeg2video")

        /** The sound it reads from MPEG-TS, in the server's names: MPEG audio, AAC in ADTS, AC3 and E-AC3. */
        private val STREAM_SOUND = setOf("aac", "ac3", "eac3", "mp2", "mp3")

        /** Failures tuning again may help: not the server's refusals (busy, stopped, gone, not allowed). */
        private val RETRIED = setOf(LiveTrouble.Kind.Other, LiveTrouble.Kind.Offline, LiveTrouble.Kind.Unavailable)

        /**
         * Watching on this device: [live]'s server, engines of [engines],
         * the device's own client id (the one its playbacks send), what it
         * plays as [caps] last probed it; [playing] keeps the event stream
         * open while it has a session, [here] tells the remote which.
         */
        fun create(
            context: Context,
            live: LiveTvRepository,
            engines: PlayerEngine.Factory,
            profile: DeviceProfile,
            scope: CoroutineScope,
            events: Flow<ServerEvent> = emptyFlow(),
            playing: (player: Any, playing: Boolean) -> Unit = { _, _ -> },
            here: (player: Any, sessionId: String?) -> Unit = { _, _ -> },
            /** The car's radio: the sound alone, on the music's engine (no video decoders, the audio focus its own), and [pausable]. */
            radio: Boolean = false,
            caps: () -> CapsSnapshot? = { null },
        ): LivePlayback {
            val app = context.applicationContext
            return LivePlayback(live, { engines.create(app, profile, music = radio) }, clientIdOf(app), scope, events, playing, here, caps, pausable = radio)
        }
    }
}
