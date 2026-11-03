package app.pley.android.ui.tv.livetv

import android.text.format.DateFormat
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawWithCache
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.graphics.addOutline
import androidx.compose.ui.graphics.drawscope.clipPath
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.unit.dp
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.PleyType
import app.pley.android.designsystem.image.ArtworkImage
import app.pley.android.designsystem.tile.ChannelLogo
import app.pley.android.model.LiveChannel
import app.pley.android.model.LiveProgram
import app.pley.android.screens.livetv.LiveTv
import app.pley.android.ui.tv.R
import java.util.Date

/** "20:00" or "8:00 PM", as the device writes times. */
@Composable
internal fun rememberClock(): (Long) -> String {
    val context = LocalContext.current
    return remember(context) {
        val format = DateFormat.getTimeFormat(context)
        val clock: (Long) -> String = { seconds -> format.format(Date(seconds * 1000)) }
        clock
    }
}

/** "20:00 – 21:00". */
@Composable
internal fun spanOf(program: LiveProgram, clock: (Long) -> String): String =
    stringResource(R.string.tv_livetv_span, clock(program.start), clock(program.stop))

/** "12 min left", "1 h 5 min left". */
@Composable
internal fun timeLeft(program: LiveProgram, now: Long): String {
    val (hours, minutes) = LiveTv.left(program, now)
    return when {
        hours == 0 -> stringResource(R.string.tv_livetv_minutes_left, minutes)
        minutes == 0 -> stringResource(R.string.tv_livetv_hours_left, hours)
        else -> stringResource(R.string.tv_livetv_hours_minutes_left, hours, minutes)
    }
}

/**
 * [text] cut at a word to a little more than the guide's two lines over
 * the grid hold, ended with "…" (Samsung's): a text is laid out whole
 * before its last line is ended with "…", and a provider's description can
 * run to pages, which held up each move along a row on a TV.
 */
internal fun shortened(text: String, chars: Int = DESCRIPTION_CHARS): String {
    if (text.length <= chars) return text
    val space = text.lastIndexOf(' ', chars)
    var end = if (space > chars / 2) space else chars
    // Not between the two halves of a character outside the BMP (an emoji).
    if (text[end - 1].isHighSurrogate()) end--
    return text.substring(0, end) + "…"
}

/** Three lines' worth of the guide's text over the grid, past which nothing shows. */
internal const val DESCRIPTION_CHARS = 240

/** "Today", "Tomorrow", "Yesterday", or the date as the device writes it. */
@Composable
internal fun dayOf(seconds: Long, now: Long): String {
    val context = LocalContext.current
    return when (LiveTv.daysApart(seconds, now)) {
        0L -> stringResource(R.string.tv_livetv_today)
        1L -> stringResource(R.string.tv_livetv_tomorrow)
        -1L -> stringResource(R.string.tv_livetv_yesterday)
        else -> DateFormat.getMediumDateFormat(context).format(Date(seconds * 1000))
    }
}

/**
 * A channel's logo in a rounded [well], or on its gradient its number (its
 * name when [name]) without one. Rounded as it is drawn, not by a clip: a
 * guide shows one in every row, and each clip is a graphics layer.
 */
@Composable
internal fun ChannelTile(
    channel: LiveChannel,
    origin: String,
    modifier: Modifier = Modifier,
    name: Boolean = false,
    style: TextStyle = PleyType.Tv.Label,
    well: Color = PleyColors.Surface3,
    shape: Shape = PleyShape.TileTv,
) {
    ChannelLogo(
        image = ArtworkImage.of(channel.logo, origin),
        label = if (name) channel.name else channel.number ?: channel.name.take(3),
        textStyle = style,
        modifier = modifier.background(well, shape).drawnIn(shape),
        inset = 8.dp,
    )
}

/** What is drawn after this (the gradient, the logo) cut to [shape] on the canvas, with no layer of its own. */
private fun Modifier.drawnIn(shape: Shape): Modifier = drawWithCache {
    val outline = Path().apply { addOutline(shape.createOutline(size, layoutDirection, this@drawWithCache)) }
    onDrawWithContent { clipPath(outline) { this@onDrawWithContent.drawContent() } }
}

/** How far [program] is at [now]: a 3 dp track with the accent over it. */
@Composable
internal fun ProgramProgress(program: LiveProgram, now: Long, modifier: Modifier = Modifier, track: Color = PleyColors.Surface4) {
    Box(modifier.height(3.dp).clip(PleyShape.Badge).background(track)) {
        Box(Modifier.fillMaxHeight().fillMaxWidth(program.progressAt(now)).background(PleyColors.Accent))
    }
}

/** A small boxed word: Live in danger red, New in the accent. */
@Composable
internal fun ProgramBadge(text: String, live: Boolean, modifier: Modifier = Modifier) {
    BasicText(
        text.uppercase(),
        style = PleyType.Tv.Badge.copy(color = if (live) PleyColors.Bg else PleyColors.AccentBright),
        maxLines = 1,
        modifier = modifier
            .clip(PleyShape.Badge)
            .background(if (live) PleyColors.Danger else PleyColors.PrimaryContainer)
            .padding(horizontal = 5.dp, vertical = 2.dp),
    )
}

/** The badge a programme carries, if any: Live first, else New. */
@Composable
internal fun BadgeOf(program: LiveProgram, modifier: Modifier = Modifier) {
    when {
        program.isLive -> ProgramBadge(stringResource(R.string.tv_livetv_badge_live), live = true, modifier = modifier)
        program.isNew -> ProgramBadge(stringResource(R.string.tv_livetv_badge_new), live = false, modifier = modifier)
    }
}
