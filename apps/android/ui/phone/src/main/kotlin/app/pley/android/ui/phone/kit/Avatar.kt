package app.pley.android.ui.phone.kit

import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.pley.android.designsystem.PleyType
import app.pley.android.designsystem.avatar.PleyAvatar
import app.pley.android.designsystem.image.ArtworkImage

/**
 * A profile's picture, or its color with the name's first letter in bold
 * (design spec §4.1, §4.7; every app's rule, [PleyAvatar]). [imageUrl] is
 * the picture's full address; while it loads, or if it fails, the letter
 * shows. Decoration for screen readers: the name is always said next to it.
 */
@Composable
fun Avatar(
    name: String,
    modifier: Modifier = Modifier,
    color: String? = null,
    imageUrl: String? = null,
    size: Dp = 32.dp,
) {
    val picture = remember(imageUrl) { imageUrl?.let { ArtworkImage(listOf(it)) } }
    PleyAvatar(
        name = name,
        color = color,
        picture = picture,
        size = size,
        style = PleyType.Phone.TitleSmall.copy(fontWeight = PleyType.Bold, fontSize = (size.value * 0.44f).sp, lineHeight = (size.value * 0.5f).sp),
        modifier = modifier,
    )
}
