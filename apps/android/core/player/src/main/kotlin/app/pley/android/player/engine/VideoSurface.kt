package app.pley.android.player.engine

import androidx.annotation.OptIn
import androidx.compose.animation.core.AnimationSpec
import androidx.compose.animation.core.SnapSpec
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.snap
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.testTag
import androidx.media3.common.Player
import androidx.media3.common.util.UnstableApi
import androidx.media3.ui.compose.PlayerSurface
import androidx.media3.ui.compose.SURFACE_TYPE_SURFACE_VIEW
import androidx.media3.ui.compose.modifiers.resizeWithContentScale
import androidx.media3.ui.compose.state.rememberPresentationState

/**
 * The video, on a SurfaceView: frames go straight to the display's
 * compositor (timed better, less power, HDR passed through, full resolution
 * on TVs whose interface is 1080p). Fitted to the screen, black until the
 * first frame, except while [holding]: a stream made ready beside the one
 * that played takes over (PlayerUiState.handoff), and the last picture
 * stays until the new one's first. The black comes at once and goes by
 * [reveal]: at once by default, as a live channel's picture shows; the TV
 * player fades its first picture in, as the native Samsung app does. The
 * player may be prepared before this exists. Never put it in a lazy list or
 * under a full-screen alpha animation. Subtitles go over it with
 * [ScreenCaptions][app.pley.android.player.subtitles.ScreenCaptions].
 */
@OptIn(markerClass = [UnstableApi::class])
@Composable
fun VideoSurface(
    player: Player?,
    modifier: Modifier = Modifier,
    reveal: AnimationSpec<Float> = snap(),
    holding: () -> Boolean = { false },
) {
    // Media3's ContentFrame in its parts: its shutter goes the moment the first frame shows, so it can't fade.
    val presentation = rememberPresentationState(player)
    PlayerSurface(
        player = player,
        modifier = modifier.background(Color.Black).resizeWithContentScale(ContentScale.Fit, presentation.videoSizeDp),
        surfaceType = SURFACE_TYPE_SURFACE_VIEW,
    )
    val covered = presentation.coverSurface && !holding()
    val shutter = animateFloatAsState(if (covered) 1f else 0f, if (covered) snap() else reveal, label = "shutter")
    val fading by remember { derivedStateOf { shutter.value > 0f } }
    // A reveal at once goes as the picture is uncovered: the snap would keep the black a frame more.
    val atOnce = reveal is SnapSpec && reveal.delay == 0
    // Drawn at its alpha, read while drawing: the fade redraws, nothing composes again.
    if (covered || (fading && !atOnce)) {
        Box(
            Modifier
                .fillMaxSize()
                .drawBehind { drawRect(Color.Black, alpha = if (covered) 1f else shutter.value) }
                .testTag(TAG_VIDEO_SHUTTER),
        )
    }
}

/** The black over the video until its first frame shows. */
const val TAG_VIDEO_SHUTTER = "pley.player.shutter"
