package app.pley.android.ui.phone.music

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.image.ArtworkImage
import app.pley.android.designsystem.image.MarkGlyph
import app.pley.android.designsystem.text.asString
import app.pley.android.screens.R as ScreensR
import app.pley.android.screens.music.BookPage
import app.pley.android.screens.music.BookRow
import app.pley.android.screens.music.BookViewModel
import app.pley.android.screens.music.MusicEvent
import app.pley.android.screens.music.MusicPageState
import app.pley.android.ui.phone.kit.EmptyState
import app.pley.android.ui.phone.kit.ErrorState
import app.pley.android.ui.phone.kit.PleyTextButton
import app.pley.android.ui.phone.kit.PleyTopAppBar
import app.pley.android.ui.phone.kit.PrimaryButton
import app.pley.android.ui.phone.kit.TonalButton
import app.pley.android.ui.phone.library.joined
import app.pley.android.ui.phone.shell.LocalPhoneNavigator
import app.pley.android.ui.phone.shell.LocalPhoneShell
import app.pley.android.ui.phone.shell.ReportLoad
import app.pley.android.ui.phone.theme.LocalPhoneMetrics

/**
 * An audiobook's page (BookView.tsx): the cover, its series, who wrote and
 * reads it, its length and parts, how far the profile is in it, Play (where
 * it was left, on any device), From the start, Mark as finished, the
 * summary, then its parts, or the chapters of a book in one file. The part
 * or chapter playing is marked; tapping a part plays the book from it, a
 * chapter from its start. It plays in the app's music, with Now Playing's
 * book controls.
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
internal fun BookScreen(bookId: Long) {
    val model: BookViewModel = viewModel(factory = BookViewModel.factory(bookId))
    val state by model.state.collectAsStateWithLifecycle()
    val chapterNow by model.chapterNow.collectAsStateWithLifecycle()
    val navigator = LocalPhoneNavigator.current
    val shell = LocalPhoneShell.current
    val resources = LocalResources.current
    LaunchedEffect(model) {
        model.events.collect { event ->
            when (event) {
                is MusicEvent.Failed -> shell.report(event.error)
                MusicEvent.NothingToPlay -> shell.toasts.show(resources.getString(ScreensR.string.book_nothing_to_play))
                MusicEvent.Played -> Unit
            }
        }
    }
    val list = rememberLazyListState()
    Column(Modifier.fillMaxSize().testTag(BOOK_PAGE)) {
        val ready = state as? MusicPageState.Ready
        val pastHeader by remember { derivedStateOf { list.firstVisibleItemIndex > 0 } }
        PleyTopAppBar(
            title = if (pastHeader) ready?.page?.title.orEmpty() else "",
            onBack = navigator::back,
            scrolled = { list.canScrollBackward },
        )
        when (val s = state) {
            MusicPageState.Loading -> MusicHeaderSkeleton(round = false)
            MusicPageState.NotFound -> EmptyState(
                title = stringResource(ScreensR.string.book_not_found),
                body = stringResource(app.pley.android.ui.phone.R.string.music_not_found_body),
                icon = PleyIcons.BookHeadphones,
            )
            is MusicPageState.Failed -> ErrorState(stringResource(ScreensR.string.book_failed), s.error, onRetry = model::retry)
            is MusicPageState.Ready -> {
                ReportLoad(s.status)
                val page = s.page
                val metrics = LocalPhoneMetrics.current
                LazyColumn(
                    state = list,
                    modifier = Modifier.fillMaxSize(),
                    contentPadding = PaddingValues(start = metrics.marginStart - 8.dp, end = metrics.marginEnd - 8.dp, bottom = 32.dp),
                    horizontalAlignment = Alignment.CenterHorizontally,
                ) {
                    item(key = "header", contentType = "header") {
                        val art = remember(page.posterUrl, page.origin) { ArtworkImage.of(page.posterUrl, page.origin) }
                        Box(Modifier.padding(horizontal = 8.dp)) {
                            MusicHeader(
                                art = art,
                                round = false,
                                kind = page.kicker.asString(),
                                title = page.title,
                                lines = listOfNotNull(
                                    page.byline?.asString(),
                                    page.readBy?.asString(),
                                    page.metaTexts.joined(resources).ifEmpty { null },
                                ),
                                busy = s.busy,
                                onPlay = model::play,
                                onShuffle = {},
                                extra = { BookProgressLine(page) },
                                mark = MarkGlyph.Audiobooks,
                                buttons = {
                                    val playingThis = s.current != null
                                    PrimaryButton(
                                        stringResource(
                                            when {
                                                playingThis && s.playing -> ScreensR.string.book_pause
                                                page.started || playingThis -> ScreensR.string.book_resume
                                                page.finished -> ScreensR.string.book_listen_again
                                                else -> ScreensR.string.book_play
                                            },
                                        ),
                                        onClick = model::play,
                                        icon = if (playingThis && s.playing) PleyIcons.Pause else PleyIcons.Play,
                                        enabled = !s.busy && (page.partsLoading || page.parts.isNotEmpty()),
                                        modifier = Modifier.fillMaxRowHeight(),
                                    )
                                    if (page.started) {
                                        TonalButton(
                                            stringResource(ScreensR.string.book_from_start),
                                            onClick = model::fromStart,
                                            icon = PleyIcons.RotateCcw,
                                            modifier = Modifier.fillMaxRowHeight(),
                                        )
                                    }
                                    PleyTextButton(
                                        stringResource(if (page.finished) ScreensR.string.book_mark_unfinished else ScreensR.string.book_mark_finished),
                                        onClick = { model.setFinished(!page.finished) },
                                        enabled = page.parts.isNotEmpty(),
                                    )
                                },
                            )
                        }
                    }
                    if (page.summary != null) {
                        item(key = "summary", contentType = "summary") {
                            Box(Modifier.widthIn(max = MAX_WIDTH).padding(horizontal = 8.dp, vertical = 8.dp)) { Summary(page.summary!!) }
                        }
                    }
                    if (page.rows.isNotEmpty()) {
                        item(key = "heading", contentType = "heading") {
                            val chapters = page.rows.first() is BookRow.Chapter
                            Box(Modifier.widthIn(max = MAX_WIDTH).fillMaxWidth()) {
                                Text(
                                    stringResource(if (chapters) ScreensR.string.book_chapters else ScreensR.string.book_parts),
                                    style = MaterialTheme.typography.titleMedium,
                                    color = PleyColors.Fg,
                                    modifier = Modifier.padding(start = 8.dp, top = 16.dp, bottom = 4.dp),
                                )
                            }
                        }
                    }
                    items(page.rows, key = { it.key }, contentType = { it.javaClass }) { row ->
                        Box(Modifier.widthIn(max = MAX_WIDTH).fillMaxWidth()) {
                            when (row) {
                                is BookRow.Part -> BookLine(
                                    number = row.number,
                                    title = row.title.asString(),
                                    time = row.time,
                                    fraction = row.fraction,
                                    heard = row.heard,
                                    current = s.current == row.id,
                                    playing = s.current == row.id && s.playing,
                                    onClick = { model.playPart(row.position) },
                                )
                                is BookRow.Chapter -> BookLine(
                                    number = (row.index + 1).toString(),
                                    title = row.title.asString(),
                                    time = row.time,
                                    fraction = 0f,
                                    heard = false,
                                    current = chapterNow == row.index,
                                    playing = chapterNow == row.index && s.playing,
                                    onClick = { model.playChapter(row.startMs) },
                                )
                            }
                        }
                    }
                }
            }
        }
    }
}

/** How far the profile is in a book: a bar, "37 % heard · 7h 2m left", and the part Play goes on from. */
@Composable
private fun BookProgressLine(page: BookPage) {
    val palette = PleyColors.palette
    val progress = page.progress ?: return
    Spacer(Modifier.height(12.dp))
    Box(
        Modifier
            .widthIn(max = 280.dp)
            .fillMaxWidth()
            .height(4.dp)
            .drawBehind {
                val r = CornerRadius(size.height / 2f)
                drawRoundRect(palette.fg.copy(alpha = 0.2f), cornerRadius = r)
                drawRoundRect(palette.accent, size = Size(size.width * progress.fraction, size.height), cornerRadius = r)
            },
    )
    Spacer(Modifier.height(6.dp))
    Text(
        listOf(progress.heard.asString(), progress.left).filter { it.isNotEmpty() }.joinToString(" · "),
        style = MaterialTheme.typography.bodySmall,
        color = PleyColors.Muted,
    )
    page.upTo?.let {
        Spacer(Modifier.height(2.dp))
        Text(it.asString(), style = MaterialTheme.typography.bodySmall, color = PleyColors.Muted)
    }
}

/**
 * A part or chapter of a book: its number, or the playing glyph for the one
 * playing; its title, with a bar of how far a part is heard; its length (a
 * chapter: where it starts), and a check once a part is heard to its end.
 */
@Composable
private fun BookLine(
    number: String,
    title: String,
    time: String,
    fraction: Float,
    heard: Boolean,
    current: Boolean,
    playing: Boolean,
    onClick: () -> Unit,
) {
    val palette = PleyColors.palette
    Row(
        Modifier
            .fillMaxWidth()
            .heightIn(min = 56.dp)
            .clip(PleyShape.Field)
            .background(if (current) PleyColors.Fg.copy(alpha = 0.05f) else Color.Transparent)
            .semantics { selected = current }
            .clickable(role = Role.Button, onClick = onClick)
            .padding(horizontal = 8.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(Modifier.width(32.dp), contentAlignment = Alignment.CenterEnd) {
            if (current) {
                Icon(if (playing) PleyIcons.AudioLines else PleyIcons.Play, contentDescription = null, size = 18.dp, tint = PleyColors.Accent)
            } else {
                Text(number, style = MaterialTheme.typography.bodyMedium, color = PleyColors.Subtle, textAlign = TextAlign.End)
            }
        }
        Spacer(Modifier.width(16.dp))
        Column(Modifier.weight(1f)) {
            Text(
                title,
                style = MaterialTheme.typography.titleMedium,
                color = if (current) PleyColors.AccentBright else PleyColors.Fg,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            if (fraction > 0f) {
                Spacer(Modifier.height(6.dp))
                Box(
                    Modifier
                        .width(160.dp)
                        .height(3.dp)
                        .drawBehind {
                            val r = CornerRadius(size.height / 2f)
                            drawRoundRect(palette.fg.copy(alpha = 0.2f), cornerRadius = r)
                            drawRoundRect(palette.accent, size = Size(size.width * fraction, size.height), cornerRadius = r)
                        },
                )
            }
        }
        Spacer(Modifier.width(12.dp))
        if (heard) {
            Icon(PleyIcons.Check, contentDescription = stringResource(ScreensR.string.book_heard), size = 16.dp, tint = PleyColors.Accent)
            Spacer(Modifier.width(8.dp))
        }
        Text(time, style = MaterialTheme.typography.bodyMedium, color = PleyColors.Muted)
    }
}

private val MAX_WIDTH = 720.dp

internal const val BOOK_PAGE = "pley.book.page"
