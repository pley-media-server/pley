package app.pley.android.screens.cast

import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.ViewModelStore
import androidx.lifecycle.viewmodel.CreationExtras
import app.pley.android.data.CastPlaying
import app.pley.android.model.CastAction
import app.pley.android.model.CastPlayState
import app.pley.android.player.PlayRequest
import app.pley.android.testing.MainDispatcherRule
import java.io.IOException
import kotlin.reflect.KClass
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test

/** Casting from a phone: the Cast button, "Play on", a device's remote and the bar. */
@OptIn(ExperimentalCoroutinesApi::class)
class CastViewModelTest {
    @get:Rule
    val main = MainDispatcherRule(StandardTestDispatcher())

    private val store = ViewModelStore()

    @After
    fun clear() = store.clear()

    private fun castTest(block: suspend TestScope.() -> Unit) = runTest(main.dispatcher) {
        try {
            block()
        } finally {
            store.clear()
        }
    }

    private class Setup(val cast: FakeCast, val launcher: RecordingLauncher, val vm: CastViewModel)

    private fun TestScope.setup(): Setup {
        val cast = FakeCast { testScheduler.currentTime }
        val launcher = RecordingLauncher()
        val factory = object : ViewModelProvider.Factory {
            @Suppress("UNCHECKED_CAST")
            override fun <V : ViewModel> create(modelClass: KClass<V>, extras: CreationExtras): V = CastViewModel(cast, launcher) as V
        }
        val vm = ViewModelProvider.create(store, factory)["cast", CastViewModel::class]
        runCurrent()
        return Setup(cast, launcher, vm)
    }

    @Test
    fun `the Cast button shows only while the profile has a device to cast to`() = castTest {
        val s = setup()
        assertFalse(s.vm.available.value)
        s.cast.devices.value = listOf(FakeCast.tv())
        runCurrent()
        assertTrue(s.vm.available.value)
        s.cast.devices.value = emptyList()
        runCurrent()
        assertFalse(s.vm.available.value)
    }

    @Test
    fun `from the player the device gets the position and version, from a title page neither`() = castTest {
        val s = setup()
        val tv = FakeCast.tv()
        s.cast.devices.value = listOf(tv)
        var left = 0
        s.vm.pick(tv, 101, fileId = 1001, positionMs = 754_000) { left++ }
        runCurrent()
        assertEquals(FakeCast.Played(tv.id, 101, 1001, 754_000), s.cast.played.single())
        assertEquals(1, left)

        s.vm.pick(tv, 203)
        runCurrent()
        assertEquals(FakeCast.Played(tv.id, 203, null, null), s.cast.played.last())
    }

    @Test
    fun `a device that plays something shows its remote instead of being sent the title`() = castTest {
        val s = setup()
        val busy = FakeCast.tv().copy(playing = CastPlaying(FakeCast.MOVIE, 0, null, CastPlayState.Playing, 0))
        s.cast.devices.value = listOf(busy)
        var left = 0
        s.vm.pick(busy, 203, positionMs = 1_000) { left++ }
        runCurrent()
        assertTrue(s.cast.played.isEmpty())
        assertEquals(busy.id, s.cast.sent.value?.deviceId)
        assertNull(s.cast.sent.value?.itemId)
        assertEquals(1, left)
    }

    @Test
    fun `a title that can't be sent says so, and the player stays`() = castTest {
        val s = setup()
        val tv = FakeCast.tv()
        s.cast.devices.value = listOf(tv)
        s.cast.failure = IOException("unreachable")
        var left = 0
        s.vm.pick(tv, 101) { left++ }
        runCurrent()
        assertEquals(0, left)
        assertEquals(CastProblem.NotSent("Living room"), s.vm.problem.value)
        s.vm.dismissProblem()
        assertNull(s.vm.problem.value)
    }

    @Test
    fun `the remote waits for the device to say it plays the title, then says it didn't answer`() = castTest {
        val s = setup()
        val tv = FakeCast.tv()
        s.cast.devices.value = listOf(tv)
        s.vm.pick(tv, 101)
        s.vm.openRemote(tv.id)
        runCurrent()
        assertEquals(CastRemoteStatus.Waiting, s.vm.remote.value?.status)

        advanceTimeBy(CastViewModel.ANSWER_MS - 1)
        runCurrent()
        assertEquals(CastRemoteStatus.Waiting, s.vm.remote.value?.status)
        advanceTimeBy(2)
        runCurrent()
        assertEquals(CastRemoteStatus.NoAnswer, s.vm.remote.value?.status)

        // A late answer still counts.
        s.cast.says(tv.id, FakeCast.MOVIE)
        runCurrent()
        assertTrue(s.vm.remote.value?.status is CastRemoteStatus.Playing)
    }

    @Test
    fun `an answer in time shows what it plays, and then that it plays nothing`() = castTest {
        val s = setup()
        val tv = FakeCast.tv()
        s.cast.devices.value = listOf(tv)
        s.vm.pick(tv, 101)
        s.vm.openRemote(tv.id)
        runCurrent()
        advanceTimeBy(2_000)
        s.cast.says(tv.id, FakeCast.MOVIE, state = CastPlayState.Buffering)
        runCurrent()
        val status = s.vm.remote.value?.status as CastRemoteStatus.Playing
        assertEquals(CastPlayState.Buffering, status.playing.state)
        advanceTimeBy(CastViewModel.ANSWER_MS)
        runCurrent()
        assertTrue(s.vm.remote.value?.status is CastRemoteStatus.Playing)

        s.cast.says(tv.id, null)
        runCurrent()
        assertEquals(CastRemoteStatus.Idle, s.vm.remote.value?.status)
        // It left the list: its remote goes.
        s.cast.devices.value = emptyList()
        runCurrent()
        assertNull(s.vm.remote.value)
        s.cast.devices.value = listOf(tv)
        runCurrent()
        assertNull(s.vm.remote.value)
    }

    @Test
    fun `the position moves on while it plays, and the skips seek from there`() = castTest {
        val s = setup()
        val tv = FakeCast.tv()
        s.cast.devices.value = listOf(tv)
        s.cast.says(tv.id, FakeCast.MOVIE, positionMs = 60_000)
        runCurrent()
        val playing = s.cast.devices.value.single()
        advanceTimeBy(5_000)
        assertEquals(65_000, playing.playing!!.positionAt(s.vm.now()))

        s.vm.skip(playing, -10_000)
        s.vm.skip(playing, 30_000)
        runCurrent()
        assertEquals(listOf(55_000L, 95_000L), s.cast.controls.map { it.third })
        assertTrue(s.cast.controls.all { it.second == CastAction.Seek })
        // Never before the start or past the end.
        s.vm.skip(playing, -120_000)
        s.vm.seek(playing, 10_000_000)
        runCurrent()
        assertEquals(listOf(0L, 6_000_000L), s.cast.controls.drop(2).map { it.third })
    }

    @Test
    fun `play here stops the device and plays on the phone from where it was`() = castTest {
        val s = setup()
        val tv = FakeCast.tv()
        s.cast.devices.value = listOf(tv)
        s.cast.says(tv.id, FakeCast.MOVIE, positionMs = 60_000, state = CastPlayState.Paused)
        s.vm.openRemote(tv.id)
        runCurrent()
        advanceTimeBy(5_000)
        s.vm.playHere(s.cast.devices.value.single())
        runCurrent()
        assertEquals(Triple(tv.id, CastAction.Stop, null), s.cast.controls.single())
        assertEquals(PlayRequest(101, startMs = 60_000), s.launcher.played.single())
        assertNull(s.vm.remote.value)
    }

    @Test
    fun `the bar shows the device last cast to while it plays something`() = castTest {
        val s = setup()
        val tv = FakeCast.tv()
        val bedroom = FakeCast.tv("tv-2", "Bedroom")
        s.cast.devices.value = listOf(tv, bedroom)
        s.cast.says(bedroom.id, FakeCast.MOVIE)
        runCurrent()
        // Nothing was cast from here.
        assertNull(s.vm.bar.value)

        s.vm.pick(tv, 101)
        runCurrent()
        assertNull(s.vm.bar.value)
        s.cast.says(tv.id, FakeCast.MOVIE)
        runCurrent()
        assertEquals(tv.id, s.vm.bar.value?.id)
        s.vm.pause(s.vm.bar.value!!)
        runCurrent()
        assertEquals(Triple(tv.id, CastAction.Pause, null), s.cast.controls.single())

        s.cast.says(tv.id, null)
        runCurrent()
        assertNull(s.vm.bar.value)
    }
}
