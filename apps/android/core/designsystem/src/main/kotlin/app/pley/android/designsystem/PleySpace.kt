package app.pley.android.designsystem

import androidx.compose.ui.unit.dp

/** The 4 dp grid (design spec §1.3). */
object PleySpace {
    val Xxs = 2.dp
    val Xs = 4.dp
    val S = 8.dp
    val M = 12.dp
    val L = 16.dp
    val Xl = 20.dp
    val Xxl = 24.dp
    val X3 = 32.dp
    val X4 = 40.dp
    val X5 = 48.dp
    val X6 = 64.dp
}

/**
 * What a TV keeps clear around the picture: some TVs crop the edges
 * (overscan). 48 dp at the sides and 27 dp above and below, 5 % of the
 * 960 by 540 dp screen Fire TV and Google TV draw on. Text and focusable
 * things stay inside; backdrops and scrolling rows bleed past.
 */
object TvSafeArea {
    val Horizontal = 48.dp
    val Vertical = 27.dp
}

/**
 * The TV layout's fixed lines, on the 960 by 540 dp canvas (design spec
 * §1.3, §2.1-§2.4). Every page starts at [Gutter].
 */
object PleyTv {
    /** The canvas a 1080p TV draws on. */
    val CanvasWidth = 960.dp
    val CanvasHeight = 540.dp

    /** The collapsed rail: 8 + a 56 dp item + 8. */
    val RailCollapsed = 72.dp

    /** The expanded rail: 8 + a 256 dp item + 8. */
    val RailExpanded = 272.dp

    /** Every page's left edge: the rail and 24. Rows keep their focused tile here. */
    val Gutter = 96.dp

    /** The right edge of text over backdrops, where the scrim is still at least 85 % (the column is 464 wide). */
    val TextColumnEnd = 560.dp

    /** Where the page puts a focused row's header, under the info block. */
    val FocusedRowHeaderY = 172.dp

    /** A row header's line (tvSection). */
    val RowHeaderHeight = 24.dp

    /** From a row header to its tiles: room for the focused tile's growth and ring. */
    val HeaderToTiles = 14.dp

    /** From tiles to their caption: the same room below. */
    val TilesToCaption = 14.dp

    /** A caption's two lines of tvCaption. */
    val CaptionHeight = 34.dp

    /** Between one row's caption and the next row's header. */
    val RowGap = 28.dp

    /** Between posters, squares and people. */
    val TileGap = 14.dp

    /** Between landscape and episode tiles, which grow 8.2 dp a side when focused. */
    val WideTileGap = 18.dp

    /** Between buttons in a row; the row reserves [ButtonLabelRoom] under itself for round buttons' labels. */
    val ButtonGap = 10.dp
    val ButtonLabelRoom = 24.dp

    /** Pills, round buttons, chips and tabs. */
    val ButtonHeight = 36.dp

    /** The least height of a row in a list, a side panel or settings. */
    val ListRowHeight = 44.dp

    /** Icons in buttons, list rows and the rail. */
    val IconSize = 20.dp

    /** A scrubber's thumb; focused, it grows and takes [PleyFocus.ThumbRingOffset]'s ring, 18 dp across in all. */
    val ScrubThumb = 10.dp
    val ScrubThumbFocused = 12.dp

    /** The rows' fade under the collapsed rail: [Bg][PleyColors.Bg] to this x… */
    val RailFadeSolidUntil = 56.dp

    /** …and clear from this x. */
    val RailFadeClearFrom = 72.dp
}

/** The phone's layout values (design spec §1.3, §3.2). */
object PleyPhone {
    /** Page margins by window width class. */
    val MarginCompact = 16.dp
    val MarginMedium = 24.dp
    val MarginExpanded = 32.dp

    val TileGap = 12.dp
    val SectionGap = 28.dp
    val HeaderToContent = 10.dp

    /** From a card's art to its caption. */
    val ArtToCaption = 8.dp

    /** Cards grow up to this much over their minimum width on wide screens. */
    const val MAX_CARD_GROWTH = 1.35f
}
