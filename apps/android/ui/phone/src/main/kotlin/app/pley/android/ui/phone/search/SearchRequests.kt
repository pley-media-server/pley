package app.pley.android.ui.phone.search

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyPhone
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.tile.SkeletonTile
import app.pley.android.designsystem.tile.TileKind
import app.pley.android.designsystem.tile.TileStyle
import app.pley.android.designsystem.tile.rememberSkeleton
import app.pley.android.screens.PleyKey
import app.pley.android.screens.R
import app.pley.android.screens.requests.RequestTexts
import app.pley.android.screens.search.TmdbSection
import app.pley.android.ui.phone.kit.MediaCard
import app.pley.android.ui.phone.kit.PleyTextButton
import app.pley.android.ui.phone.kit.TonalButton
import app.pley.android.ui.phone.kit.rememberHubRowState
import app.pley.android.ui.phone.kit.rowCardWidth
import app.pley.android.ui.phone.requests.problemBody
import app.pley.android.ui.phone.requests.problemTitle
import app.pley.android.ui.phone.requests.rememberRequestTexts
import app.pley.android.ui.phone.requests.tileId
import app.pley.android.ui.phone.requests.titleTile
import app.pley.android.ui.phone.shell.LocalPhoneNavigator
import app.pley.android.ui.phone.theme.LocalPhoneMetrics

/*
 * Discover in search: the way to Discover before typing,
 * and the Discover row after the libraries' results, with See all.
 */

/** Discover's way in, at the top of search before typing: "Discover", what it's for, and a chevron. */
@Composable
internal fun DiscoverEntry() {
    val navigator = LocalPhoneNavigator.current
    val metrics = LocalPhoneMetrics.current
    Row(
        Modifier
            .padding(start = metrics.marginStart, end = metrics.marginEnd, top = 8.dp, bottom = 8.dp)
            .fillMaxWidth()
            .clip(PleyShape.Card)
            .background(PleyColors.Surface)
            .clickable(role = Role.Button) { navigator.open(PleyKey.Discover) }
            .padding(horizontal = 16.dp, vertical = 14.dp)
            .testTag(SEARCH_DISCOVER),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(PleyIcons.Compass, contentDescription = null, size = 24.dp, tint = PleyColors.AccentBright)
        Spacer(Modifier.width(16.dp))
        Column(Modifier.weight(1f)) {
            Text(stringResource(R.string.requests_discover), style = MaterialTheme.typography.bodyLarge, color = PleyColors.Fg)
            Text(stringResource(R.string.requests_discover_hint), style = MaterialTheme.typography.bodyMedium, color = PleyColors.Muted)
        }
        Spacer(Modifier.width(12.dp))
        Icon(PleyIcons.ChevronRight, contentDescription = null, size = 20.dp, tint = PleyColors.Subtle)
    }
}

/**
 * The Discover row: TMDB's first titles for [query] that no library has, as
 * posters with their status badges, each opening its Discover page; See
 * all opens every title TMDB finds for the same words.
 * Skeleton cards while TMDB is asked; why, when it can't be searched,
 * with Retry ([onRetry]) where trying again may help.
 */
@Composable
internal fun OnTmdb(query: String, tmdb: TmdbSection, onRetry: () -> Unit, modifier: Modifier = Modifier) {
    val navigator = LocalPhoneNavigator.current
    val metrics = LocalPhoneMetrics.current
    val texts = rememberRequestTexts()
    val width = rowCardWidth(TileKind.Poster, metrics.contentWidth - metrics.margin * 2)
    Column(modifier.fillMaxWidth().testTag(SEARCH_TMDB)) {
        Row(
            Modifier.padding(start = metrics.marginStart, end = metrics.marginEnd - 8.dp).heightIn(min = 32.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(
                stringResource(R.string.requests_search_row),
                style = MaterialTheme.typography.titleLarge,
                color = PleyColors.Fg,
                modifier = Modifier.weight(1f).semantics { heading() },
            )
            if (tmdb !is TmdbSection.Failed) {
                PleyTextButton(stringResource(R.string.requests_see_all), onClick = { navigator.open(PleyKey.DiscoverResults(query)) })
            }
        }
        Spacer(Modifier.height(PleyPhone.HeaderToContent - TMDB_PRESS_ROOM))
        val padding = PaddingValues(start = metrics.marginStart, end = metrics.marginEnd, top = TMDB_PRESS_ROOM, bottom = TMDB_PRESS_ROOM)
        when (tmdb) {
            is TmdbSection.Ready -> {
                val tiles = remember(tmdb, texts) { tmdb.titles.map { it to titleTile(it, tmdb.origin, texts) } }
                LazyRow(
                    state = rememberHubRowState(),
                    modifier = Modifier.fillMaxWidth(),
                    contentPadding = padding,
                    horizontalArrangement = Arrangement.spacedBy(PleyPhone.TileGap),
                ) {
                    items(tiles, key = { it.first.tileId }, contentType = { TileKind.Poster }) { (title, tile) ->
                        MediaCard(
                            model = tile,
                            kind = TileKind.Poster,
                            onClick = { navigator.open(PleyKey.DiscoverTitle(title.kind, title.tmdbId)) },
                            modifier = Modifier.width(width),
                        )
                    }
                }
            }
            is TmdbSection.Failed -> TmdbProblem(tmdb, texts, onRetry, Modifier.padding(padding))
            else -> {
                val skeleton = rememberSkeleton(animated = true)
                Row(Modifier.padding(padding), horizontalArrangement = Arrangement.spacedBy(PleyPhone.TileGap)) {
                    repeat(TMDB_SKELETONS) { SkeletonTile(skeleton, TileKind.Poster, TileStyle.Phone, width = width) }
                }
            }
        }
    }
}

/** Why TMDB couldn't be searched ("TMDB isn't answering"), the line under it, and Retry where trying again may help. */
@Composable
private fun TmdbProblem(failed: TmdbSection.Failed, texts: RequestTexts, onRetry: () -> Unit, modifier: Modifier = Modifier) {
    val problem = failed.problem
    Column(modifier.fillMaxWidth().semantics { liveRegion = LiveRegionMode.Polite }) {
        Text(
            problemTitle(problem, failed.summary, texts, stringResource(R.string.requests_search_failed)),
            style = MaterialTheme.typography.bodyLarge,
            color = PleyColors.Fg,
        )
        problemBody(problem, failed.summary, texts)?.let { body ->
            Text(body, style = MaterialTheme.typography.bodyMedium, color = PleyColors.Muted)
        }
        if (problem.retry) {
            Spacer(Modifier.height(12.dp))
            TonalButton(stringResource(R.string.requests_retry), onClick = onRetry, icon = PleyIcons.RefreshCw)
        }
    }
}

/** The libraries have nothing: "Nothing in your libraries matches", then what TMDB has. */
@Composable
internal fun OnlyOnTmdb(query: String, tmdb: TmdbSection, onRetry: () -> Unit) {
    val metrics = LocalPhoneMetrics.current
    LazyColumn(
        modifier = Modifier.fillMaxSize().testTag(SEARCH_RESULTS),
        contentPadding = PaddingValues(top = 8.dp, bottom = 32.dp),
        verticalArrangement = Arrangement.spacedBy(PleyPhone.SectionGap - 8.dp),
    ) {
        item(key = "none", contentType = "none") {
            Text(
                stringResource(R.string.requests_nothing_in_libraries),
                style = MaterialTheme.typography.bodyMedium,
                color = PleyColors.Muted,
                modifier = Modifier.padding(start = metrics.marginStart, end = metrics.marginEnd),
            )
        }
        item(key = "tmdb", contentType = "tmdb") { OnTmdb(query, tmdb, onRetry) }
    }
}

/** Room above and below the cards for a pressed card's growth and ring, as the other rows keep. */
private val TMDB_PRESS_ROOM = 6.dp

private const val TMDB_SKELETONS = 4

internal const val SEARCH_DISCOVER = "pley.search.discover"
internal const val SEARCH_TMDB = "pley.search.tmdb"
