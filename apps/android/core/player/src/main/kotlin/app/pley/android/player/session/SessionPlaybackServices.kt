package app.pley.android.player.session

import android.content.Context
import android.graphics.Bitmap
import androidx.media3.common.text.Cue
import app.pley.android.caps.CapsGraph
import app.pley.android.common.AppLanguage
import app.pley.android.common.DeviceProfile
import app.pley.android.common.Tier
import app.pley.android.common.UiText
import app.pley.android.common.UserFacing
import app.pley.android.data.DataGraph
import app.pley.android.data.LocalMedia
import app.pley.android.data.drive.Drives
import app.pley.android.network.HttpPleyApi
import app.pley.android.network.NetworkGraph
import app.pley.android.network.OfflineException
import app.pley.android.player.PlayRequest
import app.pley.android.player.PlaybackController
import app.pley.android.player.PlaybackServices
import app.pley.android.player.PlayerEngine
import app.pley.android.player.PlayerPhase
import app.pley.android.player.PlayerUiState
import app.pley.android.player.R
import app.pley.android.player.engine.PlayTrace
import app.pley.android.player.subtitles.OkHttpPictureSource
import app.pley.android.player.subtitles.OkHttpSubtitleFetcher
import app.pley.android.player.subtitles.PreferencesSubtitleOffsets
import app.pley.android.player.trickplay.OkHttpSheetSource
import java.io.File
import java.io.IOException
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.first

/**
 * [PlaybackServices] with [SessionPlaybackController]s: [prepare] starts the
 * playback at once (the server's answer, the details and the decoders get
 * ready while the player screen opens). One video plays at a time: preparing
 * another closes the one before, whose session ends. A video on a drive
 * ([PlayRequest.drivePath]) plays in an environment of its own, with no
 * server, signed in or not. Main thread only.
 */
class SessionPlaybackServices(
    /** The environment of the server signed in to now; throws when there is none. */
    private val environment: () -> PlaybackEnvironment,
    private val dispatcher: CoroutineDispatcher = Dispatchers.Main.immediate,
    /** The environment of the video on a drive at a path. */
    private val drive: (path: String) -> PlaybackEnvironment = { throw IOException("No drives here") },
    /** Where the playbacks' reports queue: [flush] waits for it, [drop] empties it. */
    private val reporter: SessionReporter? = null,
) : PlaybackServices {
    private val controllers = LinkedHashMap<String, PlaybackController>()
    private var counter = 0

    override fun prepare(request: PlayRequest): String {
        // Play was pressed: the section ends at the first frame.
        PlayTrace.tapped()
        controllers.values.toList().forEach { it.close() }
        val key = "play-${++counter}-${request.itemId}"
        val env = try {
            request.drivePath?.let(drive) ?: environment()
        } catch (e: CancellationException) {
            throw e
        } catch (e: Exception) {
            controllers[key] = FailedPlayback(e.message ?: e.javaClass.simpleName) { controllers.remove(key) }
            return key
        }
        val controller = SessionPlaybackController(request, env, CoroutineScope(SupervisorJob() + dispatcher)) { controllers.remove(key) }
        controllers[key] = controller
        controller.start()
        return key
    }

    override fun controller(key: String): PlaybackController? = controllers[key]

    override fun closeAll() {
        controllers.values.toList().forEach { it.close() }
    }

    override suspend fun flush() {
        reporter?.flush()
    }

    override fun drop() {
        reporter?.drop()
    }

    companion object {
        /**
         * The services of the app: the server and profile signed in to now
         * ([DataGraph.session]), the engines of [engines], the capability
         * probe of [caps], reports from the app's own scope, and the
         * downloads of [local] (made when a playback first asks), and
         * videos on the drives of [drives].
         */
        fun create(
            context: Context,
            data: DataGraph,
            network: NetworkGraph,
            caps: CapsGraph,
            profile: DeviceProfile,
            engines: PlayerEngine.Factory,
            local: () -> LocalMedia = { LocalMedia.None },
            drives: () -> Drives = { Drives.None },
        ): SessionPlaybackServices {
            val app = context.applicationContext
            val reporter = SessionReporter(CoroutineScope(SupervisorJob() + Dispatchers.Default))
            val texts = ResourcePlayerTexts(app)
            val clientId = clientIdOf(app)
            val networkKind = networkKindOf(app)
            val offsets = PreferencesSubtitleOffsets(app)
            val ceilings = PreferencesCeilings(app)
            val pictureDir = File(app.cacheDir, PICTURE_SUBTITLES_DIR)
            // No server is behind a video on a drive: a call to one fails as if it were out of reach.
            val noServer = HttpPleyApi(NO_SERVER, { throw OfflineException(IOException("A video on a drive has no server")) })
            val drive: (String) -> PlaybackEnvironment = { path ->
                val video = DriveVideo(path, drives())
                PlaybackEnvironment(
                    api = noServer,
                    capabilities = caps.capabilities,
                    policy = caps.policy,
                    details = ItemDetails { video.detail(it) },
                    engines = { Media3EngineLink(engines.create(app, profile, music = false), texts) },
                    texts = texts,
                    clientId = clientId,
                    reporter = reporter,
                    subtitles = DriveSubtitleFetcher(),
                    lowMemory = profile.tier == Tier.T0,
                    offsets = offsets,
                    local = video,
                    drive = true,
                )
            }
            return SessionPlaybackServices(environment = {
                val session = data.session.session.value ?: throw NotSignedIn()
                val api = network.api(session.issuer, session.me?.id)
                PlaybackEnvironment(
                    api = api,
                    capabilities = caps.capabilities,
                    policy = caps.policy,
                    details = ItemDetails.snapshotFirst(data.items, api),
                    engines = { Media3EngineLink(engines.create(app, profile, music = false), texts) },
                    texts = texts,
                    clientId = clientId,
                    reporter = reporter,
                    subtitles = OkHttpSubtitleFetcher({ network.subtitles(session.issuer) }),
                    quality = QualityMemory.of(data.settings, networkKind),
                    ceiling = ceilings.forServer(session.issuer),
                    sheets = OkHttpSheetSource({ network.images }),
                    autoplayNext = {
                        try {
                            data.settings.settings.first().autoplayNext
                        } catch (e: CancellationException) {
                            throw e
                        } catch (_: Exception) {
                            true
                        }
                    },
                    lowMemory = profile.tier == Tier.T0,
                    offsets = offsets,
                    profileId = session.me?.id,
                    pictures = OkHttpPictureSource({ network.subtitles(session.issuer) }, pictureDir),
                    events = data.liveUpdates.events,
                    playing = data.liveUpdates::playing,
                    here = data.remote::here,
                    local = local(),
                    tracksChosen = { itemId, c -> data.watchState.streamsChosen(itemId, c.fileId, c.audioIndex, c.subtitleIndex) },
                    warmer = OkHttpStreamWarmer(network::media),
                )
            }, drive = drive, reporter = reporter)
        }

        /** Picture subtitles read beside a stream, in the app's cache while they show. */
        private const val PICTURE_SUBTITLES_DIR = "picture-subtitles"

        /** The address a video on a drive's API answers for: none it reaches. */
        private const val NO_SERVER = "http://localhost"
    }
}

/** Play was pressed with no server signed in to. Its message is the words of [text] in the language shown. */
private class NotSignedIn : IllegalStateException(), UserFacing {
    override val text: UiText = UiText.res(R.string.session_sign_in_to_play)
    override val message: String get() = AppLanguage.wordsOf(text)
}

/** A playback that couldn't start at all (nobody signed in): it says why, and nothing else. */
private class FailedPlayback(message: String, private val onClosed: () -> Unit) : PlaybackController {
    override val state: StateFlow<PlayerUiState> = MutableStateFlow(PlayerUiState(phase = PlayerPhase.Error(message, canRetry = false)))
    override val engine: PlayerEngine? = null
    override val cues: StateFlow<List<Cue>> = MutableStateFlow(emptyList())

    override fun play() = Unit

    override fun pause() = Unit

    override fun seekTo(positionMs: Long) = Unit

    override fun selectAudio(index: Long) = Unit

    override fun selectSubtitle(index: Long?) = Unit

    override fun selectQuality(name: String) = Unit

    override fun skip() = Unit

    override fun playNext() = Unit

    override fun cancelUpNext() = Unit

    override fun playOriginal() = Unit

    override fun setDetailsOpen(open: Boolean) = Unit

    override fun dismissNotice() = Unit

    override fun dismissRemoteNotice() = Unit

    override suspend fun thumbnail(positionMs: Long): Bitmap? = null

    override fun saveState(): SavedPlaybackState = SavedPlaybackState(0, null, 0, null, null, QualityPreset.ORIGINAL_ID, null)

    override fun retry() = Unit

    override fun close() = onClosed()
}
