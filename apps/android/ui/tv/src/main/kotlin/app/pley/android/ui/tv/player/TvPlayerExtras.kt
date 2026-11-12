package app.pley.android.ui.tv.player

import android.view.KeyEvent as AndroidKeyEvent
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsFocusedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.State
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusProperties
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.key.onKeyEvent
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.unit.dp
import androidx.tv.material3.Border
import androidx.tv.material3.Button
import androidx.tv.material3.ButtonDefaults
import androidx.tv.material3.Glow
import androidx.tv.material3.Text
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyFocus
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.PleyTv
import app.pley.android.designsystem.PleyType
import app.pley.android.designsystem.TvSafeArea
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.text.shownLocale
import app.pley.android.player.AudioDelays
import app.pley.android.player.PlayerUiState
import app.pley.android.player.sleep.SleepTexts
import app.pley.android.player.sleep.SleepTimer
import app.pley.android.player.subtitles.SubtitleSync
import app.pley.android.ui.tv.R

/** Lucide `moon`: the sleep timer. */
internal val MoonIcon = PleyIcon("moon", false, "M12 3a6 6 0 0 0 9 9 9 9 0 1 1-9-9Z")

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

/**
 * The sleep timer's notice, 30 s before it pauses: "Pausing in 28 s" and
 * Keep watching, at the bottom left (Skip and Up Next keep the right). The
 * count steps once a second, as wide as "Pausing in 30 s" all the way down,
 * so Keep watching stays put as it goes under 10; the button takes focus
 * while the controls are hidden, as Skip does.
 */
@Composable
internal fun BoxScope.TvSleepNotice(state: State<PlayerUiState>, visible: Boolean, controlsShown: Boolean, ui: TvPlayerUi, onKeep: () -> Unit) {
    val secondsLeft by remember(state) { derivedStateOf { state.value.sleep.secondsLeft } }
    var last by remember { mutableIntStateOf(0) }
    secondsLeft?.let { last = it }
    val resources = LocalResources.current
    val countStyle = PleyType.Tv.Label.copy(color = PleyColors.Fg)
    AnimatedVisibility(
        visible = visible,
        modifier = Modifier
            .align(Alignment.BottomStart)
            .padding(start = TvSafeArea.Horizontal, bottom = if (controlsShown) NOTICE_BOTTOM_SHOWN else NOTICE_BOTTOM_HIDDEN),
        enter = fadeIn(tween(NOTICE_FADE_MS)),
        exit = fadeOut(tween(NOTICE_FADE_MS)),
    ) {
        // Measured as the notice shows: most playbacks never have one.
        val measurer = rememberTextMeasurer()
        val density = LocalDensity.current
        val countWidth = remember(resources, measurer, density) {
            val widest = SleepTexts.notice(resources, (SleepTimer.NOTICE_MS / 1_000).toInt())
            with(density) { measurer.measure(widest, countStyle, maxLines = 1).size.width.toDp() }
        }
        Row(
            Modifier
                .background(PleyColors.Surface3, PleyShape.Control)
                .padding(start = 20.dp, end = 8.dp, top = 8.dp, bottom = 8.dp)
                .testTag(TAG_SLEEP_NOTICE),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            Icon(MoonIcon, contentDescription = null, size = 20.dp, tint = PleyColors.Muted)
            BasicText(
                SleepTexts.notice(resources, last),
                style = countStyle,
                maxLines = 1,
                modifier = Modifier.widthIn(min = countWidth).semantics { liveRegion = LiveRegionMode.Polite },
            )
            Button(
                onClick = onKeep,
                modifier = Modifier
                    // Set, as a pill's is: tv-material's Row would keep it at 40.
                    .height(PleyTv.ButtonHeight)
                    .focusRequester(ui.requester(PlayerFocus.SleepNotice))
                    .onFocusChanged { if (it.isFocused) ui.focused = PlayerFocus.SleepNotice else if (ui.focused == PlayerFocus.SleepNotice) ui.focused = null }
                    .focusProperties {
                        down = if (controlsShown) ui.requester(PlayerFocus.Scrubber) else FocusRequester.Cancel
                        left = FocusRequester.Cancel
                        right = FocusRequester.Cancel
                        up = FocusRequester.Cancel
                    },
                shape = ButtonDefaults.shape(PleyShape.Control),
                colors = ButtonDefaults.colors(
                    containerColor = PleyColors.OnArt,
                    contentColor = PleyColors.Fg,
                    focusedContainerColor = PleyColors.Accent,
                    focusedContentColor = PleyColors.OnAccent,
                    pressedContainerColor = PleyColors.AccentBright,
                    pressedContentColor = PleyColors.OnAccent,
                ),
                scale = ButtonDefaults.scale(focusedScale = PleyFocus.PILL_SCALE, pressedScale = 1f),
                glow = ButtonDefaults.glow(Glow.None, Glow.None, Glow.None),
                border = ButtonDefaults.border(Border.None, Border.None, Border.None, Border.None, Border.None),
                contentPadding = PaddingValues(horizontal = 20.dp, vertical = 4.dp),
            ) {
                Text(SleepTexts.keep(resources), style = PleyType.Tv.Label)
            }
            Spacer(Modifier.width(0.dp))
        }
    }
}

/** The notice's bottom edge: Skip's, or above the controls while they show. */
private val NOTICE_BOTTOM_HIDDEN = 51.dp
private val NOTICE_BOTTOM_SHOWN = 185.dp
private const val NOTICE_FADE_MS = 150

/** How long a playback the server stopped shows why, before the player leaves by itself. */
internal const val STOPPED_LEAVE_MS = 20_000L

internal const val TAG_SYNC = "pley.tv.player.sync"
internal const val TAG_AUDIO_DELAY = "pley.tv.player.audioDelay"
internal const val TAG_SLEEP_NOTICE = "pley.tv.player.sleepNotice"
