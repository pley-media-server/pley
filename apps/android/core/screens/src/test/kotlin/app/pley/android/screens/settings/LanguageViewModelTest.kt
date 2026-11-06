package app.pley.android.screens.settings

import app.pley.android.common.Language
import app.pley.android.common.UiText
import app.pley.android.data.LanguageRepository
import app.pley.android.data.LanguageSources
import app.pley.android.network.ApiException
import app.pley.android.screens.R
import app.pley.android.screens.inEnglish
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/**
 * The Language row: "Automatic" (this device's language, or the server's
 * on a device in a language Pley doesn't have), then each language by its
 * own name; a choice goes to the server, and a refusal is said.
 */
@OptIn(ExperimentalCoroutinesApi::class)
@RunWith(RobolectricTestRunner::class)
class LanguageViewModelTest {
    private class Account(sources: LanguageSources) : LanguageRepository {
        override val sources = MutableStateFlow(sources)
        val chosen = mutableListOf<String?>()
        var answer: CompletableDeferred<Unit>? = null
        var refusal: Exception? = null

        override suspend fun choose(language: String?) {
            chosen += language
            answer?.await()
            refusal?.let { throw it }
            val now = sources.value as LanguageSources.SignedIn
            sources.value = now.copy(account = language)
        }
    }

    private fun signedIn(account: String? = null, server: String? = null) = LanguageSources.SignedIn(account, true, server, true)

    @Before
    fun main() = Dispatchers.setMain(UnconfinedTestDispatcher())

    @After
    fun reset() = Dispatchers.resetMain()

    private suspend fun LanguageViewModel.shown(): LanguageUiState = state.first { it.available }

    @Test
    fun automaticSaysWhatItIsHere() = runTest {
        // A device in a language Pley doesn't have: the server's.
        val account = Account(signedIn(server = "fr"))
        val vm = LanguageViewModel(account) { null }
        backgroundScope.launch(UnconfinedTestDispatcher(testScheduler)) { vm.state.collect {} }
        val state = vm.shown()
        assertEquals(null, state.chosen)
        assertEquals("Automatic (Français)", state.value.inEnglish())
        assertEquals(listOf(null, Language.English, Language.French, Language.Spanish), state.choices)
        assertEquals(listOf("Automatic (Français)", "English", "Français", "Español"), state.choices.map { state.label(it).inEnglish() })

        // No server language either: English.
        account.sources.value = signedIn(server = null)
        assertEquals("Automatic (English)", vm.state.first { it.server == null }.value.inEnglish())
        account.sources.value = signedIn(account = "es", server = "de")
        val chosen = vm.state.first { it.chosen != null }
        assertEquals(Language.Spanish, chosen.chosen)
        assertEquals(UiText.res(Language.Spanish.autonym), chosen.value)
        assertEquals(UiText.res(R.string.language_automatic, UiText.res(Language.English.autonym)), chosen.label(null))
    }

    @Test
    fun theDevicesLanguageComesBeforeTheServers() = runTest {
        val vm = LanguageViewModel(Account(signedIn(server = "es"))) { Language.French }
        backgroundScope.launch(UnconfinedTestDispatcher(testScheduler)) { vm.state.collect {} }
        assertEquals("Automatic (Français)", vm.shown().value.inEnglish())
    }

    @Test
    fun withoutASignInThereIsNoRow() = runTest {
        val vm = LanguageViewModel(Account(LanguageSources.NoAccount)) { Language.English }
        backgroundScope.launch(UnconfinedTestDispatcher(testScheduler)) { vm.state.collect {} }
        assertFalse(vm.state.value.available)
        vm.choose(Language.French)
    }

    @Test
    fun aChoiceGoesToTheServerOnceAtATime() = runTest {
        val account = Account(signedIn(server = "fr")).apply { answer = CompletableDeferred() }
        val vm = LanguageViewModel(account) { Language.English }
        backgroundScope.launch(UnconfinedTestDispatcher(testScheduler)) { vm.state.collect {} }
        vm.shown()
        vm.choose(Language.Spanish)
        assertTrue(vm.state.value.saving)
        // Another choice while one is on its way, or the one shown, sends nothing.
        vm.choose(Language.French)
        assertEquals(listOf<String?>("es"), account.chosen)
        account.answer!!.complete(Unit)
        val after = vm.state.first { !it.saving && it.chosen == Language.Spanish }
        assertEquals(Language.Spanish, after.chosen)
        vm.choose(Language.Spanish)
        vm.choose(null)
        assertEquals(listOf("es", null), account.chosen)
    }

    @Test
    fun onAServerFromBeforeLanguagesAChoiceIsThisDevices() = runTest {
        // Its GET /api/server names no language: nothing goes to it, and the pick shows.
        val account = Account(LanguageSources.SignedIn(null, true, null, true, keepsLanguage = false))
        val picked = MutableStateFlow<Language?>(null)
        val vm = LanguageViewModel(account, picked, { picked.value = it }) { Language.English }
        backgroundScope.launch(UnconfinedTestDispatcher(testScheduler)) { vm.state.collect {} }
        val before = vm.shown()
        assertTrue(before.onDevice)
        assertEquals("Automatic (English)", before.value.inEnglish())
        vm.choose(Language.French)
        assertEquals(emptyList<String?>(), account.chosen)
        assertEquals(Language.French, vm.state.first { it.chosen != null }.chosen)
        // Automatic forgets the pick: the system's language, not the pick, is what it stands for.
        vm.choose(null)
        assertEquals(null, picked.value)
        assertEquals("Automatic (English)", vm.state.first { it.chosen == null }.value.inEnglish())
    }

    @Test
    fun aServerWithoutTheRouteKeepsTheChoiceOnThisDevice() = runTest {
        // It named a language, but answers the choice 404: this device's, then, and no error.
        val account = Account(signedIn(server = "en")).apply { refusal = ApiException(404, "not_found", "not found") }
        val picked = MutableStateFlow<Language?>(null)
        val vm = LanguageViewModel(account, picked, { picked.value = it }) { Language.English }
        backgroundScope.launch(UnconfinedTestDispatcher(testScheduler)) { vm.state.collect {} }
        vm.shown()
        val heard = mutableListOf<Throwable>()
        backgroundScope.launch(UnconfinedTestDispatcher(testScheduler)) { vm.errors.collect { heard += it } }
        vm.choose(Language.Spanish)
        assertEquals(listOf<String?>("es"), account.chosen)
        assertEquals(Language.Spanish, picked.value)
        val after = vm.state.first { it.onDevice }
        assertEquals(Language.Spanish, after.chosen)
        assertEquals(emptyList<Throwable>(), heard)
        // From then on the row is this device's: nothing more goes to the server.
        vm.choose(Language.French)
        assertEquals(listOf<String?>("es"), account.chosen)
        assertEquals(Language.French, picked.value)
    }

    @Test
    fun aRefusalIsSaid() = runTest {
        val refusal = ApiException(400, "bad_request", "language is one of en, fr, es, or null")
        val account = Account(signedIn()).apply { this.refusal = refusal }
        val vm = LanguageViewModel(account) { Language.English }
        backgroundScope.launch(UnconfinedTestDispatcher(testScheduler)) { vm.state.collect {} }
        vm.shown()
        val heard = mutableListOf<Throwable>()
        backgroundScope.launch(UnconfinedTestDispatcher(testScheduler)) { vm.errors.collect { heard += it } }
        vm.choose(Language.French)
        assertEquals(listOf<Throwable>(refusal), heard)
        assertEquals(null, vm.state.value.chosen)
        assertFalse(vm.state.value.saving)
    }
}
