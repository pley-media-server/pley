package app.pley.android.screens.settings

import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import app.pley.android.common.AppLanguage
import app.pley.android.common.Language
import app.pley.android.common.UiText
import app.pley.android.screens.R
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.stateIn
import java.util.Locale

/** The globe of the screens without an account: this device's language. */
data class DeviceLanguageUiState(
    /** The language shown, which the globe is labelled with. */
    val shown: Language = Language.English,
    /** The language picked on this device; `null`: Automatic. */
    val picked: Language? = null,
    /** The first of the system's languages that Pley has, which Automatic stands for; `null` when none. */
    val device: Language? = null,
) {
    /** The choices in order: "Automatic" (`null`), then each language. */
    val choices: List<Language?> get() = listOf<Language?>(null) + Language.entries

    /** What Settings' Language row says: the language shown, by its own name. */
    val value: UiText get() = UiText.res(shown.autonym)

    /** What the globe before signing in says: the language shown, by its two letters ("FR"). */
    val short: String get() = shown.code.uppercase(Locale.ROOT)

    /** How [choice] reads: a language by its own name ("Français"), Automatic by what it is here: "Automatic (English)". */
    fun label(choice: Language?): UiText = when (choice) {
        null -> UiText.res(R.string.language_automatic, UiText.res((device ?: Language.English).autonym))
        else -> UiText.res(choice.autonym)
    }
}

/**
 * The language picker of the screens shown before signing in, on phones
 * and TVs (docs/ANDROID.md, "Language"): "Automatic", the system's
 * language where Pley has it, else English, then English, Français and
 * Español. A pick shows at once and is remembered on the device
 * (AppLanguage.pick); Automatic forgets it. Once signed in, an account's
 * own language comes before it, and an account's Automatic means it.
 */
class DeviceLanguageViewModel(
    shown: StateFlow<Language>,
    private val picked: StateFlow<Language?>,
    /** The first of the system's languages that Pley has, read as the picker shows. */
    private val device: () -> Language?,
    private val pick: (Language?) -> Unit,
) : ViewModel() {
    val state: StateFlow<DeviceLanguageUiState> = combine(shown, picked) { language, choice ->
        DeviceLanguageUiState(shown = language, picked = choice, device = device())
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), DeviceLanguageUiState(shown.value, picked.value, device()))

    /** Picks [choice] on this device; `null`: Automatic. */
    fun choose(choice: Language?) {
        if (choice != picked.value) pick(choice)
    }

    companion object {
        val factory: ViewModelProvider.Factory = viewModelFactory {
            initializer {
                val app = this[ViewModelProvider.AndroidViewModelFactory.APPLICATION_KEY]
                DeviceLanguageViewModel(
                    shown = AppLanguage.shown,
                    picked = AppLanguage.picked,
                    device = { app?.let { Language.ofDevice(AppLanguage.deviceLocales(it)) } },
                    pick = { language -> app?.let { AppLanguage.pick(it, language) } },
                )
            }
        }
    }
}
