package app.pley.android.ui.phone.music

import androidx.compose.animation.Crossfade
import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.semantics.setProgress
import androidx.compose.ui.semantics.progressBarRangeInfo
import androidx.compose.ui.semantics.ProgressBarRangeInfo
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.draw.drawBehind
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.gestures.detectHorizontalDragGestures
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.sizeIn
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.layout.only
import androidx.compose.foundation.layout.WindowInsetsSides
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyMotion
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.image.Artwork
import app.pley.android.designsystem.image.ArtworkImage
import app.pley.android.designsystem.reducedMotion
import app.pley.android.model.format.formatClockMs
import app.pley.android.player.BOOK_BACK_MS
import app.pley.android.player.BOOK_FORWARD_MS
import app.pley.android.player.BOOK_SPEEDS
import app.pley.android.player.MusicError
import app.pley.android.player.MusicRepeat
import app.pley.android.player.NowPlaying
import app.pley.android.screens.PleyKey
import app.pley.android.player.MusicControl
import app.pley.android.player.sleep.SleepState
import app.pley.android.designsystem.icon.PleyIcon
import app.pley.android.ui.phone.player.CarGap
import app.pley.android.ui.phone.player.CarSized
import app.pley.android.ui.phone.player.LocalCarScreen
import app.pley.android.ui.phone.player.SleepNotice
import app.pley.android.ui.phone.player.onCarScreen
import androidx.compose.foundation.layout.BoxScope
import java.text.NumberFormat
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.flow.MutableStateFlow
import app.pley.android.screens.R as ScreensR
import app.pley.android.screens.music.NowPlayingState
import app.pley.android.screens.music.NowPlayingViewModel
import app.pley.android.screens.music.QueueRow
import app.pley.android.ui.phone.R
import app.pley.android.ui.phone.kit.EmptyState
import app.pley.android.ui.phone.kit.PleyIconButton
import app.pley.android.ui.phone.kit.PleySheet
import app.pley.android.ui.phone.shell.LocalPhoneNavigator

/**
 * Now Playing on the phone (design spec §4.10): the art at 80 % of the
 * width over a faint copy of itself, the title, the artist and album, the
 * scrubber with its times, and Shuffle, Previous, Play/Pause, Next and
 * Repeat; the queue, in a sheet, and the sleep timer at the top right. In
 * a short, wide window (a phone on its side, a car's screen) the art sits
 * beside the title, as Android Auto's own Now Playing has it, over the
 * scrubber and the controls, so all of it shows. On a car's screen it is
 * car-sized ([CarSized]): 64 dp buttons and no text under 24 sp.
 *
 * A change of track moves, it doesn't blink: the cover stays until the
 * next one fades in over it ([TrackArt]), the title and artist give way to
 * the next track's ([TrackChange]), the scrubber's thumb runs back to the
 * start, and "Loading…" shows only for a track that takes a while.
 */
@Composable
internal fun NowPlayingScreen(car: Boolean = onCarScreen()) {
    CarSized(car) { NowPlayingPage() }
}

@Composable
private fun NowPlayingPage() {
    val model: NowPlayingViewModel = viewModel(factory = NowPlayingViewModel.factory)
    val state by model.state.collectAsStateWithLifecycle()
    val navigator = LocalPhoneNavigator.current
    var queue by rememberSaveable { mutableStateOf(false) }
    val now = state.now
    val art = remember(now?.artUrl) { now?.artUrl?.let { ArtworkImage(listOf(it)) } }
    val music = rememberMusicControl()
    val noSleep = remember { MutableStateFlow(SleepState()) }
    val sleep by (music?.sleep ?: noSleep).collectAsStateWithLifecycle()
    Box(Modifier.fillMaxSize().background(PleyColors.Bg).testTag(NOW_PLAYING_PAGE)) {
        // The art, faint and unblurred, fading into the page.
        TrackArt(
            art,
            now?.title.orEmpty(),
            Modifier
                .fillMaxWidth()
                .aspectRatio(1f)
                .graphicsLayer { alpha = BACKDROP_ALPHA }
                .drawWithContent {
                    drawContent()
                    drawRect(Brush.verticalGradient(0.3f to Color.Transparent, 1f to PleyColors.Bg))
                },
            placeholder = false,
        )
        Column(
            Modifier
                .fillMaxSize()
                .windowInsetsPadding(WindowInsets.safeDrawing.only(WindowInsetsSides.Top + WindowInsetsSides.Horizontal)),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            // On a car's screen, clear of its edges, and without the label: the page says what it is.
            val car = LocalCarScreen.current
            val edge = if (car) CarGap else 4.dp
            Row(Modifier.fillMaxWidth().padding(start = edge, end = edge, top = edge, bottom = 4.dp), verticalAlignment = Alignment.CenterVertically) {
                PleyIconButton(PleyIcons.ChevronDown, stringResource(R.string.music_close_now_playing), onClick = navigator::back)
                if (car) {
                    Spacer(Modifier.weight(1f))
                } else {
                    // As much room on the label's left as the buttons take on its right: it stays centered.
                    if (now != null) Spacer(Modifier.width(if (now.book) 96.dp else 48.dp))
                    Text(
                        stringResource(R.string.music_now_playing),
                        style = MaterialTheme.typography.labelLarge,
                        color = PleyColors.Muted,
                        textAlign = TextAlign.Center,
                        modifier = Modifier.weight(1f).semantics { heading() },
                    )
                }
                if (now?.book == true) SpeedButton(model)
                if (now != null) {
                    PleyIconButton(PleyIcons.ListMusic, stringResource(R.string.music_queue), onClick = { queue = true }, tint = PleyColors.Muted)
                    if (car) Spacer(Modifier.width(CarGap))
                }
                if (music != null && now != null) MusicSleepButton(sleep, music) else Spacer(Modifier.width(48.dp))
            }
            if (now == null) {
                EmptyState(
                    title = stringResource(R.string.music_nothing_playing),
                    body = stringResource(R.string.music_nothing_playing_body),
                    icon = PleyIcons.Music,
                )
                return@Column
            }
            BoxWithConstraints(Modifier.weight(1f).fillMaxWidth(), contentAlignment = Alignment.TopCenter) {
                val onAlbum = now.albumId?.let { id -> { navigator.open(PleyKey.Album(id, now.itemId)) } }
                if (maxWidth > maxHeight && maxHeight < WIDE_BELOW) {
                    WideNowPlaying(now, art, model, onAlbum)
                } else {
                    // The art is as wide as its share of the column, and no taller than what the title, the scrubber and the
                    // controls leave: on a window a little too short for all of it at that size (a car's portrait screen,
                    // 1042 × 991 at 185 dpi on some), it is the art that is smaller, not the controls that are cut off.
                    val artSide = (minOf(maxWidth, TALL_MAX) - TALL_EDGE * 2) * ART_SHARE
                    Column(
                        Modifier
                            .fillMaxHeight()
                            .widthIn(max = TALL_MAX)
                            .windowInsetsPadding(WindowInsets.safeDrawing.only(WindowInsetsSides.Bottom))
                            .padding(start = TALL_EDGE, end = TALL_EDGE, bottom = if (LocalCarScreen.current) CarGap else 16.dp),
                        horizontalAlignment = Alignment.CenterHorizontally,
                        verticalArrangement = Arrangement.SpaceEvenly,
                    ) {
                        Box(
                            Modifier
                                .weight(1f, fill = false)
                                // Air under the cover also where it takes all the height left: the title sat against it.
                                .padding(bottom = ART_GAP)
                                .sizeIn(maxWidth = artSide, maxHeight = artSide)
                                .aspectRatio(1f)
                                .clip(PleyShape.Card)
                                .background(PleyColors.Surface2),
                        ) {
                            TrackArt(art, now.album ?: now.title, Modifier.matchParentSize())
                        }
                        TrackTitle(now, onAlbum, Modifier.fillMaxWidth(), centred = true)
                        Scrubber(now, model)
                        Controls(now, model)
                    }
                }
            }
        }
        MusicSleepNotice(sleep, music)
    }
    if (queue) QueueSheet(state, onPick = model::jumpTo, onDismiss = { queue = false })
}

/**
 * A short, wide window's Now Playing: the art beside the title, then the
 * scrubber and the controls across the width (no wider than a phone's).
 */
@Composable
private fun WideNowPlaying(now: NowPlaying, art: ArtworkImage?, model: NowPlayingViewModel, onAlbum: (() -> Unit)?) {
    Column(
        Modifier
            .fillMaxHeight()
            .widthIn(max = WIDE_MAX)
            .windowInsetsPadding(WindowInsets.safeDrawing.only(WindowInsetsSides.Bottom))
            .padding(start = 24.dp, end = 24.dp, bottom = if (LocalCarScreen.current) CarGap else 16.dp),
        // A car's touch targets keep 24 dp apart: the scrubber's and the controls' under it too.
        verticalArrangement = Arrangement.spacedBy(if (LocalCarScreen.current) CarGap else 12.dp),
    ) {
        Row(Modifier.weight(1f).fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
            Box(
                Modifier
                    .fillMaxHeight()
                    .aspectRatio(1f, matchHeightConstraintsFirst = true)
                    .clip(PleyShape.Card)
                    .background(PleyColors.Surface2),
            ) {
                TrackArt(art, now.album ?: now.title, Modifier.matchParentSize())
            }
            Spacer(Modifier.width(24.dp))
            TrackTitle(now, onAlbum, Modifier.weight(1f), centred = false)
        }
        Scrubber(now, model, inline = true)
        Controls(now, model)
    }
}

/** The title, "Artist · Album" (which opens the album), and the status line; the first two change with the track. */
@Composable
private fun TrackTitle(now: NowPlaying, onAlbum: (() -> Unit)?, modifier: Modifier, centred: Boolean) {
    val align = if (centred) TextAlign.Center else TextAlign.Start
    val across = if (centred) Alignment.CenterHorizontally else Alignment.Start
    Column(modifier, horizontalAlignment = across) {
        TrackChange(trackWords(now), Modifier.fillMaxWidth(), if (centred) Alignment.TopCenter else Alignment.TopStart) { words ->
            Column(Modifier.fillMaxWidth(), horizontalAlignment = across) {
                Text(
                    words.title,
                    style = MaterialTheme.typography.headlineSmall,
                    color = PleyColors.Fg,
                    textAlign = align,
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis,
                )
                Spacer(Modifier.height(4.dp))
                Text(
                    words.byline,
                    style = MaterialTheme.typography.bodyLarge,
                    color = PleyColors.Muted,
                    textAlign = align,
                    maxLines = if (centred) 1 else 2,
                    overflow = TextOverflow.Ellipsis,
                    modifier = onAlbum?.let { Modifier.clickable(role = Role.Button, onClick = it) } ?: Modifier,
                )
            }
        }
        // Beside the art, the block centres on it: a line kept for a status to come would lift it.
        StatusLine(now, keepRoom = centred)
    }
}

/** The sleep timer's notice over Now Playing, under its top bar. */
@Composable
private fun BoxScope.MusicSleepNotice(sleep: SleepState, music: MusicControl?) {
    if (music == null) return
    SleepNotice(sleep.secondsLeft, music = true, onKeep = music::keepListening, modifier = Modifier.padding(top = 52.dp))
}

/**
 * Loading or failing: one line under the byline, its room kept while there
 * is none when [keepRoom]. A track says it loads only once it has for
 * [LOADING_AFTER_MS]: most start sooner, and the line would blink at every
 * change of track.
 */
@Composable
private fun StatusLine(now: NowPlaying, keepRoom: Boolean = true) {
    val slow by produceState(false, now.loading, now.itemId, now.index) {
        value = false
        if (now.loading) {
            delay(LOADING_AFTER_MS)
            value = true
        }
    }
    val nextIn = now.nextIn
    val text = when {
        now.error == MusicError.Offline -> stringResource(R.string.music_error_offline)
        now.error == MusicError.SignedOut -> stringResource(R.string.music_error_signed_out)
        now.error == MusicError.Stopped -> stringResource(R.string.music_error_stopped)
        now.error != null && nextIn != null -> pluralStringResource(R.plurals.music_error_unplayable_next, nextIn, nextIn)
        now.error != null -> stringResource(R.string.music_error_unplayable)
        now.loading && slow -> stringResource(R.string.music_loading)
        else -> null
    }
    if (text == null && !keepRoom) return
    Box(Modifier.heightIn(min = 24.dp).padding(top = 6.dp)) {
        Crossfade(text to (now.error != null), animationSpec = PleyMotion.reveal(), label = "status") { (words, failed) ->
            if (words != null) {
                Text(words, style = MaterialTheme.typography.bodySmall, color = if (failed) PleyColors.DangerText else PleyColors.Subtle)
            }
        }
    }
}

/**
 * The scrubber and its times; it follows the track, or the finger while
 * dragged. [inline] puts the times either side of it, on one 48 dp row
 * (64 on a car's screen), where a short window has no room for two.
 *
 * The thumb glides from one reading of the position to the next while the
 * track plays, and runs to where a seek or another track puts it (at once
 * with reduced motion). A seek shows where it went as it is made, paused
 * too, whether it is made here or from a car, the notification or the
 * profile's remote: the position is read again then.
 */
@Composable
private fun Scrubber(now: NowPlaying, model: NowPlayingViewModel, inline: Boolean = false) {
    val car = LocalCarScreen.current
    val playingTrack = now.itemId to now.index
    val reading = rememberMusicPosition(model::position, now.playing, key = trackKey(now))
    val position = reading.value
    var dragging by remember { mutableStateOf<Float?>(null) }
    val duration = position.durationMs.takeIf { it > 0 } ?: now.durationMs
    val played = if (duration > 0) (position.positionMs.toFloat() / duration).coerceIn(0f, 1f) else 0f
    val fraction = dragging ?: played
    val shownMs = (fraction * duration).toLong()
    val scrubberLabel = stringResource(R.string.music_position)
    val enabled = duration > 0 && now.error == null
    val buffered = if (duration > 0) (position.bufferedMs.toFloat() / duration).coerceIn(0f, 1f) else 0f
    val seek: (Float) -> Unit = { to ->
        model.seekTo((to.coerceIn(0f, 1f) * duration).toLong())
        reading.refresh()
    }
    val reduced = reducedMotion()
    val thumbAt = remember { Animatable(played) }
    val scope = rememberCoroutineScope()
    LaunchedEffect(played, now.playing, playingTrack, reduced) {
        val step = played - thumbAt.value
        when {
            reduced -> thumbAt.snapTo(played)
            // Playing on: from one reading to the next, in the time between them.
            now.playing && step > 0f && step * duration <= 2 * POSITION_EVERY_MS -> thumbAt.animateTo(played, tween(POSITION_EVERY_MS.toInt(), easing = LinearEasing))
            // A seek, or another track.
            else -> thumbAt.animateTo(played, tween(PleyMotion.OVERLAY_MS, easing = PleyMotion.EaseOut))
        }
    }
    // The phone's scrubber (design spec §3.5): a 4 dp track, accent played, a 16 dp accent thumb that grows to 24 while
    // dragged; on a car's screen, 6 dp, 21 and 27 (8, 28 and 36 there).
    val trackHeight = if (car) 6.dp else 4.dp
    val thumbSize = if (car) 21.dp else 16.dp
    val thumbDragged = if (car) 27.dp else 24.dp
    val track = Modifier
        .semantics {
            contentDescription = scrubberLabel
            stateDescription = "${formatClockMs(shownMs)} / ${formatClockMs(duration)}"
            progressBarRangeInfo = ProgressBarRangeInfo(fraction, 0f..1f)
            if (enabled) {
                setProgress { target ->
                    seek(target)
                    true
                }
            }
        }
        .pointerInput(enabled, duration) {
            if (!enabled) return@pointerInput
            detectTapGestures { offset -> seek(offset.x / size.width) }
        }
        .pointerInput(enabled, duration) {
            if (!enabled) return@pointerInput
            detectHorizontalDragGestures(
                onDragStart = { offset -> dragging = (offset.x / size.width).coerceIn(0f, 1f) },
                onDragEnd = {
                    val to = dragging
                    if (to != null) {
                        seek(to)
                        // The thumb stays where it was let go, rather than back where the track was and gliding over.
                        scope.launch {
                            thumbAt.snapTo(to)
                            dragging = null
                        }
                    }
                },
                onDragCancel = { dragging = null },
            ) { change, _ -> dragging = (change.position.x / size.width).coerceIn(0f, 1f) }
        }
        .drawBehind {
            val h = trackHeight.toPx()
            val y = size.height / 2f
            val r = CornerRadius(h / 2f, h / 2f)
            drawRoundRect(PleyColors.Fg.copy(alpha = 0.25f), Offset(0f, y - h / 2f), Size(size.width, h), r)
            drawRoundRect(PleyColors.Fg.copy(alpha = 0.45f), Offset(0f, y - h / 2f), Size(size.width * buffered, h), r)
            // Under the finger while dragged, else where the thumb has glided to.
            val at = dragging ?: thumbAt.value
            drawRoundRect(if (enabled) PleyColors.Accent else PleyColors.Disabled, Offset(0f, y - h / 2f), Size(size.width * at, h), r)
            val thumb = (if (dragging != null) thumbDragged else thumbSize).toPx() / 2f
            drawCircle(if (enabled) PleyColors.Accent else PleyColors.Disabled, thumb, Offset((size.width * at).coerceIn(thumb, size.width - thumb), y))
        }
    val elapsed = formatClockMs(shownMs)
    val left = "−" + formatClockMs((duration - shownMs).coerceAtLeast(0))
    if (inline) {
        Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
            Text(elapsed, style = MaterialTheme.typography.bodySmall, color = PleyColors.Muted)
            Spacer(Modifier.width(16.dp))
            Box(Modifier.weight(1f).height(48.dp).then(track))
            Spacer(Modifier.width(16.dp))
            Text(left, style = MaterialTheme.typography.bodySmall, color = PleyColors.Muted)
        }
    } else {
        Column(Modifier.fillMaxWidth()) {
            Box(Modifier.fillMaxWidth().height(if (car) 48.dp else 40.dp).then(track))
            Row(Modifier.fillMaxWidth()) {
                Text(elapsed, style = MaterialTheme.typography.bodySmall, color = PleyColors.Muted)
                Spacer(Modifier.weight(1f))
                Text(left, style = MaterialTheme.typography.bodySmall, color = PleyColors.Muted)
            }
        }
    }
}

/**
 * Shuffle, Previous, Play/Pause, Next and Repeat, Play the largest, in the
 * middle. On a car's screen too narrow for the five 24 dp apart (a narrow
 * portrait one), Play is a car's smallest target, as the others are.
 */
@Composable
private fun Controls(now: NowPlaying, model: NowPlayingViewModel) = BoxWithConstraints(Modifier.fillMaxWidth()) {
    val playSize = if (LocalCarScreen.current && maxWidth < CAR_CONTROLS_WIDTH) 48.dp else 72.dp
    Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween, verticalAlignment = Alignment.CenterVertically) {
        // A book steps back 15 seconds and forward 30 in place of Shuffle and Repeat, as audiobook apps do.
        if (now.book) {
            StepButton(PleyIcons.RotateCcw, BOOK_BACK_MS, stringResource(ScreensR.string.book_back), onClick = model::back)
        } else {
            ModeButton(
            PleyIcons.Shuffle,
            stringResource(if (now.shuffle) R.string.music_shuffle_off else R.string.music_shuffle_on),
            on = now.shuffle,
            onClick = { model.setShuffle(!now.shuffle) },
        )
        }
        SkipButton(PleyIcons.SkipBack, stringResource(R.string.music_previous), onClick = model::previous)
        // What it will do, a track still starting too; paused, it stays Play through a seek's loading.
        val playing = now.wantsPlay
        val label = stringResource(if (playing) R.string.music_pause else R.string.music_play)
        Box(
            Modifier
                .size(playSize)
                .clip(PleyShape.Circle)
                .background(PleyColors.Accent)
                .clickable(role = Role.Button, onClick = model::toggle)
                .semantics { contentDescription = label },
            contentAlignment = Alignment.Center,
        ) {
            PlayPauseGlyph(playing, size = playSize * 4 / 9, tint = PleyColors.OnAccent)
        }
        SkipButton(PleyIcons.SkipForward, stringResource(R.string.music_next), onClick = model::next, enabled = now.hasNext)
        if (now.book) {
            StepButton(PleyIcons.RotateCw, BOOK_FORWARD_MS, stringResource(ScreensR.string.book_forward), onClick = model::forward)
            return@Row
        }
        ModeButton(
            if (now.repeat == MusicRepeat.One) PleyIcons.Repeat1 else PleyIcons.Repeat,
            stringResource(
                when (now.repeat) {
                    MusicRepeat.Off -> R.string.music_repeat_on
                    MusicRepeat.All -> R.string.music_repeat_one
                    MusicRepeat.One -> R.string.music_repeat_off
                },
            ),
            on = now.repeat != MusicRepeat.Off,
            onClick = { model.setRepeat(now.repeat.next) },
        )
    }
}

/** A book's back or forward [stepMs]: the arrow round, the seconds inside it. */
@Composable
private fun StepButton(icon: PleyIcon, stepMs: Long, label: String, onClick: () -> Unit) {
    Box(
        Modifier
            .size(48.dp)
            .clip(PleyShape.Circle)
            .clickable(role = Role.Button, onClick = onClick)
            .semantics { contentDescription = label },
        contentAlignment = Alignment.Center,
    ) {
        Icon(icon, contentDescription = null, size = if (LocalCarScreen.current) 34.dp else 30.dp, tint = PleyColors.Fg)
        Text((stepMs / 1000).toString(), style = MaterialTheme.typography.labelSmall, color = PleyColors.Fg, modifier = Modifier.padding(top = 2.dp))
    }
}

/**
 * The speed a book plays at ("1.2×"), in the top bar: a press lists the
 * speeds, slowest first, the one it plays at checked.
 */
@Composable
private fun SpeedButton(model: NowPlayingViewModel) {
    val speed by model.bookSpeed.collectAsStateWithLifecycle()
    var open by rememberSaveable { mutableStateOf(false) }
    val label = stringResource(ScreensR.string.book_speed)
    val shown = speedText(speed)
    Box(
        Modifier
            .size(48.dp)
            .clip(PleyShape.Circle)
            .clickable(role = Role.Button, onClickLabel = label, onClick = { open = true })
            .semantics { contentDescription = "$label, $shown" },
        contentAlignment = Alignment.Center,
    ) {
        Text(shown, style = MaterialTheme.typography.labelLarge, color = if (speed != 1f) PleyColors.AccentBright else PleyColors.Muted)
    }
    if (open) {
        PleySheet(onDismiss = { open = false }, title = label) {
            BOOK_SPEEDS.sorted().forEach { choice ->
                Row(
                    Modifier
                        .fillMaxWidth()
                        .heightIn(min = 56.dp)
                        .clickable(role = Role.RadioButton) {
                            model.setBookSpeed(choice)
                            open = false
                        }
                        .padding(horizontal = 24.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Text(speedText(choice), style = MaterialTheme.typography.bodyLarge, color = PleyColors.Fg, modifier = Modifier.weight(1f))
                    if (choice == speed) Icon(PleyIcons.Check, contentDescription = null, size = 20.dp, tint = PleyColors.Accent)
                }
            }
        }
    }
}

/** "1.5×", "1×", in the language shown. */
@Composable
internal fun speedText(speed: Float): String {
    val locale = LocalResources.current.configuration.locales[0]
    return remember(speed, locale) { NumberFormat.getNumberInstance(locale).apply { maximumFractionDigits = 1 }.format(speed) + "×" }
}

/** Shuffle or Repeat: in accent while [on], named for what a press does. */
@Composable
private fun ModeButton(icon: PleyIcon, label: String, on: Boolean, onClick: () -> Unit) {
    val tint by animateColorAsState(if (on) PleyColors.Accent else PleyColors.Muted, PleyMotion.reveal(), label = "mode")
    PleyIconButton(icon, label, onClick = onClick, tint = tint)
}

/** Previous or Next: an icon button, its glyph larger on a car's screen, where it sits beside a 96 dp Play. */
@Composable
private fun SkipButton(icon: PleyIcon, label: String, onClick: () -> Unit, enabled: Boolean = true) {
    if (!LocalCarScreen.current) {
        PleyIconButton(icon, label, onClick = onClick, enabled = enabled)
        return
    }
    Box(
        Modifier
            .size(48.dp)
            .clip(PleyShape.Circle)
            .clickable(enabled = enabled, role = Role.Button, onClick = onClick)
            .semantics { contentDescription = label },
        contentAlignment = Alignment.Center,
    ) {
        Icon(icon, contentDescription = null, size = 30.dp, tint = if (enabled) PleyColors.Fg else PleyColors.Disabled)
    }
}

/** The queue (design spec §4.10: "the queue in a sheet"): the track playing in accent; tapping one plays it. */
@Composable
private fun QueueSheet(state: NowPlayingState, onPick: (Int) -> Unit, onDismiss: () -> Unit) {
    PleySheet(onDismiss = onDismiss, title = stringResource(R.string.music_queue)) {
        val current = state.queue.indexOfFirst { it.current }.coerceAtLeast(0)
        val list = rememberLazyListState(initialFirstVisibleItemIndex = (current - 2).coerceAtLeast(0))
        LazyColumn(Modifier.fillMaxWidth().heightIn(max = 560.dp), state = list) {
            items(state.queue, key = { it.position }, contentType = { "row" }) { row ->
                QueueLine(row, state.origin, playing = state.now?.playing == true, onClick = { onPick(row.position) })
            }
        }
    }
}

@Composable
private fun QueueLine(row: QueueRow, origin: String, playing: Boolean, onClick: () -> Unit) {
    val art = remember(row.artPath, origin) { row.artPath?.let { ArtworkImage.of(it, origin) } }
    Row(
        Modifier
            .fillMaxWidth()
            .heightIn(min = 64.dp)
            .clickable(role = Role.Button, onClick = onClick)
            .padding(horizontal = 24.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(Modifier.size(44.dp).clip(PleyShape.Badge).background(PleyColors.Surface3)) {
            Artwork(art, row.title, Modifier.matchParentSize())
        }
        Spacer(Modifier.width(14.dp))
        Column(Modifier.weight(1f)) {
            Text(
                row.title,
                style = MaterialTheme.typography.titleMedium,
                color = if (row.current) PleyColors.AccentBright else PleyColors.Fg,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            if (row.subtitle.isNotEmpty()) {
                Text(row.subtitle, style = MaterialTheme.typography.bodySmall, color = PleyColors.Muted, maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
        }
        Spacer(Modifier.width(12.dp))
        if (row.current) {
            Icon(if (playing) PleyIcons.AudioLines else PleyIcons.Pause, contentDescription = null, size = 18.dp, tint = PleyColors.Accent)
        } else {
            Text(row.duration, style = MaterialTheme.typography.bodySmall, color = PleyColors.Muted)
        }
    }
}

/** Windows less tall than this, and wider than tall, put the art beside the title. */
private val WIDE_BELOW = 560.dp

/** The widest the tall layout's column gets, and its side margins. */
private val TALL_MAX = 520.dp
private val TALL_EDGE = 24.dp

/** The widest a wide window's controls spread. */
private val WIDE_MAX = 640.dp

/** The controls' row on a car's screen with Play at its usual size: four 48 dp buttons, Play's 72 and 24 dp between each once scaled. */
private val CAR_CONTROLS_WIDTH = 48.dp * 4 + 72.dp + CarGap * 4

/** A track loading this long says so. */
private const val LOADING_AFTER_MS = 700L

/** The art's share of the width. */
private const val ART_SHARE = 0.8f

/** The least room between the art and the title under it. */
private val ART_GAP = 16.dp

/** The faint copy of the art behind the page. */
private const val BACKDROP_ALPHA = 0.18f

internal const val NOW_PLAYING_PAGE = "pley.now_playing.page"
