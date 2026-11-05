package app.pley.android.ui.phone.downloads

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import app.pley.android.data.DownloadEntry
import app.pley.android.data.DownloadState
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.image.Artwork
import app.pley.android.designsystem.image.ArtworkImage
import app.pley.android.designsystem.text.asString
import app.pley.android.designsystem.text.formatWords
import app.pley.android.model.ItemKind
import app.pley.android.model.format.episodeLabel
import app.pley.android.model.format.formatBytes
import app.pley.android.model.format.formatRuntime
import app.pley.android.screens.PleyKey
import app.pley.android.screens.downloads.DownloadGroup
import app.pley.android.screens.downloads.DownloadsViewModel
import app.pley.android.ui.phone.R
import app.pley.android.ui.phone.kit.EmptyState
import app.pley.android.ui.phone.kit.LocalOfflineAction
import app.pley.android.ui.phone.kit.PleyTopAppBar
import app.pley.android.ui.phone.shell.LocalPhoneNavigator
import app.pley.android.ui.phone.theme.LocalPhoneMetrics

/**
 * What was downloaded to this device, to watch without the server: the
 * space it takes, then movies and shows (their episodes in order), the
 * newest first. A download that is whole plays at a tap; its menu deletes
 * it, or cancels one still coming; a failed one is tried again at a tap.
 */
@Composable
internal fun DownloadsScreen() {
    val model: DownloadsViewModel = viewModel(factory = DownloadsViewModel.factory)
    val groups by model.groups.collectAsStateWithLifecycle()
    val storage by model.storage.collectAsStateWithLifecycle()
    val navigator = LocalPhoneNavigator.current
    val metrics = LocalPhoneMetrics.current
    val list = rememberLazyListState()
    Column(Modifier.fillMaxSize().testTag(DOWNLOADS_PAGE)) {
        // Already on the downloads: the offline banner leaves out its way to them.
        CompositionLocalProvider(LocalOfflineAction provides null) {
            PleyTopAppBar(title = stringResource(R.string.downloads_title), onBack = navigator::back, scrolled = { list.canScrollBackward })
        }
        if (groups.isEmpty()) {
            EmptyState(
                title = stringResource(R.string.downloads_empty),
                body = stringResource(R.string.downloads_empty_body),
                icon = PleyIcons.Download,
            )
            return@Column
        }
        LazyColumn(
            state = list,
            modifier = Modifier.fillMaxSize(),
            contentPadding = PaddingValues(start = metrics.marginStart, end = metrics.marginEnd, bottom = 32.dp),
        ) {
            item(key = "storage", contentType = "storage") {
                val words = formatWords()
                Text(
                    stringResource(
                        R.string.downloads_storage,
                        formatBytes(storage.usedBytes, words),
                        formatBytes(storage.freeBytes, words),
                    ),
                    style = MaterialTheme.typography.bodyMedium,
                    color = PleyColors.Muted,
                    modifier = Modifier.padding(top = 4.dp, bottom = 8.dp),
                )
            }
            groups.forEach { group ->
                val showId = group.showId
                if (showId != null) {
                    item(key = "${group.key}-header", contentType = "show") {
                        ShowHeader(group, onOpen = { navigator.open(PleyKey.Item(showId)) })
                    }
                }
                items(group.entries, key = { "download-${it.itemId}" }, contentType = { "download" }) { entry ->
                    DownloadRow(
                        entry = entry,
                        episode = group.showId != null,
                        onPlay = model::play,
                        onRetry = { model.retry(it.itemId) },
                        onRemove = { model.remove(it.itemId) },
                        onOpen = { navigator.open(PleyKey.Item(it.itemId)) },
                    )
                }
            }
        }
    }
}

/** A show's name over its episodes: "3 episodes · 4.2 GB"; it opens the show. */
@Composable
private fun ShowHeader(group: DownloadGroup, onOpen: () -> Unit) {
    Column(
        Modifier
            .fillMaxWidth()
            .padding(top = 16.dp)
            .clip(PleyShape.Card)
            .clickable(role = Role.Button, onClick = onOpen)
            .padding(vertical = 6.dp),
    ) {
        Text(
            group.title,
            style = MaterialTheme.typography.titleMedium,
            color = PleyColors.Fg,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.semantics { heading() },
        )
        Text(
            pluralStringResource(
                R.plurals.downloads_episodes,
                group.entries.size,
                group.entries.size,
                formatBytes(group.sizeBytes, formatWords()),
            ),
            style = MaterialTheme.typography.bodySmall,
            color = PleyColors.Muted,
        )
    }
}

/**
 * One download: its picture (16:9), its title and where it is, a bar
 * while it comes, and its button. Tapping plays it when it is whole, else
 * opens what can be done with it (a failed one tried again or deleted).
 */
@Composable
private fun DownloadRow(
    entry: DownloadEntry,
    episode: Boolean,
    onPlay: (DownloadEntry) -> Unit,
    onRetry: (DownloadEntry) -> Unit,
    onRemove: (DownloadEntry) -> Unit,
    onOpen: (DownloadEntry) -> Unit,
) {
    var menu by remember { mutableStateOf(false) }
    val words = formatWords()
    val title = if (episode) listOf(episodeLabel(entry.seasonIndex, entry.episodeIndex, words), entry.title).filter { it.isNotEmpty() }.joinToString(" · ") else entry.title
    val size = formatBytes(entry.sizeBytes, words)
    val line = when (entry.state) {
        DownloadState.Done -> when {
            entry.watched -> stringResource(R.string.downloads_watched, size)
            entry.positionMs > 0 && (entry.durationMs ?: 0) > entry.positionMs ->
                stringResource(R.string.downloads_left, size, formatRuntime((entry.durationMs ?: 0) - entry.positionMs, words))
            else -> listOfNotNull(entry.year?.takeIf { entry.kind == ItemKind.Movie }?.toString(), size).joinToString(" · ")
        }
        DownloadState.Downloading -> stringResource(R.string.downloads_progress, (entry.fraction * 100).toInt(), size)
        DownloadState.Queued -> entry.converting?.let { stringResource(R.string.downloads_preparing, (it * 100).toInt()) }
            ?: stringResource(R.string.downloads_queued)
        DownloadState.Waiting -> stringResource(if (entry.serverAway) R.string.downloads_waiting_server else R.string.downloads_waiting)
        DownloadState.Failed -> stringResource(R.string.downloads_failed, entry.error?.asString().orEmpty())
    }
    Row(
        Modifier
            .fillMaxWidth()
            .heightIn(min = 72.dp)
            .clip(PleyShape.Card)
            .clickable(role = Role.Button) {
                if (entry.state == DownloadState.Done) onPlay(entry) else menu = true
            }
            .padding(vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(Modifier.width(112.dp).aspectRatio(16f / 9f).clip(PleyShape.TilePhone)) {
            Artwork(entry.imageUri?.let { ArtworkImage(listOf(it)) }, entry.title, Modifier.fillMaxSize())
        }
        Spacer(Modifier.width(12.dp))
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.Center) {
            Text(title, style = MaterialTheme.typography.titleSmall, color = PleyColors.Fg, maxLines = 2, overflow = TextOverflow.Ellipsis)
            Spacer(Modifier.height(2.dp))
            Text(
                line,
                style = MaterialTheme.typography.bodySmall,
                color = if (entry.state == DownloadState.Failed) PleyColors.DangerText else PleyColors.Muted,
                maxLines = 2,
                overflow = TextOverflow.Ellipsis,
            )
            val converting = entry.converting
            if (entry.state == DownloadState.Downloading || converting != null) {
                Spacer(Modifier.height(6.dp))
                ProgressLine(converting ?: entry.fraction)
            }
        }
        Box {
            DownloadIconButton(entry = entry, title = entry.title, onDownload = {}, onMore = { menu = true })
            DownloadMenu(
                entry,
                expanded = menu,
                onDismiss = { menu = false },
                onPlay = onPlay,
                onRetry = onRetry,
                onRemove = onRemove,
                onOpen = onOpen,
            )
        }
    }
}

/** A 3 dp bar: accent for what is down, fg 20 % for the rest. */
@Composable
private fun ProgressLine(fraction: Float) {
    Box(
        Modifier
            .fillMaxWidth()
            .height(3.dp)
            .drawBehind {
                val r = CornerRadius(size.height / 2f)
                drawRoundRect(PleyColors.Fg.copy(alpha = 0.2f), cornerRadius = r)
                drawRoundRect(PleyColors.Accent, size = Size(size.width * fraction.coerceIn(0f, 1f), size.height), cornerRadius = r)
            },
    )
}

/** The Downloads page's root, for tests. */
const val DOWNLOADS_PAGE = "pley.downloads"
