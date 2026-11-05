package app.pley.android

import android.app.Activity
import android.app.Application
import android.content.pm.ActivityInfo
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle
import app.pley.android.common.AppLanguage
import app.pley.android.data.LanguageRepository
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/**
 * Keeps the app in the language it should be in (docs/ANDROID.md,
 * "Language"): the account's, else the server's, else the device's, as the
 * data side learns them, and never the device's while the server hasn't
 * said. A change shows at once. Android 13 and later remake the activities
 * themselves; below 13 the open ones are remade here, all but those that
 * take a change of language themselves (the players, which play on, and
 * show it the next time they open), as Android 13 does.
 */
internal class LanguageSync(private val app: Application) : Application.ActivityLifecycleCallbacks {
    /** The activities made and not yet destroyed: the main thread's only. */
    private val open = LinkedHashSet<Activity>()

    init {
        app.registerActivityLifecycleCallbacks(this)
    }

    /** Follows [language]'s sources for as long as the calling coroutine lasts. */
    suspend fun follow(language: LanguageRepository) {
        language.sources.collect { sources ->
            val wanted = sources.wanted(AppLanguage.deviceLocales(app)) ?: return@collect
            val changed = withContext(Dispatchers.Main.immediate) {
                AppLanguage.show(app, wanted).also { changed -> if (changed && Build.VERSION.SDK_INT < 33) remake() }
            }
            // The server writes some words itself (Home's rows): asked for again, in the new language.
            if (changed) language.shown()
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
