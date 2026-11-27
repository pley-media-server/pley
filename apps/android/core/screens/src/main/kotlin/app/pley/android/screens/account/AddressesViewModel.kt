package app.pley.android.screens.account

import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import app.pley.android.auth.AddressSource
import app.pley.android.auth.AddressStatus
import app.pley.android.auth.AddressesState
import app.pley.android.auth.Tokens
import app.pley.android.common.AddressKind
import app.pley.android.common.Servers
import app.pley.android.data.SessionRepository
import app.pley.android.screens.screensGraph
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.flowOn
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/** One of the server's addresses, as Addresses shows it. */
data class AddressRow(
    val url: String,
    /** Its host and, unless it is Pley's, its port: `192.168.1.20`, `pley.example.com`. */
    val shown: String,
    val status: AddressStatus,
    /** Added on this device, so removed here; the server's own are changed on its web app. */
    val removable: Boolean,
)

/**
 * Why an address typed wasn't added: not an address; the internet's, which
 * only the server's own list gives; or one of the server's already.
 */
enum class AddressProblem { NOT_AN_ADDRESS, INTERNET, KNOWN }

/** Addresses' state (both interfaces), and the Me row's. */
data class AddressesUiState(
    /** The server, by the address signed in at; `null` while signed out. */
    val server: String? = null,
    /** The address in use, as shown; `null` while signed out. */
    val inUse: String? = null,
    /** The address in use itself, for what opens the server in the browser. */
    val inUseUrl: String? = null,
    /** Whether the address in use is a home network's ("At home"), else a VPN's or the internet's ("Away"). */
    val atHome: Boolean = false,
    /** The addresses at home, in the order they're tried. */
    val home: List<AddressRow> = emptyList(),
    /** The others (a VPN's and the internet's), in the order they're tried. */
    val away: List<AddressRow> = emptyList(),
    /** Every address is proving itself again. */
    val checking: Boolean = false,
    /** A Pley from before proofs can't tell its other addresses from another device's: it keeps to one. */
    val canMove: Boolean = true,
    val adding: Boolean = false,
    val problem: AddressProblem? = null,
) {
    /** More than the address signed in at. */
    val several: Boolean get() = home.size + away.size > 1
}

/**
 * Addresses (both interfaces; docs/ANDROID.md, "Home and away"): where the
 * server answers, which the app uses, and what it knows of the others.
 * Opening it has every address prove itself again; an address added here
 * proves itself as it is added.
 */
@OptIn(ExperimentalCoroutinesApi::class)
class AddressesViewModel(
    session: SessionRepository,
    private val tokensOf: (issuer: String) -> Tokens,
    private val io: CoroutineDispatcher = Dispatchers.IO,
) : ViewModel() {
    private val checking = MutableStateFlow(false)
    private val adding = MutableStateFlow(false)
    private val problem = MutableStateFlow<AddressProblem?>(null)
    private val canMove = MutableStateFlow(true)
    private val issuer: Flow<String?> = session.session.map { it?.issuer }.distinctUntilChanged()

    private val addresses: Flow<Pair<String, AddressesState>?> = issuer.flatMapLatest { issuer ->
        if (issuer == null) flowOf(null) else tokensOf(issuer).addresses.map { issuer to it }
    }.flowOn(io)

    val state: StateFlow<AddressesUiState> = combine(addresses, checking, adding, canMove, problem) { held, busy, add, movable, why ->
        val (server, known) = held ?: return@combine AddressesUiState(checking = busy, adding = add, canMove = movable, problem = why)
        val rows = known.addresses.map { (address, status) ->
            address.kind to AddressRow(
                url = address.url,
                shown = Servers.displayAddress(address.url),
                status = status,
                removable = address.source == AddressSource.ADDED,
            )
        }
        AddressesUiState(
            server = server,
            inUse = Servers.displayAddress(known.inUse),
            inUseUrl = known.inUse,
            atHome = Servers.kindOf(known.inUse) == AddressKind.HOME,
            home = rows.filter { it.first == AddressKind.HOME }.map { it.second },
            away = rows.filter { it.first != AddressKind.HOME }.map { it.second },
            checking = busy,
            canMove = movable,
            adding = add,
            problem = why,
        )
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), AddressesUiState())

    private val current: String? get() = state.value.server

    /** Every address proves itself again, now. */
    fun check() {
        val server = current ?: return
        if (checking.value) return
        checking.value = true
        viewModelScope.launch {
            try {
                withContext(io) {
                    val tokens = tokensOf(server)
                    canMove.value = tokens.movesBetweenAddresses()
                    tokens.checkAddresses()
                }
            } catch (e: CancellationException) {
                throw e
            } catch (_: Exception) {
                // What each address answered shows in the list.
            } finally {
                checking.value = false
            }
        }
    }

    /**
     * Adds what was typed (`nas`, `192.168.1.20`, a Tailscale name): a home
     * network's address or a VPN's. The internet's come only from the
     * server's own list, which its administrator keeps. [added] once it is.
     */
    fun add(typed: String, added: () -> Unit) {
        val server = current ?: return
        val url = Servers.candidates(typed).firstOrNull()
        when {
            url == null -> problem.value = AddressProblem.NOT_AN_ADDRESS
            Servers.kindOf(url) == AddressKind.INTERNET -> problem.value = AddressProblem.INTERNET
            else -> viewModelScope.launch {
                adding.value = true
                try {
                    if (tokensOf(server).addAddress(url)) {
                        problem.value = null
                        added()
                        check()
                    } else {
                        problem.value = AddressProblem.KNOWN
                    }
                } finally {
                    adding.value = false
                }
            }
        }
    }

    /** Removes [url], added on this device. */
    fun remove(url: String) {
        val server = current ?: return
        viewModelScope.launch { tokensOf(server).removeAddress(url) }
    }

    /** The address field changed: its problem goes. */
    fun edited() {
        problem.value = null
    }

    companion object {
        val factory: ViewModelProvider.Factory = viewModelFactory {
            initializer {
                val g = screensGraph()
                AddressesViewModel(g.data.session, g.auth::tokens)
            }
        }
    }
}
