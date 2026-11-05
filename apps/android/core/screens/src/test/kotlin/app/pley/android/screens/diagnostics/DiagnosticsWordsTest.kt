package app.pley.android.screens.diagnostics

import android.content.Context
import android.content.res.Configuration
import android.os.LocaleList
import androidx.test.core.app.ApplicationProvider
import app.pley.android.common.DeviceProfile
import app.pley.android.common.FormFactor
import app.pley.android.common.Tier
import app.pley.android.common.resolve
import java.util.Locale
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/** What Diagnostics says of the device: in English as it always was, and in French and Spanish. */
@RunWith(RobolectricTestRunner::class)
class DiagnosticsWordsTest {
    private val english = ApplicationProvider.getApplicationContext<Context>()

    private fun inLanguage(tag: String): Context =
        english.createConfigurationContext(Configuration().apply { setLocales(LocaleList(Locale.forLanguageTag(tag))) })

    private val phone = DeviceProfile(FormFactor.Phone, fireTv = false, tier = Tier.T2, model = "Pixel 7a", totalRamBytes = 8_000_000_000, lowRam = false)
    private val fireTv = DeviceProfile(FormFactor.Tv, fireTv = true, tier = Tier.T0, model = "AFTSS", totalRamBytes = 2_000_000_000, lowRam = true)
    private val tv = DeviceProfile(FormFactor.Tv, fireTv = false, tier = Tier.T1, model = "Google TV Streamer", totalRamBytes = 2_400_000_000, lowRam = false)

    private fun said(profile: DeviceProfile, context: Context): List<Pair<String, String>> =
        DiagnosticsViewModel.factsOf(profile).map { it.label.resolve(context) to it.value.resolve(context) }

    @Test
    fun englishIsWhatItAlwaysSaid() {
        for (profile in listOf(phone, fireTv, tv)) {
            assertEquals(DiagnosticsViewModel.deviceFacts(profile, Locale.ENGLISH).map { it.label to it.value }, said(profile, english))
        }
        assertEquals(
            listOf(
                "Model" to "AFTSS",
                "Kind" to "Fire TV",
                "Memory" to "2.0 GB (low-memory device)",
                "Memory tier" to "T0",
                "Player buffer" to "15-30 s ahead, 64 MiB",
            ),
            said(fireTv, english),
        )
    }

    @Test
    fun frenchAndSpanishSayTheSame() {
        assertEquals(
            listOf(
                "Modèle" to "Pixel 7a",
                "Type" to "Téléphone",
                "Mémoire" to "8,0\u00a0Go",
                "Niveau de mémoire" to "T2",
                "Tampon du lecteur" to "15-50\u00a0s d’avance, 10\u00a0s en arrière",
            ),
            said(phone, inLanguage("fr")),
        )
        assertEquals("2,4\u00a0GB", said(tv, inLanguage("es-ES"))[2].second)
        assertEquals("50-50\u00a0s por delante, 96\u00a0MiB", said(tv, inLanguage("es"))[4].second)
    }
}
