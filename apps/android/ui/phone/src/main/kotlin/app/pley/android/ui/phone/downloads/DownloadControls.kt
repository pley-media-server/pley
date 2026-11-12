package app.pley.android.ui.phone.downloads

import android.Manifest
import android.content.pm.PackageManager
import android.os.Build
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.core.content.ContextCompat
import app.pley.android.data.DownloadEntry
import app.pley.android.data.DownloadState
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.text.asString
import app.pley.android.ui.phone.R

/**
 * A download's state as one glyph, [size] square: the arrow when there is
 * none, a ring filling as it comes (the arrow inside, faint while it
 * waits), a tick in accent when it is whole, a warning when it failed.
 */
@Composable
internal fun DownloadGlyph(entry: DownloadEntry?, size: Dp = 24.dp) {
    when (entry?.state) {
        null -> Icon(PleyIcons.Download, contentDescription = null, size = size)
        DownloadState.Done -> Icon(PleyIcons.CircleCheck, contentDescription = null, size = size, tint = PleyColors.AccentBright)
        DownloadState.Failed -> Icon(PleyIcons.CircleAlert, contentDescription = null, size = size, tint = PleyColors.DangerText)
        DownloadState.Queued, DownloadState.Waiting, DownloadState.Downloading -> {
            // While the server prepares it, the ring is how far the server has come.
            val fraction = entry.converting ?: if (entry.state == DownloadState.Downloading) entry.fraction else 0f
            Box(
                Modifier
                    .size(size)
                    .drawBehind {
                        val stroke = 2.dp.toPx()
                        val inset = stroke / 2f
                        val arc = Size(this.size.width - stroke, this.size.height - stroke)
                        drawArc(PleyColors.Fg.copy(alpha = 0.25f), 0f, 360f, false, Offset(inset, inset), arc, style = Stroke(stroke))
                        if (fraction > 0f) {
                            drawArc(PleyColors.Accent, -90f, 360f * fraction, false, Offset(inset, inset), arc, style = Stroke(stroke, cap = StrokeCap.Round))
                        }
                    },
                contentAlignment = Alignment.Center,
            ) {
                Icon(
                    PleyIcons.Download,
                    contentDescription = null,
                    size = size * 0.55f,
                    tint = if (entry.state == DownloadState.Downloading) PleyColors.Fg else PleyColors.Muted,
                )
            }
        }
    }
}

/** What a download's button says to screen readers about [title]. */
@Composable
internal fun downloadDescription(entry: DownloadEntry?, title: String): String = when (entry?.state) {
    null -> stringResource(R.string.downloads_download_item, title)
    DownloadState.Done -> stringResource(R.string.downloads_downloaded_item, title)
    DownloadState.Failed -> stringResource(R.string.downloads_failed_toast, entry.error?.asString().orEmpty())
    else -> entry.converting?.let { stringResource(R.string.downloads_preparing_item, title, (it * 100).toInt()) }
        ?: stringResource(R.string.downloads_downloading_item, title, (entry.fraction * 100).toInt())
}

/**
 * An episode row's download button: 48 dp around the glyph. Tapping
 * downloads, or opens [onMore] for one there is (play it, try it again,
 * cancel or delete it).
 */
@Composable
internal fun DownloadIconButton(
    entry: DownloadEntry?,
    title: String,
    onDownload: () -> Unit,
    onMore: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val description = downloadDescription(entry, title)
    Box(
        modifier
            .size(48.dp)
            .clip(PleyShape.Circle)
            .clickable(role = Role.Button) {
                if (entry == null) onDownload() else onMore()
            }
            .semantics { contentDescription = description },
        contentAlignment = Alignment.Center,
    ) {
        DownloadGlyph(entry)
    }
}

/**
 * Runs [download] once the app may show the downloads' notification
 * (asked once, on Android 13 and later): downloading goes on either way,
 * only quieter when it may not. What waits for the answer is kept with
 * the screen, which the phone turning makes anew meanwhile.
 */
@Composable
internal fun rememberDownloadStarter(download: (List<Long>) -> Unit): (List<Long>) -> Unit {
    val context = LocalContext.current
    val current = rememberUpdatedState(download)
    var pending by rememberSaveable { mutableStateOf<LongArray?>(null) }
    val launcher = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) {
        pending?.let { ids -> current.value(ids.toList()) }
        pending = null
    }
    return remember(context, launcher) {
        { ids ->
            val asked = Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU &&
                ContextCompat.checkSelfPermission(context, Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED
            if (asked) {
                pending = ids.toLongArray()
                launcher.launch(Manifest.permission.POST_NOTIFICATIONS)
            } else {
                current.value(ids)
            }
        }
    }
}

/**
 * What can be done with a download there is, under its button: play it
 * when whole, try it again when it failed, then delete it (or cancel it
 * while it comes); [onOpen], where given, goes to its page. Why it failed,
 * or what it waits for, is over them.
 */
@Composable
internal fun DownloadMenu(
    entry: DownloadEntry?,
    expanded: Boolean,
    onDismiss: () -> Unit,
    onPlay: (DownloadEntry) -> Unit,
    onRetry: (DownloadEntry) -> Unit,
    onRemove: (DownloadEntry) -> Unit,
    onOpen: ((DownloadEntry) -> Unit)? = null,
) {
    if (entry == null) return
    val coming = entry.state != DownloadState.Done && entry.state != DownloadState.Failed
    DropdownMenu(expanded = expanded, onDismissRequest = onDismiss, containerColor = PleyColors.Surface3) {
        // Why it failed, or what it waits for, above what can be done about it.
        val note = when (entry.state) {
            DownloadState.Failed -> entry.error?.asString()
            DownloadState.Waiting -> stringResource(if (entry.serverAway) R.string.downloads_waiting_server else R.string.downloads_waiting)
            else -> null
        }
        if (note != null) {
            Text(
                note,
                style = MaterialTheme.typography.bodySmall,
                color = PleyColors.Muted,
                modifier = Modifier.widthIn(max = 280.dp).padding(horizontal = 16.dp, vertical = 8.dp),
            )
        }
        when (entry.state) {
            DownloadState.Done -> MenuAction(R.string.downloads_play, PleyIcons.Play) {
                onDismiss()
                onPlay(entry)
            }
            DownloadState.Failed -> MenuAction(R.string.downloads_retry, PleyIcons.RotateCw) {
                onDismiss()
                onRetry(entry)
            }
            else -> Unit
        }
        if (onOpen != null) {
            MenuAction(R.string.downloads_open, PleyIcons.Info) {
                onDismiss()
                onOpen(entry)
            }
        }
        MenuAction(if (coming) R.string.downloads_cancel else R.string.downloads_delete, if (coming) PleyIcons.X else PleyIcons.Trash2) {
            onDismiss()
            onRemove(entry)
        }
    }
}

@Composable
private fun MenuAction(label: Int, icon: PleyIcon, onClick: () -> Unit) {
    DropdownMenuItem(
        text = { Text(stringResource(label)) },
        leadingIcon = { Icon(icon, contentDescription = null, size = 20.dp) },
        onClick = onClick,
    )
}
