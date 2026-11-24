package app.pley.android.ui.phone.account

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.image.Artwork
import app.pley.android.designsystem.image.ArtworkImage
import app.pley.android.designsystem.image.MarkGlyph
import app.pley.android.designsystem.tile.rememberSkeleton
import app.pley.android.designsystem.tile.skeleton
import app.pley.android.screens.account.HistoryRow
import app.pley.android.screens.account.HistoryUiState
import app.pley.android.screens.account.HistoryViewModel
import app.pley.android.ui.phone.R
import app.pley.android.ui.phone.kit.DelayedSpinner
import app.pley.android.ui.phone.kit.EmptyState
import app.pley.android.ui.phone.kit.ErrorState
import app.pley.android.ui.phone.kit.PleyIconButton
import app.pley.android.ui.phone.kit.PleyTopAppBar
import app.pley.android.ui.phone.kit.TonalButton
import app.pley.android.ui.phone.shell.LocalPhoneNavigator
import app.pley.android.ui.phone.shell.LocalPhoneShell
import app.pley.android.ui.phone.shell.ReportLoad
import app.pley.android.ui.phone.theme.LocalPhoneMetrics

/**
 * Watch history (the web's HistoryList): plays newest first, each with its
 * picture, what it is, when and on which device; tapping one opens it,
 * the cross removes it at once. Show more loads 15 more.
 */
@Composable
internal fun HistoryScreen() {
    val model: HistoryViewModel = viewModel(factory = HistoryViewModel.factory)
    val state by model.state.collectAsStateWithLifecycle()
    val navigator = LocalPhoneNavigator.current
    val shell = LocalPhoneShell.current
    LaunchedEffect(model) { model.errors.collect(shell::report) }
    val list = rememberLazyListState()
    Column(Modifier.fillMaxSize().testTag(HISTORY_PAGE)) {
        PleyTopAppBar(title = stringResource(R.string.account_history), onBack = navigator::back, scrolled = { list.canScrollBackward })
        when (val s = state) {
            HistoryUiState.Loading -> HistorySkeleton()
            is HistoryUiState.Failed -> ErrorState(stringResource(R.string.account_history_failed), s.error, onRetry = model::retry)
            is HistoryUiState.Ready -> {
                ReportLoad(s.status)
                if (s.rows.isEmpty()) {
                    EmptyState(
                        title = stringResource(R.string.account_history_empty_title),
                        body = stringResource(R.string.account_history_empty),
                        icon = PleyIcons.History,
                    )
                } else {
                    val metrics = LocalPhoneMetrics.current
                    LazyColumn(
                        state = list,
                        modifier = Modifier.fillMaxSize(),
                        contentPadding = PaddingValues(start = metrics.marginStart - 8.dp, end = metrics.marginEnd - 8.dp, bottom = 32.dp),
                        horizontalAlignment = Alignment.CenterHorizontally,
                    ) {
                        items(s.rows, key = { it.entryId }, contentType = { "row" }) { row ->
                            HistoryLine(
                                row,
                                s.origin,
                                onOpen = { navigator.open(row.opens) },
                                onRemove = { model.remove(row.entryId) },
                                modifier = Modifier.animateItem(),
                            )
                        }
                        if (s.canLoadMore) {
                            item(key = "more", contentType = "more") {
                                Box(Modifier.fillMaxWidth().padding(top = 16.dp), contentAlignment = Alignment.Center) {
                                    if (s.loadingMore) {
                                        DelayedSpinner(stringResource(R.string.account_history_loading), delayMs = 0)
                                    } else {
                                        TonalButton(stringResource(R.string.account_history_more), onClick = model::loadMore)
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun HistoryLine(row: HistoryRow, origin: String, onOpen: () -> Unit, onRemove: () -> Unit, modifier: Modifier = Modifier) {
    val art = remember(row.artPaths, origin) { ArtworkImage(row.artPaths.map { absolute(it, origin) }) }
    Row(
        modifier
            .widthIn(max = 640.dp)
            .fillMaxWidth()
            .heightIn(min = 72.dp)
            .clip(PleyShape.Card)
            .clickable(role = Role.Button, onClick = onOpen)
            .padding(start = 8.dp, top = 8.dp, bottom = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(
            Modifier
                .width(40.dp)
                .height(if (row.square) 40.dp else 60.dp)
                .clip(PleyShape.Badge)
                .background(PleyColors.Surface2),
        ) {
            Artwork(art, row.title, Modifier.matchParentSize(), mark = MarkGlyph.of(row.kind))
        }
        Spacer(Modifier.width(14.dp))
        Column(Modifier.weight(1f)) {
            Text(row.title, style = MaterialTheme.typography.titleMedium, color = PleyColors.Fg, maxLines = 1, overflow = TextOverflow.Ellipsis)
            val line = listOf(row.subtitle, row.time, row.device.orEmpty()).filter { it.isNotEmpty() }.joinToString(" · ")
            Text(line, style = MaterialTheme.typography.bodySmall, color = PleyColors.Subtle, maxLines = 2, overflow = TextOverflow.Ellipsis)
        }
        PleyIconButton(PleyIcons.X, stringResource(R.string.account_history_remove, row.title), onClick = onRemove, tint = PleyColors.Muted)
    }
}

@Composable
private fun HistorySkeleton() {
    val skeleton = rememberSkeleton(animated = true)
    val metrics = LocalPhoneMetrics.current
    Column(Modifier.fillMaxWidth().padding(start = metrics.marginStart, end = metrics.marginEnd, top = 8.dp, bottom = 8.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        repeat(6) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Box(Modifier.size(40.dp, 60.dp).skeleton(skeleton, PleyShape.Badge))
                Spacer(Modifier.width(14.dp))
                Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
                    Box(Modifier.size(180.dp, 16.dp).skeleton(skeleton, PleyShape.Badge))
                    Box(Modifier.size(240.dp, 12.dp).skeleton(skeleton, PleyShape.Badge))
                }
            }
        }
    }
}

/** A path from the server made absolute on [origin]. */
internal fun absolute(path: String, origin: String): String =
    if (path.startsWith("http://") || path.startsWith("https://")) path else origin.trimEnd('/') + "/" + path.trimStart('/')

internal const val HISTORY_PAGE = "pley.history.page"
