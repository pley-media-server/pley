package app.pley.android.ui.phone.kit

import androidx.compose.foundation.focusable
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.graphics.drawscope.clipRect
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.progressBarRangeInfo
import androidx.compose.ui.semantics.ProgressBarRangeInfo
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.setProgress
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.unit.dp
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.icon.Icon
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.text.shownLocale
import app.pley.android.ui.phone.R
import java.text.NumberFormat
import kotlin.math.roundToInt

/**
 * The profile's own rating, as on the web (StarRating.tsx): five stars,
 * halves allowed, saved as 0 to 10 (a half star each). [value] is that
 * number, or null when unrated.
 *
 * Tapping the left half of a star picks the half; tapping the rating it
 * already has clears it. With a keyboard: Left and Down take half a star
 * off, Right and Up add one, Home and End go to none and five, Delete
 * clears. Screen readers hear a slider ("Your rating, 3.5 stars").
 */
@Composable
fun StarRating(
    value: Int?,
    onChange: (Int?) -> Unit,
    modifier: Modifier = Modifier,
    starSize: androidx.compose.ui.unit.Dp = 24.dp,
) {
    val current by rememberUpdatedState(value)
    val change by rememberUpdatedState(onChange)
    val shown = (value ?: 0).coerceIn(0, MAX)
    val label = stringResource(R.string.kit_rating_label)
    val locale = shownLocale()
    val described = if (value == null || value == 0) {
        stringResource(R.string.kit_rating_none)
    } else {
        // "3.5", "3,5": a half star in the way the language shown writes it.
        val text = NumberFormat.getInstance(locale).apply { maximumFractionDigits = 1 }.format(value / 2.0)
        pluralStringResource(R.plurals.kit_rating_stars, if (value == 2) 1 else 2, text)
    }
    fun pick(next: Int) = change(if (next <= 0) null else next.coerceAtMost(MAX))
    Row(
        modifier
            .semantics {
                contentDescription = label
                stateDescription = described
                progressBarRangeInfo = ProgressBarRangeInfo(shown.toFloat(), 0f..MAX.toFloat(), steps = MAX - 1)
                setProgress { target ->
                    pick(target.roundToInt())
                    true
                }
            }
            .onKeyEvent { event ->
                if (event.type != KeyEventType.KeyDown) return@onKeyEvent false
                val now = current ?: 0
                when (event.key) {
                    Key.DirectionRight, Key.DirectionUp -> pick(now + 1)
                    Key.DirectionLeft, Key.DirectionDown -> pick(now - 1)
                    Key.MoveHome -> pick(0)
                    Key.MoveEnd -> pick(MAX)
                    Key.Delete, Key.Backspace -> pick(0)
                    else -> return@onKeyEvent false
                }
                true
            }
            .focusable(),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        for (star in 1..STARS) {
            val fill = (shown / 2f - (star - 1)).coerceIn(0f, 1f)
            Box(
                Modifier
                    .size(starSize + 16.dp, maxOf(48.dp, starSize + 16.dp))
                    .pointerInput(star) {
                        detectTapGestures { offset ->
                            val picked = star * 2 - if (offset.x < size.width / 2f) 1 else 0
                            pick(if (picked == current) 0 else picked)
                        }
                    },
                contentAlignment = Alignment.Center,
            ) {
                Icon(PleyIcons.Star, contentDescription = null, size = starSize, tint = PleyColors.Control)
                if (fill > 0f) {
                    Icon(
                        PleyIcons.Star,
                        contentDescription = null,
                        size = starSize,
                        tint = PleyColors.Accent,
                        modifier = Modifier.drawWithContent {
                            clipRect(right = size.width * fill) { this@drawWithContent.drawContent() }
                        },
                    )
                }
            }
        }
    }
}

private const val STARS = 5
private const val MAX = 10
