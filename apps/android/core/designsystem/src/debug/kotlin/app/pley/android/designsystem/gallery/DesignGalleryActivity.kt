package app.pley.android.designsystem.gallery

import android.graphics.drawable.ColorDrawable
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.graphics.toArgb
import app.pley.android.common.DeviceProfile
import app.pley.android.common.Tier
import app.pley.android.designsystem.PleyImageLoader
import app.pley.android.designsystem.image.LocalArtworkImageLoader
import app.pley.android.designsystem.theme.LocalPleyPalette
import app.pley.android.designsystem.theme.PleyAccent
import app.pley.android.designsystem.theme.PleyPalette
import app.pley.android.designsystem.theme.PleyTheme
import okhttp3.OkHttpClient

/**
 * Debug builds only: every token, icon and tile of :core:designsystem on
 * one scrolling screen, the TV's on a TV and the phone's elsewhere, with
 * pictures drawn on the device. Pictures go through the real loader
 * ([PleyImageLoader]: tiers, caches, buckets, fallbacks, fades), so what
 * shows is what screens will get.
 *
 * ```
 * adb shell am start -n app.pley.android/app.pley.android.designsystem.gallery.DesignGalleryActivity
 * ```
 *
 * Extras: `--es form tv|phone` picks the interface; `--es tier T0|T1|T2`
 * overrides the device's tier (T0 drops the glow and the fades); `--es
 * theme sunshine` and `--es accent gold` draw it in a theme and an accent
 * (Midnight in its own by default).
 */
class DesignGalleryActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        val device = DeviceProfile.of(this)
        val tier = intent.getStringExtra("tier")?.let { runCatching { Tier.valueOf(it) }.getOrNull() } ?: device.tier
        val tv = intent.getStringExtra("form")?.let { it == "tv" } ?: device.isTv
        val loader = PleyImageLoader.build(this, { OkHttpClient() }, device.copy(tier = tier)) {
            add(GallerySceneFetcher.Factory())
        }
        val palette = PleyPalette.of(PleyTheme.of(intent.getStringExtra("theme")), PleyAccent.of(intent.getStringExtra("accent")))
        window.setBackgroundDrawable(ColorDrawable(palette.bg.toArgb()))
        setContent {
            CompositionLocalProvider(LocalArtworkImageLoader provides loader, LocalPleyPalette provides palette) {
                if (tv) TvGallery(tier) else PhoneGallery()
            }
        }
    }
}
