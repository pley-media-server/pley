package app.pley.android.ui.tv.search

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyTv
import app.pley.android.designsystem.PleyType
import app.pley.android.designsystem.TvSafeArea
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.tile.TileKind
import app.pley.android.model.DiscoverTitle
import app.pley.android.screens.R as ScreensR
import app.pley.android.screens.requests.RequestTexts
import app.pley.android.screens.search.TmdbSection
import app.pley.android.ui.tv.kit.FocusMemory
import app.pley.android.ui.tv.kit.SeeAll
import app.pley.android.ui.tv.kit.TvPillButton
import app.pley.android.ui.tv.kit.TvRow
import app.pley.android.ui.tv.requests.RowSkeleton
import app.pley.android.ui.tv.requests.problemBody
import app.pley.android.ui.tv.requests.problemTitle
import app.pley.android.ui.tv.requests.rememberRequestTexts
import app.pley.android.ui.tv.requests.tileId
import app.pley.android.ui.tv.requests.titleTile

/*
 * Discover in search, as the Samsung and LG apps have it (packages/tv's
 * search.tsx): after the libraries' results, TMDB's first titles that no
 * library has, with See all; why, when TMDB can't be searched.
 */

/** The Discover row ([TvTmdbRow]) as an item of the results' column. */
internal fun LazyListScope.tmdbRow(
    tmdb: TmdbSection,
    memory: FocusMemory,
    focus: FocusRequester,
    onRetry: () -> Unit,
    onOpen: (DiscoverTitle) -> Unit,
    onAll: (String) -> Unit,
) {
    item(key = TMDB_KEY, contentType = TMDB_KEY) { TvTmdbRow(tmdb, memory, focus, onRetry, onOpen, onAll) }
}

/**
 * The Discover row: TMDB's titles for the words as posters with their
 * status badges, each opening its title's page ([onOpen]), then See all
 * ([onAll]) for every title TMDB finds for them; a still row while TMDB
 * is asked; or why it can't be searched, with Retry ([onRetry]) where
 * trying again may help. [focus] finds whatever it shows that takes focus.
 */
@Composable
internal fun TvTmdbRow(
    tmdb: TmdbSection,
    memory: FocusMemory,
    focus: FocusRequester,
    onRetry: () -> Unit,
    onOpen: (DiscoverTitle) -> Unit,
    onAll: (String) -> Unit,
) {
    val texts = rememberRequestTexts()
    val title = stringResource(ScreensR.string.requests_search_row)
    val modifier = Modifier.focusRequester(focus).testTag(TV_SEARCH_TMDB)
    when (tmdb) {
        is TmdbSection.Ready -> {
            val tiles = remember(tmdb, texts) { tmdb.titles.map { titleTile(it, tmdb.origin, texts) } }
            val byId = remember(tmdb) { tmdb.titles.associateBy { it.tileId } }
            val seeAllLabel = stringResource(ScreensR.string.requests_see_all)
            val seeAll = remember(tmdb.query, seeAllLabel) { SeeAll(seeAllLabel) { onAll(tmdb.query) } }
            TvRow(
                rowKey = TMDB_KEY,
                title = title,
                tiles = tiles,
                kind = TileKind.Poster,
                onOpen = { tile -> byId[tile.id]?.let(onOpen) },
                modifier = modifier,
                memory = memory,
                seeAll = seeAll,
            )
        }
        is TmdbSection.Failed -> TmdbProblem(title, tmdb, texts, onRetry, modifier)
        else -> Column(modifier) { RowSkeleton(title) }
    }
}

/** Why TMDB couldn't be searched ("TMDB isn't answering"), the line under it, and Retry where trying again may help. */
@Composable
private fun TmdbProblem(title: String, failed: TmdbSection.Failed, texts: RequestTexts, onRetry: () -> Unit, modifier: Modifier) {
    val problem = failed.problem
    Column(modifier.fillMaxWidth().padding(start = PleyTv.Gutter, end = TvSafeArea.Horizontal)) {
        BasicText(title, style = PleyType.Tv.Section.copy(color = PleyColors.Fg), maxLines = 1, modifier = Modifier.semantics { heading() })
        Spacer(Modifier.height(PleyTv.HeaderToTiles))
        Column(Modifier.widthIn(max = ProblemWidth).semantics(mergeDescendants = true) { liveRegion = LiveRegionMode.Polite }) {
            BasicText(
                problemTitle(problem, failed.summary, texts, stringResource(ScreensR.string.requests_search_failed)),
                style = PleyType.Tv.Title.copy(color = PleyColors.Fg),
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            problemBody(problem, failed.summary, texts)?.let { body ->
                Spacer(Modifier.height(4.dp))
                BasicText(body, style = PleyType.Tv.Body.copy(color = PleyColors.Muted), maxLines = 2, overflow = TextOverflow.Ellipsis)
            }
        }
        if (problem.retry) {
            Spacer(Modifier.height(16.dp))
            TvPillButton(stringResource(ScreensR.string.requests_retry), onClick = onRetry, icon = PleyIcons.RefreshCw)
        }
    }
}

/** The libraries found nothing: the line saying so, above Discover's row. */
@Composable
internal fun NothingInLibraries(modifier: Modifier = Modifier) {
    BasicText(
        stringResource(ScreensR.string.requests_nothing_in_libraries),
        style = PleyType.Tv.Body.copy(color = PleyColors.Muted),
        maxLines = 1,
        modifier = modifier.padding(start = PleyTv.Gutter, end = TvSafeArea.Horizontal),
    )
}

/** The Discover row's key among the results' rows. */
internal const val TMDB_KEY = "Tmdb"

/** The line's height: Discover's row comes a row's gap under it. */
internal val NothingHeight = 20.dp

private val ProblemWidth = 560.dp

internal const val TV_SEARCH_TMDB = "pley.tv.search.tmdb"
