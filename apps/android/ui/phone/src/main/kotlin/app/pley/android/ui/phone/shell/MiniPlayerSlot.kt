package app.pley.android.ui.phone.shell

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.EnterTransition
import androidx.compose.animation.ExitTransition
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.shrinkVertically
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import app.pley.android.designsystem.PleyColors
import app.pley.android.designsystem.PleyMotion
import app.pley.android.designsystem.PleyShape
import app.pley.android.designsystem.icon.PleyIcons
import app.pley.android.designsystem.image.ArtworkImage
import app.pley.android.designsystem.image.MarkGlyph
import app.pley.android.designsystem.image.TrackArt
import app.pley.android.designsystem.icon.PlayPauseGlyph
import app.pley.android.designsystem.TrackChange
import app.pley.android.designsystem.reducedMotion
import app.pley.android.player.MusicPosition
import app.pley.android.player.NowPlaying
import app.pley.android.screens.PleyKey
import app.pley.android.ui.phone.R
import app.pley.android.ui.phone.kit.PleyIconButton
import app.pley.android.ui.phone.music.rememberMusicControl
import app.pley.android.ui.phone.music.rememberMusicPosition
import app.pley.android.ui.phone.music.trackKey
import app.pley.android.ui.phone.music.trackWords
import app.pley.android.ui.phone.player.CarGap
import app.pley.android.ui.phone.player.CarSized
import app.pley.android.ui.phone.player.LocalCarScreen
import app.pley.android.ui.phone.player.onCarScreen

/**
 * The mini player (design spec §3.6): a 72 dp bar above the navigation
 * bar while music plays: the 56 dp art, the title and "Artist · Album",
 * Play/Pause and Next, and a 2 dp [accent][PleyColors.Accent] line along
 * its top for how far the track has played (drawn, never recomposed).
 * Tapping it opens Now Playing; it hides while Now Playing shows. As the
 * track changes, the art and the words give way to the next one's as Now
 * Playing's do. It rises from the bottom edge as music starts and sinks
 * back as it stops (in place with reduced motion).
 */
@Composable
internal fun MiniPlayerSlot(nowPlaying: NowPlaying?, modifier: Modifier = Modifier, car: Boolean = onCarScreen()) {
    val navigator = LocalPhoneNavigator.current
    val navigation = navigator as? PhoneNavigation
    // Once Now Playing has drawn, not as soon as it is asked for; and back at once as it is closed.
    val onNowPlaying = navigation != null && navigation.nowPlayingDrawn && navigation.stack()?.lastOrNull() == PleyKey.NowPlaying
    // What played last, drawn while the bar leaves.
    val last = remember { arrayOfNulls<NowPlaying>(1) }
    if (nowPlaying != null) last[0] = nowPlaying
    // Composed with nothing playing too: music starting is then a change the bar can rise in on.
    val shown = nowPlaying ?: last[0]
    val reduced = reducedMotion()
    // Music starting or stopping: the bar rises from the bottom edge as the page makes room, and sinks back, where it
    // used to push the page up in one frame. Now Playing opening or closing swaps it at once (as Now Playing first
    // draws, still unseen under its fade): a bar still shrinking would move the page coming in.
    AnimatedVisibility(
        visible = nowPlaying != null,
        modifier = modifier,
        enter = if (reduced) EnterTransition.None else expandVertically(PleyMotion.overlay(), expandFrom = Alignment.Top) + fadeIn(PleyMotion.overlay()),
        exit = if (reduced) ExitTransition.None else shrinkVertically(PleyMotion.overlay(), shrinkTowards = Alignment.Top) + fadeOut(PleyMotion.overlay()),
        label = "mini player",
    ) {
        // On a car's screen, car-sized: 64 dp buttons, no text under 24 sp.
        if (shown != null && !onNowPlaying) CarSized(car) { MiniPlayer(shown) }
    }
}

@Composable
private fun MiniPlayer(nowPlaying: NowPlaying, modifier: Modifier = Modifier) {
    val navigator = LocalPhoneNavigator.current
    val music = rememberMusicControl()
    val position by rememberMusicPosition({ music?.position() ?: MusicPosition() }, nowPlaying.playing, key = trackKey(nowPlaying))
    val art = remember(nowPlaying.artUrl) { nowPlaying.artUrl?.let { ArtworkImage(listOf(it)) } }
    // What Play/Pause will do, a track still starting too.
    val playing = nowPlaying.wantsPlay
    // On a car's screen: 112 dp there, clear of its edges, its buttons apart and its title the larger line.
    val car = LocalCarScreen.current
    Row(
        modifier
            .fillMaxWidth()
            .height(if (car) 84.dp else 72.dp)
            .background(PleyColors.Surface2)
            .drawBehind {
                // How far along: 2 dp of accent over the surface's top edge.
                val h = 2.dp.toPx()
                val p = position
                val f = if (p.durationMs > 0) p.fraction else 0f
                drawLine(PleyColors.Surface4, Offset(0f, h / 2f), Offset(size.width, h / 2f), h)
                if (f > 0f) drawLine(PleyColors.Accent, Offset(0f, h / 2f), Offset(size.width * f, h / 2f), h)
            }
            .clickable(role = Role.Button, onClickLabel = stringResource(R.string.shell_now_playing)) { navigator.open(PleyKey.NowPlaying) }
            .padding(start = if (car) CarGap else 8.dp, end = if (car) CarGap else 4.dp)
            .testTag(MINI_PLAYER),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(Modifier.size(56.dp).clip(PleyShape.TilePhone).background(PleyColors.Surface3), contentAlignment = Alignment.Center) {
            // Without a cover, the placeholder Now Playing and the queue show for the same track: its title's colors and
            // Pley's mark, not a note of its own.
            TrackArt(art, nowPlaying.album ?: nowPlaying.title, Modifier.matchParentSize(), mark = if (nowPlaying.book) MarkGlyph.Audiobooks else MarkGlyph.Music)
        }
        Spacer(Modifier.width(12.dp))
        TrackChange(trackWords(nowPlaying), Modifier.weight(1f), Alignment.CenterStart) { words ->
            Column(Modifier.fillMaxWidth()) {
                Text(words.title, style = if (car) MaterialTheme.typography.titleLarge else MaterialTheme.typography.titleSmall, color = PleyColors.Fg, maxLines = 1, overflow = TextOverflow.Ellipsis)
                if (words.byline.isNotEmpty()) {
                    Text(words.byline, style = MaterialTheme.typography.bodySmall, color = PleyColors.Muted, maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
            }
        }
        // Play and Pause give way to each other as Now Playing's do, not in one frame.
        val label = stringResource(if (playing) R.string.music_pause else R.string.music_play)
        IconButton(onClick = { music?.toggle() }, modifier = Modifier.size(48.dp).semantics { contentDescription = label }) {
            PlayPauseGlyph(playing, size = 24.dp, tint = PleyColors.Fg)
        }
        if (car) Spacer(Modifier.width(CarGap))
        PleyIconButton(
            PleyIcons.SkipForward,
            stringResource(R.string.music_next),
            onClick = { music?.next() },
            enabled = nowPlaying.hasNext,
        )
    }
}

/** The mini player's test tag. */
internal const val MINI_PLAYER = "pley.phone.mini_player"
