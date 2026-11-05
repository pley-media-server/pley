package app.pley.android.screens.onboarding

import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.viewModelScope
import app.pley.android.auth.AuthException
import app.pley.android.auth.NO_BROWSER
import app.pley.android.auth.SignInState
import app.pley.android.auth.TOO_OLD
import app.pley.android.auth.UNENCRYPTED
import app.pley.android.common.UiText
import app.pley.android.screens.PleyKey
import java.net.ConnectException
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
import org.junit.Test
import app.pley.android.auth.R as AuthR

/**
 * A phone's sign-in in the browser: its states, its errors in the module's
 * words, a closed tab, and an answer that comes after Android ended the app.
 */
@OptIn(ExperimentalCoroutinesApi::class)
class SignInViewModelTest {
    private val server = FakeOnboardingSource.SERVER
    private val source = FakeOnboardingSource()
    private val made = mutableListOf<SignInViewModel>()

    @After
    fun tearDown() {
        made.forEach { it.viewModelScope.cancel() }
        Dispatchers.resetMain()
    }

    private fun TestScope.signIn(saved: SavedStateHandle = SavedStateHandle()): SignInViewModel {
        Dispatchers.setMain(StandardTestDispatcher(testScheduler))
        return SignInViewModel(source, saved).also { made += it }
    }

    /** [saved] as Android hands it back to a screen made again. */
    private fun restored(saved: SavedStateHandle) = SavedStateHandle(saved.keys().associateWith { saved.get<Any>(it) })

    /** What the screen saved once the browser showed the server's page. */
    private fun TestScope.savedWhileInTheBrowser(): SavedStateHandle {
        val saved = SavedStateHandle()
        val before = signIn(saved)
        before.signIn(null, server)
        advanceUntilIdle()
        before.paused()
        return restored(saved)
    }

    @Test
    fun opensWaitsFinishesThenHome() = runTest {
        val vm = signIn()
        vm.signIn(null, server)
        assertEquals(BrowserSignIn.Opening, vm.state.value)
        advanceUntilIdle()
        assertEquals(listOf(server), source.browserSignIns)
        assertEquals(BrowserSignIn.Waiting, vm.state.value)
        source.signInState.value = SignInState.Finishing
        advanceUntilIdle()
        assertEquals(BrowserSignIn.Finishing, vm.state.value)
        source.signInState.value = SignInState.SignedIn(server)
        advanceUntilIdle()
        assertEquals(BrowserSignIn.SignedIn, vm.state.value)
        assertEquals(1, source.refreshes)
        assertEquals(OnboardingMove.Restart(PleyKey.Home), vm.navigation.first())
    }

    @Test
    fun closingTheTabCancelsWithoutAWord() = runTest {
        val vm = signIn()
        vm.signIn(null, server)
        advanceUntilIdle()
        vm.paused()
        vm.resumed()
        advanceUntilIdle()
        assertEquals(BrowserSignIn.Idle, vm.state.value)
    }

    @Test
    fun comingBackWithAnAnswerIsntACancel() = runTest {
        val vm = signIn()
        vm.signIn(null, server)
        advanceUntilIdle()
        vm.paused()
        source.signInState.value = SignInState.Finishing
        vm.resumed()
        advanceUntilIdle()
        assertEquals(BrowserSignIn.Finishing, vm.state.value)
    }

    @Test
    fun anOlderServerAnUnencryptedOneAndOneThatDoesntAnswer() = runTest {
        val vm = signIn()
        source.browserError = AuthException(TOO_OLD)
        vm.signIn(null, server)
        advanceUntilIdle()
        assertEquals(BrowserSignIn.Failed(server, SignInProblem.TooOld, TOO_OLD), vm.state.value)
        vm.dismiss()
        source.browserError = AuthException(UNENCRYPTED)
        vm.signIn(null, server)
        advanceUntilIdle()
        assertEquals(SignInProblem.Unencrypted, (vm.state.value as BrowserSignIn.Failed).problem)
        source.browserError = ConnectException("refused")
        vm.signIn(null, server)
        advanceUntilIdle()
        assertEquals(BrowserSignIn.Failed(server, SignInProblem.Unreachable, NOTHING_ANSWERS), vm.state.value)
    }

    @Test
    fun aRefusalFromTheServerShowsItsWords() = runTest {
        val vm = signIn()
        vm.signIn(null, server)
        advanceUntilIdle()
        source.signInState.value = SignInState.Failed(DENIED)
        advanceUntilIdle()
        assertEquals(BrowserSignIn.Failed(server, SignInProblem.Refused, DENIED), vm.state.value)
    }

    @Test
    fun aSignInFinishedAfterTheAppWasEndedStillSignsIn() = runTest {
        val saved = savedWhileInTheBrowser()
        // The app was ended; the browser's answer started a new one, and the sign-in finished there.
        source.signInState.value = SignInState.SignedIn(server)
        val vm = signIn(saved)
        vm.resumed()
        advanceUntilIdle()
        assertEquals(BrowserSignIn.SignedIn, vm.state.value)
        assertEquals(OnboardingMove.Restart(PleyKey.Home), vm.navigation.first())
    }

    @Test
    fun aRefusalAfterTheAppWasEndedIsShown() = runTest {
        val saved = savedWhileInTheBrowser()
        source.signInState.value = SignInState.Finishing
        val vm = signIn(saved)
        advanceUntilIdle()
        assertEquals(BrowserSignIn.Finishing, vm.state.value)
        source.signInState.value = SignInState.Failed(DENIED)
        advanceUntilIdle()
        assertEquals(BrowserSignIn.Failed(server, SignInProblem.Refused, DENIED), vm.state.value)
    }

    @Test
    fun theTabClosedAfterTheAppWasEndedCancelsWithoutAWord() = runTest {
        val saved = savedWhileInTheBrowser()
        val vm = signIn(saved)
        vm.resumed()
        advanceUntilIdle()
        assertEquals(BrowserSignIn.Idle, vm.state.value)
        // Nothing is under way anymore: an old answer doesn't sign in.
        val again = signIn(restored(saved))
        source.signInState.value = SignInState.SignedIn(server)
        advanceUntilIdle()
        assertEquals(BrowserSignIn.Idle, again.state.value)
    }

    @Test
    fun anOldAnswerDoesntSignInAScreenThatStartedNothing() = runTest {
        source.signInState.value = SignInState.SignedIn(server)
        val vm = signIn()
        advanceUntilIdle()
        assertEquals(BrowserSignIn.Idle, vm.state.value)
    }

    @Test
    fun withoutABrowserItSignsInWithACode() = runTest {
        val vm = signIn()
        source.browserError = AuthException(NO_BROWSER)
        vm.signIn(null, server)
        advanceUntilIdle()
        assertEquals(BrowserSignIn.Idle, vm.state.value)
        assertEquals(OnboardingMove.Open(PleyKey.Link(server)), vm.navigation.first())
    }
}

/** What the sign-in module says when the person didn't allow the app. */
private val DENIED = UiText.res(AuthR.string.auth_denied)
