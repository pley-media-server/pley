package app.pley.android.designsystem.tile

import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.PleyTv
import app.pley.android.designsystem.PleyType

/**
 * The tiles pley draws, with their TV sizes on the 960 by 540 canvas
 * (design spec §2.3) and the phone's minimum widths (§3.2). Each is also a
 * glow size class ([tileGlow]).
 */
enum class TileKind(
    /** The well on TV. */
    val tvWidth: Dp,
    val tvHeight: Dp,
    /** The narrowest a phone card gets; it grows up to [PleyPhone.MAX_CARD_GROWTH][app.pley.android.designsystem.PleyPhone.MAX_CARD_GROWTH]× on wide screens. */
    val phoneMinWidth: Dp,
    /** People and profiles are circles. */
    val round: Boolean = false,
) {
    /** Movies, shows, seasons: 7 in a TV row, which they fill from the gutter to 10 dp short of the screen's edge. */
    Poster(110.dp, 165.dp, 118.dp),

    /** Continue Watching and extras, 16:9: 3 and most of a 4th. */
    Landscape(204.dp, 115.dp, 236.dp),

    /** Episode rows, 16:9: 4 and a peek. The phone's episode list uses 128 by 72. */
    Episode(184.dp, 104.dp, 128.dp),

    /** The library grid's posters: 7 columns. */
    LibraryGrid(96.dp, 144.dp, 118.dp),

    /** Albums. */
    Square(132.dp, 132.dp, 132.dp),

    /** Cast, a circle: 7 and most of an 8th in a row. */
    Person(96.dp, 96.dp, 96.dp, round = true),

    /** Profiles on "Who's watching?", a circle. */
    Avatar(100.dp, 100.dp, 88.dp, round = true),
    ;

    /** Width over height. */
    val aspectRatio: Float get() = tvWidth / tvHeight

    /** The gap between tiles of this kind in a TV row: wide tiles grow more when focused. */
    val tvGap: Dp get() = if (this == Landscape || this == Episode) PleyTv.WideTileGap else PleyTv.TileGap
}

/** This kind's TV well size. */
fun Modifier.tvTileSize(kind: TileKind): Modifier = size(kind.tvWidth, kind.tvHeight)

/**
 * The sizes a tile's art and overlays take on TV and on phones (design
 * spec §2.3, §3.2): corner radius, the watched tick, badges, the
 * placeholder's title.
 */
enum class TileStyle(
    val cornerRadius: Dp,
    /** The watched tick's circle, top right. */
    val tickSize: Dp,
    /** The tick glyph inside it. */
    val tickGlyph: Dp,
    /** The unwatched count pill's height. */
    val countHeight: Dp,
    /** From the art's edge to the tick, count and badges. */
    val inset: Dp,
    /** The progress bar along the bottom. */
    val progressHeight: Dp,
    /** Quality badges on art. */
    val badgeText: TextStyle,
    val badgeHeight: Dp,
    /** The placeholder's title. */
    val placeholderText: TextStyle,
    /** Caption lines under the art. */
    val captionTitle: TextStyle,
    val captionSubtitle: TextStyle,
    /** From the art to the caption. */
    val captionGap: Dp,
) {
    Tv(
        cornerRadius = PleyShape.TileTvRadius,
        tickSize = 20.dp,
        tickGlyph = 12.dp,
        countHeight = 18.dp,
        inset = 6.dp,
        progressHeight = 4.dp,
        badgeText = PleyType.Tv.Badge,
        badgeHeight = 18.dp,
        placeholderText = PleyType.Tv.Title,
        captionTitle = PleyType.Tv.Caption,
        captionSubtitle = PleyType.Tv.Caption,
        captionGap = PleyTv.TilesToCaption,
    ),
    Phone(
        cornerRadius = PleyShape.TilePhoneRadius,
        tickSize = 20.dp,
        tickGlyph = 12.dp,
        countHeight = 18.dp,
        inset = 6.dp,
        progressHeight = 4.dp,
        badgeText = PleyType.Phone.LabelSmall,
        badgeHeight = 18.dp,
        placeholderText = PleyType.Phone.TitleSmall,
        captionTitle = PleyType.Phone.TitleSmall,
        captionSubtitle = PleyType.Phone.BodySmall,
        captionGap = 8.dp,
    ),
    ;

    /** The well's shape for [kind]. */
    fun shape(kind: TileKind): Shape = if (kind.round) PleyShape.Circle else RoundedCornerShape(cornerRadius)
}
