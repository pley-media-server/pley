package app.pley.android.auth

import java.io.IOException
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.async
import kotlinx.coroutines.delay
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import okhttp3.HttpUrl.Companion.toHttpUrl
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** Which of a server's addresses the app uses, at home and away (docs/ANDROID.md, "Home and away"). */
@OptIn(ExperimentalCoroutinesApi::class)
class ServerAddressesTest {
    private val home = "http://192.168.1.20:7539"
    private val vpn = "http://100.101.102.103:7539"
    private val internet = "https://pley.example.com"

    /** How long each address takes to prove itself, and what it fails with; none: at once. */
    private val answers = mutableMapOf<String, Pair<Long, IOException?>>()

    /** The addresses asked to prove themselves, in turn, and the time each was given. */
    private val asked = mutableListOf<String>()
    private val limits = mutableMapOf<String, Long>()
    private val kept = mutableListOf<StoredAddresses>()
    private var homeNetwork = true

    private fun TestScope.book(issuer: String = internet, server: List<String> = listOf(home, vpn)) = ServerAddresses(
        issuer,
        prove = { address, timeoutMs ->
            asked += address
            limits[address] = timeoutMs
            val (ms, error) = answers[address] ?: (0L to null)
            delay(ms)
            if (error != null) throw error
        },
        keep = { kept += it },
        scope = backgroundScope,
        clock = { testScheduler.currentTime },
        homeNetwork = { homeNetwork },
    ).also { it.fromServer(server) }

    private fun ServerAddresses.statusOf(url: String): AddressStatus = state.value.addresses.first { it.address.url == url }.status

    @Test
    fun theHomeAddressIsUsedWhenItProvesItself() = runTest {
        val book = book()
        assertEquals(home, book.proved())
        // The others proved themselves too, and are ready.
        runCurrent()
        assertEquals(AddressStatus.IN_USE, book.statusOf(home))
        assertEquals(AddressStatus.READY, book.statusOf(vpn))
        assertEquals(AddressStatus.READY, book.statusOf(internet))
        // Proved: no new proofs within the half hour.
        asked.clear()
        assertEquals(home, book.proved())
        assertEquals(emptyList<String>(), asked)
    }

    @Test
    fun awayTheBestAddressThatProvesItselfIsUsed() = runTest {
        answers[home] = 3_000L to IOException("no route")
        answers[vpn] = 0L to IOException("not connected")
        answers[internet] = 100L to null
        val book = book()
        val chosen = async { book.proved() }
        // The home address hasn't answered: it has a moment to.
        advanceTimeBy(101)
        assertFalse(chosen.isCompleted)
        advanceTimeBy(ServerAddresses.HEAD_START_MS)
        assertEquals(internet, chosen.await())
        advanceTimeBy(3_000)
        assertEquals(AddressStatus.UNREACHABLE, book.statusOf(home))
        assertEquals(AddressStatus.UNREACHABLE, book.statusOf(vpn))
        assertEquals(AddressStatus.IN_USE, book.statusOf(internet))
    }

    @Test
    fun aBetterAddressThatProvesItselfLaterTakesOver() = runTest {
        answers[home] = 1_000L to null
        answers[internet] = 100L to null
        val book = book(server = listOf(home))
        val chosen = async { book.proved() }
        advanceTimeBy(101 + ServerAddresses.HEAD_START_MS)
        assertEquals(internet, chosen.await())
        advanceTimeBy(1_000)
        assertEquals(home, book.inUse)
    }

    @Test
    fun aFailedAddressWaitsBeforeItIsTriedAgain() = runTest {
        answers[home] = 0L to IOException("no route")
        val book = book(server = listOf(home))
        assertEquals(internet, book.proved())
        runCurrent()
        asked.clear()
        // Within its 5 s, the home address isn't asked again.
        advanceTimeBy(4_000)
        book.proved()
        runCurrent()
        assertEquals(emptyList<String>(), asked)
        // Then it is, in the background, and takes over once it proves itself.
        answers.remove(home)
        advanceTimeBy(1_001)
        assertEquals(internet, book.proved())
        runCurrent()
        assertEquals(listOf(home), asked)
        assertEquals(home, book.inUse)
    }

    @Test
    fun theWaitsGrowAndANewNetworkEndsThem() = runTest {
        answers[home] = 0L to IOException("no route")
        val book = book(server = listOf(home))
        book.proved()
        runCurrent()
        // 5 s, then 15 s.
        advanceTimeBy(5_001)
        book.proved()
        runCurrent()
        asked.clear()
        advanceTimeBy(14_000)
        book.proved()
        runCurrent()
        assertEquals(emptyList<String>(), asked)
        // Another network: every address is tried at once.
        ServerProofs.networkMayHaveChanged()
        answers.remove(home)
        assertEquals(home, book.proved())
    }

    @Test
    fun aRequestWithoutAnAnswerHasTheAddressesChooseAgain() = runTest {
        val book = book()
        assertEquals(home, book.proved())
        runCurrent()
        book.unanswered(home)
        answers[home] = 0L to IOException("no route")
        // The next best that proves itself, while the home address waits.
        assertEquals(vpn, book.proved())
        assertEquals(AddressStatus.UNREACHABLE, book.statusOf(home))
    }

    @Test
    fun withoutAHomeNetworkHomeAddressesComeLast() = runTest {
        homeNetwork = false
        val book = book()
        assertEquals(vpn, book.proved())
        runCurrent()
        // Tried all the same, last: a VPN of the person's own may reach them.
        assertEquals(home, asked.last())
        assertEquals(AddressStatus.READY, book.statusOf(home))
        // One that doesn't answer needs a home network, as the screens say.
        ServerProofs.networkMayHaveChanged()
        answers[home] = 0L to IOException("no route")
        book.proved()
        runCurrent()
        assertEquals(AddressStatus.NEEDS_HOME_NETWORK, book.statusOf(home))
    }

    @Test
    fun plainHttpBeyondTheHomeNetworkIsNeverTried() = runTest {
        val plain = "http://pley.example.com"
        val book = book(server = listOf(plain))
        answers[internet] = 0L to IOException("down")
        assertTrue(runCatching { book.proved() }.exceptionOrNull() is IOException)
        runCurrent()
        assertFalse(plain in asked)
        assertEquals(AddressStatus.UNENCRYPTED, book.statusOf(plain))
    }

    @Test
    fun anotherDeviceEverywhereSaysSoAndNothingElse() = runTest {
        val book = book(server = listOf(home))
        answers[home] = 0L to NotYourServerException()
        answers[internet] = 0L to NotYourServerException()
        val everywhere = runCatching { book.proved() }.exceptionOrNull()
        assertTrue(everywhere is NotYourServerException)
        assertEquals(AddressStatus.OTHER_DEVICE, book.statusOf(home))
        // Out of reach at one address and another device at the other: out of reach.
        ServerProofs.networkMayHaveChanged()
        answers[internet] = 0L to IOException("no route")
        val mixed = runCatching { book.proved() }.exceptionOrNull()
        assertTrue(mixed is IOException && mixed !is NotYourServerException)
    }

    @Test
    fun aRefreshHasTheAddressInUseProveItselfAnew() = runTest {
        val book = book()
        assertEquals(home, book.proved())
        runCurrent()
        asked.clear()
        assertEquals(home, book.proved(fresh = true))
        assertEquals(listOf(home), asked)
        // It fails: another proves itself now.
        answers[home] = 0L to IOException("gone")
        asked.clear()
        assertEquals(vpn, book.proved(fresh = true))
        assertEquals(home, asked.first())
    }

    @Test
    fun requestsThatComeTogetherWaitForOneChoice() = runTest {
        answers[home] = 3_000L to IOException("no route")
        answers[vpn] = 3_000L to IOException("no route")
        answers[internet] = 3_000L to IOException("no route")
        val book = book()
        val first = async { runCatching { book.proved() } }
        val second = async { runCatching { book.proved() } }
        advanceTimeBy(3_001)
        assertTrue(first.await().isFailure)
        assertTrue(second.await().isFailure)
        // Each address was asked once, for both, and both failed together.
        assertEquals(listOf(home, vpn, internet), asked)
        assertEquals(3_001L, testScheduler.currentTime)
    }

    @Test
    fun anAddressRemovedWhileItProvesItselfIsNotUsed() = runTest {
        val added = "http://192.168.1.30:7539"
        answers[home] = 0L to IOException("no route")
        answers[vpn] = 0L to IOException("no route")
        answers[internet] = 0L to IOException("no route")
        val book = book()
        assertTrue(book.add(added))
        answers[added] = 500L to null
        val chosen = async { runCatching { book.proved() } }
        advanceTimeBy(100)
        assertTrue(book.remove(added))
        advanceTimeBy(500)
        assertTrue(chosen.await().isFailure)
        assertEquals(internet, book.inUse)
    }

    @Test
    fun aLoneAddressHasTheTimeAnyRequestHas() = runTest {
        val alone = book(issuer = home, server = emptyList())
        alone.proved()
        assertEquals(0L, limits[home])
        // Among others, each has a limit: a better one may answer meanwhile.
        limits.clear()
        book().proved()
        runCurrent()
        assertEquals(ServerAddresses.RACED_PROOF_MS, limits[vpn])
    }

    @Test
    fun anAddressThatCantBeAskedFailsAloneNotTheApp() = runTest {
        val broken = ServerAddresses(
            internet,
            prove = { address, _ -> if (address == home) throw IllegalArgumentException("Invalid URL port") },
            keep = {},
            scope = backgroundScope,
            clock = { testScheduler.currentTime },
        ).also { it.fromServer(listOf(home)) }
        assertEquals(internet, broken.proved())
        runCurrent()
        assertEquals(AddressStatus.UNREACHABLE, broken.statusOf(home))
    }

    @Test
    fun theListsAreKeptCleanAndInOrder() = runTest {
        val book = book(server = listOf("https://PLEY.example.com/", "ftp://nas", home, home, "not an address", "http://192.168.1.20:75390", "http://192.168.1.21:0"))
        assertEquals(StoredAddresses(server = listOf(internet, home)), kept.last())
        // The sign-in's first, then the server's, then the ones added here.
        assertTrue(book.add("http://nas.local:7539"))
        assertTrue(book.add(vpn))
        assertFalse("the server's already", book.add("http://192.168.1.20:7539/"))
        assertFalse("not an address", book.add("nas files"))
        assertFalse("a port nothing listens on", book.add("http://192.168.1.20:75390"))
        assertFalse("the internet's: only the server's list gives them", book.add("https://elsewhere.example.com"))
        assertEquals(
            listOf(
                internet to AddressSource.SIGN_IN,
                home to AddressSource.SERVER,
                "http://nas.local:7539" to AddressSource.ADDED,
                vpn to AddressSource.ADDED,
            ),
            book.all().map { it.url to it.source },
        )
        // Only an address added here can be removed.
        assertFalse(book.remove(home))
        assertTrue(book.remove("http://nas.local:7539"))
        assertEquals(StoredAddresses(server = listOf(internet, home), added = listOf(vpn)), kept.last())
        // Signing out forgets them all but the sign-in address.
        book.forget()
        assertEquals(listOf(internet), book.all().map { it.url })
    }

    @Test
    fun theServersListKeepsWhatItFoundAfterItsAdministrators() = runTest {
        val set = (1..16).map { "https://pley$it.example.com" }
        val found = listOf(home, vpn)
        val book = book(server = set + found)
        assertEquals(set + found, book.all().drop(1).map { it.url })
    }

    @Test
    fun theyComeBackAsKept() = runTest {
        val book = ServerAddresses(internet, prove = { _, _ -> }, keep = {}, scope = backgroundScope, clock = { 0 })
        book.restore(StoredAddresses(server = listOf(home), added = listOf(vpn)))
        assertEquals(listOf(internet, home, vpn), book.all().map { it.url })
        assertEquals(home, book.addressOf("http://192.168.1.20:7539/api/x?q=a|b".toHttpUrl()))
        assertNull(book.addressOf("http://192.168.1.21:7539".toHttpUrl()))
    }
}
