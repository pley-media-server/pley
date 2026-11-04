package app.pley.android.network

import app.pley.android.auth.Shelf
import app.pley.android.auth.StoredTokens
import app.pley.android.auth.TokenStore
import app.pley.android.auth.Tokens
import app.pley.android.auth.Vault
import app.pley.android.common.DeviceProfile
import app.pley.android.common.FormFactor
import app.pley.android.common.Tier
import app.pley.android.model.DeviceKind
import java.io.File
import javax.crypto.KeyGenerator
import okhttp3.OkHttpClient

/** The user agent the tests' clients send. */
const val TEST_AGENT = "pleyAndroid/0.1.0 (Android 17)"

/** A phone of tier [tier]. */
fun testProfile(tier: Tier = Tier.T2, form: FormFactor = FormFactor.Phone) = DeviceProfile(
    form = form,
    fireTv = false,
    tier = tier,
    model = "Pixel 7a",
    totalRamBytes = 8L shl 30,
    lowRam = false,
)

/** Strings in a map, as the preferences file keeps them on a device. */
class MemoryShelf : Shelf {
    private val values = mutableMapOf<String, String>()

    override fun get(name: String): String? = synchronized(values) { values[name] }

    override fun put(name: String, value: String) {
        synchronized(values) { values[name] = value }
    }

    override fun remove(name: String) {
        synchronized(values) { values.remove(name) }
    }
}

/**
 * The tokens of [issuer], signed in with access token [access] (good until
 * [expiresAt]) and refresh token [refresh]; refreshes go to
 * `<issuer>/api/oauth/token`.
 */
fun testTokens(
    issuer: String,
    access: String? = "pley_at_one",
    refresh: String? = "pley_rt_one",
    expiresAt: Long = Long.MAX_VALUE / 2,
): Tokens {
    val key = KeyGenerator.getInstance("AES").apply { init(256) }.generateKey()
    val store = TokenStore(MemoryShelf(), Vault { key })
    if (access != null) {
        store.save(
            StoredTokens(
                issuer = issuer,
                tokenEndpoint = "$issuer/api/oauth/token",
                revocationEndpoint = "$issuer/api/oauth/revoke",
                accessToken = access,
                accessExpiresAt = expiresAt,
                refreshToken = refresh,
                scope = "library play playlists profiles",
            ),
        )
    }
    return Tokens(issuer, store, OkHttpClient())
}

/** No home network: discovery and sweeps find nothing. */
object NoLans : LanNetworks {
    override fun current(): List<Lan> = emptyList()

    override fun bypass(): Lan? = null
}

/** The graph the app makes, on [root], with [tokens] per origin and no Android. */
internal fun testGraph(
    cacheDir: File,
    tokens: (String) -> Tokens,
    profile: DeviceProfile = testProfile(),
    root: OkHttpClient = HttpRoot.create(TEST_AGENT),
    lans: LanNetworks = NoLans,
    deviceName: () -> String? = { null },
    deviceKind: () -> DeviceKind? = { null },
) = HttpNetworkGraph(
    root = root,
    tokens = tokens,
    profile = profile,
    cacheDir = cacheDir,
    lans = lans,
    route = null,
    localNetworkOf = { PermissionsLocalNetworkAccess(sdk = 37, fireTv = false, granted = { true }) },
    deviceName = deviceName,
    deviceKind = deviceKind,
)
