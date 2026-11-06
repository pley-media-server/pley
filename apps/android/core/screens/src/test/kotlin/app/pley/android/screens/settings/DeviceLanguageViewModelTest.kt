package app.pley.android.screens.settings

import app.pley.android.common.Language
import app.pley.android.common.UiText
import app.pley.android.screens.inEnglish
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
import org.junit.Assert.assertNull
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/**
 * The globe of the screens before signing in: labelled with the language
 * shown; "Automatic" (the system's language where Pley has it, else
 * English), then each language by its own name, the pick checked. A pick
 * is this device's; Automatic forgets it.
 */
@OptIn(ExperimentalCoroutinesApi::class)
@RunWith(RobolectricTestRunner::class)
class DeviceLanguageViewModelTest {
    /** AppLanguage's part: the language shown follows the pick, else the system's. */
    private class Device(val system: Language?) {
        val picked = MutableStateFlow<Language?>(null)
        val shown = MutableStateFlow(system ?: Language.English)
        val picks = mutableListOf<Language?>()

        fun pick(language: Language?) {
            picks += language
            picked.value = language
            shown.value = language ?: system ?: Language.English
        }

        fun model() = DeviceLanguageViewModel(shown, picked, { system }, ::pick)
    }

    @Before
    fun main() = Dispatchers.setMain(UnconfinedTestDispatcher())

    @After
    fun reset() = Dispatchers.resetMain()

    @Test
    fun theGlobeSaysTheLanguageShownAndAutomaticWhatItIsHere() = runTest {
        val device = Device(system = Language.French)
        val vm = device.model()
        backgroundScope.launch(UnconfinedTestDispatcher(testScheduler)) { vm.state.collect {} }
        val state = vm.state.value
        assertEquals("Français", state.value.inEnglish())
        assertEquals("FR", state.short)
        assertNull(state.picked)
        assertEquals(listOf(null, Language.English, Language.French, Language.Spanish), state.choices)
        assertEquals(listOf("Automatic (Français)", "English", "Français", "Español"), state.choices.map { state.label(it).inEnglish() })
    }

    @Test
    fun onADeviceInALanguagePleyDoesntHaveAutomaticIsEnglish() = runTest {
        val vm = Device(system = null).model()
        backgroundScope.launch(UnconfinedTestDispatcher(testScheduler)) { vm.state.collect {} }
        assertEquals("Automatic (English)", vm.state.value.label(null).inEnglish())
        assertEquals("English", vm.state.value.value.inEnglish())
    }

    @Test
    fun aPickShowsAndIsCheckedAutomaticForgetsIt() = runTest {
        val device = Device(system = Language.English)
        val vm = device.model()
        backgroundScope.launch(UnconfinedTestDispatcher(testScheduler)) { vm.state.collect {} }

        vm.choose(Language.Spanish)
        val picked = vm.state.first { it.picked == Language.Spanish }
        assertEquals(UiText.res(Language.Spanish.autonym), picked.value)
        // Automatic still says the system's language.
        assertEquals("Automatic (English)", picked.label(null).inEnglish())

        // The pick already made changes nothing.
        vm.choose(Language.Spanish)
        vm.choose(null)
        val automatic = vm.state.first { it.picked == null }
        assertEquals("English", automatic.value.inEnglish())
        vm.choose(null)
        assertEquals(listOf(Language.Spanish, null), device.picks)
    }

    @Test
    fun theFirstStateIsWhatTheDeviceHasAlready() {
        val device = Device(system = Language.French).apply { pick(Language.Spanish) }
        val state = device.model().state.value
        assertEquals(Language.Spanish, state.picked)
        assertEquals(Language.Spanish, state.shown)
        assertEquals(Language.French, state.device)
    }
}
