package app.pley.android.data

import android.content.Context
import androidx.datastore.core.DataMigration
import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.booleanPreferencesKey
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.emptyPreferences
import androidx.datastore.preferences.core.longPreferencesKey
import androidx.datastore.preferences.core.stringPreferencesKey
import app.pley.android.auth.AuthGraph
import app.pley.android.common.SavedServer
import app.pley.android.common.Servers
import java.io.File
import java.io.IOException
import java.util.UUID
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.catch
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.withContext
import kotlinx.serialization.SerializationException
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonArray
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.longOrNull
import kotlinx.serialization.json.put

/**
 * The web view app's preferences: its saved servers (`servers`/`saved`,
 * ServerStore) and its install id (`device`/`client_id`, PlayerActivity).
 * Read once into the native app's store, and kept in step while the web
 * view app is still shipped, so both see the same servers and the server
 * sees one player. Only touched off the main thread.
 */
interface LegacyPreferences {
    fun get(file: String, key: String): String?

    fun put(file: String, key: String, value: String)

    companion object {
        const val SERVERS_FILE = "servers"
        const val SERVERS_KEY = "saved"
        const val DEVICE_FILE = "device"
        const val CLIENT_ID_KEY = "client_id"

        /** The app's own SharedPreferences. */
        fun of(context: Context): LegacyPreferences {
            val app = context.applicationContext
            return object : LegacyPreferences {
                override fun get(file: String, key: String): String? =
                    app.getSharedPreferences(file, Context.MODE_PRIVATE).getString(key, null)

                override fun put(file: String, key: String, value: String) {
                    app.getSharedPreferences(file, Context.MODE_PRIVATE).edit().putString(key, value).apply()
                }
            }
        }
    }
}

/** Saved servers as JSON, as the web view app's ServerStore writes them: `[{url, name, id, last_used}]`. */
object SavedServersJson {
    fun encode(servers: List<SavedServer>): String = buildJsonArray {
        for (s in servers) {
            add(
                buildJsonObject {
                    put("url", s.url)
                    put("name", s.name)
                    put("id", s.id)
                    put("last_used", s.lastUsed)
                },
            )
        }
    }.toString()

    /** What [text] holds; nothing when it can't be read. */
    fun decode(text: String?): List<SavedServer> {
        if (text.isNullOrEmpty()) return emptyList()
        val array = try {
            Json.parseToJsonElement(text) as? JsonArray
        } catch (_: SerializationException) {
            null
        } catch (_: IllegalArgumentException) {
            null
        } ?: return emptyList()
        return array.mapNotNull { element ->
            val o = element as? JsonObject ?: return@mapNotNull null
            val url = (o["url"] as? JsonPrimitive)?.contentOrNull?.takeIf { it.isNotEmpty() } ?: return@mapNotNull null
            val id = o["id"]?.takeIf { it !is JsonNull }?.let { (it as? JsonPrimitive)?.contentOrNull }
            SavedServer(
                url = url,
                name = (o["name"] as? JsonPrimitive)?.contentOrNull ?: url,
                id = id,
                lastUsed = (o["last_used"] as? JsonPrimitive)?.longOrNull ?: 0,
            )
        }
    }
}

/** The DataStore keys of the servers. */
internal object ServerKeys {
    val SAVED = stringPreferencesKey("saved")
    val ACTIVE = stringPreferencesKey("active")
    val CLIENT_ID = stringPreferencesKey("client_id")
    val MIGRATED = booleanPreferencesKey("migrated_from_web_view")

    /** The profile last used on a server (an origin). */
    fun profile(origin: String) = longPreferencesKey("profile:$origin")
}

/**
 * Brings the web view app's servers and install id in, once: servers
 * already here win, the most recently used becomes the active one when
 * none is, and its install id stays the player's `client_id`.
 */
internal class WebViewMigration(private val legacy: LegacyPreferences) : DataMigration<Preferences> {
    override suspend fun shouldMigrate(currentData: Preferences): Boolean = currentData[ServerKeys.MIGRATED] != true

    override suspend fun migrate(currentData: Preferences): Preferences {
        val p = currentData.toMutablePreferences()
        var list = SavedServersJson.decode(p[ServerKeys.SAVED])
        val old = SavedServersJson.decode(legacy.get(LegacyPreferences.SERVERS_FILE, LegacyPreferences.SERVERS_KEY))
        for (server in old) {
            val origin = ProfileScope.originOf(server.url)
            if (list.none { it.url == origin || (server.id != null && it.id == server.id) }) list = list + server.copy(url = origin)
        }
        p[ServerKeys.SAVED] = SavedServersJson.encode(list)
        if (p[ServerKeys.ACTIVE] == null) Servers.lastUsed(list)?.let { p[ServerKeys.ACTIVE] = it.url }
        val clientId = legacy.get(LegacyPreferences.DEVICE_FILE, LegacyPreferences.CLIENT_ID_KEY)?.takeIf { it.isNotBlank() }
        if (clientId != null && p[ServerKeys.CLIENT_ID] == null) p[ServerKeys.CLIENT_ID] = clientId
        p[ServerKeys.MIGRATED] = true
        return p.toPreferences()
    }

    override suspend fun cleanUp() {
        // The web view app still reads its own preferences until it goes.
    }
}

/**
 * The servers in a DataStore: saved ones, the active one, the profile
 * last used on each, and the install id.
 *
 * [forgetData] deletes what was saved from a server (snapshots and HTTP
 * caches). [startupServer] is where the active origin is written for the
 * app's startup read, beside Home's copy.
 */
internal class DataStoreServers(
    private val store: DataStore<Preferences>,
    private val legacy: LegacyPreferences,
    private val auth: AuthGraph,
    private val forgetData: suspend (origin: String) -> Unit,
    app: CoroutineScope,
    private val io: CoroutineDispatcher,
    private val wallClock: () -> Long,
    private val startupServer: (() -> File)?,
) : ServersRepository {
    /** Everything stored; `null` until read. */
    val prefs: StateFlow<Preferences?> = store.data
        .catch { e -> if (e is IOException) emit(emptyPreferences()) else throw e }
        .stateIn(app, SharingStarted.Eagerly, null)

    override val saved: Flow<List<SavedServer>> = prefs.filterNotNull()
        .map { p -> SavedServersJson.decode(p[ServerKeys.SAVED]).sortedByDescending { it.lastUsed } }
        .distinctUntilChanged()

    override val active: StateFlow<String?> = prefs.map { it?.get(ServerKeys.ACTIVE) }
        .stateIn(app, SharingStarted.Eagerly, null)

    /** The active server once read: `null` until then, then the origin or "" for none. */
    val activeRead: Flow<String?> = prefs.map { p -> if (p == null) null else p[ServerKeys.ACTIVE].orEmpty() }.distinctUntilChanged()

    override suspend fun use(server: SavedServer) {
        val origin = ProfileScope.originOf(server.url)
        val now = wallClock()
        store.edit { p ->
            val list = SavedServersJson.decode(p[ServerKeys.SAVED])
            p[ServerKeys.SAVED] = SavedServersJson.encode(Servers.withServer(list, server.copy(url = origin, lastUsed = now)))
            p[ServerKeys.ACTIVE] = origin
        }
        mirror()
    }

    override suspend fun forget(origin: String) {
        val o = ProfileScope.originOf(origin)
        // Ends the sign-in on the server (revocation) before forgetting the tokens.
        if (auth.hasTokens(o)) auth.tokens(o).signOut()
        forgetData(o)
        store.edit { p ->
            val list = SavedServersJson.decode(p[ServerKeys.SAVED])
            p[ServerKeys.SAVED] = SavedServersJson.encode(list.filterNot { it.url == o })
            if (p[ServerKeys.ACTIVE] == o) p.remove(ServerKeys.ACTIVE)
            p.remove(ServerKeys.profile(o))
        }
        mirror()
    }

    override suspend fun clientId(): String {
        // The players' own (the device's preferences, which they read and write): the one they send.
        withContext(io) { legacy.get(LegacyPreferences.DEVICE_FILE, LegacyPreferences.CLIENT_ID_KEY) }?.takeIf { it.isNotBlank() }?.let { return it }
        prefs.filterNotNull().first()[ServerKeys.CLIENT_ID]?.let { return it }
        val made = "android-${UUID.randomUUID()}"
        val kept = store.edit { p -> if (p[ServerKeys.CLIENT_ID] == null) p[ServerKeys.CLIENT_ID] = made }[ServerKeys.CLIENT_ID] ?: made
        withContext(io) { legacy.put(LegacyPreferences.DEVICE_FILE, LegacyPreferences.CLIENT_ID_KEY, kept) }
        return kept
    }

    /** The profile last used on [origin], `null` when none is known. */
    suspend fun profileOf(origin: String): Long? = prefs.filterNotNull().first()[ServerKeys.profile(ProfileScope.originOf(origin))]

    suspend fun setProfile(origin: String, userId: Long?) {
        val key = ServerKeys.profile(ProfileScope.originOf(origin))
        if (prefs.value?.get(key) == userId) return
        store.edit { p -> if (userId == null) p.remove(key) else p[key] = userId }
    }

    /** The web view app's list, and the startup read's server, kept in step. */
    private suspend fun mirror() {
        val p = store.data.first()
        val list = SavedServersJson.decode(p[ServerKeys.SAVED])
        val active = p[ServerKeys.ACTIVE]
        withContext(io) {
            legacy.put(LegacyPreferences.SERVERS_FILE, LegacyPreferences.SERVERS_KEY, SavedServersJson.encode(list))
            val file = startupServer?.invoke() ?: return@withContext
            try {
                if (active == null) file.delete() else FileSnapshotStore.writeAtomically(file, active.toByteArray())
            } catch (_: IOException) {
                file.delete()
            }
        }
    }

    companion object {
        /** The DataStore's file, in filesDir. */
        const val FILE = "datastore/servers.preferences_pb"
    }
}
