package app.pley.android.screens.music

import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import app.pley.android.common.AppLanguage
import app.pley.android.data.ItemRepository
import app.pley.android.data.Loadable
import app.pley.android.data.PlayItem
import app.pley.android.data.PlayTarget
import app.pley.android.data.WatchStateRepository
import app.pley.android.data.map
import app.pley.android.data.valueOrNull
import app.pley.android.model.ItemDetail
import app.pley.android.model.ItemSummary
import app.pley.android.model.format.formatClockMs
import app.pley.android.network.ApiException
import app.pley.android.player.MusicControl
import app.pley.android.player.NowPlaying
import app.pley.android.player.sleep.SleepChoice
import app.pley.android.player.sleep.SleepState
import app.pley.android.screens.screensGraph
import java.util.Locale
import kotlin.random.Random
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOn
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.receiveAsFlow
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

/** What a music page tells its screen once, besides its state. */
sealed interface MusicEvent {
    /** Music started: the TV opens Now Playing. */
    data object Played : MusicEvent

    /** An album or artist without tracks. */
    data object NothingToPlay : MusicEvent

    /** Something the person did failed, with the error's sentence. */
    data class Failed(val error: Throwable) : MusicEvent
}

/** The state of an artist's or album's page. */
sealed interface MusicPageState<out P> {
    data object Loading : MusicPageState<Nothing>

    /** No such item, or one this profile may not see (404). */
    data object NotFound : MusicPageState<Nothing>

    data class Failed(val error: Throwable) : MusicPageState<Nothing>

    /**
     * The page, with [status] saying whether it came from the server; the
     * track [current] in the music playing (one of this page's), and
     * whether it [playing]; [busy] while Play gathers the tracks.
     */
    data class Ready<P>(
        val page: P,
        val status: Loadable<Unit>,
        val current: Long? = null,
        val playing: Boolean = false,
        val busy: Boolean = false,
    ) : MusicPageState<P>
}

/** A page's state from its item's [detail]: [make] builds the page once it is known. */
private fun <P> pageState(detail: Loadable<ItemDetail>, make: (ItemDetail) -> P): MusicPageState<P> {
    val item = detail.valueOrNull
    return when {
        item != null -> MusicPageState.Ready(make(item), detail.map { })
        detail is Loadable.Failed && (detail.error as? ApiException)?.status == 404 -> MusicPageState.NotFound
        detail is Loadable.Failed -> MusicPageState.Failed(detail.error)
        else -> MusicPageState.Loading
    }
}

/** The music playing, as a page shows it: which track and whether it plays. */
private fun Flow<NowPlaying?>.playingTrack(): Flow<Pair<Long?, Boolean>> =
    map { (it?.itemId) to (it?.playing == true || it?.loading == true) }.distinctUntilChanged()

/**
 * An artist's page: their details and albums from what was saved, then the
 * server; Play and Shuffle play every track of theirs (PlayItem, as the web's
 * usePlayItem does).
 */
@OptIn(ExperimentalCoroutinesApi::class)
class ArtistViewModel(
    private val items: ItemRepository,
    origin: Flow<String>,
    private val playItem: PlayItem,
    private val music: MusicControl,
    val artistId: Long,
    compute: CoroutineDispatcher = Dispatchers.Default,
    private val random: Random = Random.Default,
    locale: Locale = AppLanguage.locale,
) : ViewModel() {
    private val retries = MutableStateFlow(0)
    private val busy = MutableStateFlow(false)
    private val channel = Channel<MusicEvent>(Channel.BUFFERED)

    val events: Flow<MusicEvent> = channel.receiveAsFlow()

    private val page: Flow<MusicPageState<ArtistPage>> = retries
        .flatMapLatest {
            combine(items.detail(artistId), items.children(artistId), origin) { detail, albums, server ->
                pageState(detail) { artistPage(it, albums.valueOrNull, server, locale) }
            }
        }
        .flowOn(compute)

    val state: StateFlow<MusicPageState<ArtistPage>> = combine(page, busy, music.nowPlaying.playingTrack()) { p, b, (current, playing) ->
        if (p is MusicPageState.Ready) p.copy(busy = b, current = current, playing = playing) else p
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), MusicPageState.Loading)

    fun retry() = retries.update { it + 1 }

    /** Plays every track of the artist's, in album order, or shuffled. */
    fun play(shuffle: Boolean = false) {
        val ready = state.value as? MusicPageState.Ready ?: return
        if (busy.value) return
        busy.value = true
        viewModelScope.launch {
            try {
                when (val target = playItem.resolve(ready.page.summaryItem, fromStart = false)) {
                    is PlayTarget.Music -> if (target.tracks.isEmpty()) {
                        channel.send(MusicEvent.NothingToPlay)
                    } else {
                        val start = if (shuffle) random.nextInt(target.tracks.size) else target.startIndex
                        music.play(target.tracks, start, shuffle)
                        channel.send(MusicEvent.Played)
                    }
                    else -> channel.send(MusicEvent.NothingToPlay)
                }
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                channel.send(MusicEvent.Failed(e))
            } finally {
                busy.value = false
            }
        }
    }

    companion object {
        fun factory(artistId: Long): ViewModelProvider.Factory = viewModelFactory {
            initializer {
                val g = screensGraph()
                ArtistViewModel(g.data.items, g.data.servers.active.map { it.orEmpty() }.distinctUntilChanged(), g.data.playItem, g.player.music, artistId)
            }
        }
    }
}

/**
 * An album's page: its details and tracks, the track playing marked, Play,
 * Shuffle and playing from any track, and the profile's rating.
 * [highlightId] is the track it was opened from.
 */
@OptIn(ExperimentalCoroutinesApi::class)
class AlbumViewModel(
    private val items: ItemRepository,
    origin: Flow<String>,
    private val music: MusicControl,
    private val watchState: WatchStateRepository?,
    val albumId: Long,
    val highlightId: Long? = null,
    compute: CoroutineDispatcher = Dispatchers.Default,
    private val random: Random = Random.Default,
    locale: Locale = AppLanguage.locale,
) : ViewModel() {
    private val retries = MutableStateFlow(0)
    private val channel = Channel<MusicEvent>(Channel.BUFFERED)

    val events: Flow<MusicEvent> = channel.receiveAsFlow()

    private val page: Flow<MusicPageState<AlbumPage>> = retries
        .flatMapLatest {
            combine(items.detail(albumId), items.children(albumId), origin) { detail, tracks, server ->
                pageState(detail) { albumPage(it, tracks.valueOrNull, server, locale) }
            }
        }
        .flowOn(compute)

    val state: StateFlow<MusicPageState<AlbumPage>> = combine(page, music.nowPlaying.playingTrack()) { p, (current, playing) ->
        if (p is MusicPageState.Ready) {
            val mine = current?.takeIf { id -> p.page.tracks.any { it.id == id } }
            p.copy(current = mine, playing = mine != null && playing)
        } else {
            p
        }
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), MusicPageState.Loading)

    fun retry() = retries.update { it + 1 }

    private val tracks: List<ItemSummary> get() = (state.value as? MusicPageState.Ready)?.page?.tracks.orEmpty()

    /** Plays the album from its track at [position] (the first by default). */
    fun play(position: Int = 0) {
        val list = tracks
        if (list.isEmpty()) {
            channel.trySend(MusicEvent.NothingToPlay)
            return
        }
        music.play(list, position.coerceIn(0, list.lastIndex))
        channel.trySend(MusicEvent.Played)
    }

    fun shuffle() {
        val list = tracks
        if (list.isEmpty()) {
            channel.trySend(MusicEvent.NothingToPlay)
            return
        }
        music.play(list, random.nextInt(list.size), shuffle = true)
        channel.trySend(MusicEvent.Played)
    }

    /** A track's row: the track playing pauses or plays on; another one plays the album from it. */
    fun playOrToggle(position: Int) {
        val track = tracks.getOrNull(position) ?: return
        val ready = state.value as? MusicPageState.Ready
        if (ready?.current == track.id) music.toggle() else play(position)
    }

    /** The profile's rating, 0 to 10 in half stars; `null` clears it. */
    fun rate(value: Int?) {
        val repository = watchState ?: return
        viewModelScope.launch {
            try {
                repository.rate(albumId, value?.toDouble())
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                channel.send(MusicEvent.Failed(e))
            }
        }
    }

    companion object {
        fun factory(albumId: Long, highlightId: Long? = null): ViewModelProvider.Factory = viewModelFactory {
            initializer {
                val g = screensGraph()
                AlbumViewModel(
                    g.data.items,
                    g.data.servers.active.map { it.orEmpty() }.distinctUntilChanged(),
                    g.player.music,
                    g.data.watchState,
                    albumId,
                    highlightId,
                )
            }
        }
    }
}

/** A line of the queue: its [position] in the play order, and whether it is the one playing. */
data class QueueRow(
    val position: Int,
    val id: Long,
    val title: String,
    /** "Artist · Album". */
    val subtitle: String,
    val duration: String,
    val current: Boolean,
    /** The album art's path, as the server gave it; `null` without. */
    val artPath: String?,
)

/** Now Playing: the track, its controls and the queue. */
data class NowPlayingState(
    /** `null` when nothing plays. */
    val now: NowPlaying?,
    val queue: List<QueueRow>,
    val origin: String,
)

/**
 * Now Playing (design spec §4.10): what plays, its controls, and the queue
 * to jump around in.
 */
class NowPlayingViewModel(
    private val music: MusicControl,
    origin: Flow<String>,
    compute: CoroutineDispatcher = Dispatchers.Default,
) : ViewModel() {
    val state: StateFlow<NowPlayingState> = combine(music.nowPlaying, music.queue, origin) { now, queue, server ->
        NowPlayingState(
            now = now,
            queue = queue.tracks.mapIndexed { i, t ->
                QueueRow(
                    position = i,
                    id = t.id,
                    title = t.title,
                    subtitle = listOfNotNull(t.grandparentTitle, t.parentTitle).filter { it.isNotBlank() }.joinToString(" · "),
                    duration = t.durationMs?.takeIf { it > 0 }?.let(::formatClockMs).orEmpty(),
                    current = i == queue.index,
                    artPath = t.posterUrl?.takeIf { it.isNotEmpty() } ?: t.parentId?.let { "/api/items/$it/images/poster" },
                )
            },
            origin = server,
        )
    }.flowOn(compute).stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), NowPlayingState(music.nowPlaying.value, emptyList(), ""))

    /** Where the track is: read while drawing. */
    fun position() = music.position()

    fun toggle() = music.toggle()

    fun next() = music.next()

    fun previous() = music.previous()

    fun seekTo(positionMs: Long) = music.seekTo(positionMs)

    fun jumpTo(position: Int) = music.jumpTo(position)

    fun setShuffle(shuffle: Boolean) = music.setShuffle(shuffle)

    /** The music's sleep timer. */
    val sleep: StateFlow<SleepState> get() = music.sleep

    fun setSleep(choice: SleepChoice) = music.setSleep(choice)

    /** "Keep listening" during the timer's notice: the same time again. */
    fun keepListening() = music.keepListening()

    companion object {
        val factory: ViewModelProvider.Factory = viewModelFactory {
            initializer {
                val g = screensGraph()
                NowPlayingViewModel(g.player.music, g.data.servers.active.map { it.orEmpty() }.distinctUntilChanged())
            }
        }
    }
}
