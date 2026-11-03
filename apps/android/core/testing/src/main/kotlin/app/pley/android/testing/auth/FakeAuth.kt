package app.pley.android.testing.auth

import android.app.Activity
import app.pley.android.auth.AuthEvent
import app.pley.android.auth.AuthEvents
import app.pley.android.auth.AuthGraph
import app.pley.android.auth.DeviceCode
import app.pley.android.auth.DevicePoll
import app.pley.android.auth.DeviceSignIn
import app.pley.android.auth.Profiles
import app.pley.android.auth.Shelf
import app.pley.android.auth.SignInState
import app.pley.android.auth.SignOutReason
import app.pley.android.auth.TokenStore
import app.pley.android.auth.Tokens
import app.pley.android.auth.Vault
import java.util.concurrent.ConcurrentHashMap
import javax.crypto.KeyGenerator
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.flow
import okhttp3.OkHttpClient

/** A shelf in memory, as the sign-in's preferences file would be. */
class MemoryShelf : Shelf {
    private val values = ConcurrentHashMap<String, String>()

    override fun get(name: String): String? = values[name]

    override fun put(name: String, value: String) {
        values[name] = value
    }

    override fun remove(name: String) {
        values.remove(name)
    }
}

/** A vault with a software key, as the Keystore's would be on a device. */
fun softwareVault(): Vault {
    val key = KeyGenerator.getInstance("AES").apply { init(256) }.generateKey()
    return Vault { key }
}

/**
 * The TV's sign-in with a code, as a test drives it: [start] gives
 * [code] (or throws [startError]), and [poll] says Waiting, then what the
 * test sends with [answer].
 */
class FakeDeviceSignIn : DeviceSignIn {
    val started = mutableListOf<String>()
    var startError: Throwable? = null
    var code: (String) -> DeviceCode = { server ->
        DeviceCode(
            server = server,
            deviceCode = "device-code",
            userCode = "BCDF-GHJK",
            verificationUri = "$server/link",
            verificationUriComplete = "$server/link?code=BCDFGHJK",
            expiresAt = Long.MAX_VALUE,
            intervalSeconds = 5,
        )
    }
    private val answers = Channel<DevicePoll>(Channel.UNLIMITED)

    /** What the next poll says after Waiting; anything but Waiting or Unreachable ends it. */
    fun answer(poll: DevicePoll) {
        answers.trySend(poll)
    }

    override suspend fun start(server: String): DeviceCode {
        started += server
        startError?.let { throw it }
        return code(server)
    }

    override fun poll(code: DeviceCode): Flow<DevicePoll> = flow {
        emit(DevicePoll.Waiting)
        for (poll in answers) {
            emit(poll)
            if (poll != DevicePoll.Waiting && poll !is DevicePoll.Unreachable) break
        }
    }
}

/**
 * The sign-in module for tests: servers in [signedInTo] count as signed
 * in, tokens live in memory (saving them needs org.json on the test's
 * classpath: libs.json), and sign-outs are sent with [signOut].
 */
class FakeAuthGraph(private val http: OkHttpClient = OkHttpClient()) : AuthGraph {
    val signedInTo: MutableSet<String> = ConcurrentHashMap.newKeySet()
    val store = TokenStore(MemoryShelf(), softwareVault())
    private val tokens = ConcurrentHashMap<String, Tokens>()
    private val sink = AuthEvents()

    /** The browser sign-ins asked for: the server, and whether in a private tab. */
    val browserSignIns = mutableListOf<Pair<String, Boolean>>()

    override fun tokens(issuer: String): Tokens = tokens.getOrPut(issuer) { Tokens(issuer, store, http, sink) }

    override fun hasTokens(issuer: String): Boolean = issuer in signedInTo || store.has(issuer)

    override fun profiles(issuer: String, api: OkHttpClient): Profiles = Profiles(api, tokens(issuer))

    override val deviceSignIn = FakeDeviceSignIn()

    override suspend fun signInWithBrowser(activity: Activity, server: String, privateTab: Boolean) {
        browserSignIns += server to privateTab
    }

    override val signInState = MutableStateFlow<SignInState>(SignInState.Idle)

    override val events: SharedFlow<AuthEvent> = sink.events

    /** [issuer] signed the app out (the server ended it, or the tokens can't be read). */
    fun signOut(issuer: String, reason: SignOutReason = SignOutReason.ENDED) {
        signedInTo -= issuer
        store.wipe(issuer)
        sink.signedOut(issuer, reason)
    }
}
