package app.pley.android.screens.admin.livetv

import app.pley.android.model.PleyJson
import app.pley.android.network.ApiException
import app.pley.android.screens.admin.AdminDone
import app.pley.android.screens.admin.valueOrNull
import app.pley.android.testing.MainDispatcherRule
import app.pley.android.testing.data.FakeServerAdmin
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Rule
import org.junit.Test

/**
 * Live TV's pages: what each reads, the changes they send and their
 * bodies, and what the server's refusals leave shown.
 */
@OptIn(ExperimentalCoroutinesApi::class)
class LiveTvAdminViewModelTest {
    @get:Rule val main = MainDispatcherRule()

    private fun body(call: FakeServerAdmin.Call): JsonObject = call.body!!.jsonObject

    private fun JsonObject.text(key: String): String? = (get(key) as? kotlinx.serialization.json.JsonPrimitive)?.content

    // ---------------------------------------------------------------- the first page

    @Test
    fun theFirstPageReadsTheGroupsAndTheConnectionsWhileItShows() = runTest {
        val server = FakeServerAdmin()
        server.answer("GET /api/live/lineup/groups", """[{"name":"News","region":null,"label":"News","count":3,"hidden":0,"offline":0}]""")
        server.answer("GET /api/live/tuners", """[{"channel_id":9,"channel_name":"News","source_name":"Provider 1","started_at":1,"kbps":800,"viewers":[]}]""")
        val page = LiveTvAdminViewModel(server)
        assertEquals(1, page.groups.state.value.valueOrNull?.size)
        assertNull(page.tuners.value)

        val watching = backgroundScope.launch { page.watch() }
        runCurrent()
        assertEquals(listOf(9L), page.tuners.value?.map { it.channelId })
        advanceTimeBy(LiveTvAdminViewModel.TUNERS_EVERY * 2 + 1)
        assertEquals(3, server.callsTo("GET /api/live/tuners").size)
        assertEquals(1, server.callsTo("GET /api/live/lineup/groups").size)

        // Off screen, nothing is read.
        watching.cancel()
        val tunerReads = server.callsTo("GET /api/live/tuners").size
        advanceTimeBy(LiveTvAdminViewModel.TUNERS_EVERY * 3)
        assertEquals(tunerReads, server.callsTo("GET /api/live/tuners").size)
    }

    @Test
    fun aConnectionIsStoppedForEveryone() = runTest {
        val server = FakeServerAdmin()
        var tuners = """[{"channel_id":9,"channel_name":"News"}]"""
        server.answer("GET /api/live/tuners") { PleyJson.parseToJsonElement(tuners) }
        server.answer("DELETE /api/live/tuners/9") {
            tuners = "[]"
            null
        }
        val page = LiveTvAdminViewModel(server)
        val said = mutableListOf<AdminDone>()
        backgroundScope.launch(UnconfinedTestDispatcher(testScheduler)) { page.confirmations.collect { said += it } }
        val tuner = PleyJson.decodeFromString(app.pley.android.model.admin.TunerStatus.serializer(), """{"channel_id":9,"channel_name":"News"}""")
        page.stop(tuner).join()
        assertEquals(1, server.callsTo("DELETE /api/live/tuners/9").size)
        assertEquals(emptyList<Any>(), page.tuners.value)
        assertEquals(listOf(AdminDone(LiveTvAdminViewModel.STOPPED, listOf("News"))), said)
    }

    @Test
    fun groupsAndRegionsAreHiddenAndShownAtOnce() = runTest {
        val server = FakeServerAdmin()
        server.answer(
            "GET /api/live/lineup/groups",
            """[{"name":"UK| News","region":"UK","label":"News","count":10,"hidden":0},
               {"name":"UK| Films","region":"UK","label":"Films","count":8,"hidden":3},
               {"name":"","region":null,"label":"","count":2,"hidden":0}]""",
        )
        server.answer("PATCH /api/live/lineup", null)
        val page = LineupGroupsViewModel(server)
        backgroundScope.launch(UnconfinedTestDispatcher(testScheduler)) { page.sections.collect {} }
        val said = mutableListOf<AdminDone>()
        backgroundScope.launch(UnconfinedTestDispatcher(testScheduler)) { page.confirmations.collect { said += it } }
        val sections = page.sections.first { it != null }!!
        assertEquals(listOf("UK", null), sections.map { it.region })

        page.setHidden(sections[0], true)
        val region = body(server.callsTo("PATCH /api/live/lineup").single())
        assertEquals(PleyJson.parseToJsonElement("""["UK| News","UK| Films"]"""), region["groups"])
        assertEquals("true", region.text("hidden"))
        assertEquals(JsonArray(emptyList()), region["ids"])

        page.setHidden(sections[1].groups.single(), false)
        assertEquals(PleyJson.parseToJsonElement("""[""]"""), body(server.callsTo("PATCH /api/live/lineup")[1])["groups"])
        assertEquals(
            listOf(AdminDone(LineupGroupsViewModel.REGION_HIDDEN, listOf("UK")), AdminDone(LineupGroupsViewModel.UNNAMED_SHOWN)),
            said,
        )

        page.toggle("UK")
        assertEquals(setOf("UK"), page.open.value)
        page.find("films")
        page.show(GroupsShown.Shown)
        assertEquals(listOf("UK| Films"), page.sections.first { it?.size == 1 }!!.single().groups.map { it.name })
    }

    // ---------------------------------------------------------------- the lineup

    private fun lineup(total: Int, hidden: Int = 0) = PleyJson.parseToJsonElement(
        """{"total":$total,"hidden":$hidden,"offset":0,"channels":[""" +
            (1..minOf(total, LINEUP_PAGE)).joinToString(",") { """{"id":$it,"name":"Channel $it","hidden":false}""" } + "]}",
    )

    @Test
    fun theLineupIsReadAPageAtATimeAsItScrolls() = runTest {
        val server = FakeServerAdmin()
        server.answer("GET /api/live/lineup/page") { lineup(250) }
        server.answer("GET /api/live/lineup/groups", "[]")
        val page = LineupViewModel(server, "http://nas:7539")
        // The counts, and the first page of the list.
        assertEquals(2, server.callsTo("GET /api/live/lineup/page").size)
        assertEquals(250, page.list.value.total)
        assertEquals(1L, page.list.value.at(0)?.id)
        assertNull(page.list.value.at(100))

        page.visible(0, 12)
        assertEquals(2, server.callsTo("GET /api/live/lineup/page").size)
        page.visible(90, 102)
        assertEquals(3, server.callsTo("GET /api/live/lineup/page").size)
        assertEquals(
            listOf("state" to "all", "offset" to "100", "limit" to "100"),
            server.callsTo("GET /api/live/lineup/page").last().query,
        )
        assertEquals(1L, page.list.value.at(100)?.id)
        page.visible(240, 249)
        assertEquals(setOf(0, 1, 2), page.list.value.pages.keys)

        // Another filter lists from its first page, and chooses nothing.
        page.choose(5)
        page.setState(app.pley.android.model.admin.LineupState.Hidden)
        assertEquals(app.pley.android.model.admin.LineupState.Hidden, page.list.value.filter.state)
        assertEquals(setOf(0), page.list.value.pages.keys)
        assertEquals(
            listOf("state" to "hidden", "offset" to "0", "limit" to "100"),
            server.callsTo("GET /api/live/lineup/page").last().query,
        )
        assertEquals(LineupChoice(), page.choice.value)
    }

    @Test
    fun channelsAreHiddenOneAtATimeChosenOrAllThatTheFilterKeeps() = runTest {
        val server = FakeServerAdmin()
        server.answer("GET /api/live/lineup/page") { lineup(150) }
        server.answer("GET /api/live/lineup/groups", "[]")
        server.answer("PATCH /api/live/lineup", null)
        val page = LineupViewModel(server, "")
        val said = mutableListOf<AdminDone>()
        backgroundScope.launch(UnconfinedTestDispatcher(testScheduler)) { page.confirmations.collect { said += it } }

        page.setHidden(page.list.value.at(0)!!, true)
        val one = body(server.callsTo("PATCH /api/live/lineup").single())
        assertEquals(PleyJson.parseToJsonElement("[1]"), one["ids"])
        assertEquals("true", one.text("hidden"))
        assertFalse("filter" in one)

        page.choose(2)
        page.choose(3)
        page.choose(2)
        assertEquals(LineupChoice(ids = setOf(3)), page.choice.value)
        page.choose(4)
        page.setChosenHidden(false)
        val chosen = body(server.callsTo("PATCH /api/live/lineup")[1])
        assertEquals(PleyJson.parseToJsonElement("[3,4]"), chosen["ids"])
        assertEquals("false", chosen.text("hidden"))
        assertEquals(LineupChoice(), page.choice.value)

        page.find("news")
        advanceTimeBy(LineupViewModel.SEARCH_WAIT + 1)
        assertEquals("news", page.list.value.filter.q)
        page.chooseAll()
        assertEquals(150, page.choice.value.count(150))
        page.setChosenHidden(true)
        val all = body(server.callsTo("PATCH /api/live/lineup")[2])
        assertEquals(JsonArray(emptyList()), all["ids"])
        assertEquals("news", (all["filter"] as JsonObject).text("q"))
        assertEquals("all", (all["filter"] as JsonObject).text("state"))

        assertEquals(
            listOf(AdminDone(LineupViewModel.SHOWN, listOf(2)), AdminDone(LineupViewModel.HIDDEN, listOf(150))),
            said,
        )
    }

    @Test
    fun aChannelsNameNumberAndGuideAreSavedTogether() = runTest {
        val server = FakeServerAdmin()
        server.answer("GET /api/live/lineup/page") { lineup(3) }
        server.answer("GET /api/live/lineup/groups", "[]")
        server.answer("PATCH /api/live/lineup", null)
        val page = LineupViewModel(server, "")
        val channel = page.list.value.at(1)!!
        page.edit(channel)
        assertEquals(channel, page.editing.value)
        page.save(channel, " News ", "", "news.uk")
        val sent = body(server.callsTo("PATCH /api/live/lineup").single())
        assertEquals(PleyJson.parseToJsonElement("[2]"), sent["ids"])
        assertEquals("News", sent.text("name"))
        assertEquals("", sent.text("number"))
        assertEquals("news.uk", sent.text("guide_id"))
        assertFalse("hidden" in sent)
        assertNull(page.editing.value)
    }

    @Test
    fun aFirstPageThatCantBeReadIsThePagesErrorUntilRetried() = runTest {
        val server = FakeServerAdmin()
        val down = ApiException(503, "unavailable", "busy")
        server.fail("GET /api/live/lineup/page", down)
        server.answer("GET /api/live/lineup/groups", "[]")
        val page = LineupViewModel(server, "")
        assertEquals(down, page.list.value.error)
        server.answer("GET /api/live/lineup/page") { lineup(1) }
        page.retry()
        assertNull(page.list.value.error)
        assertEquals(1, page.list.value.total)
    }
}
