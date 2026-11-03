package app.pley.android.ui.tv.player

import android.view.KeyEvent as AndroidKeyEvent
import androidx.compose.animation.core.EaseOut
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Image
import androidx.compose.foundation.focusable
import androidx.compose.foundation.gestures.animateScrollBy
import androidx.compose.foundation.gestures.scrollBy
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.Stable
import androidx.compose.runtime.State
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.runtime.snapshots.Snapshot
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.input.key.onKeyEvent
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyFocus
import app.pley.android.designsystem.PleyMotion
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.PleyTv
import app.pley.android.designsystem.PleyType
import app.pley.android.designsystem.TvSafeArea
import app.pley.android.designsystem.image.Artwork
import app.pley.android.designsystem.image.ArtworkImage
import app.pley.android.designsystem.image.ArtworkLoad
import app.pley.android.designsystem.reducedMotion
import app.pley.android.player.ChapterMark
import app.pley.android.player.PlaybackController
import app.pley.android.player.PlayerUiState
import app.pley.android.ui.tv.R
import app.pley.android.ui.tv.kit.KeyRepeatGate
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.filterNotNull

/**
 * The chapters as a row of cards over the scrubber, as YouTube's TV app has
 * them (the native Samsung app's ChapterStrip): Up on the scrubber while a
 * seek waits opens it on the chapter the seek was going to, the Chapters
 * button on the one playing ([TvPlayerUi.openChapters]). Left and Right go
 * through the chapters, a card a press, without wrapping; OK, let go, goes
 * to the start of the one focused ([onGo], its index); Down or Back closes
 * it, and focus goes back where it was; Up does nothing. The video plays on
 * meanwhile, and the scrubber shows the focused chapter's start.
 *
 * The cards are the seek preview's size, in its place: 240 × 135 from
 * y = 212, the first at the gutter and 14 dp apart, each with the
 * chapter's name (label, muted; fg while focused) and when it starts
 * (caption, subtle) under its picture, and the accent ring around the
 * focused one's. The row slides 150 ms (in place with reduced motion) to
 * keep the focused card at the gutter, or the last at the right safe edge.
 * A card's picture is the chapter's own, else (or when that doesn't load)
 * the server's seek preview a moment into it, both letterboxed on black,
 * as the video is. A held key moves a card at the held-key pace of rows
 * ([CardHold]), the row jumping to it, and pictures wait until it's let go. The row is one focus target that keeps its keys, so
 * only the two cards whose look changes recompose as focus moves.
 */
@Composable
internal fun BoxScope.TvChapterStrip(state: State<PlayerUiState>, ui: TvPlayerUi, controller: PlaybackController, onGo: (Int) -> Unit) {
    val open by remember(ui) { derivedStateOf { ui.chapter != null } }
    if (!open) return
    val chapters by remember(state) { derivedStateOf { state.value.chapters } }
    val hasThumbs by remember(state) { derivedStateOf { state.value.hasThumbnails } }
    // Opened on its chapter; read unobserved, so a move doesn't compose the row again.
    val list = rememberLazyListState(initialFirstVisibleItemIndex = Snapshot.withoutReadObservation { ui.chapter ?: 0 })
    val reduced = reducedMotion()
    val pitch = with(LocalDensity.current) { CARD_WIDTH.roundToPx() + CARD_GAP.roundToPx() }
    val hold = remember { CardHold() }
    // The row follows focus: every card is as wide, so the scroll to a card is its index times their pitch.
    // While a key is held it goes there at once: a slide begun at every move would never land.
    LaunchedEffect(list, pitch, reduced) {
        snapshotFlow { ui.chapter }.filterNotNull().collectLatest { i ->
            val by = (i * pitch - (list.firstVisibleItemIndex * pitch + list.firstVisibleItemScrollOffset)).toFloat()
            when {
                by == 0f -> Unit
                reduced || hold.held -> list.scrollBy(by)
                else -> list.animateScrollBy(by, tween(PleyMotion.FOCUS_MS, easing = EaseOut))
            }
        }
    }
    val resources = LocalResources.current
    // An OK pressed while the cards have focus: only its key up, after a down they saw, goes to the chapter.
    val okDown = remember { BooleanArray(1) }
    // What a screen reader says of the strip: the chapter focused, and from when.
    val spoken by remember(state, ui, resources) {
        derivedStateOf {
            val i = ui.chapter ?: return@derivedStateOf ""
            val chapter = state.value.chapters.getOrNull(i) ?: return@derivedStateOf ""
            val name = chapter.name ?: resources.getString(R.string.tv_player_chapter, i + 1)
            resources.getString(R.string.tv_player_chapter_card, name, TvPlayerText.spoken(chapter.startMs))
        }
    }
    Box(
        Modifier
            .align(Alignment.TopStart)
            .padding(top = CHAPTERS_TOP)
            .fillMaxWidth()
            .testTag(TAG_CHAPTER_STRIP)
            .focusRequester(ui.requester(PlayerFocus.ChapterStrip))
            .onFocusChanged { if (it.isFocused) ui.focused = PlayerFocus.ChapterStrip else if (ui.focused == PlayerFocus.ChapterStrip) ui.focused = null }
            .onKeyEvent { event ->
                val native = event.nativeKeyEvent
                val down = native.action == AndroidKeyEvent.ACTION_DOWN
                val at = ui.chapter
                when (native.keyCode) {
                    AndroidKeyEvent.KEYCODE_DPAD_LEFT, AndroidKeyEvent.KEYCODE_DPAD_RIGHT -> {
                        val by = if (native.keyCode == AndroidKeyEvent.KEYCODE_DPAD_LEFT) -1 else 1
                        when {
                            !down -> hold.release()
                            hold.admit(native.repeatCount, native.eventTime) && at != null ->
                                ui.focusChapter((at + by).coerceIn(0, (chapters.size - 1).coerceAtLeast(0)))
                        }
                        true
                    }
                    AndroidKeyEvent.KEYCODE_DPAD_DOWN -> {
                        if (down) ui.closeChapters()
                        true
                    }
                    // Nothing above the cards.
                    AndroidKeyEvent.KEYCODE_DPAD_UP -> true
                    // OK goes as it's let go, as the row's buttons click: the cards close, focus goes back
                    // to the Chapters button, and an OK let go there would click it, opening them again.
                    else -> if (TvPlayerKeys.isSelect(native.keyCode)) {
                        when {
                            down -> if (native.repeatCount == 0) okDown[0] = true
                            okDown[0] -> {
                                okDown[0] = false
                                if (at != null && at < chapters.size) onGo(at)
                            }
                        }
                        true
                    } else {
                        false
                    }
                }
            }
            .semantics { contentDescription = spoken }
            .focusable(),
    ) {
        LazyRow(
            Modifier.fillMaxWidth(),
            state = list,
            contentPadding = PaddingValues(start = PleyTv.Gutter, end = TvSafeArea.Horizontal, top = RING_ROOM, bottom = RING_ROOM),
            horizontalArrangement = Arrangement.spacedBy(CARD_GAP),
            userScrollEnabled = false,
        ) {
            itemsIndexed(chapters, key = { i, c -> "${c.startMs}-$i" }, contentType = { _, _ -> "chapter" }) { i, chapter ->
                val focused by remember(i) { derivedStateOf { ui.chapter == i } }
                ChapterCard(chapter, i, focused, hasThumbs, ui, hold, controller)
            }
        }
    }
}

/** A chapter's card: its picture, its name, and when it starts. */
@Composable
private fun ChapterCard(
    chapter: ChapterMark,
    index: Int,
    focused: Boolean,
    hasThumbs: Boolean,
    ui: TvPlayerUi,
    hold: CardHold,
    controller: PlaybackController,
) {
    Column(Modifier.width(CARD_WIDTH)) {
        ChapterPicture(chapter, focused, hasThumbs, ui, hold, controller)
        Spacer(Modifier.height(CARD_TEXT_GAP))
        BasicText(
            chapter.name ?: stringResource(R.string.tv_player_chapter, index + 1),
            style = PleyType.Tv.Label.copy(color = if (focused) PleyColors.Fg else PleyColors.Muted),
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
        BasicText(TvPlayerText.time(chapter.startMs), style = PleyType.Tv.Caption.copy(color = PleyColors.Subtle), maxLines = 1)
    }
}

/**
 * A card's picture, letterboxed on black: the chapter's own, else (or when
 * it doesn't load) the server's preview a moment ([PREVIEW_STEP_MS]) into
 * the chapter. A preview that hasn't come (the previews not ready yet, a
 * sheet that didn't load) is asked for again once they are, and as focus
 * moves; not while a key is held, which would ask for a sheet for every
 * card passed, but where it comes up.
 */
@Composable
private fun ChapterPicture(
    chapter: ChapterMark,
    focused: Boolean,
    hasThumbs: Boolean,
    ui: TvPlayerUi,
    hold: CardHold,
    controller: PlaybackController,
) {
    val own = remember(chapter.imageUrl) { chapter.imageUrl?.let { ArtworkImage(listOf(it), maxWidth = PREVIEW_IMAGE_WIDTH) } }
    val load = remember(own) { ArtworkLoad() }
    val fromPreview = own == null || load.failed
    var frame by remember(chapter.startMs) { mutableStateOf<ImageBitmap?>(null) }
    if (fromPreview && hasThumbs) {
        val at = chapter.startMs / PREVIEW_STEP_MS * PREVIEW_STEP_MS + PREVIEW_STEP_MS
        LaunchedEffect(at) {
            snapshotFlow { ui.chapter to hold.held }.collect { (_, held) ->
                if (!held && frame == null) controller.thumbnail(at)?.let { frame = it.asImageBitmap() }
            }
        }
    }
    Box(
        Modifier
            .size(CARD_WIDTH, PREVIEW_HEIGHT)
            .drawBehind { if (focused) drawCardRing() }
            .clip(PleyShape.TileTv)
            // The well until a picture shows, then the black around it.
            .drawBehind { drawRect(if (if (fromPreview) frame != null else load.loaded) Color.Black else PleyColors.Surface2) },
    ) {
        val picture = frame
        when {
            !fromPreview -> Artwork(own, title = "", modifier = Modifier.fillMaxSize(), contentScale = ContentScale.Fit, load = load)
            picture != null -> Image(picture, contentDescription = null, modifier = Modifier.fillMaxSize(), contentScale = ContentScale.Fit)
        }
    }
}

/** The focused card's ring: a [PleyFocus.RingStroke] of accent bright just outside its picture, round its corners. */
private fun DrawScope.drawCardRing() {
    val stroke = PleyFocus.RingStroke.toPx()
    val half = stroke / 2f
    drawRoundRect(
        PleyColors.AccentBright,
        topLeft = Offset(-half, -half),
        size = Size(size.width + stroke, size.height + stroke),
        cornerRadius = CornerRadius(PleyShape.TileTvRadius.toPx() + half),
        style = Stroke(stroke),
    )
}

/**
 * A held Left or Right on the cards. The remote repeats a held key every
 * 50 ms or so, and the player has no window-wide [KeyRepeatGate]: the
 * cards move at most once every [KeyRepeatGate.MOVE_INTERVAL_MS], as rows
 * do, and while one is [held] the row jumps rather than slides and the
 * pictures wait.
 */
@Stable
private class CardHold {
    var held: Boolean by mutableStateOf(false)
        private set

    private var lastMove = 0L

    /** Whether a Left or Right going down ([repeatCount], at [eventTime], ms) moves a card. */
    fun admit(repeatCount: Int, eventTime: Long): Boolean {
        if (repeatCount == 0) {
            held = false
            lastMove = eventTime
            return true
        }
        held = true
        if (eventTime - lastMove < KeyRepeatGate.MOVE_INTERVAL_MS) return false
        lastMove = eventTime
        return true
    }

    /** The key came up. */
    fun release() {
        held = false
    }
}

/** The cards are the seek preview's width, so a seek going to the chapters changes only what shows. */
private val CARD_WIDTH = PREVIEW_WIDTH
private val CARD_GAP = 14.dp
private val CARD_TEXT_GAP = 6.dp

/** Room around the cards for the focused one's ring. */
private val RING_ROOM = 6.dp

/** The row's top: its cards where the seek preview is, the ring's room above them. */
private val CHAPTERS_TOP = PREVIEW_TOP - RING_ROOM

internal const val TAG_CHAPTER_STRIP = "pley.tv.player.chapterStrip"
