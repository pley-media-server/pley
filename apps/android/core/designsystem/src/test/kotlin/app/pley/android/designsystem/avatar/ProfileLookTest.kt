package app.pley.android.designsystem.avatar

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.toArgb
import java.io.File
import java.util.Locale
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * Holds every avatar the app draws to packages/tv/test/fixtures/avatar.json,
 * the rule every app follows (docs/THEMES.md, "Profile colors"): the eight
 * colors, the two inks, and each case's color, ink and initial.
 */
class ProfileLookTest {
    private val fixture: JsonObject by lazy {
        val here = File("").absoluteFile
        val file = generateSequence(here) { it.parentFile }
            .map { File(it, "packages/tv/test/fixtures/avatar.json") }
            .firstOrNull { it.isFile }
            ?: error("no packages/tv/test/fixtures/avatar.json above $here")
        Json.parseToJsonElement(file.readText()).jsonObject
    }

    @Test
    fun theColorsAndInksAreTheFixtures() {
        assertEquals(fixture.getValue("colors").jsonArray.map { it.jsonPrimitive.content }, ProfileLook.colors.map(::hex))
        val ink = fixture.getValue("ink").jsonObject
        assertEquals(ink.string("dark"), hex(ProfileLook.InkDark))
        assertEquals(ink.string("light"), hex(ProfileLook.InkLight))
    }

    @Test
    fun everyCaseHasTheFixturesColorInkAndInitial() {
        val cases = fixture.getValue("cases").jsonArray.map { it.jsonObject }
        assertEquals(22, cases.size)
        for (case in cases) {
            val name = case.string("name")
            val chosen = case.getValue("avatar_color").takeUnless { it is JsonNull }?.jsonPrimitive?.content
            val color = ProfileLook.color(name, chosen)
            assertEquals("\"$name\", $chosen: color", case.string("color"), hex(color))
            assertEquals("\"$name\", $chosen: ink", case.string("ink"), hex(ProfileLook.ink(color)))
            assertEquals("\"$name\": initial", case.string("initial"), ProfileLook.initial(name))
        }
    }

    private fun JsonObject.string(key: String) = getValue(key).jsonPrimitive.content

    private fun hex(color: Color): String = String.format(Locale.ROOT, "#%06x", color.toArgb() and 0xFFFFFF)
}
