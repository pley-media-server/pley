package app.pley.android.ui.phone.player

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.text.shownLocale
import app.pley.android.player.AudioDelays
import app.pley.android.player.subtitles.SubtitleSync
import app.pley.android.ui.phone.R
import app.pley.android.ui.phone.kit.PleyIconButton
import app.pley.android.ui.phone.kit.PleyTextButton

/**
 * The subtitle page's Sync: the offset ("+0.5 s"), 0.1 s earlier or later,
 * and Reset while it isn't 0.
 */
@Composable
internal fun SyncControl(offsetMs: Long, onChange: (Long) -> Unit) {
    Row(
        Modifier
            .fillMaxWidth()
            .heightIn(min = 56.dp)
            .padding(start = 24.dp, end = 12.dp)
            .testTag(TAG_SYNC),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(Modifier.weight(1f)) {
            Text(stringResource(R.string.player_sync), style = MaterialTheme.typography.bodyLarge, color = PleyColors.Fg)
            Text(SubtitleSync.label(offsetMs, shownLocale()), style = MaterialTheme.typography.bodySmall, color = if (offsetMs != 0L) PleyColors.AccentBright else PleyColors.Muted)
        }
        PleyTextButton(stringResource(R.string.player_sync_reset), onClick = { onChange(0) }, enabled = offsetMs != 0L)
        PleyIconButton(PleyIcons.ChevronLeft, stringResource(R.string.player_sync_earlier), onClick = { onChange(offsetMs - SubtitleSync.STEP_MS) })
        PleyIconButton(PleyIcons.ChevronRight, stringResource(R.string.player_sync_later), onClick = { onChange(offsetMs + SubtitleSync.STEP_MS) })
    }
}

/**
 * The audio page's delay: the sound later (→) or earlier (←) than the
 * picture in 50 ms steps, ±5 s, with Reset.
 */
@Composable
internal fun AudioDelayControl(delayMs: Long, onChange: (Long) -> Unit) {
    Row(
        Modifier
            .fillMaxWidth()
            .heightIn(min = 56.dp)
            .padding(start = 24.dp, end = 12.dp)
            .testTag(TAG_AUDIO_DELAY),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(Modifier.weight(1f)) {
            Text(stringResource(R.string.player_audio_delay), style = MaterialTheme.typography.bodyLarge, color = PleyColors.Fg)
            Text(
                AudioDelays.label(delayMs, shownLocale()),
                style = MaterialTheme.typography.bodySmall,
                color = if (delayMs != 0L) PleyColors.AccentBright else PleyColors.Muted,
            )
        }
        PleyTextButton(stringResource(R.string.player_sync_reset), onClick = { onChange(0) }, enabled = delayMs != 0L)
        PleyIconButton(PleyIcons.ChevronLeft, stringResource(R.string.player_audio_earlier), onClick = { onChange(delayMs - AudioDelays.STEP_MS) })
        PleyIconButton(PleyIcons.ChevronRight, stringResource(R.string.player_audio_later), onClick = { onChange(delayMs + AudioDelays.STEP_MS) })
    }
}

/** How long a playback the server stopped shows why, before the player leaves by itself. */
internal const val STOPPED_LEAVE_MS = 20_000L

internal const val TAG_SYNC = "phone-player-sync"
internal const val TAG_AUDIO_DELAY = "phone-player-audio-delay"
