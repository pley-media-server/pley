package app.pley.android.auth

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import androidx.core.content.edit
import java.security.GeneralSecurityException
import java.security.KeyStore
import java.security.ProviderException
import java.util.Base64
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec
import org.json.JSONException
import org.json.JSONObject

/** Strings kept by name, in app-private storage (preferences on the device, a map in tests). */
interface Shelf {
    fun get(name: String): String?

    /** Written before it returns: a new refresh token must outlive the process. */
    fun put(name: String, value: String)

    fun remove(name: String)
}

/** A preferences file of the app's own, which no backup or transfer takes (backup_rules, data_extraction_rules). */
class PrefsShelf(context: Context, file: String = "sign_in") : Shelf {
    private val prefs = context.applicationContext.getSharedPreferences(file, Context.MODE_PRIVATE)

    override fun get(name: String): String? = prefs.getString(name, null)

    override fun put(name: String, value: String) {
        prefs.edit(commit = true) { putString(name, value) }
    }

    override fun remove(name: String) {
        prefs.edit(commit = true) { remove(name) }
    }
}

/**
 * AES-GCM with a 256-bit key that [key] gives: the Android Keystore's on the
 * device, so the key never leaves it. Each value is bound to [aad] (the
 * server it belongs to), so one server's tokens can't be passed off as
 * another's.
 */
open class Vault(private val key: () -> SecretKey) {
    /**
     * [plain] sealed with [aad]. When the Keystore fails (the provider throws
     * ProviderException, a RuntimeException, besides the usual ones), an
     * [AuthException]: an IOException, so a request fails rather than the app.
     */
    fun seal(plain: ByteArray, aad: String): ByteArray = try {
        val cipher = Cipher.getInstance(TRANSFORMATION)
        // The provider picks a fresh 12-byte IV each time.
        cipher.init(Cipher.ENCRYPT_MODE, key())
        cipher.updateAAD(aad.toByteArray(Charsets.UTF_8))
        val iv = cipher.iv
        require(iv.size == IV_BYTES) { "unexpected IV size" }
        iv + cipher.doFinal(plain)
    } catch (_: GeneralSecurityException) {
        throw AuthException(UNKEPT)
    } catch (_: ProviderException) {
        throw AuthException(UNKEPT)
    }

    /** The value [seal] sealed with [aad]; `null` when it wasn't, the key changed, or the Keystore fails. */
    fun open(sealed: ByteArray, aad: String): ByteArray? {
        if (sealed.size <= IV_BYTES) return null
        return try {
            val cipher = Cipher.getInstance(TRANSFORMATION)
            cipher.init(Cipher.DECRYPT_MODE, key(), GCMParameterSpec(TAG_BITS, sealed, 0, IV_BYTES))
            cipher.updateAAD(aad.toByteArray(Charsets.UTF_8))
            cipher.doFinal(sealed, IV_BYTES, sealed.size - IV_BYTES)
        } catch (_: GeneralSecurityException) {
            null
        } catch (_: ProviderException) {
            null
        }
    }

    companion object {
        /** Why tokens couldn't be sealed: they aren't used, and the person signs in again. */
        const val UNKEPT = "the sign-in couldn't be kept on this device: sign in again"
        private const val TRANSFORMATION = "AES/GCM/NoPadding"
        private const val IV_BYTES = 12
        private const val TAG_BITS = 128
    }
}

/** The vault whose key lives in the Android Keystore, made on first use. */
class KeystoreVault(alias: String = "pley_sign_in") : Vault({ keystoreKey(alias) })

private fun keystoreKey(alias: String): SecretKey {
    val store = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
    (store.getEntry(alias, null) as? KeyStore.SecretKeyEntry)?.let { return it.secretKey }
    val generator = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore")
    generator.init(
        KeyGenParameterSpec.Builder(alias, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
            .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
            .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
            .setKeySize(256)
            .setRandomizedEncryptionRequired(true)
            .build(),
    )
    return generator.generateKey()
}

/** What the app holds for one server: its tokens, and where to refresh and end them. */
data class StoredTokens(
    val issuer: String,
    val tokenEndpoint: String,
    val revocationEndpoint: String,
    val accessToken: String,
    /** When the access token stops working, in milliseconds since the epoch. */
    val accessExpiresAt: Long,
    val refreshToken: String?,
    /** What the server allowed, as it wrote it ("library play …"). */
    val scope: String,
    /** The server's id and this sign-in's proof key; absent from servers before them. */
    val proof: ServerProof? = null,
) {
    /** The access token is used until a minute before it ends. */
    fun accessUsable(now: Long): Boolean = now < accessExpiresAt - REUSE_MARGIN_MS

    fun scopes(): Set<String> = scope.split(' ').filter { it.isNotEmpty() }.toSet()

    fun toJson(): JSONObject = JSONObject()
        .put("issuer", issuer)
        .put("token_endpoint", tokenEndpoint)
        .put("revocation_endpoint", revocationEndpoint)
        .put("access_token", accessToken)
        .put("access_expires_at", accessExpiresAt)
        .put("refresh_token", refreshToken ?: JSONObject.NULL)
        .put("scope", scope)
        .apply {
            if (proof != null) put("server_id", proof.serverId).put("proof_id", proof.proofId).put("proof_key", proof.proofKey)
        }

    // Never print the tokens.
    override fun toString(): String = "StoredTokens(issuer=$issuer, scope=$scope)"

    companion object {
        const val REUSE_MARGIN_MS = 60_000L

        fun fromJson(o: JSONObject): StoredTokens = StoredTokens(
            issuer = o.getString("issuer"),
            tokenEndpoint = o.getString("token_endpoint"),
            revocationEndpoint = o.getString("revocation_endpoint"),
            accessToken = o.getString("access_token"),
            accessExpiresAt = o.getLong("access_expires_at"),
            refreshToken = if (o.isNull("refresh_token")) null else o.getString("refresh_token"),
            scope = o.optString("scope"),
            proof = ServerProof.of(o),
        )
    }
}

/**
 * The tokens of each server, sealed by the [vault] and kept on the [shelf]
 * under the server's address. Nothing here reads or keeps the web view's
 * cookie: the app holds app tokens only. [unreadable] hears of a server
 * whose tokens were kept but can't be opened anymore.
 */
class TokenStore(
    private val shelf: Shelf,
    private val vault: Vault,
    private val unreadable: (issuer: String) -> Unit = {},
) {
    /** What was read or written already, so requests don't each open the vault. */
    private val known = mutableMapOf<String, StoredTokens?>()

    /**
     * Whether tokens are kept for [issuer], without opening them (no
     * Keystore): for choosing the first screen. They may still turn out
     * unreadable.
     */
    @Synchronized
    fun has(issuer: String): Boolean = if (issuer in known) known[issuer] != null else shelf.get(TOKENS + issuer) != null

    @Synchronized
    fun load(issuer: String): StoredTokens? {
        if (issuer in known) return known[issuer]
        if (shelf.get(TOKENS + issuer) == null) return null.also { known[issuer] = null }
        val tokens = openJson(TOKENS + issuer, issuer)?.let {
            try {
                StoredTokens.fromJson(it)
            } catch (_: JSONException) {
                null
            }
        }
        if (tokens == null || tokens.issuer != issuer) {
            // Unreadable (the Keystore key went with a reset): signed out.
            shelf.remove(TOKENS + issuer)
            unreadable(issuer)
        }
        known[issuer] = tokens
        return tokens
    }

    @Synchronized
    fun save(tokens: StoredTokens) {
        // Written first: when that fails, the tokens aren't used.
        known.remove(tokens.issuer)
        sealJson(TOKENS + tokens.issuer, tokens.issuer, tokens.toJson())
        known[tokens.issuer] = tokens
    }

    @Synchronized
    fun wipe(issuer: String) {
        shelf.remove(TOKENS + issuer)
        known[issuer] = null
    }

    /** The sign-in waiting for the browser for [server], kept across process death. */
    @Synchronized
    fun pendingFlow(server: String): PendingFlow? = openJson(FLOW + server, server)?.let {
        try {
            PendingFlow.fromJson(it).takeIf { flow -> flow.server == server }
        } catch (_: JSONException) {
            null
        }
    }

    /** Keeps [flow], replacing the one before for its server: one sign-in per server at a time. */
    @Synchronized
    fun keepFlow(flow: PendingFlow) {
        val servers = flowServers() + flow.server
        sealJson(FLOW + flow.server, flow.server, flow.toJson())
        shelf.put(FLOW_SERVERS, servers.joinToString("\n"))
    }

    /**
     * Takes (and forgets) the waiting sign-in whose state is [state], so a
     * second answer for it finds nothing. Sign-ins older than [maxAgeMs] are
     * forgotten too.
     */
    @Synchronized
    fun takeFlow(state: String, now: Long, maxAgeMs: Long): PendingFlow? {
        var taken: PendingFlow? = null
        val left = mutableSetOf<String>()
        for (server in flowServers()) {
            val flow = pendingFlow(server)
            when {
                flow == null || now - flow.createdAt > maxAgeMs || now < flow.createdAt -> shelf.remove(FLOW + server)
                taken == null && constantTimeEquals(flow.state, state) -> {
                    shelf.remove(FLOW + server)
                    taken = flow
                }
                else -> left += server
            }
        }
        shelf.put(FLOW_SERVERS, left.joinToString("\n"))
        return taken
    }

    private fun flowServers(): Set<String> =
        shelf.get(FLOW_SERVERS)?.split('\n')?.filter { it.isNotEmpty() }?.toSet().orEmpty()

    private fun sealJson(name: String, aad: String, o: JSONObject) {
        val sealed = vault.seal(o.toString().toByteArray(Charsets.UTF_8), aad)
        shelf.put(name, Base64.getEncoder().encodeToString(sealed))
    }

    private fun openJson(name: String, aad: String): JSONObject? {
        val text = shelf.get(name) ?: return null
        val sealed = try {
            Base64.getDecoder().decode(text)
        } catch (_: IllegalArgumentException) {
            return null
        }
        val plain = vault.open(sealed, aad) ?: return null
        return try {
            JSONObject(String(plain, Charsets.UTF_8))
        } catch (_: JSONException) {
            null
        }
    }

    companion object {
        private const val TOKENS = "tokens:"
        private const val FLOW = "flow:"
        private const val FLOW_SERVERS = "flows"
    }
}

/** Compares secrets without telling, by its timing, how much matched. */
fun constantTimeEquals(a: String, b: String): Boolean =
    java.security.MessageDigest.isEqual(a.toByteArray(Charsets.UTF_8), b.toByteArray(Charsets.UTF_8))
