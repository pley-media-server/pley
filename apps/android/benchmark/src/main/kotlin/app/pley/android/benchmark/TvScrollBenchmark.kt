package app.pley.android.benchmark

import androidx.benchmark.macro.CompilationMode
import androidx.benchmark.macro.FrameTimingMetric
import androidx.benchmark.macro.junit4.MacrobenchmarkRule
import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Assume.assumeTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/**
 * The TV's frames while a held key scrolls, with its Baseline Profile,
 * against the demo server of benchmark builds: Right held along each of
 * Home's rows in turn, and Down held through a library's grid of 1,500
 * posters. FrameTimingMetric gives each frame's time on the main thread and
 * how far past its deadline it was presented. Each run starts the app cold
 * and goes to where it scrolls (Who's watching? answered) unmeasured. On a
 * TV or a TV emulator only, skipped elsewhere; run it with adb on one
 * device (docs/ANDROID.md). The demo server has no Live TV, so the guide
 * has no benchmark here.
 */
@RunWith(AndroidJUnit4::class)
class TvScrollBenchmark {
    @get:Rule
    val rule = MacrobenchmarkRule()

    @Before
    fun onATv() = assumeTrue("TV benchmarks run on a TV", isTv())

    @Test
    fun homeRowsRightHeld() = rule.measureRepeated(
        packageName = TARGET,
        metrics = listOf(FrameTimingMetric()),
        compilationMode = CompilationMode.Partial(),
        iterations = ITERATIONS,
        setupBlock = {
            with(TvJourney(this)) {
                signedIn()
                restart()
                chooseProfile()
                intoRows()
            }
        },
    ) {
        with(TvJourney(this)) {
            repeat(ROWS) {
                hold(Key.RIGHT, ROW_HOLD_MS)
                press(Key.DOWN)
            }
        }
    }

    @Test
    fun libraryGridDownHeld() = rule.measureRepeated(
        packageName = TARGET,
        metrics = listOf(FrameTimingMetric()),
        compilationMode = CompilationMode.Partial(),
        iterations = ITERATIONS,
        setupBlock = {
            with(TvJourney(this)) {
                signedIn()
                restart()
                chooseProfile()
                railTo("library:${Demo.MOVIES}")
                intoGrid()
            }
        },
    ) {
        TvJourney(this).hold(Key.DOWN, GRID_HOLD_MS)
    }

    private companion object {
        const val ITERATIONS = 5

        /** Home's rows for Alex: Continue Watching, My List, and each library's Recently Added. */
        const val ROWS = 4

        /** Long enough to reach the end of a row of 20 at the held key's pace. */
        const val ROW_HOLD_MS = 2_500L
        const val GRID_HOLD_MS = 6_000L
    }
}
