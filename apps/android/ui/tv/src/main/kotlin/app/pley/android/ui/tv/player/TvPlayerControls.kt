package app.pley.android.ui.tv.player

import android.text.format.DateFormat
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.focusable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsFocusedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.State
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusProperties
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.layout.layout
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.ProgressBarRangeInfo
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.progressBarRangeInfo
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.setProgress
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.constrainWidth
import androidx.compose.ui.unit.dp
import androidx.compose.foundation.Image
import androidx.tv.material3.Border
import androidx.tv.material3.Glow
import androidx.tv.material3.IconButton
import androidx.tv.material3.IconButtonDefaults
import app.pley.android.designsystem.image.GradientBitmaps
import app.pley.android.designsystem.image.drawStretched
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyFocus
import app.pley.android.designsystem.PleyMotion
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.PleyTv
import app.pley.android.designsystem.PleyType
import app.pley.android.designsystem.TvSafeArea
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.image.Artwork
import app.pley.android.designsystem.image.ArtworkImage
import app.pley.android.designsystem.image.ArtworkLoad
import app.pley.android.model.MarkerKind
import app.pley.android.player.PlaybackController
import app.pley.android.player.PlayerPhase
import app.pley.android.player.PlayerUiState
import app.pley.android.player.markers.Markers
import app.pley.android.ui.tv.R
import app.pley.android.player.R as PlayerR

/*
 * The controls of design spec §2.7, on the 960 × 540 canvas: the title and
 * clock at the top, the scrubber with its times and preview, and the
 * control row. Each part fades on its own small layer ([alpha]); the
 * playhead is read where it is drawn, so the rest doesn't recompose with it.
 */

/** The title, the episode's line, how it plays, the clock and, while paused, when it ends. */
@Composable
internal fun BoxScope.TvPlayerTopBar(state: State<PlayerUiState>, alpha: State<Float>, now: () -> Long) {
    val title by remember(state) { derivedStateOf { state.value.title } }
    val line by remember(state) { derivedStateOf { TvPlayerText.episodeLine(state.value.subtitle) } }
    val paused by remember(state) { derivedStateOf { state.value.phase == PlayerPhase.Paused } }
    val method by remember(state) { derivedStateOf { state.value.method } }
    Column(
        Modifier
            .align(Alignment.TopStart)
            .padding(start = TvSafeArea.Horizontal, top = TvSafeArea.Vertical, end = 300.dp)
            .graphicsLayer { this.alpha = alpha.value },
    ) {
        BasicText(title, style = PleyType.Tv.Title.copy(color = PleyColors.Fg), maxLines = 1, overflow = TextOverflow.Ellipsis)
        if (line != null) {
            Spacer(Modifier.height(4.dp))
            BasicText(line!!, style = PleyType.Tv.Label.copy(color = PleyColors.Muted), maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
    }
    Column(
        Modifier
            .align(Alignment.TopEnd)
            .padding(end = TvSafeArea.Horizontal, top = TvSafeArea.Vertical)
            .graphicsLayer { this.alpha = alpha.value },
        horizontalAlignment = Alignment.End,
    ) {
        val clock = rememberClock(now)
        Row(verticalAlignment = Alignment.CenterVertically) {
            method?.let {
                MethodBadge(it)
                Spacer(Modifier.width(12.dp))
            }
            BasicText(clock, style = PleyType.Tv.Label.copy(color = PleyColors.Muted))
        }
        if (paused) {
            val context = LocalContext.current
            val ends by remember(state) {
                derivedStateOf {
                    val s = state.value
                    val left = (s.durationMs - s.positionMs).coerceAtLeast(0)
                    DateFormat.getTimeFormat(context).format(now() + left)
                }
            }
            BasicText(stringResource(R.string.tv_player_ends_at, ends), style = PleyType.Tv.Label.copy(color = PleyColors.Muted))
        }
    }
}

/**
 * How it plays, badged as the phone's and the web's players do: a file
 * played as it is (direct play, or downloaded) in success, a direct stream
 * in info, a transcode in the accent, on bg 80 % so a bright picture
 * doesn't wash it out. Info, in the control row, says why.
 */
@Composable
private fun MethodBadge(method: String) {
    val direct = setOf(stringResource(PlayerR.string.session_method_direct), stringResource(PlayerR.string.session_method_downloaded))
    val stream = setOf(stringResource(PlayerR.string.session_method_remux), stringResource(PlayerR.string.session_method_remux_audio))
    val colour = when (method) {
        in direct -> PleyColors.Success
        in stream -> PleyColors.Info
        else -> PleyColors.Accent
    }
    BasicText(
        method,
        style = PleyType.Tv.Caption.copy(color = colour, fontWeight = FontWeight.SemiBold),
        maxLines = 1,
        modifier = Modifier
            .background(PleyColors.OnArt, PleyShape.Badge)
            .background(colour.copy(alpha = 0.2f), PleyShape.Badge)
            .padding(horizontal = 8.dp, vertical = 3.dp),
    )
}

/** The time of day, as the TV writes it (12 or 24 hours), changing on the minute. */
@Composable
private fun rememberClock(now: () -> Long): String {
    val context = LocalContext.current
    val format = remember(context) { DateFormat.getTimeFormat(context) }
    val text by produceState(format.format(now())) {
        while (true) {
            val t = now()
            value = format.format(t)
            kotlinx.coroutines.delay(MINUTE_MS - t % MINUTE_MS + 50)
        }
    }
    return text
}

/**
 * The scrubber (design spec §2.7) from x = 48 to 912, its track's centre
 * at y = 425: the played part in accent, buffered in fg 45 %, gaps at
 * chapter starts; the thumb, 10 dp, grows to 12 with a slim ring while
 * focused (18 dp across, where the tiles' ring made it 32); the
 * times above, a running marker's name below. Left and Right on it seek
 * ([TvSeeker], keys handled by the screen): while a seek is coming, the
 * thumb, the played part and both times stand where it goes, and while the
 * chapters' cards show, at the start of the one focused; at the playhead
 * otherwise ([scrubberAt]). It says its place to screen readers as a slider.
 */
@Composable
internal fun BoxScope.TvScrubber(
    state: State<PlayerUiState>,
    seeker: TvSeeker,
    ui: TvPlayerUi,
    controller: PlaybackController,
    alpha: State<Float>,
    skipShown: Boolean,
) {
    val interactions = remember { MutableInteractionSource() }
    val focused by interactions.collectIsFocusedAsState()
    val thumbScale by animateFloatAsState(if (focused) 1f else 0f, PleyMotion.focus(), label = "thumb")
    val elapsed by remember(state, seeker, ui) { derivedStateOf { scrubberAt(state.value, seeker, ui) / 1000 } }
    val duration by remember(state) { derivedStateOf { state.value.durationMs } }
    val remaining by remember(state, seeker, ui) { derivedStateOf { (state.value.durationMs - scrubberAt(state.value, seeker, ui)).coerceAtLeast(0) / 1000 } }
    val skip = ui.requester(PlayerFocus.Skip)
    val down = ui.requester(PlayerFocus.PlayPause)
    Box(
        Modifier
            .align(Alignment.TopStart)
            .padding(top = TIMES_TOP)
            .fillMaxWidth()
            .padding(horizontal = TvSafeArea.Horizontal)
            .graphicsLayer { this.alpha = alpha.value },
    ) {
        BasicText(TvPlayerText.time(elapsed * 1000), style = PleyType.Tv.Caption.copy(color = PleyColors.Fg), modifier = Modifier.align(Alignment.TopStart))
        BasicText(TvPlayerText.remaining(0, remaining * 1000), style = PleyType.Tv.Caption.copy(color = PleyColors.Fg), modifier = Modifier.align(Alignment.TopEnd))
        val resources = LocalResources.current
        val spoken = stringResource(R.string.tv_player_position, TvPlayerText.spoken(resources, elapsed * 1000), TvPlayerText.spoken(resources, duration))
        Box(
            Modifier
                .padding(top = TRACK_CENTRE - TIMES_TOP - SCRUB_HEIGHT / 2)
                .fillMaxWidth()
                .height(SCRUB_HEIGHT)
                .testTag(TAG_SCRUBBER)
                .focusRequester(ui.requester(PlayerFocus.Scrubber))
                .onFocusChanged { if (it.isFocused) ui.focused = PlayerFocus.Scrubber else if (ui.focused == PlayerFocus.Scrubber) ui.focused = null }
                .focusProperties {
                    up = if (skipShown) skip else FocusRequester.Cancel
                    this.down = down
                    left = FocusRequester.Cancel
                    right = FocusRequester.Cancel
                }
                .semantics {
                    contentDescription = spoken
                    stateDescription = spoken
                    val d = state.value.durationMs.coerceAtLeast(1)
                    progressBarRangeInfo = ProgressBarRangeInfo((elapsed * 1000f / d).coerceIn(0f, 1f), 0f..1f)
                    setProgress { fraction ->
                        // A seek of its own, as Previous and Next are: one waiting for OK gives way to it.
                        seeker.cancel()
                        controller.seekTo((fraction * d).toLong())
                        true
                    }
                }
                .focusable(interactionSource = interactions)
                .drawBehind {
                    val s = state.value
                    drawTrack(s, scrubberAt(s, seeker, ui), thumbScale)
                },
        )
        MarkerLabel(state)
    }
}

/**
 * Where the scrubber stands: the start of the chapter focused in the cards,
 * where a seek coming goes, or the playhead. Read where it is shown.
 */
private fun scrubberAt(s: PlayerUiState, seeker: TvSeeker, ui: TvPlayerUi): Long =
    ui.chapter?.let { s.chapters.getOrNull(it)?.startMs } ?: seeker.target ?: s.positionMs

/** The track, the played part and the thumb at [at], the buffered part and chapter gaps. */
private fun androidx.compose.ui.graphics.drawscope.DrawScope.drawTrack(s: PlayerUiState, at: Long, focus: Float) {
    val h = TRACK_HEIGHT.toPx()
    val y = size.height / 2f - h / 2f
    val w = size.width
    val d = s.durationMs.toFloat().coerceAtLeast(1f)
    val gap = CHAPTER_GAP.toPx()
    val r = CornerRadius(h / 2f, h / 2f)
    fun x(ms: Long) = (ms / d).coerceIn(0f, 1f) * w
    val starts = s.chapters.map { x(it.startMs) }.filter { it > gap && it < w - gap }
    // Segments between chapter gaps, each drawn in three layers.
    val edges = buildList {
        add(0f)
        starts.forEach { add(it) }
        add(w)
    }
    // Accent up to the thumb: where it plays, or where a seek or a chapter focused would take it.
    val played = x(at)
    val buffered = x(s.bufferedMs.coerceAtLeast(s.positionMs))
    for (i in 0 until edges.size - 1) {
        val a = edges[i] + if (i == 0) 0f else gap / 2f
        val b = edges[i + 1] - if (i == edges.size - 2) 0f else gap / 2f
        if (b <= a) continue
        drawRoundRect(TRACK, Offset(a, y), Size(b - a, h), r)
        if (buffered > a) drawRoundRect(BUFFERED, Offset(a, y), Size(minOf(buffered, b) - a, h), r)
        if (played > a) drawRoundRect(PleyColors.Accent, Offset(a, y), Size(minOf(played, b) - a, h), r)
    }
    val thumbX = x(at)
    val thumb = PleyTv.ScrubThumb.toPx()
    val radius = (thumb + (PleyTv.ScrubThumbFocused.toPx() - thumb) * focus) / 2f
    val centre = Offset(thumbX.coerceIn(radius, w - radius), size.height / 2f)
    drawCircle(PleyColors.Fg, radius, centre)
    if (focus > 0f) {
        drawCircle(
            PleyColors.AccentBright.copy(alpha = focus),
            radius + PleyFocus.ThumbRingOffset.toPx(),
            centre,
            style = Stroke(PleyFocus.ThumbRingStroke.toPx()),
        )
    }
}

/** "Intro" or "Credits" under the span of the marker playing, in caption muted. */
@Composable
private fun MarkerLabel(state: State<PlayerUiState>) {
    val marker by remember(state) { derivedStateOf { Markers.active(state.value.markers, state.value.positionMs) } }
    val m = marker ?: return
    val label = stringResource(if (m.kind == MarkerKind.Intro) R.string.tv_player_intro else R.string.tv_player_credits)
    BasicText(
        label,
        style = PleyType.Tv.Caption.copy(color = PleyColors.Muted, textAlign = TextAlign.Center),
        maxLines = 1,
        modifier = Modifier
            .padding(top = TRACK_CENTRE - TIMES_TOP + 6.dp)
            .fillMaxWidth()
            .layout { measurable, constraints ->
                val placeable = measurable.measure(constraints.copy(minWidth = 0))
                val d = state.value.durationMs.toFloat().coerceAtLeast(1f)
                val mid = ((m.startMs + m.endMs) / 2f / d).coerceIn(0f, 1f) * constraints.maxWidth
                val x = (mid - placeable.width / 2f).toInt().coerceIn(0, (constraints.maxWidth - placeable.width).coerceAtLeast(0))
                layout(constraints.maxWidth, placeable.height) { placeable.place(x, 0) }
            },
    )
}

/**
 * The seek preview while scrubbing (design spec §2.7): 240 × 135 with a
 * 2 dp fg 20 % edge, its bottom at y = 347, centred on the thumb but kept
 * in the safe area; the target time under it and the chapter's name. The
 * server's seek previews when the file has them, else the chapter's
 * picture, letterboxed on black as the video is: a 2.39:1 film's previews
 * keep their shape. Without either, only the time and chapter show.
 */
@Composable
internal fun BoxScope.TvSeekPreview(state: State<PlayerUiState>, seeker: TvSeeker, controller: PlaybackController) {
    val target = seeker.target ?: return
    val hasThumbs by remember(state) { derivedStateOf { state.value.hasThumbnails } }
    val chapters by remember(state) { derivedStateOf { state.value.chapters } }
    // Previews are asked for every 2 s of the target at most.
    val bucket = target / PREVIEW_STEP_MS * PREVIEW_STEP_MS
    val chapterIndex = TvPlayerText.chapterAt(chapters, target)
    val chapter = chapterIndex?.let { chapters[it] }
    val picture by produceState<ImageBitmap?>(null, bucket, hasThumbs) {
        if (hasThumbs) controller.thumbnail(bucket)?.let { value = it.asImageBitmap() }
    }
    // Kept across the key repeats that move the target within the chapter.
    val chapterUrl = chapter?.imageUrl?.takeIf { !hasThumbs }
    val chapterImage = remember(chapterUrl) { chapterUrl?.let { ArtworkImage(listOf(it), maxWidth = PREVIEW_IMAGE_WIDTH) } }
    val chapterLoad = remember(chapterImage) { ArtworkLoad() }
    val showsPicture = hasThumbs || chapterImage != null
    Column(
        Modifier
            .align(Alignment.TopStart)
            .padding(top = if (showsPicture) PREVIEW_TOP else PREVIEW_TOP + PREVIEW_HEIGHT + 6.dp)
            .fillMaxWidth()
            .padding(horizontal = TvSafeArea.Horizontal)
            .layout { measurable, constraints ->
                val placeable = measurable.measure(constraints.copy(minWidth = 0))
                val d = state.value.durationMs.toFloat().coerceAtLeast(1f)
                val t = (seeker.target ?: target).toFloat()
                val centre = (t / d).coerceIn(0f, 1f) * constraints.maxWidth
                val x = (centre - placeable.width / 2f).toInt().coerceIn(0, (constraints.maxWidth - placeable.width).coerceAtLeast(0))
                layout(constraints.maxWidth, placeable.height) { placeable.place(x, 0) }
            }
            .testTag(TAG_PREVIEW),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        if (showsPicture) {
            Box(
                Modifier
                    .size(PREVIEW_WIDTH, PREVIEW_HEIGHT)
                    .clip(PleyShape.TileTv)
                    // The well until a picture shows, then black where the picture doesn't reach.
                    .drawBehind { drawRect(if (picture != null || chapterLoad.loaded) Color.Black else PleyColors.Surface2) }
                    .border(2.dp, PleyColors.Fg.copy(alpha = 0.2f), PleyShape.TileTv)
                    .testTag(TAG_PREVIEW_PICTURE),
            ) {
                val bitmap = picture
                when {
                    bitmap != null -> Image(bitmap, contentDescription = null, modifier = Modifier.fillMaxSize(), contentScale = ContentScale.Fit)
                    chapterImage != null -> Artwork(chapterImage, title = "", modifier = Modifier.fillMaxSize(), contentScale = ContentScale.Fit, load = chapterLoad)
                }
            }
        }
        Spacer(Modifier.height(6.dp))
        BasicText(TvPlayerText.time(target), style = PleyType.Tv.Label.copy(color = PleyColors.Fg))
        if (chapterIndex != null) {
            val name = chapter?.name
            val text = when {
                name == null -> stringResource(R.string.tv_player_chapter, chapterIndex + 1)
                name.startsWith("Chapter", ignoreCase = true) -> name
                else -> stringResource(R.string.tv_player_chapter_named, chapterIndex + 1, name)
            }
            BasicText(text, style = PleyType.Tv.Caption.copy(color = PleyColors.Muted), maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
    }
}

/** What the control row offers beyond playing and seeking. */
internal data class RowOptions(
    val previousNext: Boolean,
    val subtitles: Boolean,
    val audio: Boolean,
    val quality: Boolean,
    val chapters: Boolean,
)

/**
 * The control row, top y = 441 (design spec §2.7): Previous · Play/Pause ·
 * Next centred; Subtitles · Audio · Quality · Info at the right safe edge;
 * Chapters (their cards over the scrubber, [onChapters]), when the file has
 * them, the sleep timer and Speed at the left one. Up goes back to the
 * scrubber. As in streaming apps' players, the icons stand on the scrim
 * alone until focused.
 */
@Composable
internal fun BoxScope.TvControlRow(
    playing: Boolean,
    options: RowOptions,
    ui: TvPlayerUi,
    alpha: State<Float>,
    onPlayPause: () -> Unit,
    onPrevious: () -> Unit,
    onNext: () -> Unit,
    onChapters: () -> Unit,
) {
    val up = Modifier.focusProperties {
        this.up = ui.requester(PlayerFocus.Scrubber)
        down = FocusRequester.Cancel
    }
    // The 36 dp buttons sit this far down, so their centres line up with Play/Pause's.
    val beside = Modifier.padding(top = (PLAY_BUTTON - BUTTON) / 2).then(up)
    Box(
        Modifier
            .align(Alignment.TopStart)
            .padding(top = ROW_TOP)
            .fillMaxWidth()
            .padding(horizontal = TvSafeArea.Horizontal)
            .graphicsLayer { this.alpha = alpha.value },
    ) {
        Row(
            Modifier.align(Alignment.TopStart),
            horizontalArrangement = Arrangement.spacedBy(PleyTv.ButtonGap),
        ) {
            if (options.chapters) {
                PlayerRoundButton(stringResource(R.string.tv_player_chapters), PleyIcons.ListVideo, onChapters, beside, ui, PlayerFocus.Chapters)
            }
            PlayerRoundButton(stringResource(R.string.tv_player_sleep), MoonIcon, { ui.openPanel(PlayerPanel.Sleep) }, beside, ui, PlayerFocus.Sleep)
            PlayerRoundButton(stringResource(R.string.tv_player_speed), PleyIcons.FastForward, { ui.openPanel(PlayerPanel.Speed) }, beside, ui, PlayerFocus.Speed)
        }
        Row(
            Modifier.align(Alignment.TopCenter),
            horizontalArrangement = Arrangement.spacedBy(TRANSPORT_GAP),
        ) {
            if (options.previousNext) {
                PlayerRoundButton(stringResource(R.string.tv_player_previous), PleyIcons.SkipBack, onPrevious, beside, ui, null, iconSize = TRANSPORT_ICON)
            }
            PlayerRoundButton(
                stringResource(if (playing) R.string.tv_player_pause else R.string.tv_player_play),
                if (playing) PleyIcons.Pause else PleyIcons.Play,
                onPlayPause,
                up,
                ui,
                PlayerFocus.PlayPause,
                size = PLAY_BUTTON,
                iconSize = PLAY_ICON,
            )
            if (options.previousNext) {
                PlayerRoundButton(stringResource(R.string.tv_player_next), PleyIcons.SkipForward, onNext, beside, ui, null, iconSize = TRANSPORT_ICON)
            }
        }
        Row(
            Modifier.align(Alignment.TopEnd),
            horizontalArrangement = Arrangement.spacedBy(PleyTv.ButtonGap),
        ) {
            if (options.subtitles) {
                PlayerRoundButton(stringResource(R.string.tv_player_subtitles), PleyIcons.Captions, { ui.openPanel(PlayerPanel.Subtitles) }, beside, ui, PlayerFocus.Subtitles)
            }
            if (options.audio) {
                PlayerRoundButton(stringResource(R.string.tv_player_audio), PleyIcons.AudioLines, { ui.openPanel(PlayerPanel.Audio) }, beside, ui, PlayerFocus.Audio)
            }
            if (options.quality) {
                PlayerRoundButton(stringResource(R.string.tv_player_quality), PleyIcons.Gauge, { ui.openPanel(PlayerPanel.Quality) }, beside, ui, PlayerFocus.Quality)
            }
            PlayerRoundButton(stringResource(R.string.tv_player_info), PleyIcons.Info, { ui.openPanel(PlayerPanel.Info) }, beside, ui, PlayerFocus.Info)
        }
    }
}

/**
 * A button of the player (design spec §2.5): its [icon], [iconSize] square,
 * alone on the scrim, in fg; focused, the accent fills a
 * [control][PleyShape.Control] square behind it, 1.08 larger, and its label
 * fades in 8 dp below (always there for screen readers). [target] names it
 * for focus requests.
 */
@Composable
private fun PlayerRoundButton(
    label: String,
    icon: PleyIcon,
    onClick: () -> Unit,
    modifier: Modifier,
    ui: TvPlayerUi,
    target: PlayerFocus?,
    size: Dp = BUTTON,
    iconSize: Dp = ICON,
) {
    val interactions = remember { MutableInteractionSource() }
    val focused by interactions.collectIsFocusedAsState()
    val labelAlpha by animateFloatAsState(if (focused) 1f else 0f, PleyMotion.reveal(), label = "label")
    Box(modifier.width(size), contentAlignment = Alignment.TopCenter) {
        IconButton(
            onClick = onClick,
            interactionSource = interactions,
            modifier = Modifier
                .size(size)
                .then(if (target != null) Modifier.focusRequester(ui.requester(target)).testTag(tagOf(target)) else Modifier)
                .onFocusChanged { state ->
                    if (target != null) {
                        if (state.isFocused) ui.focused = target else if (ui.focused == target) ui.focused = null
                    } else if (state.isFocused) {
                        ui.focused = null
                    }
                }
                .semantics { contentDescription = label },
            shape = IconButtonDefaults.shape(PleyShape.Control),
            colors = IconButtonDefaults.colors(
                containerColor = Color.Transparent,
                contentColor = PleyColors.Fg,
                focusedContainerColor = PleyColors.Accent,
                focusedContentColor = PleyColors.OnAccent,
                pressedContainerColor = PleyColors.AccentBright,
                pressedContentColor = PleyColors.OnAccent,
            ),
            scale = IconButtonDefaults.scale(focusedScale = PleyFocus.ROUND_SCALE, pressedScale = 1f),
            glow = IconButtonDefaults.glow(Glow.None, Glow.None, Glow.None),
            border = IconButtonDefaults.border(Border.None, Border.None, Border.None, Border.None, Border.None),
        ) {
            Icon(icon, contentDescription = null, size = iconSize, tint = androidx.tv.material3.LocalContentColor.current)
        }
        BasicText(
            label,
            style = PleyType.Tv.Caption.copy(color = PleyColors.Fg, textAlign = TextAlign.Center),
            maxLines = 1,
            softWrap = false,
            modifier = Modifier
                .padding(top = size + 8.dp)
                .graphicsLayer { alpha = labelAlpha }
                .layout { measurable, constraints ->
                    val placeable = measurable.measure(constraints.copy(minWidth = 0, maxWidth = Constraints.Infinity))
                    val width = constraints.constrainWidth(placeable.width)
                    layout(width, placeable.height) { placeable.place((width - placeable.width) / 2, 0) }
                },
        )
    }
}

/**
 * The scrim behind the controls (design spec §2.7, black as the player's
 * exception), over the whole picture: 65 % at the top behind the title,
 * 25 % through the middle, so the picture steps back while the controls
 * are up, and 90 % at the bottom, where the row's icons stand on it alone.
 * It fades by its alpha, read while drawing: no layer over the video. It
 * is one stretched picture, not a gradient shader, which costs far more on
 * a TV's GPU (GradientBitmaps).
 */
internal fun Modifier.controlScrims(alpha: State<Float>): Modifier = drawBehind {
    val a = alpha.value
    if (a <= 0f) return@drawBehind
    drawStretched(ControlScrim, Offset.Zero, size, alpha = a)
}

private val ControlScrim by lazy {
    GradientBitmaps.vertical(
        0f to Color.Black.copy(alpha = 0.65f),
        0.22f to Color.Black.copy(alpha = 0.25f),
        0.55f to Color.Black.copy(alpha = 0.25f),
        0.78f to Color.Black.copy(alpha = 0.7f),
        1f to Color.Black.copy(alpha = 0.9f),
    )
}

/** Whether a phase counts as playing for Play/Pause: it will play once ready. */
internal fun PlayerPhase.wantsToPlay(): Boolean = this == PlayerPhase.Playing || this == PlayerPhase.Buffering || this == PlayerPhase.Preparing || this == PlayerPhase.Switching

private const val MINUTE_MS = 60_000L

/** Seek previews are asked for every 2 s of the target at most. */
internal const val PREVIEW_STEP_MS = 2_000L

/** A chapter's picture is asked for this wide, in pixels: the preview's 240 dp on a 1080p TV. */
internal const val PREVIEW_IMAGE_WIDTH = 480

private val TRACK = PleyColors.Fg.copy(alpha = 0.25f)
private val BUFFERED = PleyColors.Fg.copy(alpha = 0.45f)

private val TRACK_HEIGHT = 4.dp
private val CHAPTER_GAP = 2.dp
private val SCRUB_HEIGHT = 40.dp

/**
 * The control row's buttons: 36 dp with 20 dp icons, the transport's icons
 * 24 (the skips' seconds stay legible), Play/Pause 44 with 26. Near the
 * TV's 40 dp buttons elsewhere, and a glyph alone until focused, as in
 * streaming apps' players; at 48 and 64 dp the focused square took a
 * tenth of the screen's height.
 */
private val BUTTON = 36.dp
private val ICON = PleyTv.IconSize
private val TRANSPORT_ICON = 24.dp
private val PLAY_BUTTON = 44.dp
private val PLAY_ICON = 26.dp
private val TRANSPORT_GAP = 16.dp

/** Where things sit on the 540 dp canvas (design spec §2.7): the row's focused labels end above the safe area's 513. */
internal val TIMES_TOP = 399.dp
internal val TRACK_CENTRE = 425.dp
internal val ROW_TOP = 441.dp
internal val PREVIEW_TOP = 212.dp
internal val PREVIEW_WIDTH = 240.dp
internal val PREVIEW_HEIGHT = 135.dp

/** A control row button's test tag. */
internal fun tagOf(target: PlayerFocus): String = when (target) {
    PlayerFocus.PlayPause -> TAG_PLAY_PAUSE
    PlayerFocus.Subtitles -> TAG_SUBTITLES
    PlayerFocus.Info -> TAG_INFO
    else -> "pley.tv.player." + target.name.lowercase()
}

internal const val TAG_SCRUBBER = "pley.tv.player.scrubber"
internal const val TAG_PREVIEW = "pley.tv.player.preview"
internal const val TAG_PREVIEW_PICTURE = "pley.tv.player.previewPicture"
internal const val TAG_PLAY_PAUSE = "pley.tv.player.playPause"
internal const val TAG_SUBTITLES = "pley.tv.player.subtitles"
internal const val TAG_INFO = "pley.tv.player.info"
