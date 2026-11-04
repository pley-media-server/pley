package app.pley.android.ui.phone.remote

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
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
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcons
import kotlinx.coroutines.delay

/**
 * "Playing from Living room TV" over the live player as a channel another
 * device sent here tunes: a pill as the players' toasts look, for
 * [CAST_NOTICE_MS], read out as it shows. Nothing while [text] is `null`.
 */
@Composable
internal fun CastNotice(text: String?, modifier: Modifier = Modifier) {
    var shown by remember(text) { mutableStateOf(text != null) }
    LaunchedEffect(text) {
        if (text == null) return@LaunchedEffect
        delay(CAST_NOTICE_MS)
        shown = false
    }
    AnimatedVisibility(shown, modifier, enter = fadeIn(PleyMotion.reveal()), exit = fadeOut(PleyMotion.reveal())) {
        Row(
            Modifier
                .background(PleyColors.Surface3, PleyShape.Pill)
                .padding(horizontal = 16.dp, vertical = 10.dp)
                .semantics(mergeDescendants = true) { liveRegion = LiveRegionMode.Polite }
                .testTag(CAST_NOTICE),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Icon(PleyIcons.Cast, contentDescription = null, size = 18.dp, tint = PleyColors.Accent)
            Spacer(Modifier.width(8.dp))
            Text(text.orEmpty(), style = MaterialTheme.typography.labelLarge, color = PleyColors.Fg)
        }
    }
}

/** How long the notice of where a cast came from shows: about as long as a toast. */
internal const val CAST_NOTICE_MS = 4_000L

internal const val CAST_NOTICE = "pley.phone.cast_notice"
