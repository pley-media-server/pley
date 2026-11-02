package app.pley.android.designsystem

import androidx.compose.ui.text.PlatformTextStyle
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.Hyphens
import androidx.compose.ui.text.style.LineBreak
import androidx.compose.ui.text.style.LineHeightStyle
import androidx.compose.ui.unit.TextUnit
import androidx.compose.ui.unit.sp

/**
 * pley's type (design spec §1.2): the platform's Roboto, weights 400, 500
 * and 700 only (Fire OS 7 and 8 have no 600 or 800, so a cue built on them
 * would vanish there), never below 400 on TV. Titles break like headings
 * and never hyphenate, as the web's `text-wrap: balance`.
 *
 * The styles carry no colour: the shells' themes and components add it.
 */
object PleyType {
    /** Body text. */
    val Regular = FontWeight.W400

    /** Labels, captions. */
    val Medium = FontWeight.W500

    /** Titles, badges, a focused tile's caption. */
    val Bold = FontWeight.W700

    private val lineBox = LineHeightStyle(LineHeightStyle.Alignment.Center, LineHeightStyle.Trim.None)

    private fun style(
        size: Int,
        line: Int,
        weight: FontWeight,
        tracking: Float = 0f,
        title: Boolean = false,
        family: FontFamily = FontFamily.Default,
    ): TextStyle = TextStyle(
        fontFamily = family,
        fontSize = size.sp,
        lineHeight = line.sp,
        fontWeight = weight,
        letterSpacing = if (tracking == 0f) TextUnit.Unspecified else tracking.sp,
        lineHeightStyle = lineBox,
        platformStyle = PlatformTextStyle(includeFontPadding = false),
        lineBreak = if (title) LineBreak.Heading else LineBreak.Paragraph,
        hyphens = if (title) Hyphens.None else Hyphens.Unspecified,
    )

    /**
     * TV styles, in sp on the 960 by 540 canvas, at Google TV's density.
     * Body text is at least 14 sp and captions 13; [Tv.Badge] alone is
     * smaller, always bold and boxed.
     */
    object Tv {
        /** A title without logo art; 2 lines at most. */
        val HeroTitle = style(38, 44, Bold, -0.5f, title = true)

        /** Library, Search, Settings, Who's watching? */
        val PageTitle = style(28, 34, Bold, -0.25f, title = true)

        /** Row headers. */
        val Section = style(18, 24, Bold, title = true)

        /** Episode titles in info blocks; panel and dialog titles; placeholder titles on tiles. */
        val Title = style(16, 22, Bold, title = true)

        /** Summaries on the hero and the item page. */
        val BodyLarge = style(15, 22, Regular)

        /** Lists, settings rows, instructions, 2-line summaries. */
        val Body = style(14, 20, Regular)

        /** Buttons, the metadata line, tabs, chips. */
        val Label = style(14, 18, Medium, 0.1f)

        /** Tile captions, scrubber times. */
        val Caption = style(13, 17, Medium, 0.2f)

        /** Quality badges, as written ("1080p", never forced to capitals). */
        val Badge = style(11, 13, Bold, 0.4f)

        /** The link code, in a monospaced face. */
        val Code = style(48, 56, Bold, 4f, family = FontFamily.Monospace)

        /** PIN pad keys. */
        val PinDigit = style(28, 28, Medium)
    }

    /** Phone styles, named after the Material 3 roles the phone's theme sets them as. */
    object Phone {
        /** An item's title without logo art. */
        val DisplaySmall = style(32, 38, Bold, -0.5f, title = true)

        /** Screen titles. */
        val HeadlineMedium = style(26, 32, Bold, -0.25f, title = true)

        /** The library's name in its top bar. */
        val HeadlineSmall = style(22, 28, Bold, -0.2f, title = true)

        /** Section headers. */
        val TitleLarge = style(19, 25, Bold, title = true)

        /** List and episode titles. */
        val TitleMedium = style(16, 22, Medium, title = true)

        /** Tile titles. */
        val TitleSmall = style(14, 20, Medium, 0.1f, title = true)

        /** Summaries. */
        val BodyLarge = style(16, 24, Regular)

        /** Body, the second line of list rows. */
        val BodyMedium = style(14, 20, Regular)

        /** Tile subtitles, timestamps. */
        val BodySmall = style(12, 16, Regular, 0.1f)

        /** Buttons. */
        val LabelLarge = style(15, 20, Medium, 0.1f)

        /** Chips, the metadata line, navigation labels. */
        val LabelMedium = style(13, 16, Medium, 0.2f)

        /** Quality badges. */
        val LabelSmall = style(11, 14, Bold, 0.4f)
    }
}
