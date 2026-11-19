package app.pley.android.common

import java.util.Locale
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/** Which language Pley is shown in: the account's, else the device's (one picked on it, else the system's), else the server language, else English. */
class LanguageTest {
    private val german = listOf(Locale.GERMANY)

    @Test
    fun aTagCountsByItsLanguage() {
        assertEquals(Language.French, Language.of("fr"))
        assertEquals(Language.French, Language.of("fr-CA"))
        assertEquals(Language.Spanish, Language.of("es_ES"))
        assertEquals(Language.Spanish, Language.of(" ES-419 "))
        assertEquals(Language.English, Language.of("en-GB"))
        // Chinese is the one pair a region or script chooses, not just the first part of the tag.
        assertEquals(Language.SimplifiedChinese, Language.of("zh-CN"))
        assertEquals(Language.SimplifiedChinese, Language.of("zh-Hans"))
        assertEquals(Language.SimplifiedChinese, Language.of("zh"))
        assertEquals(Language.TraditionalChinese, Language.of("zh-TW"))
        assertEquals(Language.TraditionalChinese, Language.of("zh-Hant"))
        assertEquals(Language.TraditionalChinese, Language.of("zh-HK"))
        assertEquals(Language.TraditionalChinese, Language.of("zh-MO"))
        assertNull(Language.of("de"))
        assertNull(Language.of("français"))
        assertNull(Language.of(""))
        assertNull(Language.of(null))
    }

    @Test
    fun theAccountsComesFirstThenTheDevicesThenTheServersThenEnglish() {
        assertEquals(Language.Spanish, Language.pick("es", "fr", listOf(Locale.FRANCE)))
        // The device's English before the server's French; a German device's, the server's.
        assertEquals(Language.English, Language.pick(null, "fr", listOf(Locale.US)))
        assertEquals(Language.French, Language.pick(null, "fr", german))
        assertEquals(Language.Spanish, Language.pick(null, null, listOf(Locale.forLanguageTag("es-MX"))))
        assertEquals(Language.English, Language.pick(null, null, german))
        assertEquals(Language.English, Language.pick(null, null, emptyList()))
    }

    @Test
    fun aLanguagePickedOnTheDeviceComesAfterTheAccountsBeforeTheSystems() {
        assertEquals(Language.Spanish, Language.pick(null, "fr", listOf(Locale.FRANCE), picked = Language.Spanish))
        assertEquals(Language.Spanish, Language.pick(null, "fr", german, picked = Language.Spanish))
        assertEquals(Language.French, Language.pick("fr", "en", german, picked = Language.Spanish))
        // An account's language Pley doesn't have counts as none: the device's pick.
        assertEquals(Language.English, Language.pick("de", "fr", listOf(Locale.FRANCE), picked = Language.English))
    }

    @Test
    fun aCodePleyDoesntHaveCountsAsNone() {
        assertEquals(Language.French, Language.pick("de", "fr", german))
        assertEquals(Language.Spanish, Language.pick("de", "pt", listOf(Locale.GERMANY, Locale.forLanguageTag("es-ES"))))
        assertEquals(Language.English, Language.pick("", "xx", german))
    }

    @Test
    fun theDevicesFirstLanguagePleyHasIsTheDevices() {
        // A German device that reads French next: French, not English.
        assertEquals(Language.French, Language.ofDevice(listOf(Locale.GERMANY, Locale.forLanguageTag("fr-CA"), Locale.US)))
        assertEquals(Language.English, Language.ofDevice(listOf(Locale.GERMANY, Locale.UK, Locale.FRANCE)))
        assertEquals(Language.SimplifiedChinese, Language.ofDevice(listOf(Locale.GERMANY, Locale.CHINA)))
        assertEquals(Language.TraditionalChinese, Language.ofDevice(listOf(Locale.GERMANY, Locale.TAIWAN)))
        assertEquals(Language.TraditionalChinese, Language.ofDevice(listOf(Locale.GERMANY, Locale.forLanguageTag("zh-Hant"))))
        assertNull(Language.ofDevice(german))
    }

    @Test
    fun theDevicesRegionStaysWhereTheDeviceIsInThatLanguage() {
        val device = listOf(Locale.GERMANY, Locale.UK, Locale.forLanguageTag("fr-CA"), Locale.forLanguageTag("zh-TW"))
        // en-GB keeps its day before the month and its 24-hour clock.
        assertEquals(Locale.UK, Language.localeOf(Language.English, device))
        assertEquals(Locale.forLanguageTag("fr-CA"), Language.localeOf(Language.French, device))
        assertEquals(Locale.forLanguageTag("es"), Language.localeOf(Language.Spanish, device))
        // The two Chineses keep their own regions, and a zh-HK device keeps HK's ways.
        assertEquals(Locale.forLanguageTag("zh-TW"), Language.localeOf(Language.TraditionalChinese, device))
        assertEquals(Locale.forLanguageTag("zh-CN"), Language.localeOf(Language.SimplifiedChinese, device))
        assertEquals(
            Locale.forLanguageTag("zh-HK"),
            Language.localeOf(Language.TraditionalChinese, listOf(Locale.forLanguageTag("zh-HK"))),
        )
    }
}
