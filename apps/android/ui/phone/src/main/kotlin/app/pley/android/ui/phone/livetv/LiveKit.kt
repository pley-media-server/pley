package app.pley.android.ui.phone.livetv

import android.text.format.DateFormat
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.unit.dp
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.image.ArtworkImage
import app.pley.android.designsystem.tile.ChannelLogo
import app.pley.android.model.LiveChannel
import app.pley.android.model.LiveProgram
import app.pley.android.screens.livetv.LiveTv
import app.pley.android.ui.phone.R
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
    stringResource(R.string.livetv_span, clock(program.start), clock(program.stop))

/** "12 min left", "1 h 5 min left". */
@Composable
internal fun timeLeft(program: LiveProgram, now: Long): String {
    val (hours, minutes) = LiveTv.left(program, now)
    return when {
        hours == 0 -> stringResource(R.string.livetv_minutes_left, minutes)
        minutes == 0 -> stringResource(R.string.livetv_hours_left, hours)
        else -> stringResource(R.string.livetv_hours_minutes_left, hours, minutes)
    }
}

/** "Today", "Tomorrow", "Yesterday", or the date as the device writes it. */
@Composable
internal fun dayOf(seconds: Long, now: Long): String {
    val context = LocalContext.current
    return when (LiveTv.daysApart(seconds, now)) {
        0L -> stringResource(R.string.livetv_today)
        1L -> stringResource(R.string.livetv_tomorrow)
        -1L -> stringResource(R.string.livetv_yesterday)
        else -> DateFormat.getMediumDateFormat(context).format(Date(seconds * 1000))
    }
}

/** A channel's logo image on [origin]; `null` without one. */
internal fun logoOf(channel: LiveChannel, origin: String): ArtworkImage? = ArtworkImage.of(channel.logo, origin)

/**
 * A channel's logo in a rounded [surface3][PleyColors.Surface3] well, or
 * its number (its name when [name]) on its gradient without one.
 */
@Composable
internal fun ChannelTile(
    channel: LiveChannel,
    origin: String,
    modifier: Modifier = Modifier,
    name: Boolean = false,
    style: TextStyle = MaterialTheme.typography.labelMedium,
    well: Color = PleyColors.Surface3,
    shape: androidx.compose.ui.graphics.Shape = PleyShape.Field,
) {
    ChannelLogo(
        image = logoOf(channel, origin),
        label = if (name) channel.name else channel.number ?: channel.name.take(3),
        textStyle = style,
        modifier = modifier.clip(shape).background(well),
    )
}

/** How far [program] is at [now]: a 3 dp track with the accent over it. */
@Composable
internal fun ProgramProgress(program: LiveProgram, now: Long, modifier: Modifier = Modifier, track: Color = PleyColors.Surface4) {
    Box(modifier.height(3.dp).clip(PleyShape.Badge).background(track)) {
        Box(Modifier.fillMaxHeight().fillMaxWidth(program.progressAt(now)).background(PleyColors.Accent))
    }
}

/** A small boxed word over a programme: Live in danger red, New in the accent. */
@Composable
internal fun ProgramBadge(text: String, live: Boolean, modifier: Modifier = Modifier) {
    Text(
        text.uppercase(),
        style = MaterialTheme.typography.labelSmall.copy(letterSpacing = MaterialTheme.typography.labelSmall.letterSpacing),
        color = if (live) PleyColors.Bg else PleyColors.AccentBright,
        maxLines = 1,
        modifier = modifier
            .clip(PleyShape.Badge)
            .background(if (live) PleyColors.Danger else PleyColors.PrimaryContainer)
            .padding(horizontal = 5.dp, vertical = 1.dp),
    )
}

/** The badge a programme carries, if any: Live first, else New. */
@Composable
internal fun BadgeOf(program: LiveProgram, modifier: Modifier = Modifier) {
    when {
        program.isLive -> ProgramBadge(stringResource(R.string.livetv_badge_live), live = true, modifier = modifier)
        program.isNew -> ProgramBadge(stringResource(R.string.livetv_badge_new), live = false, modifier = modifier)
    }
}
