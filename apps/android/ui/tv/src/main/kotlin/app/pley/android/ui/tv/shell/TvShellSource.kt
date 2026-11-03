package app.pley.android.ui.tv.shell

import androidx.compose.runtime.Immutable
import androidx.compose.ui.graphics.Color
import app.pley.android.auth.AuthEvent
import app.pley.android.data.RequestNotice
import app.pley.android.data.RequestsRepository
import app.pley.android.data.SessionState
import app.pley.android.data.valueOrNull
import app.pley.android.designsystem.image.ArtworkImage
import app.pley.android.model.Library
import app.pley.android.screens.ScreensGraph
import app.pley.android.screens.requests.RequestsEntry
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.async
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.emitAll
import kotlinx.coroutines.flow.emptyFlow
import kotlinx.coroutines.flow.filterIsInstance
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.flowOn
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.merge
import kotlinx.coroutines.withContext

/**
 * What the TV's frame reads: how to start, what the rail lists, and who
 * is watching. The app's data in the app ([of]), fakes in tests.
 */
interface TvShellSource {
    /** The start gate's facts, from this device only (a disk read, no network). */
    suspend fun startFacts(): StartFacts

    /** [startFacts] if they are known already, so the first frame can be the first screen; `null` until then. */
    fun startFactsNow(): StartFacts? = null

    /** The libraries this profile sees, in the server's order; empty while unknown. */
    val libraries: Flow<List<Library>>

    /** Who is watching, for the rail's avatar; `null` while unknown. */
    val profile: Flow<RailProfile?>

    /**
     * Keeps what screens show in step with the server (server events) for
     * as long as it runs: the shell runs it while the app is started.
     */
    suspend fun keepUpdated()

    /**
     * The servers the app is signed out of while in use: a refused sign-in,
     * a sign-out, tokens this device can't read anymore (found before the
     * shell listened too). Sign-in shows again.
     */
    val signedOut: Flow<String> get() = emptyFlow()

    /** pley's own "Reduce motion" switch (the system's setting counts as well). */
    val reduceMotion: Flow<Boolean> get() = flowOf(false)

    /** Music has a queue, playing or paused: the rail leads to Now Playing. */
    val music: Flow<Boolean> get() = flowOf(false)

    /** Live TV is open to this profile and has channels: the rail leads to it. */
    val liveTv: Flow<Boolean> get() = flowOf(false)

    /** A USB drive is plugged in: the rail leads to its videos. */
    val drive: Flow<Boolean> get() = flowOf(false)

    /** Each time the server's events connect: it answers again (the offline mark goes). */
    val serverAnswered: Flow<Unit> get() = emptyFlow()

    /** The rail's way into requests (Discover, or the requests) and its number; none while unknown. */
    val requests: Flow<RequestsEntry> get() = flowOf(RequestsEntry.None)

    /**
     * News of this profile's requests to pop up, oldest first: on a TV, what
     * became ready to watch, each with its title's poster.
     */
    val notices: Flow<List<TvNotice>> get() = flowOf(emptyList())

    /** [notice] is on screen: off the queue, so newer news can't push it out. */
    fun dismiss(notice: RequestNotice) = Unit

    companion object {
        /**
         * The app's: the startup read's server and tokens ([startup]),
         * "Always start as" from the settings, and the household's size as
         * the data module last saw it ([householdSize]; 1 until it says).
         * The three are read at once, in the background, as soon as it is
         * made: made before the activity's first frame, it usually knows
         * them by then. With "Always start as" in a household of several,
         * the session's saved profile says whether it has a PIN too (from
         * the disk). Making it touches nothing else.
         */
        fun of(
            graph: ScreensGraph,
            startup: suspend () -> Pair<String?, Boolean>,
            householdSize: suspend () -> Int = { 1 },
        ): TvShellSource = GraphShellSource(graph, startup, householdSize)
    }
}

/** News of a request to pop up: [notice], and its title's poster on the server. */
@Immutable
data class TvNotice(val notice: RequestNotice, val poster: ArtworkImage? = null)

/** The rail's avatar: the profile's name, colour and picture. */
@Immutable
data class RailProfile(
    val name: String,
    /** The circle behind the initial; `null`: pley's default. */
    val color: Color? = null,
    val picture: ArtworkImage? = null,
) {
    val initial: String get() = name.trim().firstOrNull()?.uppercase() ?: "?"

    companion object {
        /** "#e07a5f" as a colour; `null` when it isn't one. */
        fun parseColor(hex: String?): Color? {
            val digits = hex?.removePrefix("#") ?: return null
            if (digits.length != 6) return null
            val rgb = digits.toLongOrNull(16) ?: return null
            return Color(0xFF000000 or rgb)
        }
    }
}

private class GraphShellSource(
    private val graph: ScreensGraph,
    private val startup: suspend () -> Pair<String?, Boolean>,
    private val householdSize: suspend () -> Int,
) : TvShellSource {
    @Volatile
    private var known: StartFacts? = null

    private val facts = CoroutineScope(SupervisorJob() + Dispatchers.Default).async {
        coroutineScope {
            val start = async { startup() }
            val settings = async { graph.data.settings.settings.first() }
            val profiles = async { householdSize() }
            val (server, signedIn) = start.await()
            val size = profiles.await()
            val always = settings.await().alwaysStartAs
            val hasPin = if (signedIn && size > 1 && always != null) leftOnPin() else null
            StartFacts(server, signedIn, profiles = size, alwaysStartAs = always, hasPin = hasPin)
        }.also { known = it }
    }

    /** Whether the profile the app was left on has a PIN, from its saved GET /api/auth/me; `null` when none was saved. */
    private suspend fun leftOnPin(): Boolean? {
        val state = graph.data.session.state.first { it !is SessionState.Starting }
        return (state as? SessionState.SignedIn)?.session?.me?.hasPin
    }

    override suspend fun startFacts(): StartFacts = facts.await()

    override fun startFactsNow(): StartFacts? = known

    // The graph's parts are made off the main thread (the app warms them as it
    // starts): each flow reaches them only once collected, and on Default.

    override val libraries: Flow<List<Library>> =
        offMain { graph.data.libraries.libraries() }.map { it.valueOrNull.orEmpty() }.distinctUntilChanged()

    override val profile: Flow<RailProfile?> = offMain { graph.data.session.session }.map { session ->
        val me = session?.me ?: return@map null
        RailProfile(
            name = me.name,
            color = RailProfile.parseColor(me.avatarColor),
            picture = ArtworkImage.of(me.avatarUrl, session.issuer),
        )
    }.distinctUntilChanged()

    override suspend fun keepUpdated() {
        // A drive plugged in or pulled out while the app was away: some TV builds don't say.
        withContext(Dispatchers.Default) { graph.drives.refresh() }
        // Server events flow while something listens to them (the data module's LiveUpdates).
        offMain { graph.data.liveUpdates.scans }.collect { }
    }

    // The session says so too, for a sign-out before anything listened to the events.
    override val signedOut: Flow<String> = merge(
        offMain { graph.auth.events }.filterIsInstance<AuthEvent.SignedOut>().map { it.issuer },
        offMain { graph.data.session.state }.filterIsInstance<SessionState.SignedOut>().map { it.issuer },
    )

    override val reduceMotion: Flow<Boolean> = offMain { graph.data.settings.settings }.map { it.reduceMotion }.distinctUntilChanged()

    override val music: Flow<Boolean> = offMain { graph.player.music.nowPlaying }.map { it != null }.distinctUntilChanged()

    override val liveTv: Flow<Boolean> = offMain { graph.data.liveTv.summary() }.map { it.valueOrNull?.available == true }.distinctUntilChanged()

    override val drive: Flow<Boolean> = offMain { graph.drives.drive }.map { it != null }.distinctUntilChanged()

    override val serverAnswered: Flow<Unit> = offMain { graph.data.liveUpdates.answered }

    // Requests are followed from just after the first frame: reading them starts their events, catching up and polling.
    override val requests: Flow<RequestsEntry> = offMain { started().state }.map(RequestsEntry::of).distinctUntilChanged()

    override val notices: Flow<List<TvNotice>> = combine(offMain { started().notices }, offMain { graph.data.servers.active }) { notices, origin ->
        notices.map { notice ->
            val poster = (notice as? RequestNotice.Ready)?.posterUrl
            TvNotice(notice, origin?.let { ArtworkImage.of(poster, it) })
        }
    }.flowOn(Dispatchers.Default)

    // Made by then: the notice was read from it.
    override fun dismiss(notice: RequestNotice) = graph.data.requests.dismiss(notice)

    private fun started(): RequestsRepository = graph.data.requests.also { it.start() }

    /** [upstream], made and collected on Default: the graph is never made on the main thread for it. */
    private fun <T> offMain(upstream: () -> Flow<T>): Flow<T> = flow { emitAll(upstream()) }.flowOn(Dispatchers.Default)
}
