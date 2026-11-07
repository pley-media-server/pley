package app.pley.android.screens.admin.folders

import app.pley.android.model.PleyJson
import app.pley.android.network.ApiException
import app.pley.android.screens.admin.AdminLoad
import app.pley.android.testing.MainDispatcherRule
import app.pley.android.testing.data.FakeServerAdmin
import kotlinx.coroutines.test.runTest
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test

/** Picking a folder on the server: the drives, a folder, up, typing one, and one that can't be read. */
class FolderBrowserTest {
    @get:Rule val main = MainDispatcherRule()

    private fun server() = FakeServerAdmin().apply {
        answer("POST /api/fs") { body ->
            val path = (body as JsonObject)["path"]?.jsonPrimitive?.content
            when (path) {
                null -> PleyJson.parseToJsonElement("""{"path":null,"parent":null,"entries":[{"name":"/mnt","path":"/mnt"}]}""")
                "/mnt" -> PleyJson.parseToJsonElement("""{"path":"/mnt","parent":"/","entries":[{"name":"media","path":"/mnt/media"}]}""")
                "/mnt/media" -> PleyJson.parseToJsonElement("""{"path":"/mnt/media","parent":"/mnt","entries":[]}""")
                else -> throw ApiException(400, "bad_request", "cannot open $path: not found")
            }
        }
    }

    @Test
    fun itOpensAtTheDrivesAndGoesInAndUp() = runTest {
        val server = server()
        val browser = FolderBrowser(this, server::api)
        assertNull(browser.view.value)
        browser.open(null)
        testScheduler.advanceUntilIdle()
        val roots = browser.view.value!!
        assertTrue(roots.atRoots)
        assertNull(roots.chosen)
        assertEquals("{}", server.calls.single().body.toString())

        browser.go("/mnt")
        testScheduler.advanceUntilIdle()
        browser.go(" /mnt/media ")
        testScheduler.advanceUntilIdle()
        assertEquals("""{"path":"/mnt/media"}""", server.calls.last().body.toString())
        assertEquals("/mnt/media", browser.view.value?.chosen)

        browser.up()
        testScheduler.advanceUntilIdle()
        assertEquals("/mnt", browser.view.value?.chosen)
        browser.close()
        assertNull(browser.view.value)
    }

    @Test
    fun aFolderThatCantBeReadSaysWhyAndUpGoesToTheDrives() = runTest {
        val server = server()
        val browser = FolderBrowser(this, server::api)
        browser.open("/gone")
        testScheduler.advanceUntilIdle()
        val failed = browser.view.value!!
        assertTrue(failed.load is AdminLoad.Failed)
        assertNull(failed.chosen)
        browser.up()
        testScheduler.advanceUntilIdle()
        assertTrue(browser.view.value!!.atRoots)
        // A blank path is the drives too.
        browser.go("  ")
        testScheduler.advanceUntilIdle()
        assertTrue(browser.view.value!!.atRoots)
    }
}
