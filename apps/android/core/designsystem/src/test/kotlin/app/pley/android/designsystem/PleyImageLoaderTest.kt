package app.pley.android.designsystem

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class PleyImageLoaderTest {
    @Test
    fun hardwareBitmapsFromAndroid10On() {
        // Android 8 and 9 make each hardware bitmap on the RenderThread, between frames.
        assertFalse(PleyImageLoader.hardwareBitmaps(26))
        assertFalse(PleyImageLoader.hardwareBitmaps(28))
        assertTrue(PleyImageLoader.hardwareBitmaps(29))
        assertTrue(PleyImageLoader.hardwareBitmaps(36))
    }
}
