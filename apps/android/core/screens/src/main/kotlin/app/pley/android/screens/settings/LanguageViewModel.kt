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
import app.pley.android.data.isNoLanguageRoute
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

/** The Language row: the account's choice, and what "Automatic" stands for here. */
data class LanguageUiState(
    /** Signed in: the account's language can be chosen. */
    val available: Boolean = false,
    /** The account's own language; `null`: Automatic. */
    val chosen: Language? = null,
    /** The server language, which Automatic stands for on a device in a language Pley doesn't have. */
    val server: Language? = null,
    /**
     * This device's language: the one picked on it before signing in (the
     * globe), else the first of the system's that Pley has; `null` when
     * none.
     */
    val device: Language? = null,
    /** A choice is on its way to the server. */
    val saving: Boolean = false,
    /**
     * The server is a Pley from before languages, which keeps no account's
     * language: the row is this device's, as the globe picks it before
     * signing in, and [chosen] is that pick.
     */
    val onDevice: Boolean = false,
) {
    /** The choices in order: "Automatic" (`null`), then each language. */
    val choices: List<Language?> get() = listOf<Language?>(null) + Language.entries

    /** What the row says: the account's language. */
    val value: UiText get() = label(chosen)

    /**
     * How [choice] reads: a language by its own name ("Français"),
     * Automatic by what it is on this device, its language, else the
     * server's: "Automatic (Français)".
     */
    fun label(choice: Language?): UiText = when (choice) {
        null -> UiText.res(R.string.language_automatic, UiText.res((device ?: server ?: Language.English).autonym))
        else -> UiText.res(choice.autonym)
    }
}

/**
 * The account's language, as the Settings of phones and TVs offer it
 * (docs/ANDROID.md, "Language"): "Automatic", which is each device's
 * language, or the server language on a device in one Pley doesn't have,
 * then English, Français and Español. A choice goes to the server
 * (PUT /api/me/language) and shows at once; one the server refuses is put
 * back, and said ([errors]). A Pley from before languages keeps none (its
 * GET /api/server names no language, and the route answers 404 or 405,
 * which the data side remembers for the server): there a choice is this
 * device's, as the globe picks it, and shows all the same.
 */
class LanguageViewModel(
    private val language: LanguageRepository,
    /** The language picked on this device ([AppLanguage.picked]): the globe's, or this row's on an older server. */
    private val picked: StateFlow<Language?> = MutableStateFlow(null),
    /** Picks this device's language ([AppLanguage.pick]); `null`: the system's again. */
    private val pick: (Language?) -> Unit = {},
    /** The first of the system's languages that Pley has, read when the row shows. */
    private val system: () -> Language?,
) : ViewModel() {
    private val saving = MutableStateFlow(false)
    private val channel = Channel<Throwable>(Channel.BUFFERED)

    /** Choices that couldn't be saved. */
    val errors: Flow<Throwable> = channel.receiveAsFlow()

    val state: StateFlow<LanguageUiState> = combine(language.sources, saving, picked) { sources, busy, here ->
        val signedIn = sources as? LanguageSources.SignedIn
        val onDevice = signedIn?.keepsLanguage == false
        LanguageUiState(
            available = signedIn != null,
            chosen = if (onDevice) here else Language.of(signedIn?.account),
            server = Language.of(signedIn?.server),
            // On this device, Automatic forgets its pick: the system's language.
            device = if (onDevice) system() else here ?: system(),
            saving = busy,
            onDevice = onDevice,
        )
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), LanguageUiState(device = picked.value ?: system()))

    /** Makes [choice] the account's language, or this device's on an older server; `null`: Automatic. */
    fun choose(choice: Language?) {
        val now = state.value
        if (!now.available || now.saving || choice == now.chosen) return
        if (now.onDevice) {
            pick(choice)
            return
        }
        saving.value = true
        viewModelScope.launch {
            try {
                language.choose(choice?.code)
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                // Without the route, the server keeps no account's language, which the data side now remembers: this device's, then.
                if (isNoLanguageRoute(e)) pick(choice) else channel.send(e)
            } finally {
                saving.update { false }
            }
        }
    }

    companion object {
        val factory: ViewModelProvider.Factory = viewModelFactory {
            initializer {
                val app = this[ViewModelProvider.AndroidViewModelFactory.APPLICATION_KEY]
                LanguageViewModel(
                    screensGraph().data.language,
                    picked = AppLanguage.picked,
                    pick = { choice -> app?.let { AppLanguage.pick(it, choice) } },
                ) {
                    app?.let { Language.ofDevice(AppLanguage.deviceLocales(it)) }
                }
            }
        }
    }
}
