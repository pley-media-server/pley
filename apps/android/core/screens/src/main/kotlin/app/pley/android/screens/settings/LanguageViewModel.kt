package app.pley.android.screens.settings

import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import app.pley.android.common.AppLanguage
import app.pley.android.common.Language
import app.pley.android.common.UiText
import app.pley.android.data.LanguageRepository
import app.pley.android.data.LanguageSources
import app.pley.android.screens.R
import app.pley.android.screens.screensGraph
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.receiveAsFlow
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

/** The Language row: the account's choice, and what "Server default" stands for. */
data class LanguageUiState(
    /** Signed in: the account's language can be chosen. */
    val available: Boolean = false,
    /** The account's own language; `null`: the server's default. */
    val chosen: Language? = null,
    /** The server's language, which "Server default" stands for; `null` when it leaves it to each device ([device]). */
    val server: Language? = null,
    /** This device's language: the first of the system's that Pley has, else English. */
    val device: Language = Language.English,
    /** A choice is on its way to the server. */
    val saving: Boolean = false,
) {
    /** The choices in order: "Server default" (`null`), then each language. */
    val choices: List<Language?> get() = listOf<Language?>(null) + Language.entries

    /** What the row says: the account's language. */
    val value: UiText get() = label(chosen)

    /**
     * How [choice] reads: a language by its own name ("Français"), the
     * server's default by what it stands for: "Server default (Français)",
     * or "Server default (this device's language, English)".
     */
    fun label(choice: Language?): UiText = when (choice) {
        null -> UiText.res(
            R.string.language_server_default,
            server?.let { UiText.res(it.autonym) } ?: UiText.res(R.string.language_this_device, UiText.res(device.autonym)),
        )
        else -> UiText.res(choice.autonym)
    }
}

/**
 * The account's language, as the Settings of phones and TVs offer it
 * (docs/ANDROID.md, "Language"): "Server default", which is the server's
 * language or, when the server leaves it to each device, this device's,
 * then English, Français and Español. A choice goes to the server
 * (PUT /api/me/language) and shows at once; one the server refuses is put
 * back, and said ([errors]).
 */
class LanguageViewModel(
    private val language: LanguageRepository,
    /** This device's language, read when the row shows. */
    private val device: () -> Language,
) : ViewModel() {
    private val saving = MutableStateFlow(false)
    private val channel = Channel<Throwable>(Channel.BUFFERED)

    /** Choices that couldn't be saved. */
    val errors: Flow<Throwable> = channel.receiveAsFlow()

    val state: StateFlow<LanguageUiState> = combine(language.sources, saving) { sources, busy ->
        val signedIn = sources as? LanguageSources.SignedIn
        LanguageUiState(
            available = signedIn != null,
            chosen = Language.of(signedIn?.account),
            server = Language.of(signedIn?.server),
            device = device(),
            saving = busy,
        )
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), LanguageUiState(device = device()))

    /** Makes [choice] the account's language; `null`: the server's default. */
    fun choose(choice: Language?) {
        val now = state.value
        if (!now.available || now.saving || choice == now.chosen) return
        saving.value = true
        viewModelScope.launch {
            try {
                language.choose(choice?.code)
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                channel.send(e)
            } finally {
                saving.update { false }
            }
        }
    }

    companion object {
        val factory: ViewModelProvider.Factory = viewModelFactory {
            initializer {
                val app = this[ViewModelProvider.AndroidViewModelFactory.APPLICATION_KEY]
                LanguageViewModel(screensGraph().data.language) {
                    app?.let { Language.ofDevice(AppLanguage.deviceLocales(it)) } ?: Language.English
                }
            }
        }
    }
}
