package app.pley.android.ui.phone.player

import android.os.Build
import android.view.Display
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.navigationBars
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalWindowInfo
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyElevation
import app.pley.android.designsystem.PleyMotion
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.image.Artwork
import app.pley.android.designsystem.image.ArtworkImage
import app.pley.android.designsystem.reducedMotion
import app.pley.android.data.SubtitleFont
import app.pley.android.player.AudioDelays
import app.pley.android.player.PlaybackDetailsUi
import app.pley.android.player.PlaybackSpeeds
import app.pley.android.player.SubtitleSearchUi
import app.pley.android.player.engine.EngineState
import app.pley.android.player.engine.PlaybackStats
import app.pley.android.player.sleep.SleepChoice
import app.pley.android.player.sleep.SleepTexts
import app.pley.android.player.subtitles.CaptionLook
import app.pley.android.player.subtitles.SubtitleSync
import androidx.compose.ui.platform.LocalResources
import app.pley.android.ui.phone.R
import app.pley.android.ui.phone.kit.DelayedSpinner
import app.pley.android.ui.phone.kit.PleyIconButton
import app.pley.android.ui.phone.settings.fontLabel
import app.pley.android.ui.phone.settings.raiseLabel

/** The subtitle style chosen on this device: one of the four sizes, the dark box, the font and how far up. */
data class SubtitleChoice(
    val sizeIndex: Int = 1,
    val box: Boolean = false,
    val font: SubtitleFont = SubtitleFont.Sans,
    val raiseIndex: Int = 0,
)

/**
 * The player's sheet (design spec §3.4 in the player's window, so the
 * system bars stay hidden): Settings with Quality, Audio, Subtitles,
 * Subtitle style, Speed, Sleep timer and Playback details, each also opened
 * straight from the bottom row, and Chapters. Subtitles has Sync while the
 * subtitles chosen can move, and Find subtitles… while the file streams
 * from the server; Subtitle style shows while the subtitles chosen are
 * text; Audio has the audio delay. [surface2][PleyColors.Surface2], 20 dp top
 * corners, over a 60 % scrim that closes it.
 */
@Composable
internal fun BoxScope.PlayerSheet(state: PlayerScreenState, style: SubtitleChoice, onStyle: (SubtitleChoice) -> Unit) {
    val page = state.sheet
    // Subtitle style stops applying while open (the next episode's subtitles are pictures): back to Settings.
    val styleGone = page == SheetPage.Style && !state.subtitleStyled
    LaunchedEffect(styleGone) {
        if (styleGone) state.back()
    }
    val reduced = reducedMotion()
    val scrim by animateFloatAsState(if (page != null) 1f else 0f, PleyMotion.overlay(reduced), label = "sheetScrim")
    if (page != null || scrim > 0f) {
        Box(
            Modifier
                .fillMaxSize()
                .drawBehind { drawRect(PleyColors.Scrim.copy(alpha = PleyColors.Scrim.alpha * scrim)) }
                .then(
                    if (page != null) {
                        Modifier.clickable(interactionSource = remember { MutableInteractionSource() }, indication = null, onClick = state::closeSheet)
                    } else {
                        Modifier
                    },
                ),
        )
    }
    var shown by remember { mutableStateOf(page) }
    if (page != null) shown = page
    val maxHeight = with(LocalDensity.current) { (LocalWindowInfo.current.containerSize.height * 0.88f).toDp() }
    AnimatedVisibility(
        page != null,
        Modifier.align(Alignment.BottomCenter),
        enter = if (reduced) fadeIn(PleyMotion.reveal()) else slideInVertically(PleyMotion.overlay()) { it } + fadeIn(PleyMotion.overlay()),
        exit = if (reduced) fadeOut(PleyMotion.reveal()) else slideOutVertically(PleyMotion.overlay()) { it } + fadeOut(PleyMotion.overlay()),
    ) {
        Column(
            Modifier
                .widthIn(max = 560.dp)
                .fillMaxWidth()
                .heightIn(max = maxHeight)
                .clip(PleyShape.Sheet)
                .background(PleyColors.Surface2)
                .drawBehind {
                    val hair = PleyElevation.Level2Hairline.toPx()
                    drawLine(PleyColors.Surface4, Offset(PleyShape.SheetRadius.toPx(), hair / 2f), Offset(size.width - PleyShape.SheetRadius.toPx(), hair / 2f), hair)
                }
                // Touches inside don't reach the scrim.
                .clickable(interactionSource = remember { MutableInteractionSource() }, indication = null, onClick = {})
                .windowInsetsPadding(WindowInsets.navigationBars),
        ) {
            Box(Modifier.fillMaxWidth().height(20.dp), contentAlignment = Alignment.Center) {
                Box(Modifier.size(32.dp, 4.dp).background(PleyColors.Control, PleyShape.Pill))
            }
            shown?.let { SheetContent(it, state, style, onStyle) }
        }
    }
}

@Composable
private fun ColumnScope.SheetContent(page: SheetPage, state: PlayerScreenState, style: SubtitleChoice, onStyle: (SubtitleChoice) -> Unit) {
    PlayerProbe.composed?.invoke("sheet")
    val title = stringResource(
        when (page) {
            SheetPage.Settings -> R.string.player_settings
            SheetPage.Quality -> R.string.player_quality
            SheetPage.Audio -> R.string.player_audio
            SheetPage.Subtitles -> R.string.player_subtitles
            SheetPage.Style -> R.string.player_subtitle_style
            SheetPage.Details -> R.string.player_details
            SheetPage.Chapters -> R.string.player_chapters
            SheetPage.Sleep -> R.string.player_sleep_timer
            SheetPage.Speed -> R.string.player_speed
            SheetPage.FindSubtitles -> R.string.player_find_subtitles
        },
    )
    Row(Modifier.fillMaxWidth().padding(start = 8.dp, end = 8.dp), verticalAlignment = Alignment.CenterVertically) {
        if ((state.sheetRoot == SheetPage.Settings && page != SheetPage.Settings) || page == SheetPage.FindSubtitles) {
            PleyIconButton(PleyIcons.ChevronLeft, stringResource(R.string.player_sheet_back), onClick = state::back)
        } else {
            Spacer(Modifier.width(16.dp))
        }
        Text(
            title,
            style = MaterialTheme.typography.titleMedium,
            color = PleyColors.Fg,
            modifier = Modifier.weight(1f).semantics { heading() },
        )
        PleyIconButton(PleyIcons.X, stringResource(R.string.player_close), onClick = state::closeSheet)
    }
    if (page == SheetPage.Chapters) {
        ChaptersPage(state)
        return
    }
    Column(Modifier.fillMaxWidth().verticalScroll(rememberScrollState()).padding(bottom = 12.dp)) {
        when (page) {
            SheetPage.Settings -> SettingsPage(state, style)
            SheetPage.Quality -> QualityPage(state)
            SheetPage.Audio -> {
                AudioDelayControl(state.audioDelayMs, onChange = state.controller::setAudioDelay)
                state.audio.forEach { track ->
                    OptionRow(track.selected, track.label) {
                        track.index?.let(state.controller::selectAudio)
                        state.closeSheet()
                    }
                }
            }
            SheetPage.Subtitles -> {
                if (state.subtitleSync) SyncControl(state.subtitleOffsetMs, onChange = state.controller::setSubtitleOffset)
                state.subtitles.forEach { track ->
                    OptionRow(track.selected, track.label) {
                        state.controller.selectSubtitle(track.index)
                        state.closeSheet()
                    }
                }
                if (state.canFindSubtitles) {
                    ActionRow(PleyIcons.Search, stringResource(R.string.player_find_subtitles)) { state.open(SheetPage.FindSubtitles) }
                }
            }
            SheetPage.Speed -> SpeedPage(state)
            SheetPage.FindSubtitles -> FindSubtitlesPage(state)
            SheetPage.Style -> StylePage(style, onStyle)
            SheetPage.Details -> DetailsPage(state)
            SheetPage.Sleep -> SleepPage(state)
            SheetPage.Chapters -> Unit
        }
    }
}

@Composable
private fun SettingsPage(state: PlayerScreenState, style: SubtitleChoice) {
    if (state.canPlayOriginal) {
        ActionRow(PleyIcons.RotateCcw, stringResource(R.string.player_play_original)) {
            state.controller.playOriginal()
            state.closeSheet()
        }
    }
    if (state.qualities.isNotEmpty()) {
        NavRow(PleyIcons.Gauge, stringResource(R.string.player_quality), state.quality) { state.open(SheetPage.Quality) }
    }
    if (state.audio.isNotEmpty()) {
        // One track still has its delay to set.
        val track = state.audio.firstOrNull { it.selected }?.label ?: stringResource(R.string.player_default)
        val delay = state.audioDelayMs.takeIf { it != 0L }?.let { "$track · ${AudioDelays.label(it)}" }
        NavRow(PleyIcons.AudioLines, stringResource(R.string.player_audio), delay ?: track) {
            state.open(SheetPage.Audio)
        }
    }
    val chosen = state.subtitles.firstOrNull { it.selected && it.index != null }
    val offset = state.subtitleOffsetMs.takeIf { chosen != null && state.subtitleSync && it != 0L }
    val subtitle = chosen?.label?.let { label -> offset?.let { "$label · ${SubtitleSync.label(it)}" } ?: label }
        ?: state.subtitles.firstOrNull { it.selected }?.label ?: stringResource(R.string.player_none)
    NavRow(PleyIcons.Captions, stringResource(R.string.player_subtitles), subtitle, enabled = state.subtitles.isNotEmpty() || state.canFindSubtitles) {
        state.open(SheetPage.Subtitles)
    }
    // Only while the subtitles chosen are text: pictures keep their own look.
    if (state.subtitleStyled) {
        NavRow(PleyIcons.SlidersHorizontal, stringResource(R.string.player_subtitle_style), sizeLabel(style.sizeIndex)) { state.open(SheetPage.Style) }
    }
    NavRow(PleyIcons.FastForward, stringResource(R.string.player_speed), PlaybackSpeeds.label(state.speed)) { state.open(SheetPage.Speed) }
    NavRow(MoonIcon, stringResource(R.string.player_sleep_timer), SleepTexts.summary(LocalResources.current, state.sleep)) { state.open(SheetPage.Sleep) }
    NavRow(PleyIcons.Info, stringResource(R.string.player_details), state.method.orEmpty()) { state.open(SheetPage.Details) }
}

/** The speeds, 1× normal; said unavailable when a download's sound can't follow. */
@Composable
private fun SpeedPage(state: PlayerScreenState) {
    val note = state.speedNote
    if (note != null) Note(note)
    PlaybackSpeeds.ALL.forEach { speed ->
        OptionRow(state.speed == speed, PlaybackSpeeds.label(speed), enabled = note == null) {
            state.controller.setSpeed(speed)
            state.closeSheet()
        }
    }
}

/**
 * Find subtitles…: the server's results for the profile's language, the
 * best first; one tapped is added and shown, and the sheet closes.
 */
@Composable
private fun FindSubtitlesPage(state: PlayerScreenState) {
    val search = state.subtitleSearch
    when (search) {
        null, SubtitleSearchUi.Searching -> Box(Modifier.fillMaxWidth().padding(24.dp), contentAlignment = Alignment.Center) {
            DelayedSpinner(stringResource(R.string.player_searching))
        }
        is SubtitleSearchUi.Unavailable -> Note(search.message)
        is SubtitleSearchUi.Found -> {
            search.message?.let { Note(it) }
            search.results.forEach { found ->
                OptionRow(false, found.label, found.hint) { state.controller.addFoundSubtitle(found.id) }
            }
        }
        is SubtitleSearchUi.Adding -> search.results.forEach { found ->
            OptionRow(false, found.label, if (found.id == search.id) stringResource(R.string.player_adding) else found.hint, enabled = false) {}
        }
    }
    // Added: the search ends, the track is chosen, and the sheet closes.
    var began by remember { mutableStateOf(false) }
    LaunchedEffect(search) {
        if (search != null) began = true else if (began && state.sheet == SheetPage.FindSubtitles) state.closeSheet()
    }
}

/** A sentence in the sheet: why something isn't offered. */
@Composable
private fun Note(text: String) {
    Text(text, style = MaterialTheme.typography.bodyMedium, color = PleyColors.Muted, modifier = Modifier.padding(horizontal = 24.dp, vertical = 12.dp))
}

/** The sleep timer's choices; the one running ticked, with its minutes left. */
@Composable
private fun SleepPage(state: PlayerScreenState) {
    val resources = LocalResources.current
    val sleep = state.sleep
    SleepChoice.ALL.forEach { choice ->
        val on = sleep.choice == choice
        val hint = if (on && choice is SleepChoice.Minutes && sleep.minutesLeft != null) SleepTexts.summary(resources, sleep) else null
        OptionRow(on, SleepTexts.choice(resources, choice), hint) {
            state.controller.setSleep(choice)
            state.closeSheet()
        }
    }
}

@Composable
private fun QualityPage(state: PlayerScreenState) {
    if (state.canPlayOriginal) {
        ActionRow(PleyIcons.RotateCcw, stringResource(R.string.player_play_original)) {
            state.controller.playOriginal()
            state.closeSheet()
        }
    }
    state.qualities.forEach { choice ->
        OptionRow(choice.selected, choice.label, choice.hint) {
            state.controller.selectQuality(choice.id)
            state.closeSheet()
        }
    }
}

@Composable
private fun sizeLabel(index: Int): String = stringResource(
    when (index) {
        0 -> R.string.player_size_small
        2 -> R.string.player_size_large
        3 -> R.string.player_size_huge
        else -> R.string.player_size_normal
    },
)

@Composable
private fun StylePage(style: SubtitleChoice, onStyle: (SubtitleChoice) -> Unit) {
    GroupLabel(stringResource(R.string.player_size))
    PlayerLook.subtitleSizes.indices.forEach { i ->
        OptionRow(style.sizeIndex == i, sizeLabel(i)) { onStyle(style.copy(sizeIndex = i)) }
    }
    GroupLabel(stringResource(R.string.player_background))
    OptionRow(!style.box, stringResource(R.string.player_background_none)) { onStyle(style.copy(box = false)) }
    OptionRow(style.box, stringResource(R.string.player_background_box)) { onStyle(style.copy(box = true)) }
    GroupLabel(stringResource(R.string.settings_subtitle_font))
    CaptionLook.fonts().forEach { font ->
        OptionRow(style.font == font, stringResource(fontLabel(font))) { onStyle(style.copy(font = font)) }
    }
    GroupLabel(stringResource(R.string.settings_subtitle_position))
    PlayerLook.subtitleRaises.forEachIndexed { i, raise ->
        OptionRow(style.raiseIndex == i, stringResource(raiseLabel(raise))) { onStyle(style.copy(raiseIndex = i)) }
    }
}

/**
 * How it plays (apps/web/src/player/PlaybackDetails.tsx): the way and why,
 * what goes in and comes out, live while open; then this device's side:
 * decoder, picture, dropped frames, the display.
 */
@Composable
private fun DetailsPage(state: PlayerScreenState) {
    val details: PlaybackDetailsUi? = state.details
    if (details == null) {
        Box(Modifier.fillMaxWidth().padding(24.dp), contentAlignment = Alignment.Center) {
            DelayedSpinner(stringResource(R.string.player_loading))
        }
        return
    }
    Column(Modifier.padding(horizontal = 24.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Text(details.method, style = MaterialTheme.typography.titleSmall, color = PlayerLook.badgeColors(details.method).text)
            details.pipeline?.let {
                Text(
                    it,
                    style = MaterialTheme.typography.labelSmall,
                    color = PleyColors.Muted,
                    modifier = Modifier.background(PleyColors.Surface3, PleyShape.Badge).padding(horizontal = 6.dp, vertical = 2.dp),
                )
            }
        }
        details.why?.let { Text(it, style = MaterialTheme.typography.bodySmall, color = PleyColors.Muted, modifier = Modifier.padding(top = 4.dp)) }
        Column(Modifier.padding(top = 12.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
            details.rows.forEach { DetailLine(it.label, it.value) }
        }
    }
    val engineState = state.controller.engine?.state?.collectAsState()
    val device = deviceLines(engineState?.value)
    if (device.isNotEmpty() || details.engine.isNotEmpty()) {
        GroupLabel(stringResource(R.string.player_details_device))
        Column(Modifier.padding(horizontal = 24.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
            if (device.isNotEmpty()) {
                device.forEach { (label, value) -> DetailLine(label, value) }
            } else {
                details.engine.forEach { Text(it, style = MaterialTheme.typography.bodySmall, color = PleyColors.Fg) }
            }
        }
    }
}

/** This device's lines: the decoder and whether it is hardware, the picture, dropped frames, audio out, the display. */
@Composable
private fun deviceLines(engine: EngineState?): List<Pair<String, String>> {
    val stats: PlaybackStats = engine?.stats ?: return emptyList()
    val lines = mutableListOf<Pair<String, String>>()
    stats.videoDecoder?.let { name ->
        val value = when (stats.videoDecoderHardware) {
            true -> stringResource(R.string.player_details_hardware, name)
            false -> stringResource(R.string.player_details_software, name)
            null -> name
        }
        lines += stringResource(R.string.player_details_decoder) to value
    }
    if (stats.width != null && stats.height != null) {
        val rate = stats.frameRate?.takeIf { it > 0 }?.let { " · %.3f fps".format(it).replace(".000", "") }.orEmpty()
        val range = stats.dynamicRange?.let { " · $it" }.orEmpty()
        lines += stringResource(R.string.player_details_picture) to "${stats.width} × ${stats.height}$rate$range"
    }
    val frames = stats.renderedFrames + stats.droppedFrames
    if (frames > 0) lines += stringResource(R.string.player_details_dropped) to stringResource(R.string.player_details_dropped_of, stats.droppedFrames, frames)
    stats.audioCodec?.let { codec ->
        val channels = stats.audioChannels?.let { " $it ch" }.orEmpty()
        lines += stringResource(R.string.player_details_audio_out) to stringResource(
            if (stats.passthrough) R.string.player_details_passthrough else R.string.player_details_decoded,
            "$codec$channels",
        )
    }
    stats.firstFrameMs?.let { lines += stringResource(R.string.player_details_first_frame) to stringResource(R.string.player_details_ms, it.toInt()) }
    displayLine(LocalView.current.display)?.let { lines += stringResource(R.string.player_details_display) to it }
    return lines
}

/** "2400 × 1080 · 120 Hz · HDR10, HLG": the display's mode now and the HDR it shows. */
@Suppress("DEPRECATION")
private fun displayLine(display: Display?): String? {
    val mode = display?.mode ?: return null
    val hdr = if (Build.VERSION.SDK_INT >= 34) mode.supportedHdrTypes else display.hdrCapabilities?.supportedHdrTypes ?: IntArray(0)
    val names = hdr.toList().mapNotNull {
        when (it) {
            Display.HdrCapabilities.HDR_TYPE_DOLBY_VISION -> "Dolby Vision"
            Display.HdrCapabilities.HDR_TYPE_HDR10 -> "HDR10"
            Display.HdrCapabilities.HDR_TYPE_HDR10_PLUS -> "HDR10+"
            Display.HdrCapabilities.HDR_TYPE_HLG -> "HLG"
            else -> null
        }
    }.distinct()
    val rate = "%.0f Hz".format(mode.refreshRate)
    return listOfNotNull("${mode.physicalWidth} × ${mode.physicalHeight}", rate, names.joinToString(", ").ifEmpty { null }).joinToString(" · ")
}

/** Chapters (apps/web/src/player/ChaptersMenu.tsx): 96 dp pictures, the one playing in accent; a tap seeks to it. */
@Composable
private fun ColumnScope.ChaptersPage(state: PlayerScreenState) {
    val chapters = state.chapters
    val current = PlayerLook.chapterAt(chapters, state.livePosition() + 250)
    val list = rememberLazyListState(initialFirstVisibleItemIndex = (current - 1).coerceAtLeast(0))
    val pictures = chapters.any { it.imageUrl != null }
    LazyColumn(Modifier.fillMaxWidth().weight(1f, fill = false), state = list) {
        itemsIndexed(chapters, key = { i, c -> "${c.startMs}-$i" }, contentType = { _, _ -> "chapter" }) { i, chapter ->
            val fallback = stringResource(R.string.player_chapter_number, i + 1)
            val title = PlayerLook.chapterTitle(chapter, i) { fallback }
            val color = if (i == current) PleyColors.Accent else PleyColors.Fg
            Row(
                Modifier
                    .fillMaxWidth()
                    .heightIn(min = 56.dp)
                    .clickable(role = Role.Button) {
                        state.controller.seekTo(chapter.startMs)
                        state.playhead.longValue = chapter.startMs
                        state.closeSheet()
                    }
                    .semantics { selected = i == current }
                    .padding(horizontal = 24.dp, vertical = 8.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(16.dp),
            ) {
                if (pictures) {
                    Box(Modifier.width(96.dp).aspectRatio(16f / 9f).clip(PleyShape.Badge).background(PleyColors.Surface3)) {
                        Artwork(chapter.imageUrl?.let { ArtworkImage(listOf(it)) }, "", Modifier.fillMaxSize())
                    }
                }
                Column(Modifier.weight(1f)) {
                    Text(title, style = MaterialTheme.typography.bodyLarge, color = color, maxLines = 1, overflow = TextOverflow.Ellipsis)
                    Text(PlayerLook.clock(chapter.startMs), style = MaterialTheme.typography.bodySmall, color = PleyColors.Muted)
                }
            }
        }
    }
}

@Composable
private fun GroupLabel(text: String) {
    Text(
        text,
        style = MaterialTheme.typography.labelMedium,
        color = PleyColors.Muted,
        modifier = Modifier.padding(start = 24.dp, end = 24.dp, top = 16.dp, bottom = 4.dp).semantics { heading() },
    )
}

@Composable
private fun DetailLine(label: String, value: String) {
    Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        Text(label, style = MaterialTheme.typography.bodySmall, color = PleyColors.Muted, modifier = Modifier.width(104.dp))
        Text(value, style = MaterialTheme.typography.bodySmall, color = PleyColors.Fg, modifier = Modifier.weight(1f))
    }
}

/** A row that opens a page: icon, label, the value now, a chevron. 56 dp. */
@Composable
private fun NavRow(icon: PleyIcon, label: String, value: String, enabled: Boolean = true, onClick: () -> Unit) {
    Row(
        Modifier
            .fillMaxWidth()
            .heightIn(min = 56.dp)
            .clickable(enabled = enabled, role = Role.Button, onClick = onClick)
            .padding(horizontal = 24.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        Icon(icon, contentDescription = null, size = 22.dp, tint = if (enabled) PleyColors.Fg else PleyColors.Disabled)
        Text(label, style = MaterialTheme.typography.bodyLarge, color = if (enabled) PleyColors.Fg else PleyColors.Disabled, modifier = Modifier.weight(1f))
        Text(value, style = MaterialTheme.typography.bodyMedium, color = PleyColors.Muted, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.widthIn(max = 220.dp))
        Icon(PleyIcons.ChevronRight, contentDescription = null, size = 18.dp, tint = PleyColors.Muted)
    }
}

/** A row that does something at once. */
@Composable
private fun ActionRow(icon: PleyIcon, label: String, onClick: () -> Unit) {
    Row(
        Modifier
            .fillMaxWidth()
            .heightIn(min = 56.dp)
            .clickable(role = Role.Button, onClick = onClick)
            .padding(horizontal = 24.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        Icon(icon, contentDescription = null, size = 22.dp, tint = PleyColors.AccentBright)
        Text(label, style = MaterialTheme.typography.bodyLarge, color = PleyColors.AccentBright)
    }
}

/** A choice: the accent check when chosen, the label and an optional hint. 56 dp. */
@Composable
private fun OptionRow(selected: Boolean, label: String, hint: String? = null, enabled: Boolean = true, onClick: () -> Unit) {
    Row(
        Modifier
            .fillMaxWidth()
            .heightIn(min = 56.dp)
            .clickable(enabled = enabled, role = Role.RadioButton, onClick = onClick)
            .semantics { this.selected = selected }
            .padding(horizontal = 24.dp, vertical = 6.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        Box(Modifier.size(22.dp)) {
            if (selected) Icon(PleyIcons.Check, contentDescription = null, size = 22.dp, tint = PleyColors.Accent)
        }
        Column(Modifier.weight(1f)) {
            Text(label, style = MaterialTheme.typography.bodyLarge, color = if (enabled) PleyColors.Fg else PleyColors.Disabled, maxLines = 1, overflow = TextOverflow.Ellipsis)
            hint?.let { Text(it, style = MaterialTheme.typography.bodySmall, color = PleyColors.Muted, maxLines = 2, overflow = TextOverflow.Ellipsis) }
        }
    }
}
