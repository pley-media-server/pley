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

    private fun signedIn(account: String?, accountKnown: Boolean = true, server: String? = null, serverKnown: Boolean = true) =
        LanguageSources.SignedIn(account, accountKnown, server, serverKnown)

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

    private val alex: Me = Fixtures.decode("me/app_token.json")
}
