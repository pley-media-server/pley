package app.pley.android.player.music

import android.os.Bundle
import android.os.Looper
import androidx.media3.common.MediaItem
import androidx.media3.common.MediaMetadata
import androidx.media3.common.PlaybackException
import androidx.media3.common.Player
import androidx.media3.common.Timeline
import androidx.media3.exoplayer.ExoPlayer
import androidx.test.core.app.ApplicationProvider
import app.pley.android.model.ItemKind
import app.pley.android.model.ItemSummary
import app.pley.android.model.PlaybackRequest
import app.pley.android.model.caps.ClientCaps
import app.pley.android.network.OfflineException
import app.pley.android.player.EngineSource
import app.pley.android.player.MusicError
import app.pley.android.player.MusicRepeat
import app.pley.android.player.session.SessionReporter
import app.pley.android.testing.network.FakePleyApi
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf

/**
 * The media session's player: the music's whole queue, as a car lists it,
 * with picks from the queue, Shuffle and Play going through the music.
 */
@RunWith(RobolectricTestRunner::class)
class QueuePlayerTest {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    private val output = SilentOutput()
    private val api = FakePleyApi()
    private val playback = MusicPlayback(
        MusicEnvironment(
            api = { api },
            caps = { ClientCaps() },
            clientId = { "android-test-music" },
            reporter = SessionReporter(scope),
            output = { output },
        ),
        scope,
    )
    private val engine = ExoPlayer.Builder(ApplicationProvider.getApplicationContext()).build()
    private val album = (1..3L).map { track(it) }
    private val player = QueuePlayer(engine, playback, words = { error, nextIn -> "Pley: $error" + (nextIn?.let { " (next in $it)" } ?: "") })

    @After
    fun tearDown() {
        scope.cancel()
        engine.release()
    }

    /** The engine holds track [id], as the output describes the track it plays. */
    private fun engineHolds(id: Long) {
        val metadata = MediaMetadata.Builder().setTitle("Track $id").setExtras(Bundle().apply { putLong(EngineMusicOutput.TRACK_ID, id) }).build()
        engine.setMediaItem(MediaItem.Builder().setUri("http://server/stream").setMediaMetadata(metadata).build())
        idle()
    }

    private fun idle() = shadowOf(Looper.getMainLooper()).idle()

    @Test
    fun theWholeQueueShowsWithTheTrackPlayingAsTheEngineHasIt() {
        playback.play(album, startIndex = 1)
        engineHolds(2)
        player.queueChanged()
        idle()

        assertEquals(3, player.mediaItemCount)
        assertEquals(1, player.currentMediaItemIndex)
        assertEquals("Track 2", player.mediaMetadata.title.toString())
        assertEquals("track:2", player.currentMediaItem?.mediaId)
        assertEquals("Track 3", player.getMediaItemAt(2).mediaMetadata.title.toString())
        assertTrue(player.hasNextMediaItem())
        assertTrue(player.isCommandAvailable(Player.COMMAND_SEEK_TO_MEDIA_ITEM))
        assertTrue(player.isCommandAvailable(Player.COMMAND_SET_SHUFFLE_MODE))
        assertTrue(player.isCommandAvailable(Player.COMMAND_SET_REPEAT_MODE))
        assertFalse(player.isCommandAvailable(Player.COMMAND_SET_SPEED_AND_PITCH))

        // The engine still holding track 2 while track 3 is asked for: the entry is track 3's own.
        playback.jumpTo(2)
        player.queueChanged()
        idle()
        assertEquals("track:3", player.currentMediaItem?.mediaId)
        assertEquals("Track 3", player.mediaMetadata.title.toString())
    }

    @Test
    fun aQueueThatWaitsShowsPausedOnItsTrackWhereItWillStartAndPlayStartsIt() {
        // What played last, brought back as a car connects: the car is shown the track with Play, not nothing to play.
        playback.cue(album, startIndex = 1, startMs = 61_000)
        player.queueChanged()
        idle()
        assertEquals(3, player.mediaItemCount)
        assertEquals("track:2", player.currentMediaItem?.mediaId)
        assertFalse(player.playWhenReady)
        assertFalse(player.isPlaying)
        assertTrue(player.playbackState != Player.STATE_IDLE)
        assertEquals(null, player.playerError)
        assertEquals(61_000L, player.currentPosition)

        player.play()
        idle()
        assertEquals(2L, (api.calls("startPlayback").single().args[0] as PlaybackRequest).itemId)
        assertTrue(playback.nowPlaying.value!!.wantsPlay)
    }

    @Test
    fun betweenTracksTheQueuesTrackShowsAsLoadingNeverAsNothingPlaying() {
        playback.play(album, startIndex = 1)
        // The engine has nothing loaded yet: the track is asked of the server. A car shows its loading screen for a
        // session with nothing in it, so the session lists the queue and says the track loads.
        player.queueChanged()
        idle()
        assertEquals(3, player.mediaItemCount)
        assertEquals("track:2", player.currentMediaItem?.mediaId)
        assertEquals(Player.STATE_BUFFERING, player.playbackState)
        assertTrue(player.playWhenReady)
        assertEquals(0, player.currentPosition)

        engineHolds(2)
        player.queueChanged()
        idle()
        assertEquals("Track 2", player.mediaMetadata.title.toString())
    }

    @Test
    fun aTrackAskedForAgainShowsItsStartNotWhereThePlayBeforeWas() {
        playback.play(album)
        idle()
        engineHolds(1)
        engine.seekTo(170_000)
        idle()
        player.queueChanged()
        idle()
        assertEquals(170_000L, player.currentPosition)

        // Repeating, or picked again: while it is asked of the server, the engine still holds the play before, near its end.
        api.hold()
        playback.jumpTo(0)
        player.queueChanged()
        idle()
        assertEquals(Player.STATE_BUFFERING, player.playbackState)
        assertEquals(0L, player.currentPosition)
        api.release()
    }

    @Test
    fun aLongQueueIsDescribedOnceAndAgainOnlyWhenItChanges() {
        var described = 0
        val counted = QueuePlayer(engine, playback, describe = { described++; QueuePlayer.plainItem(it) })
        val long = (1..2_000L).map { track(it) }
        playback.play(long)
        idle()
        counted.queueChanged()
        idle()
        assertEquals(2_000, counted.mediaItemCount)
        val once = described
        assertEquals(2_000, once)

        // What plays changes (loaded, paused, the next track): the entries already described list it.
        engineHolds(1)
        counted.queueChanged()
        playback.pause()
        counted.queueChanged()
        playback.next()
        counted.queueChanged()
        idle()
        assertEquals(1, counted.currentMediaItemIndex)
        assertEquals(once, described)

        // The queue itself changes: described again, once.
        playback.setShuffle(true)
        counted.queueChanged()
        counted.queueChanged()
        idle()
        assertEquals(once + 2_000, described)
    }

    @Test
    fun aPickFromTheQueueAndShuffleGoThroughTheMusic() {
        playback.play(album)
        engineHolds(1)
        player.queueChanged()
        idle()

        player.seekTo(2, 0)
        idle()
        assertEquals(2, playback.queueState.value.index)
        player.queueChanged()
        idle()
        assertEquals(2, player.currentMediaItemIndex)

        player.shuffleModeEnabled = true
        idle()
        assertTrue(playback.queueState.value.shuffle)
        player.queueChanged()
        idle()
        assertTrue(player.shuffleModeEnabled)
    }

    @Test
    fun aTrackIsTheSameEntryWhileItIsAskedForLoadsAndPlays() {
        val changes = mutableListOf<String>()
        var queues = 0
        val seekable = mutableListOf<Boolean>()
        player.addListener(
            object : Player.Listener {
                override fun onMediaMetadataChanged(mediaMetadata: MediaMetadata) {
                    changes += mediaMetadata.title.toString()
                }

                override fun onTimelineChanged(timeline: Timeline, reason: Int) {
                    queues++
                }

                override fun onAvailableCommandsChanged(availableCommands: Player.Commands) {
                    seekable += availableCommands.contains(Player.COMMAND_SEEK_IN_CURRENT_MEDIA_ITEM)
                }
            },
        )
        playback.play(album, startIndex = 1)
        player.queueChanged()
        idle()
        val asked = player.currentMediaItem
        val length = player.duration
        assertEquals(180_000L, length)
        assertEquals(0L, player.bufferedPosition)
        assertTrue(player.isCommandAvailable(Player.COMMAND_SEEK_IN_CURRENT_MEDIA_ITEM))

        // The output has it now, with its own name for it, a cover of its own and no length yet: a car would draw
        // Now Playing again for each of those, at every skip.
        val metadata = MediaMetadata.Builder()
            .setTitle("Track 2 (from the file)")
            .setArtworkData(byteArrayOf(1, 2, 3), MediaMetadata.PICTURE_TYPE_FRONT_COVER)
            .setExtras(Bundle().apply { putLong(EngineMusicOutput.TRACK_ID, 2) })
            .build()
        engine.setMediaItem(MediaItem.Builder().setUri("http://server/stream").setMediaMetadata(metadata).build())
        idle()
        player.queueChanged()
        idle()
        assertEquals(asked?.mediaId, player.currentMediaItem?.mediaId)
        assertEquals("Track 2", player.mediaMetadata.title.toString())
        assertEquals(length, player.duration)
        assertEquals(listOf("Track 2"), changes.distinct())
        // Nor did the queue's entries change as it loaded (a car is sent the whole queue again when they do), nor what
        // the session may do: seeking was on offer all along.
        assertEquals(1, queues)
        assertTrue(seekable.all { it })
        // A queue described without covers' addresses shows the one the output fetched.
        assertTrue(player.mediaMetadata.artworkData != null)
    }

    @Test
    fun repeatShowsAndIsSetThroughTheMusic() {
        playback.play(album, startIndex = 2)
        engineHolds(3)
        player.queueChanged()
        idle()
        assertEquals(Player.REPEAT_MODE_OFF, player.repeatMode)
        assertFalse(player.hasNextMediaItem())

        player.repeatMode = Player.REPEAT_MODE_ALL
        idle()
        assertEquals(MusicRepeat.All, playback.queueState.value.repeat)
        player.queueChanged()
        idle()
        assertEquals(Player.REPEAT_MODE_ALL, player.repeatMode)
        // The last track's next one is the first.
        assertTrue(player.hasNextMediaItem())
        player.seekToNextMediaItem()
        idle()
        assertEquals(0, playback.queueState.value.index)

        playback.setRepeat(MusicRepeat.One)
        player.queueChanged()
        idle()
        assertEquals(Player.REPEAT_MODE_ONE, player.repeatMode)
        // The track repeating, the last one has no next one: not in the car, nor on the phone.
        playback.jumpTo(2)
        player.queueChanged()
        idle()
        assertFalse(player.hasNextMediaItem())
        assertFalse(playback.nowPlaying.value!!.hasNext)
    }

    @Test
    fun aTrackThatCantStartSaysWhyInPleysWords() {
        api.fail("startPlayback", OfflineException(java.io.IOException("refused")))
        playback.play(album)
        idle()
        player.queueChanged()
        idle()
        assertEquals(MusicError.Offline, playback.nowPlaying.value?.error)
        // No player failed: the session would say nothing plays, and nothing is wrong.
        assertEquals(Player.STATE_IDLE, player.playbackState)
        assertEquals("Pley: Offline", player.playerError?.message)
        assertEquals(PlaybackException.ERROR_CODE_IO_NETWORK_CONNECTION_FAILED, player.playerError?.errorCode)
        assertFalse(player.playWhenReady)
        assertEquals("track:1", player.currentMediaItem?.mediaId)

        // Play asks for it again: loading, and no error.
        api.reset()
        player.play()
        player.queueChanged()
        idle()
        assertNull(player.playerError)
    }

    @Test
    fun aTrackThatCantBePlayedTellsTheCarWhenTheNextOnePlays() {
        api.fail("startPlayback", RuntimeException("the server can't open the file"))
        playback.play(album)
        idle()
        player.queueChanged()
        idle()
        assertEquals("Pley: Unplayable (next in ${MusicPlayback.SKIP_AFTER_S})", player.playerError?.message)
    }

    @Test
    fun itemsSetFromOutsideChangeNothing() {
        playback.play(album)
        engineHolds(1)
        player.setMediaItem(MediaItem.fromUri("http://elsewhere/other"))
        idle()
        assertEquals(3, playback.queueState.value.tracks.size)
        assertEquals("http://server/stream", engine.currentMediaItem?.localConfiguration?.uri.toString())
    }

    private fun track(id: Long) = ItemSummary(
        id = id,
        libraryId = 3,
        kind = ItemKind.Track,
        title = "Track $id",
        parentId = 50,
        parentTitle = "Deep Water",
        grandparentTitle = "Harbour Lights",
        durationMs = 180_000,
    )

    /** An output that loads and plays nothing. */
    private class SilentOutput : MusicOutput {
        override fun load(source: EngineSource, startMs: Long, play: Boolean, info: TrackInfo) {
            playWhenReady = play
        }

        override fun play() = Unit

        override fun pause() = Unit

        override fun seekTo(positionMs: Long) = Unit

        override fun stop() = Unit

        override fun release() = Unit

        override val positionMs = 0L
        override val durationMs = 0L
        override val bufferedMs = 0L
        override var playWhenReady = false
            private set

        override fun setListener(listener: MusicOutput.Listener?) = Unit
    }
}
