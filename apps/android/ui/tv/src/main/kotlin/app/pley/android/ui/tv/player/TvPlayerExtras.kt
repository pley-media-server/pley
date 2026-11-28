package app.pley.android.ui.tv.player

import android.view.KeyEvent as AndroidKeyEvent
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsFocusedAsState
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.key.onKeyEvent
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.unit.dp
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.PleyTv
import app.pley.android.designsystem.PleyType
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.text.shownLocale
import app.pley.android.player.AudioDelays
import app.pley.android.player.subtitles.SubtitleSync
import app.pley.android.ui.tv.R

/**
 * The subtitles panel's Sync row: Left and Right move the subtitles 0.1 s
 * earlier or later (held: on and on), OK puts them back. The value shows
 * as "+0.5 s"; the row takes focus as a whole, its texts never do.
 */
@Composable
internal fun SyncRow(offsetMs: Long, onChange: (Long) -> Unit, modifier: Modifier = Modifier) = StepRow(
    title = stringResource(R.string.tv_player_sync),
    hint = stringResource(R.string.tv_player_sync_hint),
    value = SubtitleSync.label(offsetMs, shownLocale()),
    onStep = { onChange(offsetMs + it * SubtitleSync.STEP_MS) },
    onReset = { onChange(0) },
    tag = TAG_SYNC,
    modifier = modifier,
)

/**
 * The audio panel's delay row: Left and Right play the sound 50 ms earlier
 * or later than the picture (held: on and on), OK puts it back.
 */
@Composable
internal fun AudioDelayRow(delayMs: Long, onChange: (Long) -> Unit, modifier: Modifier = Modifier) = StepRow(
    title = stringResource(R.string.tv_player_audio_delay),
    hint = stringResource(R.string.tv_player_sync_hint),
    value = AudioDelays.label(delayMs, shownLocale()),
    onStep = { onChange(delayMs + it * AudioDelays.STEP_MS) },
    onReset = { onChange(0) },
    tag = TAG_AUDIO_DELAY,
    modifier = modifier,
)

/** A row Left and Right step ([onStep] -1 or 1) and OK resets, showing [value] between chevrons. */
@Composable
private fun StepRow(
    title: String,
    hint: String,
    value: String,
    onStep: (Int) -> Unit,
    onReset: () -> Unit,
    tag: String,
    modifier: Modifier = Modifier,
) {
    val interactions = remember { MutableInteractionSource() }
    val focused by interactions.collectIsFocusedAsState()
    val content = if (focused) PleyColors.OnAccent else PleyColors.Fg
    val muted = if (focused) PleyColors.OnAccent else PleyColors.Muted
    Row(
        modifier
            .fillMaxWidth()
            .heightIn(min = PleyTv.ListRowHeight)
            .background(if (focused) PleyColors.Accent else Color.Transparent, PleyShape.Control)
            .onKeyEvent { event ->
                val native = event.nativeKeyEvent
                if (native.action != AndroidKeyEvent.ACTION_DOWN) return@onKeyEvent false
                when (native.keyCode) {
                    AndroidKeyEvent.KEYCODE_DPAD_LEFT -> {
                        onStep(-1)
                        true
                    }
                    AndroidKeyEvent.KEYCODE_DPAD_RIGHT -> {
                        onStep(1)
                        true
                    }
                    else -> false
                }
            }
            .semantics { stateDescription = value }
            .clickable(interactionSource = interactions, indication = null, role = Role.Button, onClick = onReset)
            .testTag(tag)
            .padding(horizontal = 16.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(Modifier.weight(1f)) {
            BasicText(title, style = PleyType.Tv.Label.copy(color = content), maxLines = 1)
            BasicText(hint, style = PleyType.Tv.Caption.copy(color = muted), maxLines = 2)
        }
        Icon(PleyIcons.ChevronLeft, contentDescription = null, size = 20.dp, tint = muted)
        BasicText(value, style = PleyType.Tv.Label.copy(color = content), maxLines = 1, modifier = Modifier.padding(horizontal = 6.dp))
        Icon(PleyIcons.ChevronRight, contentDescription = null, size = 20.dp, tint = muted)
    }
}

/** How long a playback the server stopped shows why, before the player leaves by itself. */
internal const val STOPPED_LEAVE_MS = 20_000L

internal const val TAG_SYNC = "pley.tv.player.sync"
internal const val TAG_AUDIO_DELAY = "pley.tv.player.audioDelay"
