package app.pley.android.data

import android.content.Context
import android.os.StrictMode
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import app.pley.android.common.SavedServer
import app.pley.android.model.Hubs
import app.pley.android.testing.Fixtures
import app.pley.android.testing.data.FakeServerEvents
import app.pley.android.testing.data.FakeTrackDefaultsRoute
import app.pley.android.testing.data.MemoryLegacyPreferences
import app.pley.android.testing.data.TestData
import app.pley.android.testing.auth.FakeAuthGraph
import app.pley.android.testing.network.FakePleyApi
import java.io.File
import java.util.concurrent.atomic.AtomicReference
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith

/**
 * On a device: the data side made and read from the main thread, under a
 * StrictMode that kills the app at any disk access there. Home comes from
 * its snapshot while the server holds every answer.
 */
@RunWith(AndroidJUnit4::class)
class SnapshotsOffMainThreadTest {
    private val context: Context = ApplicationProvider.getApplicationContext()
    private val dir = File(context.filesDir, "snapshot-test")
    private val home: Hubs = Fixtures.decode("hubs/home.json")

    @Before
    fun clean() {
        dir.deleteRecursively()
    }

    @After
    fun cleanUp() {
        dir.deleteRecursively()
    }

    private fun graph(api: FakePleyApi, auth: FakeAuthGraph, scope: CoroutineScope) = AppDataGraph(
        DataEnvironment(
            filesDir = { File(dir, "files") },
            cacheDir = { File(dir, "cache") },
            auth = auth,
            api = { _, _ -> api },
            events = { _, _ -> FakeServerEvents() },
            trackDefaults = FakeTrackDefaultsRoute(),
            profile = TestData.PHONE,
            legacy = MemoryLegacyPreferences(),
            foreground = MutableStateFlow(true),
            debuggable = true,
            scope = scope,
        ),
    )

    private fun signedIn(): FakeAuthGraph = FakeAuthGraph(TestData.offlineHttp()).also {
        it.store.save(
            app.pley.android.auth.StoredTokens(
                TestData.ISSUER,
                "${TestData.ISSUER}/oauth/token",
                "${TestData.ISSUER}/oauth/revoke",
                "access",
                Long.MAX_VALUE,
                "refresh",
                "library play playlists profiles",
            ),
        )
    }

    @Test
    fun homeShowsFromItsSnapshotWithNoDiskAccessOnTheMainThread() {
        // A first run, off the main thread: the server answers and Home is saved.
        val first = CoroutineScope(SupervisorJob() + Dispatchers.Default)
        runBlocking(Dispatchers.Default) {
            val data = graph(FakePleyApi(TestData.ISSUER), signedIn(), first)
            data.servers.use(SavedServer(TestData.ISSUER, "Home", null, 0))
            withTimeout(10_000) { data.hubs.home().first { it is Loadable.Ready && it.fresh } }
            withTimeout(10_000) { while (!File(dir, "files/${StartupFiles.HOME}").isFile) kotlinx.coroutines.delay(20) }
        }
        first.cancel()
        Thread.sleep(200)

        // The next start, on the main thread, the server holding everything.
        val api = FakePleyApi(TestData.ISSUER).apply { hold() }
        val auth = signedIn()
        val shown = AtomicReference<Loadable<Hubs>>()
        val failure = AtomicReference<Throwable>()
        val second = CoroutineScope(SupervisorJob() + Dispatchers.Default)
        InstrumentationRegistry.getInstrumentation().runOnMainSync {
            val policy = StrictMode.getThreadPolicy()
            StrictMode.setThreadPolicy(
                StrictMode.ThreadPolicy.Builder().detectDiskReads().detectDiskWrites().penaltyDeath().build(),
            )
            try {
                val data = graph(api, auth, second)
                shown.set(runBlocking { withTimeout(10_000) { data.hubs.home().first { it is Loadable.Ready } } })
            } catch (e: Throwable) {
                failure.set(e)
            } finally {
                StrictMode.setThreadPolicy(policy)
            }
        }
        api.release()
        second.cancel()

        failure.get()?.let { throw it }
        assertEquals(Loadable.Ready(home, fresh = false), shown.get())
    }
}
