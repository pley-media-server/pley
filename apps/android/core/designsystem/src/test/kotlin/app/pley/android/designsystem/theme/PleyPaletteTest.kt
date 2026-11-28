package app.pley.android.designsystem.theme

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.toArgb
import java.io.File
import kotlin.math.roundToInt
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertSame
import org.junit.Test

/**
 * Holds the app's colors to packages/themes/themes.json, which every app
 * draws its themes from (docs/THEMES.md, "In every app"): the themes and
 * accents, their ids and order, and every token of every theme under every
 * accent, as the web's `colorOf` gives it. A change to the file fails here
 * until ThemeTable follows it.
 */
class PleyPaletteTest {
    private val spec: JsonObject by lazy {
        val here = File("").absoluteFile
        val file = generateSequence(here) { it.parentFile }
            .map { File(it, "packages/themes/themes.json") }
            .firstOrNull { it.isFile }
            ?: error("no packages/themes/themes.json above $here")
        Json.parseToJsonElement(file.readText()).jsonObject
    }

    private val themes get() = spec.getValue("themes").jsonArray.map { it.jsonObject }
    private val accents get() = spec.getValue("accents").jsonArray.map { it.jsonObject }

    @Test
    fun theThemesAndAccentsAreTheFilesInItsOrder() {
        assertEquals(themes.map { it.string("id") }, PleyTheme.entries.map { it.id })
        assertEquals(accents.map { it.string("id") }, PleyAccent.entries.map { it.id })
        for (theme in themes) {
            val ours = PleyTheme.of(theme.string("id"))
            assertEquals(ours.id, theme.string("kind") == "light", ours.light)
            val twin = theme.getValue("twin").takeUnless { it is JsonNull }?.jsonPrimitive?.content
            // A dark theme's players draw in it; a light one's in its twin.
            assertEquals(ours.id, twin ?: ours.id, ours.twin.id)
        }
        assertEquals(Token.entries.map { it.id }.toSet(), themes.first().getValue("colors").jsonObject.keys)
    }

    @Test
    fun everyTokenOfEveryThemeUnderEveryAccentIsTheFiles() {
        for (theme in themes) {
            val id = theme.string("id")
            val colors = theme.getValue("colors").jsonObject
            val follows = theme.getValue("follows").jsonArray.map { it.jsonPrimitive.content }.toSet()
            val kind = theme.string("kind")
            for (accent in listOf<JsonObject?>(null) + accents) {
                val palette = PleyPalette.of(PleyTheme.of(id), accent?.let { PleyAccent.of(it.string("id")) })
                for (token in Token.entries) {
                    // The web's colorOf: the accent's where the theme follows the token and the accent sets it.
                    val set = accent?.getValue(kind)?.jsonObject
                    val expected = set?.get(token.id)?.takeIf { token.id in follows } ?: colors.getValue(token.id)
                    assertEquals("$palette ${token.id}", argb(expected.jsonPrimitive.content), palette.color(token).toArgb())
                }
            }
        }
    }

    /** Each color the app reads by its role is the token it names. */
    @Test
    fun theRolesAreTheirTokens() {
        for (theme in PleyTheme.entries) {
            for (accent in listOf(null) + PleyAccent.entries) {
                val p = PleyPalette.of(theme, accent)
                val roles = mapOf(
                    Token.Bg to p.bg, Token.Surface to p.surface, Token.Surface2 to p.surface2, Token.Surface3 to p.surface3,
                    Token.Border to p.border, Token.BorderStrong to p.borderStrong, Token.Fg to p.fg, Token.Muted to p.muted,
                    Token.Subtle to p.subtle, Token.Accent to p.accent, Token.AccentHover to p.accentHover, Token.AccentFg to p.accentFg,
                    Token.Success to p.success, Token.Danger to p.danger, Token.DangerFg to p.dangerFg, Token.Warning to p.warning,
                    Token.Info to p.info, Token.Shade to p.shade, Token.BrandLight to p.brandLight, Token.Brand to p.brand,
                    Token.BrandDeep to p.brandDeep, Token.BrandFar to p.brandFar, Token.BrandTile to p.brandTile,
                )
                assertEquals(Token.entries.toSet(), roles.keys)
                for ((token, color) in roles) assertEquals("$p ${token.id}", p.color(token), color)
                // A light theme's players take its dark twin, in the same accent.
                assertSame(PleyPalette.of(theme.twin, accent), p.dark)
            }
        }
    }

    @Test
    fun anUnknownIdShowsMidnightInItsOwnAccent() {
        assertEquals(PleyTheme.Midnight, PleyTheme.of(null))
        assertEquals(PleyTheme.Midnight, PleyTheme.of("dusk"))
        assertEquals(null, PleyAccent.of("teal"))
        assertSame(PleyPalette.of(PleyTheme.Midnight, null), PleyPalette.Default)
    }

    private fun JsonObject.string(key: String) = getValue(key).jsonPrimitive.content

    /** A file's color as ARGB: `#rrggbb`, or `rgb(r g b / alpha)`. */
    private fun argb(value: String): Int {
        if (value.startsWith("#")) return Color(("ff" + value.substring(1)).toLong(16)).toArgb()
        val match = Regex("""rgb\((\d+) (\d+) (\d+) / ([\d.]+)\)""").matchEntire(value) ?: error("can't read $value")
        val (r, g, b, a) = match.destructured
        return Color(r.toInt(), g.toInt(), b.toInt(), (a.toFloat() * 255).roundToInt()).toArgb()
    }
}
