package app.pley.android.ui.phone.player

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.unit.dp
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyMotion
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.reducedMotion
import app.pley.android.player.AudioDelays
import app.pley.android.player.sleep.SleepTexts
import app.pley.android.player.sleep.SleepTimer
import app.pley.android.player.subtitles.SubtitleSync
import app.pley.android.ui.phone.R
import app.pley.android.ui.phone.kit.PleyIconButton
import app.pley.android.ui.phone.kit.PleyTextButton

/** Lucide `moon`: the sleep timer. */
internal val MoonIcon = PleyIcon("moon", false, "M12 3a6 6 0 0 0 9 9 9 9 0 1 1-9-9Z")

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
            Text(SubtitleSync.label(offsetMs), style = MaterialTheme.typography.bodySmall, color = if (offsetMs != 0L) PleyColors.AccentBright else PleyColors.Muted)
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
                AudioDelays.label(delayMs),
                style = MaterialTheme.typography.bodySmall,
                color = if (delayMs != 0L) PleyColors.AccentBright else PleyColors.Muted,
            )
        }
        PleyTextButton(stringResource(R.string.player_sync_reset), onClick = { onChange(0) }, enabled = delayMs != 0L)
        PleyIconButton(PleyIcons.ChevronLeft, stringResource(R.string.player_audio_earlier), onClick = { onChange(delayMs - AudioDelays.STEP_MS) })
        PleyIconButton(PleyIcons.ChevronRight, stringResource(R.string.player_audio_later), onClick = { onChange(delayMs + AudioDelays.STEP_MS) })
    }
}

/**
 * The sleep timer's notice, 30 s before it pauses: "Pausing in 28 s" and
 * Keep watching (or Keep listening), at the top centre. The count steps
 * once a second, as wide as "Pausing in 30 s" all the way down, so the
 * notice keeps its size and Keep watching stays under the finger as the
 * count goes under 10 (the native Samsung app's notice does the same).
 */
@Composable
internal fun BoxScope.SleepNotice(secondsLeft: Int?, music: Boolean, onKeep: () -> Unit, modifier: Modifier = Modifier) {
    var last by remember { mutableIntStateOf(0) }
    if (secondsLeft != null) last = secondsLeft
    val reduced = reducedMotion()
    val resources = LocalResources.current
    val countStyle = MaterialTheme.typography.bodyMedium
    val measurer = rememberTextMeasurer()
    val density = LocalDensity.current
    val countWidth = remember(resources, measurer, density, countStyle) {
        val widest = SleepTexts.notice(resources, (SleepTimer.NOTICE_MS / 1_000).toInt())
        with(density) { measurer.measure(widest, countStyle, maxLines = 1).size.width.toDp() }
    }
    AnimatedVisibility(
        visible = secondsLeft != null,
        modifier = modifier
            .align(Alignment.TopCenter)
            .windowInsetsPadding(WindowInsets.safeDrawing)
            .padding(top = 12.dp),
        enter = fadeIn(PleyMotion.overlay(reduced)),
        exit = fadeOut(PleyMotion.overlay(reduced)),
    ) {
        Row(
            Modifier
                .background(PleyColors.Surface3, PleyShape.Control)
                .padding(start = 16.dp, end = 4.dp)
                .heightIn(min = 48.dp)
                .testTag(TAG_SLEEP_NOTICE),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Icon(MoonIcon, contentDescription = null, size = 18.dp, tint = PleyColors.Muted)
            Text(
                SleepTexts.notice(resources, last),
                style = countStyle,
                color = PleyColors.Fg,
                maxLines = 1,
                modifier = Modifier.widthIn(min = countWidth).semantics { liveRegion = LiveRegionMode.Polite },
            )
            PleyTextButton(SleepTexts.keep(resources, music), onClick = onKeep)
        }
    }
}

/** How long a playback the server stopped shows why, before the player leaves by itself. */
internal const val STOPPED_LEAVE_MS = 20_000L

internal const val TAG_SYNC = "phone-player-sync"
internal const val TAG_AUDIO_DELAY = "phone-player-audio-delay"
internal const val TAG_SLEEP_NOTICE = "phone-player-sleep-notice"
