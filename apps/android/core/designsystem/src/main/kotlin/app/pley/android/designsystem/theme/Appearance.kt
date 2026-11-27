package app.pley.android.designsystem.theme

import android.content.Context
import android.content.SharedPreferences
import android.os.StrictMode
import androidx.annotation.VisibleForTesting
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.core.content.edit

/**
 * The theme and accent shown (docs/THEMES.md, "In every app"), picked on
 * Appearance and kept on this device for each profile, by its server and
 * user id (user ids repeat across servers), in the device's own preferences; nothing goes to the server. A profile with
 * no pick on this device shows Midnight in the theme's own accent, and so
 * does a stored id this app doesn't know.
 *
 * Outside a profile (choosing a server, signing in, "Who's watching?" and
 * its PIN), the shells draw Midnight in its own accent ([PleyPalette.Default])
 * whatever the profile's pick, and the profile's comes back once the app is
 * inside it again. The device keeps the palette its first frame takes: the
 * signed-in profile's pick while the app is inside it, else Midnight. It is
 * read once as the app starts ([attach]), so a start inside a profile shows
 * its pick from the first frame; the profile's own comes once the app knows
 * who is signed in ([showProfile]).
 *
 * Kept as Compose state: the shells read [palette], so a pick or a switch
 * of profile redraws every screen open, in every activity, in the same
 * frame.
 */
object Appearance {
    private const val PREFERENCES = "appearance"
    private const val THEME = "theme"
    private const val ACCENT = "accent"

    private var shown by mutableStateOf(PleyPalette.Default)

    /** The profile whose pick shows, once known: "<server>.<user id>". */
    @Volatile
    private var profile: String? = null

    /** Whether the app shows a screen outside a profile ([outside]). */
    @Volatile
    private var outsideNow = false

    /** The theme shown. */
    val theme: PleyTheme get() = shown.theme

    /** The accent shown; `null`: the theme's own. */
    val accent: PleyAccent? get() = shown.pick

    /** The profile's palette (the one the app starts in, before it knows the profile). Read in composition, it recomposes as it changes. */
    val palette: PleyPalette get() = shown

    /** The palette a shell draws a screen in: Midnight in its own accent outside a profile ([outside]), else [palette]. */
    fun paletteFor(outside: Boolean): PleyPalette = if (outside) PleyPalette.Default else shown

    /**
     * The shell shows a screen outside a profile, or one inside it again:
     * kept, on the main thread, so the next start's first frame is drawn
     * as this one is.
     */
    fun outside(context: Context, outside: Boolean) {
        if (outside == outsideNow) return
        outsideNow = outside
        preferences(context).edit { keepFirst() }
    }

    /** As the app starts, before anything is drawn: the pick this device showed last. */
    fun attach(context: Context) {
        // One small file, read on the main thread as the app starts: the first
        // screen is drawn in its colors, and nothing is drawn before it.
        val policy = StrictMode.allowThreadDiskReads()
        try {
            shown = read(preferences(context), suffix = "")
        } finally {
            StrictMode.setThreadPolicy(policy)
        }
    }

    /**
     * The signed-in profile is known, or another one signed in ([userId] on
     * [server], a stable name of the server the sign-in is kept by), on the
     * main thread: its pick on this device shows, Midnight and the theme's
     * own accent if it has none, and is the device's last shown.
     */
    fun showProfile(context: Context, server: String, userId: Long) {
        val key = "$server.$userId"
        if (profile == key) return
        profile = key
        val preferences = preferences(context)
        shown = read(preferences, suffix = ".$key")
        preferences.edit { keepFirst() }
    }

    /**
     * Signed out, or signed in while who is watching isn't known yet (a
     * switch to a profile new to this device, a sign-in after signing out),
     * on the main thread: Midnight in its own accent until [showProfile],
     * and nothing picked meanwhile is kept for the profile before.
     */
    fun noProfile() {
        profile = null
        shown = PleyPalette.Default
    }

    /** Shows [theme] from now on, on the main thread, and keeps it for the profile and as the last shown; the accent stays. */
    fun pick(context: Context, theme: PleyTheme) = show(context, PleyPalette.of(theme, shown.pick))

    /** Shows [accent] (`null`: the theme's own) over the theme from now on, and keeps it as [pick] does. */
    fun pickAccent(context: Context, accent: PleyAccent?) = show(context, PleyPalette.of(shown.theme, accent))

    private fun show(context: Context, palette: PleyPalette) {
        if (palette === shown) return
        shown = palette
        val profile = profile
        preferences(context).edit {
            if (profile != null) keep(palette, suffix = ".$profile")
            keepFirst()
        }
    }

    /** Tests: as a new process, before [attach]. */
    @VisibleForTesting
    internal fun forget() {
        profile = null
        outsideNow = false
        shown = PleyPalette.Default
    }

    private fun preferences(context: Context) = context.getSharedPreferences(PREFERENCES, Context.MODE_PRIVATE)

    /** The pick kept under [suffix] ("" for the first frame's, ".<server>.<user id>" for a profile's). */
    private fun read(preferences: SharedPreferences, suffix: String): PleyPalette =
        PleyPalette.of(PleyTheme.of(preferences.getString(THEME + suffix, null)), PleyAccent.of(preferences.getString(ACCENT + suffix, null)))

    /** What the next start's first frame shows: the profile's pick inside it, else Midnight. */
    private fun SharedPreferences.Editor.keepFirst() = keep(if (outsideNow) PleyPalette.Default else shown, suffix = "")

    private fun SharedPreferences.Editor.keep(palette: PleyPalette, suffix: String) {
        // Midnight and the theme's own accent are what nothing kept shows: nothing to keep.
        if (palette.theme == PleyTheme.Default) remove(THEME + suffix) else putString(THEME + suffix, palette.theme.id)
        val accent = palette.pick
        if (accent == null) remove(ACCENT + suffix) else putString(ACCENT + suffix, accent.id)
    }
}
