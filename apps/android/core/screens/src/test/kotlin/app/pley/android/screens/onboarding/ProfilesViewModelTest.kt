package app.pley.android.screens.onboarding

import androidx.lifecycle.viewModelScope
import app.pley.android.auth.Needs
import app.pley.android.auth.OAuthError
import app.pley.android.auth.PinCheck
import app.pley.android.auth.Profile
import app.pley.android.auth.TokenException
import app.pley.android.common.UiText
import app.pley.android.model.Me
import app.pley.android.screens.PleyKey
import app.pley.android.screens.inEnglish
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
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import app.pley.android.auth.R as AuthR

/**
 * "Who's watching?" and its PIN pad: the household (saved, then fresh),
 * switching by what each profile needs, the server's refusals by reason
 * (never retried), and a TV's cold start as a PIN profile.
 */
@OptIn(ExperimentalCoroutinesApi::class)
@RunWith(RobolectricTestRunner::class)
class ProfilesViewModelTest {
    private val server = FakeOnboardingSource.SERVER
    private val source = FakeOnboardingSource().apply {
        active.value = server
        profiles = FakeOnboardingSource.household()
        signIn(server, Me(1, "anna", "Anna"))
    }
    private var vm: ProfilesViewModel? = null

    @After
    fun tearDown() {
        vm?.viewModelScope?.cancel()
        Dispatchers.resetMain()
        ProfileGate.passed = false
    }

    private fun TestScope.profiles(tv: Boolean = true, gate: Boolean = false, pin: Boolean = false): ProfilesViewModel {
        Dispatchers.setMain(StandardTestDispatcher(testScheduler))
        return ProfilesViewModel(source, tv, gate, pin).also { vm = it }
    }

    private fun wrong(reason: String, description: String) = TokenException(400, OAuthError("invalid_grant", description, reason))

    private fun ProfilesViewModel.type(pin: String) = pin.forEach { digit(it - '0') }

    @Test
    fun theSavedHouseholdShowsFirstThenTheServersWithFocusOnTheOneInUse() = runTest {
        source.saveHousehold(server, listOf(Profile(1, "Anna", null, null, current = true, needs = Needs.NONE)))
        val vm = profiles()
        advanceUntilIdle()
        val state = vm.state.value
        assertEquals(listOf("Anna", "Sam", "Robin", "Kids"), state.profiles.map { it.name })
        assertEquals(1L, state.focus)
        assertTrue(state.profiles[0].current)
        assertTrue(state.profiles[1].locked)
        assertEquals("$server/api/users/3/avatar?v=1", state.profiles[2].avatarUrl)
        assertEquals(4, source.savedHousehold(server)?.size)
    }

    @Test
    fun aProfileWithoutAPinSwitchesAtOnce() = runTest {
        source.profiles = FakeOnboardingSource.household(currentId = 2)
        source.signIn(server, Me(2, "sam", "Sam", hasPin = true))
        val vm = profiles()
        advanceUntilIdle()
        vm.pick(1)
        advanceUntilIdle()
        assertEquals(listOf(1L to null), source.switches)
        assertEquals(OnboardingMove.Restart(PleyKey.Home), vm.navigation.first())
        assertTrue(ProfileGate.passed)
    }

    @Test
    fun aPinProfileTakesFourToEightDigitsThenOk() = runTest {
        val vm = profiles()
        advanceUntilIdle()
        vm.pick(2)
        val pad = vm.state.value.step as ProfilesStep.Pin
        assertEquals("Sam", pad.profile.name)
        vm.type("123")
        vm.submit()
        // Three digits: OK does nothing.
        assertTrue(source.switches.isEmpty())
        vm.type("456789")
        assertEquals(8, (vm.state.value.step as ProfilesStep.Pin).digits)
        vm.delete()
        vm.delete()
        vm.delete()
        vm.delete()
        assertEquals(4, (vm.state.value.step as ProfilesStep.Pin).digits)
        vm.submit()
        advanceUntilIdle()
        assertEquals(listOf(2L to "1234"), source.switches)
        assertEquals(OnboardingMove.Restart(PleyKey.Home), vm.navigation.first())
    }

    @Test
    fun aWrongPinClearsTheDotsShowsTheSentenceAndIsNeverRetried() = runTest {
        source.onSwitch = { _, _ -> throw wrong("wrong_pin", "wrong PIN") }
        val vm = profiles()
        advanceUntilIdle()
        vm.pick(2)
        vm.type("9999")
        vm.submit()
        advanceUntilIdle()
        val pad = vm.state.value.step as ProfilesStep.Pin
        assertEquals("Wrong PIN.", pad.error.inEnglish())
        assertEquals(0, pad.digits)
        assertEquals(1, pad.shakes)
        assertFalse(pad.lockedOut)
        assertEquals(1, source.switches.size)
        // Typing again clears nothing by itself; the next OK sends once more.
        vm.type("1234")
        vm.submit()
        advanceUntilIdle()
        assertEquals(2, source.switches.size)
        assertEquals(2, (vm.state.value.step as ProfilesStep.Pin).shakes)
    }

    @Test
    fun throttledShowsTheServersWait() = runTest {
        source.onSwitch = { _, _ -> throw wrong("throttled", "too many tries: try again in 5 minutes") }
        val vm = profiles()
        advanceUntilIdle()
        vm.pick(2)
        vm.type("1234")
        vm.submit()
        advanceUntilIdle()
        assertEquals("Too many tries: try again in 5 minutes.", (vm.state.value.step as ProfilesStep.Pin).error.inEnglish())
    }

    @Test
    fun aLockedPinDimsThePad() = runTest {
        source.onSwitch = { _, _ -> throw wrong("pin_locked", "locked") }
        val vm = profiles()
        advanceUntilIdle()
        vm.pick(2)
        vm.type("1234")
        vm.submit()
        advanceUntilIdle()
        val pad = vm.state.value.step as ProfilesStep.Pin
        assertTrue(pad.lockedOut)
        assertTrue(pad.error.inEnglish()!!.startsWith("This profile's PIN is locked"))
        // The keys do nothing now.
        vm.type("1234")
        assertEquals(0, (vm.state.value.step as ProfilesStep.Pin).digits)
        vm.backToPick()
        assertEquals(ProfilesStep.Pick, vm.state.value.step)
    }

    @Test
    fun aPasswordProfileIsApprovedOnAPhoneOnTvsAndInTheBrowserOnPhones() = runTest {
        val tv = profiles(tv = true)
        advanceUntilIdle()
        tv.pick(3)
        assertEquals("Robin", (tv.state.value.step as ProfilesStep.Approve).profile.name)
        tv.viewModelScope.cancel()

        val phone = profiles(tv = false)
        advanceUntilIdle()
        phone.pick(3)
        assertEquals("Robin", (phone.state.value.step as ProfilesStep.Browser).profile.name)
        assertTrue(source.switches.isEmpty())
    }

    @Test
    fun theServerSendingToTheBrowserMeansThePasswordWay() = runTest {
        source.profiles = FakeOnboardingSource.household().map { if (it.id == 3L) it.copy(needs = Needs.NONE) else it }
        source.onSwitch = { _, _ -> throw wrong("browser", "this profile needs its password: sign in again") }
        val vm = profiles(tv = true)
        advanceUntilIdle()
        vm.pick(3)
        advanceUntilIdle()
        assertTrue(vm.state.value.step is ProfilesStep.Approve)
    }

    @Test
    fun aLockedProfileSaysWhy() = runTest {
        val vm = profiles()
        advanceUntilIdle()
        vm.pick(4)
        val pad = vm.state.value.step as ProfilesStep.Pin
        assertTrue(pad.lockedOut)
        assertEquals("This profile can't be used from the app.", pad.error.inEnglish())
    }

    @Test
    fun aTvStartingAsAPinProfileAsksItsPin() = runTest {
        source.signIn(server, Me(1, "anna", "Anna", hasPin = true))
        val vm = profiles(tv = true, gate = true)
        advanceUntilIdle()
        vm.pick(1)
        val pad = vm.state.value.step as ProfilesStep.Pin
        assertTrue(pad.confirm)
        source.pinCheck = { _, _ -> PinCheck.Failed(UiText.res(AuthR.string.auth_wrong_pin)) }
        vm.type("0000")
        vm.submit()
        advanceUntilIdle()
        assertEquals("Wrong PIN.", (vm.state.value.step as ProfilesStep.Pin).error.inEnglish())
        source.pinCheck = { _, _ -> PinCheck.Confirmed }
        vm.type("1234")
        vm.submit()
        advanceUntilIdle()
        assertEquals(listOf(1L to "0000", 1L to "1234"), source.pinChecks)
        assertEquals(OnboardingMove.Restart(PleyKey.Home), vm.navigation.first())
        assertTrue(source.switches.isEmpty())
    }

    @Test
    fun aServerOutOfReachIsNoWrongPin() = runTest {
        source.signIn(server, Me(1, "anna", "Anna", hasPin = true))
        val vm = profiles(tv = true, gate = true)
        advanceUntilIdle()
        vm.pick(1)
        source.pinCheck = { _, _ -> throw java.net.SocketTimeoutException("timeout") }
        vm.type("1234")
        vm.submit()
        advanceUntilIdle()

        val pad = vm.state.value.step as ProfilesStep.Pin
        assertEquals("Can't reach your server: no answer in time.", pad.error.inEnglish())
        assertEquals("the pad doesn't shake", 0, pad.shakes)
        assertEquals(false, pad.lockedOut)
    }

    @Test
    fun serversThatCantCheckThePinLetItThrough() = runTest {
        source.signIn(server, Me(1, "anna", "Anna", hasPin = true))
        source.pinCheck = { _, _ -> PinCheck.Unsupported }
        val vm = profiles(tv = true, gate = true)
        advanceUntilIdle()
        vm.pick(1)
        vm.type("1234")
        vm.submit()
        advanceUntilIdle()
        assertEquals(OnboardingMove.Restart(PleyKey.Home), vm.navigation.first())
    }

    @Test
    fun aTvStartingAsAPinProfileWithAlwaysStartAsOpensOnItsPad() = runTest {
        source.signIn(server, Me(1, "anna", "Anna", hasPin = true))
        source.saveHousehold(server, FakeOnboardingSource.household())
        source.profilesError = java.net.SocketTimeoutException("timeout")
        val vm = profiles(tv = true, gate = true, pin = true)
        advanceUntilIdle()
        // From what was saved, without the server.
        val pad = vm.state.value.step as ProfilesStep.Pin
        assertEquals(1L to true, pad.profile.id to pad.confirm)
        // Back shows the profiles, and the list loading again leaves them.
        vm.backToPick()
        source.profilesError = null
        vm.reload()
        advanceUntilIdle()
        assertEquals(ProfilesStep.Pick, vm.state.value.step)
        vm.pick(1)
        vm.type("1234")
        vm.submit()
        advanceUntilIdle()
        assertEquals(listOf(1L to "1234"), source.pinChecks)
        assertEquals(OnboardingMove.Restart(PleyKey.Home), vm.navigation.first())
    }

    @Test
    fun itsPadComesFirstOnlyAtAStartAndForAProfileWithAPin() = runTest {
        val noPin = profiles(tv = true, gate = true, pin = true)
        advanceUntilIdle()
        assertEquals(ProfilesStep.Pick, noPin.state.value.step)
        noPin.viewModelScope.cancel()
        source.signIn(server, Me(1, "anna", "Anna", hasPin = true))
        val answered = profiles(tv = true, gate = false, pin = true)
        advanceUntilIdle()
        assertEquals(ProfilesStep.Pick, answered.state.value.step)
        answered.viewModelScope.cancel()
        val asked = profiles(tv = true, gate = true)
        advanceUntilIdle()
        assertEquals(ProfilesStep.Pick, asked.state.value.step)
    }

    @Test
    fun theProfileInUseContinuesWithoutAPinOutsideTheColdStart() = runTest {
        source.signIn(server, Me(1, "anna", "Anna", hasPin = true))
        val vm = profiles(tv = true, gate = false)
        advanceUntilIdle()
        vm.pick(1)
        advanceUntilIdle()
        assertEquals(OnboardingMove.Restart(PleyKey.Home), vm.navigation.first())
        assertTrue(source.pinChecks.isEmpty())
    }

    @Test
    fun withoutTheProfilesScopeItOffersSigningInAsAnother() = runTest {
        source.canSwitch = false
        val vm = profiles(tv = false)
        advanceUntilIdle()
        assertFalse(vm.state.value.canSwitch)
        vm.signInAgain()
        assertTrue(vm.state.value.step is ProfilesStep.Browser)
    }

    @Test
    fun pinEntryKeepsFourToEightDigits() {
        val pin = PinEntry()
        repeat(9) { pin.add(7) }
        assertEquals(8, pin.length)
        assertFalse(pin.add(10))
        assertTrue(pin.complete)
        assertEquals("77777777", pin.take())
        assertEquals(0, pin.length)
        assertFalse(pin.complete)
        assertEquals("PinEntry(0 digits)", pin.toString())
    }
}
