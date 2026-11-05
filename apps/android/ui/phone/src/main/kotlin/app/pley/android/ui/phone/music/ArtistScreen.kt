package app.pley.android.ui.phone.music

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
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
import app.pley.android.ui.phone.remote.CastTarget
import app.pley.android.ui.phone.remote.PlayOnButton
import app.pley.android.ui.phone.shell.LocalPhoneNavigator
import app.pley.android.ui.phone.shell.LocalPhoneShell
import app.pley.android.ui.phone.shell.ReportLoad
import app.pley.android.ui.phone.theme.LocalPhoneMetrics

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
    val albums = remember(page) {
        page.albums.map { ArtworkTileModel.of(it, TileKind.Square, page.origin).copy(subtitle = it.year?.toString().orEmpty()) }
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
 * link, the rating).
 */
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
) {
    Column(Modifier.fillMaxWidth().padding(top = 8.dp, bottom = 8.dp), horizontalAlignment = Alignment.CenterHorizontally) {
        Box(
            Modifier
                .size(ART_SIZE)
                .clip(if (round) PleyShape.Circle else PleyShape.Card)
                .background(PleyColors.Surface2),
        ) {
            Artwork(art, title, Modifier.matchParentSize())
        }
        Spacer(Modifier.height(20.dp))
        Text(kind, style = MaterialTheme.typography.labelMedium, color = PleyColors.Muted)
        Spacer(Modifier.height(4.dp))
        Text(
            title,
            style = MaterialTheme.typography.headlineMedium,
            color = PleyColors.Fg,
            textAlign = TextAlign.Center,
            modifier = Modifier.semantics { heading() },
        )
        extra()
        lines.forEach { line ->
            Spacer(Modifier.height(4.dp))
            Text(line, style = MaterialTheme.typography.bodyMedium, color = PleyColors.Muted, textAlign = TextAlign.Center)
        }
        Spacer(Modifier.height(20.dp))
        Row(horizontalArrangement = Arrangement.spacedBy(12.dp), verticalAlignment = Alignment.CenterVertically) {
            PrimaryButton(stringResource(R.string.music_play), onClick = onPlay, icon = PleyIcons.Play, enabled = playEnabled && !busy)
            TonalButton(stringResource(R.string.music_shuffle), onClick = onShuffle, icon = PleyIcons.Shuffle, enabled = playEnabled && !busy)
            if (playOn != null && playEnabled) PlayOnButton(playOn)
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

/** The header while the page loads. */
@Composable
internal fun MusicHeaderSkeleton(round: Boolean) {
    val skeleton = rememberSkeleton(animated = true)
    Column(Modifier.fillMaxWidth().padding(top = 8.dp), horizontalAlignment = Alignment.CenterHorizontally) {
        Box(Modifier.size(ART_SIZE).skeleton(skeleton, if (round) PleyShape.Circle else PleyShape.Card))
        Spacer(Modifier.height(24.dp))
        Box(Modifier.size(200.dp, 28.dp).skeleton(skeleton, PleyShape.Badge))
        Spacer(Modifier.height(12.dp))
        Box(Modifier.size(140.dp, 16.dp).skeleton(skeleton, PleyShape.Badge))
    }
}

private val ART_SIZE = 200.dp

/** Summaries longer than this get More. */
private const val SUMMARY_FOLD = 220

internal const val ARTIST_PAGE = "pley.artist.page"
