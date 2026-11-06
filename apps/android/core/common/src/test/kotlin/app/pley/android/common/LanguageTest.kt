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
        assertNull(Language.ofDevice(german))
    }

    @Test
    fun theDevicesRegionStaysWhereTheDeviceIsInThatLanguage() {
        val device = listOf(Locale.GERMANY, Locale.UK, Locale.forLanguageTag("fr-CA"))
        // en-GB keeps its day before the month and its 24-hour clock.
        assertEquals(Locale.UK, Language.localeOf(Language.English, device))
        assertEquals(Locale.forLanguageTag("fr-CA"), Language.localeOf(Language.French, device))
        assertEquals(Locale.forLanguageTag("es"), Language.localeOf(Language.Spanish, device))
    }
}
