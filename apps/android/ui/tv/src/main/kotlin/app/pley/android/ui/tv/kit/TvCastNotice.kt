package app.pley.android.ui.tv.kit

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyMotion
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.PleyType
import app.pley.android.designsystem.TvSafeArea
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcons
import kotlinx.coroutines.delay

/**
 * Where something another device sent here came from, "Playing from
 * Living room TV", at the top right where TV toasts are (design spec
 * §2.10), over the player or Now Playing, for [TV_CAST_NOTICE_MS]. Never
 * focusable: there is nothing to do with it. [key] shows it again for the
 * next cast, whatever it says; nothing while [text] is `null`.
 */
@Composable
internal fun BoxScope.TvCastNotice(text: String?, key: Any? = text) {
    var shown by remember(key) { mutableStateOf(text != null) }
    LaunchedEffect(key) {
        if (text == null) return@LaunchedEffect
        delay(TV_CAST_NOTICE_MS)
        shown = false
    }
    AnimatedVisibility(
        visible = shown,
        modifier = Modifier.align(Alignment.TopEnd).padding(top = TvSafeArea.Vertical, end = TvSafeArea.Horizontal),
        enter = fadeIn(PleyMotion.reveal()),
        exit = fadeOut(PleyMotion.reveal()),
    ) {
        Row(
            Modifier
                .background(PleyColors.Surface3, PleyShape.Pill)
                .padding(horizontal = 20.dp, vertical = 10.dp)
                .semantics(mergeDescendants = true) { liveRegion = LiveRegionMode.Polite }
                .testTag(TV_CAST_NOTICE),
            horizontalArrangement = Arrangement.spacedBy(10.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Icon(PleyIcons.Cast, contentDescription = null, size = 20.dp, tint = PleyColors.Accent)
            BasicText(text.orEmpty(), style = PleyType.Tv.Label.copy(color = PleyColors.Fg))
        }
    }
}

/** How long the notice shows: about as long as a toast. */
internal const val TV_CAST_NOTICE_MS = 4_000L

internal const val TV_CAST_NOTICE = "pley.tv.cast_notice"
