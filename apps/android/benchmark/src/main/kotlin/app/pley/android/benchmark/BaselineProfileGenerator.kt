package app.pley.android.benchmark

import androidx.benchmark.macro.junit4.BaselineProfileRule
import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Assume.assumeFalse
import org.junit.Assume.assumeTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/**
 * Makes the app's Baseline and Startup Profiles from its journeys, on an
 * emulator or device with API 33 or later (docs/ANDROID.md): run on purpose,
 * the result committed. Cold start, the only source of the Startup Profile,
 * which lays out the dex for what starting runs; the TV's journey on a TV and
 * the phone's on a phone or tablet, each skipped on the other kind of device,
 * for the Baseline Profile only. Both walk
 * the demo server of benchmark builds and wait at each step for the screen
 * it leads to ([Steps]), so a walk that goes astray fails where it does
 * rather than filling the profile with the wrong screens.
 */
@RunWith(AndroidJUnit4::class)
class BaselineProfileGenerator {
    @get:Rule
    val rule = BaselineProfileRule()

    @Test
    fun startup() = rule.collect(packageName = TARGET, includeInStartupProfile = true) {
        pressHome()
        startActivityAndWait()
    }

    /**
     * The TV, with the D-pad. Each pass starts from a cleared app, so the
     * demo server runs in the app's process while it is walked: sign-in,
     * then a cold start's Who's watching?, answered, Home (a tile's actions,
     * the rows), a show's page (episodes, OK held, rows, the cast, a
     * person), search, a library's grid, Collections and a collection,
     * Playlists and a playlist, Settings (a choice's panel, the Libraries
     * page), the rail opened and closed, and the player.
     */
    @Test
    fun tv() {
        assumeTrue("The TV's journey runs on a TV", isTv())
        rule.collect(packageName = TARGET, includeInStartupProfile = false) {
            with(TvJourney(this)) {
                signIn()
                restart()
                chooseProfile()
                walkHome()
                walkShow()
                walkSearch()
                walkLibrary()
                walkCollections()
                walkPlaylists()
                walkSettings()
                railTo("home")
                railOpenAndClose()
                walkPlayer()
            }
        }
    }

    /**
     * The phone, by touch, signed in once: a cold start on Home, its page
     * and a row flung, a show's page (the cast, a person, the player), a
     * library's grid, search, Playlists and a playlist, and Me's Settings.
     */
    @Test
    fun phone() {
        assumeFalse("The phone's journey runs on a phone or tablet", isTv())
        rule.collect(packageName = TARGET, includeInStartupProfile = false) {
            with(PhoneJourney(this)) {
                signedIn()
                restart()
                flingHome()
                walkShow()
                openLibrary()
                flingGrid()
                walkSearch()
                walkPlaylists()
                walkMe()
            }
        }
    }
}
