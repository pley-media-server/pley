package app.pley.android.ui.tv.playlists

import androidx.compose.foundation.focusGroup
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
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.itemsIndexed
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import app.pley.android.data.Loadable
import app.pley.android.data.valueOrNull
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyTv
import app.pley.android.designsystem.PleyType
import app.pley.android.designsystem.TvSafeArea
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.tile.SkeletonTile
import app.pley.android.designsystem.tile.TileCaption
import app.pley.android.designsystem.tile.TileKind
import app.pley.android.designsystem.tile.TileStyle
import app.pley.android.designsystem.tile.rememberSkeleton
import app.pley.android.designsystem.tile.tvTileSize
import app.pley.android.screens.PleyKey
import app.pley.android.screens.playlists.PlaylistCard
import app.pley.android.screens.playlists.PlaylistsViewModel
import app.pley.android.ui.tv.R
import app.pley.android.ui.tv.kit.LocalTvRailFocus
import app.pley.android.ui.tv.kit.TvPillButton
import app.pley.android.ui.tv.kit.TvTileFrame
import app.pley.android.ui.tv.kit.initialFocus
import app.pley.android.ui.tv.kit.railFade
import app.pley.android.ui.tv.kit.railOnLeftStopOnRight
import app.pley.android.ui.tv.kit.rememberFocusMemory
import app.pley.android.ui.tv.kit.rememberedFocus
import app.pley.android.ui.tv.library.TvMessage
import app.pley.android.ui.tv.shell.LocalTvNavigator
import app.pley.android.ui.tv.shell.failedBody
import app.pley.android.ui.tv.shell.failedTitle

/**
 * The playlists page on TV (the web's PlaylistsPage; design spec §4.9):
 * "Playlists" with New playlist at the right safe edge, then a grid of
 * square covers (a mosaic of the first four entries), five across,
 * captioned with the title and "12 items · 3h 12m". Focus starts on the
 * first cover, or comes back to the one opened.
 */
@Composable
internal fun TvPlaylistsScreen() {
    val vm: PlaylistsViewModel = viewModel(factory = PlaylistsViewModel.Factory)
    val state by vm.state.collectAsStateWithLifecycle()
    TvPlaylistsContent(state, onRetry = vm::retry)
}

@Composable
internal fun TvPlaylistsContent(state: Loadable<List<PlaylistCard>>, onRetry: () -> Unit) {
    val navigator = LocalTvNavigator.current
    val rail = LocalTvRailFocus.current
    val memory = rememberFocusMemory()
    val list = state.valueOrNull
    val newPlaylist = { navigator.open(PleyKey.NewPlaylist()) }
    Column(Modifier.fillMaxSize().testTag(TV_PLAYLISTS)) {
        Row(
            Modifier.fillMaxWidth().padding(start = PleyTv.Gutter, end = TvSafeArea.Horizontal, top = TvSafeArea.Vertical),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            BasicText(
                stringResource(R.string.tv_playlists_title),
                style = PleyType.Tv.PageTitle.copy(color = PleyColors.Fg),
                modifier = Modifier.weight(1f).semantics { heading() },
            )
            if (list != null || state !is Loadable.Failed) {
                // Keyed by whether there are playlists: with none, a new button that takes focus as it is placed.
                key(list?.isEmpty()) {
                    Box(Modifier.railOnLeftStopOnRight(rail).focusGroup()) {
                        TvPillButton(
                            stringResource(R.string.tv_playlists_new),
                            onClick = newPlaylist,
                            icon = PleyIcons.Plus,
                            modifier = Modifier.rememberedFocus(memory, "new", initial = list?.isEmpty() == true),
                        )
                    }
                }
            }
        }
        when {
            list == null && state is Loadable.Failed -> TvMessage(
                title = failedTitle(state.error, stringResource(R.string.tv_playlists_failed)),
                body = failedBody(state.error),
                action = stringResource(R.string.tv_playlists_retry),
                onAction = onRetry,
                top = 40.dp,
            )
            list == null -> Skeletons()
            list.isEmpty() -> Column(
                Modifier.padding(start = PleyTv.Gutter, top = 40.dp, end = TvSafeArea.Horizontal),
                verticalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                BasicText(stringResource(R.string.tv_playlists_none_title), style = PleyType.Tv.Section.copy(color = PleyColors.Fg))
                BasicText(stringResource(R.string.tv_playlists_none_body), style = PleyType.Tv.Body.copy(color = PleyColors.Muted))
            }
            else -> LazyVerticalGrid(
                columns = GridCells.Fixed(COLUMNS),
                modifier = Modifier.fillMaxSize().railFade().railOnLeftStopOnRight(rail).testTag(TV_PLAYLISTS_GRID),
                contentPadding = PaddingValues(start = PleyTv.Gutter, end = TvSafeArea.Horizontal, top = 28.dp, bottom = 48.dp),
                horizontalArrangement = Arrangement.spacedBy(PleyTv.TileGap),
                verticalArrangement = Arrangement.spacedBy(22.dp),
            ) {
                itemsIndexed(list, key = { _, card -> card.id }, contentType = { _, _ -> "playlist" }) { index, card ->
                    val subtitle = listOfNotNull(card.meta.ifEmpty { null }, smartWord(card)).joinToString(" · ")
                    TvTileFrame(
                        kind = TileKind.Square,
                        onClick = { navigator.open(PleyKey.Playlist(card.id)) },
                        onLongClick = null,
                        // What the card says to screen readers: its caption, which (as every tile's) says nothing itself.
                        focusModifier = Modifier
                            .rememberedFocus(memory, "playlist/${card.id}", initial = index == 0)
                            .semantics { contentDescription = listOf(card.title, subtitle).filter { it.isNotEmpty() }.joinToString(", ") },
                        caption = { focused -> TileCaption(card.title, subtitle, TileStyle.Tv, emphasized = focused) },
                    ) {
                        TvPlaylistArt(card.artwork, card.origin, card.video, card.title, Modifier.tvTileSize(TileKind.Square))
                    }
                }
            }
        }
    }
}

@Composable
private fun smartWord(card: PlaylistCard): String? = if (card.smart) stringResource(R.string.tv_playlists_smart) else null

@Composable
private fun Skeletons() {
    val skeleton = rememberSkeleton(animated = false)
    Row(Modifier.padding(start = PleyTv.Gutter, top = 28.dp), horizontalArrangement = Arrangement.spacedBy(PleyTv.TileGap)) {
        repeat(COLUMNS) { SkeletonTile(skeleton, TileKind.Square, TileStyle.Tv) }
    }
    Spacer(Modifier.height(22.dp))
}

/** Five 150 dp covers, 16 apart, from the gutter (x = 96) to 894. */
private const val COLUMNS = 5

internal const val TV_PLAYLISTS = "pley.tv.playlists"
internal const val TV_PLAYLISTS_GRID = "pley.tv.playlists.grid"
