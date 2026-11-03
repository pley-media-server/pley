package app.pley.android.ui.phone.person

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
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.grid.LazyGridScope
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalDensity
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
import app.pley.android.designsystem.image.rememberImagePrefetcher
import app.pley.android.designsystem.tile.ArtworkTileModel
import app.pley.android.designsystem.tile.SkeletonTile
import app.pley.android.designsystem.tile.TileKind
import app.pley.android.designsystem.tile.TileStyle
import app.pley.android.designsystem.tile.rememberSkeleton
import app.pley.android.designsystem.tile.skeleton
import app.pley.android.screens.PleyKey
import app.pley.android.screens.person.CreditTile
import app.pley.android.screens.person.PersonPage
import app.pley.android.screens.person.PersonUiState
import app.pley.android.screens.person.PersonViewModel
import app.pley.android.ui.phone.R
import app.pley.android.ui.phone.kit.EmptyState
import app.pley.android.ui.phone.kit.ErrorState
import app.pley.android.ui.phone.kit.FittedCells
import app.pley.android.ui.phone.kit.LocalPhoneDeviceTier
import app.pley.android.ui.phone.kit.MediaCard
import app.pley.android.ui.phone.kit.PleyTopAppBar
import app.pley.android.ui.phone.kit.fullWidth
import app.pley.android.ui.phone.kit.gridColumns
import app.pley.android.ui.phone.kit.rememberItemGridState
import app.pley.android.ui.phone.shell.LocalPhoneNavigator
import app.pley.android.ui.phone.shell.LocalPhoneShell
import app.pley.android.ui.phone.shell.ReportLoad
import app.pley.android.ui.phone.theme.LocalPhoneMetrics

/**
 * A person's page (PersonPage.tsx; design spec §4.9): the photo, the name
 * and how many of their titles are in the libraries, then their Movies and
 * Shows as poster grids captioned with the year and the role.
 */
@Composable
internal fun PersonScreen(personId: Long) {
    val model: PersonViewModel = viewModel(factory = PersonViewModel.factory(personId))
    val state by model.state.collectAsStateWithLifecycle()
    val navigator = LocalPhoneNavigator.current
    val shell = LocalPhoneShell.current
    Column(Modifier.fillMaxSize().testTag(PERSON_PAGE)) {
        PleyTopAppBar(title = (state as? PersonUiState.Ready)?.page?.name.orEmpty(), onBack = navigator::back)
        when (val s = state) {
            PersonUiState.Loading -> PersonSkeleton()
            PersonUiState.NotFound -> EmptyState(
                title = stringResource(R.string.person_not_found),
                body = stringResource(R.string.person_not_found_body),
                icon = PleyIcons.UserRound,
            )
            is PersonUiState.Failed -> ErrorState(stringResource(R.string.person_failed), s.error, onRetry = model::retry)
            is PersonUiState.Ready -> {
                ReportLoad(s.status)
                PersonContent(s.page, onOpen = { navigator.open(PleyKey.Item(it)) }, onActions = { navigator.open(PleyKey.ItemActions(it)) })
            }
        }
    }
}

@Composable
private fun PersonContent(page: PersonPage, onOpen: (Long) -> Unit, onActions: (Long) -> Unit) {
    val metrics = LocalPhoneMetrics.current
    val gap = PleyPhone.TileGap
    val movies = remember(page) { page.movies.map { it.tile(page.origin) } }
    val shows = remember(page) { page.shows.map { it.tile(page.origin) } }
    val movieTitle = stringResource(R.string.person_movies)
    val showTitle = stringResource(R.string.person_shows)
    // About a screen composed ahead, and the posters of the next 1.5 screens asked for, as a library's grid does.
    val grid = rememberItemGridState()
    val available = metrics.contentWidth - metrics.margin * 2
    val columns = gridColumns(TileKind.Poster, available, gap)
    val width = (available - gap * (columns - 1)) / columns
    val density = LocalDensity.current
    // By the grid's index: the header and each section's title have none.
    val images = remember(movies, shows) {
        buildList<ArtworkImage?> {
            add(null)
            for (section in listOf(movies, shows)) {
                if (section.isEmpty()) continue
                add(null)
                section.forEach { add(it.image) }
            }
        }
    }
    rememberImagePrefetcher(
        state = grid,
        images = images,
        widthPx = with(density) { width.roundToPx() },
        heightPx = with(density) { (width / TileKind.Poster.aspectRatio).roundToPx() },
        tier = LocalPhoneDeviceTier.current,
    )
    LazyVerticalGrid(
        columns = remember(gap) { FittedCells(TileKind.Poster, gap) },
        state = grid,
        modifier = Modifier.fillMaxSize(),
        contentPadding = PaddingValues(start = metrics.marginStart, end = metrics.marginEnd, bottom = 32.dp),
        horizontalArrangement = Arrangement.spacedBy(gap),
        verticalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        fullWidth("header") { PersonHeader(page) }
        section("movies", movieTitle, movies, onOpen, onActions)
        section("shows", showTitle, shows, onOpen, onActions)
    }
}

private fun LazyGridScope.section(
    key: String,
    title: String,
    tiles: List<ArtworkTileModel>,
    onOpen: (Long) -> Unit,
    onActions: (Long) -> Unit,
) {
    if (tiles.isEmpty()) return
    fullWidth("$key-title") {
        Row(Modifier.padding(top = 12.dp), verticalAlignment = Alignment.Bottom) {
            Text(title, style = MaterialTheme.typography.titleLarge, color = PleyColors.Fg, modifier = Modifier.semantics { heading() })
            Spacer(Modifier.width(8.dp))
            Text(tiles.size.toString(), style = MaterialTheme.typography.bodyMedium, color = PleyColors.Subtle)
        }
    }
    items(tiles, key = { "$key-${it.id}" }, contentType = { TileKind.Poster }) { tile ->
        MediaCard(tile, TileKind.Poster, onClick = { onOpen(tile.id) }, onLongClick = { onActions(tile.id) }, modifier = Modifier.fillMaxWidth())
    }
}

@Composable
private fun PersonHeader(page: PersonPage) {
    val photo = remember(page.photoUrl, page.origin) { ArtworkImage.of(page.photoUrl, page.origin) }
    Column(Modifier.fillMaxWidth().padding(vertical = 16.dp), horizontalAlignment = Alignment.CenterHorizontally) {
        Box(Modifier.size(PhotoSize).clip(PleyShape.Circle)) {
            Artwork(photo, page.name, Modifier.matchParentSize())
            if (photo == null) {
                Text(
                    initials(page.name),
                    style = MaterialTheme.typography.displaySmall,
                    color = PleyColors.Fg.copy(alpha = 0.8f),
                    modifier = Modifier.align(Alignment.Center),
                )
            }
        }
        Spacer(Modifier.height(16.dp))
        Text(page.name, style = MaterialTheme.typography.headlineMedium, color = PleyColors.Fg, textAlign = TextAlign.Center, modifier = Modifier.semantics { heading() })
        if (page.counts.isNotEmpty()) {
            Spacer(Modifier.height(6.dp))
            Text(stringResource(R.string.person_in_libraries, page.counts), style = MaterialTheme.typography.bodyMedium, color = PleyColors.Muted)
        }
    }
}

@Composable
private fun PersonSkeleton() {
    val skeleton = rememberSkeleton(animated = true)
    val metrics = LocalPhoneMetrics.current
    Column(Modifier.fillMaxSize().padding(start = metrics.marginStart, end = metrics.marginEnd), horizontalAlignment = Alignment.CenterHorizontally) {
        Spacer(Modifier.height(16.dp))
        Box(Modifier.size(PhotoSize).skeleton(skeleton, PleyShape.Circle))
        Spacer(Modifier.height(16.dp))
        Box(Modifier.fillMaxWidth(0.5f).height(28.dp).skeleton(skeleton, PleyShape.Badge))
        Spacer(Modifier.height(24.dp))
        Row(horizontalArrangement = Arrangement.spacedBy(PleyPhone.TileGap)) {
            repeat(3) { SkeletonTile(skeleton, TileKind.Poster, TileStyle.Phone, Modifier.weight(1f), width = 112.dp) }
        }
    }
}

private fun CreditTile.tile(origin: String): ArtworkTileModel = ArtworkTileModel.of(item, TileKind.Poster, origin).copy(subtitle = subtitle)

/** "Idris Vale" → "IV". */
internal fun initials(name: String): String =
    name.split(' ').filter { it.isNotBlank() }.take(2).joinToString("") { it.first().uppercase() }

private val PhotoSize = 144.dp

internal const val PERSON_PAGE = "pley.person.page"
