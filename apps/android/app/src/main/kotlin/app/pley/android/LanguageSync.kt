package app.pley.android

import android.app.Activity
import android.app.Application
import android.app.LocaleManager
import android.content.pm.ActivityInfo
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle
import app.pley.android.common.AppLanguage
import app.pley.android.common.Language
import app.pley.android.data.LanguageRepository
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/**
 * Keeps the app in the language it should be in (docs/ANDROID.md,
 * "Language"): the account's, else the device's, else the server
 * language, as the data side learns them, and never another while the
 * server hasn't said. A change shows at once. Android 13 and later remake
 * the activities themselves; below 13 the open ones are remade here, all
 * but those that take a change of language themselves (the players, which
 * play on, and show it the next time they open), as Android 13 does.
 */
internal class LanguageSync(private val app: Application) : Application.ActivityLifecycleCallbacks {
    /** The activities made and not yet destroyed: the main thread's only. */
    private val open = LinkedHashSet<Activity>()

    /** What [follow] follows, which a language picked in the system's settings is chosen with. */
    @Volatile
    private var followed: LanguageRepository? = null

    private val picks = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)

    init {
        app.registerActivityLifecycleCallbacks(this)
    }

    /** Follows [language]'s sources for as long as the calling coroutine lasts. */
    suspend fun follow(language: LanguageRepository) {
        followed = language
        language.sources.collect { sources ->
            val wanted = sources.wanted(AppLanguage.deviceLocales(app)) ?: return@collect
            val changed = withContext(Dispatchers.Main.immediate) {
                AppLanguage.show(app, wanted).also { changed -> if (changed && Build.VERSION.SDK_INT < 33) remake() }
            }
            // The server writes some words itself (Home's rows): asked for again, in the new language.
            if (changed) language.shown()
        }
    }

    /**
     * The configuration changed (the application's onConfigurationChanged),
     * on the main thread. Android 13 and later list Pley's languages in the
     * system's settings (Apps → Pley → Language): one picked there is chosen
     * as the Language row chooses it, the account's on every device, and its
     * "System default" is Automatic. Signed out, or refused by the server,
     * the language the app goes by comes back.
     */
    fun configurationChanged() {
        if (Build.VERSION.SDK_INT < 33) return
        val locales = app.getSystemService(LocaleManager::class.java)?.applicationLocales ?: return
        val picked = if (locales.isEmpty) null else Language.of(locales[0].language) ?: return
        val language = picked ?: Language.ofDevice(AppLanguage.deviceLocales(app)) ?: Language.English
        // The app's own setting (AppLanguage.show) is heard here too: the language shown already.
        if (picked != null && language == AppLanguage.shown.value) return
        val before = AppLanguage.shown.value
        val repository = followed
        if (AppLanguage.show(app, language)) repository?.shown()
        picks.launch {
            try {
                checkNotNull(repository) { "no server" }.choose(picked?.code)
            } catch (e: CancellationException) {
                throw e
            } catch (_: Exception) {
                if (AppLanguage.show(app, before)) repository?.shown()
            }
        }
    }

    private fun remake() {
        open.toList().filterNot { it.isFinishing || takesLanguage(it) }.forEach(Activity::recreate)
    }

    /** Whether [activity] takes a change of language itself (`locale` in its manifest's configChanges). */
    private fun takesLanguage(activity: Activity): Boolean = try {
        val info = app.packageManager.getActivityInfo(activity.componentName, 0)
        info.configChanges and ActivityInfo.CONFIG_LOCALE != 0
    } catch (_: PackageManager.NameNotFoundException) {
        false
    }

    override fun onActivityCreated(activity: Activity, savedInstanceState: Bundle?) {
        open += activity
    }

    override fun onActivityDestroyed(activity: Activity) {
        open -= activity
    }

    override fun onActivityStarted(activity: Activity) = Unit

    override fun onActivityResumed(activity: Activity) = Unit

    override fun onActivityPaused(activity: Activity) = Unit

    override fun onActivityStopped(activity: Activity) = Unit

    override fun onActivitySaveInstanceState(activity: Activity, outState: Bundle) = Unit
}
