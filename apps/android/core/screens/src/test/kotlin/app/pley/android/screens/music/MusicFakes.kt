package app.pley.android.screens.music

import app.pley.android.data.ItemRepository
import app.pley.android.data.Loadable
import app.pley.android.data.WatchStateRepository
import app.pley.android.model.CollectionSummary
import app.pley.android.model.ItemDetail
import app.pley.android.model.ItemKind
import app.pley.android.model.ItemSummary
import app.pley.android.player.MusicControl
import app.pley.android.player.MusicQueueState
import app.pley.android.player.NowPlaying
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow

/** Items whose details and children the test sets. */
class FakeItems : ItemRepository {
    val details = HashMap<Long, MutableStateFlow<Loadable<ItemDetail>>>()
    val kids = HashMap<Long, MutableStateFlow<Loadable<List<ItemSummary>>>>()

    fun detailOf(id: Long): MutableStateFlow<Loadable<ItemDetail>> = details.getOrPut(id) { MutableStateFlow(Loadable.Loading) }

    fun kidsOf(id: Long): MutableStateFlow<Loadable<List<ItemSummary>>> = kids.getOrPut(id) { MutableStateFlow(Loadable.Loading) }

    override fun detail(itemId: Long): Flow<Loadable<ItemDetail>> = detailOf(itemId)
    override fun children(itemId: Long): Flow<Loadable<List<ItemSummary>>> = kidsOf(itemId)
    val decks = HashMap<Long, MutableStateFlow<Loadable<ItemSummary?>>>()

    fun deckOf(id: Long): MutableStateFlow<Loadable<ItemSummary?>> = decks.getOrPut(id) { MutableStateFlow(Loadable.Ready(null, fresh = true)) }

    override fun onDeck(itemId: Long): Flow<Loadable<ItemSummary?>> = deckOf(itemId)
    override fun related(itemId: Long): Flow<Loadable<List<ItemSummary>>> = error("not music")
    override fun collectionsOf(itemId: Long): Flow<Loadable<List<CollectionSummary>>> = error("not music")
    override suspend fun next(itemId: Long): ItemSummary? = null
    override fun prefetch(itemId: Long) = Unit
}

/** Music that records what it was asked to play. */
class FakeMusic : MusicControl {
    val now = MutableStateFlow<NowPlaying?>(null)
    val played = mutableListOf<Triple<List<Long>, Int, Boolean>>()
    val startedAt = mutableListOf<Long>()
    val seeks = mutableListOf<Long>()
    var toggles = 0
    var stops = 0

    override val nowPlaying: StateFlow<NowPlaying?> = now
    override val queue: StateFlow<MusicQueueState> = MutableStateFlow(MusicQueueState())

    override fun play(tracks: List<ItemSummary>, startIndex: Int, shuffle: Boolean) {
        played += Triple(tracks.map { it.id }, startIndex, shuffle)
    }

    override fun playFrom(tracks: List<ItemSummary>, startIndex: Int, startMs: Long) {
        played += Triple(tracks.map { it.id }, startIndex, false)
        startedAt += startMs
    }

    override fun seekTo(positionMs: Long) {
        seeks += positionMs
    }

    override fun toggle() {
        toggles++
    }

    override fun stop() {
        stops++
    }

    override fun pauseForVideo() = Unit
}

/** Ratings that are recorded. */
class FakeWatchState : WatchStateRepository {
    val ratings = mutableListOf<Pair<Long, Double?>>()
    val watched = mutableListOf<Pair<Long, Boolean>>()
    override suspend fun setWatched(itemId: Long, watched: Boolean) {
        this.watched += itemId to watched
    }
    override suspend fun rate(itemId: Long, rating: Double?) {
        ratings += itemId to rating
    }
    override suspend fun chooseStreams(itemId: Long, fileId: Long?, audioIndex: Int?, subtitleIndex: Int?, automaticAudio: Boolean) = Unit
    override suspend fun hideFromContinueWatching(itemId: Long) = Unit
}

const val ORIGIN = "http://192.168.1.20:7539"

fun track(id: Long, album: Long = 50, disc: Int? = null, artist: String = "Harbour Lights") = ItemSummary(
    id = id,
    libraryId = 3,
    kind = ItemKind.Track,
    title = "Track $id",
    index = (id % 100).toInt(),
    parentId = album,
    parentTitle = "Deep Water",
    parentIndex = disc,
    grandparentId = 40,
    grandparentTitle = artist,
    durationMs = 185_000,
)

fun album(id: Long, year: Int?, title: String = "Album $id") = ItemSummary(id, 3, ItemKind.Album, title, sortTitle = title, year = year, parentId = 40, parentTitle = "Harbour Lights")

val ALBUM_DETAIL = ItemDetail(
    id = 50,
    libraryId = 3,
    kind = ItemKind.Album,
    title = "Deep Water",
    year = 2019,
    parentId = 40,
    parentTitle = "Harbour Lights",
    posterUrl = "/api/items/50/images/poster?v=1",
    summary = "Recorded by the sea.",
)

val ARTIST_DETAIL = ItemDetail(
    id = 40,
    libraryId = 3,
    kind = ItemKind.Artist,
    title = "Harbour Lights",
    childCount = 3,
    leafCount = 30,
    posterUrl = "/api/items/40/images/poster?v=1",
    genres = listOf("Ambient"),
)
