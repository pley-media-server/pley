package app.pley.android.benchmark

import androidx.benchmark.macro.CompilationMode
import androidx.benchmark.macro.StartupMode
import androidx.benchmark.macro.StartupTimingMetric
import androidx.benchmark.macro.junit4.MacrobenchmarkRule
import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/**
 * Cold starts of the app, without and with its Baseline Profile. A
 * skeleton: the journeys, the fake server and the budgets come with the
 * measurement setup (docs/ANDROID.md). Numbers from an emulator never count.
 */
@RunWith(AndroidJUnit4::class)
class StartupBenchmark {
    @get:Rule
    val rule = MacrobenchmarkRule()

    @Test
    fun coldStartWithoutProfile() = startup(CompilationMode.None())

    @Test
    fun coldStartWithProfile() = startup(CompilationMode.Partial())

    private fun startup(compilation: CompilationMode) = rule.measureRepeated(
        packageName = TARGET,
        metrics = listOf(StartupTimingMetric()),
        compilationMode = compilation,
        startupMode = StartupMode.COLD,
        iterations = 10,
    ) {
        pressHome()
        startActivityAndWait()
    }
}

/** The app under test: its id with the suffix it was built with (docs/ANDROID.md, "Performance"). */
const val TARGET = BuildConfig.TARGET
