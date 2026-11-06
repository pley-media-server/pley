package app.pley.android.data

import app.pley.android.common.Language
import app.pley.android.model.Me
import app.pley.android.model.ServerInfo
import app.pley.android.network.ApiException
import app.pley.android.testing.Fixtures
import app.pley.android.testing.data.TestData
import app.pley.android.testing.network.FakePleyApi
import java.util.Locale
import kotlinx.coroutines.async
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

/**
 * The language shown goes by the account's, else the device's, else the
 * server language; the app learns the account's and the server's while
 * signed in, and never goes by another before the server has said.
 */
class LanguageTest {
    @get:Rule
    val folder = TemporaryFolder()

    private val french = listOf(Locale.CANADA_FRENCH)
    private val german = listOf(Locale.GERMANY)

    /** As the data side says it: a server that answered without a language is from before languages. */
    private fun signedIn(
        account: String?,
        accountKnown: Boolean = true,
        server: String? = null,
        serverKnown: Boolean = true,
        keepsLanguage: Boolean = !serverKnown || server != null,
    ) = LanguageSources.SignedIn(account, accountKnown, server, serverKnown, keepsLanguage)

    @Test
    fun theAccountsThenTheDevicesThenTheServersThenEnglish() {
        assertEquals(Language.Spanish, signedIn("es", server = "fr").wanted(french))
        assertEquals(Language.French, signedIn(null, server = "es").wanted(french))
        assertEquals(Language.French, signedIn(null, server = "fr").wanted(german))
        assertEquals(Language.French, signedIn(null, server = null).wanted(french))
        assertEquals(Language.English, signedIn(null, server = null).wanted(german))
        // A code Pley doesn't have counts as none.
        assertEquals(Language.French, signedIn("de", server = "fr").wanted(german))
        assertEquals(Language.French, signedIn("pt", server = "xx").wanted(french))
    }

    @Test
    fun nothingChangesWhileTheServerHasntSaid() {
        // The account's own choice is enough.
        assertEquals(Language.Spanish, signedIn("es", accountKnown = false, server = null, serverKnown = false).wanted(french))
        // Without it, the device's waits for the account; a device in a language Pley doesn't have, for the server too.
        assertNull(signedIn(null, accountKnown = false, server = "fr").wanted(french))
        assertEquals(Language.French, signedIn(null, server = null, serverKnown = false).wanted(french))
        assertNull(signedIn(null, server = null, serverKnown = false).wanted(german))
        assertEquals(Language.French, signedIn(null, server = "fr", serverKnown = true).wanted(german))
        assertNull(LanguageSources.Unknown.wanted(french))
        // No account: the device's.
        assertEquals(Language.French, LanguageSources.NoAccount.wanted(french))
        assertEquals(Language.English, LanguageSources.NoAccount.wanted(german))
    }

    @Test
    fun theLanguagePickedOnTheDeviceIsTheDevicesAfterTheAccounts() {
        val spanish = Language.Spanish
        // Before the system's languages, and before the server's.
        assertEquals(spanish, LanguageSources.NoAccount.wanted(french, picked = spanish))
        assertEquals(spanish, signedIn(null, server = "fr").wanted(french, picked = spanish))
        assertEquals(spanish, signedIn(null, server = "fr").wanted(german, picked = spanish))
        // The account's own comes first.
        assertEquals(Language.French, signedIn("fr", server = "en").wanted(german, picked = spanish))
        // It doesn't go before the account's answer, nor while nothing is known.
        assertNull(signedIn(null, accountKnown = false, server = null, serverKnown = false).wanted(german, picked = spanish))
        assertNull(LanguageSources.Unknown.wanted(french, picked = spanish))
        // Without it, the server's answer isn't waited for.
        assertEquals(spanish, signedIn(null, server = null, serverKnown = false).wanted(german, picked = spanish))
    }

    @Test
    fun aServerThatKeepsNoAccountsLanguageLeavesNothingToWaitFor() {
        val spanish = Language.Spanish
        // A Pley from before languages (its GET /api/server named none): the account's answer isn't waited for.
        val older = signedIn(null, accountKnown = false, server = null)
        assertEquals(false, older.keepsLanguage)
        assertEquals(spanish, older.wanted(german, picked = spanish))
        assertEquals(Language.French, older.wanted(french))
        assertEquals(Language.English, older.wanted(german))
        // What an account saved means nothing there: this device's.
        assertEquals(spanish, signedIn("fr", accountKnown = false, server = null).wanted(german, picked = spanish))
        assertEquals(Language.English, signedIn("fr", server = null).wanted(german))
        // One without the route that names a language: for a device in a language Pley doesn't have, the server's, once it said it.
        assertEquals(spanish, signedIn(null, accountKnown = false, server = "fr", keepsLanguage = false).wanted(german, picked = spanish))
        assertEquals(Language.French, signedIn("es", accountKnown = false, server = "fr", keepsLanguage = false).wanted(german))
        assertNull(signedIn(null, accountKnown = false, server = null, serverKnown = false, keepsLanguage = false).wanted(german))
        // A current server's account is still waited for.
        assertNull(signedIn(null, accountKnown = false, server = "fr").wanted(german, picked = spanish))
    }

    @Test
    fun withoutAnAccountTheDevicesLanguageShows() = runTest {
        val data = TestData(this, folder.newFolder())
        val sources = collect(data.graph.language.sources)
        testScheduler.runCurrent()
        assertEquals(LanguageSources.NoAccount, sources.last())
        // A server chosen, not signed in to: still none.
        data.useServer()
        testScheduler.runCurrent()
        assertEquals(LanguageSources.NoAccount, sources.last())
    }

    @Test
    fun signedInTheAccountsAndTheServersLanguagesAreLearnt() = runTest {
        val data = signedIn(folder.newFolder()) {
            api.answer("server", ServerInfo("Home", "1.0", language = "fr"))
        }
        val sources = collect(data.graph.language.sources)
        testScheduler.runCurrent()
        assertEquals(signedIn(null, server = "fr"), sources.last())
        assertEquals(Language.French, sources.last().wanted(german))
    }

    @Test
    fun whatWasSavedOfTheAccountWaitsForTheServersAnswer() = runTest {
        val dir = folder.newFolder()
        val first = appRun()
        signedIn(dir, first) { api.answer("me", alex.copy(language = "es")) }
        first.cancel()
        testScheduler.runCurrent()

        val after = TestData(this, dir)
        after.signIn()
        after.api.answer("server", ServerInfo("Home", "1.0", language = "en"))
        after.api.hold()
        val sources = collect(after.graph.language.sources)
        testScheduler.runCurrent()
        assertEquals(signedIn("es", accountKnown = false, server = null, serverKnown = false), sources.last())
        after.api.release()
        testScheduler.runCurrent()
        assertEquals(signedIn(null, server = "en"), sources.last())
    }

    @Test
    fun aChoiceShowsAtOnceAndGoesToTheServer() = runTest {
        val data = signedIn(folder.newFolder())
        val sources = collect(data.graph.language.sources)
        testScheduler.runCurrent()
        data.api.hold()
        val choosing = backgroundScope.async { data.graph.language.choose("es") }
        testScheduler.runCurrent()
        assertEquals("es", (sources.last() as LanguageSources.SignedIn).account)
        data.api.release()
        choosing.await()
        assertEquals(listOf(FakePleyApi.Call("setLanguage", listOf("es"))), data.api.calls("setLanguage"))
        assertEquals("es", data.graph.session.session.value?.me?.language)
    }

    @Test
    fun aRefusedChoiceIsPutBack() = runTest {
        val data = signedIn(folder.newFolder()) {
            api.answer("server", ServerInfo("Home", "1.0", language = "en"))
        }
        val sources = collect(data.graph.language.sources)
        testScheduler.runCurrent()
        data.api.fail("setLanguage", ApiException(400, "bad_request", "language is one of en, fr, es, or null"))
        val refused = runCatching { data.graph.language.choose("fr") }.exceptionOrNull()
        assertTrue(refused is ApiException)
        assertEquals(signedIn(null, server = "en"), sources.last())
    }

    @Test
    fun aServerFromBeforeLanguagesKeepsNoAccountsLanguage() = runTest {
        // Its GET /api/server names none (the fixture is such an answer): the Language row is this device's.
        val data = signedIn(folder.newFolder())
        val sources = collect(data.graph.language.sources)
        testScheduler.runCurrent()
        val now = sources.last() as LanguageSources.SignedIn
        assertTrue(now.serverKnown)
        assertEquals(false, now.keepsLanguage)
        // What shows is decided as on any server: the device's, its account having none.
        assertEquals(Language.French, now.wanted(french))
    }

    @Test
    fun onAServerFromBeforeLanguagesThePickShowsBeforeTheAccountsAnswer() = runTest {
        val dir = folder.newFolder()
        val first = appRun()
        signedIn(dir, first) { api.answer("me", alex.copy(language = "es")) }
        first.cancel()
        testScheduler.runCurrent()

        // GET /api/server names no language (the fixture is such an answer); GET /api/auth/me doesn't answer.
        val after = TestData(this, dir)
        after.signIn()
        after.api.fail("me", ApiException(503, null, "unavailable"))
        val sources = collect(after.graph.language.sources)
        testScheduler.runCurrent()
        val now = sources.last()
        assertEquals(signedIn("es", accountKnown = false, server = null), now)
        // The language picked on this device shows, and what the account saved means nothing there.
        assertEquals(Language.French, now.wanted(german, picked = Language.French))
        assertEquals(Language.English, now.wanted(german))
    }

    @Test
    fun aServerWithoutTheRouteIsRememberedWhileItIsInUse() = runTest {
        // It names a language, but answers a choice 404 (the route doesn't reach it).
        val data = signedIn(folder.newFolder()) {
            api.answer("server", ServerInfo("Home", "1.0", language = "en"))
        }
        val sources = collect(data.graph.language.sources)
        testScheduler.runCurrent()
        assertEquals(signedIn(null, server = "en"), sources.last())
        data.api.fail("setLanguage", ApiException(404, "not_found", "not found"))
        val refused = runCatching { data.graph.language.choose("fr") }.exceptionOrNull()
        assertTrue(refused is ApiException)
        // The account's is put back, and the server keeps none from now on.
        assertEquals(signedIn(null, server = "en", keepsLanguage = false), sources.last())
        // Asked again, as Settings opened again, and after the server's answers again.
        data.graph.session.refresh()
        testScheduler.runCurrent()
        assertEquals(signedIn(null, server = "en", keepsLanguage = false), data.graph.language.sources.first())

        // Another server may have the route; back on this one, it is tried again.
        val other = "http://192.168.1.30:7539"
        data.signIn(other)
        data.useServer(other)
        testScheduler.runCurrent()
        assertEquals(signedIn(null, server = "en"), sources.last())
        data.useServer()
        testScheduler.runCurrent()
        assertEquals(signedIn(null, server = "en"), sources.last())
    }

    @Test
    fun a405IsAServerWithoutTheRouteToo() = runTest {
        val data = signedIn(folder.newFolder()) {
            api.answer("server", ServerInfo("Home", "1.0", language = "en"))
        }
        val sources = collect(data.graph.language.sources)
        testScheduler.runCurrent()
        data.api.fail("setLanguage", ApiException(405, null, "method not allowed"))
        runCatching { data.graph.language.choose("es") }
        assertEquals(false, (sources.last() as LanguageSources.SignedIn).keepsLanguage)
    }

    private val alex: Me = Fixtures.decode("me/app_token.json")
}
