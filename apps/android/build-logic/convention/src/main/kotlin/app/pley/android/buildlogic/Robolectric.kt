package app.pley.android.buildlogic

import java.util.Properties
import org.gradle.api.DefaultTask
import org.gradle.api.Project
import org.gradle.api.file.ConfigurableFileCollection
import org.gradle.api.file.RegularFile
import org.gradle.api.file.RegularFileProperty
import org.gradle.api.provider.Provider
import org.gradle.api.tasks.InputFiles
import org.gradle.api.tasks.Internal
import org.gradle.api.tasks.OutputFile
import org.gradle.api.tasks.PathSensitive
import org.gradle.api.tasks.PathSensitivity
import org.gradle.api.tasks.TaskAction
import org.gradle.api.tasks.testing.Test
import org.gradle.kotlin.dsl.register
import org.gradle.kotlin.dsl.withType
import org.gradle.process.CommandLineArgumentProvider
import org.gradle.work.DisableCachingByDefault

/**
 * Robolectric's Android runtimes, one for each SDK the unit tests run on:
 * robolectric.properties' 36 and the 27 of `@Config(sdk = [27])`. Their
 * catalog versions are the ones this Robolectric asks for and move with it;
 * a test on another SDK needs its runtime here and in the catalog.
 */
private val RUNTIMES = listOf("robolectric-android-all-36", "robolectric-android-all-27")

/**
 * Gradle fetches Robolectric's runtimes as it does every other dependency,
 * with its retries and the cache CI keeps, and hands them to the tests in a
 * robolectric-deps.properties. Left to itself, Robolectric downloads them
 * from Maven Central while the tests run, into `~/.m2`, which CI starts
 * without: one failed download there failed a module's tests.
 */
internal fun Project.configureRobolectric() {
    val runtimes = files()
    RUNTIMES.forEach { alias ->
        // A configuration each: in one, Gradle would keep only the newest
        // version of android-all-instrumented.
        val runtime = configurations.create("robolectricRuntime" + alias.substringAfterLast('-')) {
            isCanBeConsumed = false
            isTransitive = false
        }
        dependencies.add(runtime.name, libs.lib(alias))
        runtimes.from(runtime)
    }
    val deps = tasks.register<RobolectricDeps>("robolectricDeps") {
        this.runtimes.from(runtimes)
        properties.set(layout.buildDirectory.file("robolectric/robolectric-deps.properties"))
    }
    tasks.withType<Test>().configureEach {
        dependsOn(deps)
        // The runtimes' content, not where Gradle keeps them, so the tests'
        // results stay in the build cache across machines.
        inputs.files(runtimes).withPathSensitivity(PathSensitivity.NONE).withPropertyName("robolectricRuntimes")
        jvmArgumentProviders.add(RobolectricDepsArgument(deps.flatMap { it.properties }))
    }
}

/** Writes where each runtime is, for Robolectric's `robolectric-deps.properties`. */
@DisableCachingByDefault(because = "Writes the paths of files already on this machine")
abstract class RobolectricDeps : DefaultTask() {
    @get:InputFiles
    @get:PathSensitive(PathSensitivity.NAME_ONLY)
    abstract val runtimes: ConfigurableFileCollection

    @get:OutputFile
    abstract val properties: RegularFileProperty

    @TaskAction
    fun write() {
        val paths = Properties()
        runtimes.files.forEach { jar ->
            require(jar.name.startsWith(ARTIFACT_PREFIX) && jar.name.endsWith(".jar")) { "Not a Robolectric runtime: $jar" }
            val version = jar.name.removePrefix(ARTIFACT_PREFIX).removeSuffix(".jar")
            // Robolectric reads `group:artifact:version` keys; forward slashes
            // keep Windows paths free of the format's escapes.
            paths["org.robolectric:android-all-instrumented:$version"] = jar.invariantSeparatorsPath
        }
        properties.get().asFile.outputStream().use { paths.store(it, null) }
    }

    private companion object {
        const val ARTIFACT_PREFIX = "android-all-instrumented-"
    }
}

/** Points Robolectric at the runtimes' paths rather than at Maven Central. */
internal class RobolectricDepsArgument(
    // Paths on this machine; the test task tracks the runtimes themselves.
    @get:Internal val properties: Provider<RegularFile>,
) : CommandLineArgumentProvider {
    override fun asArguments() = listOf("-Drobolectric-deps.properties=${properties.get().asFile.absolutePath}")
}
