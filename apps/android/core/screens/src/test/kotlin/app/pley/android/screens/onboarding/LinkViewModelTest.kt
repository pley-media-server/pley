package app.pley.android.screens.onboarding

import androidx.lifecycle.viewModelScope
import app.pley.android.auth.AuthException
import app.pley.android.auth.DevicePoll
import app.pley.android.model.Me
import app.pley.android.screens.PleyKey
import java.io.IOException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The link screen's code, with a fake device grant and the test's clock:
 * polling only while started, declines, silent renewals (three), expiry
 * on return, network trouble and the half second of "Signed in as".
 */
@OptIn(ExperimentalCoroutinesApi::class)
class LinkViewModelTest {
    private val server = FakeOnboardingSource.SERVER
    private val source = FakeOnboardingSource()
    private var vm: LinkViewModel? = null

    @After
    fun tearDown() {
        vm?.viewModelScope?.cancel()
        Dispatchers.resetMain()
        ProfileGate.passed = false
    }

    private fun TestScope.link(purpose: LinkPurpose = LinkPurpose.SignIn, requests: Boolean = true): LinkViewModel {
        Dispatchers.setMain(StandardTestDispatcher(testScheduler))
        return LinkViewModel(source, server, purpose, requests) { testScheduler.currentTime }.also { vm = it }
    }

    @Test
    fun showsTheCodeTheAddressWithItsPortAndWaits() = runTest {
        val vm = link()
        assertEquals(LinkStatus.Starting, vm.state.value.status)
        vm.started()
        runCurrent()
        val code = vm.state.value.code!!
        assertEquals("BCDF-GHJK", code.userCode)
        assertEquals(listOf("BCDF", "GHJK"), code.halves)
        assertEquals("192.168.1.20:7539/link", code.address)
        assertEquals("$server/link?code=BCDFGHJK", code.qr)
        assertEquals(LinkStatus.Waiting, vm.state.value.status)
        assertEquals(1, source.polling)
    }

    @Test
    fun aTvsCodeAsksForNoScopesOfRequests() = runTest {
        val phone = link()
        phone.started()
        runCurrent()
        phone.viewModelScope.cancel()
        link(requests = false).started()
        runCurrent()
        assertEquals(listOf(true, false), source.requestScopes)
    }

    @Test
    fun saysWhenTheServerRequiresTwoStepSignIn() = runTest {
        source.answer(server, twoStepRequired = true)
        val vm = link()
        assertFalse(vm.state.value.twoStepRequired)
        vm.started()
        runCurrent()
        assertTrue(vm.state.value.twoStepRequired)
        // Beside the code, which didn't wait for it.
        assertEquals("BCDF-GHJK", vm.state.value.code?.userCode)
        assertEquals(LinkStatus.Waiting, vm.state.value.status)
        assertEquals(listOf(server), source.probed)
    }

    @Test
    fun saysNothingOfTwoStepSignInOtherwise() = runTest {
        // Not required, or a server from before it.
        source.answer(server)
        val notRequired = link()
        runCurrent()
        assertFalse(notRequired.state.value.twoStepRequired)
        notRequired.viewModelScope.cancel()
        // No answer.
        source.answers.clear()
        val unreachable = link()
        runCurrent()
        assertFalse(unreachable.state.value.twoStepRequired)
        unreachable.viewModelScope.cancel()
        // A profile's approval doesn't ask.
        source.answer(server, twoStepRequired = true)
        source.probed.clear()
        val approve = link(LinkPurpose.Profile(3))
        runCurrent()
        assertFalse(approve.state.value.twoStepRequired)
        assertTrue(source.probed.isEmpty())
    }

    @Test
    fun theAddressKeepsItsPortAndDropsItsScheme() {
        assertEquals("192.168.1.20:7539/link", LinkViewModel.addressOf("http://192.168.1.20:7539/link"))
        assertEquals("pley.example.com/link", LinkViewModel.addressOf("https://pley.example.com/link"))
        assertEquals("nas:8080/link", LinkViewModel.addressOf("http://nas:8080/link"))
    }

    @Test
    fun pollsOnlyWhileStarted() = runTest {
        val vm = link()
        vm.started()
        runCurrent()
        assertEquals(1, source.polling)
        vm.stopped()
        runCurrent()
        assertEquals(0, source.polling)
        // Back with the same code: it is asked about again, without a new one.
        vm.started()
        runCurrent()
        assertEquals(1, source.polling)
        assertEquals(1, source.starts)
    }

    @Test
    fun aCodeThatExpiredMeanwhileIsReplacedAtOnce() = runTest {
        val vm = link()
        source.codes += FakeOnboardingSource.code(server, "BCDF-GHJK", expiresAt = 60_000)
        source.codes += FakeOnboardingSource.code(server, "LMNP-QRST", expiresAt = 1_000_000)
        vm.started()
        runCurrent()
        vm.stopped()
        advanceTimeBy(120_000)
        vm.started()
        runCurrent()
        assertEquals(2, source.starts)
        assertEquals("LMNP-QRST", vm.state.value.code?.userCode)
        assertEquals(LinkStatus.Waiting, vm.state.value.status)
    }

    @Test
    fun aDeclineShowsGetANewCode() = runTest {
        val vm = link()
        vm.started()
        runCurrent()
        source.polls.send(DevicePoll.Denied)
        runCurrent()
        assertEquals(LinkStatus.Declined, vm.state.value.status)
        assertEquals(0, source.polling)
        // Nothing is asked again by itself.
        advanceTimeBy(60_000)
        assertEquals(1, source.starts)
        vm.newCode()
        runCurrent()
        assertEquals(2, source.starts)
        assertEquals(LinkStatus.Waiting, vm.state.value.status)
    }

    @Test
    fun expiredCodesAreRenewedSilentlyThreeTimesThenWaitForOk() = runTest {
        val vm = link()
        vm.started()
        runCurrent()
        repeat(LinkViewModel.RENEWALS) { renewal ->
            source.polls.send(DevicePoll.Expired)
            runCurrent()
            assertEquals("renewal ${renewal + 1}", renewal + 2, source.starts)
            assertEquals(LinkStatus.Waiting, vm.state.value.status)
        }
        source.polls.send(DevicePoll.Expired)
        runCurrent()
        assertEquals(LinkViewModel.RENEWALS + 1, source.starts)
        assertEquals(LinkStatus.Expired, vm.state.value.status)
        // OK: a new code, and three more renewals after it.
        vm.newCode()
        runCurrent()
        assertEquals(LinkStatus.Waiting, vm.state.value.status)
        source.polls.send(DevicePoll.Expired)
        runCurrent()
        assertEquals(LinkStatus.Waiting, vm.state.value.status)
    }

    @Test
    fun networkTroubleCountsDownToTheNextTry() = runTest {
        val vm = link()
        vm.started()
        runCurrent()
        val now = testScheduler.currentTime
        source.polls.send(DevicePoll.Unreachable(8_000))
        runCurrent()
        assertEquals(LinkStatus.Unreachable(now + 8_000), vm.state.value.status)
        source.polls.send(DevicePoll.Waiting)
        runCurrent()
        assertEquals(LinkStatus.Waiting, vm.state.value.status)
    }

    @Test
    fun aCodeThatCantBeFetchedIsAskedForAgainAfterABackoff() = runTest {
        val vm = link()
        source.startError = IOException("no network")
        vm.started()
        runCurrent()
        assertEquals(LinkStatus.Unreachable(testScheduler.currentTime + 2_000), vm.state.value.status)
        assertEquals(1, source.starts)
        advanceTimeBy(2_001)
        assertEquals(2, source.starts)
        // 4 s after the second failure.
        assertEquals(LinkStatus.Unreachable(testScheduler.currentTime + 4_000 - 1), vm.state.value.status)
        source.startError = null
        advanceTimeBy(4_000)
        assertEquals(3, source.starts)
        assertEquals(LinkStatus.Waiting, vm.state.value.status)
    }

    @Test
    fun anOlderServerSaysSoInTheModulesWords() = runTest {
        val vm = link()
        source.startError = AuthException("this server can't sign in with a code yet: update it, or sign in with a password")
        vm.started()
        runCurrent()
        val status = vm.state.value.status
        assertTrue(status is LinkStatus.Failed)
        advanceTimeBy(60_000)
        assertEquals(1, source.starts)
    }

    @Test
    fun signedInShowsTheNameForHalfASecondThenHome() = runTest {
        val vm = link()
        source.profiles = FakeOnboardingSource.household()
        vm.started()
        runCurrent()
        source.signIn(server, Me(1, "anna", "Anna"))
        source.polls.send(DevicePoll.SignedIn(server))
        runCurrent()
        assertEquals(LinkStatus.SignedIn("Anna"), vm.state.value.status)
        assertEquals(1, source.refreshes)
        assertTrue(ProfileGate.passed)
        advanceTimeBy(LinkViewModel.SIGNED_IN_MS + 1)
        assertEquals(OnboardingMove.Restart(PleyKey.Home), vm.navigation.first())
        // The household is saved for the next start's "Who's watching?".
        advanceUntilIdle()
        assertEquals(4, source.savedHousehold(server)?.size)
    }

    @Test
    fun withoutTheNameAfterThreeSecondsItSaysSignedIn() = runTest {
        val vm = link()
        vm.started()
        runCurrent()
        source.polls.send(DevicePoll.SignedIn(server))
        runCurrent()
        advanceTimeBy(LinkViewModel.NAME_WAIT_MS + 1)
        val status = vm.state.value.status as LinkStatus.SignedIn
        assertNull(status.name)
    }
}
