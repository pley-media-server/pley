package app.pley.android.designsystem

import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp

/**
 * Corner radii (design spec §1.4), and shapes made from them. Tight on
 * purpose: corners are softened, never rounded off. Nothing that holds a
 * label is a pill; only faces and dots are circles.
 */
object PleyShape {
    /** Badges, progress lines, skeleton text. */
    val BadgeRadius = 2.dp

    /** Phone tiles. */
    val TilePhoneRadius = 4.dp

    /** TV tiles. */
    val TileTvRadius = 4.dp

    /** Inputs. */
    val FieldRadius = 4.dp

    /** Buttons (with a label or an icon alone), chips, tabs, Skip Intro. */
    val ControlRadius = 6.dp

    /** The phone's Home cards, the link code fields, the Up Next card's image. */
    val CardRadius = 6.dp

    /** Dialogs, the Up Next card, TV side panels (left corners), the QR tile. */
    val DialogRadius = 8.dp

    /** Phone bottom sheets, top corners. */
    val SheetRadius = 12.dp

    val Badge = RoundedCornerShape(BadgeRadius)
    val TilePhone = RoundedCornerShape(TilePhoneRadius)
    val TileTv = RoundedCornerShape(TileTvRadius)
    val Field = RoundedCornerShape(FieldRadius)
    val Control = RoundedCornerShape(ControlRadius)

    /** [Control]'s old name, while screens written before it still use it. */
    val Pill = Control
    val Card = RoundedCornerShape(CardRadius)
    val Dialog = RoundedCornerShape(DialogRadius)
    val Sheet = RoundedCornerShape(topStart = SheetRadius, topEnd = SheetRadius)

    /** Avatars, people's photos, status dots. */
    val Circle = CircleShape
}

/**
 * The TV focus ring (design spec §1.4, §2.3): a 3 dp [PleyColors.AccentBright]
 * stroke whose centre runs 4.5 dp outside the tile, so it leaves a 3 dp
 * gap and has a 7 dp inner and 10 dp outer radius around the tile's 4.
 * It is drawn in the draw phase, never as a border that changes layout.
 */
object PleyFocus {
    val RingStroke = 3.dp

    /** From the tile's edge to the stroke's centre. */
    val RingOffset = 4.5.dp

    /** The ring's radius at the stroke's centre, around a [PleyShape.TileTv] corner. */
    val RingRadius = 8.5.dp

    /** How far the ring reaches past the tile's edge: [RingOffset] plus half the stroke. */
    val RingReach = 6.dp

    /**
     * A scrubber thumb's ring (TV player, Now Playing): slimmer and closer
     * than a tile's, so the playhead stays a dot rather than a target.
     */
    val ThumbRingStroke = 2.dp
    val ThumbRingOffset = 2.dp

    /** Tiles. */
    const val TILE_SCALE = 1.08f

    /** A tile held down with Select, for 100 ms. */
    const val TILE_PRESSED_SCALE = 1.04f

    /** Round buttons and PIN keys. */
    const val ROUND_SCALE = 1.08f

    /** Avatars. */
    const val AVATAR_SCALE = 1.1f

    /** Pills, text buttons, chips, tabs and rail items. */
    const val PILL_SCALE = 1.05f

    /** The phone card's press: scale and a 2 dp accent ring. */
    const val PHONE_PRESSED_SCALE = 1.04f
    val PhonePressRing = 2.dp

    /** The focused tile's glow (T1 and T2 only): how far it reaches, and its alpha at the tile's edge. */
    val GlowWidth = 20.dp
    const val GLOW_ALPHA = 0.45f
}

/**
 * Depth comes from lighter surfaces, not shadows (design spec §1.5).
 * Level 2 (dialogs, sheets, TV side panels) is [PleyColors.Surface2] with
 * a 1 dp [PleyColors.Surface4] hairline on top over a [PleyColors.Scrim];
 * T1, T2 and phones add this shadow, T0 doesn't.
 */
object PleyElevation {
    val Level2ShadowBlur = 24.dp
    val Level2ShadowOffsetY = 8.dp
    val Level2ShadowColor = Color.Black.copy(alpha = 0.5f)
    val Level2Hairline = 1.dp
}
