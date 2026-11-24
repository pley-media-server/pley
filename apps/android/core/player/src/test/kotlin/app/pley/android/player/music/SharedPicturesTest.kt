package app.pley.android.player.music

import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.async
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import okhttp3.OkHttpClient
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** The session's covers and the car's, asked for together, fetched once. */
class SharedPicturesTest {
    private val cover = "http://127.0.0.1:7539/api/items/50/images/poster?v=1&w=512"
    private val jpeg = byteArrayOf(1, 2, 3)

    @Test
    fun aPictureAskedForAgainOnItsWayIsFetchedOnce() = runTest {
        val fetched = mutableListOf<String>()
        val answer = CompletableDeferred<ByteArray?>()
        val pictures = SharedPictures(backgroundScope) { _, url ->
            fetched += url
            answer.await()
        }
        val profile = OkHttpClient()

        // The session reads it ahead as the car asks for it; the car stops waiting, the session still gets it.
        val session = async { pictures.get(profile, cover) }
        val car = async { pictures.get(profile, cover) }
        runCurrent()
        assertEquals(listOf(cover), fetched)
        car.cancel()
        answer.complete(jpeg)
        assertArrayEquals(jpeg, session.await())
        assertTrue(car.isCancelled)

        // Once in, the next ask is a fetch of its own (the cache's, without the server).
        assertArrayEquals(jpeg, pictures.get(profile, cover))
        assertEquals(2, fetched.size)
    }

    @Test
    fun anotherProfilesAskIsItsOwn() = runTest {
        var fetches = 0
        val answer = CompletableDeferred<ByteArray?>()
        val pictures = SharedPictures(backgroundScope) { _, _ ->
            fetches++
            answer.await()
        }
        // Each profile's client has its own cache, and may not see the same pictures.
        val one = async { pictures.get(OkHttpClient(), cover) }
        val other = async { pictures.get(OkHttpClient(), cover) }
        runCurrent()
        assertEquals(2, fetches)
        answer.complete(null)
        one.await()
        other.await()
    }
}
