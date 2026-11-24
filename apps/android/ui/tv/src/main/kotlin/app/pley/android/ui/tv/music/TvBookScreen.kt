package app.pley.android.ui.tv.music

import androidx.compose.foundation.background
import androidx.compose.foundation.focusGroup
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.focusRestorer
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import app.pley.android.common.UiText
import app.pley.android.common.errorSentence
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.PleyTv
import app.pley.android.designsystem.PleyType
import app.pley.android.designsystem.TvSafeArea
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.image.Artwork
import app.pley.android.designsystem.image.ArtworkImage
import app.pley.android.designsystem.image.MarkGlyph
import app.pley.android.designsystem.text.asString
import app.pley.android.screens.PleyKey
import app.pley.android.screens.R as ScreensR
import app.pley.android.screens.music.BookPage
import app.pley.android.screens.music.BookRow
import app.pley.android.screens.music.BookViewModel
import app.pley.android.screens.music.MusicEvent
import app.pley.android.screens.music.MusicPageState
import app.pley.android.ui.tv.R
import app.pley.android.ui.tv.kit.HeldEnds
import app.pley.android.ui.tv.kit.TvPillButton
import app.pley.android.ui.tv.kit.TvRoundButton
import app.pley.android.ui.tv.kit.heldKeyStaysIn
import app.pley.android.ui.tv.kit.rememberFocusMemory
import app.pley.android.ui.tv.kit.rememberedFocus
import app.pley.android.ui.tv.shell.LocalTvNavigator
import app.pley.android.ui.tv.shell.failedBody
import app.pley.android.ui.tv.shell.failedTitle

/**
 * An audiobook on TV (BookView.tsx), laid out as an album: the cover and
 * what it is on the left (its series, who wrote and reads it, its length
 * and parts, how far the profile is in it), Play (where it was left, on
 * any device), From the start and Mark as finished under it, and its parts
 * on the right, or the chapters of a book in one file, the one playing
 * marked. Select on a part plays the book from it, on a chapter from its
 * start. Playing opens Now Playing; Back comes here.
 */
@Composable
internal fun TvBookScreen(bookId: Long) {
    val model: BookViewModel = viewModel(factory = BookViewModel.factory(bookId))
    val state by model.state.collectAsStateWithLifecycle()
    val chapterNow by model.chapterNow.collectAsStateWithLifecycle()
    val navigator = LocalTvNavigator.current
    var problem by remember { mutableStateOf<UiText?>(null) }
    LaunchedEffect(model) {
        model.events.collect { event ->
            when (event) {
                MusicEvent.Played -> navigator.open(PleyKey.NowPlaying)
                MusicEvent.NothingToPlay -> problem = UiText.res(ScreensR.string.book_nothing_to_play)
                is MusicEvent.Failed -> problem = errorSentence(event.error)
            }
        }
    }
    Box(Modifier.fillMaxSize().testTag(TV_BOOK_PAGE)) {
        when (val s = state) {
            MusicPageState.Loading -> Unit
            MusicPageState.NotFound -> MusicMessage(
                title = stringResource(ScreensR.string.book_not_found),
                body = stringResource(R.string.tv_music_not_found_body),
                action = stringResource(R.string.tv_music_back),
                onAction = navigator::back,
            )
            is MusicPageState.Failed -> MusicMessage(
                title = failedTitle(s.error, stringResource(ScreensR.string.book_failed)),
                body = failedBody(s.error),
                action = stringResource(R.string.tv_music_retry),
                onAction = model::retry,
            )
            is MusicPageState.Ready -> {
                val art = remember(s.page.posterUrl, s.page.origin) { ArtworkImage.of(s.page.posterUrl, s.page.origin) }
                FaintArt(art, s.page.title, alpha = 0.14f)
                BookContent(
                    s.page,
                    art,
                    current = s.current,
                    playing = s.playing,
                    chapterNow = chapterNow,
                    problem = problem,
                    model = model,
                    onNowPlaying = { navigator.open(PleyKey.NowPlaying) },
                )
            }
        }
    }
}

@Composable
private fun BookContent(
    page: BookPage,
    art: ArtworkImage?,
    current: Long?,
    playing: Boolean,
    chapterNow: Int,
    problem: UiText?,
    model: BookViewModel,
    onNowPlaying: () -> Unit,
) {
    val memory = rememberFocusMemory()
    Row(Modifier.fillMaxSize().padding(top = TOP)) {
        Column(
            Modifier
                .padding(start = PleyTv.Gutter)
                .width(COLUMN)
                .fillMaxHeight()
                .focusRestorer()
                .focusGroup(),
        ) {
            Box(Modifier.size(ART).clip(PleyShape.TileTv).background(PleyColors.Surface2)) {
                Artwork(art, page.title, Modifier.matchParentSize(), mark = MarkGlyph.Audiobooks)
            }
            Spacer(Modifier.height(12.dp))
            BasicText(page.kicker.asString(), style = PleyType.Tv.Caption.copy(color = PleyColors.Muted), maxLines = 1, overflow = TextOverflow.Ellipsis)
            BasicText(page.title, style = PleyType.Tv.Title.copy(color = PleyColors.Fg), maxLines = 2, overflow = TextOverflow.Ellipsis)
            page.byline?.let { BasicText(it.asString(), style = PleyType.Tv.Label.copy(color = PleyColors.Fg), maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.padding(top = 2.dp)) }
            page.readBy?.let { BasicText(it.asString(), style = PleyType.Tv.Caption.copy(color = PleyColors.Muted), maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.padding(top = 2.dp)) }
            val meta = page.metaTexts.asString()
            if (meta.isNotEmpty()) BasicText(meta, style = PleyType.Tv.Caption.copy(color = PleyColors.Muted), modifier = Modifier.padding(top = 4.dp))
            page.progress?.let { progress ->
                Box(
                    Modifier
                        .padding(top = 10.dp)
                        .fillMaxWidth()
                        .height(4.dp)
                        .drawBehind {
                            val r = CornerRadius(size.height / 2f)
                            drawRoundRect(PleyColors.Fg.copy(alpha = 0.2f), cornerRadius = r)
                            drawRoundRect(PleyColors.Accent, size = Size(size.width * progress.fraction, size.height), cornerRadius = r)
                        },
                )
                BasicText(
                    listOf(progress.heard.asString(), progress.left).filter { it.isNotEmpty() }.joinToString(" · "),
                    style = PleyType.Tv.Caption.copy(color = PleyColors.Muted),
                    modifier = Modifier.padding(top = 4.dp),
                )
            }
            Spacer(Modifier.height(16.dp))
            Row(horizontalArrangement = Arrangement.spacedBy(PleyTv.ButtonGap), verticalAlignment = Alignment.Top) {
                val playingThis = current != null
                TvPillButton(
                    stringResource(
                        when {
                            playingThis && playing -> ScreensR.string.book_pause
                            page.started || playingThis -> ScreensR.string.book_resume
                            page.finished -> ScreensR.string.book_listen_again
                            else -> ScreensR.string.book_play
                        },
                    ),
                    onClick = model::play,
                    icon = if (playingThis && playing) PleyIcons.Pause else PleyIcons.Play,
                    // Not turned off while the book is asked for: a TV moves the focus off a button that turns off,
                    // onto From the start or Mark as finished, where a second press would act. play() ignores it.
                    enabled = page.partsLoading || page.parts.isNotEmpty(),
                    modifier = Modifier.rememberedFocus(memory, "play", initial = true),
                )
                if (page.started) {
                    TvRoundButton(stringResource(ScreensR.string.book_from_start), PleyIcons.RotateCcw, onClick = model::fromStart, onArt = false, modifier = Modifier.rememberedFocus(memory, "start"))
                }
                TvRoundButton(
                    stringResource(if (page.finished) ScreensR.string.book_mark_unfinished else ScreensR.string.book_mark_finished),
                    if (page.finished) PleyIcons.CircleCheck else PleyIcons.Check,
                    onClick = { model.setFinished(!page.finished) },
                    onArt = false,
                    enabled = page.parts.isNotEmpty(),
                    modifier = Modifier.rememberedFocus(memory, "finished"),
                )
                if (playingThis) {
                    TvRoundButton(stringResource(R.string.tv_music_now_playing), PleyIcons.AudioLines, onClick = onNowPlaying, onArt = false, modifier = Modifier.rememberedFocus(memory, "now"))
                }
            }
            page.upTo?.let { BasicText(it.asString(), style = PleyType.Tv.Caption.copy(color = PleyColors.Muted), modifier = Modifier.padding(top = 8.dp)) }
            if (problem != null) BasicText(problem.asString(), style = PleyType.Tv.Caption.copy(color = PleyColors.DangerText), modifier = Modifier.padding(top = 8.dp))
            page.summary?.let {
                BasicText(it, style = PleyType.Tv.Caption.copy(color = PleyColors.Fg.copy(alpha = 0.8f)), maxLines = 6, overflow = TextOverflow.Ellipsis, modifier = Modifier.padding(top = 12.dp))
            }
        }
        Spacer(Modifier.width(24.dp))
        val list = rememberLazyListState()
        // Coming in from the left: the part or chapter playing, else the first.
        val entry = remember { FocusRequester() }
        val entryKey = page.rows.firstOrNull { (it is BookRow.Part && it.id == current) || (it is BookRow.Chapter && it.index == chapterNow) }?.key ?: page.rows.firstOrNull()?.key
        val heard = stringResource(ScreensR.string.book_heard)
        LazyColumn(
            Modifier
                .fillMaxSize()
                .padding(end = TvSafeArea.Horizontal)
                .heldKeyStaysIn(HeldEnds.UpDown)
                .focusRestorer(entry)
                .focusGroup()
                .testTag(TV_BOOK_ROWS),
            state = list,
            contentPadding = PaddingValues(bottom = 120.dp),
            verticalArrangement = Arrangement.spacedBy(2.dp),
        ) {
            if (page.partsLoading) {
                items(8, key = { "skeleton-$it" }) {
                    Box(Modifier.padding(vertical = 4.dp).size(480.dp, 36.dp).background(PleyColors.Surface2, PleyShape.Field))
                }
            } else if (page.rows.isNotEmpty()) {
                item(key = "heading") {
                    BasicText(
                        stringResource(if (page.rows.first() is BookRow.Chapter) ScreensR.string.book_chapters else ScreensR.string.book_parts),
                        style = PleyType.Tv.Label.copy(color = PleyColors.Subtle),
                        modifier = Modifier.padding(start = 12.dp, top = 4.dp, bottom = 4.dp),
                    )
                }
            }
            items(page.rows, key = { it.key }, contentType = { it.javaClass }) { row ->
                val focus = Modifier
                    .rememberedFocus(memory, row.key)
                    .then(if (row.key == entryKey) Modifier.focusRequester(entry) else Modifier)
                when (row) {
                    is BookRow.Part -> TvTrackLine(
                        number = row.number,
                        title = row.title.asString(),
                        subtitle = heard.takeIf { row.heard },
                        duration = row.time,
                        current = current == row.id,
                        playing = current == row.id && playing,
                        highlighted = false,
                        onClick = { model.playPart(row.position) },
                        modifier = focus,
                    )
                    is BookRow.Chapter -> TvTrackLine(
                        number = (row.index + 1).toString(),
                        title = row.title.asString(),
                        subtitle = null,
                        duration = row.time,
                        current = chapterNow == row.index,
                        playing = chapterNow == row.index && playing,
                        highlighted = false,
                        onClick = { model.playChapter(row.startMs) },
                        modifier = focus,
                    )
                }
            }
        }
    }
}

private val TOP = 40.dp
private val ART = 220.dp

/** The book's column: the cover, and its buttons under it (up to four). */
private val COLUMN = 340.dp

internal const val TV_BOOK_PAGE = "pley.tv.book"
internal const val TV_BOOK_ROWS = "pley.tv.book.rows"
