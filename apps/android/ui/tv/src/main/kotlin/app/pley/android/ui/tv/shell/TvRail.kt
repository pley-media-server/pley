package app.pley.android.ui.tv.shell

import androidx.compose.foundation.background
import androidx.compose.foundation.focusGroup
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
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
import androidx.compose.ui.graphics.luminance
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.tv.material3.Border
import androidx.tv.material3.Glow
import androidx.tv.material3.LocalContentColor
import androidx.tv.material3.NavigationDrawerItem
import androidx.tv.material3.NavigationDrawerItemDefaults
import androidx.tv.material3.NavigationDrawerScope
import androidx.tv.material3.Text
import app.pley.android.common.TestTags
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyFocus
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.PleyTv
import app.pley.android.designsystem.PleyType
import app.pley.android.designsystem.TvSafeArea
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.image.Artwork
import app.pley.android.designsystem.image.GradientBitmaps
import app.pley.android.designsystem.image.drawStretched
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
 * The rail (design spec §2.1): tv-material's drawer items, 56 dp wide
 * collapsed and 256 open. The avatar at the top switches profiles; then
 * [items]; Settings at the foot, at y = 465. Collapsed, the current entry
 * has a 4 by 20 accent mark and its icon in [fg][PleyColors.Fg]; open, it
 * is a [surface3][PleyColors.Surface3] pill. The focused entry is the
 * accent pill at 1.05. Open, the rail is [surface][PleyColors.Surface] at
 * 96 % with a soft edge, and tv-material dims the page behind it.
 */
@Composable
internal fun NavigationDrawerScope.TvRail(
    items: List<RailItem>,
    profile: RailProfile?,
    rail: RailState,
    onSelect: (RailItem) -> Unit,
    onProfile: () -> Unit,
    offline: Boolean = false,
) {
    val open = hasFocus
    val offlineSaid = stringResource(R.string.tv_shell_offline)
    Box(
        Modifier
            .fillMaxHeight()
            .drawBehind {
                // Painted while the rail is wider than closed, closing too: the drawer narrows
                // after focus has left it, and the page showed through it, over its entries.
                if (!open && size.width <= ClosedWidth.toPx() + 1f) return@drawBehind
                val panel = if (open) size.width - RailShadow.toPx() else size.width
                drawRect(PleyColors.Surface.copy(alpha = 0.96f), size = Size(panel, size.height))
                // A stretched picture rather than a gradient shader (GradientBitmaps): cheaper on TV GPUs.
                drawStretched(RailEdge, Offset(panel, 0f), Size(RailShadow.toPx(), size.height))
            }
            .padding(end = if (open) RailShadow else 0.dp),
    ) {
        Column(
            Modifier
                .fillMaxHeight()
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
            NavigationDrawerItem(
                selected = rail.current == PROFILE_ITEM,
                onClick = onProfile,
                leadingContent = { Avatar(profile, offline) },
                // The name over "Switch profile", set close so both lines fit the entry.
                supportingContent = {
                    Text(
                        stringResource(R.string.tv_shell_switch_profile),
                        style = PleyType.Tv.Caption.copy(lineHeight = 15.sp),
                        maxLines = 1,
                    )
                },
                // Room for its two lines when open; the entries below keep their 56 dp pitch from y = 75.
                modifier = Modifier
                    .height(AvatarItemHeight)
                    .focusRequester(rail.requester(PROFILE_ITEM))
                    .semantics { if (offline) stateDescription = offlineSaid },
                colors = railColors(collapsedSelected = false),
                scale = railScale,
                shape = railShape,
                border = railBorder(),
                glow = railGlow,
            ) {
                Text(profile?.name.orEmpty(), style = PleyType.Tv.Label.copy(lineHeight = 17.sp), maxLines = 1)
            }
            Spacer(Modifier.height(AvatarGap))
            items.forEachIndexed { i, item ->
                if (i > 0) Spacer(Modifier.height(ItemGap))
                RailEntry(item, rail, onSelect)
            }
            Spacer(Modifier.weight(1f))
            RailEntry(SettingsItem, rail, onSelect)
        }
    }
}

@Composable
private fun NavigationDrawerScope.RailEntry(item: RailItem, rail: RailState, onSelect: (RailItem) -> Unit) {
    val selected = rail.current == item.id
    val open = hasFocus
    NavigationDrawerItem(
        selected = selected,
        onClick = { onSelect(item) },
        leadingContent = { Icon(item.icon, contentDescription = null, size = PleyTv.IconSize, tint = LocalContentColor.current) },
        modifier = Modifier
            .focusRequester(rail.requester(item.id))
            .testTag("${TestTags.TV_RAIL}.${item.id}")
            .then(if (selected && !open) Modifier.currentMark() else Modifier),
        colors = railColors(collapsedSelected = selected && !open),
        scale = railScale,
        shape = railShape,
        border = railBorder(),
        glow = railGlow,
    ) {
        Text(label(item), style = PleyType.Tv.Label, maxLines = 1)
    }
}

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
    RailSlot.Drive -> stringResource(R.string.tv_shell_rail_drive)
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
 * Entries in muted, the current one in fg; focus is the accent fill. A
 * collapsed rail shows the current entry without its pill (it has the
 * mark), an open one with the surface3 pill.
 */
@Composable
private fun railColors(collapsedSelected: Boolean) = NavigationDrawerItemDefaults.colors(
    containerColor = Color.Transparent,
    contentColor = PleyColors.Muted,
    inactiveContentColor = PleyColors.Muted,
    focusedContainerColor = PleyColors.Accent,
    focusedContentColor = PleyColors.OnAccent,
    pressedContainerColor = PleyColors.AccentBright,
    pressedContentColor = PleyColors.OnAccent,
    selectedContainerColor = if (collapsedSelected) Color.Transparent else PleyColors.Surface3,
    selectedContentColor = PleyColors.Fg,
    disabledContainerColor = Color.Transparent,
    disabledContentColor = PleyColors.Disabled,
    disabledInactiveContentColor = PleyColors.Disabled,
    focusedSelectedContainerColor = PleyColors.Accent,
    focusedSelectedContentColor = PleyColors.OnAccent,
    pressedSelectedContainerColor = PleyColors.AccentBright,
    pressedSelectedContentColor = PleyColors.OnAccent,
)

private val railScale = NavigationDrawerItemDefaults.scale(
    scale = 1f,
    focusedScale = PleyFocus.PILL_SCALE,
    pressedScale = 1f,
    selectedScale = 1f,
    disabledScale = 1f,
    focusedSelectedScale = PleyFocus.PILL_SCALE,
    focusedDisabledScale = 1f,
    pressedSelectedScale = 1f,
)

/** No borders: focus is the fill (tv-material's default is a 2 dp border). */
@Composable
private fun railBorder() = NavigationDrawerItemDefaults.border(
    border = Border.None,
    focusedBorder = Border.None,
    pressedBorder = Border.None,
    selectedBorder = Border.None,
    disabledBorder = Border.None,
    focusedSelectedBorder = Border.None,
    focusedDisabledBorder = Border.None,
    pressedSelectedBorder = Border.None,
)

/** Entries are rounded a little, as every control is ([PleyShape.Control]). */
private val railShape = NavigationDrawerItemDefaults.shape(PleyShape.Control)

private val railGlow = NavigationDrawerItemDefaults.glow(Glow.None, Glow.None, Glow.None, Glow.None, Glow.None, Glow.None)

/**
 * The avatar's entry: 56 tall from y = 15, so its 32 dp circle sits at
 * y = 27 and, 4 below it, Search starts at y = 75 (design spec §2.1).
 */
private val AvatarItemTop = 15.dp
private val AvatarItemHeight = 56.dp
private val AvatarGap = 4.dp
private val AvatarSize = 32.dp

/** 48 dp entries on a 56 dp pitch. */
private val ItemGap = 8.dp

/** The open rail's soft right edge. */
private val RailShadow = 64.dp

/** The closed rail: 56 dp entries, 8 dp either side. */
private val ClosedWidth = 72.dp

/** That edge: surface at 85 % to clear. */
private val RailEdge by lazy { GradientBitmaps.horizontal(0f to PleyColors.Surface.copy(alpha = 0.85f), 1f to PleyColors.Surface.copy(alpha = 0f)) }

