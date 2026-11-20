package app.pley.android.player.music.library

import android.graphics.Bitmap
import android.net.Uri
import android.os.Looper
import androidx.media3.common.util.BitmapLoader
import com.google.common.util.concurrent.Futures
import com.google.common.util.concurrent.ListenableFuture
import java.util.concurrent.atomic.AtomicInteger
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf

/**
 * The session's covers: one kept is there at once, so the session says a
 * track and its cover together, and the covers around the track playing
 * are read ahead.
 */
@RunWith(RobolectricTestRunner::class)
class SessionArtTest {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)

    /** The server paths asked for, in order, each answered when its gate opens. */
    private val asked = mutableListOf<String>()
    private val gates = HashMap<String, CompletableDeferred<ByteArray?>>()
    private val elsewhere = mutableListOf<Uri>()
    private val other = object : BitmapLoader {
        override fun supportsMimeType(mimeType: String) = true

        override fun decodeBitmap(data: ByteArray): ListenableFuture<Bitmap> = Futures.immediateFuture(bitmap())

        override fun loadBitmap(uri: Uri): ListenableFuture<Bitmap> {
            elsewhere += uri
            return Futures.immediateFuture(bitmap())
        }
    }
    private val art = SessionArt(
        scope,
        pathOf = { uri -> uri.getQueryParameter("p").takeIf { uri.authority == "pley.art" } },
        picture = { path ->
            asked += path
            gates.getOrPut(path) { CompletableDeferred() }.await()
        },
        other = other,
        decode = { bitmap().also { decoded.incrementAndGet() } },
    )

    /** The decodings done so far, on another thread. */
    private val decoded = AtomicInteger()

    @After
    fun tearDown() = scope.cancel()

    private fun bitmap(): Bitmap = Bitmap.createBitmap(4, 4, Bitmap.Config.ARGB_8888)

    private fun cover(album: Long): Uri = Uri.parse("content://pley.art/picture?p=/api/items/$album/images/poster")

    /**
     * The server answers [album]'s cover, and what waits on it runs: its decoding, on another thread, is waited
     * for (2 s at most, a slow machine's), then what it hands back to the main thread.
     */
    private fun arrives(album: Long, bytes: ByteArray? = byteArrayOf(1)) {
        val before = decoded.get()
        gates.getOrPut("/api/items/$album/images/poster") { CompletableDeferred() }.complete(bytes)
        val until = System.currentTimeMillis() + 2_000
        do {
            shadowOf(Looper.getMainLooper()).idle()
            Thread.sleep(5)
        } while (bytes != null && decoded.get() == before && System.currentTimeMillis() < until)
        repeat(3) {
            Thread.sleep(5)
            shadowOf(Looper.getMainLooper()).idle()
        }
    }

    @Test
    fun aCoverReadOnceIsThereAtOnceTheNextTime() {
        val first = art.loadBitmap(cover(50))
        assertFalse(first.isDone)
        // Asked again while it comes (the notification and the session both ask): one request.
        assertSame(first, art.loadBitmap(cover(50)))
        arrives(50)
        assertTrue(first.isDone)

        // The next track of the album: its cover comes with it.
        val again = art.loadBitmap(cover(50))
        assertTrue(again.isDone)
        assertSame(first.get(), again.get())
        assertEquals(listOf("/api/items/50/images/poster"), asked)
    }

    @Test
    fun theCoversAroundTheTrackPlayingAreReadAheadOfThem() {
        art.warm(listOf(cover(50), cover(51), null, Uri.parse("https://elsewhere.example/cover.jpg")))
        assertEquals(listOf("/api/items/50/images/poster", "/api/items/51/images/poster"), asked)
        // Only the library's own: another address isn't fetched for a track that may never play.
        assertTrue(elsewhere.isEmpty())
        arrives(51)
        assertTrue(art.loadBitmap(cover(51)).isDone)
    }

    @Test
    fun aCoverThatCantBeHadFailsAndIsAskedForAgain() {
        val first = art.loadBitmap(cover(50))
        arrives(50, bytes = null)
        assertTrue(first.isDone)
        assertTrue(runCatching { first.get() }.isFailure)

        gates.clear()
        val again = art.loadBitmap(cover(50))
        assertFalse(again.isDone)
        assertEquals(2, asked.size)
    }

    @Test
    fun anotherProfilesCoversAreForgottenAndOtherAddressesGoToTheSessionsOwnLoader() {
        art.loadBitmap(cover(50))
        arrives(50)
        art.clear()
        gates.clear()
        assertFalse(art.loadBitmap(cover(50)).isDone)

        // A cover still on its way for the profile before is neither kept nor shown.
        val before = art.loadBitmap(cover(51))
        art.clear()
        arrives(51)
        assertTrue(before.isDone)
        assertTrue(runCatching { before.get() }.isFailure)
        gates.remove("/api/items/51/images/poster")
        assertFalse(art.loadBitmap(cover(51)).isDone)

        val icon = Uri.parse("android.resource://app.pley/drawable/music_library_recent")
        assertTrue(art.loadBitmap(icon).isDone)
        assertEquals(listOf(icon), elsewhere)
    }
}
