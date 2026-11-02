package app.pley.android.screens.onboarding

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import app.pley.android.common.FoundServer
import app.pley.android.common.SavedServer
import app.pley.android.common.Servers
import app.pley.android.network.DiscoveryEvent
import app.pley.android.network.OfflineException
import app.pley.android.network.ProbeResult
import java.net.URI
import java.net.URISyntaxException
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Job
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.receiveAsFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

/** One server in the list: saved, found on the network, or both. */
data class ServerRow(
    val url: String,
    val name: String,
    /** Its host and port, as people type it ("192.168.1.20:7539"), never the scheme. */
    val address: String,
    val saved: Boolean,
    val found: Boolean,
    /** Found, not saved, and named like a saved server it isn't. */
    val lookalike: Boolean = false,
    /** Plain http beyond the home network. */
    val unencrypted: Boolean = false,
    /** A saved server that didn't answer. */
    val unreachable: Boolean = false,
    val id: String? = null,
    /** It requires two-step sign-in, as far as the app knows (the phone's chosen card is asked). */
    val twoStepRequired: Boolean = false,
)

/** Whether the app may reach devices on the home network (Android 17, Fire OS 16). */
enum class LocalAccess {
    Allowed,

    /** The screen explains why before the system asks (TVs). */
    Explain,

    /** The system's question is on screen. */
    Asking,

    /** Refused: the screen says so, with a way to the app's settings. */
    Denied,
}

/** Something that went wrong, for the screen to word. */
sealed interface ServersProblem {
    /** Nothing answered at [address]: [message] says how. */
    data class Unreachable(val address: String, val message: String) : ServersProblem

    /** Something answers at [address], and it isn't pley. */
    data class NotPley(val address: String) : ServersProblem

    /** [address] is plain http beyond the home network: connecting again goes ahead. */
    data class Unencrypted(val address: String) : ServersProblem

    /** What was typed can't be an address. */
    data object BadAddress : ServersProblem

    /** The demo server didn't start. */
    data class Failed(val message: String) : ServersProblem
}

data class ServersUiState(
    /** Saved servers first, most recently used first, then the ones found. */
    val rows: List<ServerRow> = emptyList(),
    /** Asking the network who's there (1.5 s). */
    val searching: Boolean = false,
    /** A search ended at least once. */
    val searched: Boolean = false,
    /** "Search this network" is running: every address of the device's network is asked. */
    val sweeping: Boolean = false,
    /** The addresses a sweep asks: "192.168.1.1-254"; `null` when the network isn't known. */
    val sweepRange: String? = null,
    val access: LocalAccess = LocalAccess.Allowed,
    /** Permissions for the screen to ask the system for now; it calls [ServersViewModel.permissionsAnswered] after. */
    val ask: List<String> = emptyList(),
    /** The server being opened. */
    val busy: String? = null,
    val problem: ServersProblem? = null,
    /** About the typed address. */
    val addressProblem: ServersProblem? = null,
    val checkingAddress: Boolean = false,
    /** "Use demo server" is offered (debug builds). */
    val demo: Boolean = false,
    /** The phone's chosen card, signed in to with "Continue in browser". */
    val selected: String? = null,
)

/** A server was chosen and saved as the one in use. */
data class ChosenServer(
    val url: String,
    val name: String,
    /** The app holds tokens for it already. */
    val signedIn: Boolean,
    /** Its household's size as last seen; 1 while unknown. */
    val profiles: Int,
)

/**
 * The list of servers (phone and TV): saved ones first, then the ones that
 * answer on the network as they come, an address to type (a name on the
 * internet is tried with https first), "Search this network", forgetting a
 * saved server (which ends its sign-in), and the demo server in debug
 * builds. The chosen server is asked once whether it requires two-step
 * sign-in, for the phone to say so before the browser opens.
 *
 * Nothing reaches the home network before the system allows it (Android
 * 17's local network access, Fire OS 16's nearby devices): [explainFirst]
 * (TVs) explains why before the system asks; phones ask at once, once per
 * process. Servers on the internet need no such permission.
 */
class ServersViewModel(
    private val source: OnboardingSource,
    private val explainFirst: Boolean,
    preselect: String? = null,
) : ViewModel() {
    private val _state = MutableStateFlow(ServersUiState(demo = source.demo != null, selected = preselect))
    val state: StateFlow<ServersUiState> = _state.asStateFlow()

    private val chosenChannel = Channel<ChosenServer>(Channel.BUFFERED)

    /** Servers chosen: the screen goes on to sign in, or in. */
    val chosen: Flow<ChosenServer> = chosenChannel.receiveAsFlow()

    private var saved: List<SavedServer> = emptyList()
    private val found = LinkedHashMap<String, FoundServer>()
    private val unreachable = mutableSetOf<String>()

    /** Servers typed in or started here (the demo): listed, but not "on this network". */
    private val typed = mutableSetOf<String>()

    /** Saved servers being asked directly. */
    private val checking = mutableSetOf<String>()

    /** What servers said of two-step sign-in, by address: each is asked once, and false until it answers. */
    private val twoStep = mutableMapOf<String, Boolean>()
    private var searchJob: Job? = null
    private var sweepJob: Job? = null

    /** What to do once the system allows the home network. */
    private var pending: (() -> Unit)? = null

    /** The unencrypted address the person was warned about: connecting again goes ahead. */
    private var warnedAbout: String? = null

    init {
        viewModelScope.launch {
            source.servers.saved.collect {
                saved = it
                publish()
                checkSaved()
            }
        }
        viewModelScope.launch {
            val subnet = runCatching { source.subnet() }.getOrNull()
            _state.update { it.copy(sweepRange = subnet?.let { s -> "${s}1-254" }) }
        }
        beginSearch()
    }

    /** Looks for servers, once the home network may be reached. */
    private fun beginSearch() {
        withAccess(source.localNetwork.missing()) { search() }
    }

    /** Asks the network again who's there. */
    fun search() {
        if (searchJob?.isActive == true) return
        _state.update { it.copy(searching = true) }
        searchJob = viewModelScope.launch {
            try {
                source.discovery.search().collect { event ->
                    when (event) {
                        is DiscoveryEvent.Found -> {
                            found[event.server.url] = event.server
                            typed -= event.server.url
                            unreachable -= event.server.url
                            publish()
                        }
                        DiscoveryEvent.Blocked -> _state.update { it.copy(access = LocalAccess.Denied) }
                        DiscoveryEvent.Done -> _state.update { it.copy(searching = false, searched = true) }
                    }
                }
            } finally {
                _state.update { it.copy(searching = false, searched = true) }
            }
        }
    }

    /** "Search this network": every address of the device's network, for servers that never hear the broadcast. */
    fun searchNetwork() {
        if (sweepJob?.isActive == true) return
        withAccess(source.localNetwork.missing()) {
            _state.update { it.copy(sweeping = true) }
            sweepJob = viewModelScope.launch {
                try {
                    source.sweep.sweep().collect { server ->
                        found[server.url] = server
                        typed -= server.url
                        unreachable -= server.url
                        publish()
                    }
                } catch (e: OfflineException) {
                    if (e.blocked) _state.update { it.copy(access = LocalAccess.Denied) }
                } finally {
                    _state.update { it.copy(sweeping = false, searched = true) }
                }
            }
        }
    }

    /** The phone's card [url] is the one "Continue in browser" signs in to. */
    fun select(url: String) {
        _state.update { it.copy(selected = url, problem = null) }
        askTwoStep()
    }

    /**
     * Opens [url]: checks it answers as pley, saves it as the server in
     * use, and says so ([chosen]). A server at home waits for the system
     * to allow the home network.
     */
    fun choose(url: String) {
        if (_state.value.busy != null) return
        withAccess(source.localNetwork.missingFor(url)) {
            _state.update { it.copy(busy = url, problem = null) }
            viewModelScope.launch {
                try {
                    when (val answer = source.probe.probe(url)) {
                        is ProbeResult.Pley -> keep(answer.url, answer.info.name, idOf(url))
                        is ProbeResult.NotPley -> _state.update { it.copy(problem = ServersProblem.NotPley(addressOf(answer.url))) }
                        is ProbeResult.Unreachable -> {
                            unreachable += url
                            publish()
                            _state.update { it.copy(problem = ServersProblem.Unreachable(addressOf(url), answer.message)) }
                        }
                        ProbeResult.Blocked -> {
                            _state.update { it.copy(access = LocalAccess.Denied) }
                            requestAccess(source.localNetwork.missing().ifEmpty { source.localNetwork.required() }) { choose(url) }
                        }
                    }
                } finally {
                    _state.update { it.copy(busy = null) }
                }
            }
        }
    }

    /**
     * Connects to what was typed: `nas`, `192.168.1.20`, `pley.example.com`
     * (https first) or a full address. Plain http beyond the home network
     * is warned about once; connecting again goes ahead. With [open] the
     * server is opened at once (TVs); otherwise it joins the list, chosen
     * (phones).
     */
    fun connect(text: String, open: Boolean) {
        if (_state.value.checkingAddress) return
        val candidates = Servers.candidates(text)
        if (candidates.isEmpty()) {
            _state.update { it.copy(addressProblem = ServersProblem.BadAddress) }
            return
        }
        val missing = candidates.flatMap { source.localNetwork.missingFor(it) }.distinct()
        withAccess(missing) {
            _state.update { it.copy(checkingAddress = true, addressProblem = null) }
            viewModelScope.launch {
                try {
                    when (val answer = source.probe.probe(text)) {
                        is ProbeResult.Pley -> {
                            val url = answer.url
                            if (Servers.unencrypted(url) && warnedAbout != url) {
                                warnedAbout = url
                                _state.update { it.copy(addressProblem = ServersProblem.Unencrypted(addressOf(url))) }
                            } else {
                                found[url] = FoundServer(url, answer.info.name, id = null, version = answer.info.version)
                                typed += url
                                unreachable -= url
                                // Just asked: choosing it doesn't ask again.
                                twoStep[url] = answer.info.twoStepRequired
                                publish()
                                _state.update { it.copy(selected = url) }
                                if (open) keep(url, answer.info.name, idOf(url))
                            }
                        }
                        is ProbeResult.NotPley -> _state.update { it.copy(addressProblem = ServersProblem.NotPley(addressOf(answer.url))) }
                        is ProbeResult.Unreachable ->
                            _state.update { it.copy(addressProblem = ServersProblem.Unreachable(addressOf(candidates.last()), answer.message)) }
                        ProbeResult.Blocked -> {
                            _state.update { it.copy(access = LocalAccess.Denied) }
                            requestAccess(source.localNetwork.missing().ifEmpty { source.localNetwork.required() }) { connect(text, open) }
                        }
                    }
                } finally {
                    _state.update { it.copy(checkingAddress = false) }
                }
            }
        }
    }

    /** The typed address changed: its problem goes. */
    fun addressEdited() {
        if (_state.value.addressProblem != null) _state.update { it.copy(addressProblem = null) }
    }

    /** Forgets the saved server [url]: its sign-in is ended there, and what was saved from it deleted. */
    fun forget(url: String) {
        viewModelScope.launch {
            try {
                source.servers.forget(url)
            } catch (e: CancellationException) {
                throw e
            } catch (_: Exception) {
                // The tokens are gone whatever the server answered (Tokens.signOut).
            }
            if (_state.value.selected == url) _state.update { it.copy(selected = null) }
        }
    }

    /** "Use demo server": the fake server in the app's process (debug builds). */
    fun useDemo() {
        val demo = source.demo ?: return
        if (_state.value.busy != null) return
        _state.update { it.copy(busy = DEMO, problem = null) }
        viewModelScope.launch {
            val url = try {
                demo()
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                _state.update { it.copy(busy = null, problem = ServersProblem.Failed(e.message ?: e.javaClass.simpleName)) }
                return@launch
            }
            _state.update { it.copy(busy = null) }
            when (val answer = source.probe.probe(url)) {
                is ProbeResult.Pley -> {
                    found[answer.url] = FoundServer(answer.url, answer.info.name, id = null, version = answer.info.version)
                    typed += answer.url
                    publish()
                    keep(answer.url, answer.info.name, null)
                }
                else -> _state.update { it.copy(problem = ServersProblem.Failed("the demo server didn't answer")) }
            }
        }
    }

    // ---------------------------------------------------------------- local network access

    /** The TV's explanation was read: the system asks now. */
    fun explained() {
        val missing = source.localNetwork.missing()
        if (missing.isEmpty()) {
            permissionsAnswered()
        } else {
            askedThisProcess = true
            _state.update { it.copy(access = LocalAccess.Asking, ask = missing) }
        }
    }

    /** The system's question was answered (or the screen came back from Settings). */
    fun permissionsAnswered() {
        val missing = source.localNetwork.missing()
        if (missing.isEmpty()) {
            _state.update { it.copy(access = LocalAccess.Allowed, ask = emptyList()) }
            val next = pending
            pending = null
            next?.invoke()
            if (!_state.value.searched && searchJob?.isActive != true) search()
            checkSaved()
        } else {
            pending = null
            _state.update { it.copy(access = LocalAccess.Denied, ask = emptyList()) }
        }
    }

    /** The screen is shown again: allowed in Settings meanwhile, the network is searched. */
    fun resumed() {
        if (_state.value.access == LocalAccess.Denied && source.localNetwork.missing().isEmpty()) permissionsAnswered()
    }

    /** Runs [then] now when nothing in [missing] is missing, else once the system allows it. */
    private fun withAccess(missing: List<String>, then: () -> Unit) {
        if (missing.isEmpty()) then() else requestAccess(missing, then)
    }

    private fun requestAccess(missing: List<String>, then: () -> Unit) {
        pending = then
        when {
            explainFirst && !askedThisProcess -> _state.update { it.copy(access = LocalAccess.Explain) }
            !askedThisProcess -> {
                askedThisProcess = true
                _state.update { it.copy(access = LocalAccess.Asking, ask = missing) }
            }
            // Asked already in this process: the screen says how to allow it.
            else -> _state.update { it.copy(access = LocalAccess.Denied) }
        }
    }

    /** The system's question was shown; the screen launched it. */
    fun asked() {
        _state.update { it.copy(ask = emptyList()) }
    }

    // ---------------------------------------------------------------- internals

    private suspend fun keep(url: String, name: String, id: String?) {
        source.servers.use(SavedServer(url, name, id, lastUsed = 0))
        val profiles = runCatching { source.household.read(url)?.size }.getOrNull() ?: 1
        chosenChannel.send(ChosenServer(url, name, signedIn = source.hasTokens(url), profiles = profiles.coerceAtLeast(1)))
    }

    /** Saved servers that didn't answer the search are asked directly, where the app may reach them. */
    private fun checkSaved() {
        for (server in saved) {
            if (found.containsKey(server.url) || server.url in checking) continue
            if (source.localNetwork.missingFor(server.url).isNotEmpty()) continue
            checking += server.url
            viewModelScope.launch {
                try {
                    val answer = source.probe.probe(server.url)
                    if (answer is ProbeResult.Pley) unreachable -= server.url else unreachable += server.url
                    publish()
                } finally {
                    checking -= server.url
                }
            }
        }
    }

    /**
     * Asks the chosen server whether it requires two-step sign-in, once:
     * in the background, so Continue never waits for it, and only once the
     * app may reach it.
     */
    private fun askTwoStep() {
        val url = _state.value.selected ?: return
        if (url in twoStep || source.localNetwork.missingFor(url).isNotEmpty()) return
        twoStep[url] = false
        viewModelScope.launch {
            if (source.twoStepRequired(url)) {
                twoStep[url] = true
                publish()
            }
        }
    }

    private fun idOf(url: String): String? = found[url]?.id ?: saved.firstOrNull { it.url == url }?.id

    private fun publish() {
        val rows = Servers.merge(saved, found.values.toList()).map { entry ->
            ServerRow(
                url = entry.url,
                name = entry.name,
                address = addressOf(entry.url),
                saved = entry.saved,
                found = entry.found && entry.url !in typed,
                lookalike = entry.lookalike,
                unencrypted = Servers.unencrypted(entry.url),
                unreachable = !entry.found && entry.url in unreachable,
                id = entry.id,
                twoStepRequired = twoStep[entry.url] == true,
            )
        }
        _state.update { s ->
            val selected = s.selected?.takeIf { url -> rows.any { it.url == url } } ?: rows.firstOrNull()?.url
            s.copy(rows = rows, selected = selected)
        }
        askTwoStep()
    }

    companion object {
        /** [ServersUiState.busy] while the demo server starts. */
        const val DEMO = "demo"

        /** The system asks once per process; after a no, the screen says how to allow it. */
        @Volatile
        private var askedThisProcess = false

        /** Forgets that the question was asked (tests). */
        internal fun resetAsked() {
            askedThisProcess = false
        }

        /**
         * A server's address as people type it: the host and its port
         * ("192.168.1.20:7539", "pley.example.com"), without the scheme;
         * unlike Servers.displayAddress, pley's own port is kept.
         */
        fun addressOf(url: String): String {
            val uri = try {
                URI(url)
            } catch (_: URISyntaxException) {
                return url
            }
            val host = uri.host ?: return url
            val shown = if (host.contains(':')) "[$host]" else host
            return if (uri.port == -1) shown else "$shown:${uri.port}"
        }
    }
}
