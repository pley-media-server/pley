package app.pley.android.benchmark

import androidx.benchmark.macro.CompilationMode
import androidx.benchmark.macro.ExperimentalMetricApi
import androidx.benchmark.macro.StartupMode
import androidx.benchmark.macro.StartupTimingMetric
import androidx.benchmark.macro.TraceSectionMetric
import androidx.benchmark.macro.junit4.MacrobenchmarkRule
import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/**
 * Cold starts of the TV app, signed in to the demo server of benchmark
 * builds, with its Baseline Profile: the time to the first frame and to the
 * first screen fully drawn (Home with its pictures, or "Who's watching?"
 * when the household has several profiles, as the demo's does), and the
 * app's own startup steps. On a TV or a TV emulator only; run it with adb
 * on one device (docs/ANDROID.md), never as a connected task, which
 * installs the app on every device attached.
 */
@OptIn(ExperimentalMetricApi::class)
@RunWith(AndroidJUnit4::class)
class TvStartupBenchmark {
    @get:Rule
    val rule = MacrobenchmarkRule()

    @Test
    fun coldStart() = rule.measureRepeated(
        packageName = TARGET,
        metrics = listOf(
            StartupTimingMetric(),
            TraceSectionMetric(APP_ON_CREATE),
            TraceSectionMetric(STARTUP_READ),
        ),
        compilationMode = CompilationMode.Partial(),
        startupMode = StartupMode.COLD,
        iterations = 10,
        setupBlock = {
            // Signed in once, to the demo server ("Use demo server" approves its own code).
            TvJourney(this).signedIn()
            pressHome()
        },
    ) {
        startActivityAndWait(tvIntent())
        // Until the first screen is fully drawn, which the app reports.
        Thread.sleep(FULLY_DRAWN_WAIT_MS)
    }

    private companion object {
        const val FULLY_DRAWN_WAIT_MS = 4_000L

        // The app's trace sections (app.pley.android.common.TraceNames).
        const val APP_ON_CREATE = "pley.app.onCreate"
        const val STARTUP_READ = "pley.startup.read"
    }
}
