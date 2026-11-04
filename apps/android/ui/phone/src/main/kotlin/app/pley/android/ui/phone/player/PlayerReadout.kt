package app.pley.android.ui.phone.player

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyMotion
import app.pley.android.designsystem.PleyShape
import app.pley.android.player.PlaybackDetailsUi
import app.pley.android.player.PlayerPhase

/**
 * Settings → Playback details on a phone, as on a TV: how the video plays
 * (the way, what goes in and comes out, live), small at the top left while
 * the controls are hidden. [enabled] is the setting; it steps aside for the
 * controls, the sheet, picture in picture and errors.
 */
@Composable
internal fun BoxScope.DetailsReadout(state: PlayerScreenState, enabled: Boolean) {
    val shown = enabled && !state.controlsVisible && state.sheet == null && state.phase !is PlayerPhase.Error
    LaunchedEffect(shown) { state.readout(shown) }
    DisposableEffect(state) { onDispose { state.readout(false) } }
    val details = state.details
    // It comes once the controls have faded, so the two never overlap.
    AnimatedVisibility(
        visible = shown && details != null,
        modifier = Modifier.align(Alignment.TopStart),
        enter = fadeIn(tween(PleyMotion.REVEAL_MS, delayMillis = PleyMotion.OVERLAY_MS)),
        exit = fadeOut(tween(PleyMotion.PRESS_MS)),
    ) {
        if (details != null) Readout(details)
    }
}

@Composable
private fun Readout(details: PlaybackDetailsUi) {
    Column(
        Modifier
            .windowInsetsPadding(WindowInsets.player)
            .padding(16.dp)
            .widthIn(max = READOUT_WIDTH)
            .background(Color.Black.copy(alpha = 0.6f), PleyShape.Card)
            .padding(horizontal = 12.dp, vertical = 8.dp),
        verticalArrangement = Arrangement.spacedBy(2.dp),
    ) {
        Text(details.method, style = MaterialTheme.typography.labelMedium, color = PlayerLook.badgeColors(details.method).text)
        details.rows.take(READOUT_ROWS).forEach { row ->
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(row.label, style = MaterialTheme.typography.bodySmall, color = PleyColors.Muted, maxLines = 1, modifier = Modifier.width(LABEL_WIDTH))
                Text(row.value, style = MaterialTheme.typography.bodySmall, color = PleyColors.Fg, maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
        }
    }
}

/** The readout's lines under the way it plays: the details' first ones (the file, video, audio, subtitles, bitrate). */
private const val READOUT_ROWS = 6
private val READOUT_WIDTH = 360.dp
private val LABEL_WIDTH = 72.dp
