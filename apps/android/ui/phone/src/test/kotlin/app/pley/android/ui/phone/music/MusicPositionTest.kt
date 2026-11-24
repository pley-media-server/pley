package app.pley.android.ui.phone.music

import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleOwner
import androidx.lifecycle.LifecycleRegistry
import androidx.lifecycle.compose.LocalLifecycleOwner
import app.pley.android.player.MusicPosition
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/** Where the track is is read every half second while it plays, and not at all while the screen is out of sight. */
@RunWith(RobolectricTestRunner::class)
class MusicPositionTest {
    @get:Rule
    val compose = createComposeRule()

    private val owner = object : LifecycleOwner {
        val registry = LifecycleRegistry.createUnsafe(this)
        override val lifecycle: Lifecycle get() = registry
    }

    private var reads = 0

    @Test
    fun aPlayingTrackIsReadOnlyWhileTheScreenIsStarted() {
        owner.registry.currentState = Lifecycle.State.RESUMED
        compose.mainClock.autoAdvance = false
        compose.setContent {
            CompositionLocalProvider(LocalLifecycleOwner provides owner) {
                rememberMusicPosition({ MusicPosition(reads++ * 500L, 180_000, 0) }, playing = true, key = 1)
            }
        }
        compose.mainClock.advanceTimeBy(2_000)
        val seen = reads
        assertTrue("$seen reads in 2 s", seen >= 4)

        // The app goes out of sight (Home, the phone locked): the music plays on, unread.
        compose.runOnUiThread { owner.registry.currentState = Lifecycle.State.CREATED }
        compose.mainClock.advanceTimeBy(10_000)
        assertEquals(seen, reads)

        // Back in view, it is read at once, then every half second again.
        compose.runOnUiThread { owner.registry.currentState = Lifecycle.State.RESUMED }
        compose.mainClock.advanceTimeBy(50)
        assertEquals(seen + 1, reads)
        compose.mainClock.advanceTimeBy(1_000)
        assertTrue("$reads reads", reads >= seen + 3)
    }
}
