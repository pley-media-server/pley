@file:OptIn(ExperimentalCoroutinesApi::class)

package app.pley.android.player.music

import app.pley.android.data.ProfileScope
import app.pley.android.model.ItemKind
import app.pley.android.model.ItemSummary
import app.pley.android.model.PleyJson
import app.pley.android.model.caps.ClientCaps
import app.pley.android.player.MusicQueueState
import app.pley.android.player.MusicRepeat
import app.pley.android.player.session.SessionReporter
import app.pley.android.testing.network.FakePleyApi
import kotlin.random.Random
import kotlin.coroutines.CoroutineContext
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.async
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.drop
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.launch
import kotlinx.coroutines.plus
import kotlinx.serialization.builtins.ListSerializer
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.UnconfinedTestDispatcher
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
        work = offMain(),
    )

    /** Another dispatcher than the test's, as the app's JSON is made off the main thread, run as the test's runs. */
    private fun TestScope.offMain() = StandardTestDispatcher(testScheduler)

    private val album = (1..12L).map { track(it) }

    @Test
    fun aNewProcessWaitsForTheSignInToBeReadBackBeforeLookingForWhatWasKept() = runTest {
        resume().apply {
            keep(MusicQueueState(album, index = 4), positionMs = 61_000)
            keepRadio("channel:7@favorites")
        }
        runCurrent()

        // Android starts the process again for a car: the sign-in is still being read, and no profile is in use yet.
        signedIn = null
        val read = CompletableDeferred<Unit>()
        val again = MusicResume(
            profile = { signedIn },
            read = { whose, key -> snapshotsOf(whose)[key]?.takeIf { it.isNotEmpty() } },
            write = { _, _, _ -> },
            scope = backgroundScope,
            signInRead = { read.await() },
            work = offMain(),
        )
        val music = async { again.saved() }
        val radio = async { again.savedRadio() }
        runCurrent()
        assertTrue(music.isActive && radio.isActive)

        signedIn = PARENT
        read.complete(Unit)
        assertEquals(4, music.await()!!.index)
        assertEquals("channel:7@favorites", radio.await())
    }

    @Test
    fun aSavedPlaceThatCantBeReadIsNothingKeptRatherThanACrash() = runTest {
        resume().keep(MusicQueueState(album, index = 4), positionMs = 61_000)
        runCurrent()
        val failing = MusicResume(
            profile = { signedIn },
            read = { whose, key -> if (key == MusicResume.PLACE) error("The disk said no") else snapshotsOf(whose)[key] },
            write = { _, _, _ -> },
            scope = backgroundScope,
            work = offMain(),
        )
        assertNull(failing.saved())
    }

    @Test
    fun theQueuesJsonIsMadeAndReadOffTheMainThreadAndWrittenInOrder() = runTest {
        // Off the main thread: a dispatcher of its own, run only when the test says.
        val elsewhere = Held()
        val resume = MusicResume(
            profile = { signedIn },
            read = { whose, key -> snapshotsOf(whose)[key]?.takeIf { it.isNotEmpty() } },
            write = { whose, key, bytes ->
                written += key
                snapshotsOf(whose)[key] = bytes
            },
            scope = backgroundScope,
            work = elsewhere,
        )
        resume.keep(MusicQueueState(album, index = 4), positionMs = 61_000)
        resume.keep(MusicQueueState(album, index = 5), positionMs = 0)
        runCurrent()
        // The main thread made nothing: the queue's JSON waits for the other side.
        assertTrue(written.isEmpty())

        repeat(3) {
            elsewhere.run()
            runCurrent()
        }
        // The queue once, then the order it was given in, then the latest place.
        assertEquals(listOf(MusicResume.QUEUE, MusicResume.GIVEN, MusicResume.PLACE), written)
        val saved = async { resume.saved() }
        repeat(3) {
            runCurrent()
            elsewhere.run()
        }
        assertEquals(album, saved.await()!!.tracks)
        assertEquals(5, saved.await()!!.index)
    }

    @Test
    fun aQueueBeingMadeAsTheProfileSignsOutIsNotWritten() = runTest {
        val elsewhere = Held()
        val resume = MusicResume(
            profile = { signedIn },
            read = { whose, key -> snapshotsOf(whose)[key] },
            write = { _, key, _ -> written += key },
            scope = backgroundScope,
            work = elsewhere,
        )
        resume.keep(MusicQueueState(album, index = 4), positionMs = 61_000)
        runCurrent()
        // Signed out, then in again, while the queue's JSON is being made: the saved things went with the sign-in.
        resume.signedOut()
        repeat(3) {
            elsewhere.run()
            runCurrent()
        }
        assertTrue(written.isEmpty())
    }

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
    fun aQueueKeptWithRepeatOnIsNeverWrittenEmptyAsItComesBack() = runTest {
        resume().keep(MusicQueueState(album, index = 4, repeat = MusicRepeat.All), positionMs = 61_000)
        runCurrent()

        // Another process: the music, and what keeps it as the app wires them (ServiceMusicControl), the queue kept as it
        // changes, on the main thread there and then, and written from there.
        val main = UnconfinedTestDispatcher(testScheduler)
        val queues = mutableListOf<Int>()
        val again = MusicResume(
            profile = { signedIn },
            read = { whose, key -> snapshotsOf(whose)[key]?.takeIf { it.isNotEmpty() } },
            write = { whose, key, bytes ->
                if (key == MusicResume.QUEUE) queues += bytes.size
                snapshotsOf(whose)[key] = bytes
            },
            scope = backgroundScope + main,
            work = offMain(),
        )
        val server = FakePleyApi()
        val music = MusicPlayback(
            MusicEnvironment(
                api = { server },
                caps = { ClientCaps() },
                clientId = { "android-test-music" },
                reporter = SessionReporter(backgroundScope),
                output = { error("Nothing plays here") },
            ),
            backgroundScope,
        )
        val keep = { if (!music.suggested) again.keep(music.queueState.value, music.position().positionMs) }
        backgroundScope.launch(main) { music.queueState.drop(1).collect { keep() } }
        backgroundScope.launch(main) {
            music.nowPlaying.map { now -> now?.let { Triple(it.wantsPlay, it.seeks, it.ended) } }.distinctUntilChanged().drop(1).collect { keep() }
        }

        music.cue(again.saved()!!)
        runCurrent()
        assertEquals(MusicRepeat.All, music.queueState.value.repeat)
        // Android may end the app at any moment: the queue is in the store at each, never written as none.
        assertTrue("The queue's writes, by their size: $queues", queues.isNotEmpty() && queues.none { it == 0 })
        val back = resume().saved()!!
        assertEquals(album, back.tracks)
        assertEquals(4, back.index)
        assertEquals(61_000L, back.positionMs)
        assertEquals(MusicRepeat.All, back.repeat)
    }

    @Test
    fun theRadiosChannelIsKeptUntilMusicPlaysAgainAndWrittenOnlyWhenItChanges() = runTest {
        val first = resume()
        assertEquals(null, first.savedRadio())
        first.keepRadio("channel:7@favorites")
        first.keepRadio("channel:7@favorites")
        runCurrent()
        assertEquals(1, written.count { it == MusicResume.RADIO })
        // Another process reads it back; the music kept beside it stays.
        first.keep(MusicQueueState(album, index = 1), positionMs = 0)
        runCurrent()
        val second = resume()
        assertEquals("channel:7@favorites", second.savedRadio())
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
    fun afterASignOutThatLeftTheMusicInItsQueueItIsWrittenWholeAgainAndNothingOnItsWayIsWritten() = runTest {
        val resume = resume()
        resume.keep(MusicQueueState(album, index = 2), positionMs = 0)
        runCurrent()

        // The server ends the sign-in as the place is on its way to the store: the saved things go with the sign-in.
        resume.keep(MusicQueueState(album, index = 3), positionMs = 0)
        signedIn = null
        stores.clear()
        resume.signedOut()
        runCurrent()
        assertTrue(stores.values.all { it.isEmpty() })

        // Signed in again, the music played on: its tracks are written again, not only its place.
        signedIn = PARENT
        resume.keep(MusicQueueState(album, index = 4), positionMs = 0)
        runCurrent()
        assertEquals(album, resume().saved()!!.tracks)
        assertEquals(4, resume().saved()!!.index)

        // Signed in as another profile instead, the queue left playing isn't given to it.
        signedIn = null
        resume.signedOut()
        signedIn = CHILD
        resume.keep(MusicQueueState(album, index = 5), positionMs = 0)
        runCurrent()
        assertNull(resume().saved())
    }

    @Test
    fun musicPlayingOnAfterASwitchLeavesTheChannelTheProfileSignedInPlayedLast() = runTest {
        signedIn = CHILD
        resume().keepRadio("channel:7")
        runCurrent()
        signedIn = PARENT
        val resume = resume()
        resume.keep(MusicQueueState(album, index = 2), positionMs = 0)
        runCurrent()

        // The first profile's album, played on as the other: that profile's car is still shown its channel.
        signedIn = CHILD
        resume.keepRadio(null)
        runCurrent()
        assertEquals("channel:7", resume().savedRadio())

        // Music it starts itself is what it played last.
        resume.keep(MusicQueueState((101..110L).map { track(it) }), positionMs = 0)
        resume.keepRadio(null)
        runCurrent()
        assertNull(resume().savedRadio())
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
        resume.keepRadio("channel:7@favorites")
        runCurrent()
        signedIn = CHILD
        assertNull(resume.savedRadio())
        // Music played as the other profile: its radio is none, and the first profile's stays.
        resume.keepRadio(null)
        runCurrent()
        signedIn = PARENT
        assertEquals("channel:7@favorites", resume().savedRadio())
        // Signed out, nothing is kept or read.
        signedIn = null
        val writes = written.size
        resume.keep(MusicQueueState(album), positionMs = 0)
        resume.keepRadio("channel:8")
        runCurrent()
        assertEquals(writes, written.size)
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
        // A queue that reads, but of something other than tracks, isn't music's.
        val films = listOf(track(1).copy(kind = ItemKind.Movie), track(2))
        store[MusicResume.QUEUE] = PleyJson.encodeToString(ListSerializer(ItemSummary.serializer()), films).encodeToByteArray()
        assertNull(resume.saved())
        store[MusicResume.QUEUE] = PleyJson.encodeToString(ListSerializer(ItemSummary.serializer()), listOf(track(1), track(2))).encodeToByteArray()
        assertTrue(resume.saved() != null)
    }

    /** A dispatcher whose work waits until [run]. */
    private class Held : CoroutineDispatcher() {
        private val waiting = ArrayDeque<Runnable>()

        override fun dispatch(context: CoroutineContext, block: Runnable) {
            waiting += block
        }

        fun run() {
            while (waiting.isNotEmpty()) waiting.removeFirst().run()
        }
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
