package app.pley.android.benchmark

import androidx.benchmark.macro.CompilationMode
import androidx.benchmark.macro.FrameTimingMetric
import androidx.benchmark.macro.junit4.MacrobenchmarkRule
import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Assume.assumeFalse
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/**
 * The phone's frames while a finger flings, with its Baseline Profile,
 * against the demo server of benchmark builds: Home's page down and up and
 * a row along and back, and a library's grid of 1,500 posters down and up.
 * FrameTimingMetric gives each frame's time on the main thread and how far
 * past its deadline it was presented. Each run starts the app cold and
 * goes to where it scrolls unmeasured; signing in, through the browser,
 * happens once. On a phone or tablet (or their emulator), skipped on TVs;
 * run it with adb on one device (docs/ANDROID.md).
 */
@RunWith(AndroidJUnit4::class)
class PhoneScrollBenchmark {
    @get:Rule
    val rule = MacrobenchmarkRule()

    @Before
    fun onAPhone() = assumeFalse("Phone benchmarks run on a phone or tablet", isTv())

    @Test
    fun homeFlung() = rule.measureRepeated(
        packageName = TARGET,
        metrics = listOf(FrameTimingMetric()),
        compilationMode = CompilationMode.Partial(),
        iterations = ITERATIONS,
        setupBlock = {
            with(PhoneJourney(this)) {
                signedIn()
                restart()
            }
        },
    ) {
        PhoneJourney(this).flingHome(times = FLINGS)
    }

    @Test
    fun libraryGridFlung() = rule.measureRepeated(
        packageName = TARGET,
        metrics = listOf(FrameTimingMetric()),
        compilationMode = CompilationMode.Partial(),
        iterations = ITERATIONS,
        setupBlock = {
            with(PhoneJourney(this)) {
                signedIn()
                restart()
                openLibrary()
            }
        },
    ) {
        PhoneJourney(this).flingGrid(times = FLINGS)
    }

    private companion object {
        const val ITERATIONS = 5
        const val FLINGS = 3
    }
}
