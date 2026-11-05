package app.pley.android.auth

import android.content.Context
import android.net.ConnectivityManager
import android.net.Network
import android.net.NetworkCapabilities
import android.net.NetworkRequest
import app.pley.android.common.Addresses
import app.pley.android.common.AppLanguage
import app.pley.android.common.UiText
import app.pley.android.common.UserFacing
import java.io.IOException
import java.security.MessageDigest
import java.security.SecureRandom
import java.util.Base64
import java.util.concurrent.atomic.AtomicInteger
import javax.crypto.Mac
import javax.crypto.spec.SecretKeySpec
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import okhttp3.OkHttpClient
import okhttp3.Request
import org.json.JSONException
import org.json.JSONObject

/**
 * What tells the server from another device at its address later
 * (docs/API.md, "Telling servers apart"): its id, and the key of this
 * sign-in with which it proves it is the one signed in to.
 */
data class ServerProof(val serverId: String, val proofId: String, val proofKey: String) {
    // Never print the key.
    override fun toString(): String = "ServerProof(serverId=$serverId, proofId=$proofId)"

    companion object {
        private val PROOF_ID = Regex("^[1-9][0-9]{0,18}$")
        private val BASE64URL = Regex("^[A-Za-z0-9_-]*$")

        /** The server's id and proof key in [o], when it has them and they are written as they should be. */
        fun of(o: JSONObject): ServerProof? {
            val serverId = text(o, "server_id") ?: return null
            val proofId = text(o, "proof_id")?.takeIf { PROOF_ID.matches(it) } ?: return null
            val proofKey = text(o, "proof_key")?.takeIf { keyBytes(it) != null } ?: return null
            return ServerProof(serverId, proofId, proofKey)
        }

        /** The 32 bytes of a proof key written in base64url without padding; `null` for anything else. */
        internal fun keyBytes(key: String): ByteArray? {
            if (!BASE64URL.matches(key)) return null
            val bytes = try {
                Base64.getUrlDecoder().decode(key)
            } catch (_: IllegalArgumentException) {
                return null
            }
            return bytes.takeIf { it.size == KEY_BYTES }
        }

        private fun text(o: JSONObject, name: String): String? =
            if (o.isNull(name)) null else (o.opt(name) as? String)?.ifEmpty { null }

        private const val KEY_BYTES = 32
    }
}

/** Why a token isn't sent to a server's address: what answers there can't prove it is the server signed in to. */
val NOT_YOUR_SERVER: UiText = UiText.res(R.string.auth_not_your_server)

/**
 * What answers at the server's address can't prove it is the server signed
 * in to. Not an [AuthException]: the app treats it as the server being out
 * of reach, shows what it saved and tries again later.
 */
class NotYourServerException : IOException(), UserFacing {
    override val text: UiText get() = NOT_YOUR_SERVER
    override val message: String get() = AppLanguage.wordsOf(text)
}

/** The server's proofs, as the app checks them. */
object ServerProofs {
    /** How long a server's proof counts, unless a request to it gets no answer or the network changes meanwhile. */
    const val TTL_MS = 30 * 60_000L

    private val networks = AtomicInteger()

    /** Counts the networks the device joined or lost: a proof counts on the network it was made on. */
    val network: Int get() = networks.get()

    /** The device may be on another network: every server proves itself again before its next token. */
    fun networkMayHaveChanged() {
        networks.incrementAndGet()
    }

    /**
     * Calls [networkMayHaveChanged] each time the device joins or loses its
     * default network, or any Wi-Fi or Ethernet network: the home network
     * is reached through the Wi-Fi even when mobile data is the default (a
     * Wi-Fi without internet), and moving to another such Wi-Fi doesn't
     * change the default.
     */
    fun followNetworks(context: Context) {
        val connectivity = context.getSystemService(ConnectivityManager::class.java) ?: return
        fun changes() = object : ConnectivityManager.NetworkCallback() {
            override fun onAvailable(network: Network) = networkMayHaveChanged()

            override fun onLost(network: Network) = networkMayHaveChanged()
        }
        val homes = NetworkRequest.Builder()
            .addTransportType(NetworkCapabilities.TRANSPORT_WIFI)
            .addTransportType(NetworkCapabilities.TRANSPORT_ETHERNET)
            // A Wi-Fi without internet counts too.
            .removeCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET)
            .build()
        try {
            connectivity.registerDefaultNetworkCallback(changes())
            connectivity.registerNetworkCallback(homes, changes())
        } catch (_: RuntimeException) {
            // Too many callbacks registered app-wide: sign-in still works, and
            // a proof still lasts half an hour at most, or until a request
            // gets no answer.
        }
    }

    /** The longest answer read: `/api/server`'s is a few hundred bytes. */
    private const val MAX_ANSWER_BYTES = 64 * 1024L

    private val secureRandom = SecureRandom()

    /** [n] random bytes from the system. */
    fun randomBytes(n: Int): ByteArray = ByteArray(n).also(secureRandom::nextBytes)

    /** The proof of [serverId], asked at [address], for [challenge] under [key], in base64url without padding. */
    fun proof(key: ByteArray, serverId: String, address: String, challenge: String): String {
        val mac = Mac.getInstance("HmacSHA256")
        mac.init(SecretKeySpec(key, "HmacSHA256"))
        val digest = mac.doFinal("pley-server-proof\n$serverId\n$address\n$challenge".toByteArray(Charsets.UTF_8))
        return Base64.getUrlEncoder().withoutPadding().encodeToString(digest)
    }

    /**
     * Asks the server at [issuer] to prove it holds [proof]'s key. Throws
     * an IOException when nothing answers or the server (or a proxy before
     * it) fails for now, or [NotYourServerException] when what answers
     * can't prove it.
     */
    suspend fun prove(http: OkHttpClient, issuer: String, proof: ServerProof, random: (Int) -> ByteArray) {
        val key = ServerProof.keyBytes(proof.proofKey) ?: throw NotYourServerException()
        val challenge = Base64.getUrlEncoder().withoutPadding().encodeToString(random(CHALLENGE_BYTES))
        // The address as the server reads it from the request: the proof covers it.
        val address = Addresses.origin(issuer) ?: issuer.trimEnd('/')
        val request = Request.Builder()
            .url("$address/api/server?challenge=$challenge&proof_id=${proof.proofId}")
            .header("Accept", "application/json")
            .build()
        val text = withContext(Dispatchers.IO) {
            http.newCall(request).execute().use { response ->
                if (response.code >= 500) throw IOException("the server couldn't answer (HTTP ${response.code})")
                if (response.code != 200) throw NotYourServerException()
                response.peekBody(MAX_ANSWER_BYTES).string()
            }
        }
        val answer = try {
            JSONObject(text)
        } catch (_: JSONException) {
            throw NotYourServerException()
        }
        val id = answer.opt("id") as? String
        val given = answer.opt("proof") as? String
        if (id != proof.serverId || given == null) throw NotYourServerException()
        val expected = proof(key, proof.serverId, address, challenge)
        if (!MessageDigest.isEqual(given.toByteArray(Charsets.UTF_8), expected.toByteArray(Charsets.UTF_8))) {
            throw NotYourServerException()
        }
    }

    private const val CHALLENGE_BYTES = 16
}
