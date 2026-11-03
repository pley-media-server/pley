package app.pley.android.player.session

import android.graphics.Bitmap
import androidx.media3.common.text.Cue
import app.pley.android.caps.CapsSnapshot
import app.pley.android.common.describe
import app.pley.android.data.LocalFile
import app.pley.android.model.AddSubtitle
import app.pley.android.model.ItemDetail
import app.pley.android.model.ItemKind
import app.pley.android.model.ItemSummary
import app.pley.android.model.Marker
import app.pley.android.model.MarkerKind
import app.pley.android.model.MediaFile
import app.pley.android.model.MediaStream
import app.pley.android.model.PlaybackInfo
import app.pley.android.model.PlaybackMethod
import app.pley.android.model.PlaybackMode
import app.pley.android.model.ProgressState
import app.pley.android.model.ServerEvent
import app.pley.android.model.StreamChoice
import app.pley.android.model.StreamKind
import app.pley.android.model.SubtitleResult
import app.pley.android.model.SubtitleTrack
import app.pley.android.model.TranscodeInfo
import app.pley.android.model.format.episodeLabel
import app.pley.android.model.format.formatBitrate
import app.pley.android.model.format.languageName
import app.pley.android.model.format.resolutionLabel
import app.pley.android.model.format.streamLabel
import app.pley.android.network.ApiException
import app.pley.android.network.OfflineException
import app.pley.android.player.AudioDelays
import app.pley.android.player.ChapterMark
import app.pley.android.player.EngineSource
import app.pley.android.player.FoundSubtitle
import app.pley.android.player.PlaybackSpeeds
import app.pley.android.player.PlayRequest
import app.pley.android.player.PlaybackController
import app.pley.android.player.PlayerEngine
import app.pley.android.player.PlayerPhase
import app.pley.android.player.PlayerUiState
import app.pley.android.player.QualityChoice
import app.pley.android.player.SubtitleSearchUi
import app.pley.android.player.TrackChoice
import app.pley.android.player.UpNextUi
import app.pley.android.player.markers.CreditsGate
import app.pley.android.player.markers.Markers
import app.pley.android.player.queue.NextUp
import app.pley.android.player.sleep.SleepChoice
import app.pley.android.player.sleep.SleepTimer
import app.pley.android.player.subtitles.PictureSubtitles
import app.pley.android.player.subtitles.PictureTrack
import app.pley.android.player.subtitles.ServerSubtitles
import app.pley.android.player.subtitles.SubtitleSync
import app.pley.android.player.subtitles.TimedCueTrack
import app.pley.android.player.trickplay.TrickplayThumbnails
import java.io.IOException
import java.util.UUID
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Deferred
import kotlinx.coroutines.Job
import kotlinx.coroutines.async
import kotlinx.coroutines.cancel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withTimeoutOrNull

/**
 * The playback of one video, then of what plays after it, on one engine:
 * [PlaybackController] as the web player plays (apps/web/src/player/VideoPlayer.tsx).
 *
 * - **Start.** [start] runs at tap time: the item's details, the device's
 *   capabilities and the engine come together, then POST /api/playback
 *   ([PlaybackPlan]), the Dolby Vision guards ([DvGuards]), and the engine
 *   loads the answer.
 * - **Reports.** Progress every 10 s while playing and every 30 s paused,
 *   on pause, and 1 s after the last seek; `stopped` then DELETE at the end,
 *   on closing and before any new session, through the app-scoped
 *   [SessionReporter]. One server session at a time,
 *   two only while the next is made ready beside it.
 * - **Recovery.** [Recovery]'s ladder for errors, 12 s stalls and dropped frames.
 * - **Quality.** The quality chosen plays as it is; Automatic
 *   (docs/QUALITY.md) plays the original and lets [AutoQuality] step down
 *   when it doesn't play well and back up when the network allows, each
 *   change a new session where it was. Dropped frames at the original stay
 *   the recovery ladder's, and count as Automatic's step down. It starts
 *   at most at what the server knows this network carries ([NetworkStart],
 *   asked alongside the item's details, with a speed test on a network it
 *   doesn't know), else at what this device remembers.
 * - **Changes while it plays.** A quality picked, a track that needs a new
 *   session, an Automatic step or dropped frames' switch: the new session
 *   is made ready beside the one playing ([beginBeside]), and the player
 *   moves to it on the last picture, without a spinner. Recoveries from
 *   errors and stalls start the new session at once.
 * - **Tracks.** The audio is the server's choice for this device unless the
 *   person picks a track. Switched by the engine in direct play; a new
 *   session when a stream needs other audio or subtitles burned in; choices
 *   are saved with PUT /api/items/{id}/streams, the audio only when picked.
 * - **Markers and Up Next.** Skip Intro and Skip Credits; entering credits,
 *   or the end, brings up what plays next with a 10 s countdown.
 * - **Subtitles.** The server's WebVTT for streams and files beside the
 *   media, and pictures side-loaded beside a stream (its .mks), drawn at
 *   the playhead moved by the subtitle offset; a file's own text tracks
 *   move through the server's WebVTT too while an offset is set.
 * - **Speed and audio delay**, for this title only: the engine's
 *   ([EngineLink.setSpeed], [EngineLink.setAudioDelay]), with the sound
 *   decoded here and untunneled while they need it ([EngineLink.setSoundNeeds]).
 *   A stream whose sound nothing here decodes has the server convert it;
 *   a download's plays at 1×, as does a stream the server can't convert.
 * - **Find subtitles…**: the server's search online for the file playing;
 *   one picked is added on the server and shown as any subtitle file.
 * - **Sleep timer**, and a session the server stopped: it says so and
 *   plays no more.
 * - **Downloads.** A file downloaded to the device plays from there as it
 *   is, with no session on the server; its progress goes through
 *   [PlaybackEnvironment.local], which keeps it while the server is out of
 *   reach. A quality picked by hand, or a recovery's fallback, streams
 *   from the server instead.
 * - **A video on a drive** ([PlaybackEnvironment.drive]) plays as a download
 *   does, with no server at all: its tracks and chapters come from Media3
 *   as it loads ([driveTracks]), Sync moves the subtitle files beside it
 *   only, and what only a conversion would play says so in place of a
 *   fallback. Dolby Vision profile 7 plays as its HDR10 base layer.
 *
 * Runs on [scope]'s dispatcher (the main thread); the engine's events come there too.
 */
class SessionPlaybackController(
    private val request: PlayRequest,
    private val env: PlaybackEnvironment,
    private val scope: CoroutineScope,
    private val onClosed: () -> Unit = {},
) : PlaybackController, EngineLink.Listener {
    private val ui = MutableStateFlow(PlayerUiState())
    override val state: StateFlow<PlayerUiState> = ui.asStateFlow()

    private val cueState = MutableStateFlow<List<Cue>>(emptyList())
    override val cues: StateFlow<List<Cue>> = cueState.asStateFlow()

    private val texts get() = env.texts
    private var link: EngineLink? = null
    override val engine: PlayerEngine? get() = link?.engine

    // ---------------------------------------------------------------- what plays

    private var current: PlayRequest = request
    private var item: ItemDetail? = null
    private var caps: CapsSnapshot? = null
    private var file: MediaFile? = null

    /**
     * Whether the file about to play comes out as Dolby Vision, as the
     * engine judged it for the display mode it plays in ([judgeDolbyVision]),
     * and [caps] as the server is told of it then.
     */
    private var dolbyVision: Boolean? = null
    private var fileCaps: CapsSnapshot? = null

    /** The item's files downloaded here, of the same file ([downloadedFiles]). */
    private var downloads: Set<Long> = emptySet()

    /** The audio playing; `null` while automatic, until the session says which track the server chose. */
    private var audio: MediaStream? = null

    /**
     * The person named [audio] (the request, or the audio menu), so saved
     * choices carry it; otherwise it is the server's choice for this
     * device, and they leave it out.
     */
    private var audioPicked = false
    private var subtitle: MediaStream? = null

    /** The quality chosen: the menu's or the remembered one, Automatic included. */
    private var picked: QualityPreset = Qualities.AUTOMATIC

    /** What plays: the original or a transcode, Automatic's choice while [picked] is Automatic. */
    private var quality: QualityPreset = Qualities.ORIGINAL
    private var markers: List<Marker> = emptyList()

    /** The mode a Dolby Vision guard asked for, for this item's later sessions too. */
    private var guardMode: PlaybackMode? = null

    /** A quality was picked by hand in this playback: a downloaded file streams from the server instead. */
    private var streamingPicked = false

    /** A download the server made plays: its file holds the subtitle tracks a conversion keeps. */
    private var playingConverted = false

    /** A download plays, with no session on the server. */
    private var playingLocal = false

    /** A video on a drive: the tracks asked for (its window came back), chosen once Media3 has read them. */
    private var pendingAudio: Long? = null
    private var pendingSubtitle: Long? = null

    // ---------------------------------------------------------------- the session

    private val sessionState = MutableStateFlow<ServerPlaybackSession?>(null)
    private val session: ServerPlaybackSession? get() = sessionState.value
    private var sessionJob: Job? = null
    private var startJob: Job? = null

    /**
     * The run of [beginBeside] making a session ready beside the one
     * playing, while it does: what the one playing does (a stall, dropped
     * frames, Automatic's watch) no longer decides anything.
     */
    private var preparing: Any? = null

    /** The playhead as last known: kept while a session (re)starts and the engine sits at 0. */
    private var lastPositionMs = 0L

    /** Where playback last started or was sought to: an end that comes right after it can be media that falls short. */
    private var startedAtMs = 0L

    /** The engine shows this session's media: only then is its position the playhead. */
    private var mediaShown = false
    private val wantPlay = MutableStateFlow(true)
    private var ended = false
    private var closed = false
    private var tracksChecked = false

    private val recovery = Recovery()
    private val network = NetworkStart(env.api, env.clientId, env.reporter, env.now)
    private val autoQuality = AutoQuality(env.ceiling, network::report)

    /** Automatic went down: the next session to start says at what, once. */
    private var autoNoticePending = false
    private val struggle = StruggleMeter()
    private val credits = CreditsGate()
    private val nextUp = NextUp(env.api, env.local)
    private val serverSubtitles = ServerSubtitles(scope, env.subtitles, env.parser, env.work, onError = { notice(texts.subtitlesFailed) })
    private val pictureSubtitles = env.pictures?.let { PictureSubtitles(scope, it, onError = ::picturesFailed) }

    private val offsetWrites = Mutex()

    /** The server has no .mks of picture subtitles (an older server, or its transcoder): they are burned in. */
    private var picturesUnavailable = false

    /** The subtitle offset of the file playing: positive shows them later. */
    private val offset = MutableStateFlow(0L)

    /** The server stopped the playback: nothing plays again. */
    private var stopped = false
    private val sleep = SleepTimer(scope, env.now, ::itemLeftMs, { item?.id }, ::onSleep)
    private var trickplay: TrickplayThumbnails? = null
    private var trickplayWatch: Job? = null

    private var lastAdvanceAt = 0L
    private var lastAdvancePos = -1L
    private var lastStruggleSample = 0L

    /**
     * Until when dropped frames don't count: a sound played later than the
     * picture sets the picture ahead after each start and seek, and it drops
     * frames to get there.
     */
    private var struggleQuietUntil = 0L

    /** The speed and the audio delay of this title. */
    private var speed = 1f
    private var audioDelayMs = 0L

    /**
     * Another speed than 1× while the sound goes to a receiver as it is and
     * nothing here decodes it: sessions are asked for with this device's
     * decoders only, so the server converts the sound.
     */
    private var serverDecodes = false

    /** The session playing was asked for with this device's decoders only ([serverDecodes]). */
    private var playingConvertedSound = false
    private var searchJob: Job? = null
    private var seekReport: Job? = null

    // ---------------------------------------------------------------- Up Next

    private var next: ItemSummary? = null
    private var nextDetail: Pair<Long, ItemDetail>? = null

    /** Track choices saved so far ([saveStreams]). */
    private var choicesSaved = 0
    private var countdown: Job? = null
    private var nextLookup: Job? = null

    // ---------------------------------------------------------------- details

    private var detailsJob: Job? = null
    private var live: TranscodeInfo? = null

    /** Starts preparing: called once, when someone presses play. */
    fun start() {
        startJob = scope.launch { prepareFirst() }
        scope.launch { ticker() }
        scope.launch { progressReports() }
        scope.launch { subtitleCues() }
        scope.launch { serverStops() }
        scope.launch { sleep.state.collect { s -> ui.update { it.copy(sleep = s) } } }
    }

    private suspend fun prepareFirst() {
        try {
            val detail = scope.async { detailOf(current.itemId) }
            val lookup = networkLookup()
            val snapshot = scope.async {
                try {
                    env.capabilities.snapshot.value ?: env.capabilities.warmUp()
                } catch (e: CancellationException) {
                    throw e
                } catch (_: Exception) {
                    null
                }
            }
            if (link == null) link = env.engines().also { it.setListener(this) }
            val found = detail.await()
            caps = snapshot.await()
            load(found, current, lookup)
            begin(PlaybackPlan.startMs(found, current))
        } catch (e: CancellationException) {
            throw e
        } catch (e: Exception) {
            failWith(e)
        }
    }

    /**
     * What the server knows of the network, asked at once when the quality
     * remembered is Automatic (only Automatic uses it), alongside the item's details.
     */
    private fun networkLookup(): Deferred<NetworkStart.Lookup?> = scope.async {
        // A file on a drive plays as it is: there is no network to ask about.
        if (env.drive) return@async null
        val remembered = try {
            env.quality.remembered()
        } catch (e: CancellationException) {
            throw e
        } catch (_: Exception) {
            null
        }
        if (Qualities.choose(null, remembered).automatic) network.lookUp() else null
    }

    /** Where Automatic starts for this file: the server's ceiling ([lookup], with a speed test if it asks), else the device's memory. */
    private suspend fun startCeiling(lookup: Deferred<NetworkStart.Lookup?>?): StartCeiling {
        val found = try {
            lookup?.await()
        } catch (e: CancellationException) {
            throw e
        } catch (_: Exception) {
            null
        } ?: return StartCeiling.Device
        return network.resolve(found)
    }

    /** Takes [detail] as the item playing, with [req]'s choices; [lookup] is the network's, for Automatic. */
    private suspend fun load(detail: ItemDetail, req: PlayRequest, lookup: Deferred<NetworkStart.Lookup?>? = null) {
        item = detail
        current = req
        downloads = downloadedFiles(detail)
        val downloaded = req.fileId ?: detail.files.firstOrNull { it.id in downloads }?.id
        val chosen = PlaybackPlan.file(detail, downloaded, caps, env.policy)
        file = chosen
        audio = PlaybackPlan.initialAudio(chosen, req.audioIndex)
        audioPicked = audio != null
        subtitle = PlaybackPlan.initialSubtitle(chosen, req.subtitleIndex)
        // A file on a drive has its own tracks once Media3 has read them.
        pendingAudio = req.audioIndex.takeIf { env.drive && audio == null }
        pendingSubtitle = req.subtitleIndex.takeIf { env.drive && subtitle == null }
        val remembered = try {
            env.quality.remembered()
        } catch (e: CancellationException) {
            throw e
        } catch (_: Exception) {
            null
        }
        picked = Qualities.choose(chosen, remembered)
        val streams = chosen != null && localFile(chosen.id) == null
        quality = playedFor(picked, if (picked.automatic && streams) startCeiling(lookup) else StartCeiling.Device)
        lookup?.cancel()
        offset.value = chosen?.let { f ->
            try {
                env.offsets.offset(offsetKey(f.id))
            } catch (e: CancellationException) {
                throw e
            } catch (_: Exception) {
                0L
            }
        } ?: 0L
        markers = Markers.forFile(detail, chosen?.id)
        val episode = detail.kind == ItemKind.Episode
        ui.update {
            it.copy(
                itemId = detail.id,
                title = if (episode) detail.grandparentTitle ?: detail.title else if (detail.year != null) "${detail.title} (${detail.year})" else detail.title,
                subtitle = if (episode) listOf(episodeLabel(detail.parentIndex, detail.index), detail.title).filter { s -> s.isNotEmpty() }.joinToString(" · ") else null,
                durationMs = chosen?.durationMs ?: detail.durationMs ?: 0,
                markers = markers,
                chapters = chosen?.chapters.orEmpty().map { c -> ChapterMark(c.title, c.startMs, c.endMs, c.thumbUrl?.let { u -> env.api.url(u) }) },
                hasThumbnails = false,
            )
        }
        publishChoices()
        dropTrickplay()
        val sheets = env.sheets
        if (chosen != null && sheets != null) {
            val thumbs = TrickplayThumbnails(scope, env.api, chosen.id, sheets, env.sheetDecoder, env.lowMemory, env.work)
            trickplay = thumbs
            thumbs.start()
            trickplayWatch = scope.launch { thumbs.info.collect { info -> ui.update { it.copy(hasThumbnails = info != null) } } }
        }
    }

    // ---------------------------------------------------------------- sessions

    /**
     * Starts a server session at [atMs] and loads it, ending the one before.
     * The previous run of this is cancelled (a new choice wins). [alongside]:
     * a quality, track or Automatic change while it plays, made ready beside
     * the session playing ([beginBeside]). Recoveries from errors and stalls
     * start at once: the stream playing is what failed. [refused], for a
     * change the session playing can do without: the server can't make the
     * next one beside it, and it plays on.
     */
    private fun restart(atMs: Long, alongside: Boolean = false, refused: (() -> Unit)? = null) {
        if (stopped) return
        sessionJob?.cancel()
        startJob?.cancel()
        sessionJob = scope.launch {
            try {
                begin(atMs, alongside, refused)
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                failWith(e)
            }
        }
    }

    private suspend fun begin(atMs: Long, alongside: Boolean = false, refused: (() -> Unit)? = null) {
        val detail = item ?: return
        val chosen = file ?: return fail(texts.noFile, canRetry = false)
        // A file on a drive plays as it is, always: no server streams it.
        val local = localFile(chosen.id)?.takeIf { env.drive || (recovery.fallbackMode == null && !streamingPicked) }
        // A change while the media shows: the session playing plays on while the next one gets ready.
        val playing = session?.takeIf { alongside && local == null && link != null && mediaShown && !ended && ui.value.phase !is PlayerPhase.Error }
        if (playing != null) return beginBeside(playing, detail, chosen, atMs, refused)

        // The session before ends first ("stopped", then DELETE), unless the server is slow to say so.
        if (endSession(atMs)) withTimeoutOrNull(END_WAIT_MS) { env.reporter.flush() }
        reset(atMs, handoff = false)
        playingConverted = local?.converted == true
        judgeDolbyVision(detail, chosen)
        if (local != null) return beginLocal(detail, chosen, local, atMs)
        loadSession(detail, chosen, startSession(detail, chosen, choicesAt(detail, chosen, atMs), keepPrevious = false), atMs)
    }

    /**
     * Asks the engine whether [chosen]'s video comes out as Dolby Vision in
     * the display mode it plays in, before the server is asked: the server
     * is told the screen's Dolby Vision only where it is shown, and the
     * engine's decoder follows the same answer.
     */
    private suspend fun judgeDolbyVision(detail: ItemDetail, chosen: MediaFile) {
        dolbyVision = try {
            link?.dolbyVisionShown(PlaybackPlan.videoStream(chosen), chosen.durationMs ?: detail.durationMs)
        } catch (e: CancellationException) {
            throw e
        } catch (_: Exception) {
            null
        }
        fileCaps = PlaybackPlan.capsFor(caps, dolbyVision, decoded = serverDecodes)
    }

    /**
     * A change while it plays (a quality, a track, Automatic's step): the
     * next session starts beside [playing] (`keep_previous`), which plays on
     * while the next one's stream is asked for as the engine will ask for it
     * ([StreamWarmer]), so the transcoder makes its first segment; then the
     * player moves over from where [playing] got to, its last picture kept
     * meanwhile ([PlayerUiState.handoff]). With no transcode free for two at
     * once (503) it moves as before: [playing] ends first. A change [playing]
     * can do without ([refused]) is dropped instead, and [playing] plays on,
     * when the server can't make it, or has no transcode free that ending
     * [playing] would give back (it plays the file as it is).
     */
    private suspend fun beginBeside(
        playing: ServerPlaybackSession,
        detail: ItemDetail,
        chosen: MediaFile,
        atMs: Long,
        refused: (() -> Unit)? = null,
    ) {
        val token = Any()
        preparing = token
        // The stall watch starts over: the next session is on its way.
        lastAdvanceAt = env.now()
        try {
            judgeDolbyVision(detail, chosen)
            val started = try {
                startSession(detail, chosen, choicesAt(detail, chosen, atMs), keepPrevious = true)
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                val noneFree = e is ApiException && e.status == NO_TRANSCODE_FREE
                if (refused != null && !(noneFree && playing.info.method != PlaybackMethod.Direct)) {
                    refused()
                    return
                }
                if (e !is ApiException) throw e
                if (!noneFree) {
                    // As when the session before ended first: it ends, and the screen says what failed.
                    lastPositionMs = livePosition()
                    endSession(lastPositionMs)
                    throw e
                }
                null
            }
            if (started == null) {
                val at = livePosition()
                if (endSession(at)) withTimeoutOrNull(END_WAIT_MS) { env.reporter.flush() }
                reset(at, handoff = false)
                playingConverted = false
                return loadSession(detail, chosen, startSession(detail, chosen, choicesAt(detail, chosen, at), keepPrevious = false), at)
            }
            try {
                if (started.info.hls) withTimeoutOrNull(PREPARE_MAX_MS) { env.warmer.warm(env.api.url(started.info.url), atMs + PREPARE_LEAD_MS) }
            } catch (e: CancellationException) {
                started.discard()
                throw e
            } catch (_: Exception) {
                // Not asked for ahead: the player moves over all the same, and waits for it there.
            }
            // The session playing ended meanwhile (its end, the server's stop): this one isn't needed.
            if (session !== playing || closed || stopped) {
                started.discard()
                return
            }
            // From where the last one got to while this one got ready.
            val at = livePosition()
            endSession(at)
            reset(at, handoff = true)
            playingConverted = false
            loadSession(detail, chosen, started, at)
        } finally {
            if (preparing === token) preparing = null
        }
    }

    /** A new session's start at [atMs]: where it is, nothing ended, shown as preparing. */
    private fun reset(atMs: Long, handoff: Boolean) {
        lastPositionMs = atMs
        startedAtMs = atMs
        mediaShown = false
        tracksChecked = false
        ended = false
        struggle.reset()
        quietAfterJump()
        recovery.onJump()
        lastAdvanceAt = env.now()
        lastAdvancePos = -1
        // Automatic audio is chosen again for each session: a stream (a lower
        // quality, say) has the track it copies rather than converting the
        // one the file played as it is.
        if (!audioPicked) audio = null
        ui.update { it.copy(phase = PlayerPhase.Preparing, positionMs = atMs, skipLabel = null, handoff = handoff) }
    }

    /** What a session of [chosen] at [atMs] asks for, as chosen now. */
    private fun choicesAt(detail: ItemDetail, chosen: MediaFile, atMs: Long): PlaybackPlan.Choices {
        // A lower quality is always a transcode; the original keeps a fallback's mode, or a guard's.
        val mode = if (!quality.original) PlaybackMode.Transcode else recovery.fallbackMode ?: guardMode ?: quality.mode
        val decision = fileCaps?.let { c -> runCatching { env.policy.decide(detail, chosen.id, c) }.getOrNull() }
        // Automatic audio names no track: the server picks it again.
        val wanted = audio?.takeIf { audioPicked }
        return PlaybackPlan.Choices(
            file = chosen,
            startMs = atMs,
            audio = wanted,
            subtitle = subtitle,
            quality = quality,
            mode = mode,
            streaming = PlaybackPlan.streams(mode, decision, chosen, wanted, fileCaps),
            sideload = sideload,
        )
    }

    /**
     * POST /api/playback for [first], asked again with a Dolby Vision
     * guard's mode, or with the track indexes, when the answer needs them.
     */
    private suspend fun startSession(detail: ItemDetail, chosen: MediaFile, first: PlaybackPlan.Choices, keepPrevious: Boolean): ServerPlaybackSession {
        var choices = first
        var sent = PlaybackPlan.request(detail.id, choices, env.clientId(), fileCaps, keepPrevious)
        var started = ServerPlaybackSession.start(env.api, sent, env.reporter)

        DvGuards.retryMode(started.info, chosen, fileCaps)?.let { guarded ->
            started.discard()
            guardMode = guarded
            choices = choices.copy(mode = guarded, streaming = true)
            sent = PlaybackPlan.request(detail.id, choices, env.clientId(), fileCaps, keepPrevious)
            started = ServerPlaybackSession.start(env.api, sent, env.reporter)
        }
        if (PlaybackPlan.needsIndexes(started.info, sent, choices)) {
            started.discard()
            choices = choices.copy(streaming = true)
            sent = PlaybackPlan.request(detail.id, choices, env.clientId(), fileCaps, keepPrevious)
            started = ServerPlaybackSession.start(env.api, sent, env.reporter)
        }
        return started
    }

    /** Plays [started] from [atMs]: the session playing from now on, loaded into the engine. */
    private fun loadSession(detail: ItemDetail, chosen: MediaFile, started: ServerPlaybackSession, atMs: Long) {
        sessionState.value = started
        live = null
        playingLocal = false
        playingConvertedSound = serverDecodes
        val info = started.info
        // Automatic: the track the server chose plays, switched to by the engine in direct play.
        if (audio == null) audio = PlaybackPlan.playedAudio(chosen, info.audioIndex)
        autoQuality.jumped()
        syncAutoOriginal(info)
        if (autoNoticePending) {
            autoNoticePending = false
            playingPreset(info).takeIf { !it.original }?.let { notice(texts.loweredNotice(label(it))) }
        }
        ui.update {
            it.copy(
                method = texts.method(info),
                note = noteNow(),
                canPlayOriginal = canPlayOriginal(),
                durationMs = info.durationMs.takeIf { d -> d > 0 } ?: it.durationMs,
                canFindSubtitles = true,
                // A download's sound kept the speed at 1×: the server can convert this one's.
                speedNote = null,
            )
        }
        publishChoices()
        refreshDetails()
        val engineLink = link ?: return
        // The video as it arrives, so a TV matches its display mode before the
        // first frame: the file's stream when it is copied; a transcode keeps
        // the frame rate, but its dynamic range is the server's to choose.
        val video = PlaybackPlan.videoStream(chosen)?.let { v ->
            if (info.method == PlaybackMethod.Transcode) v.copy(dynamicRange = null, dvProfile = null, dvBlCompat = null) else v
        }
        val source = EngineSource(
            url = env.api.url(info.url),
            hls = info.hls,
            mimeType = if (info.hls) null else PlaybackPlan.mimeTypeOf(chosen.container),
            video = video,
            durationMs = info.durationMs.takeIf { it > 0 } ?: chosen.durationMs ?: detail.durationMs,
            dolbyVision = dolbyVision.takeIf { info.method != PlaybackMethod.Transcode },
        )
        engineLink.prepare(source, atMs, wantPlay.value)
        engineLink.setSpeed(speed)
        engineLink.setAudioDelay(audioDelayMs)
        applyTracks()
    }

    /**
     * Plays [chosen] from its download [local], as it is: a session of the
     * device's own, whose reports go through the downloads (sent at once,
     * or kept while the server is out of reach), and the subtitles saved
     * beside the file. A file on a drive plays the same way: its reports
     * keep where it stopped on the device, with its length as Media3 reads it.
     */
    private fun beginLocal(detail: ItemDetail, chosen: MediaFile, local: LocalFile, atMs: Long) {
        // No server to choose the audio: the download's own choice.
        if (audio == null) audio = PlaybackPlan.localAudio(chosen, caps)
        val info = PlaybackInfo(
            sessionId = "local-${UUID.randomUUID()}",
            method = PlaybackMethod.Direct,
            url = local.uri,
            fileId = chosen.id,
            durationMs = chosen.durationMs ?: detail.durationMs ?: 0,
            audioIndex = audio?.index,
            subtitles = PlaybackPlan.subtitleStreams(chosen).map { s ->
                SubtitleTrack(
                    index = s.index,
                    codec = s.codec,
                    language = s.language,
                    title = s.title,
                    default = s.default,
                    forced = s.forced,
                    external = s.external,
                    textBased = s.textBased == true,
                    url = local.subtitles[s.index],
                )
            },
        )
        val length = { info.durationMs.takeIf { it > 0 } ?: link?.durationMs?.takeIf { it > 0 } }
        sessionState.value = ServerPlaybackSession(LocalReportsApi(env.api, env.local), detail.id, info, env.reporter, length)
        live = null
        playingLocal = true
        playingConvertedSound = false
        ui.update {
            it.copy(
                method = if (env.drive) texts.method(info) else texts.downloaded,
                note = null,
                canPlayOriginal = false,
                durationMs = info.durationMs.takeIf { d -> d > 0 } ?: it.durationMs,
                // Away from the server, nothing to search with.
                canFindSubtitles = false,
            )
        }
        publishChoices()
        refreshDetails()
        val engineLink = link ?: return
        val source = EngineSource(
            url = local.uri,
            hls = false,
            mimeType = PlaybackPlan.mimeTypeOf(chosen.container),
            video = PlaybackPlan.videoStream(chosen),
            durationMs = info.durationMs.takeIf { it > 0 },
            dolbyVision = dolbyVision,
            profile7BaseLayer = env.drive,
        )
        engineLink.prepare(source, atMs, wantPlay.value)
        engineLink.setSpeed(speed)
        engineLink.setAudioDelay(audioDelayMs)
        applyTracks()
    }

    /**
     * The item's details: the server's (or what was saved of them). A
     * download waits for the server [SERVER_DETAIL_WAIT_MS] at most, then
     * starts from what was saved with it: away from home, the server's
     * address may take long to fail.
     */
    private suspend fun detailOf(itemId: Long): ItemDetail {
        val saved = env.local.detail(itemId)?.takeIf { d -> d.files.any { env.local.file(it.id) != null } }
        return try {
            if (saved == null) env.details.detail(itemId) else withTimeoutOrNull(SERVER_DETAIL_WAIT_MS) { env.details.detail(itemId) } ?: saved
        } catch (e: CancellationException) {
            throw e
        } catch (e: Exception) {
            saved ?: env.local.detail(itemId) ?: throw e
        }
    }

    /**
     * The files of [detail] whose download here is of that very file, by
     * its name and size as the details saved with it have them: a server
     * whose database was made anew numbers its files again, and another
     * title's download must not play for this one (it streams instead).
     */
    private suspend fun downloadedFiles(detail: ItemDetail): Set<Long> {
        val here = detail.files.filter { env.local.file(it.id) != null }
        if (here.isEmpty()) return emptySet()
        val saved = env.local.detail(detail.id)?.files.orEmpty().associateBy { it.id }
        return here.filter { f ->
            saved[f.id]?.let { s -> s.path == f.path && (s.size <= 0 || f.size <= 0 || s.size == f.size) } ?: true
        }.mapTo(HashSet()) { it.id }
    }

    /** The download of file [id] of the item playing, when it is of that file. */
    private fun localFile(id: Long): LocalFile? = if (id in downloads) env.local.file(id) else null

    /** Reports `stopped` at [positionMs] and deletes the session playing; false when none plays. */
    private fun endSession(positionMs: Long): Boolean {
        val s = session ?: return false
        sessionState.value = null
        s.end(positionMs)
        return true
    }

    /**
     * The tracks chosen, where they play: the engine in direct play, the
     * server's WebVTT for streams, files beside the media and text moved by
     * an offset, the server's .mks for pictures beside a stream.
     */
    private fun applyTracks() {
        val info = session?.info ?: return
        val engineLink = link ?: return
        val chosenFile = file
        val sub = subtitle
        if (!info.hls) {
            audio?.let { a -> engineLink.selectAudio(PlaybackPlan.audioStreams(chosenFile).indexOf(a), a.language) }
            when {
                sub == null -> {
                    engineLink.selectText(null, null)
                    hideServerSubtitles()
                }
                // Pictures next to the media: the server's .mks of them (it
                // makes no WebVTT of pictures).
                sub.external && PlaybackPlan.isImage(sub) -> {
                    engineLink.selectText(null, null)
                    if (sideload) showPictures(sub) else hideServerSubtitles()
                }
                sub.external || (offset.value != 0L && !PlaybackPlan.isImage(sub) && !env.drive) -> {
                    engineLink.selectText(null, null)
                    showServerSubtitles(sub)
                }
                else -> {
                    val embedded = if (playingConverted) {
                        PlaybackPlan.convertedSubtitleStreams(chosenFile)
                    } else {
                        PlaybackPlan.subtitleStreams(chosenFile).filter { !it.external }
                    }
                    engineLink.selectText(embedded.indexOf(sub).takeIf { it >= 0 }, sub.language)
                    hideServerSubtitles()
                }
            }
        } else {
            engineLink.selectText(null, null)
            when {
                sub == null -> hideServerSubtitles()
                !PlaybackPlan.isImage(sub) -> showServerSubtitles(sub)
                !info.burnSubtitles && sideload -> showPictures(sub)
                else -> hideServerSubtitles()
            }
        }
    }

    private fun showServerSubtitles(sub: MediaStream) {
        val info = session?.info ?: return
        pictureSubtitles?.hide()
        val path = info.subtitles.firstOrNull { it.index == sub.index }?.url ?: "/api/files/${info.fileId}/subtitles/${sub.index}.vtt"
        serverSubtitles.show(info.fileId, sub.index, if (isSavedSubtitle(path, info.url)) path else env.api.url(path))
    }

    /**
     * Pictures beside a stream: the server's .mks of the track, at the stream's own positions (the file's), asked for
     * from where the player is each time, until the track is whole.
     */
    private fun showPictures(sub: MediaStream) {
        val info = session?.info ?: return
        val pictures = pictureSubtitles ?: return
        serverSubtitles.hide()
        val url = env.api.url(PlaybackPlan.pictureSubtitlesPath(info.fileId, sub.index))
        pictures.show(info.fileId, sub.index) { PlaybackPlan.pictureUrlAt(url, livePosition()) }
    }

    private fun hideServerSubtitles() {
        serverSubtitles.hide()
        pictureSubtitles?.hide()
    }

    /** Pictures chosen for a stream are drawn by this player, not burned in. */
    private val sideload: Boolean get() = !picturesUnavailable && PlaybackPlan.sideloadsPictures(caps, pictureSubtitles != null)

    /**
     * The pictures beside the stream couldn't be loaded. When the server
     * can't make them at all (404: a server without the route; 503: its
     * transcoder can't), the stream starts again with them burned in, as
     * before side-loading; otherwise the screen says so.
     */
    private fun picturesFailed(error: Throwable) {
        val status = (error as? ApiException)?.status
        val info = session?.info
        if ((status == 404 || status == 503) && !closed && !stopped && info != null && info.hls && !info.burnSubtitles) {
            picturesUnavailable = true
            if (PlaybackPlan.isImage(subtitle)) {
                publishChoices()
                restart(livePosition(), alongside = true)
                return
            }
        }
        notice(texts.subtitlesFailed)
    }

    /**
     * Where a file's subtitle offset is kept: the file's own address, so
     * files of different servers don't mix (a file on a drive, its id made
     * of its path), and the profile watching, so one profile's timing never
     * moves another's subtitles.
     */
    private fun offsetKey(fileId: Long): String {
        val file = if (env.drive) "drive:$fileId" else env.api.url("/api/files/$fileId")
        return env.profileId?.let { "$file#profile=$it" } ?: file
    }

    // ---------------------------------------------------------------- engine events

    override fun onStatus(status: EngineStatus) {
        if (session == null || closed) return
        // The new stream shows its own picture (or won't): the last one no longer stands in.
        if (status != EngineStatus.Buffering && ui.value.handoff) ui.update { it.copy(handoff = false) }
        when (status) {
            EngineStatus.Buffering -> if (!ended) phase(PlayerPhase.Buffering)
            EngineStatus.Ready -> {
                mediaShown = true
                phase(if (link?.playWhenReady == true) PlayerPhase.Playing else PlayerPhase.Paused)
            }
            EngineStatus.Ended -> if (endedShort()) onEndedShort() else onEnded()
            EngineStatus.Idle -> Unit
        }
    }

    override fun onPlayWhenReady(playWhenReady: Boolean) {
        if (closed) return
        val was = wantPlay.value
        wantPlay.value = playWhenReady
        if (playWhenReady) sleep.resumed()
        if (session == null) return
        if (!playWhenReady) {
            struggle.reset()
            if (was && !ended) report(ProgressState.Paused)
            if (link?.status == EngineStatus.Ready) phase(PlayerPhase.Paused)
        } else if (link?.status == EngineStatus.Ready) {
            phase(PlayerPhase.Playing)
        }
    }

    override fun onPlaying(playing: Boolean) {
        if (playing && session != null) phase(PlayerPhase.Playing)
    }

    /** A video or an audio track that no decoder here takes: the server has to help, as for a decoder error. */
    override fun onTracks(tracks: EngineTracks) {
        if (env.drive && session != null) driveTracks(tracks)
        if (tracksChecked || session == null) return
        tracksChecked = true
        val videoUnplayable = tracks.video.isNotEmpty() && tracks.video.none { it.supported }
        val audioUnplayable = tracks.audio.isNotEmpty() && tracks.audio.none { it.supported }
        if (videoUnplayable || audioUnplayable) {
            val what = if (videoUnplayable) "video" else "audio"
            onError(EngineError.Unsupported(audio = !videoUnplayable, container = false, detail = "no $what track plays here"))
        }
    }

    /**
     * A file on a drive, as Media3 read it: its streams and chapters fill
     * the menus, the audio it plays shows as chosen, and tracks asked for
     * before (its window came back) are chosen once they are there.
     */
    private fun driveTracks(tracks: EngineTracks) {
        val detail = item ?: return
        val chosen = file ?: return
        val streams = DriveStreams.streams(tracks, chosen.streams.filter { it.external })
        val known = chosen.copy(streams = streams, chapters = DriveStreams.chapters(tracks.chapters, link?.durationMs ?: 0))
        if (known == chosen) return
        file = known
        item = detail.copy(files = detail.files.map { if (it.id == known.id) known else it })
        val audios = PlaybackPlan.audioStreams(known)
        val asked = pendingAudio?.let { wanted -> audios.firstOrNull { it.index.toLong() == wanted } }
        val playing = audios.getOrNull(tracks.audio.indexOfFirst { it.selected })
        audio = when {
            asked != null -> asked
            audioPicked -> audios.firstOrNull { it.index == audio?.index } ?: playing
            else -> playing
        }
        if (asked != null) audioPicked = true
        pendingSubtitle?.let { wanted -> PlaybackPlan.subtitleStreams(known).firstOrNull { it.index.toLong() == wanted } }?.let { subtitle = it }
        val choose = pendingAudio != null || pendingSubtitle != null
        pendingAudio = null
        pendingSubtitle = null
        ui.update { it.copy(chapters = known.chapters.map { c -> ChapterMark(c.title, c.startMs, c.endMs, null) }) }
        publishChoices()
        refreshDetails()
        if (choose) applyTracks()
    }

    override fun onError(error: EngineError) {
        val s = session ?: return
        if (closed) return
        if (error is EngineError.Stopped) {
            // A stream answers 410 once the server stopped it, also to players that didn't hear the event.
            stopByServer(error.message, byAdmin = true)
            return
        }
        val videoCodec = PlaybackPlan.videoStream(file)?.codec
        act(recovery.decide(error, s.info.method, videoCodec), starving = false)
    }

    /** Does what the recovery ladder says; [alongside]: a new session is made ready beside the one playing, which plays on. */
    private fun act(action: Recovery.Action, starving: Boolean, alongside: Boolean = false) {
        // A file on a drive has no server: what only a conversion would play doesn't play, and a
        // file that still can't be read (the drive pulled out) says so, not that a server is away.
        if (env.drive && action is Recovery.Action.NewSession && action.mode != null) {
            return fail(texts.driveUnplayable, canRetry = false)
        }
        if (env.drive && action is Recovery.Action.Fail && (action.failure == Failure.Network || action.failure == Failure.Stopped)) {
            return fail(texts.driveUnreadable, canRetry = true)
        }
        when (action) {
            is Recovery.Action.NewSession -> {
                if (action.reason == FallbackReason.NotSmooth && recovery.struggleSwitched && !automatic) {
                    ui.update { it.copy(canPlayOriginal = true) }
                }
                restart(livePosition(), alongside)
            }
            Recovery.Action.CheckServer -> {
                val s = session ?: return
                scope.launch {
                    val speed = try {
                        env.api.playbackStatus(s.id).transcode?.speed
                    } catch (e: CancellationException) {
                        throw e
                    } catch (_: IOException) {
                        null
                    }
                    if (session === s) act(recovery.afterServerCheck(speed, starving), starving)
                }
            }
            Recovery.Action.SuggestLowerQuality -> {
                // Automatic goes lower by itself; otherwise the person is offered a lower quality.
                val step = if (automatic) autoQuality.serverTooSlow(env.now()) else null
                if (step != null) applyAuto(step) else ui.update { it.copy(suggestLowerQuality = true, note = texts.slowServer) }
            }
            is Recovery.Action.Fail -> fail(
                texts.failure(action.failure, action.detail),
                canRetry = action.failure != Failure.Unplayable,
                offline = action.failure == Failure.Network,
            )
        }
    }

    /**
     * The media ended moments after where it started, a minute or more
     * before the end the server gives: the stream or file doesn't hold that
     * part (resuming past its real end). That isn't the end of the video.
     */
    private fun endedShort(): Boolean {
        if (ended) return false
        val engineLink = link ?: return false
        val expected = session?.info?.durationMs?.takeIf { it > 0 } ?: file?.durationMs ?: return false
        val reached = engineLink.positionMs
        return reached + SHORT_END_MS < expected && reached < startedAtMs + SHORT_PLAY_MS
    }

    /**
     * Recovers from media that fell short ([Recovery.onEndedShort]). Where
     * it was to play from stays the playhead, for the next session and the
     * reports: nothing is reported at the media's end, nothing is marked watched.
     */
    private fun onEndedShort() {
        val s = session ?: return
        mediaShown = false
        lastPositionMs = startedAtMs
        ui.update { it.copy(positionMs = startedAtMs, skipLabel = null) }
        act(recovery.onEndedShort(s.info.method), starving = false)
    }

    private fun onEnded() {
        if (ended) return
        ended = true
        // The sleep timer ends with this item: what comes next waits for the person.
        val sleeping = sleep.itemEnded(item?.id)
        val s = session
        val duration = s?.info?.durationMs?.takeIf { it > 0 } ?: link?.durationMs?.takeIf { it > 0 } ?: lastPositionMs
        lastPositionMs = duration
        endSession(duration)
        ui.update { it.copy(positionMs = duration, skipLabel = null) }
        val detail = item ?: return phase(PlayerPhase.Ended)
        if (!nextUp.hasNext(detail.kind, current.queue)) return phase(PlayerPhase.Ended)
        phase(PlayerPhase.Paused)
        val known = next
        if (known != null) {
            showUpNext(known, restartCountdown = !sleeping && countdown?.isActive != true)
            return
        }
        lookUpNext { found -> if (found == null) phase(PlayerPhase.Ended) else showUpNext(found, restartCountdown = !sleeping) }
    }

    // ---------------------------------------------------------------- the clock

    private suspend fun ticker() {
        while (true) {
            delay(TICK_MS)
            tick()
        }
    }

    private fun tick() {
        val engineLink = link ?: return
        val s = session ?: return
        if (closed) return
        val now = env.now()
        val reconnecting = engineLink.reconnecting
        if (reconnecting != ui.value.reconnecting) ui.update { it.copy(reconnecting = reconnecting) }
        if (engineLink.soundUndecodable) soundUndecodable()
        val moving = mediaShown && engineLink.status == EngineStatus.Ready && engineLink.playWhenReady && !ended
        if (mediaShown && !ended) {
            val pos = engineLink.positionMs
            lastPositionMs = pos
            if (moving) recovery.onPosition(pos)
            val marker = if (ui.value.phase is PlayerPhase.Error) null else Markers.active(markers, pos)
            ui.update {
                it.copy(
                    positionMs = pos,
                    bufferedMs = engineLink.bufferedMs,
                    durationMs = s.info.durationMs.takeIf { d -> d > 0 } ?: engineLink.durationMs.takeIf { d -> d > 0 } ?: it.durationMs,
                    skipLabel = marker?.let { m -> if (m.kind == MarkerKind.Intro) texts.skipIntro else texts.skipCredits },
                )
            }
            credits.entered(marker)?.let { onCredits() }
        }
        // The next session is on its way, ready within PREPARE_MAX_MS: this one's stalls and drops no longer count.
        if (preparing != null) {
            lastAdvanceAt = now
            return
        }
        // A stall: meant to play, not moving (web: STALL_MS). Not while the engine reconnects: a new
        // session wouldn't reach the server either, and the engine goes on from where it stood.
        val stalling = wantPlay.value && !ended && ui.value.phase !is PlayerPhase.Error && engineLink.status != EngineStatus.Idle &&
            !reconnecting
        val pos = if (mediaShown) engineLink.positionMs else -1
        if (!stalling || pos != lastAdvancePos) {
            lastAdvanceAt = now
            lastAdvancePos = pos
        } else if (now - lastAdvanceAt >= STALL_MS) {
            lastAdvanceAt = now
            // A file on a drive waits for no server, and the same file again would only stand still
            // again: one whose picture never came isn't one this TV plays; one that showed and then
            // stood still can't be read any more (the drive pulled out), as on LG TVs.
            if (env.drive) return fail(if (mediaShown) texts.driveUnreadable else texts.driveStuck, canRetry = true)
            val starving = engineLink.status == EngineStatus.Buffering && engineLink.bufferedMs - engineLink.positionMs < STARVING_MS
            act(recovery.onStall(s.info.method, starving), starving)
            return
        }
        // Dropped frames, sampled every 2 s of playback.
        if (moving && now >= struggleQuietUntil && now - lastStruggleSample >= STRUGGLE_SAMPLE_MS) {
            lastStruggleSample = now
            val counts = engineLink.frameCounts()
            if (counts != null && struggle.sample(now, counts)) {
                struggle.reset()
                onStruggling(s.info.method, now)
                return
            }
        }
        // A stream in several sizes: Media3 moves between them as the
        // connection allows, without a new session (stalls and dropped
        // frames still lower it); Automatic only steps back up from it.
        if (automatic && !stopped && ui.value.phase !is PlayerPhase.Error) {
            val step = autoQuality.sample(
                AutoSample(
                    atMs = now,
                    watching = mediaShown && wantPlay.value && !ended && !reconnecting && engineLink.status != EngineStatus.Idle,
                    buffering = engineLink.status == EngineStatus.Buffering,
                    positionMs = engineLink.positionMs,
                    bufferedMs = engineLink.bufferedMs,
                    durationMs = ui.value.durationMs,
                    bandwidthKbps = engineLink.bandwidthKbps,
                    sizes = s.info.adaptive,
                ),
            )
            if (step != null) applyAuto(step)
        }
    }

    /**
     * The decoder drops frames. The original switches to the server's
     * quality, once, by the recovery ladder (in Automatic, that is its step
     * down); a transcode in Automatic steps down to fewer lines.
     */
    private fun onStruggling(method: PlaybackMethod, now: Long) {
        // A file on a drive has nothing smoother to go to: it plays on as it is.
        if (env.drive) return
        val action = recovery.onStruggling(method)
        if (action != null) {
            if (automatic) {
                autoQuality.deviceFellBehindAtOriginal(now, convertedKbps = null)
                autoNoticePending = true
            }
            act(action, starving = false, alongside = true)
        } else if (automatic) {
            autoQuality.deviceFellBehind(now)?.let(::applyAuto)
        }
    }

    /**
     * Plays what Automatic asked for, from where it is: a new session at
     * that quality. Going down says so once, as the new session starts;
     * going back up is silent.
     */
    private fun applyAuto(step: AutoStep) {
        if (closed || stopped || item == null) return
        autoQuality.applied(step, env.now())
        quality = step.to
        autoNoticePending = !step.up
        ui.update { it.copy(suggestLowerQuality = false) }
        publishChoices()
        restart(livePosition(), alongside = true)
    }

    /** Automatic plays, streamed from the server: it watches and adapts (a download plays as it is). */
    private val automatic: Boolean get() = picked.automatic && autoQuality.active

    /**
     * What [choice] plays: Automatic starts afresh for the file (at the
     * original, or at most at [start]'s ceiling), a download plays as it
     * is, and every other quality plays as itself.
     */
    private fun playedFor(choice: QualityPreset, start: StartCeiling = StartCeiling.Device): QualityPreset {
        val f = file
        if (!choice.automatic || f == null || localFile(f.id) != null) {
            autoQuality.stop()
            return if (choice.automatic) Qualities.ORIGINAL else choice
        }
        val file = AutoFile(
            offered = Qualities.forFile(f),
            originalKbps = Qualities.fileKbps(f),
            lines = Qualities.linesClassOf(f),
            cap = Qualities.closest(f),
        )
        return autoQuality.begin(file, env.now(), start)
    }

    /** Tells Automatic what the original plays at in [info]: the file, or the server's conversion of it. */
    private fun syncAutoOriginal(info: PlaybackInfo) {
        if (!automatic || !quality.original) return
        val converted = info.method == PlaybackMethod.Transcode
        val kbps = if (converted) serverQuality(info)?.maxBitrateKbps else Qualities.fileKbps(file)
        autoQuality.originalPlaysAt(kbps, converted)
    }

    /** The quality the server converts the original at in [info]; `null` when it doesn't, or plays a quality asked for. */
    private fun serverQuality(info: PlaybackInfo?): QualityPreset? =
        info?.autoQuality?.takeIf { quality.original }?.let { Qualities.byId(it) }?.takeIf { !it.original && !it.automatic }

    /** [preset]'s name for the file playing: one taller than the file goes by the file's lines ("8 Mbps 720p"). */
    private fun label(preset: QualityPreset): String = texts.quality(preset, Qualities.linesClassOf(file))

    /** What plays in [info]: the quality, or the original's conversion at the server's quality. */
    private fun playingPreset(info: PlaybackInfo?): QualityPreset = serverQuality(info) ?: quality

    /** Why it plays this way when the player chose it: Automatic's lowering, else a fallback's reason. */
    private fun noteNow(): String? =
        autoQuality.loweredFor?.takeIf { automatic }?.let(texts::lowered) ?: recovery.fallbackReason?.let(texts::fallback)

    /** "Play original" after the switch for dropped frames; Automatic has Original in its menu instead. */
    private fun canPlayOriginal(): Boolean = recovery.struggleSwitched && recovery.fallbackMode != null && !automatic

    /** Progress every 10 s while playing and every 30 s paused, so the server keeps the session and its transcode. */
    private suspend fun progressReports() {
        combine(sessionState, wantPlay) { s, playing -> s to playing }.collectLatest { (s, playing) ->
            if (s == null) return@collectLatest
            while (true) {
                delay(if (playing) PROGRESS_MS else PAUSED_HEARTBEAT_MS)
                if (!ended && ui.value.phase !is PlayerPhase.Error) report(if (playing) ProgressState.Playing else ProgressState.Paused)
            }
        }
    }

    /**
     * The server's subtitles showing at the playhead moved by the offset
     * (text, or pictures beside a stream), worked out when what shows changes.
     */
    private suspend fun subtitleCues() {
        val pictures = pictureSubtitles?.track ?: MutableStateFlow<PictureTrack?>(null)
        combine(serverSubtitles.index, pictures, offset) { text, picture, shift -> Pair<TimedCueTrack?, Long>(text ?: picture, shift) }
            .collectLatest { (track, shift) ->
                if (track == null) {
                    cueState.value = emptyList()
                    return@collectLatest
                }
                while (true) {
                    val pos = livePosition()
                    val showing = track.cuesAt(SubtitleSync.trackTime(pos, shift))
                    if (showing != cueState.value) cueState.value = showing
                    // Media time: at 2× it comes in half the time.
                    val change = SubtitleSync.untilChange(track, pos, shift)?.let { (it / speed).toLong() }
                    delay(change?.coerceIn(CUE_MIN_MS, CUE_MAX_MS) ?: CUE_IDLE_MS)
                }
            }
    }

    /** The server's word that it stopped this playback's session (an administrator, or the profile elsewhere). */
    private suspend fun serverStops() {
        env.events.collect { event ->
            if (event is ServerEvent.SessionStopped && event.sessionId == session?.id) stopByServer(event.message, event.byAdmin)
        }
    }

    /**
     * The server ended the session: the engine stops, nothing more is asked
     * of the server (only `stopped`, for the resume point), and the screen
     * says why, without Try again, then leaves.
     */
    private fun stopByServer(message: String?, byAdmin: Boolean) {
        if (closed || stopped) return
        stopped = true
        val position = livePosition()
        lastPositionMs = position
        session?.let { s ->
            sessionState.value = null
            s.stoppedByServer(position)
        }
        sessionJob?.cancel()
        startJob?.cancel()
        seekReport?.cancel()
        nextLookup?.cancel()
        hideUpNext()
        hideServerSubtitles()
        sleep.cancel()
        link?.stop()
        ui.update {
            it.copy(
                phase = PlayerPhase.Error(texts.stopped(message, byAdmin), canRetry = false),
                skipLabel = null,
                stoppedByServer = true,
            )
        }
    }

    /** How long the item playing has left, for the sleep timer; `null` while unknown. */
    private fun itemLeftMs(): Long? {
        if (session == null || !mediaShown || ended) return null
        val duration = ui.value.durationMs.takeIf { it > 0 } ?: return null
        return duration - livePosition()
    }

    /** The sleep timer ended: it pauses, and what plays next doesn't start by itself. */
    private fun onSleep() {
        countdown?.cancel()
        countdown = null
        ui.update { it.copy(upNextSecondsLeft = null, upNextCard = it.upNextCard?.copy(secondsLeft = null)) }
        link?.pause()
    }

    private fun report(state: ProgressState) {
        session?.progress(livePosition(), state)
    }

    private fun livePosition(): Long {
        val engineLink = link
        return if (mediaShown && engineLink != null && session != null && !ended) engineLink.positionMs else lastPositionMs
    }

    // ---------------------------------------------------------------- the screen's commands

    override fun play() {
        if (closed || stopped) return
        if (ended) {
            restart(0)
            return
        }
        link?.play()
    }

    override fun pause() {
        if (closed) return
        link?.pause()
    }

    override fun seekTo(positionMs: Long) {
        val engineLink = link ?: return
        if (closed || stopped) return
        val duration = ui.value.durationMs
        val target = if (duration > 0) positionMs.coerceIn(0, (duration - SEEK_END_MARGIN_MS).coerceAtLeast(0)) else positionMs.coerceAtLeast(0)
        hideUpNext()
        if (ended || session == null) {
            lastPositionMs = target
            ui.update { it.copy(positionMs = target) }
            pictureSubtitles?.jumped()
            restart(target)
            return
        }
        engineLink.seekTo(target)
        lastPositionMs = target
        startedAtMs = target
        // Pictures the server is still copying out of the file are asked for from here at once.
        pictureSubtitles?.jumped()
        recovery.onJump()
        autoQuality.jumped()
        struggle.reset()
        quietAfterJump()
        lastAdvanceAt = env.now()
        ui.update { it.copy(positionMs = target) }
        seekReport?.cancel()
        seekReport = scope.launch {
            delay(SEEK_REPORT_MS)
            report(if (wantPlay.value) ProgressState.Playing else ProgressState.Paused)
        }
    }

    override fun selectAudio(index: Long) {
        val chosen = PlaybackPlan.audioStreams(file).firstOrNull { it.index.toLong() == index } ?: return
        if (chosen.index == audio?.index) return
        audio = chosen
        audioPicked = true
        saveStreams()
        publishChoices()
        // A download's other track may be one this device decodes: its speed can be tried again.
        if (playingLocal && ui.value.speedNote != null) ui.update { it.copy(speedNote = null) }
        val s = session ?: return
        val engineLink = link
        if (!s.info.hls && engineLink != null && PlaybackPlan.audioPlaysHere(chosen, caps)) {
            // The engine loads the other track: its wait is no play issue.
            autoQuality.jumped()
            engineLink.selectAudio(PlaybackPlan.audioStreams(file).indexOf(chosen), chosen.language)
        } else {
            restart(livePosition(), alongside = true)
        }
    }

    override fun selectSubtitle(index: Long?) {
        val chosen = index?.let { i -> PlaybackPlan.subtitleStreams(file).firstOrNull { it.index.toLong() == i } ?: return }
        if (chosen?.index == subtitle?.index) return
        subtitle = chosen
        saveStreams()
        publishChoices()
        val info = session?.info ?: return
        val burn = info.hls && PlaybackPlan.isImage(chosen) && !sideload
        if (burn != info.burnSubtitles || (burn && info.subtitleIndex != chosen?.index)) restart(livePosition(), alongside = true) else applyTracks()
    }

    override fun setSubtitleOffset(offsetMs: Long) {
        if (closed) return
        val moved = SubtitleSync.clamp(offsetMs)
        if (moved == offset.value) return
        val wasZero = offset.value == 0L
        offset.value = moved
        ui.update { it.copy(subtitleOffsetMs = moved) }
        file?.id?.let { id ->
            val key = offsetKey(id)
            scope.launch {
                try {
                    // One at a time, in the order chosen: a held key's steps don't land out of order.
                    offsetWrites.withLock { env.offsets.remember(key, moved) }
                } catch (e: CancellationException) {
                    throw e
                } catch (_: Exception) {
                    // Not remembered: this viewing keeps it all the same.
                }
            }
        }
        // A file's own text track moves through the server's WebVTT while an offset is set.
        if (wasZero != (moved == 0L)) applyTracks()
    }

    override fun setSpeed(speed: Float) {
        if (closed || stopped || ui.value.speedNote != null) return
        val wanted = PlaybackSpeeds.ALL.minBy { kotlin.math.abs(it - speed) }
        if (wanted == this.speed) return
        this.speed = wanted
        link?.setSpeed(wanted)
        ui.update { it.copy(speed = wanted) }
        if (wanted == 1f && serverDecodes) {
            // Back at 1×: the sound as the file has it, from a session made ready beside this one.
            serverDecodes = false
            soundNeeds(reload = false)
            restart(livePosition(), alongside = true)
        } else {
            soundNeeds(reload = true)
            // The media may load again where it is for its sound (decoded, or passed through again): that wait is no play issue.
            struggle.reset()
            autoQuality.jumped()
        }
    }

    override fun setAudioDelay(delayMs: Long) {
        if (closed || stopped) return
        val moved = AudioDelays.clamp(delayMs)
        if (moved == audioDelayMs) return
        audioDelayMs = moved
        link?.setAudioDelay(moved)
        soundNeeds(reload = true)
        // The picture jumps to the sound's new place: that wait or those dropped frames are no play issue.
        struggle.reset()
        autoQuality.jumped()
        quietAfterJump()
        ui.update { it.copy(audioDelayMs = moved) }
    }

    /** After a start, a seek or a new audio delay: dropped frames don't count while the picture catches up with a later sound. */
    private fun quietAfterJump() {
        struggleQuietUntil = if (audioDelayMs > 0) env.now() + audioDelayMs + QUIET_AFTER_DELAY_MS else 0L
    }

    /**
     * What the speed and the audio delay need of the sound, told to the
     * engine: decoded here for another speed than 1×, untunneled for any
     * speed or delay. [reload]: the media playing follows at once.
     */
    private fun soundNeeds(reload: Boolean) {
        link?.setSoundNeeds(decoded = speed != 1f, untunneled = speed != 1f || audioDelayMs != 0L, reload = reload)
    }

    /**
     * Another speed than 1×, while the sound goes to a receiver as it is
     * and nothing here decodes it ([EngineLink.soundUndecodable]): a stream
     * is asked for again with this device's decoders only, made ready
     * beside the one playing, so the server converts the sound and the
     * video stays as it is. A download has no server to ask: it plays at 1×,
     * as does a stream the server can't make, or whose sound it still sends
     * as it is (its file played as it is), and the speed says why.
     */
    private fun soundUndecodable() {
        if (speed == 1f) return
        when {
            playingLocal -> speedBack(if (env.drive) texts.driveSpeedUndecodable else texts.speedUndecodable)
            // Until the session asked for so plays, the one before still passes its sound through.
            serverDecodes -> if (playingConvertedSound) speedBack(texts.speedUnconverted)
            else -> {
                serverDecodes = true
                restart(livePosition(), alongside = true, refused = { speedBack(texts.speedUnconverted) })
            }
        }
    }

    /** The speed back at 1×, [note] saying why: the sound plays as it is, at its own speed. */
    private fun speedBack(note: String) {
        speed = 1f
        serverDecodes = false
        link?.setSpeed(1f)
        soundNeeds(reload = true)
        ui.update { it.copy(speed = 1f, speedNote = note) }
    }

    override fun searchSubtitles() {
        val detail = item ?: return
        val chosen = file ?: return
        if (closed || stopped || !ui.value.canFindSubtitles) return
        searchJob?.cancel()
        ui.update { it.copy(subtitleSearch = SubtitleSearchUi.Searching) }
        searchJob = scope.launch {
            val next = try {
                val found = env.api.searchSubtitles(detail.id, chosen.id, null)
                when {
                    !found.setup -> SubtitleSearchUi.Unavailable(texts.searchNotSetUp)
                    else -> {
                        val results = found.results.map(::foundSubtitle)
                        SubtitleSearchUi.Found(results, if (results.isEmpty()) texts.searchNothing else null)
                    }
                }
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                SubtitleSearchUi.Unavailable(searchFailure(e))
            }
            ui.update { if (it.subtitleSearch != null) it.copy(subtitleSearch = next) else it }
        }
    }

    override fun addFoundSubtitle(resultId: String) {
        val detail = item ?: return
        val chosen = file ?: return
        val shown = ui.value.subtitleSearch as? SubtitleSearchUi.Found ?: return
        if (closed || stopped) return
        searchJob?.cancel()
        ui.update { it.copy(subtitleSearch = SubtitleSearchUi.Adding(resultId, shown.results)) }
        searchJob = scope.launch {
            val track = try {
                env.api.addSubtitle(detail.id, AddSubtitle(resultId, chosen.id))
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                ui.update { if (it.subtitleSearch != null) it.copy(subtitleSearch = SubtitleSearchUi.Unavailable(searchFailure(e))) else it }
                return@launch
            }
            // An ordinary text track beside the media from now on: the menu lists it, and it shows as one.
            val stream = MediaStream(
                index = track.index,
                kind = StreamKind.Subtitle,
                codec = track.codec,
                language = track.language,
                title = track.title,
                default = track.default,
                forced = track.forced,
                textBased = true,
                external = true,
                added = true,
            )
            if (file?.id == chosen.id) {
                val withTrack = chosen.copy(streams = chosen.streams.filter { it.index != stream.index || it.kind != StreamKind.Subtitle } + stream)
                file = withTrack
                item = detail.copy(files = detail.files.map { if (it.id == withTrack.id) withTrack else it })
                ui.update { it.copy(subtitleSearch = null, notice = texts.subtitleAdded) }
                publishChoices()
                selectSubtitle(stream.index.toLong())
            } else {
                ui.update { it.copy(subtitleSearch = null) }
            }
        }
    }

    override fun closeSubtitleSearch() {
        searchJob?.cancel()
        searchJob = null
        ui.update { it.copy(subtitleSearch = null) }
    }

    /** A result as the list shows it: the release, then its language, whether it fits this very file, and downloads. */
    private fun foundSubtitle(r: SubtitleResult): FoundSubtitle = FoundSubtitle(
        id = r.id,
        label = r.name.ifBlank { languageName(r.language) ?: r.id },
        hint = texts.foundHint(languageName(r.language), r.matched == "hash", r.hearingImpaired, r.downloads).ifEmpty { null },
    )

    /** Why searching or adding failed, in a sentence: an older server has no search at all. */
    private fun searchFailure(e: Exception): String = when {
        e is ApiException && (e.status == 404 || e.status == 405) -> texts.searchUnsupported
        e is ApiException && e.status == 503 -> texts.searchNotSetUp
        else -> texts.searchFailed(describe(e))
    }

    override fun setSleep(choice: SleepChoice) {
        if (closed || stopped) return
        sleep.set(choice)
    }

    override fun keepWatching() {
        if (closed || stopped) return
        sleep.extend()
        // What plays next was held for the timer: its countdown goes on.
        val known = next
        if (known != null && ui.value.upNextCard != null && countdown?.isActive != true && !ended) showUpNext(known, restartCountdown = true)
    }

    override fun selectQuality(name: String) {
        val wanted = Qualities.byIdOrLabel(name)
        val choice = Qualities.forFile(file).firstOrNull { it.id == wanted.id } ?: return
        if (choice.id == picked.id) return
        val before = quality
        picked = choice
        streamingPicked = !choice.original && !choice.automatic
        // Automatic picked again starts afresh: what was known of the network is forgotten.
        if (choice.automatic) autoQuality.forgetCeiling()
        quality = playedFor(choice)
        autoNoticePending = false
        scope.launch { env.quality.remember(choice.id) }
        // Original and Automatic at the original play the same: it plays on, as it is.
        val same = quality.id == before.id
        if (same) {
            if (automatic && recovery.fallbackReason == FallbackReason.NotSmooth) {
                autoQuality.deviceFellBehindAtOriginal(env.now(), serverQuality(session?.info)?.maxBitrateKbps)
            }
            session?.info?.let(::syncAutoOriginal)
        } else {
            recovery.qualityChanged()
        }
        ui.update {
            it.copy(
                suggestLowerQuality = false,
                note = if (same) noteNow() else null,
                canPlayOriginal = same && canPlayOriginal(),
            )
        }
        publishChoices()
        refreshDetails()
        if (item != null && !same) restart(livePosition(), alongside = true)
    }

    override fun skip() {
        val marker = Markers.active(markers, livePosition()) ?: return
        val duration = ui.value.durationMs
        seekTo(if (duration > 0) minOf(Markers.skipTarget(marker), duration) else Markers.skipTarget(marker))
    }

    override fun playNext() {
        if (closed || stopped) return
        val known = next
        if (known != null) {
            switchTo(known)
            return
        }
        lookUpNext { found ->
            if (found == null) {
                notice(if (current.queue != null) texts.lastInQueue else texts.lastEpisode)
            } else {
                switchTo(found)
            }
        }
    }

    override fun cancelUpNext() {
        hideUpNext()
        if (ended) phase(PlayerPhase.Ended)
    }

    override fun playOriginal() {
        if (recovery.fallbackMode == null) return
        recovery.playOriginal()
        ui.update { it.copy(canPlayOriginal = false, note = null) }
        restart(livePosition(), alongside = true)
    }

    override fun setDetailsOpen(open: Boolean) {
        detailsJob?.cancel()
        detailsJob = null
        if (!open) {
            ui.update { it.copy(details = null) }
            return
        }
        detailsJob = scope.launch {
            while (true) {
                val s = session
                if (s != null && s.info.hls) {
                    live = try {
                        env.api.playbackStatus(s.id).transcode
                    } catch (e: CancellationException) {
                        throw e
                    } catch (_: IOException) {
                        live
                    }
                }
                refreshDetails(force = true)
                delay(DETAILS_MS)
            }
        }
    }

    private fun refreshDetails(force: Boolean = false) {
        if (!force && ui.value.details == null) return
        val info = session?.info ?: return
        val details = PlaybackDetails.build(
            texts = texts,
            info = info,
            file = file,
            audioIndex = audio?.index,
            note = noteNow(),
            positionMs = livePosition(),
            live = live,
            engine = link?.stats().orEmpty(),
            subtitle = subtitle,
        ).let { if (info.url.startsWith("file:") && !env.drive) it.copy(method = texts.downloaded) else it }
        ui.update { it.copy(details = details) }
    }

    override fun dismissNotice() = ui.update { it.copy(notice = null) }

    override suspend fun thumbnail(positionMs: Long): Bitmap? = trickplay?.thumbnail(positionMs)

    override fun saveState(): SavedPlaybackState = SavedPlaybackState(
        itemId = item?.id ?: current.itemId,
        fileId = file?.id ?: current.fileId,
        positionMs = livePosition(),
        // Only a picked track: the server chooses again otherwise.
        audioIndex = audio?.takeIf { audioPicked }?.index?.toLong(),
        subtitleIndex = subtitle?.index?.toLong(),
        quality = picked.id,
        queue = current.queue,
        drivePath = current.drivePath,
    )

    override fun retry() {
        if (closed || stopped) return
        recovery.retry()
        ui.update { it.copy(phase = PlayerPhase.Preparing) }
        if (item == null) {
            startJob?.cancel()
            startJob = scope.launch { prepareFirst() }
        } else {
            restart(lastPositionMs)
        }
    }

    override fun close() {
        if (closed) return
        val position = livePosition()
        closed = true
        endSession(position)
        sleep.cancel()
        hideServerSubtitles()
        dropTrickplay()
        link?.setListener(null)
        link?.release()
        link = null
        scope.cancel()
        onClosed()
    }

    // ---------------------------------------------------------------- Up Next

    private fun onCredits() {
        val detail = item ?: return
        if (!nextUp.hasNext(detail.kind, current.queue) || ui.value.upNextCard != null) return
        lookUpNext { found -> if (found != null && !closed) showUpNext(found, restartCountdown = true) }
    }

    /** Finds what plays next (once at a time), then calls [then] with it. */
    private fun lookUpNext(then: (ItemSummary?) -> Unit) {
        val detail = item ?: return then(null)
        // Asked at the end too, when the session has ended already: by the file.
        val fromDownload = !streamingPicked && file?.let { localFile(it.id) } != null
        nextLookup?.cancel()
        nextLookup = scope.launch {
            val found = try {
                nextUp.after(detail.id, detail.kind, current.queue, fromDownload)
            } catch (e: CancellationException) {
                throw e
            } catch (_: Exception) {
                null
            }
            if (found != null) next = found
            then(found)
        }
    }

    private fun showUpNext(found: ItemSummary, restartCountdown: Boolean) {
        next = found
        // The next item's details and still, ahead of time.
        if (nextDetail?.first != found.id) {
            val choices = choicesSaved
            scope.launch {
                try {
                    val detail = env.details.detail(found.id)
                    // Tracks picked meanwhile change what it starts with: asked again then.
                    if (choices == choicesSaved) nextDetail = found.id to detail
                } catch (e: CancellationException) {
                    throw e
                } catch (_: Exception) {
                    // Asked for again when it plays.
                }
            }
        }
        val card = UpNextUi(
            itemId = found.id,
            title = found.title,
            heading = texts.upNext(NextUp.labelOf(found)),
            // A download's picture is a file here.
            imageUrl = (found.thumbUrl ?: found.backdropUrl)?.let { if (it.startsWith("file:")) it else env.api.url(it, UP_NEXT_IMAGE_WIDTH) },
            secondsLeft = null,
        )
        ui.update { it.copy(upNextCard = card, upNext = card.title, upNextSecondsLeft = null) }
        if (!restartCountdown) return
        countdown?.cancel()
        countdown = scope.launch {
            if (!env.autoplayNext() || sleep.endsWith(item?.id)) return@launch
            for (left in COUNTDOWN_SECONDS downTo 1) {
                setSecondsLeft(left)
                delay(1_000)
            }
            setSecondsLeft(0)
            switchTo(found)
        }
    }

    private fun setSecondsLeft(left: Int) = ui.update {
        it.copy(upNextSecondsLeft = left, upNextCard = it.upNextCard?.copy(secondsLeft = left))
    }

    private fun hideUpNext() {
        countdown?.cancel()
        countdown = null
        ui.update { it.copy(upNextCard = null, upNext = null, upNextSecondsLeft = null) }
    }

    /** Plays [target] on the same engine: the session before ends, the next one starts where [target] resumes. */
    private fun switchTo(target: ItemSummary) {
        hideUpNext()
        nextLookup?.cancel()
        val position = livePosition()
        val endedBefore = endSession(position)
        next = null
        recovery.newItem()
        autoNoticePending = false
        // Speed and audio delay are this title's: the next one plays as it is.
        speed = 1f
        audioDelayMs = 0
        serverDecodes = false
        link?.setSpeed(1f)
        link?.setAudioDelay(0)
        // The media playing goes: the next one's sound goes out as it would.
        soundNeeds(reload = false)
        searchJob?.cancel()
        credits.reset()
        guardMode = null
        hideServerSubtitles()
        dropTrickplay()
        ended = false
        mediaShown = false
        val req = current.copy(
            itemId = target.id,
            fileId = null,
            startMs = NextUp.startOf(target),
            fromStart = false,
            audioIndex = null,
            subtitleIndex = null,
        )
        current = req
        ui.update {
            PlayerUiState(
                phase = PlayerPhase.Preparing,
                itemId = target.id,
                title = it.title,
                positionMs = req.startMs ?: 0,
                sleep = it.sleep,
            )
        }
        sessionJob?.cancel()
        startJob?.cancel()
        sessionJob = scope.launch {
            try {
                val lookup = networkLookup()
                if (endedBefore) withTimeoutOrNull(END_WAIT_MS) { env.reporter.flush() }
                val prefetched = nextDetail?.takeIf { it.first == target.id }?.second
                val detail = prefetched ?: detailOf(target.id)
                load(detail, req, lookup)
                begin(req.startMs ?: 0)
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                failWith(e)
            }
        }
    }

    // ---------------------------------------------------------------- state

    /** The menus: tracks and qualities, as chosen now. */
    private fun publishChoices() {
        val chosenFile = file
        val streaming = session?.info?.hls ?: false
        val info = session?.info
        val audioChoices = PlaybackPlan.audioStreams(chosenFile).map { TrackChoice(it.index.toLong(), streamLabel(it), it.index == audio?.index) }
        val subs = PlaybackPlan.subtitleStreams(chosenFile)
        val subtitleChoices = if (subs.isEmpty()) {
            emptyList()
        } else {
            listOf(TrackChoice(null, texts.subtitlesOff, subtitle == null)) +
                subs.map { TrackChoice(it.index.toLong(), streamLabel(it), it.index == subtitle?.index, burnIn = streaming && PlaybackPlan.isImage(it) && !sideload) }
        }
        val sub = subtitle
        // Text moves always; pictures only when drawn here from the server's .mks. On a drive, the
        // files beside it only: the server's WebVTT moves a file's own tracks, and a drive has no server.
        val syncable = sub != null &&
            (if (env.drive) sub.external else !PlaybackPlan.isImage(sub) || (streaming && sideload && info?.burnSubtitles != true))
        val auto = serverQuality(info)
        val playing = label(auto ?: quality)
        val fileHint = chosenFile?.let { f ->
            listOfNotNull(resolutionLabel(f.width, f.height), formatBitrate(f.bitrate).ifEmpty { null }).joinToString(" · ").ifEmpty { null }
        }
        // A download plays as it is: there is nothing to convert, and the server may be away.
        val downloaded = info?.url?.startsWith("file:") == true
        val qualities = if (downloaded) emptyList() else Qualities.forFile(chosenFile).map { preset ->
            QualityChoice(
                id = preset.id,
                label = label(preset),
                hint = when {
                    preset.automatic -> if (automatic && (auto != null || !quality.original)) texts.automaticNow(playing) else texts.automaticHint
                    preset.original -> fileHint
                    else -> null
                },
                selected = preset.id == picked.id,
            )
        }
        ui.update {
            it.copy(
                audio = audioChoices,
                subtitles = subtitleChoices,
                qualities = qualities,
                quality = if (automatic) texts.automatic(playing) else label(quality),
                subtitleSync = syncable,
                subtitleOffsetMs = offset.value,
            )
        }
    }

    /**
     * PUT /api/items/{id}/streams: the next episode, or the next viewing,
     * starts with the same tracks. The audio only when picked: left out,
     * the audio chosen before stays, as subtitles alone were picked.
     */
    private fun saveStreams() {
        // A file on a drive keeps its tracks for this viewing: there is no server to save them.
        if (env.drive) return
        val itemId = item?.id ?: return
        val choice = StreamChoice(fileId = file?.id, audioIndex = audio?.takeIf { audioPicked }?.index, subtitleIndex = subtitle?.index)
        env.reporter.send { env.api.setStreams(itemId, choice) }
        env.tracksChosen(itemId, choice)
        // The next episode's details, asked for before, name the tracks it
        // started with then: it is asked for again, after this choice is saved.
        choicesSaved++
        nextDetail = null
    }

    private fun dropTrickplay() {
        trickplayWatch?.cancel()
        trickplayWatch = null
        trickplay?.close()
        trickplay = null
    }

    private fun phase(phase: PlayerPhase) {
        ui.update { if (it.phase is PlayerPhase.Error) it else it.copy(phase = phase) }
    }

    private fun notice(text: String) = ui.update { it.copy(notice = text) }

    private fun fail(message: String, canRetry: Boolean, offline: Boolean = false) {
        ui.update { it.copy(phase = PlayerPhase.Error(message, canRetry, offline), skipLabel = null, handoff = false) }
    }

    private fun failWith(error: Exception) {
        // A file on a drive asks nothing of a server: what fails is reading it.
        if (env.drive) return fail(texts.driveUnreadable, canRetry = true)
        val downloaded = file?.let { localFile(it.id) } != null
        if (error is OfflineException && downloaded && recovery.fallbackReason == FallbackReason.NotSmooth) {
            // A download that dropped frames went to the server for a smoother copy, which is out
            // of reach: it plays on as it is (it doesn't switch twice).
            recovery.playOriginal()
            ui.update { it.copy(canPlayOriginal = false, note = null) }
            restart(lastPositionMs)
            return
        }
        // A download that didn't play here as it is went to the server, which is out of reach.
        val leftDownload = recovery.fallbackMode != null && downloaded
        if (error is OfflineException && !leftDownload) {
            return fail(if (error.blocked) describe(error) else texts.unreachable(describe(error)), canRetry = true, offline = true)
        }
        val message = if (leftDownload && error is OfflineException) texts.downloadNeedsServer else describe(error)
        val canRetry = !(error is ApiException && (error.status == 403 || error.status == 404 || error.signedOut))
        fail(message, canRetry)
    }

    companion object {
        const val TICK_MS = 500L
        const val PROGRESS_MS = 10_000L

        /** While paused: the server forgets players it hasn't heard from for 90 s. */
        const val PAUSED_HEARTBEAT_MS = 30_000L
        const val SEEK_REPORT_MS = 1_000L

        /** How long playback may stand still, wanting to play, before it counts as stalled. */
        const val STALL_MS = 12_000L

        /** An end at least this far before the server's duration… */
        const val SHORT_END_MS = 60_000L

        /** …that comes less than this after where playback started is media that falls short, not the end. */
        const val SHORT_PLAY_MS = 5_000L

        /** Less than this buffered ahead while waiting: starving for data. */
        const val STARVING_MS = 1_000L
        const val STRUGGLE_SAMPLE_MS = 2_000L
        const val DETAILS_MS = 2_000L
        const val COUNTDOWN_SECONDS = 10

        /** The longest a new session waits for the one before to be ended. */
        const val END_WAIT_MS = 2_000L

        /** How long a session made ready beside the one playing may take before the player moves to it anyway. */
        const val PREPARE_MAX_MS = 20_000L

        /** How far past where it starts that session's first segment is asked for: where the player will be by then. */
        const val PREPARE_LEAD_MS = 4_000L

        /** The server's answer when no transcode is free for two sessions at once. */
        private const val NO_TRANSCODE_FREE = 503

        /** How long a download waits for the server's details before starting from its own. */
        const val SERVER_DETAIL_WAIT_MS = 1_500L

        /** Seeks stop this short of the end, so the end isn't reached by a seek. */
        const val SEEK_END_MARGIN_MS = 250L
        const val UP_NEXT_IMAGE_WIDTH = 480
        private const val CUE_MIN_MS = 20L
        private const val CUE_MAX_MS = 250L
        private const val CUE_IDLE_MS = 500L

        /** Beyond a later sound's own delay, how long dropped frames don't count after a jump. */
        const val QUIET_AFTER_DELAY_MS = 3_000L
    }
}

/**
 * Whether a subtitle track's [path] is a file saved on this device, which
 * only a download's own session names (its media's [sessionUrl] is a file
 * too): a server naming one can't have the app read its files.
 */
internal fun isSavedSubtitle(path: String, sessionUrl: String): Boolean = path.startsWith("file:") && sessionUrl.startsWith("file:")
