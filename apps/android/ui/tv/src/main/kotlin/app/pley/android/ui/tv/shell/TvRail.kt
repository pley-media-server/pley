package app.pley.android.ui.tv.shell

import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.AnimationVector1D
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.focusGroup
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsFocusedAsState
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.wrapContentHeight
import androidx.compose.foundation.layout.wrapContentSize
import androidx.compose.foundation.layout.wrapContentWidth
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.focus.FocusDirection
import androidx.compose.ui.focus.FocusEnterExitScope
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusProperties
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.CompositingStrategy
import androidx.compose.ui.graphics.TransformOrigin
import androidx.compose.ui.graphics.drawscope.clipRect
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.luminance
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.util.lerp
import app.pley.android.common.TestTags
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyFocus
import app.pley.android.designsystem.PleyMotion
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.PleyTv
import app.pley.android.designsystem.PleyType
import app.pley.android.designsystem.TvSafeArea
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.image.Artwork
import app.pley.android.designsystem.image.GradientBitmaps
import app.pley.android.designsystem.image.drawStretched
import app.pley.android.designsystem.reducedMotion
import app.pley.android.screens.R as ScreensR
import app.pley.android.screens.requests.RequestsEntry
import app.pley.android.ui.tv.R
import app.pley.android.ui.tv.kit.TvRailFocus
import app.pley.android.ui.tv.requests.rememberRequestTexts
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch

/**
 * The rail's focus, for the shell and for content leaving on the left:
 * one requester per entry, the current entry, and whether the rail (and so
 * the open drawer) has focus.
 *
 * Going to the rail saves which element of the page had focus
 * ([FocusRequester.saveFocusedChild] on [content], D-pad rule 2), and
 * Right from the rail ([backToContent]) restores it.
 *
 * Focus comes into the rail only when someone sends it there: Left
 * (D-pad rule 3) or Back, to the current entry ([focusCurrent]). When the
 * focused element goes with its page and the next one has nothing to
 * focus yet (still loading), Android hands focus to the first thing it
 * finds, top left: the rail's avatar, which would open the rail by itself.
 * The rail refuses it ([enter]), and the page's initial focus takes it.
 * When focus is nowhere (what had it went with a redraw of its page) and
 * stays so, the next Up, Down or Right brings it back to the page.
 */
@Stable
class RailState(private val content: FocusRequester = FocusRequester()) : TvRailFocus {
    private val requesters = HashMap<String, FocusRequester>()

    /** The current entry's id; `null` while none (sign-in, Who's watching?). */
    var current: String? = null

    /**
     * The button that has focus in requests' news while the card shows
     * ([TvRequestNotices]): the window coming back (from the player, the
     * assistant) gives focus to it, and the page's place, saved as the card
     * came, is kept for when it closes.
     */
    internal var notice: FocusRequester? = null

    /** Whether focus is in the rail, which is what opens it. */
    var hasFocus by mutableStateOf(false)
        private set

    /** Set when focus comes into the rail, cleared when another page shows ([pageChanged]). */
    private var sentFromPage = false

    override val keepsFocus: Boolean get() = hasFocus && sentFromPage

    /** Whether the rail shows at all: not on the sign-in screens. */
    var shown by mutableStateOf(false)
        internal set

    /** Whether focus is on the page, as the shell's content tells it ([enter]). */
    internal var contentFocused = false

    /** Set while [focusCurrent] sends focus to an entry: the one way in ([enter]). */
    private var sending = false

    /** The rail's own, while it shows ([TvRail]): where [enter] waits for focus to settle. */
    internal var scope: CoroutineScope? = null

    internal fun requester(id: String): FocusRequester = requesters.getOrPut(id) { FocusRequester() }

    internal fun focusChanged(hasFocus: Boolean) {
        if (hasFocus && !this.hasFocus) sentFromPage = true
        this.hasFocus = hasFocus
    }

    /** Another page shows: chosen in the rail, it takes focus from it ([keepsFocus]). */
    internal fun pageChanged() {
        sentFromPage = false
    }

    override fun focusCurrent(): Boolean {
        if (!shown) return false
        val id = current ?: "home"
        val requester = requesters[id] ?: requesters["home"] ?: return false
        if (!hasFocus) runCatching { content.saveFocusedChild() }
        sending = true
        try {
            return runCatching { requester.requestFocus() }.isSuccess
        } finally {
            sending = false
        }
    }

    /**
     * Focus coming into the rail other than by [focusCurrent]: Left (with
     * nothing focused, while a page loads) goes to the current entry, not
     * the nearest one; anything else, such as Android placing focus on the
     * first thing it finds, is refused.
     *
     * Up, Down or Right with nothing focused (neither the page nor the
     * rail: what had focus went with a redraw) goes back to the page, so
     * the key isn't lost, but only once focus has stayed nowhere for a few
     * frames. Android comes here too when a focused tile leaves its row,
     * and that row hands focus to the tile in its place a frame later (a
     * row going with its last tile, two): taking focus to the page at once
     * would scroll it to wherever focus landed first.
     */
    internal fun FocusEnterExitScope.enter() {
        if (sending) return
        cancelFocusChange()
        when (requestedFocusDirection) {
            FocusDirection.Left -> focusCurrent()
            FocusDirection.Up, FocusDirection.Down, FocusDirection.Right -> if (!contentFocused) {
                scope?.launch {
                    repeat(SETTLE_FRAMES) { withFrameNanos { } }
                    if (!hasFocus && !contentFocused) backToContent()
                }
            }
            else -> Unit
        }
    }

    /** Back to the page: the element that had focus there, or the page's first. */
    fun backToContent() {
        runCatching { if (!content.restoreFocusedChild()) content.requestFocus() }
    }
}

/**
 * The rail (design spec §2.1): 40 dp entries collapsed, 40 tall on a
 * 44 dp pitch. Collapsed, the icons are in the middle of the 56 dp the
 * pages keep dark under it ([PleyTv.RailFadeSolidUntil]), 16 dp from the
 * screen's edge and from the page's. Open, the panel is as wide as its
 * longest label needs, 240 dp at the least and 320 at the most
 * ([openPillWidth]), and an entry is 8 dp from the screen's edge and
 * from the panel's. The avatar at the top switches profiles; then [items],
 * from y = 67; Settings at the foot, at y = 473. The entries between them
 * scroll when they don't all fit ([railScroll]), drawn only between the
 * avatar and Settings; Up and Down go through them in order, whatever is
 * scrolled out of view. Collapsed, the current entry has a 4 by 20 accent
 * mark and its icon in [fg][PleyColors.Fg]; open, it is a
 * [surface3][PleyColors.Surface3] pill. The focused entry is the accent
 * pill at 1.05, grown from its icon, so the icons stay in their column,
 * and drawn that much narrower, so it is as wide as the current entry's
 * pill and ends as far from the panel's edge as it starts from the
 * screen's. Open, the rail is [surface][PleyColors.Surface] at 96 % with
 * a soft edge, and the shell dims the page behind it.
 *
 * Focus in it is what opens it. Over 200 ms ([PleyMotion.OVERLAY_MS]),
 * eased out as the Samsung app's rail is ([PleyMotion.EaseOut]), the
 * panel widens from 72 dp to its open width with its edge, the pills from 40 dp to theirs,
 * and the labels fade in as they slide 24 dp into place; leaving, the
 * same back, the labels fading where they are, and the panel fading over
 * its last 15 % into the dark under the closed entries. With
 * reduced motion (Settings' or the system's) it opens and closes in place,
 * and an entry's scale changes at once. What moves is drawn, in the rail's
 * own layer: the rail is laid out open or closed as focus comes and goes,
 * once each way (closed, it sits left of the page, where Left looks for
 * it), and nothing of the page behind is laid out or drawn again.
 */
@Composable
internal fun TvRail(
    items: List<RailItem>,
    profile: RailProfile?,
    rail: RailState,
    onSelect: (RailItem) -> Unit,
    onProfile: () -> Unit,
    offline: Boolean = false,
) {
    val open = rail.hasFocus
    val opening = rememberRailOpening(open)
    val pill = openPillWidth(items, profile)
    val panel = pill + RailInset * 2
    rail.scope = rememberCoroutineScope()
    val focus = remember { RailFocus() }
    val offlineSaid = stringResource(R.string.tv_shell_offline)
    Box(
        Modifier
            .fillMaxHeight()
            // Its own layer: each frame of the opening redraws the rail, not the page under it.
            .graphicsLayer()
            .drawBehind {
                val shown = opening.value
                // Closed, the page's own fade is under the entries: nothing to paint.
                if (shown <= 0f) return@drawBehind
                // Past the rail's bounds while it closes: it is laid out closed by then.
                val width = lerp(PleyTv.RailCollapsed.toPx(), panel.toPx(), shown)
                // It slides at full strength and fades over the last of the way: closed, it meets the page's fade under the entries.
                val strength = (shown / RAIL_FADE).coerceAtMost(1f)
                drawRect(PleyColors.Surface.copy(alpha = 0.96f * strength), size = Size(width, size.height))
                // A stretched picture rather than a gradient shader (GradientBitmaps): cheaper on TV GPUs.
                drawStretched(RailEdge, Offset(width, 0f), Size(RailShadow.toPx(), size.height), alpha = shown)
            },
    ) {
        Column(
            Modifier
                .fillMaxHeight()
                .width(if (open) panel else PleyTv.RailCollapsed)
                .testTag(TestTags.TV_RAIL)
                .onFocusChanged { rail.focusChanged(it.hasFocus) }
                .focusProperties { onEnter = { with(rail) { enter() } } }
                .focusGroup()
                // The open rail covers the page's left: Right goes back to where focus was there.
                .onPreviewKeyEvent { event ->
                    if (event.key == Key.DirectionRight && event.type == KeyEventType.KeyDown) {
                        rail.backToContent()
                        true
                    } else {
                        false
                    }
                }
                .padding(start = RailInset, end = RailInset, top = AvatarItemTop, bottom = TvSafeArea.Vertical),
        ) {
            RailPill(
                selected = rail.current == PROFILE_ITEM,
                open = open,
                opening = opening,
                openWidth = pill,
                onClick = onProfile,
                leadingSize = AvatarSize,
                leadingContent = { Avatar(profile, offline) },
                // The name over "Switch profile", set close so both lines fit the entry.
                supportingContent = { ink ->
                    BasicText(
                        stringResource(R.string.tv_shell_switch_profile),
                        style = PleyType.Tv.Caption.copy(color = if (ink == PleyColors.OnAccent) ink else PleyColors.Muted, lineHeight = 15.sp),
                        maxLines = 1,
                    )
                },
                // Room for its two lines when open; the entries below keep their 44 dp pitch from y = 67.
                height = AvatarItemHeight,
                modifier = Modifier
                    .focusRequester(rail.requester(PROFILE_ITEM))
                    .focusProperties { down = rail.requester(items.firstOrNull()?.id ?: SettingsItem.id) }
                    .onFocusChanged { if (it.isFocused) focus.at = RailFocus.NO_ENTRY }
                    .semantics { if (offline) stateDescription = offlineSaid },
            ) { ink ->
                BasicText(profile?.name.orEmpty(), style = PleyType.Tv.Label.copy(color = ink, lineHeight = 17.sp), maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
            Spacer(Modifier.height(AvatarGap))
            RailEntries(items, rail, open, opening, pill, focus, onSelect, Modifier.weight(1f))
            Spacer(Modifier.height(ItemGap))
            RailEntry(
                SettingsItem,
                rail,
                open,
                opening,
                pill,
                onSelect,
                up = items.lastOrNull()?.id ?: PROFILE_ITEM,
                onFocused = { focus.at = RailFocus.NO_ENTRY },
            )
        }
    }
}

/**
 * Where focus is in the rail: an entry's index, or [NO_ENTRY] on the
 * avatar or Settings; `null` while it is outside the rail.
 */
private class RailFocus {
    var at: Int? = null

    companion object {
        const val NO_ENTRY = -1
    }
}

/**
 * The entries between the avatar and Settings, scrolled by [railScroll]
 * as focus moves through them, over a row's glide (at once with reduced
 * motion). While focus is outside the rail, the page's entry is placed by
 * the same rule, so opening the rail on it moves nothing; rebuilt entries
 * and a new size place the one that counts by it too. Entries scrolled past
 * either end are cut at the gaps beside the avatar and Settings, never
 * drawn over them; across, nothing is cut, so a pill's growth and its
 * opening width show whole.
 */
@Composable
private fun RailEntries(
    items: List<RailItem>,
    rail: RailState,
    open: Boolean,
    opening: Animatable<Float, AnimationVector1D>,
    openWidth: Dp,
    focus: RailFocus,
    onSelect: (RailItem) -> Unit,
    modifier: Modifier,
) {
    val reduced = reducedMotion()
    val scope = rememberCoroutineScope()
    val density = LocalDensity.current
    // How far up the entries are moved, in dp.
    val scroll = remember { Animatable(0f) }
    var viewport by remember { mutableFloatStateOf(0f) }
    fun show(index: Int) {
        val target = railScroll(scroll.targetValue, index, items.size, viewport)
        if (target == scroll.targetValue) return
        scope.launch {
            if (reduced) scroll.snapTo(target) else scroll.animateTo(target, tween(PleyMotion.FOCUS_MS, easing = PleyMotion.EaseOut))
        }
    }
    val current = rail.current
    LaunchedEffect(items, viewport, current, open) {
        if (!open) focus.at = null
        // The focused entry, else the page's; on the avatar or Settings, none.
        val index = when (val at = focus.at) {
            null -> items.indexOfFirst { it.id == current }
            RailFocus.NO_ENTRY -> RailFocus.NO_ENTRY
            else -> minOf(at, items.lastIndex)
        }
        show(index)
    }
    Box(
        modifier
            .fillMaxWidth()
            .onSizeChanged { viewport = with(density) { it.height.toDp().value } }
            .drawWithContent {
                val gap = ItemGap.toPx()
                val across = (OpenPillMost + RailInset * 2).toPx()
                clipRect(left = -across, top = -gap, right = size.width + across, bottom = size.height + gap) {
                    this@drawWithContent.drawContent()
                }
            },
    ) {
        Column(
            Modifier
                .wrapContentHeight(Alignment.Top, unbounded = true)
                .graphicsLayer { translationY = -scroll.value.dp.toPx() },
        ) {
            items.forEachIndexed { i, item ->
                if (i > 0) Spacer(Modifier.height(ItemGap))
                RailEntry(
                    item,
                    rail,
                    open,
                    opening,
                    openWidth,
                    onSelect,
                    up = items.getOrNull(i - 1)?.id ?: PROFILE_ITEM,
                    down = items.getOrNull(i + 1)?.id ?: SettingsItem.id,
                    onFocused = {
                        focus.at = i
                        show(i)
                    },
                )
            }
        }
    }
}

/**
 * How far up, in dp, the entries between the avatar and Settings are
 * moved with entry [focused] of [count] focused, in a [viewport] dp tall:
 * by whole entries, so none is ever cut in half at either end. Going down,
 * focus moves until it is the fourth entry shown ([RAIL_PIVOT]), then stays
 * there while the entries move under it an entry a press, until the last
 * ones are all in view and focus goes on to the last; going up is the same
 * backwards. Where an entry shows depends on it alone, never on the way
 * focus came, as the TV's lists keep their focused row about a third of
 * the way down ([TvScrolling.root][app.pley.android.ui.tv.kit.TvScrolling.root]).
 * Clamped at the ends: the first entry at the top (y = 67); at the other,
 * the last nine whole, the last ending at y = 459, above Settings. An
 * index that isn't an entry's (the avatar, Settings) keeps [scroll],
 * within the ends.
 */
internal fun railScroll(scroll: Float, focused: Int, count: Int, viewport: Float): Float {
    if (viewport <= 0f) return scroll
    val pitch = EntryHeight.value + ItemGap.value
    // Whole entries in the view, the last one's gap aside: nine in the 402 dp between the avatar and Settings.
    val fits = ((viewport + ItemGap.value) / pitch).toInt()
    val end = (count - fits).coerceAtLeast(0) * pitch
    if (focused !in 0 until count) return scroll.coerceIn(0f, end)
    return ((focused - RAIL_PIVOT) * pitch).coerceIn(0f, end)
}

@Composable
private fun RailEntry(
    item: RailItem,
    rail: RailState,
    open: Boolean,
    opening: Animatable<Float, AnimationVector1D>,
    openWidth: Dp,
    onSelect: (RailItem) -> Unit,
    up: String? = null,
    down: String? = null,
    onFocused: () -> Unit = {},
) {
    val selected = rail.current == item.id
    val interactions = remember { MutableInteractionSource() }
    // Read whether or not a number shows, so one that comes on a focused entry has its focused colours.
    val focused by interactions.collectIsFocusedAsState()
    RailPill(
        selected = selected,
        open = open,
        opening = opening,
        openWidth = openWidth,
        onClick = { onSelect(item) },
        leadingContent = { ink ->
            Box {
                Icon(item.icon, contentDescription = null, size = PleyTv.IconSize, tint = ink)
                val requests = item.requests
                if (requests != null && requests.badge > 0) RailCount(requests, focused)
            }
        },
        interactions = interactions,
        modifier = Modifier
            .focusRequester(rail.requester(item.id))
            // In order, not by where entries are drawn: one scrolled away lies under the avatar or Settings.
            .focusProperties {
                if (up != null) this.up = rail.requester(up)
                if (down != null) this.down = rail.requester(down)
            }
            .onFocusChanged { if (it.isFocused) onFocused() }
            .testTag("${TestTags.TV_RAIL}.${item.id}")
            .then(if (selected && !open) Modifier.currentMark() else Modifier),
    ) { ink ->
        BasicText(label(item), style = PleyType.Tv.Label.copy(color = ink), maxLines = 1, overflow = TextOverflow.Ellipsis)
    }
}

/**
 * How far the rail is open, 0 to 1, following [open] over 200 ms, at
 * once with reduced motion. Read only while drawing and in layers.
 */
@Composable
private fun rememberRailOpening(open: Boolean): Animatable<Float, AnimationVector1D> {
    val reduced = reducedMotion()
    val opening = remember { Animatable(if (open) 1f else 0f) }
    LaunchedEffect(open, reduced) {
        val target = if (open) 1f else 0f
        // Both ways on the Samsung app's ease out, as its rail moves.
        if (reduced) opening.snapTo(target) else opening.animateTo(target, tween(PleyMotion.OVERLAY_MS, easing = PleyMotion.EaseOut))
    }
    return opening
}

/**
 * One of the rail's entries: a pill drawn as wide as the rail is open
 * ([opening]: 40 dp closed, [openWidth] open), [height] tall, and laid out at
 * either end ([open]). [leadingContent] (an icon, the avatar) is centred
 * on the closed pill; [content] (the label), with [supportingContent]
 * under it and [trailingContent] at the end, fades in as the pill widens,
 * sliding in as the rail opens, cut at its edge. Each is given the
 * colour to draw in.
 *
 * Entries are in [muted][PleyColors.Muted], the current one ([selected])
 * in fg, on the [surface3][PleyColors.Surface3] pill while [open]. Focused,
 * the pill is the accent fill with [onAccent][PleyColors.OnAccent]
 * content, and grows to 1.05 in its layer, about its icon (in place with
 * reduced motion); pressed, accentBright. No border and no glow: focus is
 * the fill.
 */
@Composable
private fun RailPill(
    selected: Boolean,
    open: Boolean,
    opening: Animatable<Float, AnimationVector1D>,
    openWidth: Dp,
    onClick: () -> Unit,
    leadingContent: @Composable (ink: Color) -> Unit,
    modifier: Modifier = Modifier,
    leadingSize: Dp = LeadingSize,
    height: Dp = EntryHeight,
    supportingContent: (@Composable (ink: Color) -> Unit)? = null,
    trailingContent: (@Composable (ink: Color) -> Unit)? = null,
    interactions: MutableInteractionSource = remember { MutableInteractionSource() },
    content: @Composable (ink: Color) -> Unit,
) {
    val focused by interactions.collectIsFocusedAsState()
    val pressed by interactions.collectIsPressedAsState()
    val reduced = reducedMotion()
    val scale = remember { Animatable(1f) }
    LaunchedEffect(focused, reduced) {
        val target = if (focused) PleyFocus.PILL_SCALE else 1f
        when {
            reduced -> scale.snapTo(target)
            // 150 ms eased out to 1.05, 120 eased in back, as the Samsung app's entries.
            focused -> scale.animateTo(target, tween(PleyMotion.FOCUS_MS, easing = PleyMotion.EaseOut))
            else -> scale.animateTo(target, tween(PleyMotion.UNFOCUS_MS, easing = PleyMotion.EaseIn))
        }
    }
    val fill = when {
        pressed -> PleyColors.AccentBright
        focused -> PleyColors.Accent
        selected && open -> PleyColors.Surface3
        else -> Color.Transparent
    }
    val ink = when {
        pressed || focused -> PleyColors.OnAccent
        selected -> PleyColors.Fg
        else -> PleyColors.Muted
    }
    Box(
        modifier
            // Its focus area is where it shows at rest; the widening between is drawn.
            .width(if (!open) ClosedPillWidth else if (focused) focusedPillWidth(openWidth) else openWidth)
            .height(height)
            .graphicsLayer {
                scaleX = scale.value
                scaleY = scale.value
                // About its icon, which stays in the rail's column: the pill grows to the right, into the panel.
                transformOrigin = TransformOrigin(PillPivot.toPx() / size.width, 0.5f)
            }
            .drawWithContent {
                // Past its bounds while the rail closes.
                val width = pillWidth(opening.value, scale.value, openWidth)
                if (fill.alpha > 0f) {
                    val radius = PleyShape.ControlRadius.toPx()
                    drawRoundRect(fill, size = Size(width, size.height), cornerRadius = CornerRadius(radius, radius))
                }
                clipRect(right = width) { this@drawWithContent.drawContent() }
            }
            .semantics { this.selected = selected }
            .clickable(interactionSource = interactions, indication = null, role = Role.Button, onClick = onClick),
    ) {
        Box(
            Modifier
                .padding(start = (ClosedPillWidth - leadingSize) / 2)
                .size(leadingSize)
                .align(Alignment.CenterStart),
            contentAlignment = Alignment.Center,
        ) {
            leadingContent(ink)
        }
        Row(
            Modifier
                .align(Alignment.CenterStart)
                // Laid out open, the rail closed too: opening it only draws them.
                .wrapContentWidth(Alignment.Start, unbounded = true)
                .width(openWidth)
                .padding(start = LabelStart, end = LabelEnd)
                .graphicsLayer {
                    alpha = opening.value
                    // Opening, they slide in from a little to the left as they fade in; closing, they fade
                    // where they are, as the Samsung app's labels do.
                    translationX = if (open) -LabelSlide.toPx() * (1f - opening.value) else 0f
                    // Text doesn't overlap itself: faded as it draws, with no offscreen buffer.
                    compositingStrategy = CompositingStrategy.ModulateAlpha
                },
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Column(Modifier.weight(1f)) {
                content(ink)
                supportingContent?.invoke(ink)
            }
            trailingContent?.invoke(ink)
        }
    }
}

/**
 * A pill's width as drawn when the rail is [opening] that far open: 40 dp
 * closed to [openWidth] open. [grown] about its icon (its focus scale),
 * an open pill is drawn that much narrower, so that it ends as near the
 * panel's edge as it starts from the screen's, no nearer: the focused
 * entry's pill and the current one's are then as wide as each other.
 */
private fun Density.pillWidth(opening: Float, grown: Float, openWidth: Dp): Float {
    val pivot = PillPivot.toPx()
    val open = pivot + (openWidth.toPx() + pivot * (grown - 2f)) / grown
    return lerp(ClosedPillWidth.toPx(), open, opening)
}

/** An open pill with focus, as laid out: what [pillWidth] draws of it once it has grown. */
private fun focusedPillWidth(openWidth: Dp): Dp = PillPivot + (openWidth + PillPivot * (PleyFocus.PILL_SCALE - 2f)) / PleyFocus.PILL_SCALE

/**
 * An open pill's width: room for the longest of the labels (the entries',
 * the profile's name, "Switch profile") after the icon, 224 dp at the
 * least and 304 at the most, where a longer label is cut. So the panel is
 * narrow where the names are short, and widens for a library's long name.
 */
@Composable
private fun openPillWidth(items: List<RailItem>, profile: RailProfile?): Dp {
    val measurer = rememberTextMeasurer()
    val density = LocalDensity.current
    val labels = items.map { label(it) } + label(SettingsItem) + profile?.name.orEmpty()
    val under = stringResource(R.string.tv_shell_switch_profile)
    return remember(labels, under, measurer, density) {
        val widest = maxOf(
            labels.maxOf { measurer.measure(it, PleyType.Tv.Label, maxLines = 1).size.width },
            measurer.measure(under, PleyType.Tv.Caption, maxLines = 1).size.width,
        )
        with(density) { (LabelStart + widest.toDp() + LabelEnd).coerceIn(OpenPillLeast, OpenPillMost) }
    }
}

@Composable
private fun label(item: RailItem): String = when (item.slot) {
    RailSlot.Search -> stringResource(R.string.tv_shell_rail_search)
    RailSlot.Home -> stringResource(R.string.tv_shell_rail_home)
    RailSlot.Discover -> stringResource(ScreensR.string.requests_discover)
    RailSlot.Requests -> stringResource(if (item.requests?.manage == true) ScreensR.string.requests_manage else ScreensR.string.requests_yours)
    RailSlot.LiveTv -> stringResource(R.string.tv_shell_rail_live_tv)
    RailSlot.Library -> item.name.orEmpty()
    RailSlot.Libraries -> stringResource(R.string.tv_shell_rail_libraries)
    RailSlot.Playlists -> stringResource(R.string.tv_shell_rail_playlists)
    RailSlot.Collections -> stringResource(R.string.tv_shell_rail_collections)
    RailSlot.NowPlaying -> stringResource(R.string.tv_shell_rail_now_playing)
    RailSlot.Drive -> stringResource(R.string.tv_shell_rail_drive)
    RailSlot.Settings -> stringResource(R.string.tv_shell_rail_settings)
}

/**
 * The number on the way into requests ([RequestsEntry.badge]), as the
 * Samsung and LG apps draw it: astride the icon's upper right, ringed in
 * the page's colour, "99+" past 99; in the accent (its ink while the entry
 * has focus), or the danger colour while failed requests wait for a
 * manager. Screen readers hear it in words.
 */
@Composable
private fun BoxScope.RailCount(entry: RequestsEntry, focused: Boolean) {
    val texts = rememberRequestTexts()
    val said = remember(entry, texts) { texts.count(entry) }
    val fill = when {
        entry.alarm -> PleyColors.Danger
        focused -> PleyColors.OnAccent
        else -> PleyColors.Accent
    }
    val ink = when {
        entry.alarm -> PleyColors.Bg
        focused -> PleyColors.AccentBright
        else -> PleyColors.OnAccent
    }
    Box(
        Modifier
            .align(Alignment.TopStart)
            .offset(x = CountX, y = CountY)
            .wrapContentSize(Alignment.TopStart, unbounded = true)
            .background(PleyColors.Bg, PleyShape.Circle)
            .padding(CountRing)
            .heightIn(min = CountHeight)
            .widthIn(min = CountHeight)
            .background(fill, PleyShape.Circle)
            .padding(horizontal = 5.dp)
            .clearAndSetSemantics { contentDescription = said },
        contentAlignment = Alignment.Center,
    ) {
        BasicText(if (entry.badge > MAX_COUNT) "$MAX_COUNT+" else entry.badge.toString(), style = PleyType.Tv.Badge.copy(color = ink), maxLines = 1)
    }
}

/** The profile's 32 dp circle: its picture, or its initial on its colour; marked while [offline]. */
@Composable
private fun Avatar(profile: RailProfile?, offline: Boolean) {
    val color = profile?.color ?: PleyColors.Surface3
    Box(Modifier.size(AvatarSize)) {
        Box(
            Modifier.size(AvatarSize).clip(PleyShape.Circle).background(color),
            contentAlignment = Alignment.Center,
        ) {
            val picture = profile?.picture
            if (picture != null) {
                Artwork(picture, profile.name, Modifier.size(AvatarSize))
            } else {
                val ink = if (color.luminance() > 0.45f) PleyColors.OnAccent else PleyColors.Fg
                BasicText(profile?.initial ?: "", style = PleyType.Tv.Label.copy(color = ink, fontWeight = PleyType.Bold))
            }
        }
        if (offline) OfflineMark()
    }
}

/** Collapsed, the current entry's 4 by 20 accent mark, 4 dp left of its pill and 8 from its icon (a state line, not focus). */
private fun Modifier.currentMark(): Modifier = drawBehind {
    val w = 4.dp.toPx()
    val h = 20.dp.toPx()
    drawRoundRect(PleyColors.Accent, Offset(-w, (size.height - h) / 2f), Size(w, h), CornerRadius(w / 2f, w / 2f))
}

/**
 * The avatar's entry: 48 tall from y = 15, so its 32 dp circle sits at
 * y = 23 and, 4 below it, Search starts at y = 67 (design spec §2.1).
 */
private val AvatarItemTop = 15.dp
private val AvatarItemHeight = 48.dp
private val AvatarGap = 4.dp
private val AvatarSize = 32.dp

/**
 * 40 dp entries, as tall as the library's chips, on a 44 dp pitch: nine
 * fit between the avatar and Settings, which is 40 tall at y = 473, 27
 * from the bottom.
 */
private val EntryHeight = 40.dp
private val ItemGap = 4.dp

/** The entries shown above the focused one while the entries scroll under it ([railScroll]). */
private const val RAIL_PIVOT = 3

/** The number's 18 dp pill, ringed 2 dp, its corner 9 right of the icon's and 10 above it. */
private val CountHeight = 18.dp
private val CountRing = 2.dp
private val CountX = 9.dp
private val CountY = (-10).dp

/** What the number counts to before it says "99+". */
private const val MAX_COUNT = 99

/** From the rail's edges to its pills: the screen's edge, and the open panel's. */
private val RailInset = 8.dp

/**
 * A closed pill: its icon's middle is at x = 28, the middle of the dark
 * the pages keep under the rail. Open, the label starts 16 dp after the
 * icon and stops 16 dp short of the pill's end, room for it to grow with
 * focus.
 */
private val ClosedPillWidth = 40.dp
private val LabelStart = 56.dp
private val LabelEnd = 16.dp

/** An open pill at its narrowest and its widest ([openPillWidth]): a panel of 240 dp and of 320. */
private val OpenPillLeast = 224.dp
private val OpenPillMost = 304.dp

/** The icon's middle in a pill: what a focused pill grows about. */
private val PillPivot = ClosedPillWidth / 2

/** How far the labels slide in from as the rail opens: a little, so they don't cross the icons. */
private val LabelSlide = 24.dp

/** The box an entry's icon is centred in. */
private val LeadingSize = 24.dp

/** The open rail's soft right edge. */
private val RailShadow = 64.dp

/** The last of the panel's way, as a part of it, over which it fades into the closed rail's dark: 30 dp. */
private const val RAIL_FADE = 0.15f

/** Frames a key from nowhere waits before it takes focus to the page: past a row's own handing on ([RailState.enter]). */
private const val SETTLE_FRAMES = 3

/** That edge: surface at 85 % to clear. */
private val RailEdge by lazy { GradientBitmaps.horizontal(0f to PleyColors.Surface.copy(alpha = 0.85f), 1f to PleyColors.Surface.copy(alpha = 0f)) }
