import app.pley.android.buildlogic.UNIT_TESTS_LIMIT
import app.pley.android.buildlogic.lib
import app.pley.android.buildlogic.libs
import org.gradle.api.JavaVersion
import org.gradle.api.Plugin
import org.gradle.api.Project
import org.gradle.api.plugins.JavaPluginExtension
import org.gradle.api.tasks.testing.Test
import org.gradle.kotlin.dsl.configure
import org.gradle.kotlin.dsl.dependencies
import org.gradle.kotlin.dsl.withType
import org.jetbrains.kotlin.gradle.dsl.JvmTarget
import org.jetbrains.kotlin.gradle.dsl.KotlinJvmProjectExtension

/**
 * `pley.jvm.library`: plain Kotlin for the JVM (no Android), for code the
 * app and its tests share. `testDebugUnitTest` runs its tests too, so one
 * command tests every module.
 */
class JvmLibraryConventionPlugin : Plugin<Project> {
    override fun apply(target: Project): Unit = with(target) {
        pluginManager.apply("org.jetbrains.kotlin.jvm")
        extensions.configure<JavaPluginExtension> {
            sourceCompatibility = JavaVersion.VERSION_17
            targetCompatibility = JavaVersion.VERSION_17
        }
        extensions.configure<KotlinJvmProjectExtension> {
            compilerOptions.jvmTarget.set(JvmTarget.JVM_17)
        }
        dependencies {
            add("testImplementation", libs.lib("junit"))
        }
        tasks.withType<Test>().configureEach { timeout.set(UNIT_TESTS_LIMIT) }
        tasks.register("testDebugUnitTest") {
            group = "verification"
            description = "Runs this JVM module's tests, with the Android modules' debug unit tests."
            dependsOn("test")
        }
    }
}
