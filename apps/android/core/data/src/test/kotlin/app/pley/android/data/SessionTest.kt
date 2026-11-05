package app.pley.android.data

import app.pley.android.auth.SignOutReason
import app.pley.android.model.Hub
import app.pley.android.model.Hubs
import app.pley.android.model.Me
import app.pley.android.testing.Fixtures
import app.pley.android.testing.data.TestData
import app.pley.android.testing.network.FakePleyApi
import java.io.File
import kotlinx.coroutines.cancel
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

class SessionTest {
    @get:Rule
    val folder = TemporaryFolder()

    private val alex: Me = Fixtures.decode("me/app_token.json")
    private val sam = alex.copy(id = 3, username = "sam", name = "Sam", displayName = "Sam")
    private val alexHome: Hubs = Fixtures.decode("hubs/home.json")
    private val samHome = Hubs(listOf(Hub("recently_added:3", "Recently Added in Music", 3)))

    @Test
    fun `no server, signed out, then signed in`() = runTest {
        val data = TestData(this, folder.newFolder())
        val states = collect(data.graph.session.state)
        testScheduler.runCurrent()
        assertEquals(SessionState.NoServer, states.last())

        data.useServer()
        testScheduler.runCurrent()
        assertEquals(SessionState.SignedOut(data.issuer), states.last())
        assertNull(data.graph.session.session.value)
    }

    @Test
    fun `signed in, the session names the profile and what the app may do`() = runTest {
        val data = signedIn(folder.newFolder())
        val session = data.graph.session.session.value!!
        assertEquals(data.issuer, session.issuer)
        assertEquals(alex, session.me)
        assertEquals(setOf("library", "play", "playlists", "profiles"), session.scopes)
    }

    @Test
    fun `the next start knows the profile from this device alone`() = runTest {
        val dir = folder.newFolder()
        val first = appRun()
        signedIn(dir, first)
        first.cancel()
        testScheduler.runCurrent()

        val after = TestData(this, dir)
        after.signIn()
        after.api.hold()
        val states = collect(after.graph.session.state)
        testScheduler.runCurrent()

        assertEquals(alex, (states.last() as SessionState.SignedIn).session.me)
        // Only GET /api/auth/me is waiting, and GET /api/server for the server's language.
        assertEquals(listOf("me", "server"), after.api.calls.map { it.method }.sorted())
        assertEquals("http://192.168.1.20:7539", File(dir, "files/${StartupFiles.SERVER}").readText())
    }

    @Test
    fun `switching profiles shows the other profile's things, never a mix`() = runTest {
        val dir = folder.newFolder()
        val data = signedIn(dir) {
            apiOf[3] = FakePleyApi(issuer).apply { answer("homeHubs", samHome) }
        }
        val seen = collect(data.graph.hubs.home())
        testScheduler.runCurrent()
        assertEquals(Loadable.Ready(alexHome, fresh = true), seen.last())

        data.api.answer("me", sam)
        val before = seen.size
        data.graph.session.switchProfile(3, pin = "1234")
        testScheduler.runCurrent()

        val afterSwitch = seen.drop(before)
        assertTrue(afterSwitch.isNotEmpty())
        assertTrue(afterSwitch.none { it.valueOrNull == alexHome })
        assertEquals(Loadable.Ready(samHome, fresh = true), seen.last())
        assertEquals(sam, data.graph.session.session.value!!.me)

        // Each profile's snapshots in its own folder; the startup copy is the one in use.
        assertTrue(snapshotFile(dir, ProfileScope(data.issuer, 2), SnapshotKeys.HOME).isFile)
        assertTrue(snapshotFile(dir, ProfileScope(data.issuer, 3), SnapshotKeys.HOME).isFile)
        val startup = File(dir, "files/${StartupFiles.HOME}").readText()
        assertTrue(startup.contains("recently_added:3"))
        assertFalse(startup.contains("continue_watching"))

        // Back to Alex: Alex's saved Home shows first, without a request of Sam's.
        data.api.answer("me", alex)
        data.api.hold()
        data.graph.session.switchProfile(2)
        testScheduler.runCurrent()
        assertEquals(Loadable.Ready(alexHome, fresh = false), seen.last())
        data.api.release()
        testScheduler.runCurrent()
        assertEquals(Loadable.Ready(alexHome, fresh = true), seen.last())
    }

    @Test
    fun `a profile switched elsewhere is followed when the server says so`() = runTest {
        val dir = folder.newFolder()
        val data = signedIn(dir) { apiOf[3] = FakePleyApi(issuer).apply { answer("homeHubs", samHome) } }
        val seen = collect(data.graph.hubs.home())
        testScheduler.runCurrent()

        data.api.answer("me", sam)
        data.graph.session.refresh()
        testScheduler.runCurrent()

        assertEquals(Loadable.Ready(samHome, fresh = true), seen.last())
    }

    @Test
    fun `signing out ends the sign-in and deletes the server's snapshots and HTTP caches`() = runTest {
        val dir = folder.newFolder()
        val data = signedIn(dir)
        collect(data.graph.hubs.home())
        testScheduler.runCurrent()
        val cache = File(dir, "cache/http/${ProfileScope.serverKey(data.issuer)}/2/journal")
        cache.parentFile!!.mkdirs()
        cache.writeText("x")
        val home = snapshotFile(dir, ProfileScope(data.issuer, 2), SnapshotKeys.HOME)
        assertTrue(home.isFile)

        data.graph.session.signOut()
        testScheduler.runCurrent()

        assertEquals(SessionState.SignedOut(data.issuer), data.graph.session.state.value)
        assertFalse(data.auth.hasTokens(data.issuer))
        assertFalse(home.exists())
        assertFalse(File(dir, "cache/http/${ProfileScope.serverKey(data.issuer)}").exists())
        assertFalse(File(dir, "files/${StartupFiles.HOME}").exists())
        // The APIs whose caches were there go too: signing in again caches anew.
        assertEquals(listOf(data.issuer), data.forgottenApis.toList())
    }

    @Test
    fun `being signed out by the server deletes what was saved too`() = runTest {
        val dir = folder.newFolder()
        val data = signedIn(dir)
        collect(data.graph.hubs.home())
        testScheduler.runCurrent()
        val home = snapshotFile(dir, ProfileScope(data.issuer, 2), SnapshotKeys.HOME)
        assertTrue(home.isFile)

        data.auth.signOut(data.issuer, SignOutReason.ENDED)
        testScheduler.runCurrent()

        assertFalse(home.exists())
    }
}
