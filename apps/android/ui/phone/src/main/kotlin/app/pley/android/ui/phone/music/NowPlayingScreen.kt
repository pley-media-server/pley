package app.pley.android.ui.phone.music

import androidx.compose.foundation.background
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
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
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
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.image.Artwork
import app.pley.android.designsystem.image.ArtworkImage
import app.pley.android.designsystem.image.MarkGlyph
import app.pley.android.model.format.formatClockMs
import app.pley.android.player.MusicError
import app.pley.android.player.NowPlaying
import app.pley.android.screens.PleyKey
import app.pley.android.player.MusicControl
import app.pley.android.player.sleep.SleepState
import app.pley.android.ui.phone.player.SleepNotice
import androidx.compose.foundation.layout.BoxScope
import kotlinx.coroutines.flow.MutableStateFlow
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
 * scrubber with its times, Shuffle, Previous, Play/Pause, Next, and the
 * queue in a sheet; the sleep timer at the top right.
 */
@Composable
internal fun NowPlayingScreen() {
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
        if (art != null) {
            Artwork(
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
            )
        }
        Column(
            Modifier
                .fillMaxSize()
                .windowInsetsPadding(WindowInsets.safeDrawing.only(WindowInsetsSides.Top + WindowInsetsSides.Horizontal)),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            Row(Modifier.fillMaxWidth().padding(horizontal = 4.dp, vertical = 4.dp), verticalAlignment = Alignment.CenterVertically) {
                PleyIconButton(PleyIcons.ChevronDown, stringResource(R.string.music_close_now_playing), onClick = navigator::back)
                Text(
                    stringResource(R.string.music_now_playing),
                    style = MaterialTheme.typography.labelLarge,
                    color = PleyColors.Muted,
                    textAlign = TextAlign.Center,
                    modifier = Modifier.weight(1f).semantics { heading() },
                )
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
            Column(
                Modifier.weight(1f).widthIn(max = 520.dp).padding(horizontal = 24.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.SpaceEvenly,
            ) {
                Box(
                    Modifier
                        .fillMaxWidth(ART_SHARE)
                        .aspectRatio(1f)
                        .clip(PleyShape.Card)
                        .background(PleyColors.Surface2),
                ) {
                    Artwork(art, now.album ?: now.title, Modifier.matchParentSize(), mark = MarkGlyph.Music)
                }
                Column(Modifier.fillMaxWidth(), horizontalAlignment = Alignment.CenterHorizontally) {
                    Text(
                        now.title,
                        style = MaterialTheme.typography.headlineSmall,
                        color = PleyColors.Fg,
                        textAlign = TextAlign.Center,
                        maxLines = 2,
                        overflow = TextOverflow.Ellipsis,
                    )
                    Spacer(Modifier.height(4.dp))
                    Text(
                        now.byline,
                        style = MaterialTheme.typography.bodyLarge,
                        color = PleyColors.Muted,
                        textAlign = TextAlign.Center,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                        modifier = now.albumId?.let { id -> Modifier.clickable(role = Role.Button) { navigator.open(PleyKey.Album(id, now.itemId)) } } ?: Modifier,
                    )
                    StatusLine(now)
                }
                Scrubber(now, model)
                Controls(now, model, onQueue = { queue = true })
            }
        }
        MusicSleepNotice(sleep, music)
    }
    if (queue) QueueSheet(state, onPick = model::jumpTo, onDismiss = { queue = false })
}

/** The sleep timer's notice over Now Playing, under its top bar. */
@Composable
private fun BoxScope.MusicSleepNotice(sleep: SleepState, music: MusicControl?) {
    if (music == null) return
    SleepNotice(sleep.secondsLeft, music = true, onKeep = music::keepListening, modifier = Modifier.padding(top = 52.dp))
}

/** Loading or failing: one line under the byline. */
@Composable
private fun StatusLine(now: NowPlaying) {
    val text = when {
        now.error == MusicError.Offline -> stringResource(R.string.music_error_offline)
        now.error == MusicError.SignedOut -> stringResource(R.string.music_error_signed_out)
        now.error == MusicError.Stopped -> stringResource(R.string.music_error_stopped)
        now.error != null -> stringResource(R.string.music_error_unplayable)
        now.loading -> stringResource(R.string.music_loading)
        else -> null
    }
    Box(Modifier.heightIn(min = 24.dp).padding(top = 6.dp)) {
        if (text != null) {
            Text(text, style = MaterialTheme.typography.bodySmall, color = if (now.error != null) PleyColors.DangerText else PleyColors.Subtle)
        }
    }
}

/** The scrubber and its times; it follows the track, or the finger while dragged. */
@Composable
private fun Scrubber(now: NowPlaying, model: NowPlayingViewModel) {
    val position by rememberMusicPosition(model::position, now.playing, key = now.itemId to now.index)
    var dragging by remember { mutableStateOf<Float?>(null) }
    val duration = position.durationMs.takeIf { it > 0 } ?: now.durationMs
    val fraction = dragging ?: if (duration > 0) (position.positionMs.toFloat() / duration).coerceIn(0f, 1f) else 0f
    val shownMs = (fraction * duration).toLong()
    val scrubberLabel = stringResource(R.string.music_position)
    Column(Modifier.fillMaxWidth()) {
        val enabled = duration > 0 && now.error == null
        val buffered = if (duration > 0) (position.bufferedMs.toFloat() / duration).coerceIn(0f, 1f) else 0f
        // The phone's scrubber (design spec §3.5): a 4 dp track, accent played, a 16 dp accent thumb that grows to 24 while dragged.
        Box(
            Modifier
                .fillMaxWidth()
                .height(40.dp)
                .semantics {
                    contentDescription = scrubberLabel
                    stateDescription = "${formatClockMs(shownMs)} / ${formatClockMs(duration)}"
                    progressBarRangeInfo = ProgressBarRangeInfo(fraction, 0f..1f)
                    if (enabled) {
                        setProgress { target ->
                            model.seekTo((target.coerceIn(0f, 1f) * duration).toLong())
                            true
                        }
                    }
                }
                .pointerInput(enabled, duration) {
                    if (!enabled) return@pointerInput
                    detectTapGestures { offset -> model.seekTo(((offset.x / size.width).coerceIn(0f, 1f) * duration).toLong()) }
                }
                .pointerInput(enabled, duration) {
                    if (!enabled) return@pointerInput
                    detectHorizontalDragGestures(
                        onDragStart = { offset -> dragging = (offset.x / size.width).coerceIn(0f, 1f) },
                        onDragEnd = {
                            dragging?.let { model.seekTo((it * duration).toLong()) }
                            dragging = null
                        },
                        onDragCancel = { dragging = null },
                    ) { change, _ -> dragging = (change.position.x / size.width).coerceIn(0f, 1f) }
                }
                .drawBehind {
                    val h = 4.dp.toPx()
                    val y = size.height / 2f
                    val r = CornerRadius(h / 2f, h / 2f)
                    drawRoundRect(PleyColors.Fg.copy(alpha = 0.25f), Offset(0f, y - h / 2f), Size(size.width, h), r)
                    drawRoundRect(PleyColors.Fg.copy(alpha = 0.45f), Offset(0f, y - h / 2f), Size(size.width * buffered, h), r)
                    drawRoundRect(if (enabled) PleyColors.Accent else PleyColors.Disabled, Offset(0f, y - h / 2f), Size(size.width * fraction, h), r)
                    val thumb = (if (dragging != null) 24.dp else 16.dp).toPx() / 2f
                    drawCircle(if (enabled) PleyColors.Accent else PleyColors.Disabled, thumb, Offset((size.width * fraction).coerceIn(thumb, size.width - thumb), y))
                },
        )
        Row(Modifier.fillMaxWidth()) {
            Text(formatClockMs(shownMs), style = MaterialTheme.typography.bodySmall, color = PleyColors.Muted)
            Spacer(Modifier.weight(1f))
            Text("−" + formatClockMs((duration - shownMs).coerceAtLeast(0)), style = MaterialTheme.typography.bodySmall, color = PleyColors.Muted)
        }
    }
}

@Composable
private fun Controls(now: NowPlaying, model: NowPlayingViewModel, onQueue: () -> Unit) {
    Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween, verticalAlignment = Alignment.CenterVertically) {
        PleyIconButton(
            PleyIcons.Shuffle,
            stringResource(if (now.shuffle) R.string.music_shuffle_off else R.string.music_shuffle_on),
            onClick = { model.setShuffle(!now.shuffle) },
            tint = if (now.shuffle) PleyColors.Accent else PleyColors.Muted,
        )
        PleyIconButton(PleyIcons.SkipBack, stringResource(R.string.music_previous), onClick = model::previous)
        val playing = now.playing || (now.loading && now.error == null)
        val label = stringResource(if (playing) R.string.music_pause else R.string.music_play)
        Box(
            Modifier
                .size(72.dp)
                .clip(PleyShape.Circle)
                .background(PleyColors.Accent)
                .clickable(role = Role.Button, onClick = model::toggle)
                .semantics { contentDescription = label },
            contentAlignment = Alignment.Center,
        ) {
            Icon(if (playing) PleyIcons.Pause else PleyIcons.Play, contentDescription = null, size = 32.dp, tint = PleyColors.OnAccent)
        }
        PleyIconButton(PleyIcons.SkipForward, stringResource(R.string.music_next), onClick = model::next, enabled = now.hasNext)
        PleyIconButton(PleyIcons.ListMusic, stringResource(R.string.music_queue), onClick = onQueue, tint = PleyColors.Muted)
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
            Artwork(art, row.title, Modifier.matchParentSize(), mark = MarkGlyph.Music)
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

/** The art's share of the width. */
private const val ART_SHARE = 0.8f

/** The faint copy of the art behind the page. */
private const val BACKDROP_ALPHA = 0.18f

internal const val NOW_PLAYING_PAGE = "pley.now_playing.page"
