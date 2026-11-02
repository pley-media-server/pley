package app.pley.android.screens.onboarding

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import app.pley.android.auth.AuthException
import app.pley.android.auth.Needs
import app.pley.android.auth.PinCheck
import app.pley.android.auth.Profile
import app.pley.android.auth.Profiles
import app.pley.android.auth.SignedOutException
import app.pley.android.auth.TokenException
import app.pley.android.common.describe
import app.pley.android.screens.PleyKey
import java.io.IOException
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.receiveAsFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

/** A profile on "Who's watching?". */
data class ProfileTile(
    val id: Long,
    val name: String,
    /** The picture's full address, or `null` for the initial on [color]. */
    val avatarUrl: String?,
    /** "#e07a5f"; `null`: picked from the name, as the web does. */
    val color: String?,
    val needs: Needs,
    /** The profile the app uses now. */
    val current: Boolean,
) {
    /** A lock badge: switching to it takes its PIN. */
    val locked: Boolean get() = needs == Needs.PIN

    val initial: String get() = name.trim().firstOrNull()?.uppercase() ?: "?"
}

/** Up to 8 digits; OK from 4 (PINs are 4 to 8 digits long). Never shown, never kept. */
class PinEntry {
    private val digits = StringBuilder()

    val length: Int get() = digits.length
    val complete: Boolean get() = digits.length >= MIN

    /** Adds [digit] (0 to 9); `false` once there are 8. */
    fun add(digit: Int): Boolean {
        if (digit !in 0..9 || digits.length >= MAX) return false
        digits.append('0' + digit)
        return true
    }

    fun delete(): Boolean {
        if (digits.isEmpty()) return false
        digits.deleteCharAt(digits.lastIndex)
        return true
    }

    fun clear() {
        digits.setLength(0)
    }

    /** The PIN, once; the entry is empty after. */
    fun take(): String = digits.toString().also { clear() }

    override fun toString(): String = "PinEntry($length digits)"

    companion object {
        const val MIN = 4
        const val MAX = 8
    }
}

/** What "Who's watching?" shows. */
sealed interface ProfilesStep {
    /** The household's profiles. */
    data object Pick : ProfilesStep

    /**
     * The PIN pad for [profile]: [digits] typed, the server's [error] after
     * a refusal (the dots cleared; [shakes] counts them, for the shake),
     * [lockedOut] when the PIN is locked or the profile can't be used.
     * [confirm]: the profile is the one in use (a TV starting as it).
     */
    data class Pin(
        val profile: ProfileTile,
        val digits: Int = 0,
        val error: String? = null,
        val shakes: Int = 0,
        val lockedOut: Boolean = false,
        val busy: Boolean = false,
        val confirm: Boolean = false,
    ) : ProfilesStep

    /** TVs: [profile] needs its password, so a phone approves a new code where its owner picks it. */
    data class Approve(val profile: ProfileTile) : ProfilesStep

    /** Phones: [profile] needs its password, typed on pley's page in the browser. */
    data class Browser(val profile: ProfileTile) : ProfilesStep
}

data class ProfilesUiState(
    val server: String? = null,
    val loading: Boolean = true,
    val profiles: List<ProfileTile> = emptyList(),
    /** The profile to focus first: the last one used, so OK continues. */
    val focus: Long? = null,
    /** False without the `profiles` scope: "Sign in as another profile" instead. */
    val canSwitch: Boolean = true,
    val step: ProfilesStep = ProfilesStep.Pick,
    /** On the picker: the list couldn't load, or a switch failed. */
    val error: String? = null,
    /** The profile being switched to. */
    val busy: Long? = null,
)

/**
 * "Who's watching?" and the PIN pad (phone and TV): the household from
 * GET /api/oauth/profiles (shown at once from what was saved), and the
 * switch-profile grant with a PIN when the profile has one. Wrong PINs
 * clear the dots and show the server's sentence; nothing is ever retried
 * by itself (100 wrong PINs in a row turn a PIN off). A profile that
 * needs its password is approved on a phone (TVs) or signed in to again
 * in the browser (phones).
 *
 * [gate]: a TV's cold start, where continuing as a PIN profile asks its
 * PIN (Profiles.confirmPin; servers that can't check it let it through).
 */
class ProfilesViewModel(
    private val source: OnboardingSource,
    private val tv: Boolean,
    private val gate: Boolean,
) : ViewModel() {
    private val _state = MutableStateFlow(ProfilesUiState())
    val state: StateFlow<ProfilesUiState> = _state.asStateFlow()

    private val moves = Channel<OnboardingMove>(Channel.BUFFERED)

    /** Home once a profile is chosen; signing in again when the server signed the app out. */
    val navigation: Flow<OnboardingMove> = moves.receiveAsFlow()

    private val pin = PinEntry()
    private var server: String? = null
    private var list: List<Profile> = emptyList()

    init {
        viewModelScope.launch { load() }
    }

    /** Loads the household again (Retry). */
    fun reload() {
        viewModelScope.launch { load() }
    }

    private suspend fun load() {
        val origin = server ?: source.servers.active.filterNotNull().first().also { server = it }
        _state.update { it.copy(server = origin, error = null) }
        val saved = runCatching { source.household.read(origin) }.getOrNull()
        if (!saved.isNullOrEmpty() && list.isEmpty()) show(origin, saved)
        val profiles = source.profiles(origin)
        if (!profiles.available()) {
            _state.update { it.copy(loading = false, canSwitch = false) }
            if (saved.isNullOrEmpty()) return
        }
        try {
            val fresh = profiles.list()
            source.household.write(origin, fresh)
            show(origin, fresh)
        } catch (e: CancellationException) {
            throw e
        } catch (_: SignedOutException) {
            signedOut()
        } catch (e: IOException) {
            _state.update { it.copy(loading = false, error = if (list.isEmpty()) sentence(describe(e)) else null) }
        }
    }

    private fun show(origin: String, profiles: List<Profile>) {
        list = profiles
        // A saved list may be from before a switch: the session says who the app is now.
        val me = source.session.session.value?.me?.takeIf { m -> profiles.any { it.id == m.id } }
        val tiles = profiles.map { profile ->
            val tile = profile.tile(origin)
            when {
                me == null -> tile
                profile.id == me.id -> tile.copy(current = true, needs = if (me.hasPin) Needs.PIN else Needs.NONE)
                else -> tile.copy(current = false)
            }
        }
        _state.update { s ->
            s.copy(
                loading = false,
                profiles = tiles,
                focus = s.focus?.takeIf { id -> tiles.any { it.id == id } } ?: tiles.firstOrNull { it.current }?.id ?: tiles.firstOrNull()?.id,
            )
        }
    }

    /** [id] was chosen. */
    fun pick(id: Long) {
        val tile = _state.value.profiles.firstOrNull { it.id == id } ?: return
        if (_state.value.busy != null) return
        _state.update { it.copy(focus = id, error = null) }
        when {
            tile.current -> {
                val hasPin = source.session.session.value?.me?.let { it.id == id && it.hasPin } == true
                if (gate && hasPin) openPad(tile, confirm = true) else done()
            }
            else -> when (tile.needs) {
                Needs.NONE -> switchTo(tile, null)
                Needs.PIN -> openPad(tile, confirm = false)
                Needs.PASSWORD -> _state.update { it.copy(step = if (tv) ProfilesStep.Approve(tile) else ProfilesStep.Browser(tile)) }
                Needs.LOCKED -> {
                    pin.clear()
                    _state.update { it.copy(step = ProfilesStep.Pin(tile, lockedOut = true, error = sentence(LOCKED))) }
                }
            }
        }
    }

    /** "Sign in as another profile" (no `profiles` scope), or a password profile: the way that takes. */
    fun signInAgain() {
        val me = _state.value.profiles.firstOrNull { it.current } ?: ProfileTile(0, "", null, null, Needs.PASSWORD, current = true)
        _state.update { it.copy(step = if (tv) ProfilesStep.Approve(me.copy(id = 0)) else ProfilesStep.Browser(me.copy(id = 0))) }
    }

    private fun openPad(tile: ProfileTile, confirm: Boolean) {
        pin.clear()
        _state.update { it.copy(step = ProfilesStep.Pin(tile, confirm = confirm)) }
    }

    // ---------------------------------------------------------------- the PIN pad

    fun digit(d: Int) = editPin { pin.add(d) }

    fun delete() = editPin { pin.delete() }

    fun clear() = editPin {
        pin.clear()
        true
    }

    private inline fun editPin(change: () -> Boolean) {
        val step = _state.value.step as? ProfilesStep.Pin ?: return
        if (step.lockedOut || step.busy) return
        if (change()) _state.update { it.copy(step = step.copy(digits = pin.length)) }
    }

    /** OK: sends the PIN (4 digits at least). */
    fun submit() {
        val step = _state.value.step as? ProfilesStep.Pin ?: return
        if (step.lockedOut || step.busy || !pin.complete) return
        val typed = pin.take()
        _state.update { it.copy(step = step.copy(digits = 0, busy = true, error = null)) }
        if (step.confirm) confirm(step.profile, typed) else switchTo(step.profile, typed)
    }

    /** Back on the pad, "Use another profile", or leaving the approval: the picker again. */
    fun backToPick() {
        pin.clear()
        _state.update { it.copy(step = ProfilesStep.Pick, busy = null) }
    }

    // ---------------------------------------------------------------- switching

    private fun switchTo(tile: ProfileTile, typed: String?) {
        _state.update { it.copy(busy = tile.id) }
        viewModelScope.launch {
            try {
                source.session.switchProfile(tile.id, typed)
                done()
            } catch (e: CancellationException) {
                throw e
            } catch (_: SignedOutException) {
                signedOut()
            } catch (e: TokenException) {
                refused(tile, e)
            } catch (e: AuthException) {
                failed(tile, e.message.orEmpty())
            } catch (e: IOException) {
                unreachable(tile, e)
            } finally {
                _state.update { it.copy(busy = null) }
            }
        }
    }

    private fun confirm(tile: ProfileTile, typed: String) {
        val origin = server ?: return
        viewModelScope.launch {
            val profile = list.firstOrNull { it.id == tile.id } ?: Profile(tile.id, tile.name, null, tile.color, current = true, needs = Needs.NONE)
            try {
                when (val check = source.profiles(origin).confirmPin(profile, typed)) {
                    PinCheck.Confirmed, PinCheck.Unsupported -> done()
                    is PinCheck.Failed -> wrong(tile, check.message, lockedOut = PIN_LOCKED_WORDS in check.message)
                }
            } catch (e: CancellationException) {
                throw e
            } catch (_: SignedOutException) {
                signedOut()
            } catch (e: IOException) {
                unreachable(tile, e)
            }
        }
    }

    /** The server refused the switch: by its reason. */
    private fun refused(tile: ProfileTile, e: TokenException) {
        val oauth = e.oauth
        when (oauth?.reason) {
            "browser" -> _state.update { it.copy(step = if (tv) ProfilesStep.Approve(tile) else ProfilesStep.Browser(tile)) }
            "pin_locked" -> wrong(tile, Profiles.message(oauth), lockedOut = true)
            "throttled" -> wrong(tile, Profiles.message(oauth), lockedOut = false)
            "wrong_pin" -> if (tile.needs == Needs.PIN || _state.value.step is ProfilesStep.Pin) {
                wrong(tile, Profiles.message(oauth), lockedOut = false)
            } else {
                // The list was older than the profile's new PIN.
                openPad(tile.copy(needs = Needs.PIN), confirm = false)
            }
            else -> failed(tile, Profiles.message(oauth))
        }
    }

    private fun wrong(tile: ProfileTile, message: String, lockedOut: Boolean) {
        pin.clear()
        _state.update { s ->
            val step = s.step as? ProfilesStep.Pin ?: ProfilesStep.Pin(tile)
            s.copy(step = step.copy(digits = 0, busy = false, error = sentence(message), shakes = step.shakes + 1, lockedOut = lockedOut))
        }
    }

    /**
     * The server couldn't be asked: not a wrong PIN, so the pad doesn't
     * shake; it says so, and the PIN can be typed again once it answers.
     */
    private fun unreachable(tile: ProfileTile, e: IOException) {
        val message = sentence("can't reach your server: ${describe(e)}")
        pin.clear()
        _state.update { s ->
            when (val step = s.step) {
                is ProfilesStep.Pin -> s.copy(step = step.copy(digits = 0, busy = false, error = message))
                else -> s.copy(error = message)
            }
        }
    }

    private fun failed(tile: ProfileTile, message: String) {
        if (_state.value.step is ProfilesStep.Pin) {
            wrong(tile, message, lockedOut = false)
        } else {
            _state.update { it.copy(error = sentence(message)) }
        }
    }

    private fun done() {
        ProfileGate.passed = true
        pin.clear()
        moves.trySend(OnboardingMove.Restart(PleyKey.Home))
    }

    private fun signedOut() {
        val origin = server ?: return
        moves.trySend(OnboardingMove.Restart(if (tv) PleyKey.Link(origin) else PleyKey.SignIn(origin)))
    }

    private companion object {
        const val LOCKED = "this profile can't be used from the app"

        /** How the sign-in module words a PIN locked after too many wrong tries (Profiles.message). */
        const val PIN_LOCKED_WORDS = "PIN is locked"
    }
}

/** [this] as a tile, its picture's address made whole against [origin]. */
internal fun Profile.tile(origin: String) = ProfileTile(
    id = id,
    name = name,
    avatarUrl = avatarUrl?.let { if (it.startsWith("http://") || it.startsWith("https://")) it else origin.trimEnd('/') + "/" + it.trimStart('/') },
    color = avatarColor,
    needs = needs,
    current = current,
)
