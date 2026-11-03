package app.pley.android.ui.phone.player

import android.graphics.Bitmap
import androidx.compose.animation.core.animateDpAsState
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.layout.Layout
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.ProgressBarRangeInfo
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.progressBarRangeInfo
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.setProgress
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyShape
import app.pley.android.model.format.spokenDuration
import app.pley.android.ui.phone.R
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.map

/** The elapsed time, the seek bar, and the time left (or the length: tap it). */
@Composable
internal fun TimesAndSeekBar(state: PlayerScreenState) {
    // The times change once a second, not with every playhead update.
    val shownSecond by remember(state) { derivedStateOf { (state.scrubMs ?: state.playhead.longValue) / 1000 } }
    val position = shownSecond * 1000
    val duration = state.durationMs
    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
        Text(
            PlayerLook.clock(position),
            style = MaterialTheme.typography.bodySmall,
            color = PleyColors.Fg,
            modifier = Modifier.widthIn(min = 48.dp),
        )
        SeekBar(state, Modifier.weight(1f).padding(horizontal = 12.dp))
        val label = stringResource(if (state.showRemaining) R.string.player_show_total else R.string.player_show_remaining)
        Box(
            Modifier
                .heightIn(min = 48.dp)
                .widthIn(min = 56.dp)
                .clip(PleyShape.Badge)
                .clickable(onClickLabel = label) { state.showRemaining = !state.showRemaining; state.touched() },
            contentAlignment = Alignment.CenterEnd,
        ) {
            Text(
                if (state.showRemaining) PlayerLook.remaining(position, duration) else PlayerLook.clock(duration),
                style = MaterialTheme.typography.bodySmall,
                color = PleyColors.Fg,
                textAlign = TextAlign.End,
            )
        }
    }
}

/**
 * The seek bar (design spec §3.5): a 4 dp track (accent played, fg 45 %
 * buffered, fg 25 % the rest) with 2 dp gaps between chapters, and a 16 dp
 * accent knob that grows to 24 while dragged. Dragging shows the preview
 * at the knob and seeks on release. Everything moving is drawn, not composed.
 */
@Composable
internal fun SeekBar(state: PlayerScreenState, modifier: Modifier = Modifier) {
    val knob by animateDpAsState(if (state.scrubMs != null) 24.dp else 16.dp, label = "knob")
    val name = stringResource(R.string.player_seek_bar)
    val second by remember(state) { derivedStateOf { state.playhead.longValue / 1000 } }
    val duration = state.durationMs
    val spoken = stringResource(R.string.player_position_of, spokenDuration(second * 1000), spokenDuration(duration))
    Layout(
        content = {
            Box(
                Modifier
                    .fillMaxWidth()
                    .height(40.dp)
                    .semantics {
                        contentDescription = name
                        stateDescription = spoken
                        progressBarRangeInfo = ProgressBarRangeInfo(second * 1000f, 0f..duration.coerceAtLeast(1).toFloat())
                        setProgress { target ->
                            state.touched()
                            state.controller.seekTo(target.toLong())
                            true
                        }
                    }
                    .pointerInput(state) {
                        awaitEachGesture {
                            val down = awaitFirstDown()
                            fun at(x: Float): Long {
                                val d = state.durationMs
                                return if (d <= 0 || size.width == 0) 0 else (x / size.width).coerceIn(0f, 1f).times(d).toLong()
                            }
                            state.touched()
                            state.scrubMs = at(down.position.x)
                            down.consume()
                            while (true) {
                                val event = awaitPointerEvent()
                                val change = event.changes.firstOrNull { it.id == down.id } ?: break
                                if (!change.pressed) break
                                state.scrubMs = at(change.position.x)
                                change.consume()
                            }
                            val target = state.scrubMs
                            if (target != null) {
                                state.controller.seekTo(target)
                                state.playhead.longValue = target
                            }
                            state.scrubMs = null
                            state.touched()
                        }
                    }
                    .drawBehind {
                        val d = state.durationMs.toFloat()
                        val y = size.height / 2f
                        val track = 4.dp.toPx()
                        val gap = 2.dp.toPx()
                        val w = size.width
                        val pos = (state.scrubMs ?: state.playhead.longValue).toFloat()
                        val played = if (d > 0) (pos / d).coerceIn(0f, 1f) * w else 0f
                        val buffered = if (d > 0) (state.bufferedMs / d).coerceIn(0f, 1f) * w else 0f
                        val cuts = if (d > 0) state.chapters.drop(1).map { it.startMs / d * w } else emptyList()
                        val radius = CornerRadius(track / 2f)

                        fun segment(from: Float, to: Float, color: Color) {
                            if (to <= from) return
                            var start = from
                            for (cut in cuts) {
                                if (cut <= start || cut >= to) continue
                                drawRoundRect(color, Offset(start, y - track / 2f), Size(cut - gap / 2f - start, track), radius)
                                start = cut + gap / 2f
                            }
                            drawRoundRect(color, Offset(start, y - track / 2f), Size(to - start, track), radius)
                        }
                        segment(0f, w, PleyColors.Fg.copy(alpha = 0.25f))
                        segment(0f, buffered, PleyColors.Fg.copy(alpha = 0.45f))
                        segment(0f, played, PleyColors.Accent)
                        drawCircle(PleyColors.Accent, knob.toPx() / 2f, Offset(played, y))
                    },
            )
            ScrubPreview(state)
        },
        modifier = modifier,
    ) { measurables, constraints ->
        val bar = measurables[0].measure(constraints)
        val preview = measurables.getOrNull(1)?.measure(Constraints())
        layout(bar.width, bar.height) {
            bar.place(0, 0)
            val scrub = state.scrubMs
            val d = state.durationMs
            if (scrub != null && preview != null && preview.width > 0) {
                val x = if (d > 0) (scrub.toFloat() / d * bar.width).toInt() else 0
                val gutter = 16.dp.roundToPx()
                val left = (x - preview.width / 2).coerceIn(-gutter, (bar.width - preview.width + gutter).coerceAtLeast(-gutter))
                preview.place(left, -preview.height - 4.dp.roundToPx())
            }
        }
    }
}

/**
 * Above the knob while it is dragged: the seek preview (160 by 90, 224 by
 * 126 on tablets) when the file has them, and "Part 1 · 1:02:03".
 */
@Composable
private fun ScrubPreview(state: PlayerScreenState) {
    if (state.scrubMs == null) return
    var picture by remember { mutableStateOf<Bitmap?>(null) }
    LaunchedEffect(state) {
        snapshotFlow { state.scrubMs }
            .filterNotNull()
            .map { it / PREVIEW_STEP_MS * PREVIEW_STEP_MS }
            .distinctUntilChanged()
            .collect { at -> if (state.hasThumbnails) picture = state.controller.thumbnail(at) ?: picture }
    }
    val wide = LocalConfiguration.current.smallestScreenWidthDp >= 600
    val tile = if (wide) Size(224f, 126f) else Size(160f, 90f)
    val at by remember(state) { derivedStateOf { (state.scrubMs ?: 0) / 1000 * 1000 } }
    val chapters = state.chapters
    val index = PlayerLook.chapterAt(chapters, at)
    val chapterName = if (index >= 0) {
        val fallback = stringResource(R.string.player_chapter_number, index + 1)
        PlayerLook.chapterTitle(chapters[index], index) { fallback }
    } else {
        null
    }
    Column(horizontalAlignment = Alignment.CenterHorizontally) {
        if (state.hasThumbnails) PreviewTile(picture, tile.width.dp, tile.height.dp)
        val time = PlayerLook.clock(at)
        Text(
            if (chapterName != null) stringResource(R.string.player_chapter_at, chapterName, time) else time,
            style = MaterialTheme.typography.labelLarge,
            color = PleyColors.Fg,
            modifier = Modifier
                .padding(top = 6.dp)
                .background(Color.Black.copy(alpha = 0.6f), PleyShape.Badge)
                .padding(horizontal = 8.dp, vertical = 2.dp),
        )
    }
}

/**
 * A seek preview in its [width] × [height] tile, letterboxed on black as
 * the video is, so a 2.39:1 film's keep their shape; the well until one
 * comes.
 */
@Composable
internal fun PreviewTile(image: Bitmap?, width: Dp, height: Dp, modifier: Modifier = Modifier) {
    val picture = remember(image) { image?.asImageBitmap() }
    Box(
        modifier
            .size(width, height)
            .clip(PleyShape.Field)
            .background(if (picture != null) Color.Black else PleyColors.Surface2)
            .border(2.dp, PleyColors.Fg.copy(alpha = 0.2f), PleyShape.Field),
    ) {
        if (picture != null) {
            Image(picture, contentDescription = null, contentScale = ContentScale.Fit, modifier = Modifier.size(width, height))
        }
    }
}

/** Previews are asked for at most once per this much of the video. */
private const val PREVIEW_STEP_MS = 1_000L
