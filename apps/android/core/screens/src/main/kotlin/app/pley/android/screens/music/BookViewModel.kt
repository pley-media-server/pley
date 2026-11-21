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
import app.pley.android.model.Chapter
import app.pley.android.model.ItemKind
import app.pley.android.model.ItemSummary
import app.pley.android.network.ApiException
import app.pley.android.player.MusicControl
import app.pley.android.screens.R
import app.pley.android.screens.screensGraph
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
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.flowOn
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.receiveAsFlow
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

/**
 * An audiobook's page: its details and parts from what was saved, then the
 * server; the chapters of a book in one file; the part playing marked.
 * Play goes on where the profile is in the book (the server's on-deck part,
 * on any device), or plays and pauses the book while it is in the player;
 * a part or a chapter picked plays from it. Books play in the app's music,
 * at the books' speed.
 */
@OptIn(ExperimentalCoroutinesApi::class)
class BookViewModel(
    private val items: ItemRepository,
    origin: Flow<String>,
    private val playItem: PlayItem,
    private val music: MusicControl,
    private val watchState: WatchStateRepository?,
    val bookId: Long,
    /** Between the last two names of a list ("A, B and C"), in the language shown. */
    private val and: () -> String,
    compute: CoroutineDispatcher = Dispatchers.Default,
) : ViewModel() {
    private val retries = MutableStateFlow(0)
    private val busy = MutableStateFlow(false)
    private val channel = Channel<MusicEvent>(Channel.BUFFERED)

    val events: Flow<MusicEvent> = channel.receiveAsFlow()

    /** The chapters of the book's one part, from its file; none for a book of several parts. */
    private fun chaptersOf(parts: List<ItemSummary>?): Flow<List<Chapter>> {
        val only = parts?.singleOrNull() ?: return flowOf(emptyList())
        return items.detail(only.id).map { it.valueOrNull?.files?.firstOrNull()?.chapters.orEmpty() }.distinctUntilChanged()
    }

    private val page: Flow<MusicPageState<BookPage>> = retries
        .flatMapLatest {
            val parts = items.children(bookId).map { loaded -> loaded.map { list -> list.filter { it.kind == ItemKind.Part } } }
            val withChapters = parts.flatMapLatest { loaded -> chaptersOf(loaded.valueOrNull).map { loaded to it } }
            // Made again when the language shown changes: the lengths and "and" are written in it.
            combine(items.detail(bookId), withChapters, items.onDeck(bookId), origin, AppLanguage.shown) { detail, (list, chapters), next, server, _ ->
                val book = detail.valueOrNull
                when {
                    book != null -> MusicPageState.Ready(
                        bookPage(book, list.valueOrNull, next.valueOrNull, chapters, server, and(), AppLanguage.formatWords()),
                        detail.map { },
                    )
                    detail is Loadable.Failed && (detail.error as? ApiException)?.status == 404 -> MusicPageState.NotFound
                    detail is Loadable.Failed -> MusicPageState.Failed(detail.error)
                    else -> MusicPageState.Loading
                }
            }
        }
        .flowOn(compute)

    /** The part of this book in the player and whether it plays; none once the player heard the end of what it had. */
    private val inPlayer = music.nowPlaying.map { now ->
        if (now == null || now.ended || !now.book) null else Triple(now.itemId, now.playing || now.loading, now.chapter)
    }.distinctUntilChanged()

    val state: StateFlow<MusicPageState<BookPage>> = combine(page, busy, inPlayer) { p, b, playing ->
        if (p is MusicPageState.Ready) {
            val mine = playing?.takeIf { (id) -> p.page.parts.any { it.id == id } }
            p.copy(busy = b, current = mine?.first, playing = mine?.second == true)
        } else {
            p
        }
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), MusicPageState.Loading)

    /** Which chapter of the book's one file plays, -1 when it doesn't. */
    val chapterNow: StateFlow<Int> = combine(page, inPlayer) { p, playing ->
        val parts = (p as? MusicPageState.Ready)?.page?.parts.orEmpty()
        playing?.takeIf { (id) -> parts.singleOrNull()?.id == id }?.third ?: -1
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), -1)

    fun retry() = retries.update { it + 1 }

    private val ready: MusicPageState.Ready<BookPage>? get() = state.value as? MusicPageState.Ready

    /**
     * Play: the book in the player plays or pauses; otherwise it goes on
     * where the profile is in it, asked of the server then, so a book heard
     * elsewhere since goes on from there.
     */
    fun play() {
        val r = ready ?: return
        if (r.current != null) {
            music.toggle()
            return
        }
        if (busy.value) return
        busy.value = true
        viewModelScope.launch {
            try {
                when (val target = playItem.resolve(r.page.summaryItem(), fromStart = false)) {
                    is PlayTarget.Music -> if (target.tracks.isEmpty()) {
                        channel.send(MusicEvent.NothingToPlay)
                    } else {
                        music.playFrom(target.tracks, target.startIndex, target.startMs)
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

    /** From the start of the book's first part. */
    fun fromStart() = start(0, 0)

    /** A part's row: the part playing pauses or plays on; another plays the book from it, where it was left in it. */
    fun playPart(position: Int) {
        val r = ready ?: return
        val part = r.page.parts.getOrNull(position) ?: return
        if (r.current == part.id) {
            music.toggle()
            return
        }
        start(position, if (part.userState.watched) 0 else part.userState.positionMs)
    }

    /** A chapter of the book's one file: from its start, the part playing moved there. */
    fun playChapter(startMs: Long) {
        val r = ready ?: return
        if (r.current != null) {
            music.seekTo(startMs)
            if (!r.playing) music.toggle()
            return
        }
        start(0, startMs)
    }

    private fun start(position: Int, startMs: Long) {
        val parts = ready?.page?.parts.orEmpty()
        if (parts.isEmpty()) {
            channel.trySend(MusicEvent.NothingToPlay)
            return
        }
        music.playFrom(parts, position.coerceIn(0, parts.lastIndex), startMs.coerceAtLeast(0))
        channel.trySend(MusicEvent.Played)
    }

    /** Every part heard to its end, or none: the book leaves Continue Listening, or starts over. */
    fun setFinished(finished: Boolean) {
        val repository = watchState ?: return
        viewModelScope.launch {
            try {
                repository.setWatched(bookId, finished)
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                channel.send(MusicEvent.Failed(e))
            }
        }
    }

    /** The book as a listing has it, for Play. */
    private fun BookPage.summaryItem(): ItemSummary = ItemSummary(id = id, libraryId = 0, kind = ItemKind.Audiobook, title = title)

    companion object {
        fun factory(bookId: Long): ViewModelProvider.Factory = viewModelFactory {
            initializer {
                val g = screensGraph()
                val context = checkNotNull(this[ViewModelProvider.AndroidViewModelFactory.APPLICATION_KEY])
                BookViewModel(
                    g.data.items,
                    g.data.servers.active.map { it.orEmpty() }.distinctUntilChanged(),
                    g.data.playItem,
                    g.player.music,
                    g.data.watchState,
                    bookId,
                    and = { AppLanguage.resources(context).getString(R.string.book_and) },
                )
            }
        }
    }
}
