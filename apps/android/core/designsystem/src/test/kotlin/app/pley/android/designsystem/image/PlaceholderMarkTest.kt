package app.pley.android.designsystem.image

import app.pley.android.model.ItemKind
import app.pley.android.model.TitleKind
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.float
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * The mark's copy here is packages/placeholder/placeholder.json's, which
 * every app draws from: its glyphs, its posters, its sizes and which kind
 * of item takes which glyph.
 */
class PlaceholderMarkTest {
    private val marks: JsonObject get() = placeholderFile

    private fun number(name: String): Float = marks.getValue(name).jsonPrimitive.float

    private fun text(name: String): String = marks.getValue(name).jsonPrimitive.content

    /** The glyph's name in placeholder.json: "movies". */
    private val MarkGlyph.key: String get() = name.lowercase()

    @Test
    fun eachGlyphIsMarksJsons() {
        val glyphs = marks.getValue("glyphs").jsonObject
        assertEquals(glyphs.keys, MarkGlyph.entries.map { it.key }.toSet())
        for (glyph in MarkGlyph.entries) assertEquals(glyph.name, glyphs.getValue(glyph.key).jsonPrimitive.content, glyph.path)
    }

    @Test
    fun thePostersAreMarksJsons() {
        val posters = marks.getValue("posters").jsonArray.map { it.jsonObject }
        val expected = posters.map { it.getValue("path").jsonPrimitive.content to it.getValue("alpha").jsonPrimitive.float }
        assertEquals(expected, PlaceholderMark.POSTERS)
    }

    @Test
    fun theSizesAreMarksJsons() {
        assertEquals(number("grid"), PlaceholderMark.GRID, 0f)
        assertEquals(number("share"), PlaceholderMark.SHARE, 0f)
        assertEquals(number("shareOverTitle"), PlaceholderMark.SHARE_OVER_TITLE, 0f)
        assertEquals(number("max"), PlaceholderMark.Max.value, 0f)
        assertEquals(number("gap"), PlaceholderMark.Gap.value, 0f)
    }

    @Test
    fun eachKindTakesMarksJsonsGlyph() {
        val kinds = marks.getValue("kinds").jsonObject.mapValues { it.value.jsonPrimitive.content }
        assertEquals(kinds, MarkGlyph.KINDS.mapValues { it.value.key })
        assertEquals(text("default"), MarkGlyph.DEFAULT.key)
        // A kind it doesn't name, or none, takes the default.
        assertEquals(MarkGlyph.DEFAULT, MarkGlyph.of("collection"))
        assertEquals(MarkGlyph.DEFAULT, MarkGlyph.of(null))
        // The app's own kinds, as the API names them.
        for (kind in ItemKind.entries) assertEquals(kind.name, kinds[kind.wire] ?: text("default"), MarkGlyph.of(kind).key)
        for (kind in TitleKind.entries) assertEquals(kind.name, kinds[kind.wire] ?: text("default"), MarkGlyph.of(kind).key)
    }
}
