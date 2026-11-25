package app.pley.android.player.music

import android.net.Uri
import androidx.media3.common.ParserException
import androidx.media3.common.PlaybackException
import androidx.media3.common.util.UnstableApi
import androidx.media3.datasource.DataSpec
import androidx.media3.datasource.HttpDataSource
import androidx.media3.exoplayer.ExoPlaybackException
import java.io.IOException
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/** Why a track didn't play, as the queue reads it and the log says it. */
@RunWith(RobolectricTestRunner::class)
@androidx.annotation.OptIn(UnstableApi::class)
class EngineMusicOutputTest {
    private val spec = DataSpec(Uri.parse("http://192.0.2.1:7539/api/stream/7?token=secret"))

    @Test
    fun aFailureIsLoggedByItsKindsAndStatusNeverItsAddress() {
        val http = HttpDataSource.InvalidResponseCodeException(500, "Server Error", IOException("boom"), emptyMap(), spec, ByteArray(0))
        val error = ExoPlaybackException.createForSource(IOException(http), PlaybackException.ERROR_CODE_IO_BAD_HTTP_STATUS)
        val line = EngineMusicOutput.describeFailure(error)
        assertEquals("ERROR_CODE_IO_BAD_HTTP_STATUS < IOException < HTTP 500 < IOException", line)
        assertFalse(line.contains("192.0.2.1") || line.contains("token"))
        assertEquals(OutputError.Other, EngineMusicOutput.kindOf(error))

        val parse = ExoPlaybackException.createForSource(
            ParserException.createForMalformedContainer("Invalid FLAC stream marker", null),
            PlaybackException.ERROR_CODE_PARSING_CONTAINER_MALFORMED,
        )
        // A parser's words, with Media3's flags after them.
        assertTrue(EngineMusicOutput.describeFailure(parse).startsWith("ERROR_CODE_PARSING_CONTAINER_MALFORMED < ParserException: Invalid FLAC stream marker"))

        // A playlist's line it quotes, address and all: the address is left out.
        val playlist = ExoPlaybackException.createForSource(
            ParserException.createForMalformedManifest("Couldn't match #EXT-X-KEY in https://192.0.2.1/api/stream/7/key?token=abc", null),
            PlaybackException.ERROR_CODE_PARSING_MANIFEST_MALFORMED,
        )
        val quoted = EngineMusicOutput.describeFailure(playlist)
        assertTrue(quoted, quoted.contains("Couldn't match #EXT-X-KEY in <address>"))
        assertFalse(quoted, quoted.contains("192.0.2.1") || quoted.contains("token"))
    }
}
