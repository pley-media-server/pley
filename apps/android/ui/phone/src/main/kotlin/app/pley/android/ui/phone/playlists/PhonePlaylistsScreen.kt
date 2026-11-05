package app.pley.android.ui.phone.playlists

import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.foundation.lazy.grid.rememberLazyGridState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import app.pley.android.data.Loadable
import app.pley.android.data.valueOrNull
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyPhone
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.reducedMotion
import app.pley.android.designsystem.tile.rememberSkeleton
import app.pley.android.designsystem.tile.skeleton
import app.pley.android.screens.PleyKey
import app.pley.android.screens.playlists.PlaylistCard
import app.pley.android.screens.playlists.PlaylistsViewModel
import app.pley.android.ui.phone.R
import app.pley.android.ui.phone.kit.EmptyState
import app.pley.android.ui.phone.kit.ErrorState
import app.pley.android.ui.phone.kit.PleyIconButton
import app.pley.android.ui.phone.kit.PleyTopAppBar
import app.pley.android.ui.phone.kit.PrimaryButton
import app.pley.android.ui.phone.kit.cardPress
import app.pley.android.ui.phone.library.joined
import app.pley.android.ui.phone.shell.LocalPhoneNavigator
import app.pley.android.ui.phone.shell.LocalPhoneShell
import app.pley.android.ui.phone.shell.ReportLoad
import app.pley.android.ui.phone.theme.LocalPhoneMetrics
import kotlin.math.floor
import kotlin.math.max

/**
 * The Playlists tab (the web's PlaylistsPage): "Playlists" with New
 * playlist, then a grid of square covers (a mosaic of the first four
 * entries) with the title and "12 items · 3h 12m" under each. Empty, a
 * page that says how to make one.
 */
@Composable
internal fun PhonePlaylistsScreen() {
    val vm: PlaylistsViewModel = viewModel(factory = PlaylistsViewModel.Factory)
    val state by vm.state.collectAsStateWithLifecycle()
    val shell = LocalPhoneShell.current
    ReportLoad(state)
    PlaylistsContent(state, onRetry = vm::retry)
}

@Composable
internal fun PlaylistsContent(state: Loadable<List<PlaylistCard>>, onRetry: () -> Unit) {
    val navigator = LocalPhoneNavigator.current
    val list = state.valueOrNull
    val newPlaylist = { navigator.open(PleyKey.NewPlaylist()) }
    val grid = rememberLazyGridState()
    Column(Modifier.fillMaxSize().testTag(PLAYLISTS_PAGE)) {
        PleyTopAppBar(
            title = stringResource(R.string.playlists_title),
            scrolled = { grid.canScrollBackward },
            actions = {
                if (!list.isNullOrEmpty()) PleyIconButton(PleyIcons.Plus, stringResource(R.string.playlists_new), onClick = newPlaylist)
            },
        )
        Box(Modifier.weight(1f).fillMaxWidth()) {
            when {
                list == null && state is Loadable.Failed -> Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState())) {
                    ErrorState(stringResource(R.string.playlists_failed), state.error, onRetry = onRetry)
                }
                list == null -> Skeletons()
                list.isEmpty() -> Column(
                    Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(top = 32.dp),
                    horizontalAlignment = Alignment.CenterHorizontally,
                ) {
                    EmptyState(
                        title = stringResource(R.string.playlists_none_title),
                        body = stringResource(R.string.playlists_none_body),
                        icon = PleyIcons.ListPlus,
                        action = { PrimaryButton(stringResource(R.string.playlists_new), onClick = newPlaylist, icon = PleyIcons.Plus) },
                    )
                }
                else -> Grid(list, grid) { navigator.open(PleyKey.Playlist(it.id)) }
            }
        }
    }
}

@Composable
private fun Grid(list: List<PlaylistCard>, state: androidx.compose.foundation.lazy.grid.LazyGridState, onOpen: (PlaylistCard) -> Unit) {
    val metrics = LocalPhoneMetrics.current
    val columns = playlistColumns(metrics.contentWidth - metrics.margin * 2)
    LazyVerticalGrid(
        columns = androidx.compose.foundation.lazy.grid.GridCells.Fixed(columns),
        state = state,
        modifier = Modifier.fillMaxSize().testTag(PLAYLISTS_GRID),
        contentPadding = PaddingValues(start = metrics.marginStart, end = metrics.marginEnd, top = 8.dp, bottom = 32.dp),
        horizontalArrangement = Arrangement.spacedBy(PleyPhone.TileGap),
        verticalArrangement = Arrangement.spacedBy(20.dp),
    ) {
        items(list, key = { it.id }, contentType = { "playlist" }) { card -> PlaylistCardView(card, onClick = { onOpen(card) }) }
    }
}

/** A playlist's card: its cover, which grows with the accent ring while pressed, and its two lines. */
@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun PlaylistCardView(card: PlaylistCard, onClick: () -> Unit) {
    val interactions = remember { MutableInteractionSource() }
    val reduced = reducedMotion()
    val meta = card.metaTexts.joined(LocalResources.current)
    Column(
        Modifier
            .fillMaxWidth()
            .semantics(mergeDescendants = true) { contentDescription = "${card.title}, $meta" }
            .combinedClickable(interactionSource = interactions, indication = null, role = Role.Button, onClick = onClick),
    ) {
        PlaylistArt(
            artwork = card.artwork,
            origin = card.origin,
            video = card.video,
            title = card.title,
            modifier = Modifier.cardPress(interactions, PleyShape.TilePhone, reduced).fillMaxWidth().aspectRatio(1f),
            glyph = 48.dp,
        )
        Spacer(Modifier.height(PleyPhone.ArtToCaption))
        Text(card.title, style = MaterialTheme.typography.titleSmall, color = PleyColors.Fg, maxLines = 1, overflow = TextOverflow.Ellipsis)
        Text(
            listOfNotNull(meta.ifEmpty { null }, if (card.smart) stringResource(R.string.playlists_smart) else null).joinToString(" · "),
            style = MaterialTheme.typography.bodySmall,
            color = PleyColors.Muted,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
    }
}

@Composable
private fun Skeletons() {
    val skeleton = rememberSkeleton(animated = true)
    val metrics = LocalPhoneMetrics.current
    val columns = playlistColumns(metrics.contentWidth - metrics.margin * 2)
    LazyVerticalGrid(
        columns = androidx.compose.foundation.lazy.grid.GridCells.Fixed(columns),
        modifier = Modifier.fillMaxSize(),
        contentPadding = PaddingValues(start = metrics.marginStart, end = metrics.marginEnd, top = 8.dp),
        horizontalArrangement = Arrangement.spacedBy(PleyPhone.TileGap),
        verticalArrangement = Arrangement.spacedBy(20.dp),
        userScrollEnabled = false,
    ) {
        items(6, contentType = { "skeleton" }) {
            Column {
                Box(Modifier.fillMaxWidth().aspectRatio(1f).skeleton(skeleton, PleyShape.TilePhone))
                Spacer(Modifier.height(PleyPhone.ArtToCaption))
                Box(Modifier.fillMaxWidth(0.7f).height(12.dp).skeleton(skeleton, PleyShape.Badge))
                Spacer(Modifier.height(6.dp))
                Box(Modifier.fillMaxWidth(0.4f).height(10.dp).skeleton(skeleton, PleyShape.Badge))
            }
        }
    }
}

/** Covers of at least 150 dp (the web's 180, less for phones), two columns at least. */
internal fun playlistColumns(available: Dp, gap: Dp = PleyPhone.TileGap): Int =
    max(2, floor((available + gap) / (CARD_MIN + gap)).toInt())

private val CARD_MIN = 150.dp

internal const val PLAYLISTS_PAGE = "pley.playlists"
internal const val PLAYLISTS_GRID = "pley.playlists.grid"
