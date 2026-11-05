package app.pley.android.data

import app.pley.android.common.Language
import java.util.Locale
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.flowOf

/**
 * What the language Pley is shown in goes by (docs/ANDROID.md, "Language"),
 * as far as the app knows it this time: the account's own choice and the
 * server's.
 */
sealed interface LanguageSources {
    /** Still reading, or signed out of the server in use: the language shown stays. */
    data object Unknown : LanguageSources

    /** No server chosen: the device's. */
    data object NoServer : LanguageSources

    /**
     * Signed in. [account] is the profile's own choice ("fr"; `null`: the
     * server's), once GET /api/auth/me answered ([accountKnown]); [server]
     * the server's (`null`: each device's own), once GET /api/server
     * answered ([serverKnown]).
     */
    data class SignedIn(
        val account: String?,
        val accountKnown: Boolean,
        val server: String?,
        val serverKnown: Boolean,
    ) : LanguageSources

    /**
     * The language to show, the device's being [device] (the system's
     * languages, the one it prefers first): the account's, else the
     * server's, else the device's, else English. `null` while the server
     * hasn't said what decides: the language shown stays, rather than the
     * device's showing for a moment.
     */
    fun wanted(device: List<Locale>): Language? = when (this) {
        Unknown -> null
        NoServer -> Language.ofDevice(device) ?: Language.English
        is SignedIn -> Language.of(account) ?: when {
            !accountKnown -> null
            else -> Language.of(server) ?: if (serverKnown) Language.ofDevice(device) ?: Language.English else null
        }
    }
}

/** The account's language, and what decides the one shown. */
interface LanguageRepository {
    val sources: Flow<LanguageSources>

    /**
     * Makes [language] ("en", "fr", "es"; `null`: the server's) the
     * account's (PUT /api/me/language), shown at once and put back if the
     * server refuses.
     */
    suspend fun choose(language: String?)

    /** The app is now shown in another language: what the server wrote in the one before is asked for again. */
    fun shown() {}

    /** Without a server: nothing decides, and nothing can be chosen. */
    object None : LanguageRepository {
        override val sources: Flow<LanguageSources> = flowOf(LanguageSources.Unknown)

        override suspend fun choose(language: String?) = throw IllegalStateException("not signed in")
    }
}

/** The account's language from the session (GET /api/auth/me) and the server's (GET /api/server). */
internal class SessionLanguage(
    private val session: SessionManager,
    /** Home's and the libraries' rows, whose titles the server writes in the language asked for. */
    private val cache: () -> ResourceCache?,
) : LanguageRepository {
    override val sources: Flow<LanguageSources> = combine(session.state, session.meState, session.serverLanguage) { state, me, server ->
        when (state) {
            SessionState.Starting, is SessionState.SignedOut -> LanguageSources.Unknown
            SessionState.NoServer -> LanguageSources.NoServer
            is SessionState.SignedIn -> {
                val said = server?.takeIf { it.issuer == state.session.issuer }
                LanguageSources.SignedIn(
                    account = me?.value?.language,
                    // What was saved may be older than a choice made elsewhere: the server's answer decides.
                    accountKnown = me?.fresh == true,
                    server = said?.language,
                    serverKnown = said != null,
                )
            }
        }
    }.distinctUntilChanged()

    override suspend fun choose(language: String?) = session.setLanguage(language)

    override fun shown() {
        cache()?.revalidate { key, _ -> SnapshotKeys.isWorded(key) }
    }
}
