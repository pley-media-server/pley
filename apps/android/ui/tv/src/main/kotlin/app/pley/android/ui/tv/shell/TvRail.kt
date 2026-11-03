package app.pley.android.ui.tv.shell

import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.AnimationVector1D
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.focusGroup
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsFocusedAsState
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.wrapContentWidth
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
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
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
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
import app.pley.android.ui.tv.R
import app.pley.android.ui.tv.kit.TvRailFocus

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
 */
@Stable
class RailState(private val content: FocusRequester = FocusRequester()) : TvRailFocus {
    private val requesters = HashMap<String, FocusRequester>()

    /** The current entry's id; `null` while none (sign-in, Who's watching?). */
    var current: String? = null

    /** Whether focus is in the rail, which is what opens it. */
    var hasFocus by mutableStateOf(false)
        private set

    /** Set when focus comes into the rail, cleared when another page shows ([pageChanged]). */
    private var sentFromPage = false

    override val keepsFocus: Boolean get() = hasFocus && sentFromPage

    /** Whether the rail shows at all: not on the sign-in screens. */
    var shown by mutableStateOf(false)
        internal set

    /** Set while [focusCurrent] sends focus to an entry: the one way in ([enter]). */
    private var sending = false

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
     */
    internal fun FocusEnterExitScope.enter() {
        if (sending) return
        cancelFocusChange()
        if (requestedFocusDirection == FocusDirection.Left) focusCurrent()
    }

    /** Back to the page: the element that had focus there, or the page's first. */
    fun backToContent() {
        runCatching { if (!content.restoreFocusedChild()) content.requestFocus() }
    }
}

/**
 * The rail (design spec §2.1): 56 dp entries collapsed, 256 open. The
 * avatar at the top switches profiles; then [items]; Settings at the foot,
 * at y = 465. Collapsed, the current entry has a 4 by 20 accent mark and
 * its icon in [fg][PleyColors.Fg]; open, it is a [surface3][PleyColors.Surface3]
 * pill. The focused entry is the accent pill at 1.05. Open, the rail is
 * [surface][PleyColors.Surface] at 96 % with a soft edge, and the shell
 * dims the page behind it.
 *
 * Focus in it is what opens it. Over 200 ms ([PleyMotion.OVERLAY_MS]) the
 * panel widens from 72 to 272 dp with its edge, the pills from 56 to 256,
 * and the labels fade in as they widen; leaving, the same back. With
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
                val panel = lerp(PleyTv.RailCollapsed.toPx(), PleyTv.RailExpanded.toPx(), shown)
                drawRect(PleyColors.Surface.copy(alpha = 0.96f), size = Size(panel, size.height))
                // A stretched picture rather than a gradient shader (GradientBitmaps): cheaper on TV GPUs.
                drawStretched(RailEdge, Offset(panel, 0f), Size(RailShadow.toPx(), size.height), alpha = shown)
            },
    ) {
        Column(
            Modifier
                .fillMaxHeight()
                .width(if (open) PleyTv.RailExpanded else PleyTv.RailCollapsed)
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
                .padding(start = 8.dp, end = 8.dp, top = AvatarItemTop, bottom = TvSafeArea.Vertical),
        ) {
            RailPill(
                selected = rail.current == PROFILE_ITEM,
                open = open,
                opening = opening,
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
                // Room for its two lines when open; the entries below keep their 56 dp pitch from y = 75.
                height = AvatarItemHeight,
                modifier = Modifier
                    .focusRequester(rail.requester(PROFILE_ITEM))
                    .semantics { if (offline) stateDescription = offlineSaid },
            ) { ink ->
                BasicText(profile?.name.orEmpty(), style = PleyType.Tv.Label.copy(color = ink, lineHeight = 17.sp), maxLines = 1)
            }
            Spacer(Modifier.height(AvatarGap))
            items.forEachIndexed { i, item ->
                if (i > 0) Spacer(Modifier.height(ItemGap))
                RailEntry(item, rail, open, opening, onSelect)
            }
            Spacer(Modifier.weight(1f))
            RailEntry(SettingsItem, rail, open, opening, onSelect)
        }
    }
}

@Composable
private fun RailEntry(
    item: RailItem,
    rail: RailState,
    open: Boolean,
    opening: Animatable<Float, AnimationVector1D>,
    onSelect: (RailItem) -> Unit,
) {
    val selected = rail.current == item.id
    RailPill(
        selected = selected,
        open = open,
        opening = opening,
        onClick = { onSelect(item) },
        leadingContent = { ink -> Icon(item.icon, contentDescription = null, size = PleyTv.IconSize, tint = ink) },
        modifier = Modifier
            .focusRequester(rail.requester(item.id))
            .testTag("${TestTags.TV_RAIL}.${item.id}")
            .then(if (selected && !open) Modifier.currentMark() else Modifier),
    ) { ink ->
        BasicText(label(item), style = PleyType.Tv.Label.copy(color = ink), maxLines = 1)
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
        if (reduced) opening.snapTo(target) else opening.animateTo(target, PleyMotion.overlay())
    }
    return opening
}

/**
 * One of the rail's entries: a pill drawn as wide as the rail is open
 * ([opening]: 56 dp closed, 256 open), [height] tall, and laid out at
 * either end ([open]). [leadingContent] (an icon, the avatar) is centred
 * on the closed pill; [content] (the label), with [supportingContent]
 * under it and [trailingContent] at the end, fades in as the pill widens,
 * cut at its edge. Each is given the colour to draw in.
 *
 * Entries are in [muted][PleyColors.Muted], the current one ([selected])
 * in fg, on the [surface3][PleyColors.Surface3] pill while [open]. Focused,
 * the pill is the accent fill with [onAccent][PleyColors.OnAccent]
 * content, and grows to 1.05 in its layer (in place with reduced motion);
 * pressed, accentBright. No border and no glow: focus is the fill.
 */
@Composable
private fun RailPill(
    selected: Boolean,
    open: Boolean,
    opening: Animatable<Float, AnimationVector1D>,
    onClick: () -> Unit,
    leadingContent: @Composable (ink: Color) -> Unit,
    modifier: Modifier = Modifier,
    leadingSize: Dp = LeadingSize,
    height: Dp = EntryHeight,
    supportingContent: (@Composable (ink: Color) -> Unit)? = null,
    trailingContent: (@Composable (ink: Color) -> Unit)? = null,
    content: @Composable (ink: Color) -> Unit,
) {
    val interactions = remember { MutableInteractionSource() }
    val focused by interactions.collectIsFocusedAsState()
    val pressed by interactions.collectIsPressedAsState()
    val reduced = reducedMotion()
    val scale = remember { Animatable(1f) }
    LaunchedEffect(focused, reduced) {
        if (focused) scale.animateTo(PleyFocus.PILL_SCALE, PleyMotion.focus(reduced)) else scale.animateTo(1f, PleyMotion.unfocus(reduced))
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
            .width(if (open) OpenPillWidth else ClosedPillWidth)
            .height(height)
            .graphicsLayer {
                scaleX = scale.value
                scaleY = scale.value
                // About the pill's middle, as wide as it shows.
                transformOrigin = TransformOrigin(pillWidth(opening.value) / 2f / size.width, 0.5f)
            }
            .drawWithContent {
                // Past its bounds while the rail closes.
                val width = pillWidth(opening.value)
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
                .width(OpenPillWidth)
                .padding(start = ClosedPillWidth, end = LabelEnd)
                .graphicsLayer {
                    alpha = opening.value
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

/** A pill's width when the rail is [opening] that far open: 56 dp closed to 256 open. */
private fun Density.pillWidth(opening: Float): Float = lerp(ClosedPillWidth.toPx(), OpenPillWidth.toPx(), opening)

@Composable
private fun label(item: RailItem): String = when (item.slot) {
    RailSlot.Search -> stringResource(R.string.tv_shell_rail_search)
    RailSlot.Home -> stringResource(R.string.tv_shell_rail_home)
    RailSlot.LiveTv -> stringResource(R.string.tv_shell_rail_live_tv)
    RailSlot.Library -> item.name.orEmpty()
    RailSlot.Libraries -> stringResource(R.string.tv_shell_rail_libraries)
    RailSlot.Playlists -> stringResource(R.string.tv_shell_rail_playlists)
    RailSlot.Collections -> stringResource(R.string.tv_shell_rail_collections)
    RailSlot.NowPlaying -> stringResource(R.string.tv_shell_rail_now_playing)
    RailSlot.Settings -> stringResource(R.string.tv_shell_rail_settings)
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

/** Collapsed, the current entry's 4 by 20 accent mark at its left edge (a state line, not focus). */
private fun Modifier.currentMark(): Modifier = drawBehind {
    val w = 4.dp.toPx()
    val h = 20.dp.toPx()
    drawRoundRect(PleyColors.Accent, Offset(0f, (size.height - h) / 2f), Size(w, h), CornerRadius(w / 2f, w / 2f))
}

/**
 * The avatar's entry: 56 tall from y = 15, so its 32 dp circle sits at
 * y = 27 and, 4 below it, Search starts at y = 75 (design spec §2.1).
 */
private val AvatarItemTop = 15.dp
private val AvatarItemHeight = 56.dp
private val AvatarGap = 4.dp
private val AvatarSize = 32.dp

/** 48 dp entries on a 56 dp pitch. */
private val EntryHeight = 48.dp
private val ItemGap = 8.dp

/** A pill closed and open; the label starts where the closed pill ends, and stops 12 dp short of the open one's end. */
private val ClosedPillWidth = 56.dp
private val OpenPillWidth = 256.dp
private val LabelEnd = 12.dp

/** The box an entry's icon is centred in. */
private val LeadingSize = 24.dp

/** The open rail's soft right edge. */
private val RailShadow = 64.dp

/** That edge: surface at 85 % to clear. */
private val RailEdge by lazy { GradientBitmaps.horizontal(0f to PleyColors.Surface.copy(alpha = 0.85f), 1f to PleyColors.Surface.copy(alpha = 0f)) }
