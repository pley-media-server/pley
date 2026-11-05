package app.pley.android.common

import androidx.annotation.StringRes
import java.util.Locale

/**
 * The languages Pley is in (docs/API.md, "Language"), English first, the
 * fallback. [code] is the ISO 639-1 code the server takes and answers;
 * [autonym] is the language's name in itself ("Français"), the same in
 * every language.
 */
enum class Language(val code: String, @StringRes val autonym: Int) {
    English("en", R.string.language_en),
    French("fr", R.string.language_fr),
    Spanish("es", R.string.language_es),
    ;

    companion object {
        /** The language Pley has for [tag] ("fr", "fr-CA", "es_ES"), `null` for any other. */
        fun of(tag: String?): Language? {
            val base = tag?.trim()?.split('-', '_')?.firstOrNull()?.lowercase() ?: return null
            return entries.firstOrNull { it.code == base }
        }

        /** The first of the device's languages ([device], the one it prefers first) that Pley has. */
        fun ofDevice(device: List<Locale>): Language? = device.firstNotNullOfOrNull { of(it.language) }

        /**
         * The language to show: the account's own, else the server's, else
         * the first of the device's that Pley has, else English. A code
         * Pley doesn't have counts as none.
         */
        fun pick(account: String?, server: String?, device: List<Locale>): Language =
            of(account) ?: of(server) ?: ofDevice(device) ?: English

        /**
         * The locale [language] is shown in: the device's own where the
         * device is in that language, so its region's ways stay (en-GB's
         * 24-hour clock, fr-CA's dates), else the language alone.
         */
        fun localeOf(language: Language, device: List<Locale>): Locale =
            device.firstOrNull { of(it.language) == language } ?: Locale.forLanguageTag(language.code)
    }
}
