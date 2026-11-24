package app.pley.android.player.music.library

import app.pley.android.data.DataGraph
import app.pley.android.data.Loadable
import app.pley.android.data.SessionState
import app.pley.android.model.ItemSummary
import app.pley.android.model.Library
import app.pley.android.model.LibraryKind
import app.pley.android.model.LibrarySort
import app.pley.android.model.PlaylistDetail
import app.pley.android.model.PlaylistSummary
import app.pley.android.network.ApiException
import app.pley.android.network.NetworkGraph
import app.pley.android.network.PleyApi
import java.util.concurrent.ConcurrentLinkedQueue
import kotlin.coroutines.AbstractCoroutineContextElement
import kotlin.coroutines.CoroutineContext
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.cancelChildren
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch
import kotlinx.coroutines.withTimeoutOrNull

/** No one is signed in: the car asks for Pley to be opened on the phone. */
class SignedOutException : IllegalStateException("Signed out")

/** What the music library is read from: the server signed in to now, as the profile sees it. */
interface MusicSource {
    /** The music libraries, in the profile's order. */
    suspend fun libraries(): List<Library>

    /** The albums added last to [libraryId], newest first. */
    suspend fun recentlyAdded(libraryId: Long): List<ItemSummary>

    /** What the profile played last in [libraryId] (tracks, the newest first). */
    suspend fun recentlyPlayed(libraryId: Long): List<ItemSummary>

    /** [limit] artists of [libraryId] from [offset], by name. */
    suspend fun artists(libraryId: Long, offset: Int, limit: Int): List<ItemSummary>

    /** An artist's albums, an album's tracks. */
    suspend fun children(itemId: Long): List<ItemSummary>

    /** An artist, album or track; `null` when the profile can't see it. */
    suspend fun item(itemId: Long): ItemSummary?

    /** The profile's playlists, by title. */
    suspend fun playlists(): List<PlaylistSummary>

    suspend fun playlist(playlistId: Long): PlaylistDetail

    /** Artists, albums, tracks and audiobooks matching [query], closest first. */
    suspend fun search(query: String): List<ItemSummary>

    /** The audiobook libraries, in the profile's order: none from a server that doesn't give them to the car. */
    suspend fun bookLibraries(): List<Library>

    /** [libraryId]'s books being heard (the one heard last first), and those added last. */
    suspend fun bookShelves(libraryId: Long): BookShelves

    /** [limit] books of [libraryId], by title. */
    suspend fun books(libraryId: Long, limit: Int): List<ItemSummary>

    /** A book's parts in the order they play, each with where the profile is in it: asked of the server each time. */
    suspend fun parts(bookId: Long): List<ItemSummary>

    /** The part of [bookId] to go on with ([parts]'s): the one heard last part way, else the next to hear. */
    suspend fun onDeck(bookId: Long): ItemSummary?
}

/** An audiobook library's rows: the books being heard, the one heard last first, and those added last. */
data class BookShelves(val listening: List<ItemSummary> = emptyList(), val added: List<ItemSummary> = emptyList())

/**
 * [MusicSource] over the app's repositories, which keep what they last
 * read: a car out of signal browses what was seen before. The server's
 * answer is waited for briefly; what was saved is used when it is slow.
 */
class RepositoryMusicSource(private val data: DataGraph, private val network: NetworkGraph) : MusicSource {
    override suspend fun libraries(): List<Library> =
        data.libraries.libraries().settled().filter { it.kind == LibraryKind.Music }

    override suspend fun recentlyAdded(libraryId: Long): List<ItemSummary> =
        data.hubs.library(libraryId).settled().hubs.firstOrNull { it.key.startsWith(RECENTLY_ADDED) }?.items.orEmpty()

    override suspend fun recentlyPlayed(libraryId: Long): List<ItemSummary> =
        signedInApi().history(limit = HISTORY_LIMIT, libraryId = libraryId).entries.map { it.item }

    override suspend fun artists(libraryId: Long, offset: Int, limit: Int): List<ItemSummary> =
        signedInApi().libraryItems(libraryId, sort = LibrarySort.Title, offset = offset, limit = limit).items

    override suspend fun children(itemId: Long): List<ItemSummary> = data.items.children(itemId).settled()

    override suspend fun item(itemId: Long): ItemSummary? = try {
        data.items.detail(itemId).settled().toSummary()
    } catch (e: ApiException) {
        if (e.status == 404) null else throw e
    }

    override suspend fun playlists(): List<PlaylistSummary> = data.playlists.playlists().settled()

    override suspend fun playlist(playlistId: Long): PlaylistDetail = data.playlists.playlist(playlistId).settled()

    // Asked of the server itself, not through the app's search: that keeps only its latest query, so a car's search
    // overtaken by its next one (a letter typed, its results read) was never answered, and the car went on waiting.
    override suspend fun search(query: String): List<ItemSummary> = signedInApi().search(query.trim(), SEARCH_LIMIT).items

    // The same libraries as the music's, read once for both. The books themselves are read from the server itself: what
    // the car shows of them (how far the profile is) is to be the latest.
    override suspend fun bookLibraries(): List<Library> =
        data.libraries.libraries().settled().filter { it.kind == LibraryKind.Audiobooks }

    override suspend fun bookShelves(libraryId: Long): BookShelves {
        val hubs = signedInApi().libraryHubs(libraryId).hubs
        return BookShelves(
            listening = hubs.firstOrNull { it.key.startsWith(CONTINUE_LISTENING) }?.items.orEmpty(),
            added = hubs.firstOrNull { it.key.startsWith(RECENTLY_ADDED) }?.items.orEmpty(),
        )
    }

    override suspend fun books(libraryId: Long, limit: Int): List<ItemSummary> =
        signedInApi().libraryItems(libraryId, sort = LibrarySort.Title, offset = 0, limit = limit).items

    override suspend fun parts(bookId: Long): List<ItemSummary> = signedInApi().children(bookId).items

    override suspend fun onDeck(bookId: Long): ItemSummary? = signedInApi().onDeck(bookId).item

    /** [api], once the sign-in has been read back ([signInRead]). */
    private suspend fun signedInApi(): PleyApi {
        data.signInRead()
        return api()
    }

    private fun api(): PleyApi {
        val session = data.session.session.value ?: throw SignedOutException()
        return network.api(session.issuer, session.me?.id)
    }

    private suspend fun <T> Flow<Loadable<T>>.settled(): T = settledIn(data)

    private companion object {
        /** A library's recently added row ("recently_added:<library id>"). */
        const val RECENTLY_ADDED = "recently_added"

        /** An audiobook library's books being heard ("continue_listening:<library id>"). */
        const val CONTINUE_LISTENING = "continue_listening"

        /** The plays read for Recently played: enough for a row of albums. */
        const val HISTORY_LIMIT = 100

        /** The most results asked of a search: the library shows fifteen of each kind. */
        const val SEARCH_LIMIT = 200
    }
}

/** [freshOrSaved], once the sign-in has been read back. Signed out, [SignedOutException]. */
internal suspend fun <T> Flow<Loadable<T>>.settledIn(data: DataGraph): T {
    data.signInRead()
    if (data.session.session.value == null) throw SignedOutException()
    return freshOrSaved()
}

/**
 * The server's answer, or what was saved when the server is slow: after
 * [SAVED_WAIT_MS] once something saved is there, which a car shows at
 * once rather than wait on a slow network, else after [FRESH_WAIT_MS].
 * The answer still comes in after, for the next time the car asks, and
 * the [SavedAnswers] in the caller's context hear of it. A failure with
 * nothing saved throws it.
 */
internal suspend fun <T> Flow<Loadable<T>>.freshOrSaved(): T {
    var saved: T? = null
    val fresh = withTimeoutOrNull(FRESH_WAIT_MS) {
        coroutineScope {
            val answer = CompletableDeferred<Loadable<T>?>()
            launch {
                answer.complete(
                    first {
                        if (it is Loadable.Ready && !it.fresh && saved == null) {
                            saved = it.value
                            this@coroutineScope.launch {
                                delay(SAVED_WAIT_MS)
                                answer.complete(null)
                            }
                        }
                        (it is Loadable.Ready && it.fresh) || it is Loadable.Failed
                    },
                )
            }
            answer.await().also { coroutineContext.cancelChildren() }
        }
    }
    return when (fresh) {
        is Loadable.Ready -> fresh.value
        is Loadable.Failed -> fresh.stale ?: saved ?: throw fresh.error
        else -> saved?.also { savedFor(currentCoroutineContext()[SavedAnswers]) } ?: when (val late = first { it !is Loadable.Loading }) {
            is Loadable.Ready -> late.value
            is Loadable.Failed -> late.stale ?: throw late.error
            Loadable.Loading -> error("Still loading")
        }
    }
}

/** [answers] hear when the server's answer comes in after what was saved was given: whether it came, or failed. */
private fun <T> Flow<Loadable<T>>.savedFor(answers: SavedAnswers?) {
    answers?.add { first { it is Loadable.Failed || (it is Loadable.Ready && it.fresh) } is Loadable.Ready }
}

/**
 * The answers a car's list was made from that were what was saved, the
 * server slow ([freshOrSaved]). In the context of what reads a list, it
 * says whether the list was made from them, and waits for the server's
 * own answers, which [SavedLists] then reads the list again from.
 */
class SavedAnswers : AbstractCoroutineContextElement(SavedAnswers) {
    private val waits = ConcurrentLinkedQueue<suspend () -> Boolean>()

    /** Whether any of the list was what was saved. */
    val any: Boolean get() = waits.isNotEmpty()

    internal fun add(fresh: suspend () -> Boolean) {
        waits += fresh
    }

    /** Waits for the server's answer to each; whether every one came (a failure leaves what was shown as it is). */
    suspend fun fresh(): Boolean = waits.all { it() }

    companion object Key : CoroutineContext.Key<SavedAnswers>
}

/** How long the server's answer is waited for when nothing is saved yet. */
internal const val FRESH_WAIT_MS = 2_500L

/** How long the server's answer is waited for once something saved is there to show. */
internal const val SAVED_WAIT_MS = 300L

/**
 * Waits, up to [SIGN_IN_WAIT_MS], for the sign-in to be read back from the device. A process Android starts again for
 * the music (a car connecting, a headset's Play) reads it as the music service starts, and until it has, no profile
 * is signed in: nothing kept would be found, and the car would be told to sign in.
 */
internal suspend fun DataGraph.signInRead() {
    withTimeoutOrNull(SIGN_IN_WAIT_MS) { session.state.first { it !is SessionState.Starting } }
}

/** The longest the music waits for the sign-in to be read back; the video player waits as long. */
private const val SIGN_IN_WAIT_MS = 5_000L
