package app.pley.android.ui.phone.kit

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.shrinkVertically
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.State
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyMotion
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.reducedMotion
import app.pley.android.ui.phone.R
import app.pley.android.ui.phone.theme.LocalPhoneMetrics

/**
 * Whether the app shows what it had saved because the server can't be
 * reached. The shell provides it; [OfflineBanner] reads it, so only the
 * banner changes when it does.
 */
val LocalOffline = staticCompositionLocalOf<State<Boolean>> { mutableStateOf(false) }

/**
 * A screen's top bar (design spec §3.1): [bg][PleyColors.Bg], the [title]
 * in headlineSmall, gaining a 1 dp [surface4][PleyColors.Surface4]
 * hairline once the page under it has [scrolled] (read only while
 * drawing). [onBack] adds the back arrow. Under it, the [OfflineBanner]
 * while the server can't be reached.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun PleyTopAppBar(
    title: String,
    modifier: Modifier = Modifier,
    onBack: (() -> Unit)? = null,
    scrolled: () -> Boolean = { false },
    windowInsets: WindowInsets = TopAppBarDefaults.windowInsets,
    actions: @Composable RowScope.() -> Unit = {},
) {
    PleyTopAppBar(
        title = {
            Text(title, style = MaterialTheme.typography.headlineSmall, maxLines = 1, overflow = TextOverflow.Ellipsis)
        },
        modifier = modifier,
        onBack = onBack,
        scrolled = scrolled,
        windowInsets = windowInsets,
        actions = actions,
    )
}

/** [PleyTopAppBar] with any [title] content: the pley mark and wordmark on Home. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun PleyTopAppBar(
    title: @Composable () -> Unit,
    modifier: Modifier = Modifier,
    onBack: (() -> Unit)? = null,
    scrolled: () -> Boolean = { false },
    windowInsets: WindowInsets = TopAppBarDefaults.windowInsets,
    actions: @Composable RowScope.() -> Unit = {},
) {
    val margin = LocalPhoneMetrics.current.margin
    Column(modifier.fillMaxWidth()) {
        TopAppBar(
            title = title,
            navigationIcon = {
                if (onBack != null) {
                    PleyIconButton(PleyIcons.ArrowLeft, stringResource(R.string.kit_back), onBack)
                }
            },
            actions = {
                actions()
                Spacer(Modifier.width((margin - 16.dp).coerceAtLeast(0.dp)))
            },
            windowInsets = windowInsets,
            colors = TopAppBarDefaults.topAppBarColors(
                containerColor = PleyColors.Bg,
                scrolledContainerColor = PleyColors.Bg,
                titleContentColor = PleyColors.Fg,
                navigationIconContentColor = PleyColors.Fg,
                actionIconContentColor = PleyColors.Fg,
            ),
            modifier = Modifier
                .padding(start = if (onBack == null) (margin - 16.dp).coerceAtLeast(0.dp) else 0.dp)
                .drawWithContent {
                    drawContent()
                    if (scrolled()) {
                        val hair = 1.dp.toPx()
                        drawLine(PleyColors.Surface4, Offset(0f, size.height - hair / 2f), Offset(size.width, size.height - hair / 2f), hair)
                    }
                },
        )
        OfflineBanner()
    }
}

/**
 * "Pley is offline" (design spec §3.4): a [surface3][PleyColors.Surface3]
 * strip under the top bar while [LocalOffline] holds, announced when it
 * appears, with the way to the downloads at its end ([LocalOfflineAction]).
 * The app tries the server again by itself; the strip goes when it answers.
 */
@Composable
fun OfflineBanner(modifier: Modifier = Modifier) {
    val offline = LocalOffline.current
    val reduced = reducedMotion()
    AnimatedVisibility(
        visible = offline.value,
        enter = if (reduced) fadeIn(PleyMotion.reveal()) else expandVertically(PleyMotion.overlay()) + fadeIn(PleyMotion.reveal()),
        exit = if (reduced) fadeOut(PleyMotion.reveal()) else shrinkVertically(PleyMotion.overlay()) + fadeOut(PleyMotion.reveal()),
        modifier = modifier,
    ) {
        Row(
            Modifier
                .fillMaxWidth()
                .background(PleyColors.Surface3)
                .padding(start = LocalPhoneMetrics.current.marginStart, end = LocalPhoneMetrics.current.marginEnd, top = 10.dp, bottom = 10.dp)
                .semantics(mergeDescendants = true) { liveRegion = LiveRegionMode.Polite },
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Icon(PleyIcons.ServerOff, contentDescription = null, size = 20.dp, tint = PleyColors.Muted)
            Spacer(Modifier.width(12.dp))
            Text(stringResource(R.string.shell_offline), Modifier.weight(1f), style = MaterialTheme.typography.bodyMedium, color = PleyColors.Fg)
            LocalOfflineAction.current?.let { action ->
                PleyTextButton(action.label, onClick = action.onClick)
            }
        }
    }
}
