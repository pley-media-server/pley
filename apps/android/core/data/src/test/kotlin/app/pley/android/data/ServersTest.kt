package app.pley.android.data

import app.pley.android.common.SavedServer
import app.pley.android.testing.data.MemoryLegacyPreferences
import app.pley.android.testing.data.TestData
import java.io.File
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

class ServersTest {
    @get:Rule
    val folder = TemporaryFolder()

    /** What the web view app's ServerStore and PlayerActivity leave in their preferences. */
    private val webView = mapOf(
        ("servers" to "saved") to
            """[{"url":"http://192.168.1.20:7539","name":"Living room","id":"a1b2","last_used":1761300000000},""" +
            """{"url":"https://pley.example.com/","name":"Away","id":null,"last_used":1761390000000},""" +
            """{"url":"","name":"broken"},"not an object"]""",
        ("device" to "client_id") to "android-6f1c2e1a-0d7b-4c43-9a0e-3a1d2f6b9c11",
    )

    @Test
    fun `the web view app's servers and install id come over once`() = runTest {
        val legacy = MemoryLegacyPreferences(webView)
        val data = TestData(this, folder.newFolder(), legacy = legacy)
        val servers = data.graph.servers

        val saved = servers.saved.first()
        assertEquals(listOf("https://pley.example.com", "http://192.168.1.20:7539"), saved.map { it.url })
        assertEquals(listOf("Away", "Living room"), saved.map { it.name })
        assertEquals(listOf(null, "a1b2"), saved.map { it.id })
        testScheduler.runCurrent()
        // The most recently used is the one in use.
        assertEquals("https://pley.example.com", servers.active.value)
        // The player keeps its id: the server ends this client's earlier sessions by it.
        assertEquals("android-6f1c2e1a-0d7b-4c43-9a0e-3a1d2f6b9c11", servers.clientId())
    }

    @Test
    fun `the migration runs once`() = runTest {
        val dir = folder.newFolder()
        val legacy = MemoryLegacyPreferences(webView)
        val first = appRun()
        val before = TestData(this, dir, legacy = legacy, scope = first)
        before.graph.servers.forget("https://pley.example.com")
        first.cancel()
        testScheduler.runCurrent()

        // The web view app's list still names it, but it stays forgotten.
        legacy.values["servers" to "saved"] = webView["servers" to "saved"]!!
        val after = TestData(this, dir, legacy = legacy)
        assertEquals(listOf("http://192.168.1.20:7539"), after.graph.servers.saved.first().map { it.url })
    }

    @Test
    fun `a fresh install makes an install id, and keeps it`() = runTest {
        val legacy = MemoryLegacyPreferences()
        val data = TestData(this, folder.newFolder(), legacy = legacy)
        val id = data.graph.servers.clientId()
        assertTrue(id.startsWith("android-"))
        assertEquals(id, data.graph.servers.clientId())
        // The web view app's player uses the same one.
        assertEquals(id, legacy.values["device" to "client_id"])
    }

    @Test
    fun `the install id is the one the players keep in the device's preferences`() = runTest {
        val legacy = MemoryLegacyPreferences()
        val data = TestData(this, folder.newFolder(), legacy = legacy)
        data.graph.servers.clientId()
        // The players read and write it there: what they send is the app's, the event stream's receiver too.
        legacy.values["device" to "client_id"] = "android-player"
        assertEquals("android-player", data.graph.servers.clientId())
    }

    @Test
    fun `using a server makes it the active one, first in the list`() = runTest {
        val dir = folder.newFolder()
        val legacy = MemoryLegacyPreferences(webView)
        val data = TestData(this, dir, legacy = legacy)
        val servers = data.graph.servers

        servers.use(SavedServer("http://192.168.1.20:7539/", "Living room", "a1b2", 0))
        testScheduler.runCurrent()

        assertEquals("http://192.168.1.20:7539", servers.active.value)
        val saved = servers.saved.first()
        assertEquals("http://192.168.1.20:7539", saved.first().url)
        assertEquals(TestData.WALL_CLOCK_START, saved.first().lastUsed)
        assertEquals(2, saved.size)
        // The startup read and the web view app see the same.
        assertEquals("http://192.168.1.20:7539", File(dir, "files/${StartupFiles.SERVER}").readText())
        assertEquals(saved.toSet(), SavedServersJson.decode(legacy.values["servers" to "saved"]).toSet())
    }

    @Test
    fun `a server found at a new address replaces the old one`() = runTest {
        val data = TestData(this, folder.newFolder(), legacy = MemoryLegacyPreferences(webView))
        data.graph.servers.use(SavedServer("http://192.168.1.30:7539", "Living room", "a1b2", 0))
        val saved = data.graph.servers.saved.first()
        assertEquals(listOf("http://192.168.1.30:7539", "https://pley.example.com"), saved.map { it.url })
    }

    @Test
    fun `forgetting a server signs out there and deletes what was saved from it`() = runTest {
        val dir = folder.newFolder()
        val data = signedIn(dir)
        collect(data.graph.hubs.home())
        testScheduler.runCurrent()
        val home = snapshotFile(dir, ProfileScope(data.issuer, 2), SnapshotKeys.HOME)
        assertTrue(home.isFile)

        data.graph.servers.forget(data.issuer)
        testScheduler.runCurrent()

        assertFalse(data.auth.hasTokens(data.issuer))
        assertFalse(home.exists())
        assertNull(data.graph.servers.active.value)
        assertTrue(data.graph.servers.saved.first().isEmpty())
        assertEquals(SessionState.NoServer, data.graph.session.state.value)
        assertFalse(File(dir, "files/${StartupFiles.SERVER}").exists())
    }

    @Test
    fun `saved servers read as the web view app writes them`() {
        val servers = listOf(
            SavedServer("http://nas:7539", "NAS", "id-1", 5),
            SavedServer("https://pley.example.com", "Away", null, 7),
        )
        assertEquals(servers, SavedServersJson.decode(SavedServersJson.encode(servers)))
        assertTrue(SavedServersJson.decode("not json").isEmpty())
        assertTrue(SavedServersJson.decode(null).isEmpty())
    }
}
