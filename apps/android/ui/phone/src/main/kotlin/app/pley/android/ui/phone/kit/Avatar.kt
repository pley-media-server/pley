package app.pley.android.ui.phone.kit

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.PleyType
import coil3.compose.AsyncImage

/**
 * The web's profile colors and how it picks text on them
 * (apps/web/src/components/Avatar.tsx), so a profile looks the same in
 * the browser and the app.
 */
object AvatarColors {
    /** The colors profiles can choose, and the fallback picked from the name. */
    val palette: List<Color> = listOf(
        Color(0xFF38BDF8), Color(0xFFA78BFA), Color(0xFFF472B6), Color(0xFFFB923C),
        Color(0xFFFACC15), Color(0xFF4ADE80), Color(0xFF2DD4BF), Color(0xFFF87171),
    )

    private val DarkText = Color(0xFF0B0D10)

    /** The profile's [chosen] color ("#a78bfa"), or one picked from [name] as the web picks it. */
    fun of(name: String, chosen: String?): Color {
        chosen?.let(::parse)?.let { return it }
        // As `for (const ch of name)` with charCodeAt(0): one step per code point, its first UTF-16 unit.
        var hash = 0L
        var i = 0
        while (i < name.length) {
            hash = (hash * 31 + name[i].code) and 0xFFFFFFFFL
            i += Character.charCount(name.codePointAt(i))
        }
        return palette[(hash % palette.size).toInt()]
    }

    /** Dark text on light colors, white on dark ones: the web's luminance test, on unconverted channels. */
    fun textOn(color: Color): Color {
        val luminance = 0.2126f * color.red + 0.7152f * color.green + 0.0722f * color.blue
        return if (luminance > 0.45f) DarkText else Color.White
    }

    /** "#rgb" or "#rrggbb", with or without the hash; null for anything else. */
    fun parse(hex: String): Color? {
        val digits = hex.removePrefix("#")
        if (digits.length != 3 && digits.length != 6 || digits.any { Character.digit(it, 16) < 0 }) return null
        val full = if (digits.length == 3) digits.map { "$it$it" }.joinToString("") else digits
        return Color(0xFF000000 or full.toLong(16))
    }
}

/**
 * A profile's picture, or its color with the name's first letter in bold
 * (design spec §4.1, §4.7). [imageUrl] is the picture's full address; if
 * it fails, the letter shows. Decoration for screen readers: the name is
 * always said next to it.
 */
@Composable
fun Avatar(
    name: String,
    modifier: Modifier = Modifier,
    color: String? = null,
    imageUrl: String? = null,
    size: Dp = 32.dp,
) {
    val fill = remember(name, color) { AvatarColors.of(name, color) }
    var broken by remember(imageUrl) { mutableStateOf(false) }
    Box(
        modifier
            .size(size)
            .clip(PleyShape.Circle)
            .background(if (imageUrl != null && !broken) PleyColors.Surface3 else fill)
            .clearAndSetSemantics {},
        contentAlignment = Alignment.Center,
    ) {
        if (imageUrl != null && !broken) {
            AsyncImage(
                model = imageUrl,
                contentDescription = null,
                contentScale = ContentScale.Crop,
                onError = { broken = true },
                modifier = Modifier.size(size),
            )
        } else {
            Text(
                text = name.trim().firstOrNull()?.uppercase() ?: "?",
                color = AvatarColors.textOn(fill),
                style = PleyType.Phone.TitleSmall.copy(fontWeight = PleyType.Bold, fontSize = (size.value * 0.44f).sp, lineHeight = (size.value * 0.5f).sp),
            )
        }
    }
}
