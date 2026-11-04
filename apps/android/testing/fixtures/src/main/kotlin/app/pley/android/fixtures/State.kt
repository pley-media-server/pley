package app.pley.android.fixtures

import app.pley.android.model.Control
import app.pley.android.model.PlaybackMethod
import app.pley.android.model.ProgressState
import java.util.concurrent.ConcurrentHashMap
import mockwebserver3.MockResponse

/** What a route needs of the caller's token (docs/API.md, "Scopes"). */
internal sealed interface Need {
    /** Nothing: no token. */
    data object Public : Need

    /** Any app's token. */
    data object AnyApp : Need

    data class Scope(val name: String) : Need
}

/** Who asks: the profile the token acts as and what it was allowed. */
internal class Caller(
    val profile: Profile,
    val scopes: Set<String>,
    /** The sign-in the token belongs to; `null` without one (requireAuth off). */
    val grant: OAuth.Grant?,
    /** When the access token ends, in ms; `null` without one. */
    val expiresAtMs: Long?,
) {
    val scope: String get() = OAuth.SCOPES.filter { it in scopes }.joinToString(" ")
}

/**
 * One route: [method] and a path whose `*` parts are its values
 * ("/api/items/ * /children", without the spaces).
 */
internal class Route(
    val method: String,
    path: String,
    val need: Need,
    /** Reads only what never changes (the catalogue, the media), so runs without the lock. */
    val free: Boolean = false,
    val handle: (call: Call, caller: Caller?, values: List<String>) -> MockResponse,
) {
    val pattern: List<String> = path.split('/').filter { it.isNotEmpty() }
}

/** A route for apps: always called with a caller. */
internal fun route(
    method: String,
    path: String,
    need: Need,
    free: Boolean = false,
    handle: (Call, Caller, List<String>) -> MockResponse,
) = Route(method, path, need, free) { call, caller, values -> handle(call, caller!!, values) }

/** A route without a token. */
internal fun public(method: String, path: String, handle: (Call, List<String>) -> MockResponse) =
    Route(method, path, Need.Public) { call, _, values -> handle(call, values) }

internal val LIBRARY = Need.Scope("library")
internal val PLAY = Need.Scope("play")
internal val PLAYLISTS = Need.Scope("playlists")

/**
 * A playback session: the file it plays and how, what its player obeys from
 * the remote and calls its device, and where its last report had it.
 */
internal class PlaySession(
    val id: String,
    val profileId: Long,
    val itemId: Long,
    val fileId: Long,
    val method: PlaybackMethod,
    val clientId: String?,
    val controls: List<Control> = emptyList(),
    val device: String? = null,
    @Volatile var positionMs: Long = 0,
    @Volatile var state: ProgressState = ProgressState.Playing,
    /** When it was last heard from, on the server's clock (ms). */
    @Volatile var reportedAtMs: Long = 0,
)

/** A session the server stopped, and why. */
internal class StoppedSession(val profileId: Long, val message: String?)

/** The committed media (scripts/make-android-fixtures.ps1), read from the classpath once. */
internal object Media {
    private const val ROOT = "app/pley/android/fixtures/media/"
    private val cache = ConcurrentHashMap<String, ByteArray>()

    /** The bytes of [name] ("sample-1080p.mp4", "hls/0.ts"), or `null` when there's no such file. */
    fun bytes(name: String): ByteArray? {
        cache[name]?.let { return it }
        val stream = Media::class.java.classLoader?.getResourceAsStream(ROOT + name) ?: return null
        val bytes = stream.use { it.readBytes() }
        cache[name] = bytes
        return bytes
    }

    fun sizes(): Map<Clip, Long> = Clip.entries.associateWith { clip ->
        (bytes(clip.resource) ?: error("the fixture media aren't on the classpath: ${clip.resource}")).size.toLong()
    }
}

/** Everything the FixtureServer knows, behind one lock. */
internal class State(val options: FixtureServer.Options) {
    val lock = Any()
    val catalog = Catalog(Media.sizes())
    val household = Household(catalog)
    val views = Views(catalog)
    val journal = Journal()
    val pictures = Pictures()
    val oauth = OAuth(this)
    val events = EventHub(this)
    val sessions = LinkedHashMap<String, PlaySession>()

    /** Sessions stopped by DELETE /api/sessions/{id}: their streams answer 410 with the message. */
    val stoppedSessions = HashMap<String, StoppedSession>()

    /** How often each subtitle track was asked for, by "file:index". */
    val subtitleAsks = HashMap<String, Int>()

    /** Automatic's network memory (ExtraRoutes). */
    val network = NetworkMemory()

    /** Subtitle search is set up. */
    @Volatile
    var subtitleSearch = true

    /** Subtitles added to files (POST /api/items/{id}/subtitles), by file. */
    val addedSubtitles = HashMap<Long, MutableList<app.pley.android.model.SubtitleTrack>>()

    fun nowMs(): Long = options.clock()

    /** Unix seconds. */
    fun now(): Long = options.clock() / 1_000

    fun profile(id: Long): Profile? = household.profiles[id]
}
