package app.pley.android.player.music.library

import android.content.ContentProvider
import android.content.ContentValues
import android.content.Context
import android.database.Cursor
import android.net.Uri
import android.os.ParcelFileDescriptor
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
        return Uri.Builder().scheme("content").authority(authority(context)).appendPath(PICTURE).appendQueryParameter(PATH, path)
            .apply { seed?.let { appendQueryParameter(HUE, PlaceholderCover.hue(it).toString()) } }
            .build()
    }

    /**
     * A cover for what has none, [seed] its title ([PlaceholderCover]): drawn
     * by the provider from the title's hue alone, the title not in the address.
     */
    fun placeholderUri(context: Context, seed: String): Uri = placeholderOf(context, PlaceholderCover.hue(seed))

    /**
     * The picture of artist [artistId], who has none of their own on the
     * server: the cover of their newest album, looked for only when the car
     * asks for it ([MusicLibrary.artistPicture]), so a list of hundreds of
     * artists asks the server for nothing until its rows are shown. Else the
     * placeholder of [seed], the artist's name, whose hue alone is in the
     * address.
     */
    fun artistUri(context: Context, artistId: Long, seed: String): Uri = Uri.Builder().scheme("content").authority(authority(context))
        .appendPath(ARTIST).appendPath(artistId.toString()).appendQueryParameter(HUE, PlaceholderCover.hue(seed).toString()).build()

    /** The artist an [artistUri] names, a positive id of digits alone; `null` for any other address. */
    internal fun artistOf(context: Context, uri: Uri): Long? {
        if (uri.scheme != "content" || uri.authority != authority(context)) return null
        val segments = uri.pathSegments
        if (segments.size != 2 || segments[0] != ARTIST) return null
        val id = segments[1]
        if (id.isEmpty() || id.length > 19 || id.any { it !in '0'..'9' }) return null
        return id.toLongOrNull()?.takeIf { it > 0 }
    }

    /** The hue a [placeholderUri] names; `null` for any other address. */
    internal fun hueOf(context: Context, uri: Uri): Int? {
        if (uri.scheme != "content" || uri.authority != authority(context) || uri.pathSegments != listOf(PLACEHOLDER)) return null
        return hueIn(uri)
    }

    /** The placeholder shown in place of the picture [uri] names when it can't be had; `null` when it has no hue. */
    internal fun standIn(context: Context, uri: Uri): Uri? {
        if (pathOf(context, uri) == null && artistOf(context, uri) == null) return null
        return hueIn(uri)?.let { placeholderOf(context, it) }
    }

    private fun placeholderOf(context: Context, hue: Int): Uri = Uri.Builder().scheme("content").authority(authority(context))
        .appendPath(PLACEHOLDER).appendQueryParameter(HUE, hue.toString()).build()

    private fun hueIn(uri: Uri): Int? = uri.getQueryParameter(HUE)?.toIntOrNull()?.takeIf { it in 0 until 360 }

    /** The server path [uri] names, when it is one of these addresses. */
    internal fun pathOf(context: Context, uri: Uri): String? {
        if (uri.scheme != "content" || uri.authority != authority(context) || uri.pathSegments != listOf(PICTURE)) return null
        return uri.getQueryParameter(PATH)?.takeIf(::servable)
    }

    /** Only the server's item and collection pictures and Live TV's channel logos: no other route of the API goes through the provider. */
    internal fun servable(path: String): Boolean = path.length <= MAX_PATH && PICTURE_PATH.matches(path)

    private const val PICTURE = "picture"
    private const val PATH = "p"
    private const val PLACEHOLDER = "placeholder"
    private const val ARTIST = "artist"
    private const val HUE = "h"
    private const val MAX_PATH = 256
    private val PICTURE_PATH =
        Regex("""(/api/(items|collections)/\d{1,19}/images/[a-z]{1,16}|/api/live/channels/\d{1,19}/logo)(\?[A-Za-z0-9=&._-]{0,128})?""")
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
        val path = MusicArt.pathOf(context, uri)
        val artist = if (path == null) MusicArt.artistOf(context, uri) ?: throw FileNotFoundException() else null
        val owner = context.applicationContext as? PlayerGraphOwner ?: throw FileNotFoundException()
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
