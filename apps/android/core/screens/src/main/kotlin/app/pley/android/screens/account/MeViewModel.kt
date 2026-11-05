package app.pley.android.screens.account

import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import app.pley.android.data.AppSettings
import app.pley.android.data.Session
import app.pley.android.data.SessionRepository
import app.pley.android.data.TrackDefaultsRepository
import app.pley.android.data.TrackDefaultsUnsupported
import app.pley.android.data.valueOrNull
import app.pley.android.model.SubtitleMode
import app.pley.android.model.TrackDefaults
import app.pley.android.model.format.FormatWords
import app.pley.android.model.format.languageName
import app.pley.android.player.MusicControl
import app.pley.android.screens.screensGraph
import java.text.Collator
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.catch
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.emptyFlow
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.onStart
import kotlinx.coroutines.flow.receiveAsFlow
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

/** Who the app acts as, as the Me page shows it (GET /api/auth/me). */
data class MeProfile(
    val id: Long,
    val name: String,
    /** The picture's full address; `null` without one. */
    val avatarUrl: String?,
    /** "#38bdf8", the circle behind the initial. */
    val avatarColor: String?,
    /**
     * A managed profile (no password, reached by switching from a home
     * member), else a user. Apps' tokens are never an administrator's, so
     * that is never said.
     */
    val managed: Boolean,
    /** The sign-in may switch profiles ("profiles" scope). */
    val canSwitch: Boolean,
    /** The server's address, "http://192.168.1.20:7539". */
    val server: String,
)

/** The profile's track defaults and whether this app may change them. */
data class TrackDefaultsUi(
    val defaults: TrackDefaults,
    /** `false`: shown read-only, to change on the web (an older server, or no "play" scope). */
    val editable: Boolean,
    val saving: Boolean = false,
) {
    /** Subtitles that need a language to read: "When the audio is in another language" and "Always". */
    val readsSubtitles: Boolean get() = defaults.subtitleMode == SubtitleMode.Foreign || defaults.subtitleMode == SubtitleMode.Always
}

data class MeUiState(
    /** `null` until the profile is known. */
    val profile: MeProfile? = null,
    val trackDefaults: TrackDefaultsUi? = null,
    /** Pages of the web open in a private tab (Settings → Privacy). */
    val privateTab: Boolean = true,
    val signingOut: Boolean = false,
)

/** Which track default a change was of. */
enum class TrackField { AudioLanguage, SubtitleMode, SubtitleLanguage }

/** What the Me page tells its screen once. */
sealed interface MeEvent {
    data class Saved(val field: TrackField) : MeEvent

    /** The server doesn't let the app change track defaults: they show read-only now. */
    data object ReadOnly : MeEvent

    data class Failed(val error: Throwable) : MeEvent

    /** Signed out of [server]: back to signing in to it. */
    data class SignedOut(val server: String?) : MeEvent
}

/**
 * The Me page (apps/web/src/pages/AccountPage.tsx): who is signed in, the
 * profile's audio and subtitle defaults (PUT /api/me/track-defaults),
 * switching profile or server, and signing out, which ends the sign-in on
 * the server, deletes what was saved from it and stops the music.
 */
class MeViewModel(
    private val session: SessionRepository,
    private val trackDefaults: TrackDefaultsRepository,
    settings: AppSettings?,
    private val music: MusicControl?,
) : ViewModel() {
    private val readOnly = MutableStateFlow(false)
    private val saving = MutableStateFlow(false)
    private val signingOut = MutableStateFlow(false)
    private val channel = Channel<MeEvent>(Channel.BUFFERED)

    val events: Flow<MeEvent> = channel.receiveAsFlow()

    private val privateTab: Flow<Boolean> = (settings?.settings?.map { it.privateSignInTab } ?: emptyFlow())
        .onStart { emit(true) }
        .catch { emit(true) }

    val state: StateFlow<MeUiState> = combine(
        session.session,
        trackDefaults.defaults(),
        combine(readOnly, saving, signingOut, ::Triple),
        privateTab,
    ) { s, defaults, (blocked, busy, leaving), private ->
        MeUiState(
            profile = s?.let(::profileOf),
            trackDefaults = defaults.valueOrNull?.let { TrackDefaultsUi(it, editable = !blocked && mayChange(s), saving = busy) },
            privateTab = private,
            signingOut = leaving,
        )
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), MeUiState())

    fun setAudioLanguage(code: String?) = save(TrackField.AudioLanguage) { it.copy(audioLanguage = code) }

    fun setSubtitleMode(mode: SubtitleMode) = save(TrackField.SubtitleMode) { it.copy(subtitleMode = mode) }

    fun setSubtitleLanguage(code: String?) = save(TrackField.SubtitleLanguage) { it.copy(subtitleLanguage = code) }

    private fun save(field: TrackField, change: (TrackDefaults) -> TrackDefaults) {
        val current = state.value.trackDefaults ?: return
        if (!current.editable || saving.value) return
        val next = change(current.defaults)
        if (next == current.defaults) return
        saving.value = true
        viewModelScope.launch {
            try {
                trackDefaults.save(next)
                channel.send(MeEvent.Saved(field))
            } catch (e: CancellationException) {
                throw e
            } catch (_: TrackDefaultsUnsupported) {
                readOnly.value = true
                channel.send(MeEvent.ReadOnly)
            } catch (e: Exception) {
                channel.send(MeEvent.Failed(e))
            } finally {
                saving.value = false
            }
        }
    }

    /** Signs out of the server: the music stops, the sign-in ends there, what was saved from it goes. */
    fun signOut() {
        if (signingOut.value) return
        signingOut.update { true }
        val server = session.session.value?.issuer
        viewModelScope.launch {
            try {
                music?.stop()
                session.signOut()
                channel.send(MeEvent.SignedOut(server))
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                channel.send(MeEvent.Failed(e))
            } finally {
                signingOut.value = false
            }
        }
    }

    companion object {
        val factory: ViewModelProvider.Factory = viewModelFactory {
            initializer {
                val g = screensGraph()
                MeViewModel(g.data.session, g.data.trackDefaults, g.data.settings, g.player.music)
            }
        }

        /** What [s] shows of its profile; the picture made absolute on its server. */
        fun profileOf(s: Session): MeProfile? {
            val me = s.me ?: return null
            return MeProfile(
                id = me.id,
                name = me.name,
                avatarUrl = me.avatarUrl?.takeIf { it.isNotEmpty() }?.let { absolute(it, s.issuer) },
                avatarColor = me.avatarColor,
                managed = me.managed,
                canSwitch = "profiles" in s.scopes,
                server = s.issuer.trimEnd('/'),
            )
        }

        /** Changing track defaults needs the "play" scope; a session not yet known may try. */
        fun mayChange(s: Session?): Boolean = s?.me == null || s.scopes.isEmpty() || "play" in s.scopes

        private fun absolute(path: String, origin: String): String =
            if (path.startsWith("http://") || path.startsWith("https://")) path else origin.trimEnd('/') + "/" + path.trimStart('/')
    }
}

/**
 * The languages offered as audio and subtitle defaults, as the web offers
 * them (apps/web/src/lib/languages.ts LANGUAGES): ISO 639-2/B codes, the
 * form ffprobe reports, named and sorted in the language shown.
 */
object TrackLanguages {
    val CODES: List<String> = listOf(
        "ara", "bul", "cat", "chi", "cze", "dan", "dut", "eng", "fin", "fre", "ger", "gre", "heb", "hin", "hrv", "hun",
        "ind", "ita", "jpn", "kor", "nor", "per", "pol", "por", "rum", "rus", "slo", "spa", "srp", "swe", "tha", "tur",
        "ukr", "vie",
    )

    /** Code and name in [words]' language, in its order of names (Spanish "Árabe" with the A's, not after "Vietnamita"). */
    fun all(words: FormatWords): List<Pair<String, String>> {
        val order = Collator.getInstance(words.locale)
        return CODES.map { it to (languageName(it, words) ?: it) }.sortedWith(compareBy(order) { it.second })
    }

    /** [code]'s name in [words]' language; `null` for none. */
    fun nameOf(code: String?, words: FormatWords): String? = languageName(code, words)
}

/** The order the web lists subtitle modes in. */
val SUBTITLE_MODES: List<SubtitleMode> = listOf(SubtitleMode.Off, SubtitleMode.Forced, SubtitleMode.Foreign, SubtitleMode.Always)

