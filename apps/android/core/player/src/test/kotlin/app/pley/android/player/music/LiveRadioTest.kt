package app.pley.android.player.music

import android.os.Looper
import androidx.media3.common.MediaItem
import androidx.media3.common.MediaMetadata
import androidx.media3.common.Player
import androidx.media3.common.util.UnstableApi
import app.pley.android.model.LiveChannel
import app.pley.android.player.live.LivePlayback
import app.pley.android.testing.Fixtures
import app.pley.android.testing.data.FakeLiveTv
import app.pley.android.testing.player.engine.FakePlayerEngine
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf

/** Live TV's sound in the car: tuning a picked channel, going through its list, pausing, and yielding the sound. */
@RunWith(RobolectricTestRunner::class)
@androidx.annotation.OptIn(UnstableApi::class)
class LiveRadioTest {
    private val live = FakeLiveTv()
    private val engines = mutableListOf<FakePlayerEngine>()
    private val channels = Fixtures.decode<List<LiveChannel>>("live/channels.json")
    private val radio = LiveRadio(
        { scope -> LivePlayback(live, { FakePlayerEngine().also { engines += it } }, { "android-1" }, scope, pausable = true) },
        { c -> MediaItem.Builder().setMediaId("channel:${c.id}").setMediaMetadata(MediaMetadata.Builder().setTitle(c.name).build()).build() },
    ) { "It doesn't play" }

    @After
    fun release() = radio.release()

    private fun settle() = shadowOf(Looper.getMainLooper()).idle()

    private fun playing(): Player {
        radio.play(channels, 0)
        settle()
        engines.last().player.ready()
        settle()
        return radio.player
    }

    @Test
    fun aPickedChannelIsWatchedAndShowsAsLiveAmongItsList() {
        radio.play(channels, 0)
        settle()
        assertEquals("watch 1 android-1", live.sent.first())
        val player = radio.player
        assertEquals(Player.STATE_BUFFERING, player.playbackState)
        assertTrue(player.playWhenReady)
        assertEquals(listOf("channel:1", "channel:2", "channel:5"), (0 until player.mediaItemCount).map { player.getMediaItemAt(it).mediaId })
        assertEquals(0, player.currentMediaItemIndex)
        // Live: no seeking within it, and no length.
        assertTrue(player.isCurrentMediaItemLive)
        assertFalse(player.isCurrentMediaItemSeekable)

        engines.last().player.ready()
        settle()
        assertEquals(Player.STATE_READY, player.playbackState)
    }

    @Test
    fun nextAndPreviousChangeChannelThroughTheList() {
        val player = playing()
        player.seekToNext()
        settle()
        assertEquals(1, player.currentMediaItemIndex)
        assertEquals("watch 2 android-1", live.sent.last())
        player.seekToPrevious()
        settle()
        assertEquals(0, player.currentMediaItemIndex)
        assertEquals("watch 1 android-1", live.sent.last())
    }

    @Test
    fun aChannelMadeReadyWaitsAndPlayTunesIt() {
        // The channel played when a car was last connected, or one the Assistant asks to have ready before it speaks.
        radio.cue(channels, 1)
        settle()
        assertEquals(emptyList<String>(), live.sent)
        val player = radio.player
        assertTrue(radio.active)
        assertEquals(1, player.currentMediaItemIndex)
        assertEquals("channel:2", player.currentMediaItem?.mediaId)
        assertFalse(player.playWhenReady)
        assertEquals(Player.STATE_READY, player.playbackState)
        assertTrue(player.isCurrentMediaItemLive)

        player.play()
        settle()
        assertEquals("watch 2 android-1", live.sent.last())
        assertTrue(player.playWhenReady)
        assertEquals(Player.STATE_BUFFERING, player.playbackState)
    }

    @Test
    fun aChannelMadeReadyWhileAnotherPlaysLetsThatOneGoAndPlayTunesTheNewOne() {
        val player = playing()
        radio.cue(channels, 2)
        settle()
        assertTrue(live.sent.last().startsWith("leave "))
        assertFalse(player.playWhenReady)
        assertEquals("channel:5", player.currentMediaItem?.mediaId)

        player.play()
        settle()
        assertEquals("watch 5 android-1", live.sent.last())
    }

    @Test
    fun pauseLetsTheChannelGoAndPlayTunesItAgain() {
        val player = playing()
        player.pause()
        settle()
        assertTrue(live.sent.last().startsWith("leave "))
        assertFalse(player.playWhenReady)
        assertTrue(radio.active)

        player.play()
        settle()
        assertEquals("watch 1 android-1", live.sent.last())
        assertTrue(player.playWhenReady)
    }

    @Test
    fun anotherAppsSoundTakingOverStopsTheRadioRatherThanTakingItBack() {
        val player = playing()
        val engine = engines.last()
        // The engine pauses as its audio focus goes (another app plays, headphones pulled out).
        engine.player.pause()
        settle()
        assertFalse(engine.player.playWhenReady)
        assertTrue(live.sent.last().startsWith("leave "))
        assertFalse(player.playWhenReady)
    }

    @Test
    fun aCarsEditsToTheListChangeNothingAndAChannelListedTwiceIsFine() {
        radio.play(channels + channels.first(), 0)
        settle()
        val player = radio.player
        assertEquals(4, player.mediaItemCount)
        // A car's queue may ask to remove or move entries: the list is the one picked from.
        player.removeMediaItem(1)
        player.moveMediaItem(0, 2)
        player.addMediaItem(MediaItem.fromUri("http://elsewhere/other"))
        settle()
        assertEquals(listOf("channel:1", "channel:2", "channel:5", "channel:1"), (0 until player.mediaItemCount).map { player.getMediaItemAt(it).mediaId })
    }

    @Test
    fun closedItHasNothing() {
        val player = playing()
        radio.close()
        settle()
        assertFalse(radio.active)
        assertEquals(0, player.mediaItemCount)
        assertEquals(Player.STATE_IDLE, player.playbackState)
        assertTrue(live.sent.last().startsWith("leave "))
    }
}
