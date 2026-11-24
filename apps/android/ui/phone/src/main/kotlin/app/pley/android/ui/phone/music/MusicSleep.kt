package app.pley.android.ui.phone.music

import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.platform.testTag
import app.pley.android.designsystem.PleyColors
import app.pley.android.player.MusicControl
import app.pley.android.player.sleep.SleepChoice
import app.pley.android.player.sleep.SleepState
import app.pley.android.player.sleep.SleepTexts
import app.pley.android.ui.phone.kit.PleyIconButton
import app.pley.android.ui.phone.kit.PleySheet
import app.pley.android.ui.phone.player.MoonIcon

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
                SleepTexts.summary(resources, sleep, music = true)
            } else {
                SleepTexts.choice(resources, choice, music = true)
            }
            MusicChoiceRow(label, chosen) {
                music.setSleep(choice)
                onChosen()
            }
        }
    }
}

internal const val TAG_MUSIC_SLEEP = "now-playing-sleep"
