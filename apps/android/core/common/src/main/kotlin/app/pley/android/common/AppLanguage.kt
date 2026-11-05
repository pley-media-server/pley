package app.pley.android.common

import android.app.LocaleManager
import android.content.Context
import android.content.res.Configuration
import android.content.res.Resources
import android.os.Build
import android.os.LocaleList
import android.os.StrictMode
import androidx.annotation.RequiresApi
import androidx.core.content.edit
import app.pley.android.model.format.FormatWords
import java.util.Locale
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

/**
 * The language the app is shown in, one of [Language], and the locale it
 * is shown in ([Language.localeOf]: the device's region where the device
 * is in that language). docs/ANDROID.md, "Language", says which one.
 *
 * Android 13 and later keep it as the app's own language
 * (LocaleManager.applicationLocales): the system applies it to the whole
 * app before its first screen, and remakes the activities when it changes.
 * Below 13 the app remembers it itself, and each of its contexts is made
 * in it ([wrap], from attachBaseContext); when it changes, the app remakes
 * its activities ([shown]).
 *
 * The device's languages ([deviceLocales]) are always the system's, never
 * the app's own.
 */
object AppLanguage {
    private const val PREFERENCES = "language"
    private const val KEY = "language"

    private val _shown = MutableStateFlow(Language.English)

    /** The language shown. */
    val shown: StateFlow<Language> = _shown.asStateFlow()

    /** The locale the language is shown in: the one dates and numbers are written in. */
    @Volatile
    var locale: Locale = Locale.ENGLISH
        private set

    @Volatile
    private var app: Context? = null

    /** Below Android 13: the application's resources in the language shown, and that language's locale. */
    @Volatile
    private var words: Pair<Locale, Resources>? = null

    /** The format functions' words in the locale shown, and that locale. */
    @Volatile
    private var format: Pair<Locale, FormatWords>? = null

    /**
     * As the app starts, before anything is drawn (the application's
     * attachBaseContext): the language shown last, else the device's;
     * [base] made in it.
     */
    fun attach(base: Context): Context {
        val device = deviceLocales(base)
        val last = if (Build.VERSION.SDK_INT >= 33) {
            base.getSystemService(LocaleManager::class.java)?.applicationLocales?.takeIf { !it.isEmpty }?.get(0)?.language.let(Language::of)
        } else {
            remembered(base)
        }
        val language = last ?: Language.ofDevice(device) ?: Language.English
        set(language, Language.localeOf(language, device))
        return wrap(base)
    }

    /** The application is made (its onCreate): Android 13 and later keep the language started in. */
    fun started(context: Context) {
        app = context.applicationContext
        if (Build.VERSION.SDK_INT >= 33) keep(context, locale)
    }

    /**
     * Shows the app in [language] from now on, on the main thread; whether
     * that changed anything (the language shown already changes nothing).
     */
    fun show(context: Context, language: Language): Boolean {
        val locale = Language.localeOf(language, deviceLocales(context))
        if (language == _shown.value && locale == this.locale) return false
        set(language, locale)
        if (Build.VERSION.SDK_INT >= 33) keep(context, locale) else remember(context, language)
        return true
    }

    /** The configuration changed (the application's onConfigurationChanged): below Android 13, Android set the device's locale as the default again. */
    fun configurationChanged() {
        if (Build.VERSION.SDK_INT < 33) Locale.setDefault(locale)
    }

    /**
     * [base] in the language shown, for an activity's or a service's
     * attachBaseContext. Android 13 and later do it themselves.
     */
    fun wrap(base: Context): Context {
        if (Build.VERSION.SDK_INT >= 33) return base
        // Android sets the default locale again as the configuration changes.
        Locale.setDefault(locale)
        return base.createConfigurationContext(only(locale))
    }

    /**
     * The app's resources in the language shown, for words made where
     * nothing is drawn and no activity or service is at hand (the player's
     * messages). Below Android 13 the application's own stay in the
     * language the app started in.
     */
    fun resources(context: Context): Resources {
        val app = context.applicationContext
        if (Build.VERSION.SDK_INT >= 33) return app.resources
        val locale = locale
        words?.let { (at, resources) -> if (at == locale) return resources }
        return app.createConfigurationContext(only(locale)).resources.also { words = locale to it }
    }

    /**
     * The format functions' words ("26 min left", "3 hours ago", a track's
     * language) in the locale shown, for a state made where nothing is
     * drawn that is made again as [shown] changes: a ViewModel's that
     * combines it. Made for [locale] itself, so they are in the new
     * language as soon as [shown] says it, before the system has applied
     * it. Before the app is made (unit tests), English.
     */
    fun formatWords(): FormatWords {
        val app = app ?: return FormatWords.English
        val locale = locale
        format?.let { (at, words) -> if (at == locale) return words }
        return formatWords(app.createConfigurationContext(only(locale)).resources).also { format = locale to it }
    }

    /**
     * [text] in the language shown, for what can only be a String where
     * nothing is drawn: an error's message, read by logs and by code that
     * still words errors from it (describe). Screens and ViewModels hold
     * the UiText. Before the app is made (unit tests), the UiText as code.
     */
    fun wordsOf(text: UiText): String = app?.let { text.resolve(resources(it)) } ?: text.toString()

    /** The device's languages, the one it prefers first: the system's, never the app's own. */
    fun deviceLocales(context: Context): List<Locale> {
        val locales = if (Build.VERSION.SDK_INT >= 33) {
            context.getSystemService(LocaleManager::class.java)?.systemLocales ?: Resources.getSystem().configuration.locales
        } else {
            Resources.getSystem().configuration.locales
        }
        return List(locales.size()) { locales[it] }
    }

    private fun set(language: Language, locale: Locale) {
        this.locale = locale
        Locale.setDefault(locale)
        _shown.value = language
    }

    /** Android 13 and later: the app's language, which the system keeps, applies and remembers. */
    @RequiresApi(33)
    private fun keep(context: Context, locale: Locale) {
        val manager = context.getSystemService(LocaleManager::class.java) ?: return
        if (manager.applicationLocales.takeIf { !it.isEmpty }?.get(0) != locale) manager.applicationLocales = LocaleList(locale)
    }

    /** A configuration that changes the locale alone, applied over the system's. */
    private fun only(locale: Locale): Configuration = Configuration().apply {
        setLocales(LocaleList(locale))
        setLayoutDirection(locale)
    }

    private fun remembered(context: Context): Language? {
        // One small file, read on the main thread as the app starts: the first
        // screen is made in what it says, and nothing is drawn before it.
        val policy = StrictMode.allowThreadDiskWrites()
        return try {
            Language.of(context.getSharedPreferences(PREFERENCES, Context.MODE_PRIVATE).getString(KEY, null))
        } finally {
            StrictMode.setThreadPolicy(policy)
        }
    }

    private fun remember(context: Context, language: Language) {
        context.getSharedPreferences(PREFERENCES, Context.MODE_PRIVATE).edit { putString(KEY, language.code) }
    }
}
