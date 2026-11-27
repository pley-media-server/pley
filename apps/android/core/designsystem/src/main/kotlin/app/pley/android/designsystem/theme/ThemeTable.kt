package app.pley.android.designsystem.theme

/**
 * Each theme's colors and the tokens an accent replaces in it, and each
 * accent's colors for the dark themes and the light ones: a copy of
 * packages/themes/themes.json, where every app reads them from, which
 * ThemeTableTest holds this file to. Colors are ARGB; a light theme's
 * `shade` is translucent, as the file's `rgb(… / alpha)`.
 */
internal object ThemeTable {
    /** What a theme follows of an accent: its colors and the logo's, and the states an accent may move. */
    private val ACCENT_LOGO_STATES = setOf(
        Token.Accent, Token.AccentHover, Token.AccentFg,
        Token.BrandLight, Token.Brand, Token.BrandDeep, Token.BrandFar,
        Token.Success, Token.Warning,
    )

    /** A theme's own colors, with no accent picked. */
    fun colors(theme: PleyTheme): Map<Token, Long> = when (theme) {
        PleyTheme.Midnight -> mapOf(
            Token.Bg to 0xFF0B0D10,
            Token.Surface to 0xFF12151A,
            Token.Surface2 to 0xFF191D24,
            Token.Surface3 to 0xFF222731,
            Token.Border to 0xFF2A303B,
            Token.BorderStrong to 0xFF3A4250,
            Token.Fg to 0xFFE9EBEF,
            Token.Muted to 0xFFA1A9B6,
            Token.Subtle to 0xFF7E8795,
            Token.Accent to 0xFF38BDF8,
            Token.AccentHover to 0xFF7DD3FC,
            Token.AccentFg to 0xFF03131F,
            Token.Success to 0xFF3FB27F,
            Token.Danger to 0xFFED6C6D,
            Token.DangerFg to 0xFF0B0D10,
            Token.Warning to 0xFFF0A83C,
            Token.Info to 0xFF4C9AFF,
            Token.Shade to 0xFF000000,
            Token.BrandLight to 0xFFBAE6FD,
            Token.Brand to 0xFF38BDF8,
            Token.BrandDeep to 0xFF0284C7,
            Token.BrandFar to 0xFF0C4A6E,
            Token.BrandTile to 0xFF0A1622,
        )
        PleyTheme.Midday -> mapOf(
            Token.Bg to 0xFFF3F5F8,
            Token.Surface to 0xFFFFFFFF,
            Token.Surface2 to 0xFFEEF1F5,
            Token.Surface3 to 0xFFE3E7ED,
            Token.Border to 0xFFD5DAE2,
            Token.BorderStrong to 0xFFBCC4CF,
            Token.Fg to 0xFF12161C,
            Token.Muted to 0xFF4A5361,
            Token.Subtle to 0xFF5D6675,
            Token.Accent to 0xFF03669C,
            Token.AccentHover to 0xFF075985,
            Token.AccentFg to 0xFFFFFFFF,
            Token.Success to 0xFF126E46,
            Token.Danger to 0xFFB42D36,
            Token.DangerFg to 0xFFFFFFFF,
            Token.Warning to 0xFF855700,
            Token.Info to 0xFF1E5DC0,
            Token.Shade to 0x6B101A2C,
            Token.BrandLight to 0xFF38BDF8,
            Token.Brand to 0xFF0369A1,
            Token.BrandDeep to 0xFF3D9FD8,
            Token.BrandFar to 0xFF9AC9ED,
            Token.BrandTile to 0xFFDDEEFA,
        )
        PleyTheme.Twilight -> mapOf(
            Token.Bg to 0xFF140E22,
            Token.Surface to 0xFF1C142E,
            Token.Surface2 to 0xFF261B3C,
            Token.Surface3 to 0xFF31244B,
            Token.Border to 0xFF3A2B55,
            Token.BorderStrong to 0xFF4F3B72,
            Token.Fg to 0xFFF8EEF6,
            Token.Muted to 0xFFC9B8D1,
            Token.Subtle to 0xFF9A88AB,
            Token.Accent to 0xFFFF79B0,
            Token.AccentHover to 0xFFFFA0C8,
            Token.AccentFg to 0xFF2A0619,
            Token.Success to 0xFF4CC38A,
            Token.Danger to 0xFFF0727E,
            Token.DangerFg to 0xFF140E22,
            Token.Warning to 0xFFFFA24C,
            Token.Info to 0xFF7AA7FF,
            Token.Shade to 0xFF000000,
            Token.BrandLight to 0xFFFFD3A6,
            Token.Brand to 0xFFFF79B0,
            Token.BrandDeep to 0xFFC2448F,
            Token.BrandFar to 0xFF6E2C7A,
            Token.BrandTile to 0xFF1F1233,
        )
        PleyTheme.Starlight -> mapOf(
            Token.Bg to 0xFFF6F0E5,
            Token.Surface to 0xFFFDFAF4,
            Token.Surface2 to 0xFFF1E9DB,
            Token.Surface3 to 0xFFE8DFCD,
            Token.Border to 0xFFDDD2BD,
            Token.BorderStrong to 0xFFC6B79C,
            Token.Fg to 0xFF2A2318,
            Token.Muted to 0xFF5C5141,
            Token.Subtle to 0xFF685D4A,
            Token.Accent to 0xFF835613,
            Token.AccentHover to 0xFF774E11,
            Token.AccentFg to 0xFFFFFAF2,
            Token.Success to 0xFF206C45,
            Token.Danger to 0xFFAC2F28,
            Token.DangerFg to 0xFFFFFFFF,
            Token.Warning to 0xFF8A5200,
            Token.Info to 0xFF2B5DAF,
            Token.Shade to 0x61422C0C,
            Token.BrandLight to 0xFFD6A548,
            Token.Brand to 0xFF8A5A12,
            Token.BrandDeep to 0xFFC0904A,
            Token.BrandFar to 0xFFDCC08A,
            Token.BrandTile to 0xFFEFE3C8,
        )
        PleyTheme.Moonlight -> mapOf(
            Token.Bg to 0xFF0C0D0E,
            Token.Surface to 0xFF141517,
            Token.Surface2 to 0xFF1C1E20,
            Token.Surface3 to 0xFF25272A,
            Token.Border to 0xFF2E3033,
            Token.BorderStrong to 0xFF404347,
            Token.Fg to 0xFFF3EAD9,
            Token.Muted to 0xFFBDB6A8,
            Token.Subtle to 0xFF8E8A82,
            Token.Accent to 0xFFF5C542,
            Token.AccentHover to 0xFFF9D97C,
            Token.AccentFg to 0xFF1C1604,
            Token.Success to 0xFF5BC28C,
            Token.Danger to 0xFFE8726D,
            Token.DangerFg to 0xFF0C0D0E,
            Token.Warning to 0xFFF59A45,
            Token.Info to 0xFF6FA8FF,
            Token.Shade to 0xFF000000,
            Token.BrandLight to 0xFFFFF3C9,
            Token.Brand to 0xFFF5C542,
            Token.BrandDeep to 0xFFB48D2B,
            Token.BrandFar to 0xFF66532A,
            Token.BrandTile to 0xFF18191B,
        )
        PleyTheme.Nightshade -> mapOf(
            Token.Bg to 0xFF0A0F0C,
            Token.Surface to 0xFF111814,
            Token.Surface2 to 0xFF18211C,
            Token.Surface3 to 0xFF212C25,
            Token.Border to 0xFF29352E,
            Token.BorderStrong to 0xFF3A4A41,
            Token.Fg to 0xFFE7EFE9,
            Token.Muted to 0xFFA2B3A8,
            Token.Subtle to 0xFF798C81,
            Token.Accent to 0xFF5FD39A,
            Token.AccentHover to 0xFF8FE3B8,
            Token.AccentFg to 0xFF03140A,
            Token.Success to 0xFF3CC2A6,
            Token.Danger to 0xFFE8726D,
            Token.DangerFg to 0xFF0A0F0C,
            Token.Warning to 0xFFF0A83C,
            Token.Info to 0xFF6AA6FF,
            Token.Shade to 0xFF000000,
            Token.BrandLight to 0xFFC9F6DC,
            Token.Brand to 0xFF5FD39A,
            Token.BrandDeep to 0xFF2F9A68,
            Token.BrandFar to 0xFF1D5A3E,
            Token.BrandTile to 0xFF0F1A14,
        )
        PleyTheme.Greenlight -> mapOf(
            Token.Bg to 0xFFF1F6F2,
            Token.Surface to 0xFFFBFDFB,
            Token.Surface2 to 0xFFE8F0EA,
            Token.Surface3 to 0xFFDCE7DF,
            Token.Border to 0xFFCDDBD1,
            Token.BorderStrong to 0xFFB2C4B7,
            Token.Fg to 0xFF122017,
            Token.Muted to 0xFF45564A,
            Token.Subtle to 0xFF58695D,
            Token.Accent to 0xFF1A6E45,
            Token.AccentHover to 0xFF16623E,
            Token.AccentFg to 0xFFFFFFFF,
            Token.Success to 0xFF0D6A60,
            Token.Danger to 0xFFB02C35,
            Token.DangerFg to 0xFFFFFFFF,
            Token.Warning to 0xFF855700,
            Token.Info to 0xFF1E5DC0,
            Token.Shade to 0x610E2C1A,
            Token.BrandLight to 0xFF5FD39A,
            Token.Brand to 0xFF1A7048,
            Token.BrandDeep to 0xFF4FAE80,
            Token.BrandFar to 0xFF98CDB0,
            Token.BrandTile to 0xFFDBEFE2,
        )
        PleyTheme.Sunshine -> mapOf(
            Token.Bg to 0xFFFFF7E0,
            Token.Surface to 0xFFFFFFFF,
            Token.Surface2 to 0xFFE8F4FF,
            Token.Surface3 to 0xFFEFE7FF,
            Token.Border to 0xFFDDD3F6,
            Token.BorderStrong to 0xFFC3B4F0,
            Token.Fg to 0xFF24124F,
            Token.Muted to 0xFF4F437C,
            Token.Subtle to 0xFF62578F,
            Token.Accent to 0xFF5A2FF0,
            Token.AccentHover to 0xFF4A1FD6,
            Token.AccentFg to 0xFFFFFFFF,
            Token.Success to 0xFF0C713D,
            Token.Danger to 0xFFB81E3F,
            Token.DangerFg to 0xFFFFFFFF,
            Token.Warning to 0xFFA14700,
            Token.Info to 0xFF135EC2,
            Token.Shade to 0x573A1E64,
            Token.BrandLight to 0xFFFFD84A,
            Token.Brand to 0xFFFF8A1F,
            Token.BrandDeep to 0xFFFF5FA2,
            Token.BrandFar to 0xFF3FC1FF,
            Token.BrandTile to 0xFF3B2A7A,
        )
        PleyTheme.Blacklight -> mapOf(
            Token.Bg to 0xFF000000,
            Token.Surface to 0xFF0D0D0D,
            Token.Surface2 to 0xFF171717,
            Token.Surface3 to 0xFF222222,
            Token.Border to 0xFF2E2E2E,
            Token.BorderStrong to 0xFF4A4A4A,
            Token.Fg to 0xFFFFFFFF,
            Token.Muted to 0xFFB5B5B5,
            Token.Subtle to 0xFF8E8E8E,
            Token.Accent to 0xFFFFFFFF,
            Token.AccentHover to 0xFFD6D6D6,
            Token.AccentFg to 0xFF000000,
            Token.Success to 0xFFE0E0E0,
            Token.Danger to 0xFF8F8F8F,
            Token.DangerFg to 0xFF000000,
            Token.Warning to 0xFFA6A6A6,
            Token.Info to 0xFFC4C4C4,
            Token.Shade to 0xFF000000,
            Token.BrandLight to 0xFFFFFFFF,
            Token.Brand to 0xFFBDBDBD,
            Token.BrandDeep to 0xFF8A8A8A,
            Token.BrandFar to 0xFF4D4D4D,
            Token.BrandTile to 0xFF141414,
        )
    }

    /** The tokens an accent picked replaces in [theme]. */
    fun follows(theme: PleyTheme): Set<Token> = when (theme) {
        PleyTheme.Midnight -> ACCENT_LOGO_STATES
        PleyTheme.Midday -> ACCENT_LOGO_STATES
        PleyTheme.Twilight -> ACCENT_LOGO_STATES
        PleyTheme.Starlight -> ACCENT_LOGO_STATES
        PleyTheme.Moonlight -> ACCENT_LOGO_STATES
        PleyTheme.Nightshade -> ACCENT_LOGO_STATES
        PleyTheme.Greenlight -> ACCENT_LOGO_STATES
        PleyTheme.Sunshine -> setOf(Token.Accent, Token.AccentHover, Token.AccentFg, Token.Success, Token.Warning)
        PleyTheme.Blacklight -> setOf(Token.Accent, Token.AccentHover, Token.AccentFg, Token.BrandLight, Token.Brand, Token.BrandDeep, Token.BrandFar)
    }

    /** [accent]'s colors for the dark themes, or the light ones ([light]). */
    fun accent(accent: PleyAccent, light: Boolean): Map<Token, Long> = when (accent) {
        PleyAccent.Sky -> if (light) {
            mapOf(Token.Accent to 0xFF025D8F, Token.AccentHover to 0xFF034E78, Token.AccentFg to 0xFFFFFFFF, Token.BrandLight to 0xFF38BDF8, Token.Brand to 0xFF0369A1, Token.BrandDeep to 0xFF3D9FD8, Token.BrandFar to 0xFF9AC9ED)
        } else {
            mapOf(Token.Accent to 0xFF38BDF8, Token.AccentHover to 0xFF7DD3FC, Token.AccentFg to 0xFF03131F, Token.BrandLight to 0xFFBAE6FD, Token.Brand to 0xFF38BDF8, Token.BrandDeep to 0xFF0284C7, Token.BrandFar to 0xFF0C4A6E)
        }
        PleyAccent.Violet -> if (light) {
            mapOf(Token.Accent to 0xFF5A2FF0, Token.AccentHover to 0xFF4A1FD6, Token.AccentFg to 0xFFFFFFFF, Token.BrandLight to 0xFFB39AFD, Token.Brand to 0xFF5A2FF0, Token.BrandDeep to 0xFF8B6CF6, Token.BrandFar to 0xFFC6B6F8)
        } else {
            mapOf(Token.Accent to 0xFFB39AFD, Token.AccentHover to 0xFFCDBFFE, Token.AccentFg to 0xFF160B33, Token.BrandLight to 0xFFE2D9FF, Token.Brand to 0xFFB39AFD, Token.BrandDeep to 0xFF7B5CE6, Token.BrandFar to 0xFF43307F)
        }
        PleyAccent.Miami -> if (light) {
            mapOf(Token.Accent to 0xFFA61E6A, Token.AccentHover to 0xFF901A5C, Token.AccentFg to 0xFFFFFFFF, Token.BrandLight to 0xFFFF79B0, Token.Brand to 0xFFA61E6A, Token.BrandDeep to 0xFFDB5B9B, Token.BrandFar to 0xFFEEB4CF)
        } else {
            mapOf(Token.Accent to 0xFFFF79B0, Token.AccentHover to 0xFFFFA0C8, Token.AccentFg to 0xFF2A0619, Token.BrandLight to 0xFFFFD3A6, Token.Brand to 0xFFFF79B0, Token.BrandDeep to 0xFFC2448F, Token.BrandFar to 0xFF6E2C7A)
        }
        PleyAccent.Gold -> if (light) {
            mapOf(Token.Accent to 0xFF835613, Token.AccentHover to 0xFF774E11, Token.AccentFg to 0xFFFFFAF2, Token.BrandLight to 0xFFD6A548, Token.Brand to 0xFF8A5A12, Token.BrandDeep to 0xFFC0904A, Token.BrandFar to 0xFFDCC08A, Token.Warning to 0xFF9A3F17)
        } else {
            mapOf(Token.Accent to 0xFFF5C542, Token.AccentHover to 0xFFF9D97C, Token.AccentFg to 0xFF1C1604, Token.BrandLight to 0xFFFFF3C9, Token.Brand to 0xFFF5C542, Token.BrandDeep to 0xFFB48D2B, Token.BrandFar to 0xFF66532A, Token.Warning to 0xFFFF8F5A)
        }
        PleyAccent.Mint -> if (light) {
            mapOf(Token.Accent to 0xFF18663F, Token.AccentHover to 0xFF145A37, Token.AccentFg to 0xFFFFFFFF, Token.BrandLight to 0xFF5FD39A, Token.Brand to 0xFF1A7048, Token.BrandDeep to 0xFF4FAE80, Token.BrandFar to 0xFF98CDB0, Token.Success to 0xFF0D6A60)
        } else {
            mapOf(Token.Accent to 0xFF5FD39A, Token.AccentHover to 0xFF8FE3B8, Token.AccentFg to 0xFF03140A, Token.BrandLight to 0xFFC9F6DC, Token.Brand to 0xFF5FD39A, Token.BrandDeep to 0xFF2F9A68, Token.BrandFar to 0xFF1D5A3E, Token.Success to 0xFF3CC2A6)
        }
        PleyAccent.Amber -> if (light) {
            mapOf(Token.Accent to 0xFFA03D00, Token.AccentHover to 0xFF8B3500, Token.AccentFg to 0xFFFFFFFF, Token.BrandLight to 0xFFFF6F3C, Token.Brand to 0xFFA03D00, Token.BrandDeep to 0xFFDE5A2C, Token.BrandFar to 0xFFEFAE92, Token.Warning to 0xFF6F5A00)
        } else {
            mapOf(Token.Accent to 0xFFFF6F3C, Token.AccentHover to 0xFFFF956C, Token.AccentFg to 0xFF1F0702, Token.BrandLight to 0xFFFFCBB0, Token.Brand to 0xFFFF6F3C, Token.BrandDeep to 0xFFC4461A, Token.BrandFar to 0xFF6B2612, Token.Warning to 0xFFE8C23A)
        }
    }
}
