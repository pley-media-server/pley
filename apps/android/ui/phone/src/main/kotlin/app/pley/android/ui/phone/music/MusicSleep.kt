package app.pley.android.ui.phone.music

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.size
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.player.MusicControl
import app.pley.android.player.sleep.SleepChoice
import app.pley.android.player.sleep.SleepState
import app.pley.android.player.sleep.SleepTexts
import app.pley.android.ui.phone.kit.PleyIconButton
import app.pley.android.ui.phone.player.CarSizedWindow
import app.pley.android.ui.phone.player.MoonIcon

/**
 * Now Playing's sleep timer: a moon (accent while it runs) opening its
 * choices, the one running ticked with its minutes left.
 */
@Composable
internal fun MusicSleepButton(sleep: SleepState, music: MusicControl) {
    var open by remember { mutableStateOf(false) }
    val resources = LocalResources.current
    val on = sleep.choice != SleepChoice.Off
    Box {
        PleyIconButton(
            MoonIcon,
            SleepTexts.title(resources),
            onClick = { open = true },
            tint = if (on) PleyColors.Accent else PleyColors.Muted,
            modifier = Modifier.testTag(TAG_MUSIC_SLEEP),
        )
        DropdownMenu(expanded = open, onDismissRequest = { open = false }, containerColor = PleyColors.Surface2) {
            CarSizedWindow { SleepChoices(sleep, music, onChosen = { open = false }) }
        }
    }
}

/** The menu's choices, the one running ticked with its minutes left. */
@Composable
private fun SleepChoices(sleep: SleepState, music: MusicControl, onChosen: () -> Unit) {
    val resources = LocalResources.current
    Column {
        SleepChoice.ALL.forEach { choice ->
            val chosen = sleep.choice == choice
            DropdownMenuItem(
                text = {
                    val label = if (chosen && choice is SleepChoice.Minutes && sleep.minutesLeft != null) {
                        SleepTexts.summary(resources, sleep, music = true)
                    } else {
                        SleepTexts.choice(resources, choice, music = true)
                    }
                    Text(label, style = MaterialTheme.typography.bodyLarge)
                },
                leadingIcon = {
                    Box(Modifier.size(20.dp)) {
                        if (chosen) Icon(PleyIcons.Check, contentDescription = null, size = 20.dp, tint = PleyColors.Accent)
                    }
                },
                onClick = {
                    music.setSleep(choice)
                    onChosen()
                },
            )
        }
    }
}

internal const val TAG_MUSIC_SLEEP = "now-playing-sleep"
