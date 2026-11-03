package app.pley.android.fixtures

import app.pley.android.model.CollectionDetail
import app.pley.android.model.CollectionList
import app.pley.android.model.CollectionRef
import app.pley.android.model.Health
import app.pley.android.model.HistoryPage
import app.pley.android.model.Hub
import app.pley.android.model.Hubs
import app.pley.android.model.ItemCollections
import app.pley.android.model.ItemDetail
import app.pley.android.model.ItemKind
import app.pley.android.model.ItemList
import app.pley.android.model.ItemsPage
import app.pley.android.model.Library
import app.pley.android.model.LibraryFilters
import app.pley.android.model.LibrarySort
import app.pley.android.model.MaybeItem
import app.pley.android.model.Me
import app.pley.android.model.PersonCredit
import app.pley.android.model.PersonDetail
import app.pley.android.model.PersonRole
import app.pley.android.model.PersonSummary
import app.pley.android.model.PleyJson
import app.pley.android.model.SearchResults
import app.pley.android.model.ServerInfo
import java.security.MessageDigest
import kotlinx.serialization.builtins.ListSerializer
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.jsonObject
import mockwebserver3.MockResponse

/**
 * What the app reads: the server, the profile, libraries and their
 * listings, hubs, items, people, collections, search, history and every
 * picture (docs/API.md: "Libraries", "Items", "Home & search", "Cast and
 * crew", "Collections", "Watch history").
 */
internal class BrowseRoutes(private val s: State) {
    private val catalog get() = s.catalog
    private val views get() = s.views

    fun routes(): List<Route> = listOf(
        public("GET", "/api/health") { _, _ -> Reply.json(Health.serializer(), Health("ok")) },
        public("GET", "/api/server") { call, _ -> server(call) },
        route("GET", "/api/auth/me", Need.AnyApp) { _, c, _ -> Reply.json(Me.serializer(), views.me(c.profile, c.scope)) },
        route("GET", "/api/users/*/avatar", Need.AnyApp, free = true) { call, _, v ->
            val profile = v[0].toLongOrNull()?.let(s::profile)
            if (profile == null || !profile.hasAvatar) {
                Reply.notFound("no such picture")
            } else {
                Reply.picture(s.pictures.artwork(-profile.id, Pictures.Shape.Square, call.query("w")?.toIntOrNull()))
            }
        },
        route("GET", "/api/libraries", LIBRARY) { _, _, _ -> Reply.json(ListSerializer(Library.serializer()), catalog.libraries) },
        route("GET", "/api/libraries/*", LIBRARY) { _, _, v ->
            library(v[0])?.let { Reply.json(Library.serializer(), it) } ?: Reply.notFound("no such library")
        },
        route("GET", "/api/libraries/*/items", LIBRARY) { call, c, v -> listing(call, c, v[0]) },
        route("GET", "/api/libraries/*/filters", LIBRARY) { _, _, v ->
            val library = library(v[0]) ?: return@route Reply.notFound("no such library")
            val items = top(library.id)
            Reply.json(
                LibraryFilters.serializer(),
                LibraryFilters(items.flatMap { it.genres }.distinct().sorted(), items.mapNotNull { it.year }.distinct().sortedDescending()),
            )
        },
        route("GET", "/api/libraries/*/hubs", LIBRARY) { _, c, v ->
            val library = library(v[0]) ?: return@route Reply.notFound("no such library")
            Reply.json(Hubs.serializer(), Hubs(libraryHubs(library, c.profile)))
        },
        route("GET", "/api/libraries/*/collections", LIBRARY) { _, _, v ->
            val library = library(v[0]) ?: return@route Reply.notFound("no such library")
            val list = catalog.collections.values.filter { it.libraryId == library.id }.map(views::collectionSummary)
                .filter { it.itemCount > 0 }
                .sortedBy { it.title.lowercase() }
            Reply.json(CollectionList.serializer(), CollectionList(list))
        },
        route("GET", "/api/collections/*", LIBRARY) { _, c, v ->
            val collection = v[0].toLongOrNull()?.let { catalog.collections[it] } ?: return@route Reply.notFound("no such collection")
            Reply.json(CollectionDetail.serializer(), views.collectionDetail(collection, c.profile))
        },
        route("GET", "/api/collections/*/images/*", LIBRARY, free = true) { call, _, v ->
            val collection = v[0].toLongOrNull()?.let { catalog.collections[it] }
            val shape = when (v[1]) {
                "poster" -> Pictures.Shape.Poster
                "backdrop" -> Pictures.Shape.Wide
                else -> null
            }
            if (collection == null || shape == null) {
                Reply.notFound("no such picture")
            } else {
                Reply.picture(s.pictures.artwork(collection.id * 1_000 + 7, shape, call.query("w")?.toIntOrNull()))
            }
        },
        route("GET", "/api/items/*", LIBRARY) { _, c, v ->
            val item = item(v[0]) ?: return@route Reply.notFound("no such item")
            Reply.json(ItemDetail.serializer(), views.detail(item, c.profile))
        },
        route("GET", "/api/items/*/children", LIBRARY) { _, c, v ->
            val item = item(v[0]) ?: return@route Reply.notFound("no such item")
            val children = catalog.children[item.id].orEmpty().mapNotNull { views.summary(it, c.profile) }
            Reply.json(ItemList.serializer(), ItemList(children))
        },
        route("GET", "/api/items/*/on-deck", LIBRARY) { _, c, v ->
            val item = item(v[0]) ?: return@route Reply.notFound("no such item")
            Reply.json(MaybeItem.serializer(), MaybeItem(onDeck(item, c.profile)?.let { views.summary(it, c.profile) }))
        },
        route("GET", "/api/items/*/next", LIBRARY) { _, c, v ->
            val item = item(v[0]) ?: return@route Reply.notFound("no such item")
            Reply.json(MaybeItem.serializer(), MaybeItem(next(item)?.let { views.summary(it, c.profile) }))
        },
        route("GET", "/api/items/*/related", LIBRARY) { call, c, v ->
            val item = item(v[0]) ?: return@route Reply.notFound("no such item")
            val limit = (call.query("limit")?.toIntOrNull() ?: 20).coerceIn(0, 60)
            Reply.json(ItemList.serializer(), ItemList(related(item, limit).map { views.summary(it, c.profile) }))
        },
        route("GET", "/api/items/*/images/*", LIBRARY, free = true) { call, _, v -> itemImage(call, v[0], v[1]) },
        route("GET", "/api/items/*/collections", LIBRARY) { _, _, v ->
            val item = item(v[0]) ?: return@route Reply.notFound("no such item")
            val refs = catalog.itemCollections[item.id].orEmpty().mapNotNull { catalog.collections[it] }.map { CollectionRef(it.id, it.title) }
            Reply.json(ItemCollections.serializer(), ItemCollections(refs))
        },
        route("GET", "/api/people/*", LIBRARY) { _, c, v -> person(v[0], c) },
        route("GET", "/api/people/*/photo", LIBRARY, free = true) { call, _, v ->
            val person = v[0].toLongOrNull()?.let { catalog.people[it] }
            if (person == null || !person.hasPhoto) {
                Reply.notFound("no photo")
            } else {
                Reply.picture(s.pictures.artwork(person.id * 3 + 1, Pictures.Shape.Poster, call.query("w")?.toIntOrNull()))
            }
        },
        route("GET", "/api/history", LIBRARY) { call, c, _ -> history(call, c) },
        route("GET", "/api/hubs/home", LIBRARY) { _, c, _ -> Reply.json(Hubs.serializer(), Hubs(homeHubs(c.profile))) },
        route("GET", "/api/search", LIBRARY) { call, c, _ -> search(call, c) },
        route("GET", "/api/events", LIBRARY) { call, c, _ -> s.events.open(call, c) },
        // Only pley's web app reads these; apps' tokens are refused.
        route("GET", "/api/activity", Need.Scope(WEB_ONLY)) { _, _, _ -> Reply.noContent() },
    )

    private fun library(id: String): Library? = id.toLongOrNull()?.let { want -> catalog.libraries.firstOrNull { it.id == want } }

    private fun item(id: String): ItemDetail? = id.toLongOrNull()?.let(catalog::item)

    private fun top(library: Long): List<ItemDetail> = catalog.topLevel[library].orEmpty().mapNotNull(catalog::item)

    // ---------------------------------------------------------------- listings

    /**
     * GET /api/libraries/{id}/items, with an ETag that changes with the
     * query and the profile's state, and 304 for a tag that still holds.
     */
    private fun listing(call: Call, caller: Caller, id: String): MockResponse {
        val library = library(id) ?: return Reply.notFound("no such library")
        val profile = caller.profile
        val tag = "\"" + sha1("${library.id}|${call.url.encodedQuery.orEmpty()}|${profile.id}:${profile.version}").take(20) + "\""
        val cache = mapOf("ETag" to tag, "Cache-Control" to "private, no-cache")
        if (call.header("If-None-Match") == tag) {
            return MockResponse.Builder().code(304).apply { cache.forEach { (k, v) -> setHeader(k, v) } }.build()
        }
        val sortName = call.query("sort") ?: LibrarySort.Title.wire
        val sort = LibrarySort.entries.firstOrNull { it.wire == sortName } ?: return Reply.badRequest("unknown sort: $sortName")
        val order = call.query("order")
        if (order != null && order != "asc" && order != "desc") return Reply.badRequest("order is asc or desc")
        val offset = call.query("offset")?.toIntOrNull()?.coerceAtLeast(0) ?: 0
        val limit = call.query("limit")?.toIntOrNull()?.coerceAtLeast(0)

        var items = top(library.id)
        call.query("genre")?.let { genre -> items = items.filter { genre in it.genres } }
        call.query("year")?.toIntOrNull()?.let { year -> items = items.filter { it.year == year } }
        call.query("watched")?.let { watched -> items = items.filter { views.state(it, profile).watched == (watched == "true") } }

        val ascendingByDefault = sort == LibrarySort.Title || sort == LibrarySort.Year || sort == LibrarySort.Duration
        val descending = order?.let { it == "desc" } ?: !ascendingByDefault
        // Each item's key once: a show's comes from all its episodes.
        val keys: Map<Long, Double> = items.associate { item ->
            item.id to when (sort) {
                LibrarySort.Title -> 0.0
                LibrarySort.Year -> (item.year ?: 0).toDouble()
                LibrarySort.Added -> item.addedAt.toDouble()
                LibrarySort.Rating -> item.rating ?: -1.0
                LibrarySort.UserRating -> views.state(item, profile).rating ?: -1.0
                LibrarySort.Release -> item.releaseDate.orEmpty().filter(Char::isDigit).toDoubleOrNull() ?: 0.0
                LibrarySort.Duration -> (item.durationMs ?: 0L).toDouble()
                LibrarySort.LastPlayed -> (views.state(item, profile).lastPlayedAt ?: 0L).toDouble()
                LibrarySort.Bitrate -> (views.mediaSummary(item)?.bitrate ?: 0L).toDouble()
            }
        }
        val byTitle = compareBy<ItemDetail> { it.sortTitle.lowercase() }.thenBy { it.id }
        val key = compareBy<ItemDetail> { keys.getValue(it.id) }
        val sorted = items.sortedWith((if (descending) key.reversed() else key).then(byTitle))
        val page = sorted.drop(offset).let { if (limit != null) it.take(limit) else it }
            .map { views.summary(it, profile, listing = true, media = sort == LibrarySort.Bitrate) }
        return Reply.json(ItemsPage.serializer(), ItemsPage(sorted.size, page), headers = cache)
    }

    // ---------------------------------------------------------------- hubs

    /** Continue Watching: in progress, newest played first, but what was taken out of it. */
    private fun inProgress(profile: Profile, library: Long?): List<ItemDetail> =
        profile.states.entries
            .filter { (_, state) -> state.positionMs > 0 && !state.watched }
            .sortedByDescending { it.value.lastPlayedAt ?: 0L }
            .mapNotNull { catalog.item(it.key) }
            .filter { library == null || it.libraryId == library }
            .filterNot { views.hiddenFromContinue(it, profile) }
            .take(20)

    /** The newest of a library: movies, shows, or albums. */
    private fun recent(library: Library): List<ItemDetail> {
        val items = if (library.id == FixtureIds.MUSIC) {
            top(library.id).flatMap { artist -> catalog.children[artist.id].orEmpty().mapNotNull(catalog::item) }
        } else {
            top(library.id)
        }
        return items.sortedByDescending { it.addedAt }.take(20)
    }

    private fun homeHubs(profile: Profile): List<Hub> = buildList {
        val watching = inProgress(profile, null)
        if (watching.isNotEmpty()) add(Hub("continue_watching", "Continue Watching", null, watching.map { views.summary(it, profile) }))
        val listed = views.listed(profile)
        if (listed.isNotEmpty()) add(Hub("my_list", "My List", null, listed.take(20).map { views.summary(it, profile) }))
        for (library in catalog.libraries) {
            add(Hub("recently_added:${library.id}", "Recently Added in ${library.name}", library.id, recent(library).map { views.summary(it, profile) }))
        }
    }

    private fun libraryHubs(library: Library, profile: Profile): List<Hub> {
        val id = library.id
        val top = top(id)
        val rows = listOf(
            Hub("continue_watching:$id", "Continue Watching", id, inProgress(profile, id).map { views.summary(it, profile) }),
            Hub("recently_added:$id", "Recently Added", id, recent(library).map { views.summary(it, profile) }),
            Hub(
                "recently_released:$id", "Recently Released", id,
                if (id == FixtureIds.MUSIC) emptyList() else top.sortedByDescending { it.releaseDate.orEmpty() }.take(20).map { views.summary(it, profile) },
            ),
            Hub(
                "top_rated:$id", "Top Rated", id,
                top.filter { it.rating != null }.sortedByDescending { it.rating }.take(20).map { views.summary(it, profile) },
            ),
        )
        return rows.filter { it.items.isNotEmpty() }
    }

    // ---------------------------------------------------------------- items

    /** The episode Play starts: in progress, else the next unwatched, else the first. */
    private fun onDeck(item: ItemDetail, profile: Profile): ItemDetail? {
        if (item.kind != ItemKind.Show && item.kind != ItemKind.Season) return null
        val episodes = catalog.leaves(item.id)
        val chosen = episodes.firstOrNull { profile.state(it).positionMs > 0 && !profile.state(it).watched }
            ?: episodes.firstOrNull { !profile.state(it).watched }
            ?: episodes.firstOrNull()
        return chosen?.let(catalog::item)
    }

    private fun next(item: ItemDetail): ItemDetail? {
        val top = when (item.kind) {
            ItemKind.Episode -> item.grandparentId
            ItemKind.Track -> item.parentId
            else -> return null
        } ?: return null
        val leaves = catalog.leaves(top)
        val at = leaves.indexOf(item.id)
        return leaves.getOrNull(at + 1)?.let(catalog::item)
    }

    /** Titles of the same library sharing genres, most shared first, then by rating. */
    private fun related(item: ItemDetail, limit: Int): List<ItemDetail> {
        if (item.kind != ItemKind.Movie && item.kind != ItemKind.Show) return emptyList()
        return top(item.libraryId)
            .filter { it.id != item.id }
            .map { it to it.genres.count(item.genres::contains) }
            .filter { it.second > 0 }
            .sortedWith(compareByDescending<Pair<ItemDetail, Int>> { it.second }.thenByDescending { it.first.rating ?: 0.0 }.thenBy { it.first.id })
            .take(limit)
            .map { it.first }
    }

    private fun itemImage(call: Call, id: String, kind: String): MockResponse {
        val item = item(id) ?: return Reply.notFound("no such item")
        val shape = when (kind) {
            "poster" -> if (item.kind == ItemKind.Artist || item.kind == ItemKind.Album || item.kind == ItemKind.Track) Pictures.Shape.Square else Pictures.Shape.Poster
            "backdrop", "thumb" -> Pictures.Shape.Wide
            else -> return Reply.notFound("no such picture")
        }
        // Falls back to the parent's: a season's backdrop is its show's.
        val owner = when {
            kind == "backdrop" && item.kind == ItemKind.Season -> item.parentId
            kind == "backdrop" && item.kind == ItemKind.Episode -> item.grandparentId
            kind == "poster" && (item.kind == ItemKind.Episode || item.kind == ItemKind.Track) -> item.parentId
            else -> item.id
        } ?: item.id
        val seed = owner * 3 + if (kind == "poster") 0 else 1
        return Reply.picture(s.pictures.artwork(seed, shape, call.query("w")?.toIntOrNull()))
    }

    private fun person(id: String, caller: Caller): MockResponse {
        val person = id.toLongOrNull()?.let { catalog.people[it] } ?: return Reply.notFound("no such person")
        val items = catalog.credits[person.id].orEmpty().distinct().mapNotNull(catalog::item)
        // Episode credits count towards their show.
        val byTitle = items.groupBy { if (it.kind == ItemKind.Episode) it.grandparentId ?: it.id else it.id }
        val credits = byTitle.mapNotNull { (titleId, credited) ->
            val title = catalog.item(titleId) ?: return@mapNotNull null
            val roles = credited.flatMap { item ->
                (item.cast + item.crew).filter { it.id == person.id }.map { credit ->
                    PersonRole(credit.role, credit.character, if (item.kind == ItemKind.Episode) 1 else null)
                }
            }.distinctBy { it.role to it.character }
            PersonCredit(views.summary(title, caller.profile), roles)
        }.sortedByDescending { it.item.year ?: 0 }
        if (credits.isEmpty()) return Reply.notFound("no such person")
        return Reply.json(PersonDetail.serializer(), PersonDetail(person.id, person.name, person.photoUrl, credits))
    }

    // ---------------------------------------------------------------- search, history

    private fun search(call: Call, caller: Caller): MockResponse {
        val q = call.query("q").orEmpty().trim().lowercase()
        val limit = (call.query("limit")?.toIntOrNull() ?: 40).coerceIn(0, 200)
        if (q.isEmpty()) return Reply.json(SearchResults.serializer(), SearchResults())
        val words = q.split(' ').filter { it.isNotEmpty() }
        fun matches(name: String): Boolean {
            val parts = name.lowercase().split(' ', '-', ':')
            return words.all { word -> parts.any { it.startsWith(word) } }
        }
        val items = catalog.items.values
            .filter { it.kind != ItemKind.Extra && it.kind != ItemKind.Season && matches(it.title) }
            .sortedWith(
                compareBy<ItemDetail> { if (it.title.lowercase() == q) 0 else if (it.title.lowercase().startsWith(q)) 1 else 2 }
                    .thenBy { if (it.kind == ItemKind.Movie || it.kind == ItemKind.Show) 0 else 1 }
                    .thenBy { it.sortTitle.lowercase() },
            )
            .take(limit)
            .map { views.summary(it, caller.profile) }
        val people = catalog.people.values
            .filter { matches(it.name) }
            .sortedByDescending { catalog.credits[it.id].orEmpty().size }
            .take(limit)
            .map { PersonSummary(it.id, it.name, it.photoUrl) }
        return Reply.json(SearchResults.serializer(), SearchResults(items, people))
    }

    private fun history(call: Call, caller: Caller): MockResponse {
        val offset = call.query("offset")?.toIntOrNull()?.coerceAtLeast(0) ?: 0
        val limit = (call.query("limit")?.toIntOrNull() ?: 50).coerceIn(0, 200)
        val itemId = call.query("item_id")?.toLongOrNull()
        val libraryId = call.query("library_id")?.toLongOrNull()
        val since = call.query("since")?.toLongOrNull()
        val under = itemId?.let { catalog.leaves(it).toSet() + it }
        val records = caller.profile.history
            .filter { under == null || it.itemId in under }
            .filter { libraryId == null || catalog.item(it.itemId)?.libraryId == libraryId }
            .filter { since == null || it.viewedAt >= since }
            .sortedByDescending { it.viewedAt }
        val entries = records.drop(offset).take(limit).mapNotNull { views.history(it, caller.profile) }
        return Reply.json(HistoryPage.serializer(), HistoryPage(records.size, entries))
    }

    /** The server, with its id, and its proof when asked (docs/API.md, "Telling servers apart"). */
    private fun server(call: Call): MockResponse {
        val info = PleyJson.encodeToJsonElement(ServerInfo.serializer(), ServerInfo(NAME, VERSION)).jsonObject
        val challenge = call.query("challenge")
        val proofId = call.query("proof_id")
        if (challenge == null && proofId == null) return Reply.json(JsonObject(info + ("id" to JsonPrimitive(s.oauth.serverId))).toString())
        val proof = if (challenge != null && proofId != null) s.oauth.proof(call, challenge, proofId) else null
        proof ?: return Reply.error(400, "bad_request", "challenge and proof_id go together, each written as docs/API.md says")
        val answer = info + ("id" to JsonPrimitive(s.oauth.serverId)) + ("proof" to JsonPrimitive(proof))
        return Reply.json(JsonObject(answer).toString(), headers = mapOf("Cache-Control" to "no-store"))
    }

    companion object {
        const val NAME = "Fixture Server"
        const val VERSION = "0.1.0-fixtures"

        /** A scope no app has: the route is the web app's only. */
        const val WEB_ONLY = "web"

        private fun sha1(text: String): String =
            MessageDigest.getInstance("SHA-1").digest(text.toByteArray()).joinToString("") { "%02x".format(it) }
    }
}
