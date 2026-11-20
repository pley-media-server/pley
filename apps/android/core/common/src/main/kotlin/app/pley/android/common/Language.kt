package app.pley.android.common

import androidx.annotation.StringRes
import java.util.Locale

/**
 * The languages Pley is in (docs/API.md, "Language"), English first, the
 * fallback. [code] is the language code the server takes and answers;
 * [autonym] is the language's name in itself ("Français"), the same in
 * every language. [short] is the mark on the globe before signing in ("FR").
 */
enum class Language(val code: String, @StringRes val autonym: Int, val short: String) {
    English("en", R.string.language_en, "EN"),
    French("fr", R.string.language_fr, "FR"),
    Spanish("es", R.string.language_es, "ES"),
    SimplifiedChinese("zh-CN", R.string.language_zh_hans, "简"),
    TraditionalChinese("zh-TW", R.string.language_zh_hant, "繁"),
    ;

    companion object {
        /**
         * The language Pley has for [tag] ("fr", "fr-CA", "es_ES", "zh-CN"),
         * `null` for any other. The two Chinese codes are the server's own;
         * a device may name Chinese by its script or a region instead.
         */
        fun of(tag: String?): Language? {
            val parts = tag?.trim()?.lowercase()?.split('-', '_') ?: return null
            // Chinese is two languages: Traditional by its script or a region that writes it.
            if (parts[0] == "zh") {
                return if (parts.any { it in listOf("hant", "tw", "hk", "mo") }) TraditionalChinese else SimplifiedChinese
            }
            return entries.firstOrNull { it.code == parts[0] }
        }

        /** The first of the device's languages ([device], the one it prefers first) that Pley has. */
        fun ofDevice(device: List<Locale>): Language? = device.firstNotNullOfOrNull { of(it.toLanguageTag()) }

        /**
         * The language to show: the account's own, else this device's (the
         * one [picked] on it before signing in, else the first of the
         * system's that Pley has), else the server language, else English.
         * A code Pley doesn't have counts as none.
         */
        fun pick(account: String?, server: String?, device: List<Locale>, picked: Language? = null): Language =
            of(account) ?: picked ?: ofDevice(device) ?: of(server) ?: English

        /**
         * The locale [language] is shown in: the device's own where the
         * device is in that language, so its region's ways stay (en-GB's
         * 24-hour clock, fr-CA's dates, zh-TW's numbers), else the
         * language's own, which keeps zh-CN and zh-TW apart.
         */
        fun localeOf(language: Language, device: List<Locale>): Locale =
            device.firstOrNull { of(it.toLanguageTag()) == language } ?: Locale.forLanguageTag(language.code)
    }
}
