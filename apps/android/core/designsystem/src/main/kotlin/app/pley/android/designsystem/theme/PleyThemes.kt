package app.pley.android.designsystem.theme

/**
 * Pley's nine themes (docs/THEMES.md, "The themes"), in the picker's order:
 * the ids and order of packages/themes/themes.json. [light] is the theme's
 * kind, which of an accent's two sets it takes.
 */
enum class PleyTheme(val id: String, val light: Boolean) {
    Midnight("midnight", light = false),
    Midday("midday", light = true),
    Twilight("twilight", light = false),
    Starlight("starlight", light = true),
    Moonlight("moonlight", light = false),
    Nightshade("nightshade", light = false),
    Greenlight("greenlight", light = true),
    Sunshine("sunshine", light = true),
    Blacklight("blacklight", light = false),
    ;

    /** The dark theme a light theme's players draw in, its "twin"; a dark theme is its own. */
    val twin: PleyTheme
        get() = when (this) {
            Midday, Sunshine -> Midnight
            Starlight -> Moonlight
            Greenlight -> Nightshade
            else -> this
        }

    companion object {
        /** Pley's own, for a device that picked none. */
        val Default = Midnight

        /** The theme of [id]; Midnight for none, or one this app doesn't know. */
        fun of(id: String?): PleyTheme = entries.firstOrNull { it.id == id } ?: Default
    }
}

/**
 * The accents picked over a theme's own (docs/THEMES.md, "Accents"), in the
 * picker's order after Default (`null`, the theme's own).
 */
enum class PleyAccent(val id: String) {
    Sky("sky"),
    Violet("violet"),
    Miami("miami"),
    Gold("gold"),
    Mint("mint"),
    Amber("amber"),
    ;

    companion object {
        /** The accent of [id]; `null` (the theme's own) for none, or one this app doesn't know. */
        fun of(id: String?): PleyAccent? = entries.firstOrNull { it.id == id }
    }
}

/** The colors a theme names (docs/THEMES.md, "Tokens"), by their names in themes.json. */
enum class Token(val id: String) {
    Bg("bg"),
    Surface("surface"),
    Surface2("surface-2"),
    Surface3("surface-3"),
    Border("border"),
    BorderStrong("border-strong"),
    Fg("fg"),
    Muted("muted"),
    Subtle("subtle"),
    Accent("accent"),
    AccentHover("accent-hover"),
    AccentFg("accent-fg"),
    Success("success"),
    Danger("danger"),
    DangerFg("danger-fg"),
    Warning("warning"),
    Info("info"),
    Shade("shade"),
    BrandLight("brand-light"),
    Brand("brand"),
    BrandDeep("brand-deep"),
    BrandFar("brand-far"),
    BrandTile("brand-tile"),
}
