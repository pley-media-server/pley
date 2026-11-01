package app.pley.android.ui.phone.kit

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.produceState
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import app.pley.android.common.sentenceOf
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.network.OfflineException
import app.pley.android.ui.phone.R
import kotlinx.coroutines.delay

/**
 * A page with nothing to show (design spec §8): an optional [icon], the
 * [title] in headlineSmall and the [body] (when there is one) in bodyMedium
 * [muted][PleyColors.Muted], centred, then an optional action ("Clear
 * filters"). "No libraries yet", "Nothing here yet", "No matches".
 */
@Composable
fun EmptyState(
    title: String,
    body: String?,
    modifier: Modifier = Modifier,
    icon: PleyIcon? = null,
    action: (@Composable () -> Unit)? = null,
) {
    Column(
        modifier.fillMaxWidth().padding(horizontal = 32.dp, vertical = 48.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Column(Modifier.widthIn(max = 420.dp), horizontalAlignment = Alignment.CenterHorizontally) {
            if (icon != null) {
                Icon(icon, contentDescription = null, size = 40.dp, tint = PleyColors.Muted)
                Spacer(Modifier.height(16.dp))
            }
            Text(
                title,
                style = MaterialTheme.typography.headlineSmall,
                color = PleyColors.Fg,
                textAlign = TextAlign.Center,
                modifier = Modifier.semantics { heading() },
            )
            if (body != null) {
                Spacer(Modifier.height(8.dp))
                Text(body, style = MaterialTheme.typography.bodyMedium, color = PleyColors.Muted, textAlign = TextAlign.Center)
            }
            if (action != null) {
                Spacer(Modifier.height(24.dp))
                action()
            }
        }
    }
}

/**
 * Something failed to load (design spec §8): "Couldn't load your home
 * screen", the error's own sentence under it, and Retry as a tonal
 * button. The sentence is [describe]d from [error], so the server's and
 * the network's words come through as they are; it is announced.
 */
@Composable
fun ErrorState(
    title: String,
    error: Throwable,
    onRetry: () -> Unit,
    modifier: Modifier = Modifier,
) {
    // The server answered nothing: said as the player says it, with what to check.
    val offline = error is OfflineException
    EmptyState(
        title = if (offline) stringResource(R.string.kit_offline_title) else title,
        body = if (offline && !(error as OfflineException).blocked) {
            stringResource(R.string.kit_offline_body, sentenceOf(error))
        } else {
            sentenceOf(error)
        },
        icon = if (offline) PleyIcons.ServerOff else PleyIcons.CircleAlert,
        modifier = modifier.semantics { liveRegion = LiveRegionMode.Polite },
        action = { TonalButton(stringResource(R.string.kit_retry), onClick = onRetry, icon = PleyIcons.RefreshCw) },
    )
}

/**
 * A spinner that shows only once something has taken [delayMs] (400 ms,
 * design spec §8), so quick actions never flash one. [label] is what
 * screen readers hear ("Signing in…").
 */
@Composable
fun DelayedSpinner(
    label: String,
    modifier: Modifier = Modifier,
    size: Dp = 24.dp,
    delayMs: Long = SPINNER_DELAY_MS,
) {
    val shown by produceState(initialValue = delayMs <= 0) {
        delay(delayMs)
        value = true
    }
    Box(modifier.size(size), contentAlignment = Alignment.Center) {
        if (shown) {
            CircularProgressIndicator(
                modifier = Modifier.size(size).semantics { contentDescription = label },
                color = PleyColors.Accent,
                trackColor = PleyColors.Surface4,
                strokeWidth = (size.value / 10f).coerceAtLeast(2f).dp,
            )
        }
    }
}

/** How long an action runs before its spinner shows. */
const val SPINNER_DELAY_MS = 400L

/** A [DelayedSpinner] with its text beside it ("Finish signing in in your browser."). */
@Composable
fun BusyLine(text: String, modifier: Modifier = Modifier) {
    androidx.compose.foundation.layout.Row(modifier, horizontalArrangement = Arrangement.spacedBy(12.dp), verticalAlignment = Alignment.CenterVertically) {
        DelayedSpinner(label = text, size = 20.dp)
        Text(text, style = MaterialTheme.typography.bodyMedium, color = PleyColors.Muted)
    }
}
