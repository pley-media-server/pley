package app.pley.android.ui.tv.theme

import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The TV tree never uses Compose Material 3: tv-material and Material 3
 * each have a MaterialTheme, and mixing them mixes colors, type and
 * shapes (https://developer.android.com/training/tv/playback/compose).
 * This reads every source file of :ui:tv.
 */
class NoMaterial3Test {
    @Test
    fun noSourceImportsMaterial3() {
        // Unit tests run in the module's directory.
        val sources = File("src").walkTopDown().filter { it.isFile && (it.extension == "kt" || it.extension == "kts") }.toList()
        assertTrue("found no sources under ${File("src").absolutePath}", sources.size > 10)
        val offenders = sources.filter { file ->
            file.readLines().any { line -> line.trimStart().startsWith("import androidx.compose.material3") }
        }.map { it.path }
        assertEquals(emptyList<String>(), offenders)
    }

    @Test
    fun theBuildDoesNotDependOnMaterial3() {
        val build = File("build.gradle.kts").readText()
        assertTrue(build.contains("tv.material"))
        assertTrue("no compose material3 in :ui:tv", !build.contains("compose.material3"))
    }
}
