package app.pley.android.player.music

import android.os.SystemClock
import app.pley.android.auth.AuthException
import app.pley.android.model.Chapter
import app.pley.android.model.Control
import app.pley.android.model.ItemKind
import app.pley.android.model.ItemSummary
import app.pley.android.model.PlaybackMethod
import app.pley.android.model.PlaybackMode
import app.pley.android.model.PlaybackRequest
import app.pley.android.model.playableChapters
import app.pley.android.model.ProgressState
import app.pley.android.model.ServerEvent
import app.pley.android.model.caps.ClientCaps
import app.pley.android.network.ApiException
import app.pley.android.network.OfflineException
import app.pley.android.network.PleyApi
import app.pley.android.player.EngineSource
import app.pley.android.player.MusicError
import app.pley.android.player.MusicPosition
import app.pley.android.player.MusicQueueState
import app.pley.android.player.MusicRepeat
import app.pley.android.player.NowPlaying
import app.pley.android.player.chapterIndexAt
import app.pley.android.player.session.ServerPlaybackSession
import app.pley.android.player.session.SessionReporter
import app.pley.android.player.sleep.SleepChoice
import app.pley.android.player.sleep.SleepState
import app.pley.android.player.sleep.SleepTimer
import kotlin.random.Random
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.emptyFlow
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.withTimeoutOrNull

/** What music plays with: the server, what the device plays, the output. */
class MusicEnvironment(
    /** The API of the server and profile signed in to now; throws when there is none. */
    val api: () -> PleyApi,
    /** What this device plays, for the server's choice of direct play or a conversion. */
    val caps: suspend () -> ClientCaps,
    /** Music's own player id: a video starting doesn't end the music's session, nor the other way round. */
    val clientId: () -> String,
    val reporter: SessionReporter,
    /** Makes the output, on the main thread, when music first plays. */
    val output: () -> MusicOutput,
    /** A monotonic clock in ms, for the sleep timer. */
    val now: () -> Long = SystemClock::elapsedRealtime,
    /** The server's events, to hear of the track's session being stopped, and the remote's commands. */
    val events: Flow<ServerEvent> = emptyFlow(),
    /**
     * Told whether music is under way (LiveUpdates.playing): the event
     * stream stays open in the background meanwhile, for the remote.
     */
    val playing: (player: Any, playing: Boolean) -> Unit = { _, _ -> },
    /** Told which track's session plays now, `null` once none (RemoteRepository.here): no remote shows it here. */
    val here: (player: Any, sessionId: String?) -> Unit = { _, _ -> },
)

/**
 * Music's queue and the server's side of it (apps/web/src/audio/AudioPlayer.tsx):
 *
 * - one server session per track (POST /api/playback), the one before
 *   ended first when its reports go out quickly ([FLUSH_WAIT_MS]); the
 *   server ends it anyway as the next starts, so a transcode never waits
 *   on the last one;
 * - while a track plays, the one that plays by itself after it is asked
 *   for [AHEAD_MS] before its end, beside it (`keep_previous`, `ahead`:
 *   not listed as playing until it reports), and waits
 *   after it in the output, which goes on to it without a gap or a pause
 *   to load ([lookAhead]); whatever changes what comes next takes it back,
 *   and the track to come is then asked for as it comes;
 * - progress every 10 s while playing, on pause, and every 30 s paused (the
 *   server lets a session it hasn't heard from go, its transcode with it);
 *   at the end of a track `stopped` at its length and DELETE, then the next
 *   track by itself;
 * - a track the device can't decode is asked for again as a transcode, and
 *   a session that expired is started again where it was, up to three
 *   times in a row; a failure the player can't name asks the server
 *   whether the session goes on first;
 * - Previous restarts a track that has played more than 3 s;
 * - Repeat plays the queue again from its start once it ends, or the
 *   track again once it does, each time with a session of its own;
 * - a sleep timer pauses after the minutes chosen, or at the end of the
 *   track (the next one waits, paused);
 * - a video or a channel ends the music's session ([pauseForVideo]): the
 *   queue and the place stay, and Play starts a new session there;
 * - a queue can wait without playing ([cue]): what was playing when the app
 *   was last in memory, or what a car asked to have ready; Play starts it;
 * - the remote, on another of the profile's devices, plays and pauses,
 *   seeks, skips, and goes to the next or previous track ([obey]); the
 *   event stream stays open while music is under way, the screen off too,
 *   and is let go once it has been paused for 90 s.
 *
 * It lives as long as the app's process, so it plays on with no screen
 * open; the [MusicService] shows it in the notification and on the lock
 * screen. Main thread only.
 */
class MusicPlayback(
    private val env: MusicEnvironment,
    private val scope: CoroutineScope,
    random: Random = Random.Default,
) {
    private val queue = MusicQueue(random)
    private val now = MutableStateFlow<NowPlaying?>(null)
    private val order = MutableStateFlow(MusicQueueState())

    val nowPlaying: StateFlow<NowPlaying?> = now.asStateFlow()
    val queueState: StateFlow<MusicQueueState> = order.asStateFlow()

    private var out: MusicOutput? = null
    private var session: ServerPlaybackSession? = null
        set(value) {
            field = value
            env.here(this, value?.id)
        }
    private var api: PleyApi? = null
    private var loadJob: Job? = null
    private var ticker: Job? = null

    /** The mode to ask for after direct play failed to decode the current track. */
    private var fallback: PlaybackMode? = null

    /** New sessions asked for the current track since it last played. */
    private var recreates = 0
    private var playing = false
    private var loading = false

    /** Whether the track being asked for plays once it loads: Play and Pause meanwhile change it. */
    private var startPlays = true

    /**
     * Whether the output holds the queue's track. Not from a change of
     * track until the new one is loaded: the output still has the last
     * one, its position and its length, which aren't this track's. That
     * is so for a track asked for again too (it repeats, or is picked once
     * more), where the output's track has the same id.
     */
    internal var held = false
        private set

    /** Seeks made so far ([NowPlaying.seeks]). */
    private var seeks = 0
    private var ended = false

    /** A video or a channel took over ([pauseForVideo]): the session went, the place stays. */
    private var yielded = false

    /** Where a queue that waits for Play starts ([cue]); `null` once its track has been asked for. */
    private var waitingAt: Long? = null
    private var error: MusicError? = null

    /** Counting down to the next track after one that can't be played ([NowPlaying.nextIn]). */
    private var skipJob: Job? = null
    private var nextIn: Int? = null

    /** Tracks that couldn't be played one after another, skipped by themselves: a queue none of plays stops. */
    private var failedInARow = 0

    /** Paused as the person left the TV app ([pauseForLeave]): plays on when they're back, if nothing changed it since. */
    private var playOnReturn = false

    /** Where the track [startTrack] asks the server for starts, until the output has it (its place is the track before's). */
    private var askedAtMs: Long? = null

    /**
     * The track that plays by itself once this one ends, at [index] of the
     * play order: asked for ahead ([lookAhead]), its [session] once the
     * server answers, which then waits after this track in the output.
     */
    private class Ahead(val index: Int, val trackId: Long) {
        var job: Job? = null
        var session: ServerPlaybackSession? = null
    }

    private var ahead: Ahead? = null

    /** The server refused what was asked ahead for this track (no transcode free beside it, say): the next is asked for as it comes. */
    private var aheadFailed = false

    /**
     * The queue waiting was suggested by the library, not played on this
     * device ([cue]): not kept as what played last until a track of it starts.
     */
    var suggested = false
        private set

    /** Paused for [PAUSED_HOLD_MS]: the event stream is let go in the background until it plays again. */
    private var rested = false
    private var resting: Job? = null

    private val timer = SleepTimer(scope, env.now, ::trackLeftMs, { queue.current?.id }) { pause() }

    /** The sleep timer. */
    val sleep: StateFlow<SleepState> get() = timer.state

    init {
        scope.launch {
            env.events.collect { event ->
                when {
                    event is ServerEvent.SessionStopped && event.sessionId == session?.id -> stoppedByServer()
                    event is ServerEvent.PlayerCommand && event.sessionId == session?.id -> obey(event)
                }
            }
        }
    }

    /**
     * A command of the profile's remote for the track playing, as the
     * web's music obeys it: a seek stays within the track, a skip goes from
     * where it is now, next and previous as the buttons here. What changed
     * is reported at once, so the remote sees it (a pause reports by
     * itself, and another track with its new session).
     */
    private fun obey(command: ServerEvent.PlayerCommand) {
        val s = session ?: return
        val o = out ?: return
        when (command.command) {
            Control.Play -> {
                // Playing already: said again, so a remote a little behind settles.
                if (!o.playWhenReady) play()
                s.progress(lastPosition(), ProgressState.Playing)
            }
            // A pause reports by itself; paused already, it is said again.
            Control.Pause -> if (o.playWhenReady) pause() else s.progress(lastPosition(), ProgressState.Paused)
            Control.Seek -> command.positionMs?.let { seekNow(s, o, it) }
            Control.Skip -> command.offsetMs?.let { seekNow(s, o, lastPosition() + it) }
            Control.Next -> {
                next()
                // To the next chapter of the same part: the same session, moved.
                if (session === s) s.progress(lastPosition(), if (o.playWhenReady) ProgressState.Playing else ProgressState.Paused)
            }
            Control.Previous -> {
                previous()
                // Back to the start of the same track, or of a chapter: the same session, moved.
                if (session === s) s.progress(lastPosition(), if (o.playWhenReady) ProgressState.Playing else ProgressState.Paused)
            }
            // Music has no tracks to pick, and doesn't say it takes these.
            Control.Audio, Control.Subtitles, Control.Quality, Control.Volume, Control.Unknown -> Unit
        }
    }

    /** Seeks to [positionMs], within the track, and reports where it went. */
    private fun seekNow(s: ServerPlaybackSession, o: MusicOutput, positionMs: Long) {
        val duration = o.durationMs.takeIf { it > 0 } ?: s.info.durationMs.takeIf { it > 0 } ?: queue.current?.durationMs
        val target = (if (duration != null) positionMs.coerceAtMost(duration) else positionMs).coerceAtLeast(0)
        seekTo(target)
        s.progress(target, if (o.playWhenReady) ProgressState.Playing else ProgressState.Paused)
    }

    /**
     * The server ended the track's session (an administrator, or the profile
     * elsewhere): the music stops and says so, asking for no new session.
     * Play starts the track again.
     */
    private fun stoppedByServer() {
        val s = session
        session = null
        // Where it was is kept (best effort); nothing is deleted.
        s?.stoppedByServer(lastPosition())
        dropAhead()
        loadJob?.cancel()
        timer.cancel()
        out?.stop()
        fail(MusicError.Stopped)
    }

    fun setSleep(choice: SleepChoice) {
        if (queue.isEmpty && choice != SleepChoice.Off) return
        timer.set(choice)
        // At the end of this track, the next one waits: it isn't to play on without a gap.
        lookAhead()
    }

    /** "Keep listening": the same time again. */
    fun keepListening() {
        timer.extend()
        lookAhead()
    }

    /** Told when the queue is forgotten ([stop]): the service lets go of its session. */
    internal var onStopped: (() -> Unit)? = null

    /** The output, made the first time music plays (the media session plays through it). */
    fun output(): MusicOutput = out ?: env.output().also {
        it.setListener(listener)
        out = it
    }

    /** Whether there is anything to play: the service runs while there is. */
    val active: Boolean get() = !queue.isEmpty

    fun position(): MusicPosition {
        // A queue that waits, where it starts: no output is made for it until it plays.
        waitingAt?.let { return MusicPosition(it, queue.current?.durationMs ?: 0, 0) }
        val o = out ?: return MusicPosition()
        // Between tracks: where the one coming was asked for (its start, or the place a waiting one is played from),
        // not where the last one was, and never 0 for a track asked for further in: that would be kept as its place.
        if (!held) return MusicPosition(askedAtMs ?: 0, queue.current?.durationMs ?: 0, 0)
        val duration = o.durationMs.takeIf { it > 0 } ?: queue.current?.durationMs ?: 0
        val position = if (ended) duration else o.positionMs
        return MusicPosition(position.coerceAtLeast(0), duration, o.bufferedMs.coerceAtLeast(0))
    }

    /** Plays [tracks] from the one at [startIndex], [startMs] into it (what was playing, brought back where it was). */
    fun play(tracks: List<ItemSummary>, startIndex: Int = 0, shuffle: Boolean = false, startMs: Long = 0) {
        if (tracks.isEmpty()) return
        failedInARow = 0
        queue.start(tracks, startIndex, shuffle)
        startTrack(startMs.coerceAtLeast(0))
    }

    /**
     * Makes [tracks] the queue without playing it: the track at
     * [startIndex] waits, [startMs] into it, and Play starts it there. It
     * is what was playing when the app was last in memory, shown again for
     * Play to bring back, or what a car or the Assistant asked to have
     * ready before it asks to play. Nothing is asked of the server
     * meanwhile. The same queue cued again, still waiting, stays as it is.
     * What was playing shuffled comes with [given]: where each of [tracks],
     * as they played, was in the order the queue was given, so it waits
     * shuffled as it was ([MusicQueue.restore]). A [suggested] queue (the
     * library's, for a car with nothing kept) is [suggested] until a track
     * of it starts: nothing played it on this device.
     */
    fun cue(
        tracks: List<ItemSummary>,
        startIndex: Int = 0,
        shuffle: Boolean = false,
        startMs: Long = 0,
        given: List<Int>? = null,
        suggested: Boolean = false,
    ) {
        if (tracks.isEmpty()) return
        val at = startIndex.coerceIn(tracks.indices)
        val waiting = queue.state
        if (waitingAt != null && !shuffle && waiting.index == at && waiting.given == given.orEmpty() && waiting.tracks.map { it.id } == tracks.map { it.id }) {
            // The same queue made ready again (the Assistant's prepare, then its play) stays as it is; asked for at
            // another place (a book picked in the car, where the profile is in it now), it waits there.
            val place = startMs.coerceAtLeast(0)
            if (waitingAt != place) {
                waitingAt = place
                sought()
            }
            return
        }
        this.suggested = suggested
        failedInARow = 0
        // A new queue: the server's refusal of what was asked ahead was for the track before.
        aheadFailed = false
        cancelSkip()
        dropAhead()
        loadJob?.cancel()
        stopTicker()
        endSession()
        out?.stop()
        askedAtMs = null
        if (given == null || !queue.restore(tracks, given, at)) queue.start(tracks, at, shuffle)
        held = false
        fallback = null
        recreates = 0
        playing = false
        loading = false
        ended = false
        error = null
        // As after a video: no session, the place kept, and Play starts the track there.
        yielded = true
        waitingAt = startMs.coerceAtLeast(0)
        publish()
    }

    /** Plays, or after the queue's end, a failure or a video, starts the current track again. */
    fun play() {
        playOnReturn = false
        if (queue.isEmpty) return
        when {
            ended -> startTrack(0)
            // Where it waits (sought to, or cued), else where it was asked for, else where the output was.
            error != null || yielded -> startTrack(waitingAt ?: askedAtMs ?: lastPosition(), sameTrack = true, play = true)
            starting -> {
                startPlays = true
                publish()
            }
            else -> output().play()
        }
    }

    fun pause() {
        playOnReturn = false
        // Paused on a track that can't be played: the next one waits too.
        if (skipJob != null) {
            cancelSkip()
            publish()
        }
        if (starting) startPlays = false
        out?.pause()
        // The output may have been paused already (the track before): Play/Pause shows the track coming waits.
        if (starting) publish()
    }

    /** A track is being asked for (not the server asked about a failure): the output still holds the last one. */
    private val starting: Boolean get() = startJob.let { it != null && it === loadJob && it.isActive }

    /**
     * Whether it plays, or will as soon as it can ([NowPlaying.wantsPlay]):
     * what Play/Pause shows, and what [toggle] turns round. Not after the
     * queue's end, a failure or a video: Play starts the track again there.
     * While the next track is counted down to after a failure, it will play:
     * Pause stops the count.
     */
    private val wantsPlay: Boolean
        get() = when {
            queue.isEmpty -> false
            skipJob != null -> true
            ended || error != null || yielded -> false
            starting -> startPlays
            else -> out?.playWhenReady == true
        }

    /** The job asking for the current track's session ([startTrack]). */
    private var startJob: Job? = null

    fun toggle() {
        if (wantsPlay) pause() else play()
    }

    fun next() {
        failedInARow = 0
        // A book's part with chapters: the next chapter, until its last.
        val chapter = chapterNow()
        if (chapter != null) {
            chapters.getOrNull(chapter + 1)?.let {
                toChapter(it.startMs)
                return
            }
        }
        if (queue.next()) startTrack(0)
    }

    fun previous() {
        failedInARow = 0
        // A book's part with chapters: back to the start of the chapter, or the one before within its first seconds,
        // as Previous does with tracks; from the first chapter's start, the part before.
        // Before the first chapter (an intro without a mark), the part's own rule.
        val chapter = chapterNow()?.takeIf { it >= 0 }
        var from = lastPosition()
        if (chapter != null) {
            val start = chapters[chapter].startMs
            val to = if (from - start > MusicQueue.RESTART_AFTER_MS) start else chapters.getOrNull(chapter - 1)?.startMs
            if (to != null) {
                toChapter(to)
                return
            }
            // Within the first chapter's first seconds, after an intro: the part before, as from the part's start.
            from = 0
        }
        val o = out
        // Pressed again while the track before is still asked for, it goes one more back: that one hasn't played.
        when (queue.previous(from)) {
            MusicQueue.Previous.Moved -> startTrack(0)
            // A queue that waits waits on, at the track's start, as a paused one does.
            MusicQueue.Previous.Restart -> when {
                waitingAt != null -> {
                    waitingAt = 0
                    sought()
                }
                o == null || session == null || ended || error != null -> startTrack(0, sameTrack = !ended)
                else -> {
                    o.seekTo(0)
                    sought()
                }
            }
        }
    }

    private val speed = MutableStateFlow(1f)

    /** The speed a book's parts play at (Speed, in the car and on the app's pages); music always plays as recorded. */
    val bookSpeed: StateFlow<Float> = speed.asStateFlow()

    fun setBookSpeed(value: Float) {
        speed.value = value
        out?.setSpeed(speedOf(queue.current))
        // The next chapter comes sooner or later, and so does the next part.
        watchChapters()
        lookAhead()
    }

    private fun speedOf(track: ItemSummary?): Float = if (track?.kind == ItemKind.Part) speed.value else 1f

    /** Moves [offsetMs] from where the track is, within it. */
    fun seekBy(offsetMs: Long) {
        val duration = position().durationMs.takeIf { it > 0 }
        val to = (lastPosition() + offsetMs).coerceAtLeast(0)
        seekTo(if (duration != null) to.coerceAtMost(duration) else to)
    }

    /** The chapters of the part playing ([NowPlaying.chapters]); empty for music. */
    private var chapters: List<Chapter> = emptyList()

    /** The part [chapters] are of, once read (or being read). */
    private var chaptersOf: Long? = null
    private var chaptersJob: Job? = null

    /** Wakes as the next chapter starts, so what plays says it ([NowPlaying.chapter]). */
    private var chapterTicker: Job? = null

    /**
     * The chapters of [track], a book's part, read from its file once per
     * part: none for music, a part of one chapter, or one the server can't
     * say (it plays as a whole meanwhile).
     */
    private fun readChapters(track: ItemSummary?) {
        if (track?.kind != ItemKind.Part) {
            chaptersJob?.cancel()
            chaptersOf = null
            chapters = emptyList()
            return
        }
        if (chaptersOf == track.id) return
        chaptersJob?.cancel()
        chaptersOf = track.id
        chapters = emptyList()
        chaptersJob = scope.launch {
            val read = try {
                env.api().item(track.id).files.firstOrNull()?.chapters.orEmpty()
            } catch (e: CancellationException) {
                throw e
            } catch (_: Exception) {
                // Read again the next time the part starts (startTrack): it plays as a whole meanwhile.
                return@launch
            }
            if (chaptersOf != track.id) return@launch
            chapters = playableChapters(read)
            publish()
            watchChapters()
        }
    }

    /**
     * To the chapter starting at [startMs]: a seek, or the track asked for
     * again from there while it is still being asked for.
     */
    private fun toChapter(startMs: Long) {
        // Asked again as Play or Pause left it while it was asked for, not as the output held the track before.
        if (starting) startTrack(startMs, sameTrack = true, play = startPlays) else seekTo(startMs)
    }

    /** Which of [chapters] the position is in; `null` when the part has none, or isn't where it plays yet. */
    private fun chapterNow(): Int? {
        if (chapters.isEmpty() || ended || (!held && waitingAt == null && askedAtMs == null)) return null
        return chapterIndexAt(chapters, lastPosition())
    }

    /** While a part with chapters plays, wakes as the next one starts (at the speed it plays) and says so. */
    private fun watchChapters() {
        chapterTicker?.cancel()
        chapterTicker = null
        if (chapters.isEmpty() || !playing) return
        chapterTicker = scope.launch {
            while (isActive) {
                val at = chapterNow() ?: return@launch
                val nextStart = chapters.getOrNull(at + 1)?.startMs ?: return@launch
                val waitMs = ((nextStart - lastPosition()) / speedOf(queue.current).coerceAtLeast(0.1f)).toLong()
                delay(waitMs.coerceIn(CHAPTER_SLACK_MS, CHAPTER_CHECK_MS) + CHAPTER_SLACK_MS)
                publish()
            }
        }
    }

    fun seekTo(positionMs: Long) {
        // After a video or a failure the session is gone: the place moves, and Play starts the track there, as for a
        // queue that waits.
        if (held && session == null && !ended && (yielded || error != null)) {
            held = false
            waitingAt = positionMs
        }
        // A queue that waits starts where it was sought to.
        if (waitingAt != null) {
            waitingAt = positionMs.coerceIn(0, (queue.current?.durationMs?.takeIf { it > 0 } ?: Long.MAX_VALUE))
            sought()
            return
        }
        if (ended || session == null) return
        out?.seekTo(positionMs.coerceAtLeast(0))
        sought()
    }

    /** The position moved: whatever shows it reads it again, paused too, when nothing else would tell it. */
    private fun sought() {
        seeks++
        publish()
        watchChapters()
        lookAhead()
    }

    fun jumpTo(index: Int) {
        failedInARow = 0
        if (queue.jumpTo(index)) startTrack(0)
    }

    fun setShuffle(shuffle: Boolean) {
        queue.setShuffle(shuffle)
        // Another track may come next.
        lookAhead()
        publish()
    }

    /** Repeats the queue, the track, or nothing, from the track playing on; the queue's end, passed already, stays. */
    fun setRepeat(repeat: MusicRepeat) {
        queue.repeat = repeat
        lookAhead()
        publish()
    }

    /**
     * A video or a channel starts: the music pauses and lets go of its
     * session, which its paused reports would otherwise keep through a
     * whole film. The queue and the place stay, and Play starts a new
     * session there. A track still being asked for isn't loaded.
     */
    fun pauseForVideo() {
        playOnReturn = false
        if (queue.isEmpty) return
        cancelSkip()
        dropAhead()
        loadJob?.cancel()
        yielded = true
        stopTicker()
        endSession()
        out?.pause()
        playing = false
        loading = false
        publish()
    }

    /**
     * The person leaves the TV app: music meant to play (playing, waiting
     * for data, or a track being asked for that will play) pauses as for a
     * video ([pauseForVideo]). With [playOnReturn], [returnAfterLeave] plays
     * it on. Music paused, at its end, failed or waiting paused (the sleep
     * timer's next track) stays as it is.
     */
    fun pauseForLeave(playOnReturn: Boolean) {
        if (!wantsPlay) return
        pauseForVideo()
        this.playOnReturn = playOnReturn
    }

    /**
     * The person is back in the TV app: music [pauseForLeave] paused plays
     * on, with [playOn], unless it was played, paused or changed meanwhile
     * (the phone's remote, a media key, a video). Either way it is
     * forgotten.
     */
    fun returnAfterLeave(playOn: Boolean) {
        val again = playOn && playOnReturn && yielded
        playOnReturn = false
        if (again) play()
    }

    /** Ends the session, stops the output and forgets the queue. */
    fun stop() {
        playOnReturn = false
        cancelSkip()
        dropAhead()
        loadJob?.cancel()
        stopTicker()
        timer.cancel()
        endSession()
        out?.release()
        out = null
        chapterTicker?.cancel()
        chapterTicker = null
        suggested = false
        queue.clear()
        readChapters(null)
        queue.repeat = MusicRepeat.Off
        held = false
        fallback = null
        recreates = 0
        playing = false
        loading = false
        ended = false
        yielded = false
        waitingAt = null
        error = null
        askedAtMs = null
        now.value = null
        order.value = MusicQueueState()
        env.playing(this, false)
        onStopped?.invoke()
    }

    /** Waits until the reports made so far went out: the end of the session [stop] ended, say. */
    suspend fun flush() = env.reporter.flush()

    /** Drops the reports that haven't gone out yet ([SessionReporter.drop]). */
    fun drop() = env.reporter.drop()

    /**
     * Plays the current track from [atMs] with a new session, ending the one
     * before. [sameTrack] keeps the transcode fallback and the count of
     * sessions asked for: it is the same track, asked again.
     */
    private fun startTrack(atMs: Long, sameTrack: Boolean = false, play: Boolean? = null) {
        val track = queue.current ?: return
        suggested = false
        if (!sameTrack) {
            fallback = null
            recreates = 0
            aheadFailed = false
        }
        // Asked for as it comes: whatever was asked ahead is let go.
        dropAhead()
        // Chapters that couldn't be read are asked for again.
        if (chapters.isEmpty()) chaptersOf = null
        val wantPlay = play ?: (!sameTrack || error != null || (out?.playWhenReady ?: true))
        waitingAt = null
        cancelSkip()
        askedAtMs = atMs
        playOnReturn = false
        loadJob?.cancel()
        stopTicker()
        endSession()
        val o = output()
        if (!sameTrack) {
            // Before the output stops: it tells its listeners there and then, and the queue they are shown is this one.
            held = false
            o.stop()
        }
        ended = false
        yielded = false
        error = null
        loading = true
        playing = false
        startPlays = wantPlay
        loadJob = scope.launch {
            // The session before goes first (stopped, then DELETE) when that is quick. Not waited for long: the server
            // ends this player's sessions as it starts the next (its transcode limit), and keeps the place a `stopped`
            // that comes later says, so on a slow network Next doesn't wait for two answers before it is asked.
            withTimeoutOrNull(FLUSH_WAIT_MS) { env.reporter.flush() }
            val started = try {
                val server = env.api()
                api = server
                ServerPlaybackSession.start(server, requestFor(track, atMs, fallback ?: PlaybackMode.Auto), env.reporter)
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                // Whether it was to play, as Play/Pause said while it was asked for.
                fail(errorOf(e), wanted = startPlays)
                return@launch
            }
            session = started
            val server = api ?: return@launch
            o.setSpeed(speedOf(track))
            o.load(sourceOf(started, server), atMs, startPlays, trackInfo(track, server))
            held = true
            askedAtMs = null
            // From now on, also while it waits paused (the sleep timer's next track).
            startTicker()
            publish()
        }
        startJob = loadJob
        // Said once the track counts as starting: Play/Pause shows whether it will play, not what the last one did.
        publish()
    }

    /** What the server is asked for [track], [atMs] into it; [ahead]: beside the track playing, which plays on. */
    private suspend fun requestFor(track: ItemSummary, atMs: Long, mode: PlaybackMode, ahead: Boolean = false) = PlaybackRequest(
        itemId = track.id,
        startMs = atMs.takeIf { it > 0 },
        mode = mode,
        clientId = env.clientId(),
        caps = capsOrNone(),
        controls = CONTROLS,
        keepPrevious = true.takeIf { ahead },
        ahead = true.takeIf { ahead },
    )

    private fun sourceOf(session: ServerPlaybackSession, server: PleyApi): EngineSource {
        val info = session.info
        return EngineSource(url = server.url(info.url), hls = info.hls, durationMs = info.durationMs.takeIf { it > 0 })
    }

    /**
     * Asks for the track that plays by itself after this one, [AHEAD_MS]
     * before this one's end (at the speed it plays), and puts it after it
     * in the output, which plays on to it without a gap ([MusicOutput.setNext]).
     * Only while this track plays, held by the output, and with a track
     * to come; not when the sleep timer ends with this track (the next one
     * waits), nor again for this track once the server refused it (out of
     * its reach, it is asked again at the next report). What was asked
     * ahead is let go when the track to come changes (Shuffle, Repeat),
     * the timer is to end with this track, or a seek takes it back more
     * than [KEEP_AHEAD_MS] from its end: the server forgets a session it
     * doesn't hear from.
     */
    private fun lookAhead() {
        val o = out
        val index = queue.upcoming
        val next = index?.let(queue::trackAt)
        val left = timeLeftMs()
        val possible = o != null && o.takesNext && session != null && held && !starting && !ended && error == null &&
            !yielded && waitingAt == null && next != null && !timer.endsWith(queue.current?.id)
        val a = ahead
        if (a != null) {
            if (possible && a.index == index && a.trackId == next?.id && (left == null || left <= KEEP_AHEAD_MS)) return
            dropAhead()
        }
        if (!possible || !playing || aheadFailed || index == null || next == null || left == null || left > AHEAD_MS) return
        val asked = Ahead(index, next.id)
        ahead = asked
        asked.job = scope.launch {
            val (started, server) = try {
                val server = env.api()
                ServerPlaybackSession.start(server, requestFor(next, 0, PlaybackMode.Auto, ahead = true), env.reporter) to server
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                if (ahead === asked) ahead = null
                // Refused (no transcode free beside this one): the next track is asked for as it comes. Out of reach for a
                // moment (a tunnel, a change of network) it is asked again at the next report, so the music still goes on
                // without a gap once the network is back.
                if (e is ApiException) aheadFailed = true
                return@launch
            }
            // Let go while it was asked for (Shuffle, a seek, Next): the server's session goes, and the output isn't told.
            if (ahead !== asked) {
                started.discard()
                return@launch
            }
            asked.session = started
            out?.setNext(sourceOf(started, server), trackInfo(next, server))
        }
    }

    /** Lets go of what [lookAhead] asked for, in the output and on the server (never played: no `stopped`). */
    private fun dropAhead() {
        val a = ahead ?: return
        ahead = null
        a.job?.cancel()
        a.session?.discard()
        out?.setNext(null, null)
    }

    /** How long the track has left at the speed it plays; `null` while that isn't known. */
    private fun timeLeftMs(): Long? {
        val p = position()
        if (p.durationMs <= 0) return null
        return ((p.durationMs - p.positionMs).coerceAtLeast(0) / speedOf(queue.current).coerceAtLeast(0.1f)).toLong()
    }

    private suspend fun capsOrNone(): ClientCaps = try {
        env.caps()
    } catch (e: CancellationException) {
        throw e
    } catch (_: Exception) {
        ClientCaps()
    }

    /** Where the queue's track is; its start while the output still holds the track before it, or where it waits ([cue]). */
    private fun lastPosition(): Long = if (held) out?.positionMs?.coerceAtLeast(0) ?: 0 else waitingAt ?: askedAtMs ?: 0

    /** How long the track has left, for the sleep timer; `null` while it isn't playing. */
    private fun trackLeftMs(): Long? {
        if (session == null || ended || error != null) return null
        val position = position()
        return if (position.durationMs > 0) position.durationMs - position.positionMs else null
    }

    /** Ends the current track's session at the output's position. */
    private fun endSession() {
        val s = session ?: return
        session = null
        s.end(lastPosition())
    }

    /** The track fails for good, [wanted] or not playing, as Play/Pause said until now. */
    private fun fail(why: MusicError, wanted: Boolean = wantsPlay) {
        loading = false
        playing = false
        error = why
        stopTicker()
        dropAhead()
        // A track the server can't play, while the music plays: the next one plays by itself after a moment to read why,
        // as a driver can't look for Next. Not when the server is out of reach (the next track would fail the same way),
        // not at the queue's end, and not round a queue none of whose tracks plays.
        if (why == MusicError.Unplayable && wanted && queue.hasNext && failedInARow + 1 < queue.size) skipSoon()
        publish()
    }

    /** Counts [SKIP_AFTER_S] seconds down, then plays the next track; Play, Pause, Next or any other track stops it. */
    private fun skipSoon() {
        cancelSkip()
        skipJob = scope.launch {
            for (left in SKIP_AFTER_S downTo 1) {
                nextIn = left
                publish()
                delay(1_000)
            }
            skipJob = null
            nextIn = null
            failedInARow++
            if (queue.next()) startTrack(0) else publish()
        }
    }

    private fun cancelSkip() {
        skipJob?.cancel()
        skipJob = null
        nextIn = null
    }

    /**
     * Progress every 10 s while the track plays (or waits for data to), and
     * every 30 s while it is paused: the server keeps a session, and its
     * transcode, as long as it hears from it.
     */
    private fun startTicker() {
        if (ticker?.isActive == true) return
        ticker = scope.launch {
            var quiet = 0L
            while (isActive) {
                delay(PROGRESS_EVERY_MS)
                val s = session ?: continue
                val wants = out?.playWhenReady == true
                quiet += PROGRESS_EVERY_MS
                if (wants || quiet >= PAUSED_EVERY_MS) {
                    quiet = 0
                    s.progress(lastPosition(), if (wants) ProgressState.Playing else ProgressState.Paused)
                    // Paused a while: the session asked ahead, which reports nothing until it plays, is let go.
                    if (!wants) dropAhead()
                }
                lookAhead()
            }
        }
    }

    private fun stopTicker() {
        ticker?.cancel()
        ticker = null
    }

    private val listener = object : MusicOutput.Listener {
        override fun onPlayingChanged(playing: Boolean) {
            this@MusicPlayback.playing = playing
            if (playing) {
                loading = false
                recreates = 0
                failedInARow = 0
            }
            publish()
            watchChapters()
            lookAhead()
        }

        override fun onBuffering(buffering: Boolean) {
            if (loading == buffering) return
            // The output stopped as the next track is asked for: still loading, not paused.
            if (!buffering && starting) return
            loading = buffering
            publish()
        }

        override fun onPlayWhenReadyChanged(playWhenReady: Boolean) {
            if (playWhenReady) timer.resumed()
            if (!playWhenReady && !ended) session?.progress(lastPosition(), ProgressState.Paused)
            publish()
        }

        override fun onEnded() {
            // The output didn't go on to what was asked ahead (it came too late): the next track is asked for as it comes.
            dropAhead()
            val s = session
            session = null
            stopTicker()
            s?.end(s.info.durationMs.takeIf { it > 0 } ?: out?.durationMs ?: lastPosition())
            // The sleep timer ends with this track: the next one waits, paused.
            val sleeping = timer.itemEnded(queue.current?.id)
            // The next one, the first again when the queue repeats, or this one again when the track does.
            if (queue.advance()) {
                startTrack(0, play = !sleeping)
            } else {
                ended = true
                playing = false
                loading = false
                publish()
            }
        }

        /**
         * The track ended and the output went on to the one asked for ahead:
         * this one's session ends at its length, as at any end, and that one's
         * is the track's.
         */
        override fun onNext() {
            val a = ahead
            ahead = null
            val track = queue.current
            // The session asked ahead, for the track still to come (what changes it takes back what was asked first).
            val next = a?.session?.takeIf { queue.upcoming == a.index && queue.trackAt(a.index)?.id == a.trackId }
            val s = session
            session = null
            // Said to play before the track before ends, so the server lists one or the other all along.
            next?.progress(0, ProgressState.Playing)
            s?.end(s.info.durationMs.takeIf { it > 0 } ?: track?.durationMs ?: 0)
            val sleeping = timer.itemEnded(track?.id)
            if (next == null) {
                // Not the track to come: that one is asked for now.
                a?.session?.discard()
                if (queue.advance()) {
                    startTrack(0, play = !sleeping)
                } else {
                    out?.stop()
                    ended = true
                    playing = false
                    loading = false
                    publish()
                }
                return
            }
            queue.advance()
            session = next
            fallback = null
            recreates = 0
            aheadFailed = false
            askedAtMs = null
            out?.setSpeed(speedOf(queue.current))
            // The sleep timer ended with the track before (and paused): this one waits at its start.
            if (sleeping) {
                out?.seekTo(0)
                next.progress(0, ProgressState.Paused)
            }
            publish()
            watchChapters()
            // Playing on with no change the output tells of: a track shorter than the ticker's step is asked ahead now.
            lookAhead()
        }

        override fun onError(error: OutputError) {
            // A video or the person leaving the TV app took over, and the session went: the output's last fetch
            // failing changes nothing, and Play starts a new session anyway.
            if (yielded) return
            val s = session
            val direct = s?.info?.method == PlaybackMethod.Direct
            when {
                error == OutputError.Stopped -> stoppedByServer()
                error == OutputError.Decode && direct && fallback == null -> decode()
                error == OutputError.Gone -> again(MusicError.Unplayable)
                error == OutputError.Network -> again(MusicError.Offline)
                error == OutputError.Other && s != null -> askServer(s, direct)
                else -> {
                    endSession()
                    fail(MusicError.Unplayable)
                }
            }
        }
    }

    /** Played as it is, the device couldn't decode it: once more as a transcode. */
    private fun decode() {
        fallback = PlaybackMode.Transcode
        startTrack(lastPosition(), sameTrack = true)
    }

    /** A new session where it was, three times in a row at most; then the track fails as [otherwise]. */
    private fun again(otherwise: MusicError) {
        if (recreates < MAX_RECREATES) {
            recreates++
            startTrack(lastPosition(), sameTrack = true)
        } else {
            endSession()
            fail(otherwise)
        }
    }

    /**
     * The player stopped for a reason it can't name (the server answered
     * 500, say), which the server can: it is asked whether [s] goes on.
     * It does: the track is at fault, asked for once more as a transcode,
     * else in a new session. Stopped from elsewhere (410): the music says
     * so. Gone, or the server out of reach: a new session where it was, as
     * for a session that expired. A track started meanwhile drops the answer.
     */
    private fun askServer(s: ServerPlaybackSession, direct: Boolean) {
        val server = api ?: return again(MusicError.Unplayable)
        loadJob?.cancel()
        loadJob = scope.launch {
            val failure = try {
                server.playbackStatus(s.id)
                null
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                e
            }
            if (session !== s) return@launch
            when {
                failure == null -> if (direct && fallback == null) decode() else again(MusicError.Unplayable)
                failure is ApiException && failure.status == 410 -> stoppedByServer()
                failure is OfflineException -> again(MusicError.Offline)
                else -> again(MusicError.Unplayable)
            }
        }
    }

    private fun publish() {
        // Under way, between tracks too, so the stream doesn't drop and open again at each: not after the end, a
        // failure or a video, which let the session go, nor once paused for a while.
        val underWay = !queue.isEmpty && !ended && !yielded && error == null
        watchRest(underWay)
        env.playing(this, underWay && !rested)
        val track = queue.current
        readChapters(track)
        order.value = queue.state
        now.value = track?.let {
            NowPlaying(
                itemId = it.id,
                title = it.title,
                artist = it.grandparentTitle,
                playing = playing,
                album = it.parentTitle,
                albumId = it.parentId,
                artistId = it.grandparentId,
                artUrl = artOf(it, api ?: currentApi()),
                durationMs = it.durationMs ?: 0,
                index = queue.index,
                count = queue.size,
                shuffle = queue.shuffle,
                loading = loading && error == null,
                ended = ended,
                error = error,
                repeat = queue.repeat,
                wantsPlay = wantsPlay,
                seeks = seeks,
                nextIn = nextIn,
                book = it.kind == ItemKind.Part,
                chapters = chapters,
                chapter = chapterNow() ?: -1,
            )
        }
    }

    /**
     * Paused as long as the server keeps a session it hasn't heard from,
     * music lets go of the event stream in the background: held, it would
     * reconnect, and the server's keep-alives wake the radio, for as long as
     * the app lives. Play holds it again; the paused reports go on meanwhile.
     */
    private fun watchRest(underWay: Boolean) {
        if (underWay && out?.playWhenReady == false) {
            if (resting == null && !rested) {
                resting = scope.launch {
                    delay(PAUSED_HOLD_MS)
                    resting = null
                    rested = true
                    publish()
                }
            }
        } else {
            resting?.cancel()
            resting = null
            rested = false
        }
    }

    private fun currentApi(): PleyApi? = try {
        env.api().also { api = it }
    } catch (_: Exception) {
        null
    }

    private fun trackInfo(track: ItemSummary, server: PleyApi) = TrackInfo(
        id = track.id,
        title = track.title,
        artist = track.grandparentTitle,
        album = track.parentTitle,
        artUrl = artPath(track)?.let { server.url(it, NOTIFICATION_ART_PX) },
    )

    companion object {
        /** Progress reports while playing, as on the web. */
        const val PROGRESS_EVERY_MS = 10_000L

        /** While paused: the server forgets a session it hasn't heard from for 90 s, and its transcode with it. */
        const val PAUSED_EVERY_MS = 30_000L

        /** Paused this long, the event stream is let go in the background: the server's window for a session unheard. */
        const val PAUSED_HOLD_MS = 90_000L

        /** How long a track that can't be played says so before the next one plays. */
        const val SKIP_AFTER_S = 5

        /** New sessions asked in a row for one track before it counts as failed. */
        const val MAX_RECREATES = 3

        /**
         * The longest a new track waits for the last one's reports to go out:
         * on a quick network they go first, as on the web; on a slow one they
         * follow it, in their order.
         */
        const val FLUSH_WAIT_MS = 150L

        /**
         * How long before a track's end the next one is asked for ([lookAhead]):
         * time to answer and to read its start on a phone's network, checked
         * at each progress report.
         */
        const val AHEAD_MS = 30_000L

        /** How far from a track's end what was asked ahead is kept: a seek a little back doesn't let it go. */
        const val KEEP_AHEAD_MS = 60_000L

        /** How long past a chapter's start it is said ([watchChapters]): the output's position runs a little behind. */
        private const val CHAPTER_SLACK_MS = 250L

        /** The longest between looks at which chapter plays, should the output's clock and this one's drift. */
        private const val CHAPTER_CHECK_MS = 60_000L

        /** What music obeys from the remote: everything (the web's). */
        val CONTROLS = listOf(Control.Play, Control.Pause, Control.Seek, Control.Skip, Control.Next, Control.Previous)

        /** The notification's art: the server's 480 bucket. */
        const val NOTIFICATION_ART_PX = 480

        /** The album art's path: the track's own picture, else its album's. */
        fun artPath(track: ItemSummary): String? =
            track.posterUrl?.takeIf { it.isNotEmpty() } ?: track.parentId?.let { "/api/items/$it/images/poster" }

        private fun artOf(track: ItemSummary, api: PleyApi?): String? {
            val path = artPath(track) ?: return null
            return api?.url(path)
        }

        /** Why starting a track failed, as music shows it. */
        fun errorOf(e: Throwable): MusicError = when {
            e is OfflineException -> MusicError.Offline
            e is ApiException && e.signedOut -> MusicError.SignedOut
            e is AuthException -> MusicError.SignedOut
            e is IllegalStateException -> MusicError.SignedOut
            else -> MusicError.Unplayable
        }
    }
}
