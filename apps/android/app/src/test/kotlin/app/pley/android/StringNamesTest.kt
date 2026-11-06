package app.pley.android

import java.io.File
import javax.xml.parsers.DocumentBuilderFactory
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.w3c.dom.Element

/**
 * No two modules give a word the same name: the app keeps one value for
 * each name, so the other module would show the first one's words (the
 * player's "Audio: English" said only "Audio"). A module's own tests can't
 * see it. Reads the project's own sources; unit tests run in :app's folder.
 */
class StringNamesTest {
    private val project = File("..").canonicalFile

    @Test
    fun everyModuleNamesItsOwnWords() {
        val owners = mutableMapOf<String, MutableSet<String>>()
        project.walkTopDown()
            .onEnter { it.name != "build" && it.name != ".gradle" && it.name != "benchmark" }
            .filter { it.extension == "xml" && it.parentFile.name == "values" && it.parentFile.parentFile.name == "res" && it.parentFile.parentFile.parentFile.name == "main" }
            .forEach { file ->
                // <module>/src/main/res/values/<file>.xml
                val module = file.parentFile.parentFile.parentFile.parentFile.parentFile.relativeTo(project).path
                val nodes = DocumentBuilderFactory.newInstance().newDocumentBuilder().parse(file).documentElement.childNodes
                for (i in 0 until nodes.length) {
                    val element = nodes.item(i) as? Element ?: continue
                    if (element.tagName in WORDS) owners.getOrPut(element.getAttribute("name")) { mutableSetOf() } += module
                }
            }
        assertTrue("found no words", owners.size > 500)
        assertEquals(emptyMap<String, Set<String>>(), owners.filterValues { it.size > 1 })
    }

    private companion object {
        val WORDS = setOf("string", "plurals", "string-array")
    }
}
