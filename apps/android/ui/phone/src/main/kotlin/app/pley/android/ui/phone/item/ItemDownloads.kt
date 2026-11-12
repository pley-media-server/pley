package app.pley.android.ui.phone.item

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.Stable
import androidx.compose.runtime.State
import androidx.compose.runtime.compositionLocalOf
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import app.pley.android.common.errorText
import app.pley.android.common.resolve
import app.pley.android.data.DownloadEntry
import app.pley.android.data.DownloadState
import app.pley.android.data.Downloads
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.text.formatWords
import app.pley.android.model.format.formatBytes
import app.pley.android.screens.PleyGraphOwner
import app.pley.android.screens.PleyKey
import app.pley.android.screens.downloads.DownloadMessage
import app.pley.android.screens.downloads.DownloadsViewModel
import app.pley.android.ui.phone.R
import app.pley.android.ui.phone.downloads.DownloadGlyph
import app.pley.android.ui.phone.downloads.DownloadMenu
import app.pley.android.ui.phone.downloads.downloadDescription
import app.pley.android.ui.phone.downloads.rememberDownloadStarter
import app.pley.android.ui.phone.shell.LocalPhoneNavigator
import app.pley.android.ui.phone.shell.LocalPhoneShell

/**
 * The item page's downloads: each item's (read per row, so a download's
 * progress redraws its own row only) and what to do with them. `null`
 * where nothing downloads: TVs, and screens in tests.
 */
@Stable
internal class ItemDownloads(
    private val byItem: State<Map<Long, DownloadEntry>>,
    val download: (List<Long>) -> Unit,
    val retry: (Long) -> Unit,
    val remove: (Long) -> Unit,
    val play: (DownloadEntry) -> Unit,
) {
    /** The download of [itemId], as it changes. */
    @Composable
    fun of(itemId: Long): DownloadEntry? {
        val entry by remember(itemId) { derivedStateOf { byItem.value[itemId] } }
        return entry
    }

    /** Whether all of [itemIds] are downloaded or coming. */
    fun hasAll(itemIds: List<Long>): Boolean = itemIds.all { it in byItem.value }
}

internal val LocalItemDownloads = compositionLocalOf<ItemDownloads?> { null }

/**
 * Makes the page's [ItemDownloads] from the screen's own DownloadsViewModel,
 * saying when one starts (with the way to Downloads) and when one can't.
 */
@Composable
internal fun rememberItemDownloads(): ItemDownloads? {
    // Screens in tests run without the app's graph: no downloads there.
    val owner = LocalContext.current.applicationContext as? PleyGraphOwner ?: return null
    if (owner.screensGraph.downloads === Downloads.None) return null
    val model: DownloadsViewModel = viewModel(factory = DownloadsViewModel.factory)
    val shell = LocalPhoneShell.current
    val navigator = LocalPhoneNavigator.current
    val resources = LocalResources.current
    val byItem = model.byItem.collectAsStateWithLifecycle()
    LaunchedEffect(model) {
        model.events.collect { event ->
            when (event) {
                is DownloadMessage.Failed -> shell.toasts.show(
                    resources.getString(R.string.downloads_failed_toast, errorText(event.error).resolve(resources)),
                    error = true,
                )
            }
        }
    }
    val start = rememberDownloadStarter { ids ->
        model.download(ids)
        shell.toasts.show(
            resources.getString(R.string.downloads_started),
            actionLabel = resources.getString(R.string.downloads_started_action),
            onAction = { navigator.open(PleyKey.Downloads) },
        )
    }
    return remember(model, byItem, start) { ItemDownloads(byItem, start, model::retry, model::remove, model::play) }
}

/**
 * Download beside Play, a 48 dp square like Play on…: the arrow, a ring
 * filling as it comes, a tick once it is here (its menu plays or deletes
 * it) or a warning when it failed (its menu says why, tries it again or
 * deletes it). [DownloadStatus] says the rest under Play.
 */
@Composable
internal fun DownloadButton(itemId: Long, title: String, modifier: Modifier = Modifier) {
    val downloads = LocalItemDownloads.current ?: return
    val entry = downloads.of(itemId)
    var menu by remember { mutableStateOf(false) }
    val description = downloadDescription(entry, title)
    Box(modifier) {
        Box(
            Modifier
                .size(48.dp)
                .clip(PleyShape.Control)
                .background(PleyColors.Surface3)
                .clickable(role = Role.Button) {
                    if (entry == null) downloads.download(listOf(itemId)) else menu = true
                }
                .semantics { contentDescription = description },
            contentAlignment = Alignment.Center,
        ) {
            DownloadGlyph(entry)
        }
        DownloadMenu(
            entry,
            expanded = menu,
            onDismiss = { menu = false },
            onPlay = downloads.play,
            onRetry = { downloads.retry(it.itemId) },
            onRemove = { downloads.remove(it.itemId) },
        )
    }
}

/**
 * One line under Play while a download is coming or failed: "Downloading ·
 * 45 % of 1.4 GB", "Waiting for Wi-Fi", "Didn't download". None when there
 * is no download or it is here: the button's tick says that.
 */
@Composable
internal fun DownloadStatus(itemId: Long) {
    val entry = LocalItemDownloads.current?.of(itemId) ?: return
    val label = when (entry.state) {
        DownloadState.Done -> return
        DownloadState.Failed -> stringResource(R.string.downloads_failed_short)
        DownloadState.Queued -> entry.converting?.let { stringResource(R.string.downloads_preparing, (it * 100).toInt()) }
            ?: stringResource(R.string.downloads_queued)
        DownloadState.Waiting -> stringResource(if (entry.serverAway) R.string.downloads_waiting_server else R.string.downloads_waiting)
        DownloadState.Downloading -> stringResource(R.string.downloads_progress, (entry.fraction * 100).toInt(), formatBytes(entry.sizeBytes, formatWords()))
    }
    Text(
        label,
        style = MaterialTheme.typography.bodySmall,
        color = if (entry.state == DownloadState.Failed) PleyColors.DangerText else PleyColors.Muted,
        maxLines = 1,
        overflow = TextOverflow.Ellipsis,
        modifier = Modifier.padding(top = 8.dp),
    )
}
