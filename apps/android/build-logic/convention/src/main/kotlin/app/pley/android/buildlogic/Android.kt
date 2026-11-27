package app.pley.android.buildlogic

import com.android.build.api.dsl.CommonExtension
import java.time.Duration
import org.gradle.api.JavaVersion
import org.gradle.api.Project
import org.gradle.api.artifacts.VersionCatalog
import org.gradle.api.artifacts.VersionCatalogsExtension
import org.gradle.kotlin.dsl.getByType

/** What every module builds against: Android 17's SDK, down to Android 8 (Fire OS 7 and up). */
object Sdk {
    const val COMPILE = 37
    const val TARGET = 37
    const val MIN = 26

    /** Robolectric's newest SDK: unit tests run on it rather than on the target. */
    const val ROBOLECTRIC = 36
}

/**
 * How long a module's unit tests may run. The slowest module takes a minute
 * or two and all of them together 4 to 20, so a healthy run never reaches
 * it, and it ends within CI's hour whenever in the job the module's tests
 * start. A test that waits for ever fails its task at it, and the job fails
 * naming the module; without it, it holds CI until the job's own limit,
 * which ends the job canceled, with no report.
 */
internal val UNIT_TESTS_LIMIT: Duration = Duration.ofMinutes(15)

/** The version catalog, gradle/libs.versions.toml. */
val Project.libs: VersionCatalog
    get() = extensions.getByType<VersionCatalogsExtension>().named("libs")

fun VersionCatalog.lib(alias: String) = findLibrary(alias).get()

/**
 * The settings every Android module shares: SDK levels, Java 17, unit tests
 * with Android resources on Robolectric's SDK and its runtimes from Gradle,
 * and lint that fails the build, also on a word missing in French or Spanish
 * (docs/ANDROID.md, "Language").
 */
internal fun Project.configureAndroid(android: CommonExtension) {
    android.compileSdk = Sdk.COMPILE
    android.defaultConfig.minSdk = Sdk.MIN
    android.compileOptions.sourceCompatibility = JavaVersion.VERSION_17
    android.compileOptions.targetCompatibility = JavaVersion.VERSION_17
    android.testOptions.unitTests.isIncludeAndroidResources = true
    // Robolectric's Android 16 runtime reaches into JDK internals that JDK 21
    // closes off.
    android.testOptions.unitTests.all {
        it.jvmArgs(
            "--add-opens=java.base/jdk.internal.access=ALL-UNNAMED",
            "--add-exports=java.base/jdk.internal.access=ALL-UNNAMED",
            "--add-opens=java.base/java.io=ALL-UNNAMED",
        )
        it.timeout.set(UNIT_TESTS_LIMIT)
    }
    // robolectric.properties (sdk=36) for every module's unit tests.
    android.sourceSets.findByName("test")?.resources?.srcDir(
        rootProject.layout.projectDirectory.dir("build-logic/robolectric"),
    )
    configureRobolectric()
    android.lint.abortOnError = true
    android.lint.checkReleaseBuilds = true
    // Versions move through the catalog, not lint.
    android.lint.disable += setOf("NewerVersionAvailable", "GradleDependency", "AndroidGradlePluginVersion")
    // Every language has every word, every count its forms (French and Spanish:
    // one, many, other), and a translation the arguments of the English.
    android.lint.error += setOf("MissingTranslation", "MissingQuantity", "ImpliedQuantity", "StringFormatCount")
}
