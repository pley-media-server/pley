package app.pley.android.ui.phone.music

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.FlowRowScope
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.foundation.lazy.grid.rememberLazyGridState
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalWindowInfo
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyPhone
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.image.Artwork
import app.pley.android.designsystem.image.ArtworkImage
import app.pley.android.designsystem.image.MarkGlyph
import app.pley.android.designsystem.text.formatWords
import app.pley.android.designsystem.tile.ArtworkTileModel
import app.pley.android.designsystem.tile.SkeletonTile
import app.pley.android.designsystem.tile.TileKind
import app.pley.android.designsystem.tile.TileStyle
import app.pley.android.designsystem.tile.rememberSkeleton
import app.pley.android.designsystem.tile.skeleton
import app.pley.android.screens.PleyKey
import app.pley.android.screens.music.ArtistPage
import app.pley.android.screens.music.ArtistViewModel
import app.pley.android.screens.music.MusicEvent
import app.pley.android.screens.music.MusicPageState
import app.pley.android.ui.phone.R
import app.pley.android.ui.phone.kit.EmptyState
import app.pley.android.ui.phone.kit.ErrorState
import app.pley.android.ui.phone.kit.FittedCells
import app.pley.android.ui.phone.kit.MediaCard
import app.pley.android.ui.phone.kit.PleyTextButton
import app.pley.android.ui.phone.kit.PleyTopAppBar
import app.pley.android.ui.phone.kit.PrimaryButton
import app.pley.android.ui.phone.kit.TonalButton
import app.pley.android.ui.phone.kit.fullWidth
import app.pley.android.ui.phone.library.joined
import app.pley.android.ui.phone.player.CarGap
import app.pley.android.ui.phone.player.LocalCarScreen
import app.pley.android.ui.phone.remote.CastTarget
import app.pley.android.ui.phone.remote.PlayOnButton
import app.pley.android.ui.phone.shell.LocalPhoneNavigator
import app.pley.android.ui.phone.shell.LocalPhoneShell
import app.pley.android.ui.phone.shell.ReportLoad
import app.pley.android.ui.phone.theme.LocalPhoneMetrics
import app.pley.android.ui.phone.theme.WidthClass

/**
 * An artist's page (MusicViews.tsx ArtistView; design spec §4.10): the
 * round picture, the name, "4 albums · 40 tracks", Play and Shuffle (every
 * track of theirs), what the server says of them, and their albums,
 * newest first, as square cards.
 */
@Composable
internal fun ArtistScreen(artistId: Long) {
    val model: ArtistViewModel = viewModel(factory = ArtistViewModel.factory(artistId))
    val state by model.state.collectAsStateWithLifecycle()
    val navigator = LocalPhoneNavigator.current
    val shell = LocalPhoneShell.current
    val resources = LocalResources.current
    LaunchedEffect(model) {
        model.events.collect { event ->
            when (event) {
                is MusicEvent.Failed -> shell.report(event.error)
                MusicEvent.NothingToPlay -> shell.toasts.show(resources.getString(R.string.music_nothing_to_play))
                MusicEvent.Played -> Unit
            }
        }
    }
    val grid = rememberLazyGridState()
    Column(Modifier.fillMaxSize().testTag(ARTIST_PAGE)) {
        val ready = state as? MusicPageState.Ready
        val pastHeader by remember { derivedStateOf { grid.firstVisibleItemIndex > 0 } }
        PleyTopAppBar(
            title = if (pastHeader) ready?.page?.title.orEmpty() else "",
            onBack = navigator::back,
            scrolled = { grid.canScrollBackward },
        )
        when (val s = state) {
            MusicPageState.Loading -> MusicHeaderSkeleton(round = true)
            MusicPageState.NotFound -> EmptyState(
                title = stringResource(R.string.music_artist_not_found),
                body = stringResource(R.string.music_not_found_body),
                icon = PleyIcons.Music,
            )
            is MusicPageState.Failed -> ErrorState(stringResource(R.string.music_artist_failed), s.error, onRetry = model::retry)
            is MusicPageState.Ready -> {
                ReportLoad(s.status)
                ArtistContent(
                    s.page,
                    busy = s.busy,
                    grid = grid,
                    onPlay = { model.play(shuffle = false) },
                    onShuffle = { model.play(shuffle = true) },
                    onAlbum = { navigator.open(PleyKey.Album(it)) },
                    onActions = { navigator.open(PleyKey.ItemActions(it)) },
                )
            }
        }
    }
}

@Composable
private fun ArtistContent(
    page: ArtistPage,
    busy: Boolean,
    grid: androidx.compose.foundation.lazy.grid.LazyGridState,
    onPlay: () -> Unit,
    onShuffle: () -> Unit,
    onAlbum: (Long) -> Unit,
    onActions: (Long) -> Unit,
) {
    val metrics = LocalPhoneMetrics.current
    val gap = PleyPhone.TileGap
    val words = formatWords()
    val albums = remember(page, words) {
        page.albums.map { ArtworkTileModel.of(it, TileKind.Square, page.origin, words).copy(subtitle = it.year?.toString().orEmpty()) }
    }
    LazyVerticalGrid(
        columns = remember(gap) { FittedCells(TileKind.Square, gap) },
        state = grid,
        modifier = Modifier.fillMaxSize(),
        contentPadding = PaddingValues(start = metrics.marginStart, end = metrics.marginEnd, bottom = 32.dp),
        horizontalArrangement = Arrangement.spacedBy(gap),
        verticalArrangement = Arrangement.spacedBy(20.dp),
    ) {
        fullWidth("header") {
            val art = remember(page.posterUrl, page.origin) { ArtworkImage.of(page.posterUrl, page.origin) }
            MusicHeader(
                art = art,
                round = true,
                kind = stringResource(R.string.music_artist),
                title = page.title,
                lines = listOf(page.countTexts.joined(LocalResources.current), page.genres.take(3).joinToString(" · ")).filter { it.isNotEmpty() },
                busy = busy,
                onPlay = onPlay,
                onShuffle = onShuffle,
                playOn = CastTarget(itemId = page.id, shuffles = true),
            )
        }
        if (page.summary != null) fullWidth("summary") { Summary(page.summary!!) }
        fullWidth("albums-title") {
            Text(
                stringResource(R.string.music_albums),
                style = MaterialTheme.typography.titleLarge,
                color = PleyColors.Fg,
                modifier = Modifier.padding(top = 8.dp).semantics { heading() },
            )
        }
        if (page.albumsLoading) {
            fullWidth("albums-loading") {
                val skeleton = rememberSkeleton(animated = true)
                Row(horizontalArrangement = Arrangement.spacedBy(gap)) {
                    repeat(3) { SkeletonTile(skeleton, TileKind.Square, TileStyle.Phone, Modifier.weight(1f), width = 112.dp) }
                }
            }
        } else if (albums.isEmpty()) {
            fullWidth("albums-empty") {
                Text(stringResource(R.string.music_no_albums), style = MaterialTheme.typography.bodyMedium, color = PleyColors.Muted)
            }
        }
        items(albums, key = { "album-${it.id}" }, contentType = { TileKind.Square }) { tile ->
            MediaCard(tile, TileKind.Square, onClick = { onAlbum(tile.id) }, onLongClick = { onActions(tile.id) }, modifier = Modifier.fillMaxWidth())
        }
    }
}

/**
 * The top of an artist's or album's page (MusicViews.tsx MusicHeader):
 * the art (round for artists), what it is, the title, its [lines], and
 * Play and Shuffle, with Play on… sending [playOn] beside them while
 * another device takes it; [extra] goes under the lines (the artist's
 * link, the rating). From 600 dp wide the art stands beside the rest, as
 * an item's poster does: on a short, wide screen (a car's, a phone on its
 * side) Play is in view without scrolling. Where the window is short as
 * well ([shortWindow]), the art is smaller and what it is isn't said, so
 * Play and Shuffle stay side by side and above the bar of what plays.
 * [buttons] take the place of Play and Shuffle (a book's).
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
internal fun MusicHeader(
    art: ArtworkImage?,
    round: Boolean,
    kind: String,
    title: String,
    lines: List<String>,
    busy: Boolean,
    onPlay: () -> Unit,
    onShuffle: () -> Unit,
    playEnabled: Boolean = true,
    playOn: CastTarget? = null,
    extra: @Composable () -> Unit = {},
    buttons: (@Composable FlowRowScope.() -> Unit)? = null,
    mark: MarkGlyph = MarkGlyph.Music,
) {
    val wide = LocalPhoneMetrics.current.widthClass != WidthClass.Compact
    val short = wide && shortWindow()
    val cover = @Composable {
        Box(
            Modifier
                .size(if (short) ART_SIZE_SHORT else ART_SIZE)
                .clip(if (round) PleyShape.Circle else PleyShape.Card)
                .background(PleyColors.Surface2),
        ) {
            // An album's or an artist's: music's glyph; a book's, the book's.
            Artwork(art, title, Modifier.matchParentSize(), mark = mark)
        }
    }
    val words = @Composable { centred: Boolean ->
        val align = if (centred) TextAlign.Center else TextAlign.Start
        if (!short) {
            Text(kind, style = MaterialTheme.typography.labelMedium, color = PleyColors.Muted)
            Spacer(Modifier.height(4.dp))
        }
        Text(
            title,
            style = MaterialTheme.typography.headlineMedium,
            color = PleyColors.Fg,
            textAlign = align,
            modifier = Modifier.semantics { heading() },
        )
        extra()
        lines.forEach { line ->
            Spacer(Modifier.height(4.dp))
            Text(line, style = MaterialTheme.typography.bodyMedium, color = PleyColors.Muted, textAlign = align)
        }
        Spacer(Modifier.height(if (short) 12.dp else 20.dp))
        // Shuffle goes under Play when they don't fit side by side (beside the art, a car's larger text, longer
        // words in French or Spanish); side by side, they are as tall as each other. A car's buttons keep 24 dp apart.
        val gap = if (LocalCarScreen.current) CarGap else 12.dp
        FlowRow(
            horizontalArrangement = Arrangement.spacedBy(gap, if (centred) Alignment.CenterHorizontally else Alignment.Start),
            verticalArrangement = Arrangement.spacedBy(gap),
            itemVerticalAlignment = Alignment.CenterVertically,
        ) {
            if (buttons != null) {
                buttons()
                return@FlowRow
            }
            PrimaryButton(
                stringResource(R.string.music_play),
                onClick = onPlay,
                icon = PleyIcons.Play,
                enabled = playEnabled && !busy,
                modifier = Modifier.fillMaxRowHeight(),
            )
            TonalButton(
                stringResource(R.string.music_shuffle),
                onClick = onShuffle,
                icon = PleyIcons.Shuffle,
                enabled = playEnabled && !busy,
                modifier = Modifier.fillMaxRowHeight(),
            )
            if (playOn != null && playEnabled) PlayOnButton(playOn)
        }
    }
    if (wide) {
        Row(Modifier.fillMaxWidth().padding(top = 8.dp, bottom = 8.dp), verticalAlignment = Alignment.CenterVertically) {
            cover()
            Spacer(Modifier.width(24.dp))
            Column(Modifier.weight(1f)) { words(false) }
        }
    } else {
        Column(Modifier.fillMaxWidth().padding(top = 8.dp, bottom = 8.dp), horizontalAlignment = Alignment.CenterHorizontally) {
            cover()
            Spacer(Modifier.height(20.dp))
            words(true)
        }
    }
}

/** What the server says of an artist or album: four lines, More for the rest. */
@Composable
internal fun Summary(text: String) {
    var open by rememberSaveable { mutableStateOf(false) }
    Column(Modifier.fillMaxWidth()) {
        Text(
            text,
            style = MaterialTheme.typography.bodyLarge,
            color = PleyColors.Fg.copy(alpha = 0.85f),
            maxLines = if (open) Int.MAX_VALUE else 4,
            overflow = androidx.compose.ui.text.style.TextOverflow.Ellipsis,
        )
        if (text.length > SUMMARY_FOLD) {
            PleyTextButton(stringResource(if (open) R.string.music_less else R.string.music_more), onClick = { open = !open })
        }
    }
}

/** The header while the page loads, laid out as [MusicHeader] will be. */
@Composable
internal fun MusicHeaderSkeleton(round: Boolean) {
    val skeleton = rememberSkeleton(animated = true)
    val wide = LocalPhoneMetrics.current.widthClass != WidthClass.Compact
    val cover = Modifier.size(if (wide && shortWindow()) ART_SIZE_SHORT else ART_SIZE).skeleton(skeleton, if (round) PleyShape.Circle else PleyShape.Card)
    if (wide) {
        Row(Modifier.fillMaxWidth().padding(top = 8.dp), verticalAlignment = Alignment.CenterVertically) {
            Box(cover)
            Spacer(Modifier.width(24.dp))
            Column {
                Box(Modifier.size(200.dp, 28.dp).skeleton(skeleton, PleyShape.Badge))
                Spacer(Modifier.height(12.dp))
                Box(Modifier.size(140.dp, 16.dp).skeleton(skeleton, PleyShape.Badge))
            }
        }
    } else {
        Column(Modifier.fillMaxWidth().padding(top = 8.dp), horizontalAlignment = Alignment.CenterHorizontally) {
            Box(cover)
            Spacer(Modifier.height(24.dp))
            Box(Modifier.size(200.dp, 28.dp).skeleton(skeleton, PleyShape.Badge))
            Spacer(Modifier.height(12.dp))
            Box(Modifier.size(140.dp, 16.dp).skeleton(skeleton, PleyShape.Badge))
        }
    }
}

private val ART_SIZE = 200.dp

/** The art beside the title in a short window. */
private val ART_SIZE_SHORT = 144.dp

/** Windows less tall than this are short: a phone on its side, a car's low or split screen. */
private val SHORT_BELOW = 480.dp

/** Whether the window is short ([SHORT_BELOW]), in the dp the page is drawn in (a car's are a third larger). */
@Composable
internal fun shortWindow(): Boolean {
    val height = LocalWindowInfo.current.containerSize.height
    return with(LocalDensity.current) { height.toDp() } < SHORT_BELOW
}

/** Summaries longer than this get More. */
private const val SUMMARY_FOLD = 220

internal const val ARTIST_PAGE = "pley.artist.page"
