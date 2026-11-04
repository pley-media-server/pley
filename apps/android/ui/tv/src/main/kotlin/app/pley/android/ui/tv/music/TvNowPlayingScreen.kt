package app.pley.android.ui.tv.music

import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.snap
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.focusGroup
import androidx.compose.foundation.focusable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsFocusedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusProperties
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onKeyEvent
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.ProgressBarRangeInfo
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.progressBarRangeInfo
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import androidx.tv.material3.Glow
import androidx.tv.material3.IconButton
import androidx.tv.material3.IconButtonDefaults
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyFocus
import app.pley.android.designsystem.PleyMotion
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.PleyTv
import app.pley.android.designsystem.PleyType
import app.pley.android.designsystem.TvSafeArea
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.image.Artwork
import app.pley.android.designsystem.image.ArtworkImage
import app.pley.android.designsystem.reducedMotion
import app.pley.android.model.format.formatClockMs
import app.pley.android.player.MusicError
import app.pley.android.player.NowPlaying
import app.pley.android.player.sleep.SleepChoice
import app.pley.android.player.sleep.SleepTexts
import app.pley.android.screens.music.NowPlayingState
import app.pley.android.screens.music.NowPlayingViewModel
import app.pley.android.ui.tv.R
import app.pley.android.ui.tv.kit.TvRoundButton
import app.pley.android.ui.tv.kit.HeldEnds
import app.pley.android.ui.tv.kit.heldKeyStaysIn
import app.pley.android.ui.tv.kit.initialFocus
import app.pley.android.ui.tv.player.MoonIcon
import app.pley.android.ui.tv.shell.LocalTvNavigator
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.collectLatest

/**
 * Now Playing on TV (design spec §4.10): the album art at 300 dp at
 * (96, 58), the title, the artist and album and the scrubber beside it,
 * Shuffle, Previous, Play/Pause and Next as round buttons, and the queue
 * under them, over a faint copy of the art. Focus starts on Play/Pause.
 * The scrubber takes focus: Left and Right seek 10 s. The music plays on
 * when the screen is left; after two minutes without a key it dims to
 * 30 % over 400 ms, to spare the screen, and the next key wakes it over
 * 200 ms (at once with reduced motion).
 */
@Composable
internal fun TvNowPlayingScreen() {
    val model: NowPlayingViewModel = viewModel(factory = NowPlayingViewModel.factory)
    val state by model.state.collectAsStateWithLifecycle()
    val navigator = LocalTvNavigator.current
    // Read by the dimming's effect only: a key doesn't recompose the page.
    var lastKey by remember { mutableLongStateOf(System.currentTimeMillis()) }
    var dimmed by remember { mutableStateOf(false) }
    LaunchedEffect(Unit) {
        snapshotFlow { lastKey }.collectLatest {
            dimmed = false
            delay(DIM_AFTER_MS)
            dimmed = true
        }
    }
    val reduced = reducedMotion()
    val dim = animateFloatAsState(
        targetValue = if (dimmed) 1f - DIMMED else 0f,
        animationSpec = if (reduced) snap() else tween(if (dimmed) PleyMotion.REST_MS else PleyMotion.OVERLAY_MS, easing = LinearEasing),
        label = "dim",
    )
    val now = state.now
    val art = remember(now?.artUrl) { now?.artUrl?.let { ArtworkImage(listOf(it)) } }
    Box(
        Modifier
            .fillMaxSize()
            .onPreviewKeyEvent {
                lastKey = System.currentTimeMillis()
                false
            }
            .drawWithContent {
                drawContent()
                val black = dim.value
                if (black > 0f) drawRect(Color.Black.copy(alpha = black))
            }
            .testTag(TV_NOW_PLAYING_PAGE),
    ) {
        FaintArt(art, now?.title.orEmpty(), alpha = 0.2f)
        if (now == null) {
            MusicMessage(
                title = stringResource(R.string.tv_music_nothing_playing),
                body = stringResource(R.string.tv_music_nothing_playing_body),
                action = stringResource(R.string.tv_music_back),
                onAction = navigator::back,
            )
            return@Box
        }
        Row(Modifier.fillMaxSize().padding(start = PleyTv.Gutter, top = 58.dp, end = TvSafeArea.Horizontal)) {
            Box(Modifier.size(ART).clip(PleyShape.Dialog).background(PleyColors.Surface2)) {
                Artwork(art, now.album ?: now.title, Modifier.matchParentSize())
            }
            Spacer(Modifier.width(32.dp))
            Column(Modifier.fillMaxSize().focusGroup()) {
                BasicText(now.title, style = PleyType.Tv.PageTitle.copy(color = PleyColors.Fg), maxLines = 2, overflow = TextOverflow.Ellipsis)
                Spacer(Modifier.height(4.dp))
                BasicText(now.byline, style = PleyType.Tv.BodyLarge.copy(color = PleyColors.Muted), maxLines = 1, overflow = TextOverflow.Ellipsis)
                Status(now, model)
                Spacer(Modifier.height(14.dp))
                Scrubber(now, model)
                Spacer(Modifier.height(16.dp))
                Controls(now, model)
                Queue(state, onPick = model::jumpTo)
            }
        }
    }
}

@Composable
private fun Status(now: NowPlaying, model: NowPlayingViewModel) {
    val sleep by model.sleep.collectAsStateWithLifecycle()
    val pausingIn = sleep.secondsLeft
    val text = when {
        now.error == MusicError.Offline -> stringResource(R.string.tv_music_error_offline)
        now.error == MusicError.SignedOut -> stringResource(R.string.tv_music_error_signed_out)
        now.error == MusicError.Stopped -> stringResource(R.string.tv_music_error_stopped)
        now.error != null -> stringResource(R.string.tv_music_error_unplayable)
        now.loading -> stringResource(R.string.tv_music_loading)
        pausingIn != null -> SleepTexts.notice(LocalResources.current, pausingIn)
        else -> ""
    }
    BasicText(
        text,
        style = PleyType.Tv.Caption.copy(color = if (now.error != null) PleyColors.DangerText else PleyColors.Subtle),
        modifier = Modifier.height(22.dp).padding(top = 4.dp),
    )
}

/**
 * The scrubber (design spec §2.7, here as wide as the column): a 4 dp
 * track, the played part in accent, the thumb a 10 dp circle that grows
 * to 12 dp with a slim ring when focused, as the player's; the times at
 * either end.
 * Focused, Left and Right seek 10 s and Select plays or pauses.
 */
@Composable
private fun Scrubber(now: NowPlaying, model: NowPlayingViewModel) {
    val interactions = remember { MutableInteractionSource() }
    val focused by interactions.collectIsFocusedAsState()
    val position by rememberPosition(model::position, now.playing, key = now.itemId to now.index)
    val duration = position.durationMs.takeIf { it > 0 } ?: now.durationMs
    val label = stringResource(R.string.tv_music_position)
    Column(Modifier.fillMaxWidth()) {
        Box(
            Modifier
                .fillMaxWidth()
                .height(24.dp)
                .semantics {
                    contentDescription = label
                    progressBarRangeInfo = ProgressBarRangeInfo(position.positionMs.toFloat(), 0f..duration.coerceAtLeast(1).toFloat())
                }
                .onKeyEvent { event ->
                    if (event.type != KeyEventType.KeyDown) return@onKeyEvent false
                    when (event.key) {
                        Key.DirectionLeft -> {
                            model.seekTo((model.position().positionMs - SEEK_MS).coerceAtLeast(0))
                            true
                        }
                        Key.DirectionRight -> {
                            model.seekTo((model.position().positionMs + SEEK_MS).coerceAtMost(duration))
                            true
                        }
                        Key.DirectionCenter, Key.Enter -> {
                            model.toggle()
                            true
                        }
                        else -> false
                    }
                }
                .focusable(interactionSource = interactions)
                .drawBehind {
                    val h = 4.dp.toPx()
                    val y = size.height / 2f
                    val f = if (duration > 0) (position.positionMs.toFloat() / duration).coerceIn(0f, 1f) else 0f
                    val r = CornerRadius(h / 2f, h / 2f)
                    drawRoundRect(PleyColors.Fg.copy(alpha = 0.25f), Offset(0f, y - h / 2f), Size(size.width, h), r)
                    val buffered = if (duration > 0) (position.bufferedMs.toFloat() / duration).coerceIn(0f, 1f) else 0f
                    drawRoundRect(PleyColors.Fg.copy(alpha = 0.45f), Offset(0f, y - h / 2f), Size(size.width * buffered, h), r)
                    drawRoundRect(PleyColors.Accent, Offset(0f, y - h / 2f), Size(size.width * f, h), r)
                    val thumb = (if (focused) PleyTv.ScrubThumbFocused else PleyTv.ScrubThumb).toPx() / 2f
                    val x = (size.width * f).coerceIn(thumb, size.width - thumb)
                    drawCircle(PleyColors.Fg, thumb, Offset(x, y))
                    if (focused) drawCircle(PleyColors.AccentBright, thumb + PleyFocus.ThumbRingOffset.toPx(), Offset(x, y), style = Stroke(PleyFocus.ThumbRingStroke.toPx()))
                },
        )
        Row(Modifier.fillMaxWidth().padding(top = 4.dp)) {
            BasicText(formatClockMs(position.positionMs), style = PleyType.Tv.Caption.copy(color = PleyColors.Fg))
            Spacer(Modifier.weight(1f))
            BasicText("−" + formatClockMs((duration - position.positionMs).coerceAtLeast(0)), style = PleyType.Tv.Caption.copy(color = PleyColors.Fg))
        }
    }
}

@Composable
private fun Controls(now: NowPlaying, model: NowPlayingViewModel) {
    Row(
        Modifier.height(64.dp + PleyTv.ButtonLabelRoom),
        horizontalArrangement = Arrangement.spacedBy(16.dp),
        verticalAlignment = Alignment.Top,
    ) {
        Box(Modifier.padding(top = 8.dp)) {
            TvRoundButton(
                stringResource(if (now.shuffle) R.string.tv_music_shuffle_off else R.string.tv_music_shuffle),
                PleyIcons.Shuffle,
                onClick = { model.setShuffle(!now.shuffle) },
                onArt = true,
            )
        }
        Box(Modifier.padding(top = 8.dp)) { TvRoundButton(stringResource(R.string.tv_music_previous), PleyIcons.SkipBack, onClick = model::previous, onArt = true) }
        PlayPause(now, onClick = model::toggle)
        Box(Modifier.padding(top = 8.dp)) { TvRoundButton(stringResource(R.string.tv_music_next), PleyIcons.SkipForward, onClick = model::next, onArt = true, enabled = now.hasNext) }
        Box(Modifier.padding(top = 8.dp)) { SleepButton(model) }
    }
}

/**
 * The sleep timer: each Select moves to the next choice (Off, 15 … 90 min,
 * End of this track), its label saying the one running; during the
 * timer's last 30 s it is "Keep listening".
 */
@Composable
private fun SleepButton(model: NowPlayingViewModel) {
    val sleep by model.sleep.collectAsStateWithLifecycle()
    val resources = LocalResources.current
    val notice = sleep.secondsLeft != null
    val label = when {
        notice -> SleepTexts.keep(resources, music = true)
        sleep.choice == SleepChoice.Off -> SleepTexts.title(resources)
        else -> SleepTexts.title(resources) + ": " + SleepTexts.summary(resources, sleep, music = true)
    }
    TvRoundButton(
        label,
        MoonIcon,
        onClick = {
            if (notice) {
                model.keepListening()
            } else {
                val all = SleepChoice.ALL
                model.setSleep(all[(all.indexOf(sleep.choice) + 1) % all.size])
            }
        },
        modifier = Modifier.testTag(TV_MUSIC_SLEEP),
        onArt = true,
    )
}

/** Play/Pause: a 64 dp circle with a 32 dp glyph (design spec §2.7), the page's initial focus. */
@Composable
private fun PlayPause(now: NowPlaying, onClick: () -> Unit) {
    val playing = now.playing || (now.loading && now.error == null)
    val label = stringResource(if (playing) R.string.tv_music_pause else R.string.tv_music_play)
    IconButton(
        onClick = onClick,
        modifier = Modifier
            .size(64.dp)
            .initialFocus()
            .semantics { contentDescription = label }
            .testTag(TV_PLAY_PAUSE),
        shape = IconButtonDefaults.shape(PleyShape.Circle),
        colors = IconButtonDefaults.colors(
            containerColor = PleyColors.Fg.copy(alpha = 0.15f),
            contentColor = PleyColors.Fg,
            focusedContainerColor = PleyColors.Accent,
            focusedContentColor = PleyColors.OnAccent,
            pressedContainerColor = PleyColors.AccentBright,
            pressedContentColor = PleyColors.OnAccent,
        ),
        scale = IconButtonDefaults.scale(focusedScale = PleyFocus.ROUND_SCALE, pressedScale = 1f),
        glow = IconButtonDefaults.glow(Glow.None, Glow.None, Glow.None),
    ) {
        Icon(if (playing) PleyIcons.Pause else PleyIcons.Play, contentDescription = null, size = 32.dp, tint = androidx.tv.material3.LocalContentColor.current)
    }
}

/**
 * The queue under the controls, from the track playing on; Select plays
 * one. Down from the controls lands on the track playing, remembered as
 * each track starts unless focus is in the queue; after a look down the
 * queue, on the line left last.
 */
@Composable
private fun Queue(state: NowPlayingState, onPick: (Int) -> Unit) {
    if (state.queue.size <= 1) return
    val current = state.queue.indexOfFirst { it.current }.coerceAtLeast(0)
    val list = rememberLazyListState(initialFirstVisibleItemIndex = current)
    val landing = remember { QueueLanding() }
    // The next track comes to the top, unless someone is looking down the queue: what has focus stays in view.
    var browsing by remember { mutableStateOf(false) }
    LaunchedEffect(current) {
        if (browsing) return@LaunchedEffect
        landing.position = current
        list.scrollToItem(current)
    }
    BasicText(stringResource(R.string.tv_music_queue), style = PleyType.Tv.Label.copy(color = PleyColors.Subtle), modifier = Modifier.padding(start = 12.dp, bottom = 4.dp))
    LazyColumn(
        Modifier
            .fillMaxSize()
            .onFocusChanged { browsing = it.hasFocus }
            .focusProperties { onEnter = { landing.enter() } }
            .heldKeyStaysIn(HeldEnds.UpDown)
            .focusGroup()
            .testTag(TV_QUEUE),
        state = list,
        contentPadding = PaddingValues(bottom = TvSafeArea.Vertical + 60.dp),
        verticalArrangement = Arrangement.spacedBy(2.dp),
    ) {
        items(state.queue, key = { it.position }, contentType = { "row" }) { row ->
            val line = remember { FocusRequester() }
            DisposableEffect(landing, row.position) {
                landing.lines[row.position] = line
                onDispose { if (landing.lines[row.position] === line) landing.lines.remove(row.position) }
            }
            TvTrackLine(
                number = (row.position + 1).toString(),
                title = row.title,
                subtitle = row.subtitle,
                duration = row.duration,
                current = row.current,
                playing = row.current && state.now?.playing == true,
                highlighted = false,
                onClick = { onPick(row.position) },
                modifier = Modifier.focusRequester(line).onFocusChanged { if (it.isFocused) landing.position = row.position },
            )
        }
    }
}

/**
 * Where focus lands in the queue, by queue position, and the lines it can
 * go to while they are composed. Not snapshot state: it follows focus, and
 * nothing is drawn from it.
 */
@Stable
private class QueueLanding {
    var position = 0
    val lines = HashMap<Int, FocusRequester>()

    /** Focus coming into the queue goes to [position]'s line; where Compose puts it when that line isn't composed. */
    fun enter() {
        lines[position]?.let { runCatching { it.requestFocus() } }
    }
}

private val ART = 300.dp

/** Left and Right on the scrubber. */
private const val SEEK_MS = 10_000L

/** Two minutes without a key, then the screen dims (design spec §4.10). */
private const val DIM_AFTER_MS = 120_000L

/** How bright the dimmed screen stays. */
private const val DIMMED = 0.3f

internal const val TV_NOW_PLAYING_PAGE = "pley.tv.now_playing"
internal const val TV_PLAY_PAUSE = "pley.tv.now_playing.play_pause"
internal const val TV_QUEUE = "pley.tv.now_playing.queue"
internal const val TV_MUSIC_SLEEP = "pley.tv.now_playing.sleep"
