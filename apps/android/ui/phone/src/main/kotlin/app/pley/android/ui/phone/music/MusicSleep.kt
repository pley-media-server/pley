package app.pley.android.ui.phone.music

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.displayCutout
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.platform.testTag
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
import app.pley.android.designsystem.reducedMotion
import app.pley.android.player.MusicControl
import app.pley.android.player.sleep.SleepChoice
import app.pley.android.player.sleep.SleepState
import app.pley.android.player.sleep.SleepTexts
import app.pley.android.player.sleep.SleepTimer
import app.pley.android.ui.phone.kit.PleyIconButton
import app.pley.android.ui.phone.kit.PleySheet
import app.pley.android.ui.phone.kit.PleyTextButton

/** Lucide `moon`: the sleep timer. */
private val MoonIcon = PleyIcon("moon", false, "M12 3a6 6 0 0 0 9 9 9 9 0 1 1-9-9Z")

/**
 * Now Playing's sleep timer: a moon (accent while it runs) opening its
 * choices in a sheet, as the queue and a book's speed open, the one
 * running checked with its minutes left. On a car's screen the sheet is
 * car-sized, as the page is.
 */
@Composable
internal fun MusicSleepButton(sleep: SleepState, music: MusicControl) {
    var open by rememberSaveable { mutableStateOf(false) }
    val resources = LocalResources.current
    val on = sleep.choice != SleepChoice.Off
    PleyIconButton(
        MoonIcon,
        SleepTexts.title(resources),
        onClick = { open = true },
        tint = if (on) PleyColors.Accent else PleyColors.Muted,
        modifier = Modifier.testTag(TAG_MUSIC_SLEEP),
    )
    if (open) {
        PleySheet(onDismiss = { open = false }, title = SleepTexts.title(resources)) {
            SleepChoices(sleep, music, onChosen = { open = false })
        }
    }
}

/** The sheet's choices, the one running checked with its minutes left. */
@Composable
private fun SleepChoices(sleep: SleepState, music: MusicControl, onChosen: () -> Unit) {
    val resources = LocalResources.current
    MusicChoices {
        SleepChoice.ALL.forEach { choice ->
            val chosen = sleep.choice == choice
            val label = if (chosen && choice is SleepChoice.Minutes && sleep.minutesLeft != null) {
                SleepTexts.summary(resources, sleep)
            } else {
                SleepTexts.choice(resources, choice)
            }
            MusicChoiceRow(label, chosen) {
                music.setSleep(choice)
                onChosen()
            }
        }
    }
}

/**
 * The sleep timer's notice, 30 s before it pauses: "Pausing in 28 s" and
 * Keep listening, at the top center of Now Playing, under its top bar. The
 * count steps once a second, as wide as "Pausing in 30 s" all the way down,
 * so the notice keeps its size and Keep listening stays under the finger as
 * the count goes under 10.
 */
@Composable
internal fun BoxScope.MusicSleepNotice(secondsLeft: Int?, onKeep: () -> Unit) {
    var last by remember { mutableIntStateOf(0) }
    if (secondsLeft != null) last = secondsLeft
    val reduced = reducedMotion()
    val resources = LocalResources.current
    val countStyle = MaterialTheme.typography.bodyMedium
    AnimatedVisibility(
        visible = secondsLeft != null,
        modifier = Modifier
            .align(Alignment.TopCenter)
            .windowInsetsPadding(WindowInsets.displayCutout)
            .padding(top = 64.dp),
        enter = fadeIn(PleyMotion.overlay(reduced)),
        exit = fadeOut(PleyMotion.overlay(reduced)),
    ) {
        // Measured as the notice shows: most music never has one.
        val measurer = rememberTextMeasurer()
        val density = LocalDensity.current
        val countWidth = remember(resources, measurer, density, countStyle) {
            val widest = SleepTexts.notice(resources, (SleepTimer.NOTICE_MS / 1_000).toInt())
            with(density) { measurer.measure(widest, countStyle, maxLines = 1).size.width.toDp() }
        }
        Row(
            Modifier
                .background(PleyColors.Surface3, PleyShape.Control)
                .padding(start = 16.dp, end = 4.dp)
                .heightIn(min = 48.dp)
                .testTag(TAG_MUSIC_SLEEP_NOTICE),
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
            PleyTextButton(SleepTexts.keep(resources), onClick = onKeep)
        }
    }
}

internal const val TAG_MUSIC_SLEEP = "now-playing-sleep"
internal const val TAG_MUSIC_SLEEP_NOTICE = "now-playing-sleep-notice"
