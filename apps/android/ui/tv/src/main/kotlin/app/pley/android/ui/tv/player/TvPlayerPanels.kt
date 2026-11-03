package app.pley.android.ui.tv.player

import android.view.KeyEvent as AndroidKeyEvent
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.focusGroup
import androidx.compose.foundation.focusable
import androidx.compose.foundation.gestures.animateScrollBy
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicText
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.State
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusProperties
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.key.onKeyEvent
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import app.pley.android.data.Settings
import app.pley.android.data.SubtitleFont
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyElevation
import app.pley.android.designsystem.PleyMotion
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.PleyType
import app.pley.android.designsystem.TvSafeArea
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.reducedMotion
import app.pley.android.player.PlaybackController
import app.pley.android.player.PlaybackSpeeds
import app.pley.android.player.SubtitleSearchUi
import app.pley.android.player.PlayerUiState
import app.pley.android.player.engine.EngineState
import app.pley.android.player.sleep.SleepChoice
import app.pley.android.player.sleep.SleepState
import app.pley.android.player.sleep.SleepTexts
import app.pley.android.player.subtitles.CaptionLook
import app.pley.android.ui.tv.R
import app.pley.android.ui.tv.item.FactLine
import app.pley.android.ui.tv.item.PanelHeading
import app.pley.android.ui.tv.item.PanelRow
import app.pley.android.ui.tv.kit.focusRow
import app.pley.android.ui.tv.settings.fontLabel
import app.pley.android.ui.tv.settings.raiseLabel
import kotlin.math.abs
import kotlinx.coroutines.launch
import java.util.Locale

/**
 * The player's side panels (design spec §2.7): Subtitles (with Sync while
 * the subtitles chosen can move, Find subtitles… while the file streams
 * from the server, and their size, background, font and position while
 * they are text, when [onSubtitleStyle] saves them), Find subtitles' results, Audio (with the
 * audio delay), Quality, Sleep timer, Speed and Info. 360 dp (Info 440 dp) from the right edge, full height, surface2
 * with 16 dp left corners. It slides in on its own layer over 200 ms (in
 * place with reduced motion) and the scrim behind it fades by its colour
 * while drawing, so no layer covers the video. Focus starts on the current
 * choice and stays inside; choosing closes the panel, and Back does too.
 */
@Composable
internal fun BoxScope.TvPlayerPanels(
    ui: TvPlayerUi,
    state: State<PlayerUiState>,
    engine: State<EngineState?>,
    controller: PlaybackController,
    captionLook: CaptionLook = CaptionLook(),
    onSubtitleStyle: ((CaptionLook) -> Unit)? = null,
) {
    val open = ui.panel != null
    val reduced = reducedMotion()
    val shown by animateFloatAsState(if (open) 1f else 0f, PleyMotion.overlay(reduced), label = "panel")
    val present by remember { derivedStateOf { open || shown > 0f } }
    if (!present) return
    Box(Modifier.fillMaxSize().drawBehind { drawRect(PleyColors.Scrim.copy(alpha = PleyColors.Scrim.alpha * shown)) })
    val panel = ui.panel ?: ui.lastPanel ?: return
    Box(
        Modifier
            .align(Alignment.CenterEnd)
            .width(if (panel == PlayerPanel.Info) INFO_WIDTH else PANEL_WIDTH)
            .fillMaxHeight()
            .graphicsLayer { translationX = (1f - shown) * size.width }
            .clip(RoundedCornerShape(topStart = PleyShape.DialogRadius, bottomStart = PleyShape.DialogRadius))
            .drawBehind {
                drawRect(PleyColors.Surface2)
                drawRect(PleyColors.Surface4, size = androidx.compose.ui.geometry.Size(size.width, PleyElevation.Level2Hairline.toPx()))
            }
            .testTag(TAG_PANEL)
            // Focus stays in the panel until it closes.
            .focusProperties { onExit = { if (ui.panel != null) cancelFocusChange() } }
            .focusGroup()
            .padding(start = 24.dp, end = TvSafeArea.Horizontal, top = TvSafeArea.Vertical, bottom = TvSafeArea.Vertical),
    ) {
        when (panel) {
            PlayerPanel.Info -> InfoPanel(state, engine, ui)
            else -> ChoicePanel(panel, state, ui, controller, captionLook, onSubtitleStyle)
        }
    }
}

/** A list of choices, the current one ticked and focused first. */
@Composable
private fun ChoicePanel(
    panel: PlayerPanel,
    state: State<PlayerUiState>,
    ui: TvPlayerUi,
    controller: PlaybackController,
    captionLook: CaptionLook,
    onSubtitleStyle: ((CaptionLook) -> Unit)?,
) {
    val s by remember(state) { derivedStateOf { PanelData.of(state.value) } }
    val focus = ui.requester(PlayerFocus.Panel)
    val title = stringResource(
        when (panel) {
            PlayerPanel.Subtitles -> R.string.tv_player_subtitles
            PlayerPanel.Audio -> R.string.tv_player_audio
            PlayerPanel.Quality -> R.string.tv_player_quality
            PlayerPanel.Info -> R.string.tv_player_info_title
            PlayerPanel.Sleep -> R.string.tv_player_sleep
            PlayerPanel.Speed -> R.string.tv_player_speed
            PlayerPanel.FindSubtitles -> R.string.tv_player_find_subtitles
        },
    )
    val original = stringResource(R.string.tv_player_play_original)
    val originalHint = stringResource(R.string.tv_player_play_original_hint)
    val style = SubtitleStyleTexts(
        size = stringResource(R.string.tv_settings_subtitle_size),
        sizes = SubtitleSizeLabels.map { stringResource(it) },
        background = stringResource(R.string.tv_settings_subtitle_background),
        backgrounds = listOf(stringResource(R.string.tv_settings_background_none), stringResource(R.string.tv_settings_background_box)),
        font = stringResource(R.string.tv_settings_subtitle_font),
        fonts = CaptionLook.fonts().map { it to stringResource(fontLabel(it)) },
        position = stringResource(R.string.tv_settings_subtitle_position),
        positions = Settings.SUBTITLE_RAISES.map { stringResource(raiseLabel(it)) },
    )
    val findSubtitles = stringResource(R.string.tv_player_find_subtitles)
    val searching = stringResource(R.string.tv_player_searching)
    val adding = stringResource(R.string.tv_player_adding)
    val resources = LocalResources.current
    // Each panel's own list: its current choice may be further down than shows.
    val list = remember(panel) { LazyListState() }
    LaunchedEffect(panel, ui.panel != null) {
        if (ui.panel != null) {
            withFrameNanos { }
            list.focusRow(focus)
        }
    }
    if (panel == PlayerPanel.FindSubtitles) {
        // The search moves on (results, a refusal): focus goes to what shows; added, the panel closes.
        val stage = s.subtitleSearch?.let { it::class }
        val began = remember { BooleanArray(1) }
        LaunchedEffect(stage) {
            if (ui.panel != PlayerPanel.FindSubtitles) return@LaunchedEffect
            if (stage == null) {
                if (began[0]) ui.closePanel()
                return@LaunchedEffect
            }
            began[0] = true
            withFrameNanos { }
            list.focusRow(focus)
        }
    }
    LazyColumn(Modifier.fillMaxSize(), state = list) {
        item(key = "title") { PanelTitle(title) }
        when (panel) {
            PlayerPanel.Subtitles -> {
                if (s.subtitleSync) {
                    item(key = "sync") { SyncRow(s.subtitleOffsetMs, onChange = { controller.setSubtitleOffset(it) }) }
                }
                choices(
                    s.subtitles.map { Row(it.label, it.selected, null) },
                    focus,
                ) { i ->
                    controller.selectSubtitle(s.subtitles[i].index)
                    ui.closePanel()
                }
                if (s.canFindSubtitles) {
                    item(key = "find") {
                        PanelRow(
                            findSubtitles,
                            onClick = {
                                controller.searchSubtitles()
                                ui.openPanel(PlayerPanel.FindSubtitles)
                            },
                            icon = PleyIcons.Search,
                            focusRequester = if (s.subtitles.isEmpty()) focus else null,
                        )
                    }
                }
                // Pictures keep their own look, and subtitles off have none.
                if (onSubtitleStyle != null && s.subtitleStyled) subtitleStyle(captionLook, style, onSubtitleStyle)
            }
            PlayerPanel.FindSubtitles -> when (val search = s.subtitleSearch) {
                null, SubtitleSearchUi.Searching -> item(key = "searching") { PanelNote(searching, Modifier.focusRequester(focus).focusable()) }
                is SubtitleSearchUi.Unavailable -> item(key = "unavailable") { PanelNote(search.message, Modifier.focusRequester(focus).focusable()) }
                is SubtitleSearchUi.Found -> {
                    search.message?.let { message -> item(key = "nothing") { PanelNote(message, Modifier.focusRequester(focus).focusable()) } }
                    choices(search.results.map { Row(it.label, false, it.hint) }, focus) { i -> controller.addFoundSubtitle(search.results[i].id) }
                }
                is SubtitleSearchUi.Adding -> choices(
                    search.results.map { Row(it.label, false, if (it.id == search.id) adding else it.hint) },
                    focus,
                ) { }
            }
            PlayerPanel.Speed -> {
                s.speedNote?.let { note -> item(key = "note") { PanelNote(note) } }
                choices(PlaybackSpeeds.ALL.map { Row(PlaybackSpeeds.label(it), it == s.speed, null) }, focus) { i ->
                    controller.setSpeed(PlaybackSpeeds.ALL[i])
                    ui.closePanel()
                }
            }
            PlayerPanel.Sleep -> choices(
                SleepChoice.ALL.map { choice ->
                    val on = s.sleep.choice == choice
                    val left = s.sleep.minutesLeft?.takeIf { on && choice is SleepChoice.Minutes }
                    Row(SleepTexts.choice(resources, choice), on, left?.let { SleepTexts.summary(resources, s.sleep) })
                },
                focus,
            ) { i ->
                controller.setSleep(SleepChoice.ALL[i])
                ui.closePanel()
            }
            PlayerPanel.Audio -> {
                item(key = "delay") { AudioDelayRow(s.audioDelayMs, onChange = { controller.setAudioDelay(it) }) }
                choices(s.audio.map { Row(it.label, it.selected, null) }, focus) { i ->
                    s.audio[i].index?.let(controller::selectAudio)
                    ui.closePanel()
                }
            }
            PlayerPanel.Quality -> {
                s.note?.let { note -> item(key = "note") { PanelNote(note) } }
                if (s.canPlayOriginal) {
                    item(key = "original") {
                        PanelRow(original, onClick = {
                            controller.playOriginal()
                            ui.closePanel()
                        }, supporting = originalHint, icon = PleyIcons.RotateCcw)
                    }
                }
                choices(s.qualities.map { Row(it.label, it.selected, it.hint) }, focus) { i ->
                    controller.selectQuality(s.qualities[i].id)
                    ui.closePanel()
                }
            }
            PlayerPanel.Info -> Unit
        }
    }
}

private data class Row(val label: String, val checked: Boolean, val supporting: String?)

/** The words of the subtitles' size, background, font and position rows. */
private class SubtitleStyleTexts(
    val size: String,
    val sizes: List<String>,
    val background: String,
    val backgrounds: List<String>,
    val font: String,
    val fonts: List<Pair<SubtitleFont, String>>,
    val position: String,
    val positions: List<String>,
)

/** Small, Normal, Large and Huge: [Settings.SUBTITLE_SIZES]'s names. */
private val SubtitleSizeLabels = listOf(
    R.string.tv_settings_size_small,
    R.string.tv_settings_size_medium,
    R.string.tv_settings_size_large,
    R.string.tv_settings_size_largest,
)

/**
 * The subtitles' size and background under the tracks, as Settings has
 * them (and saved there): choosing keeps the panel open, and the
 * subtitles change behind it.
 */
private fun LazyListScope.subtitleStyle(look: CaptionLook, texts: SubtitleStyleTexts, onStyle: (CaptionLook) -> Unit) {
    val size = Settings.SUBTITLE_SIZES.indices.minBy { abs(Settings.SUBTITLE_SIZES[it] - look.textFraction) }
    item(key = "size") { PanelHeading(texts.size) }
    itemsIndexed(texts.sizes, key = { i, _ -> "size-$i" }) { i, label ->
        PanelRow(label, onClick = { onStyle(look.copy(textFraction = Settings.SUBTITLE_SIZES[i])) }, checked = i == size)
    }
    item(key = "background") { PanelHeading(texts.background) }
    itemsIndexed(texts.backgrounds, key = { i, _ -> "background-$i" }) { i, label ->
        PanelRow(label, onClick = { onStyle(look.copy(box = i == 1)) }, checked = look.box == (i == 1))
    }
    item(key = "font") { PanelHeading(texts.font) }
    itemsIndexed(texts.fonts, key = { _, (font, _) -> "font-$font" }) { _, (font, label) ->
        PanelRow(label, onClick = { onStyle(look.copy(font = font)) }, checked = look.font == font)
    }
    val raise = Settings.SUBTITLE_RAISES.indices.minBy { abs(Settings.SUBTITLE_RAISES[it] - look.raise) }
    item(key = "position") { PanelHeading(texts.position) }
    itemsIndexed(texts.positions, key = { i, _ -> "position-$i" }) { i, label ->
        PanelRow(label, onClick = { onStyle(look.copy(raise = Settings.SUBTITLE_RAISES[i])) }, checked = i == raise)
    }
}

/** Rows of [rows]; the checked one (or the first) takes [focus]. */
private fun LazyListScope.choices(rows: List<Row>, focus: FocusRequester, onChoose: (Int) -> Unit) {
    val first = rows.indexOfFirst { it.checked }.takeIf { it >= 0 } ?: 0
    itemsIndexed(rows, key = { i, _ -> "row-$i" }) { i, row ->
        PanelRow(
            row.label,
            onClick = { onChoose(i) },
            checked = row.checked,
            supporting = row.supporting,
            focusRequester = if (i == first) focus else null,
        )
    }
}

/** What the choice panels read, so they recompose with their choices, not with the playhead. */
private data class PanelData(
    val subtitles: List<app.pley.android.player.TrackChoice>,
    val audio: List<app.pley.android.player.TrackChoice>,
    val qualities: List<app.pley.android.player.QualityChoice>,
    val note: String?,
    val canPlayOriginal: Boolean,
    val subtitleSync: Boolean,
    val subtitleStyled: Boolean,
    val subtitleOffsetMs: Long,
    val sleep: SleepState,
    val speed: Float,
    val speedNote: String?,
    val audioDelayMs: Long,
    val canFindSubtitles: Boolean,
    val subtitleSearch: SubtitleSearchUi?,
) {
    companion object {
        fun of(s: PlayerUiState) = PanelData(
            subtitles = s.subtitles,
            audio = s.audio,
            qualities = s.qualities,
            note = s.note,
            canPlayOriginal = s.canPlayOriginal,
            subtitleSync = s.subtitleSync,
            subtitleStyled = s.subtitleStyled,
            subtitleOffsetMs = s.subtitleOffsetMs,
            sleep = s.sleep,
            speed = s.speed,
            speedNote = s.speedNote,
            audioDelayMs = s.audioDelayMs,
            canFindSubtitles = s.canFindSubtitles,
            subtitleSearch = s.subtitleSearch,
        )
    }
}

/**
 * Info, the stats for nerds ([TvReadout]): how the server sends it and
 * why, the file, every stream in play in full, and what this TV does with
 * them (decoders, the display's mode and HDR, frames, buffer, network).
 * Read once a second; Up and Down scroll it.
 */
@Composable
private fun InfoPanel(state: State<PlayerUiState>, engine: State<EngineState?>, ui: TvPlayerUi) {
    val input by rememberReadout(state, engine)
    val resources = LocalResources.current
    val sections = remember(input, resources) { TvReadout.sections(resources, input) }
    val scroll = rememberScrollState()
    val scope = rememberCoroutineScope()
    val step = with(LocalDensity.current) { 96.dp.toPx() }
    Column(
        Modifier
            .fillMaxSize()
            .focusRequester(ui.requester(PlayerFocus.Panel))
            .onKeyEvent { event ->
                val native = event.nativeKeyEvent
                if (native.action != AndroidKeyEvent.ACTION_DOWN) return@onKeyEvent false
                when (native.keyCode) {
                    AndroidKeyEvent.KEYCODE_DPAD_DOWN -> {
                        scope.launch { scroll.animateScrollBy(step) }
                        true
                    }
                    AndroidKeyEvent.KEYCODE_DPAD_UP -> {
                        scope.launch { scroll.animateScrollBy(-step) }
                        true
                    }
                    else -> false
                }
            }
            .focusable()
            .verticalScroll(scroll),
    ) {
        PanelTitle(stringResource(R.string.tv_player_info_title))
        if (input.details == null) PanelNote(stringResource(R.string.tv_player_info_waiting))
        sections.forEach { section ->
            PanelHeading(section.title)
            Column(Modifier.padding(horizontal = 16.dp)) {
                section.facts.forEach { FactLine(it, labelWidth = INFO_LABEL_WIDTH) }
            }
        }
        Spacer(Modifier.height(24.dp))
    }
}

/** A MIME type as people know the codec: "video/hevc" is HEVC, "audio/eac3-joc" is E-AC-3 JOC (Atmos). */
internal fun codecName(mime: String): String = when (mime.lowercase(Locale.ROOT)) {
    "video/hevc" -> "HEVC"
    "video/avc" -> "H.264"
    "video/dolby-vision" -> "Dolby Vision"
    "video/av01" -> "AV1"
    "video/x-vnd.on2.vp9" -> "VP9"
    "video/mpeg2" -> "MPEG-2"
    "audio/ac3" -> "AC-3"
    "audio/eac3" -> "E-AC-3"
    "audio/eac3-joc" -> "E-AC-3 JOC (Atmos)"
    "audio/true-hd" -> "TrueHD"
    "audio/vnd.dts" -> "DTS"
    "audio/vnd.dts.hd" -> "DTS-HD"
    "audio/mp4a-latm" -> "AAC"
    "audio/opus" -> "Opus"
    "audio/flac" -> "FLAC"
    "audio/raw" -> "PCM"
    else -> mime.substringAfter('/')
}

@Composable
private fun PanelTitle(text: String) {
    BasicText(text, style = PleyType.Tv.Title.copy(color = PleyColors.Fg), maxLines = 1, modifier = Modifier.padding(start = 16.dp, top = 8.dp, bottom = 8.dp))
}

@Composable
private fun PanelNote(text: String, modifier: Modifier = Modifier) {
    BasicText(text, style = PleyType.Tv.Body.copy(color = PleyColors.Muted), modifier = modifier.padding(horizontal = 16.dp, vertical = 8.dp))
}

private val PANEL_WIDTH = 360.dp

/** Info is wider: its readout has a label column. */
private val INFO_WIDTH = 440.dp
private val INFO_LABEL_WIDTH = 112.dp

internal const val TAG_PANEL = "pley.tv.player.panel"
