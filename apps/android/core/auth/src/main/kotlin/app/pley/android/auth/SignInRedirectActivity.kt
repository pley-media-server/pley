package app.pley.android.auth

import android.app.Activity
import android.content.Context
import android.content.Intent
import android.os.Bundle
import app.pley.android.common.AppLanguage
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.MainScope
import kotlinx.coroutines.cancel
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/**
 * Where the browser comes back after signing in
 * (`app.pley.android:/oauth/callback`). Any app can open this address, so
 * the answer counts only when it carries the state of a waiting sign-in
 * and comes from that sign-in's server ([SignIn.handle]); the rest is
 * ignored. It shows nothing: it reads the answer, hands it over off the
 * main thread (the waiting sign-in is on disk, sealed by the Keystore),
 * brings back the activity that started the sign-in (closing the Custom
 * Tab above it), and ends.
 */
class SignInRedirectActivity : Activity() {
    private val main = MainScope()

    override fun attachBaseContext(newBase: Context) {
        super.attachBaseContext(AppLanguage.wrap(newBase))
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        // Recreated after process death: the answer was handled the first time.
        if (savedInstanceState == null) handle(intent) else finish()
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        handle(intent)
    }

    override fun onDestroy() {
        main.cancel()
        super.onDestroy()
    }

    private fun handle(intent: Intent?) {
        val callback = intent?.takeIf { it.action == Intent.ACTION_VIEW }?.dataString?.let(Callback::parse)
        if (callback == null) {
            finish()
            return
        }
        val app = applicationContext
        main.launch {
            val flow = withContext(Dispatchers.IO) { Auth.callback(app, callback) }
            flow?.returnTo?.let(::bringBack)
            finish()
        }
    }

    private fun bringBack(activity: String) {
        try {
            // Only the app's own activities: the class name was kept by the app itself.
            startActivity(
                Intent().setClassName(packageName, activity)
                    .addFlags(Intent.FLAG_ACTIVITY_CLEAR_TOP or Intent.FLAG_ACTIVITY_SINGLE_TOP),
            )
        } catch (_: android.content.ActivityNotFoundException) {
            // That screen is gone from the app: Auth.state still says how it went.
        }
    }
}
