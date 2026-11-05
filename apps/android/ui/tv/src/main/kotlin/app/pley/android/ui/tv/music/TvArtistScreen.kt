package app.pley.android.ui.tv.music

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
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
import app.pley.android.designsystem.text.asString
import app.pley.android.designsystem.text.formatWords
import app.pley.android.designsystem.tile.ArtworkTileModel
import app.pley.android.designsystem.tile.TileKind
import app.pley.android.screens.PleyKey
import app.pley.android.screens.music.ArtistPage
import app.pley.android.screens.music.ArtistViewModel
import app.pley.android.screens.music.MusicEvent
import app.pley.android.screens.music.MusicPageState
import app.pley.android.ui.tv.R
import app.pley.android.ui.tv.item.tileActions
import app.pley.android.ui.tv.kit.BackdropScrim
import app.pley.android.ui.tv.kit.TvBackdrop
import app.pley.android.ui.tv.kit.TvPillButton
import app.pley.android.ui.tv.kit.TvRoundButton
import app.pley.android.ui.tv.kit.TvRow
import app.pley.android.ui.tv.kit.rememberFocusMemory
import app.pley.android.ui.tv.kit.rememberedFocus
import app.pley.android.ui.tv.shell.LocalTvNavigator
import app.pley.android.ui.tv.shell.failedBody
import app.pley.android.ui.tv.shell.failedTitle

/**
 * An artist on TV (MusicViews.tsx ArtistView; design spec §4.10): their
 * backdrop, the round picture, the name, "4 albums · 40 tracks" and their
 * genres; Play and Shuffle (every track of theirs, which opens Now
 * Playing); then their albums, newest first, as a row of square tiles.
 * Focus starts on Play; Down goes to the albums.
 */
@Composable
internal fun TvArtistScreen(artistId: Long) {
    val model: ArtistViewModel = viewModel(factory = ArtistViewModel.factory(artistId))
    val state by model.state.collectAsStateWithLifecycle()
    val navigator = LocalTvNavigator.current
    val actions = tileActions()
    var problem by remember { mutableStateOf<UiText?>(null) }
    LaunchedEffect(model) {
        model.events.collect { event ->
            when (event) {
                MusicEvent.Played -> navigator.open(PleyKey.NowPlaying)
                MusicEvent.NothingToPlay -> problem = UiText.res(R.string.tv_music_nothing_to_play)
                is MusicEvent.Failed -> problem = errorSentence(event.error)
            }
        }
    }
    Box(Modifier.fillMaxSize().testTag(TV_ARTIST_PAGE)) {
        when (val s = state) {
            MusicPageState.Loading -> ArtistSkeleton()
            MusicPageState.NotFound -> MusicMessage(
                title = stringResource(R.string.tv_music_artist_not_found),
                body = stringResource(R.string.tv_music_not_found_body),
                action = stringResource(R.string.tv_music_back),
                onAction = navigator::back,
            )
            is MusicPageState.Failed -> MusicMessage(
                title = failedTitle(s.error, stringResource(R.string.tv_music_artist_failed)),
                body = failedBody(s.error),
                action = stringResource(R.string.tv_music_retry),
                onAction = model::retry,
            )
            is MusicPageState.Ready -> ArtistContent(
                s.page,
                busy = s.busy,
                musicPlaying = s.current != null,
                problem = problem,
                onPlay = { model.play(shuffle = false) },
                onShuffle = { model.play(shuffle = true) },
                onAlbum = { navigator.open(PleyKey.Album(it)) },
                onActions = { actions.open(it, continueWatching = false) },
                onNowPlaying = { navigator.open(PleyKey.NowPlaying) },
            )
        }
    }
}

@Composable
private fun ArtistContent(
    page: ArtistPage,
    busy: Boolean,
    musicPlaying: Boolean,
    problem: UiText?,
    onPlay: () -> Unit,
    onShuffle: () -> Unit,
    onAlbum: (Long) -> Unit,
    onActions: (Long) -> Unit,
    onNowPlaying: () -> Unit,
) {
    val memory = rememberFocusMemory()
    val backdrop = remember(page.backdropUrl, page.origin) { ArtworkImage.of(page.backdropUrl, page.origin) }
    val picture = remember(page.posterUrl, page.origin) { ArtworkImage.of(page.posterUrl, page.origin) }
    val words = formatWords()
    val albums = remember(page, words) {
        page.albums.map { ArtworkTileModel.of(it, TileKind.Square, page.origin, words).copy(subtitle = it.year?.toString().orEmpty()) }
    }
    TvBackdrop(backdrop, page.title, scrim = BackdropScrim.Item)
    Column(Modifier.fillMaxSize().padding(top = TOP)) {
        Row(Modifier.padding(start = PleyTv.Gutter, end = TvSafeArea.Horizontal), verticalAlignment = Alignment.CenterVertically) {
            Box(Modifier.size(PICTURE).clip(PleyShape.Circle).background(PleyColors.Surface2)) {
                Artwork(picture, page.title, Modifier.matchParentSize())
            }
            Spacer(Modifier.width(28.dp))
            Column(Modifier.widthIn(max = 520.dp)) {
                BasicText(stringResource(R.string.tv_music_artist), style = PleyType.Tv.Caption.copy(color = PleyColors.Muted))
                BasicText(page.title, style = PleyType.Tv.PageTitle.copy(color = PleyColors.Fg), maxLines = 2, overflow = TextOverflow.Ellipsis)
                val meta = listOf(page.counts, page.genres.take(3).joinToString(" · ")).filter { it.isNotEmpty() }.joinToString("  ·  ")
                if (meta.isNotEmpty()) BasicText(meta, style = PleyType.Tv.Label.copy(color = PleyColors.Muted), modifier = Modifier.padding(top = 6.dp))
                page.summary?.let {
                    BasicText(it, style = PleyType.Tv.Body.copy(color = PleyColors.Fg.copy(alpha = 0.85f)), maxLines = 2, overflow = TextOverflow.Ellipsis, modifier = Modifier.padding(top = 8.dp))
                }
            }
        }
        Spacer(Modifier.height(18.dp))
        Row(
            Modifier.padding(start = PleyTv.Gutter).height(PleyTv.ButtonHeight + PleyTv.ButtonLabelRoom),
            horizontalArrangement = Arrangement.spacedBy(PleyTv.ButtonGap),
        ) {
            TvPillButton(stringResource(R.string.tv_music_play), onClick = onPlay, icon = PleyIcons.Play, enabled = !busy, modifier = Modifier.rememberedFocus(memory, "play", initial = true))
            TvRoundButton(stringResource(R.string.tv_music_shuffle), PleyIcons.Shuffle, onClick = onShuffle, enabled = !busy, modifier = Modifier.rememberedFocus(memory, "shuffle"))
            if (musicPlaying) TvRoundButton(stringResource(R.string.tv_music_now_playing), PleyIcons.AudioLines, onClick = onNowPlaying, modifier = Modifier.rememberedFocus(memory, "now"))
            if (problem != null) BasicText(problem.asString(), style = PleyType.Tv.Caption.copy(color = PleyColors.DangerText), modifier = Modifier.padding(start = 8.dp, top = 14.dp))
        }
        if (albums.isEmpty()) {
            BasicText(
                stringResource(if (page.albumsLoading) R.string.tv_music_loading else R.string.tv_music_no_albums),
                style = PleyType.Tv.Body.copy(color = PleyColors.Muted),
                modifier = Modifier.padding(start = PleyTv.Gutter, top = 16.dp),
            )
        } else {
            TvRow(
                rowKey = "albums",
                title = stringResource(R.string.tv_music_albums),
                tiles = albums,
                kind = TileKind.Square,
                onOpen = { onAlbum(it.id) },
                onActions = { onActions(it.id) },
                memory = memory,
                modifier = Modifier.fillMaxWidth().padding(top = 4.dp),
            )
        }
    }
}

@Composable
private fun ArtistSkeleton() {
    Column(Modifier.padding(start = PleyTv.Gutter, top = TOP)) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Box(Modifier.size(PICTURE).background(PleyColors.Surface2, PleyShape.Circle))
            Spacer(Modifier.width(28.dp))
            Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                Box(Modifier.size(280.dp, 36.dp).background(PleyColors.Surface2, PleyShape.Badge))
                Box(Modifier.size(200.dp, 18.dp).background(PleyColors.Surface2, PleyShape.Badge))
            }
        }
    }
}

private val TOP = 40.dp
private val PICTURE = 120.dp

internal const val TV_ARTIST_PAGE = "pley.tv.artist"
