@file:OptIn(ExperimentalCoroutinesApi::class)

package app.pley.android.player.music

import app.pley.android.data.ProfileScope
import app.pley.android.model.ItemKind
import app.pley.android.model.ItemSummary
import app.pley.android.player.MusicQueueState
import app.pley.android.player.MusicRepeat
import kotlin.random.Random
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * What music was playing, kept for Play to bring back once the app has
 * left memory: the queue and the place in it, read back by another run.
 */
class MusicResumeTest {
    /** Each profile's snapshots; the keys written, in order. */
    private val stores = HashMap<ProfileScope, HashMap<String, ByteArray>>()
    private val written = mutableListOf<String>()

    /** The profile signed in. */
    private var signedIn: ProfileScope? = PARENT

    /** The snapshots of the profile most tests play as. */
    private val store: HashMap<String, ByteArray> get() = snapshotsOf(PARENT)

    private fun snapshotsOf(profile: ProfileScope) = stores.getOrPut(profile) { HashMap() }

    private fun TestScope.resume() = MusicResume(
        profile = { signedIn },
        read = { whose, key -> snapshotsOf(whose)[key]?.takeIf { it.isNotEmpty() } },
        write = { whose, key, bytes ->
            written += key
            snapshotsOf(whose)[key] = bytes
        },
        scope = backgroundScope,
    )

    private val album = (1..12L).map { track(it) }

    @Test
    fun whatWasPlayingIsReadBackByAnotherRunWhereItWas() = runTest {
        val first = resume()
        first.keep(MusicQueueState(album, index = 4, repeat = MusicRepeat.All), positionMs = 61_000)
        runCurrent()

        // Another process: nothing in memory, the store alone.
        val saved = resume().saved()!!
        assertEquals(album, saved.tracks)
        assertEquals(4, saved.index)
        assertEquals(61_000L, saved.positionMs)
        assertEquals(MusicRepeat.All, saved.repeat)
    }

    @Test
    fun theRadiosChannelIsKeptUntilMusicPlaysAgainAndWrittenOnlyWhenItChanges() = runTest {
        val first = resume()
        assertEquals(null, first.savedRadio())
        first.keepRadio("channel:7@favourites")
        first.keepRadio("channel:7@favourites")
        runCurrent()
        assertEquals(1, written.count { it == MusicResume.RADIO })
        // Another process reads it back; the music kept beside it stays.
        first.keep(MusicQueueState(album, index = 1), positionMs = 0)
        runCurrent()
        val second = resume()
        assertEquals("channel:7@favourites", second.savedRadio())
        assertEquals(1, second.saved()!!.index)

        second.keepRadio(null)
        runCurrent()
        assertEquals(null, resume().savedRadio())
        assertEquals(1, resume().saved()!!.index)
    }

    @Test
    fun thePlaceIsWrittenAsItMovesAndTheQueueOnlyWhenItChanges() = runTest {
        val resume = resume()
        resume.keep(MusicQueueState(album, index = 0), positionMs = 0)
        runCurrent()
        resume.keep(MusicQueueState(album.toList(), index = 0), positionMs = 15_000)
        resume.keep(MusicQueueState(album.toList(), index = 1), positionMs = 0)
        runCurrent()
        assertEquals(1, written.count { it == MusicResume.QUEUE })
        assertEquals(1, resume.saved()!!.index)

        resume.keep(MusicQueueState(album.reversed(), index = 1), positionMs = 0)
        runCurrent()
        assertEquals(2, written.count { it == MusicResume.QUEUE })
        assertEquals(11L, resume.saved()!!.tracks[1].id)
    }

    @Test
    fun aQueueStoppedIsForgottenAndTheEmptyOneAProcessStartsWithForgetsNothing() = runTest {
        resume().keep(MusicQueueState(album, index = 2), positionMs = 5_000)
        runCurrent()
        // A new run keeps nothing until music plays: what the last one kept stays.
        val next = resume()
        assertEquals(2, next.saved()?.index)

        next.keep(MusicQueueState(), positionMs = 0)
        runCurrent()
        assertNull(next.saved())
        assertNull(resume().saved())
    }

    @Test
    fun musicThatPlaysOnAfterASwitchOfProfileIsKeptForNeither() = runTest {
        val resume = resume()
        resume.keep(MusicQueueState(album, index = 2), positionMs = 5_000)
        runCurrent()

        // Another profile now, the album playing on and changed: that profile is never shown it.
        signedIn = CHILD
        resume.keep(MusicQueueState(album.reversed(), index = 3), positionMs = 0)
        resume.keep(MusicQueueState(album.reversed(), index = 4), positionMs = 30_000)
        runCurrent()
        assertNull(resume().saved())
        assertTrue(snapshotsOf(CHILD).isEmpty())

        // Stopped there: the profile that played it keeps what it had.
        resume.keep(MusicQueueState(), positionMs = 0)
        runCurrent()
        signedIn = PARENT
        val kept = resume().saved()!!
        assertEquals(album, kept.tracks)
        assertEquals(2, kept.index)
        assertEquals(5_000L, kept.positionMs)

        // What the other profile plays next is its own.
        signedIn = CHILD
        resume.keep(MusicQueueState(album, index = 7), positionMs = 0)
        runCurrent()
        assertEquals(7, resume().saved()!!.index)
        signedIn = PARENT
        assertEquals(2, resume().saved()!!.index)
    }

    @Test
    fun musicStartedAfterASwitchOfProfileIsKeptForTheProfileSignedIn() = runTest {
        val resume = resume()
        resume.keep(MusicQueueState(album, index = 2), positionMs = 5_000)
        runCurrent()

        // Another album, started as the other profile while the first one played: that profile's from then on.
        signedIn = CHILD
        val other = (101..110L).map { track(it) }
        resume.keep(MusicQueueState(other, index = 0), positionMs = 0)
        resume.keep(MusicQueueState(other, index = 3), positionMs = 20_000)
        runCurrent()
        val childs = resume().saved()!!
        assertEquals(other, childs.tracks)
        assertEquals(3, childs.index)
        signedIn = PARENT
        val parents = resume().saved()!!
        assertEquals(album, parents.tracks)
        assertEquals(2, parents.index)
    }

    @Test
    fun whatIsOnItsWayToTheStoreAsTheProfileChangesGoesToTheProfileItIsFor() = runTest {
        val resume = resume()
        resume.keep(MusicQueueState(album, index = 1), positionMs = 0)
        // The profile changes before the store is written to.
        signedIn = CHILD
        runCurrent()
        assertTrue(snapshotsOf(CHILD).isEmpty())
        signedIn = PARENT
        assertEquals(1, resume().saved()!!.index)
    }

    @Test
    fun theRadiosChannelIsKeptForTheProfileThatPlayedIt() = runTest {
        val resume = resume()
        resume.keepRadio("channel:7@favourites")
        runCurrent()
        signedIn = CHILD
        assertNull(resume.savedRadio())
        // Music played as the other profile: its radio is none, and the first profile's stays.
        resume.keepRadio(null)
        runCurrent()
        signedIn = PARENT
        assertEquals("channel:7@favourites", resume().savedRadio())
        // Signed out, nothing is kept or read.
        signedIn = null
        resume.keep(MusicQueueState(album), positionMs = 0)
        runCurrent()
        assertNull(resume.saved())
        assertNull(resume.savedRadio())
    }

    @Test
    fun aShuffledQueueComesBackShuffledAndShuffleOffPlaysItInTheOrderGiven() = runTest {
        val queue = MusicQueue(Random(3)).apply { start(album, startIndex = 5, shuffle = true) }
        resume().keep(queue.state, positionMs = 0)
        runCurrent()
        val saved = resume().saved()!!
        assertEquals(queue.tracks, saved.tracks)
        val back = MusicQueue().apply { assertTrue(restore(saved.tracks, saved.given!!, saved.index)) }
        back.setShuffle(false)
        assertEquals(album, back.tracks)
        assertEquals(6L, back.current?.id)

        // Of a long queue, the tracks kept come back in the order given among themselves.
        val long = (1..1_200L).map { track(it) }
        val big = MusicQueue(Random(5)).apply { start(long, startIndex = 600, shuffle = true) }
        resume().keep(big.state, positionMs = 0)
        runCurrent()
        val window = resume().saved()!!
        assertEquals(MusicResume.MAX_TRACKS, window.tracks.size)
        val restored = MusicQueue().apply { assertTrue(restore(window.tracks, window.given!!, window.index)) }
        restored.setShuffle(false)
        assertEquals(window.tracks.sortedBy { it.id }, restored.tracks)
        assertEquals(601L, restored.current?.id)

        // In the order given, none is kept; nor one that doesn't place each track once.
        resume().keep(MusicQueueState(album, index = 1), positionMs = 0)
        runCurrent()
        assertNull(resume().saved()!!.given)
        store[MusicResume.GIVEN] = "0 0 1 2 3 4 5 6 7 8 9 10".encodeToByteArray()
        assertNull(resume().saved()!!.given)
    }

    @Test
    fun aLongQueueKeepsTheTracksAroundTheOnePlaying() = runTest {
        val long = (1..1_200L).map { track(it) }
        val resume = resume()
        resume.keep(MusicQueueState(long, index = 400), positionMs = 0)
        runCurrent()
        val saved = resume.saved()!!
        assertEquals(MusicResume.MAX_TRACKS, saved.tracks.size)
        // The same track, with some before it for Previous and the rest after.
        assertEquals(401L, saved.tracks[saved.index].id)
        assertEquals(50, saved.index)

        // Still within what was kept: only the place moves.
        resume.keep(MusicQueueState(long, index = 401), positionMs = 0)
        runCurrent()
        assertEquals(402L, resume.saved()!!.let { it.tracks[it.index].id })
        assertEquals(1, written.count { it == MusicResume.QUEUE })
    }

    @Test
    fun aTrackLeftAtItsEndStartsOverAndWhatCantBeReadIsNothing() = runTest {
        val resume = resume()
        resume.keep(MusicQueueState(album, index = 11), positionMs = 179_000)
        runCurrent()
        assertEquals(0L, resume.saved()!!.positionMs)

        store[MusicResume.PLACE] = "99 -4 Sideways".encodeToByteArray()
        val odd = resume.saved()!!
        assertEquals(0, odd.index)
        assertEquals(0L, odd.positionMs)
        assertEquals(MusicRepeat.Off, odd.repeat)

        store[MusicResume.QUEUE] = "not a queue".encodeToByteArray()
        assertNull(resume.saved())
        // A queue of something other than tracks isn't music's.
        store[MusicResume.QUEUE] = store.getValue(MusicResume.PLACE)
        assertTrue(resume.saved() == null)
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

    private companion object {
        val PARENT = ProfileScope("https://pley.example", 1)
        val CHILD = ProfileScope("https://pley.example", 2)
    }
}
