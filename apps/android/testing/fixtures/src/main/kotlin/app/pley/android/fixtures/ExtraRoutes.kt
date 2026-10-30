package app.pley.android.fixtures

import app.pley.android.model.AddSubtitle
import app.pley.android.model.PlaybackNetwork
import app.pley.android.model.SubtitleResult
import app.pley.android.model.SubtitleResults
import app.pley.android.model.SubtitleTrack
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.intOrNull
import mockwebserver3.MockResponse
import okio.Buffer

/** What the server remembers of each client's network (docs/API.md, "Network memory and speed test"). */
internal class NetworkMemory {
    /** The client is on another network than the server's (a speed test is due when nothing is known). */
    var remote = false

    /** The routes are there; off, they answer 404 as an older server does. */
    var available = true

    val ceilings = HashMap<String, Pair<Int, Long>>()
    val probes = HashMap<String, Triple<Int, Boolean, Long>>()
}

/**
 * Automatic's network memory and its speed test, and subtitles found
 * online (docs/API.md): GET and PUT /api/playback/network as the server
 * keeps them (a ceiling for 24 h, a speed test for 7 days at 70 %), GET
 * /api/playback/speedtest (zeros, as many as asked), GET
 * /api/items/{id}/subtitles/search (two results per language, or 503
 * "not set up") and POST /api/items/{id}/subtitles, whose tracks the
 * subtitle route then serves.
 */
internal class ExtraRoutes(private val s: State) {
    private val network get() = s.network

    fun routes(): List<Route> = listOf(
        route("GET", "/api/playback/network", PLAY) { call, c, _ ->
            if (!network.available) return@route Reply.notFound("no such session")
            val client = call.query("client_id")?.takeIf { it.length in 1..128 } ?: return@route Reply.badRequest("client_id is needed")
            Reply.json(PlaybackNetwork.serializer(), answer(key(c, client)))
        },
        route("PUT", "/api/playback/network", PLAY) { call, c, _ ->
            if (!network.available) return@route Reply.error(405, "bad_request", "PUT isn't allowed here")
            val body = runCatching { app.pley.android.model.PleyJson.parseToJsonElement(call.text) as? JsonObject }.getOrNull()
                ?: return@route Reply.badRequest("the body isn't JSON")
            val client = (body["client_id"] as? JsonPrimitive)?.content?.takeIf { it.length in 1..128 } ?: return@route Reply.badRequest("client_id is needed")
            val key = key(c, client)
            val now = s.nowMs()
            when {
                "probe_kbps" in body -> {
                    val kbps = (body["probe_kbps"] as? JsonPrimitive)?.intOrNull ?: return@route Reply.badRequest("probe_kbps")
                    val complete = (body["probe_complete"] as? JsonPrimitive)?.booleanOrNull ?: false
                    network.probes[key] = Triple(kbps, complete, now)
                }
                "ceiling_kbps" in body -> {
                    val value = body["ceiling_kbps"]
                    if (value is JsonNull) {
                        network.ceilings.remove(key)
                        network.probes[key] = Triple(0, true, now)
                    } else {
                        val kbps = (value as? JsonPrimitive)?.intOrNull ?: return@route Reply.badRequest("ceiling_kbps")
                        network.ceilings[key] = kbps to now
                    }
                }
                else -> return@route Reply.badRequest("probe_kbps or ceiling_kbps")
            }
            Reply.json(PlaybackNetwork.serializer(), answer(key))
        },
        route("GET", "/api/playback/speedtest", PLAY, free = true) { call, _, _ ->
            val n = (call.query("bytes")?.toLongOrNull() ?: DEFAULT_BYTES).coerceIn(MIN_BYTES, MAX_BYTES)
            MockResponse.Builder()
                .code(200)
                .setHeader("Content-Type", "application/octet-stream")
                .setHeader("Cache-Control", "no-store")
                .body(Buffer().write(ByteArray(n.toInt())))
                .build()
        },
        route("GET", "/api/items/*/subtitles/search", PLAY) { call, _, v ->
            val item = v[0].toLongOrNull()?.let(s.catalog::item) ?: return@route Reply.notFound("no such item")
            if (!s.subtitleSearch) {
                return@route Reply.json(
                    """{"code":"unavailable","message":"Subtitle search isn't set up on this server","setup":false}""",
                    code = 503,
                )
            }
            val languages = call.query("language")?.split(',')?.filter { it.isNotBlank() }?.ifEmpty { null } ?: listOf("en")
            val results = languages.flatMap { language ->
                listOf(
                    SubtitleResult("opensubtitles:${item.id}1", "opensubtitles", language, "${item.title}.1080p.BluRay.x264-GRP", "srt", downloads = 1234, matched = "hash"),
                    SubtitleResult("opensubtitles:${item.id}2", "opensubtitles", language, "${item.title}.720p.WEB", "srt", hearingImpaired = true, downloads = 56, matched = "title"),
                )
            }
            Reply.json(SubtitleResults.serializer(), SubtitleResults(results))
        },
        route("POST", "/api/items/*/subtitles", PLAY) { call, _, v ->
            val item = v[0].toLongOrNull()?.let(s.catalog::item) ?: return@route Reply.notFound("no such item")
            val add = call.body(AddSubtitle.serializer()) ?: return@route Reply.badRequest("the body isn't a subtitle to add")
            val file = add.fileId?.let { id -> item.files.firstOrNull { it.id == id } } ?: item.files.firstOrNull() ?: return@route Reply.notFound("no file")
            val added = s.addedSubtitles.getOrPut(file.id) { mutableListOf() }
            val index = ADDED_FROM + added.size
            val track = SubtitleTrack(
                index = index,
                codec = "subrip",
                language = "eng",
                title = "Found online",
                external = true,
                textBased = true,
                url = "/api/files/${file.id}/subtitles/$index.vtt",
                added = true,
            )
            added += track
            Reply.json(SubtitleTrack.serializer(), track, code = 201)
        },
    )

    /**
     * GET's answer for [key]: a ceiling of the last 24 h, else 70 % of a
     * speed test of the last 7 days; on the server's own network, never a test.
     */
    private fun answer(key: String): PlaybackNetwork {
        val now = s.nowMs()
        val local = !network.remote
        network.ceilings[key]?.takeIf { now - it.second < DAY_MS }?.let { (kbps, at) ->
            return PlaybackNetwork(local = local, ceilingKbps = kbps, source = "playback", since = at)
        }
        if (local) return PlaybackNetwork(local = true)
        network.probes[key]?.takeIf { now - it.third < WEEK_MS }?.let { (kbps, complete, at) ->
            return if (complete) PlaybackNetwork() else PlaybackNetwork(ceilingKbps = (kbps * 0.7).toInt(), source = "probe", since = at)
        }
        return PlaybackNetwork(probe = true)
    }

    private fun key(caller: Caller, client: String) = "${caller.profile.id}|$client"

    companion object {
        const val DEFAULT_BYTES = 33_554_432L
        const val MIN_BYTES = 65_536L
        const val MAX_BYTES = 67_108_864L
        const val DAY_MS = 24 * 3_600_000L
        const val WEEK_MS = 7 * DAY_MS

        /** Added subtitles' indexes, clear of the file's streams and sidecars. */
        const val ADDED_FROM = 5_000

        /** The subtitle route's text for an added track. */
        fun cues(track: SubtitleTrack): String = "WEBVTT\n\n00:00:01.000 --> 00:00:04.000\nFound online (${track.language})\n\n"
    }
}
