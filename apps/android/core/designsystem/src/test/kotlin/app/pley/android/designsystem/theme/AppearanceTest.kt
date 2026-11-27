package app.pley.android.designsystem.theme

import android.content.Context
import androidx.test.core.app.ApplicationProvider
import org.junit.After
import org.junit.Assert.assertSame
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/**
 * The theme kept on the device for each profile (docs/THEMES.md, "In every
 * app"): each profile's own pick, Midnight for one new to the device and
 * outside a profile, and a start inside a profile in its pick from the
 * first frame.
 */
@RunWith(RobolectricTestRunner::class)
class AppearanceTest {
    private val context: Context get() = ApplicationProvider.getApplicationContext()

    @Before
    fun start() {
        context.getSharedPreferences("appearance", Context.MODE_PRIVATE).edit().clear().commit()
        Appearance.forget()
    }

    @After
    fun end() = Appearance.forget()

    @Test
    fun twoProfilesKeepTheirOwnPicks() {
        Appearance.attach(context)
        Appearance.showProfile(context, SERVER, 1)
        Appearance.pick(context, PleyTheme.Sunshine)
        Appearance.pickAccent(context, PleyAccent.Mint)
        Appearance.showProfile(context, SERVER, 2)
        Appearance.pick(context, PleyTheme.Blacklight)
        // Switching back shows the first profile's at once.
        Appearance.showProfile(context, SERVER, 1)
        assertSame(PleyPalette.of(PleyTheme.Sunshine, PleyAccent.Mint), Appearance.palette)
        Appearance.showProfile(context, SERVER, 2)
        assertSame(PleyPalette.of(PleyTheme.Blacklight, null), Appearance.palette)
    }

    @Test
    fun theSameUserIdOnTwoServersKeepsTwoPicks() {
        Appearance.attach(context)
        Appearance.showProfile(context, SERVER, 1)
        Appearance.pick(context, PleyTheme.Nightshade)
        Appearance.showProfile(context, OTHER_SERVER, 1)
        assertSame(PleyPalette.Default, Appearance.palette)
        Appearance.pick(context, PleyTheme.Midday)
        Appearance.showProfile(context, SERVER, 1)
        assertSame(PleyPalette.of(PleyTheme.Nightshade, null), Appearance.palette)
        Appearance.showProfile(context, OTHER_SERVER, 1)
        assertSame(PleyPalette.of(PleyTheme.Midday, null), Appearance.palette)
    }

    @Test
    fun aProfileNewToTheDeviceShowsMidnight() {
        Appearance.attach(context)
        Appearance.showProfile(context, SERVER, 1)
        Appearance.pick(context, PleyTheme.Twilight)
        Appearance.pickAccent(context, PleyAccent.Gold)
        Appearance.showProfile(context, SERVER, 3)
        assertSame(PleyPalette.Default, Appearance.palette)
    }

    @Test
    fun aStartInsideAProfileIsInItsPickFromTheFirstFrame() {
        Appearance.attach(context)
        Appearance.showProfile(context, SERVER, 1)
        Appearance.pick(context, PleyTheme.Starlight)
        Appearance.showProfile(context, SERVER, 2)
        Appearance.pick(context, PleyTheme.Moonlight)
        Appearance.pickAccent(context, PleyAccent.Violet)
        // The app starts again, the profile signed in still the second: its pick before it is known.
        Appearance.forget()
        Appearance.attach(context)
        assertSame(PleyPalette.of(PleyTheme.Moonlight, PleyAccent.Violet), Appearance.palette)
        // Another profile signed in meanwhile: its own once known, and the next start's.
        Appearance.showProfile(context, SERVER, 1)
        assertSame(PleyPalette.of(PleyTheme.Starlight, null), Appearance.palette)
        Appearance.forget()
        Appearance.attach(context)
        assertSame(PleyPalette.of(PleyTheme.Starlight, null), Appearance.palette)
    }

    @Test
    fun outsideAProfileIsMidnightAndThePickComesBack() {
        Appearance.attach(context)
        Appearance.showProfile(context, SERVER, 1)
        Appearance.pick(context, PleyTheme.Sunshine)
        Appearance.pickAccent(context, PleyAccent.Gold)
        val pick = PleyPalette.of(PleyTheme.Sunshine, PleyAccent.Gold)
        // "Who's watching?", signing in, the servers: Midnight in its own accent, whatever was shown before.
        Appearance.outside(context, true)
        assertSame(PleyPalette.Default, Appearance.paletteFor(outside = true))
        // A start there begins in Midnight too.
        Appearance.forget()
        Appearance.attach(context)
        assertSame(PleyPalette.Default, Appearance.palette)
        // Back inside the profile: its pick, here and at the next start.
        Appearance.showProfile(context, SERVER, 1)
        Appearance.outside(context, false)
        assertSame(pick, Appearance.paletteFor(outside = false))
        Appearance.forget()
        Appearance.attach(context)
        assertSame(pick, Appearance.palette)
    }

    @Test
    fun aPickBeforeAnyProfileIsTheDevicesOnly() {
        Appearance.attach(context)
        Appearance.pick(context, PleyTheme.Greenlight)
        assertSame(PleyPalette.of(PleyTheme.Greenlight, null), Appearance.palette)
        Appearance.showProfile(context, SERVER, 5)
        assertSame(PleyPalette.Default, Appearance.palette)
    }

    @Test
    fun anUnknownStoredIdShowsMidnightInItsOwnAccent() {
        context.getSharedPreferences("appearance", Context.MODE_PRIVATE).edit().putString("theme.$SERVER.7", "dusk").putString("accent.$SERVER.7", "teal").commit()
        Appearance.attach(context)
        Appearance.showProfile(context, SERVER, 7)
        assertSame(PleyPalette.Default, Appearance.palette)
    }

    private companion object {
        const val SERVER = "a3f0"
        const val OTHER_SERVER = "9c12"
    }
}
