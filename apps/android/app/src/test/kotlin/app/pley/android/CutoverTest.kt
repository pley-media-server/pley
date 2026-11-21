package app.pley.android

import java.io.File
import javax.xml.parsers.DocumentBuilderFactory
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.w3c.dom.Element

/**
 * The native app is the whole app: its two launchers (and Android Auto's
 * in the sideloaded build), nothing else exported, and no web view
 * outside the one-time clean-up (migration/).
 * Reads the project's own sources; unit tests run in :app's folder.
 */
class CutoverTest {
    private val project = File("..").canonicalFile

    /** The manifests release builds merge: every module's main one, and the app's flavors'. */
    private val releaseManifests: List<File> = project.walkTopDown()
        .onEnter { it.name != "build" && it.name != ".gradle" }
        .filter { it.name == "AndroidManifest.xml" }
        .filter { it.parentFile.name in setOf("main", "release", "store", "direct") && it.parentFile.parentFile.name == "src" }
        .filter { !it.path.contains("${File.separator}benchmark${File.separator}") }
        .toList()

    private fun components(): List<Element> = releaseManifests.flatMap(::components)

    private fun components(file: File): List<Element> {
        val doc = DocumentBuilderFactory.newInstance().apply { isNamespaceAware = true }.newDocumentBuilder().parse(file)
        return listOf("activity", "activity-alias", "service", "receiver", "provider").flatMap { tag ->
            val nodes = doc.getElementsByTagName(tag)
            (0 until nodes.length).map { nodes.item(it) as Element }
        }
    }

    private fun Element.attr(name: String): String = getAttributeNS(ANDROID, name)

    private fun Element.simpleName(): String = attr("name").substringAfterLast('.')

    private fun Element.categories(): Set<String> {
        val nodes = getElementsByTagName("category")
        return (0 until nodes.length).map { (nodes.item(it) as Element).attr("name") }.toSet()
    }

    @Test
    fun onlyTheAppsOwnDoorsAreOpen() {
        assertTrue(releaseManifests.size >= 5)
        // A flavor's manifest adds to a component main declares (the direct build's car launcher): each counts once.
        val exported = components().filter { it.attr("exported") == "true" }.map { it.simpleName() }.distinct().sorted()
        assertEquals(listOf("MainActivity", "MusicService", "SignInRedirectActivity", "TvActivity", "WatchNextImageProvider"), exported)
    }

    @Test
    fun phonesAndTvsEachHaveOneLauncher() {
        val all = components()
        assertEquals(listOf("MainActivity"), all.filter { LAUNCHER in it.categories() }.map { it.simpleName() })
        val tv = all.filter { LEANBACK_LAUNCHER in it.categories() }
        assertEquals(listOf("TvActivity"), tv.map { it.simpleName() })
        assertEquals("@drawable/tv_banner", tv.single().attr("banner"))
        // Google TV's home screen draws this icon in a circle: the app's P, never the banner.
        assertEquals("", tv.single().attr("icon"))
    }

    @Test
    fun onlyTheSideloadedBuildOpensOnACarsScreen() {
        val car = releaseManifests.flatMap { file ->
            components(file).filter { CAR_LAUNCHER in it.categories() }.map { "${file.parentFile.name}:${it.simpleName()}" }
        }
        assertEquals(listOf("direct:MainActivity"), car)
    }

    @Test
    fun noWebViewOutsideTheCleanUp() {
        val never = Regex("""\bWebView\b|\bCookieManager\b|\bWebStorage\b|android\.webkit|androidx\.webkit|pley_token""")
        val sources = project.walkTopDown()
            .onEnter { it.name != "build" && it.name != ".gradle" }
            .filter { it.extension == "kt" || it.extension == "java" }
            .filter { it.path.contains("${File.separator}src${File.separator}main${File.separator}") }
            .toList()
        assertTrue(sources.size > 100)
        val offenders = sources
            .filter { !it.path.contains("${File.separator}migration${File.separator}") }
            .filter { never.containsMatchIn(it.readText()) }
            .map { it.relativeTo(project).path }
        assertEquals(emptyList<String>(), offenders)
    }

    private companion object {
        const val ANDROID = "http://schemas.android.com/apk/res/android"
        const val LAUNCHER = "android.intent.category.LAUNCHER"
        const val LEANBACK_LAUNCHER = "android.intent.category.LEANBACK_LAUNCHER"
        const val CAR_LAUNCHER = "android.intent.category.CAR_LAUNCHER"
    }
}
