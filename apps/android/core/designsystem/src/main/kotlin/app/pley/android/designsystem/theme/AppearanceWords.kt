package app.pley.android.designsystem.theme

import androidx.annotation.StringRes
import app.pley.android.designsystem.R

/** The theme's name, the web's (account.theme.names). */
@get:StringRes
val PleyTheme.nameRes: Int
    get() = when (this) {
        PleyTheme.Midnight -> R.string.appearance_theme_midnight
        PleyTheme.Midday -> R.string.appearance_theme_midday
        PleyTheme.Twilight -> R.string.appearance_theme_twilight
        PleyTheme.Starlight -> R.string.appearance_theme_starlight
        PleyTheme.Moonlight -> R.string.appearance_theme_moonlight
        PleyTheme.Nightshade -> R.string.appearance_theme_nightshade
        PleyTheme.Greenlight -> R.string.appearance_theme_greenlight
        PleyTheme.Sunshine -> R.string.appearance_theme_sunshine
        PleyTheme.Blacklight -> R.string.appearance_theme_blacklight
    }

/** The theme's caption, the web's (account.theme.about). */
@get:StringRes
val PleyTheme.aboutRes: Int
    get() = when (this) {
        PleyTheme.Midnight -> R.string.appearance_about_midnight
        PleyTheme.Midday -> R.string.appearance_about_midday
        PleyTheme.Twilight -> R.string.appearance_about_twilight
        PleyTheme.Starlight -> R.string.appearance_about_starlight
        PleyTheme.Moonlight -> R.string.appearance_about_moonlight
        PleyTheme.Nightshade -> R.string.appearance_about_nightshade
        PleyTheme.Greenlight -> R.string.appearance_about_greenlight
        PleyTheme.Sunshine -> R.string.appearance_about_sunshine
        PleyTheme.Blacklight -> R.string.appearance_about_blacklight
    }

/** An accent's name (account.theme.accent.names); `null`, the theme's own, is Default. */
@StringRes
fun accentNameRes(accent: PleyAccent?): Int = when (accent) {
    null -> R.string.appearance_accent_own
    PleyAccent.Sky -> R.string.appearance_accent_sky
    PleyAccent.Violet -> R.string.appearance_accent_violet
    PleyAccent.Miami -> R.string.appearance_accent_miami
    PleyAccent.Gold -> R.string.appearance_accent_gold
    PleyAccent.Mint -> R.string.appearance_accent_mint
    PleyAccent.Amber -> R.string.appearance_accent_amber
}
