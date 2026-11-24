package app.pley.android.player.music.library

import android.content.ContentProvider
import android.content.ContentValues
import android.content.Context
import android.database.Cursor
import android.net.Uri
import android.os.ParcelFileDescriptor
import androidx.core.net.toUri
import app.pley.android.player.PlayerGraphOwner
import java.io.FileNotFoundException
import java.io.IOException
import java.util.concurrent.ArrayBlockingQueue
import java.util.concurrent.ExecutorService
import java.util.concurrent.RejectedExecutionException
import java.util.concurrent.ThreadPoolExecutor
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeoutOrNull

/**
 * The library's pictures for cars: Android Auto can't fetch them from the
 * server (Pley's pictures need the profile's token, which never leaves the
 * app), so it reads them from [MusicArtProvider] by these addresses. The
 * provider isn't exported: the music service grants reading it to Android
 * Auto and the Google app as they connect, and to the system's media
 * controls (MusicService); no other app.
 */
object MusicArt {
    fun authority(context: Context): String = "${context.packageName}.music.art"

    /**
     * The address of the server's picture at [path] ("/api/items/12/images/poster?v=…"); `null` for one it doesn't
     * serve. With [seed], the placeholder's hue goes with it ([standIn]): a track without a cover of its own is given
     * its album's, which the album may not have either.
     */
    fun uri(context: Context, path: String, seed: String? = null): Uri? {
        if (!servable(path)) return null
        val hue = seed?.let { "?$HUE=${PlaceholderCover.hue(it)}" }.orEmpty()
        val short = shortOf(path) ?: return Uri.Builder().scheme("content").authority(authority(context)).appendPath(PICTURE)
            .appendQueryParameter(PATH, path)
            .apply { seed?.let { appendQueryParameter(HUE, PlaceholderCover.hue(it).toString()) } }
            .build()
        return address(context, short + hue)
    }

    /**
     * [path] as the few segments of a short address ("i/12/poster/1700000000"), when it has one of the server's usual
     * forms; `null` otherwise, and its address carries it whole. Cars are sent their lists in one parcel that Media3
     * cuts at 256 KiB, and a row's address takes up much of a row: written out as a query, in its escaped form, it
     * took twice the room.
     */
    private fun shortOf(path: String): String? {
        ITEM_PICTURE.matchEntire(path)?.let { found ->
            val (of, id, kind, version) = found.destructured
            return listOf(if (of == "items") ITEM else COLLECTION, id, kind, version).filter { it.isNotEmpty() }.joinToString("/")
        }
        return CHANNEL_LOGO.matchEntire(path)?.let { found ->
            val (id, version) = found.destructured
            listOf(LOGO, id, version).filter { it.isNotEmpty() }.joinToString("/")
        }
    }

    /** An address of [segments], already made of nothing but digits, lowercase letters, "/", "?", "=" and "-". */
    private fun address(context: Context, segments: String): Uri = "content://${authority(context)}/$segments".toUri()

    /**
     * A cover for what has none, [seed] its title ([PlaceholderCover]): drawn
     * by the provider from the title's hue alone, the title not in the address.
     * The address names the drawing ([PlaceholderCover.DRAWING]): Android
     * Auto keeps each cover it has read by its address, and showed the
     * placeholders as first drawn after they were drawn brighter.
     */
    fun placeholderUri(context: Context, seed: String): Uri = placeholderOf(context, PlaceholderCover.hue(seed))

    /**
     * The address of a channel's logo on its plate ([LogoPlate]) from the server's logo at [path]
     * ("/api/live/channels/12/logo?v=0a1b2c3d" as "p/12/0a1b2c3d"); `null` for any other path. The letter names the
     * plate's drawing, and the server's version the logo: Android Auto keeps each picture by its address, and had
     * kept the logos unplated under theirs. It is as short as the logo's: a car's list of 500 channels is near the
     * 256 KiB a list is cut at.
     */
    fun logoUri(context: Context, path: String): Uri? {
        if (!servable(path)) return null
        val (id, version) = CHANNEL_LOGO.matchEntire(path)?.destructured ?: return null
        return address(context, listOf(PLATE, id, version).filter { it.isNotEmpty() }.joinToString("/"))
    }

    /** The server's logo a [logoUri] plates, and its channel; `null` for any other address. */
    internal fun logoOf(context: Context, uri: Uri): Pair<String, Long>? {
        val segments = segmentsOf(context, uri) ?: return null
        if (segments.size !in 2..3 || segments[0] != PLATE || !ID.matches(segments[1])) return null
        val version = segments.getOrNull(2)
        if (version != null && !LOGO_VERSION.matches(version)) return null
        val channel = segments[1].toLongOrNull()?.takeIf { it > 0 } ?: return null
        val path = "/api/live/channels/$channel/logo" + (version?.let { "?v=$it" }.orEmpty())
        return if (servable(path)) path to channel else null
    }

    /**
     * The picture of artist [artistId], who has none of their own on the
     * server: the cover of their newest album, looked for only when the car
     * asks for it ([MusicLibrary.artistPicture]), so a list of hundreds of
     * artists asks the server for nothing until its rows are shown. Else the
     * placeholder of [seed], the artist's name, whose hue alone is in the
     * address. Their count of [albums] goes in it too: a car keeps a cover
     * by its address, and an album added is a newer cover.
     */
    fun artistUri(context: Context, artistId: Long, albums: Int?, seed: String): Uri =
        address(context, listOfNotNull(ARTIST, artistId, albums?.takeIf { it >= 0 }).joinToString("/") + "?$HUE=${PlaceholderCover.hue(seed)}")

    /** The artist an [artistUri] names, a positive id of digits alone; `null` for any other address. */
    internal fun artistOf(context: Context, uri: Uri): Long? {
        val segments = segmentsOf(context, uri) ?: return null
        if (segments.size !in 2..3 || segments[0] != ARTIST || !segments.drop(1).all { ID.matches(it) }) return null
        return segments[1].toLongOrNull()?.takeIf { it > 0 }
    }

    /**
     * The hue a [placeholderUri] names; `null` for any other address, and for
     * one of another drawing than [PlaceholderCover.DRAWING], which isn't
     * drawn: the car then shows no cover, rather than this drawing under an
     * address it keeps for another, and asks for the lists' new addresses
     * the next time it reads them.
     */
    internal fun hueOf(context: Context, uri: Uri): Int? {
        val segments = segmentsOf(context, uri) ?: return null
        if (segments.size != 3 || segments[0] != PLACEHOLDER || segments[1] != PlaceholderCover.DRAWING.toString() || !ID.matches(segments[2])) return null
        return segments[2].toIntOrNull()?.takeIf { it in 0 until 360 }
    }

    /** The placeholder shown in place of the picture [uri] names when it can't be had; `null` when it has no hue. */
    internal fun standIn(context: Context, uri: Uri): Uri? {
        if (pathOf(context, uri) == null && artistOf(context, uri) == null) return null
        return uri.getQueryParameter(HUE)?.takeIf { ID.matches(it) }?.toIntOrNull()?.takeIf { it in 0 until 360 }?.let { placeholderOf(context, it) }
    }

    private fun placeholderOf(context: Context, hue: Int): Uri = address(context, "$PLACEHOLDER/${PlaceholderCover.DRAWING}/$hue")

    /** The server path [uri] names, when it is one of these addresses: a short one, or one carrying the path whole. */
    internal fun pathOf(context: Context, uri: Uri): String? {
        val segments = segmentsOf(context, uri) ?: return null
        val path = when {
            segments == listOf(PICTURE) -> uri.getQueryParameter(PATH)
            (segments.size == 2 || segments.size == 3) && segments[0] == LOGO -> {
                val version = segments.getOrNull(2)
                if (!ID.matches(segments[1]) || (version != null && !LOGO_VERSION.matches(version))) return null
                "/api/live/channels/${segments[1]}/logo" + (version?.let { "?v=$it" }.orEmpty())
            }
            (segments.size == 3 || segments.size == 4) && (segments[0] == ITEM || segments[0] == COLLECTION) -> {
                val (_, id, kind) = segments
                val version = segments.getOrNull(3)
                if (!ID.matches(id) || !KIND.matches(kind) || (version != null && !VERSION.matches(version))) return null
                val of = if (segments[0] == ITEM) "items" else "collections"
                "/api/$of/$id/images/$kind" + (version?.let { "?v=$it" }.orEmpty())
            }
            else -> null
        }
        return path?.takeIf(::servable)
    }

    /** [uri]'s path segments, when it is one of the provider's addresses. */
    private fun segmentsOf(context: Context, uri: Uri): List<String>? =
        if (uri.scheme != "content" || uri.authority != authority(context)) null else uri.pathSegments

    /** Only the server's item and collection pictures and Live TV's channel logos: no other route of the API goes through the provider. */
    internal fun servable(path: String): Boolean = path.length <= MAX_PATH && PICTURE_PATH.matches(path)

    // What each kind of address starts with: a letter, as the address is sent with every row of a car's list.
    private const val ITEM = "i"
    private const val COLLECTION = "c"
    private const val LOGO = "l"
    private const val ARTIST = "a"
    private const val PLACEHOLDER = "h"

    /** A channel's logo on a plate: another letter, not a number, whenever the plate is drawn otherwise. */
    private const val PLATE = "p"

    /** The address that carries a server path whole, for one of an unusual form. */
    private const val PICTURE = "picture"
    private const val PATH = "p"
    private const val HUE = "h"
    private const val MAX_PATH = 256
    private val PICTURE_PATH =
        Regex("""(/api/(items|collections)/\d{1,19}/images/[a-z]{1,16}|/api/live/channels/\d{1,19}/logo)(\?[A-Za-z0-9=&._-]{0,128})?""")
    private val ITEM_PICTURE = Regex("""/api/(items|collections)/(\d{1,19})/images/([a-z]{1,16})(?:\?v=(-?\d{1,19}))?""")
    private val CHANNEL_LOGO = Regex("""/api/live/channels/(\d{1,19})/logo(?:\?v=([0-9a-f]{1,16}))?""")
    private val ID = Regex("""\d{1,19}""")
    private val KIND = Regex("""[a-z]{1,16}""")
    private val VERSION = Regex("""-?\d{1,19}""")

    /** A logo's version: a few bytes of a hash, in hex, which change with its provider's picture (the server's logo_path). */
    private val LOGO_VERSION = Regex("""[0-9a-f]{1,16}""")
}

/**
 * Hands the car the pictures [MusicArt] names: fetched from the server
 * with the profile's token (JPEG, PNG or WebP, as the server serves them),
 * at most 2 MB each, in a pipe.
 */
class MusicArtProvider : ContentProvider() {
    override fun onCreate(): Boolean = true

    override fun openFile(uri: Uri, mode: String): ParcelFileDescriptor {
        val context = context ?: throw FileNotFoundException()
        if (mode != "r") throw FileNotFoundException("Read only")
        MusicArt.hueOf(context, uri)?.let { hue -> return pipe { PlaceholderCover.png(context, hue) } }
        val logo = MusicArt.logoOf(context, uri)
        val path = logo?.first ?: MusicArt.pathOf(context, uri)
        val artist = if (path == null) MusicArt.artistOf(context, uri) ?: throw FileNotFoundException() else null
        val owner = context.applicationContext as? PlayerGraphOwner ?: throw FileNotFoundException()
        if (logo != null) {
            // A logo that can't be had or read is a placeholder of the channel's hue, so its row lines up with the others.
            return pipe {
                val bytes = runBlocking { withTimeoutOrNull(FETCH_MS) { owner.playerGraph.musicLibrary?.picture(logo.first) } }
                bytes?.let(LogoPlate::png) ?: PlaceholderCover.png(context, (logo.second % 360).toInt())
            }
        }
        val standIn = MusicArt.standIn(context, uri)?.let { MusicArt.hueOf(context, it) }
        // One that can't be had in FETCH_MS is its placeholder, as the app shows it; without one, the pipe ends empty and
        // the car shows no cover.
        return pipe {
            runBlocking {
                withTimeoutOrNull(FETCH_MS) {
                    val library = owner.playerGraph.musicLibrary
                    if (artist != null) library?.artistPicture(artist) else path?.let { library?.picture(it) }
                }
            } ?: standIn?.let { PlaceholderCover.png(context, it) }
        }
    }

    /**
     * A pipe that goes back at once, [bytes] following, made on the provider's
     * own threads: a car's grid of covers asks for many at a time, and a binder
     * thread held while each one comes from the server holds up every other
     * call into the app.
     */
    private fun pipe(bytes: () -> ByteArray?): ParcelFileDescriptor {
        val (read, write) = ParcelFileDescriptor.createPipe()
        try {
            FETCHES.execute {
                ParcelFileDescriptor.AutoCloseOutputStream(write).use { out ->
                    try {
                        bytes()?.let(out::write)
                    } catch (_: IOException) {
                        // The car stopped reading.
                    }
                }
            }
        } catch (_: RejectedExecutionException) {
            // More waiting than any car's screen shows at once: this one ends empty now, and shows no cover.
            write.close()
        }
        return read
    }

    override fun getType(uri: Uri): String = "image/*"

    override fun query(uri: Uri, projection: Array<out String>?, selection: String?, selectionArgs: Array<out String>?, sortOrder: String?): Cursor? = null

    override fun insert(uri: Uri, values: ContentValues?): Uri? = null

    override fun delete(uri: Uri, selection: String?, selectionArgs: Array<out String>?): Int = 0

    override fun update(uri: Uri, values: ContentValues?, selection: String?, selectionArgs: Array<out String>?): Int = 0

    private companion object {
        const val FETCH_MS = 10_000L

        /**
         * The pictures fetched at once, and the most that wait their turn. Six, as the pictures' client takes at once
         * from a server on the slowest devices: a grid read before mostly comes from the cache, and a new one fills sooner.
         */
        val FETCHES: ExecutorService = ThreadPoolExecutor(6, 6, 0L, TimeUnit.MILLISECONDS, ArrayBlockingQueue(128))
    }
}
