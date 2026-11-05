package app.pley.android.screens.onboarding

import androidx.lifecycle.viewModelScope
import app.pley.android.common.FoundServer
import app.pley.android.common.SavedServer
import app.pley.android.network.ProbeResult
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

/**
 * The servers list: saved first, then found, the lookalike and
 * unencrypted tags, local network access before anything at home, typed
 * addresses (https first, the unencrypted warning), choosing, forgetting
 * and the demo server.
 */
@OptIn(ExperimentalCoroutinesApi::class)
class ServersViewModelTest {
    private val nas = "http://192.168.1.20:7539"
    private val office = "http://192.168.1.42:7539"
    private var vm: ServersViewModel? = null

    @Before
    fun setUp() {
        ServersViewModel.resetAsked()
    }

    @After
    fun tearDown() {
        vm?.viewModelScope?.cancel()
        Dispatchers.resetMain()
    }

    private fun TestScope.servers(source: FakeOnboardingSource, tv: Boolean = false): ServersViewModel {
        Dispatchers.setMain(StandardTestDispatcher(testScheduler))
        return ServersViewModel(source, explainFirst = tv).also { vm = it }
    }

    @Test
    fun savedServersComeFirstThenTheOnesFoundWithTheirPorts() = runTest {
        val source = FakeOnboardingSource(
            saved = listOf(SavedServer(nas, "Living room NAS", "a", lastUsed = 5)),
            found = listOf(
                FoundServer(office, "Office", "b", "1.0"),
                FoundServer("http://192.168.1.99:7539", "Living Room NAS", "c", "1.0"),
                FoundServer(nas, "Living room NAS", "a", "1.0"),
            ),
        )
        source.answer(nas)
        val vm = servers(source)
        advanceUntilIdle()
        val rows = vm.state.value.rows
        assertEquals(listOf(nas, office, "http://192.168.1.99:7539"), rows.map { it.url })
        assertEquals("192.168.1.20:7539", rows[0].address)
        assertTrue(rows[0].saved && rows[0].found)
        // Named like the saved server, and it isn't: tagged.
        assertTrue(rows[2].lookalike)
        assertFalse(rows[1].lookalike)
        assertTrue(vm.state.value.searched)
        assertEquals(nas, vm.state.value.selected)
        assertEquals("192.168.1.1-254", vm.state.value.sweepRange)
    }

    @Test
    fun aSavedServerThatDoesntAnswerSaysSo() = runTest {
        val source = FakeOnboardingSource(saved = listOf(SavedServer(nas, "Living room NAS", null, 1)))
        val vm = servers(source)
        advanceUntilIdle()
        assertTrue(vm.state.value.rows.single().unreachable)
    }

    @Test
    fun phonesAskForLocalNetworkAccessBeforeSearching() = runTest {
        val source = FakeOnboardingSource(found = listOf(FoundServer(nas, "Living room NAS", null, "1.0")))
        source.missing = listOf(ACCESS)
        val vm = servers(source)
        advanceUntilIdle()
        assertEquals(LocalAccess.Asking, vm.state.value.access)
        assertEquals(listOf(ACCESS), vm.state.value.ask)
        assertEquals(0, source.searches)
        // Allowed: the search runs.
        vm.asked()
        source.missing = emptyList()
        vm.permissionsAnswered()
        advanceUntilIdle()
        assertEquals(LocalAccess.Allowed, vm.state.value.access)
        assertEquals(1, source.searches)
        assertEquals(nas, vm.state.value.rows.single().url)
    }

    @Test
    fun aRefusalSaysHowToAllowItAndSettingsBringsItBack() = runTest {
        val source = FakeOnboardingSource()
        source.missing = listOf(ACCESS)
        val vm = servers(source)
        advanceUntilIdle()
        vm.permissionsAnswered()
        assertEquals(LocalAccess.Denied, vm.state.value.access)
        assertEquals(0, source.searches)
        // Choosing a server at home doesn't ask again in this process.
        vm.choose(nas)
        assertEquals(LocalAccess.Denied, vm.state.value.access)
        assertTrue(source.probed.isEmpty())
        // Allowed in Settings meanwhile.
        source.missing = emptyList()
        vm.resumed()
        advanceUntilIdle()
        assertEquals(LocalAccess.Allowed, vm.state.value.access)
        assertEquals(1, source.searches)
    }

    @Test
    fun tvsExplainBeforeTheSystemAsks() = runTest {
        val source = FakeOnboardingSource()
        source.missing = listOf(ACCESS, NEARBY)
        val vm = servers(source, tv = true)
        advanceUntilIdle()
        assertEquals(LocalAccess.Explain, vm.state.value.access)
        assertTrue(vm.state.value.ask.isEmpty())
        vm.explained()
        assertEquals(LocalAccess.Asking, vm.state.value.access)
        assertEquals(listOf(ACCESS, NEARBY), vm.state.value.ask)
    }

    @Test
    fun serversOnTheInternetNeedNoPermission() = runTest {
        val source = FakeOnboardingSource()
        source.missing = listOf(ACCESS)
        source.answer("pley.example.com", url = "https://pley.example.com", name = "Away")
        val vm = servers(source)
        advanceUntilIdle()
        vm.asked()
        vm.permissionsAnswered()
        vm.connect("pley.example.com", open = false)
        advanceUntilIdle()
        assertNull(vm.state.value.addressProblem)
        assertEquals("https://pley.example.com", vm.state.value.selected)
        val typed = vm.state.value.rows.single { it.url == "https://pley.example.com" }
        assertEquals("pley.example.com", typed.address)
        // Typed, not found: it isn't "on this network".
        assertFalse(typed.found)
    }

    @Test
    fun anUnencryptedAddressOnTheInternetIsWarnedAboutOnce() = runTest {
        val source = FakeOnboardingSource()
        source.internet = { true }
        source.answer("http://pley.example.com", url = "http://pley.example.com")
        val vm = servers(source)
        advanceUntilIdle()
        vm.connect("http://pley.example.com", open = true)
        advanceUntilIdle()
        assertEquals(ServersProblem.Unencrypted("pley.example.com"), vm.state.value.addressProblem)
        assertTrue(source.used.isEmpty())
        // Connect again: it goes ahead.
        vm.connect("http://pley.example.com", open = true)
        advanceUntilIdle()
        assertNull(vm.state.value.addressProblem)
        assertEquals("http://pley.example.com", source.used.single().url)
        assertTrue(vm.state.value.rows.single().unencrypted)
    }

    @Test
    fun whatIsntAnAddressOrIsntPleySaysSo() = runTest {
        val source = FakeOnboardingSource()
        source.answers["router"] = ProbeResult.NotPley("http://router:7539")
        val vm = servers(source)
        advanceUntilIdle()
        vm.connect("not an address", open = true)
        assertEquals(ServersProblem.BadAddress, vm.state.value.addressProblem)
        vm.connect("router", open = true)
        advanceUntilIdle()
        assertEquals(ServersProblem.NotPley("router:7539"), vm.state.value.addressProblem)
        vm.connect("192.168.1.77", open = true)
        advanceUntilIdle()
        assertEquals(ServersProblem.Unreachable("192.168.1.77:7539", NOTHING_ANSWERS), vm.state.value.addressProblem)
    }

    @Test
    fun choosingSavesTheServerAndSaysWhereToGo() = runTest {
        val source = FakeOnboardingSource(found = listOf(FoundServer(nas, "Living room NAS", "a", "1.0")))
        source.answer(nas)
        val vm = servers(source)
        advanceUntilIdle()
        vm.choose(nas)
        advanceUntilIdle()
        assertEquals(SavedServer(nas, "Living room NAS", "a", 0), source.used.single())
        assertEquals(ChosenServer(nas, "Living room NAS", signedIn = false, profiles = 1), vm.chosen.first())
        // Signed in there already, with a household of four.
        source.tokens += nas
        source.saveHousehold(nas, FakeOnboardingSource.household())
        vm.choose(nas)
        advanceUntilIdle()
        assertEquals(ChosenServer(nas, "Living room NAS", signedIn = true, profiles = 4), vm.chosen.first())
    }

    @Test
    fun theChosenServerSaysWhetherItRequiresTwoStepSignIn() = runTest {
        val source = FakeOnboardingSource(found = listOf(FoundServer(nas, "Living room NAS", "a", "1.0"), FoundServer(office, "Office", "b", "1.0")))
        source.answer(nas, twoStepRequired = true)
        source.answer(office)
        val vm = servers(source)
        advanceUntilIdle()
        // The first card is chosen, and asked.
        assertEquals(nas, vm.state.value.selected)
        assertTrue(vm.state.value.rows.single { it.url == nas }.twoStepRequired)
        assertFalse(vm.state.value.rows.single { it.url == office }.twoStepRequired)
        assertEquals(listOf(nas), source.probed)
        // Another card is asked once chosen; each server once.
        vm.select(office)
        advanceUntilIdle()
        vm.select(nas)
        advanceUntilIdle()
        assertEquals(listOf(nas, office), source.probed)
        assertFalse(vm.state.value.rows.single { it.url == office }.twoStepRequired)
        assertTrue(vm.state.value.rows.single { it.url == nas }.twoStepRequired)
    }

    @Test
    fun twoStepSignInIsAskedOnlyOnceTheServerMayBeReached() = runTest {
        val source = FakeOnboardingSource(saved = listOf(SavedServer(nas, "Living room NAS", null, 1)))
        source.missing = listOf(ACCESS)
        source.answer(nas, twoStepRequired = true)
        val vm = servers(source)
        advanceUntilIdle()
        assertEquals(nas, vm.state.value.selected)
        assertTrue(source.probed.isEmpty())
        assertFalse(vm.state.value.rows.single().twoStepRequired)
        // Allowed: it is asked.
        vm.asked()
        source.missing = emptyList()
        vm.permissionsAnswered()
        advanceUntilIdle()
        assertTrue(vm.state.value.rows.single().twoStepRequired)
    }

    @Test
    fun forgettingASavedServerEndsItsSignIn() = runTest {
        val source = FakeOnboardingSource(saved = listOf(SavedServer(nas, "Living room NAS", null, 1)))
        source.answer(nas)
        val vm = servers(source)
        advanceUntilIdle()
        vm.forget(nas)
        advanceUntilIdle()
        assertEquals(listOf(nas), source.forgotten)
        assertTrue(vm.state.value.rows.isEmpty())
    }

    @Test
    fun searchThisNetworkAddsWhatAnswers() = runTest {
        val source = FakeOnboardingSource()
        source.sweepFinds = listOf(FoundServer("http://192.168.1.50:7539", "Docker", null, "1.0"))
        val vm = servers(source, tv = true)
        advanceUntilIdle()
        vm.searchNetwork()
        advanceUntilIdle()
        assertFalse(vm.state.value.sweeping)
        assertEquals("Docker", vm.state.value.rows.single().name)
    }

    @Test
    fun theDemoServerIsOfferedOnlyWhenTheBuildHasIt() = runTest {
        val none = servers(FakeOnboardingSource())
        assertFalse(none.state.value.demo)
        none.viewModelScope.cancel()

        val source = FakeOnboardingSource()
        source.demoUrl = "http://127.0.0.1:41234"
        source.answer("http://127.0.0.1:41234", name = "Pley demo")
        val vm = servers(source)
        advanceUntilIdle()
        assertTrue(vm.state.value.demo)
        vm.useDemo()
        advanceUntilIdle()
        assertEquals(ChosenServer("http://127.0.0.1:41234", "Pley demo", signedIn = false, profiles = 1), vm.chosen.first())
    }

    private companion object {
        const val ACCESS = "android.permission.ACCESS_LOCAL_NETWORK"
        const val NEARBY = "android.permission.NEARBY_WIFI_DEVICES"
    }
}
