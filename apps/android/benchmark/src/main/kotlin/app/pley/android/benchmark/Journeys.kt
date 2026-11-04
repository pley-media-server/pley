package app.pley.android.benchmark

import android.app.UiAutomation
import android.content.Intent
import android.content.pm.PackageManager
import android.graphics.Rect
import android.os.SystemClock
import android.view.InputDevice
import android.view.KeyCharacterMap
import android.view.KeyEvent
import android.view.ViewConfiguration
import androidx.benchmark.macro.MacrobenchmarkScope
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.uiautomator.By
import androidx.test.uiautomator.BySelector
import androidx.test.uiautomator.Direction
import androidx.test.uiautomator.StaleObjectException
import androidx.test.uiautomator.UiDevice
import androidx.test.uiautomator.UiObject2
import androidx.test.uiautomator.Until
import java.util.regex.Pattern
import kotlin.math.abs

/**
 * What the journeys find on screen: the app's test tags, which UI Automator
 * reads as resource ids (both interfaces' roots set testTagsAsResourceId,
 * and so do the players', in windows of their own).
 * Copies of app.pley.android.common.TestTags and of the screens' own tags
 * (ui/tv's and ui/phone's *_TAG and *_PAGE constants): this module sees
 * only the app it runs against.
 */
internal object Tags {
    const val HOME_ROWS = "pley.home.rows"
    const val LIBRARY_GRID = "pley.library.grid"
    const val ITEM_PLAY = "pley.item.play"
    const val TV_RAIL = "pley.tv.rail"

    const val TV_SERVERS = "pley.servers"
    const val TV_DEMO = "pley.servers.demo"
    const val TV_PROFILES = "pley.profiles"
    const val TV_ROW = "pley.tv.row."
    const val TV_ITEM = "pley.tv.item"
    const val TV_EPISODES = TV_ROW + "episode"
    const val TV_ITEM_PANEL = "pley.tv.item.panel"
    const val TV_ITEM_ACTIONS = "pley.tv.item.actions"
    const val TV_TILE_ACTIONS = "pley.tv.tile.actions"
    const val TV_PERSON = "pley.tv.person"
    const val TV_SEARCH_FIELD = "pley.tv.search.field"
    const val TV_SEARCH_RESULTS = "pley.tv.search.results"
    const val TV_SETTINGS = "pley.tv.settings"
    const val TV_SETTINGS_CATEGORY = "pley.tv.settings.category."
    const val TV_SETTINGS_PANE = "pley.tv.settings.pane"
    const val TV_CHOICES = "pley.tv.settings.choices"
    const val TV_PLAYLISTS_GRID = "pley.tv.playlists.grid"
    const val TV_PLAYLIST = "pley.tv.playlist"
    const val TV_PLAYER = "pley.tv.player"

    const val PHONE_DEMO = "pley.signin.demo"
    const val PHONE_CONTINUE = "pley.signin.continue"
    const val PHONE_TAB = "pley.phone.tab."
    const val PHONE_ITEM = "pley.item.page"
    const val PHONE_PERSON = "pley.person.page"
    const val PHONE_SEARCH_FIELD = "pley.search.field"
    const val PHONE_SEARCH_RESULTS = "pley.search.results"
    const val PHONE_PLAYLISTS_GRID = "pley.playlists.grid"
    const val PHONE_PLAYLIST = "pley.playlist"
    const val PHONE_ME = "pley.me.page"
    const val PHONE_SETTINGS = "pley.settings.page"
    const val PHONE_PLAYER = "phone-player"
}

/** What the demo server of benchmark builds (testing/fixtures) holds that the journeys go to. */
internal object Demo {
    /** The household's first profile, without a PIN. */
    const val PROFILE = "Alex"

    /** Its video libraries' ids, as the rail and Home's rows name them. */
    const val MOVIES = 1L
    const val SHOWS = 2L

    /** A video playlist. */
    const val PLAYLIST = "Favourites"

    /** A word in titles of every kind: movies, shows, episodes, albums and tracks. */
    const val QUERY = "harbor"

    /** Home's row of what was added to [library] last. */
    fun recentlyAdded(library: Long) = "recently_added:$library"
}

/** The remote's keys the journeys press. */
internal enum class Key(val code: Int) {
    UP(KeyEvent.KEYCODE_DPAD_UP),
    DOWN(KeyEvent.KEYCODE_DPAD_DOWN),
    LEFT(KeyEvent.KEYCODE_DPAD_LEFT),
    RIGHT(KeyEvent.KEYCODE_DPAD_RIGHT),
    CENTER(KeyEvent.KEYCODE_DPAD_CENTER),
    BACK(KeyEvent.KEYCODE_BACK),
}

/** Whether the device under test is a TV (Android TV, Google TV, Fire TV): the TV's journeys run there, the phone's elsewhere. */
internal fun isTv(): Boolean =
    InstrumentationRegistry.getInstrumentation().context.packageManager.hasSystemFeature(PackageManager.FEATURE_LEANBACK)

/** The TV app, as its launcher starts it. */
internal fun tvIntent(): Intent = Intent(Intent.ACTION_MAIN)
    .addCategory("android.intent.category.LEANBACK_LAUNCHER")
    .setClassName(TARGET, "app.pley.android.TvActivity")

/**
 * Steps a journey takes, each waiting for what it leads to rather than for
 * a set time, so a journey fails where it goes astray instead of walking on
 * through the wrong screens.
 */
internal open class Steps(protected val scope: MacrobenchmarkScope) {
    protected val device: UiDevice get() = scope.device

    /** Waits for what [selector] finds, failing with [what] after [timeoutMs]. */
    fun await(selector: BySelector, what: String, timeoutMs: Long = AWAIT_MS): UiObject2 =
        device.wait(Until.findObject(selector), timeoutMs) ?: error("Journey: no $what after ${timeoutMs / 1000} s")

    /** Waits for the first of [selectors] to show, failing with [what] after [timeoutMs]. */
    fun awaitAny(what: String, timeoutMs: Long, vararg selectors: BySelector) {
        val until = SystemClock.uptimeMillis() + timeoutMs
        while (SystemClock.uptimeMillis() < until) {
            if (selectors.any { device.hasObject(it) }) return
            SystemClock.sleep(POLL_MS)
        }
        error("Journey: no $what after ${timeoutMs / 1000} s")
    }

    /** One press of [key], then [restMs] for what it moved. */
    fun press(key: Key, restMs: Long = STEP_MS) {
        device.pressKeyCode(key.code)
        SystemClock.sleep(restMs)
    }

    /**
     * [key] held for [holdMs], as a remote's is: the press, the repeats
     * Android makes of a held key (the first, a long press, after the
     * long-press time, then one every 50 ms), and the release. Injected
     * rather than `input keyevent --duration`, which only API 33 and later
     * have: Fire OS 7 is API 28. Without accessibility ([remote]), as a
     * remote's keys come.
     */
    fun hold(key: Key, holdMs: Long) {
        val automation = remote()
        val down = SystemClock.uptimeMillis()
        fun send(action: Int, repeat: Int, flags: Int = 0) {
            val event = KeyEvent(down, SystemClock.uptimeMillis(), action, key.code, repeat, 0, KeyCharacterMap.VIRTUAL_KEYBOARD, 0, flags, InputDevice.SOURCE_DPAD)
            automation.injectInputEvent(event, false)
        }
        send(KeyEvent.ACTION_DOWN, 0)
        var next = down + ViewConfiguration.getKeyRepeatTimeout()
        var repeat = 0
        while (next < down + holdMs) {
            SystemClock.sleep((next - SystemClock.uptimeMillis()).coerceAtLeast(0))
            repeat++
            send(KeyEvent.ACTION_DOWN, repeat, if (repeat == 1) KeyEvent.FLAG_LONG_PRESS else 0)
            next += ViewConfiguration.getKeyRepeatDelay()
        }
        SystemClock.sleep((down + holdMs - SystemClock.uptimeMillis()).coerceAtLeast(0))
        send(KeyEvent.ACTION_UP, 0)
        SystemClock.sleep(STEP_MS)
    }

    /** One press of [key] as [hold] sends it: without accessibility, then [restMs]. */
    fun tap(key: Key, restMs: Long = STEP_MS) {
        val automation = remote()
        val down = SystemClock.uptimeMillis()
        for (action in listOf(KeyEvent.ACTION_DOWN, KeyEvent.ACTION_UP)) {
            val event = KeyEvent(down, SystemClock.uptimeMillis(), action, key.code, 0, 0, KeyCharacterMap.VIRTUAL_KEYBOARD, 0, 0, InputDevice.SOURCE_DPAD)
            automation.injectInputEvent(event, false)
        }
        SystemClock.sleep(restMs)
    }

    /**
     * Keys injected as a remote's come: with UiAutomator's accessibility
     * connection let go. While it's connected, Compose describes every
     * change of the screen to it (in a held scroll on a Google TV Streamer,
     * a fifth of the main thread), which nobody without an accessibility
     * service pays. The next step that looks for something on the screen
     * connects it again. The first call of a journey lets the screen settle
     * without it.
     */
    private fun remote(): UiAutomation {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val first = !remoteNow
        remoteNow = true
        return instrumentation.getUiAutomation(UiAutomation.FLAG_DONT_USE_ACCESSIBILITY).also {
            if (first) SystemClock.sleep(UNOBSERVED_SETTLE_MS)
        }
    }

    /** Whether this journey has sent keys without accessibility yet. */
    private var remoteNow = false

    /**
     * The element focus is on: the smallest of those that say so. The
     * window's own view says so too, and holds everything, so it never
     * counts: with only it, nothing in the page has focus.
     */
    fun focused(): UiObject2? = device.findObjects(By.focused(true))
        .mapNotNull { element -> element.boundsNow()?.let { element to it } }
        .filter { (_, bounds) -> !bounds.fillsTheScreen() }
        .minByOrNull { (_, bounds) -> bounds.area() }
        ?.first

    /**
     * Its bounds, or null when it went after it was found (the rail
     * closing, a row sliding): what's gone has no focus to give.
     */
    private fun UiObject2.boundsNow(): Rect? = try {
        visibleBounds
    } catch (_: StaleObjectException) {
        null
    }

    /**
     * Moves focus with the D-pad until it is on or in what [target] finds:
     * toward it while it shows, else [toward] (a row not composed yet lies
     * that way). False after [max] steps.
     */
    fun focusOn(target: BySelector, toward: Key, max: Int = FOCUS_STEPS): Boolean {
        repeat(max) {
            val goal = device.findObject(target)
            if (goal != null && holdsFocus(goal, target)) return true
            press(goal?.let(::directionTo) ?: toward)
        }
        return device.findObject(target)?.let { holdsFocus(it, target) } == true
    }

    /** [focusOn], failing with [what] when focus doesn't get there. */
    fun moveTo(target: BySelector, toward: Key, what: String, max: Int = FOCUS_STEPS) {
        check(focusOn(target, toward, max)) { "Journey: focus didn't reach $what (it is on ${focused()?.describe()})" }
    }

    /** Back until [selector] shows, at most [max] times. */
    fun backTo(selector: BySelector, what: String, max: Int = 4) {
        repeat(max) {
            if (device.hasObject(selector)) return
            press(Key.BACK, restMs = PAGE_MS)
        }
        await(selector, what)
    }

    /** Whether focus is on or in [goal]; not while it is going (looked for again at the next step). */
    private fun holdsFocus(goal: UiObject2, target: BySelector): Boolean = try {
        goal.isFocused || goal.hasObject(By.focused(true)) || focused()?.hasObject(target) == true
    } catch (_: StaleObjectException) {
        false
    }

    /** The way to [goal] from the focused element: up or down when they share columns, left or right when they share rows. */
    private fun directionTo(goal: UiObject2): Key? {
        val from = focused()?.boundsNow() ?: return null
        val to = goal.boundsNow() ?: return null
        val columns = from.left < to.right && to.left < from.right
        val rows = from.top < to.bottom && to.top < from.bottom
        val dx = to.centerX() - from.centerX()
        val dy = to.centerY() - from.centerY()
        return when {
            columns && !rows -> if (dy > 0) Key.DOWN else Key.UP
            rows && !columns -> if (dx > 0) Key.RIGHT else Key.LEFT
            abs(dx) > abs(dy) -> if (dx > 0) Key.RIGHT else Key.LEFT
            else -> if (dy > 0) Key.DOWN else Key.UP
        }
    }

    /** What scrolls in this: itself, or the first of its parts that does. */
    fun UiObject2.scroller(): UiObject2 = if (isScrollable) this else findObject(By.scrollable(true)) ?: this

    private fun Rect.area(): Int = width() * height()

    private fun Rect.fillsTheScreen(): Boolean =
        width() >= device.displayWidth * 0.95 && height() >= device.displayHeight * 0.95

    private fun UiObject2.describe(): String = resourceName ?: text ?: contentDescription ?: className

    companion object {
        const val AWAIT_MS = 10_000L

        /** Starting the demo server, signing in and the first screen. */
        const val SIGN_IN_MS = 45_000L

        /** A cold start's first screen; a page whose data the demo server makes. */
        const val START_MS = 15_000L

        /** After opening a page: its data from the demo server, its pictures. */
        const val PAGE_MS = 1_500L

        const val STEP_MS = 300L

        /** How long the screen takes to hear that accessibility went: Compose stops describing it. */
        const val UNOBSERVED_SETTLE_MS = 500L
        const val POLL_MS = 100L
        const val FOCUS_STEPS = 24
    }
}

/** The TV app's journeys, with the D-pad, against the demo server of benchmark builds. */
internal class TvJourney(scope: MacrobenchmarkScope) : Steps(scope) {
    /**
     * Clears the app, then signs in to the demo server: Choose your server,
     * then Use demo server, whose code the server approves at once. Home
     * follows, or Who's watching? where the household asks.
     */
    fun signIn() {
        device.executeShellCommand("pm clear $TARGET")
        scope.pressHome()
        scope.startActivityAndWait(tvIntent())
        await(By.res(Tags.TV_SERVERS), "Choose your server", START_MS)
        moveTo(By.res(Tags.TV_DEMO), Key.RIGHT, "Use demo server")
        press(Key.CENTER)
        awaitAny("Home or Who's watching? after signing in", SIGN_IN_MS, By.res(Tags.HOME_ROWS), By.res(Tags.TV_PROFILES))
        signedIn = true
    }

    /** [signIn] once in this run of the tests. */
    fun signedIn() {
        if (!signedIn) signIn()
    }

    /** The app started again, cold: the demo's household of three asks Who's watching? first. */
    fun restart() {
        scope.killProcess()
        scope.startActivityAndWait(tvIntent())
        awaitAny("Who's watching? or Home at a cold start", START_MS, By.res(Tags.TV_PROFILES), By.res(Tags.HOME_ROWS))
    }

    /** On Who's watching?, [name] chosen with the D-pad, then Home; nothing to do where Home shows already. */
    fun chooseProfile(name: String = Demo.PROFILE) {
        if (!device.hasObject(By.res(Tags.TV_PROFILES))) return
        moveTo(By.textContains(name), Key.RIGHT, "the profile $name")
        press(Key.CENTER, restMs = PAGE_MS)
        await(By.res(Tags.HOME_ROWS), "Home after choosing $name", START_MS)
    }

    /** Down from Home's hero into its first row. */
    fun intoRows() {
        await(By.res(Tags.HOME_ROWS), "Home")
        press(Key.DOWN, restMs = PAGE_MS)
    }

    /** The rail opened and [id]'s entry chosen ("home", "search", "library:1", "settings"…). */
    fun railTo(id: String) {
        moveTo(By.res(Tags.TV_RAIL), Key.LEFT, "the rail", max = RAIL_STEPS)
        moveTo(By.res("${Tags.TV_RAIL}.$id"), Key.DOWN, "the rail's $id")
        press(Key.CENTER, restMs = PAGE_MS)
    }

    /** The rail opened from the page and closed again with Right, back to where focus was. */
    fun railOpenAndClose() {
        moveTo(By.res(Tags.TV_RAIL), Key.LEFT, "the rail", max = RAIL_STEPS)
        press(Key.RIGHT, restMs = PAGE_MS)
    }

    /**
     * Home: OK held on the first row's tile for its actions, then down
     * through the rows, a few tiles along each, and Back to the hero.
     */
    fun walkHome(rows: Int = 4, tiles: Int = 6) {
        intoRows()
        hold(Key.CENTER, LONG_PRESS_MS)
        // A page of actions, or a panel over Home: the tile's actions, which every page shares, or an item page's.
        val actions = By.res(Pattern.compile(listOf(Tags.TV_ITEM_ACTIONS, Tags.TV_TILE_ACTIONS, Tags.TV_ITEM_PANEL).joinToString("|") { Pattern.quote(it) }))
        await(actions, "a tile's actions after OK held", START_MS)
        press(Key.BACK, restMs = PAGE_MS)
        check(device.wait(Until.gone(actions), AWAIT_MS)) { "Journey: Back didn't close a tile's actions" }
        await(By.res(Tags.HOME_ROWS), "Home after a tile's actions")
        repeat(rows) {
            repeat(tiles) { press(Key.RIGHT, restMs = TILE_MS) }
            press(Key.DOWN)
        }
        press(Key.BACK, restMs = PAGE_MS)
    }

    /**
     * A show's page, from Home's row of shows: along its episodes, OK held
     * on one for its actions, its rows down to the cast, and a person's
     * page from there.
     */
    fun walkShow() {
        railTo("home")
        moveTo(By.res(Tags.TV_ROW + Demo.recentlyAdded(Demo.SHOWS)), Key.DOWN, "Home's row of shows")
        press(Key.CENTER, restMs = PAGE_MS)
        await(By.res(Tags.TV_ITEM), "a show's page", START_MS)
        moveTo(By.res(Tags.TV_EPISODES), Key.DOWN, "the show's episodes")
        repeat(4) { press(Key.RIGHT, restMs = TILE_MS) }
        hold(Key.CENTER, LONG_PRESS_MS)
        await(By.res(Tags.TV_ITEM_PANEL), "an episode's actions after OK held")
        press(Key.BACK, restMs = PAGE_MS)
        moveTo(By.res(Tags.TV_ROW + "cast"), Key.DOWN, "the show's cast")
        repeat(2) { press(Key.RIGHT, restMs = TILE_MS) }
        press(Key.CENTER, restMs = PAGE_MS)
        await(By.res(Tags.TV_PERSON), "a person's page", START_MS)
        repeat(3) { press(Key.DOWN, restMs = TILE_MS) }
        backTo(By.res(Tags.TV_ITEM), "the show's page after the person's")
        backTo(By.res(Tags.HOME_ROWS), "Home after the show's page")
    }

    /** Search: [Demo.QUERY] typed into the field, then down into the result rows and along them. */
    fun walkSearch() {
        railTo("search")
        val field = await(By.res(Tags.TV_SEARCH_FIELD), "Search's field")
        // Typed through the field's own text, as a screen reader would: the keyboard differs from one TV to the next.
        val text = field.findObject(By.clazz(EDIT_TEXT)) ?: error("Journey: Search's field has no text to set")
        text.text = Demo.QUERY
        val rows = By.res(Tags.TV_SEARCH_RESULTS).hasDescendant(By.res(Pattern.compile(Pattern.quote(Tags.TV_ROW) + ".+")))
        await(rows, "search results for ${Demo.QUERY}", START_MS)
        moveTo(By.res(Tags.TV_SEARCH_RESULTS), Key.DOWN, "the results")
        repeat(2) {
            repeat(4) { press(Key.RIGHT, restMs = TILE_MS) }
            press(Key.DOWN)
        }
    }

    /** A library's grid from the rail: in from its chips, down and back up. */
    fun walkLibrary(library: Long = Demo.MOVIES, rows: Int = 6) {
        railTo("library:$library")
        intoGrid()
        repeat(rows) { press(Key.DOWN, restMs = TILE_MS) }
        repeat(rows) { press(Key.UP, restMs = TILE_MS) }
    }

    /** The open library's grid, focused from its chips. */
    fun intoGrid() {
        await(By.res(Tags.LIBRARY_GRID), "a library's grid", START_MS)
        moveTo(By.res(Tags.LIBRARY_GRID), Key.DOWN, "the library's grid")
    }

    /** The Collections page from the rail, its first shelf focused, then a collection's page. */
    fun walkCollections() {
        railTo("collections")
        val shelf = By.res(Pattern.compile(Pattern.quote(Tags.TV_ROW + "library:") + ".+"))
        await(shelf, "the Collections page", START_MS)
        moveTo(shelf, Key.DOWN, "a shelf of collections")
        press(Key.CENTER, restMs = PAGE_MS)
        await(By.text(COLLECTION_LABEL), "a collection's page", START_MS)
        repeat(3) { press(Key.DOWN, restMs = TILE_MS) }
        press(Key.BACK, restMs = PAGE_MS)
    }

    /** The Playlists page from the rail, then the demo's video playlist. */
    fun walkPlaylists() {
        railTo("playlists")
        await(By.res(Tags.TV_PLAYLISTS_GRID), "the Playlists page", START_MS)
        moveTo(By.textContains(Demo.PLAYLIST), Key.DOWN, "the playlist ${Demo.PLAYLIST}")
        press(Key.CENTER, restMs = PAGE_MS)
        await(By.res(Tags.TV_PLAYLIST), "a playlist's page", START_MS)
        repeat(3) { press(Key.DOWN, restMs = TILE_MS) }
        press(Key.BACK, restMs = PAGE_MS)
    }

    /**
     * Settings from the rail: Playback's Quality, whose panel opens and
     * closes unchanged, then Profile's Sort libraries, the Libraries page.
     */
    fun walkSettings() {
        railTo("settings")
        await(By.res(Tags.TV_SETTINGS), "Settings")
        moveTo(By.res(Tags.TV_SETTINGS_CATEGORY + PLAYBACK), Key.DOWN, "Settings' Playback")
        moveTo(By.res(Tags.TV_SETTINGS_PANE), Key.RIGHT, "Playback's settings")
        moveTo(By.textContains(QUALITY), Key.DOWN, "Playback's Quality")
        press(Key.CENTER, restMs = PAGE_MS)
        await(By.res(Tags.TV_CHOICES), "Quality's panel")
        repeat(2) { press(Key.DOWN) }
        press(Key.BACK, restMs = PAGE_MS)
        moveTo(By.res(Tags.TV_SETTINGS_CATEGORY + PROFILE), Key.LEFT, "Settings' Profile")
        moveTo(By.res(Tags.TV_SETTINGS_PANE), Key.RIGHT, "Profile's settings")
        moveTo(By.textContains(SORT_LIBRARIES), Key.DOWN, "Profile's Sort libraries")
        press(Key.CENTER, restMs = PAGE_MS)
        await(By.text(LIBRARIES_TITLE), "the Libraries page", START_MS)
        repeat(2) { press(Key.RIGHT, restMs = TILE_MS) }
        press(Key.BACK, restMs = PAGE_MS)
    }

    /** The player from Home's hero: a moment of playing, the controls, then back to Home. */
    fun walkPlayer() {
        railTo("home")
        await(By.res(Tags.HOME_ROWS), "Home")
        moveTo(By.text(Pattern.compile("$RESUME|$PLAY")), Key.UP, "Home's Resume")
        press(Key.CENTER, restMs = PAGE_MS)
        await(By.res(Tags.TV_PLAYER), "the player", START_MS)
        SystemClock.sleep(PLAYING_MS)
        // OK shows the controls; Right steps a seek that waits for OK; Back lets it go, then leaves.
        press(Key.CENTER, restMs = PAGE_MS)
        repeat(2) { press(Key.RIGHT) }
        press(Key.DOWN)
        backTo(By.res(Tags.HOME_ROWS), "Home after the player")
    }

    private companion object {
        var signedIn = false

        /** From a row's far end, along it to its first tile and on into the rail. */
        const val RAIL_STEPS = 40
        const val TILE_MS = 150L
        const val LONG_PRESS_MS = 1_000L
        const val PLAYING_MS = 5_000L
        const val EDIT_TEXT = "android.widget.EditText"

        // Settings' categories (ui/tv's settings Category names).
        const val PLAYBACK = "Playback"
        const val PROFILE = "Profile"

        // ui/tv's words (res/values/strings_tv_*.xml).
        const val COLLECTION_LABEL = "Collection"
        const val QUALITY = "Quality"
        const val SORT_LIBRARIES = "Sort libraries"
        const val LIBRARIES_TITLE = "Libraries"
        const val RESUME = "Resume"
        const val PLAY = "Play"
    }
}

/** The phone app's journeys, by touch, against the demo server of benchmark builds. */
internal class PhoneJourney(scope: MacrobenchmarkScope) : Steps(scope) {
    /**
     * Clears the app, then signs in to the demo server: Use demo server,
     * then Continue in browser, whose page signs in at once and comes back
     * to the app. A phone without a browser signs in with a code instead,
     * which the demo server approves by itself. Once in this run of the
     * tests: the browser is slow to open.
     */
    fun signedIn() {
        if (signedIn) return
        device.executeShellCommand("pm clear $TARGET")
        scope.pressHome()
        scope.startActivityAndWait()
        await(By.res(Tags.PHONE_DEMO), "Use demo server", START_MS).click()
        awaitAny("Continue in browser or Home", SIGN_IN_MS, By.res(Tags.PHONE_CONTINUE), By.res(Tags.PHONE_TAB + "Home"))
        device.findObject(By.res(Tags.PHONE_CONTINUE))?.click()
        check(device.wait(Until.hasObject(By.res(Tags.PHONE_TAB + "Home")), SIGN_IN_MS)) {
            "Journey: signing in didn't come back from the browser (${device.currentPackageName} shows): " +
                "on a new device, open the browser once first, past its welcome pages"
        }
        signedIn = true
    }

    /** The app started again, cold, on Home. */
    fun restart() {
        scope.killProcess()
        scope.startActivityAndWait()
        await(By.res(Tags.HOME_ROWS), "Home at a cold start", START_MS)
    }

    /** The tab [name] (Home, Libraries, Search, Playlists, Me) chosen in the bar or the rail. */
    fun tab(name: String) {
        await(By.res(Tags.PHONE_TAB + name), "the tab $name").click()
        SystemClock.sleep(PAGE_MS)
    }

    /** Home's page flung down and back up, then its first row along and back. */
    fun flingHome(times: Int = 2) {
        val page = await(By.res(Tags.HOME_ROWS), "Home")
        page.setGestureMargin(device.displayWidth / 5)
        repeat(times) { page.fling(Direction.DOWN) }
        repeat(times) { page.fling(Direction.UP) }
        val row = await(By.res(Pattern.compile(Pattern.quote(Tags.HOME_ROWS + ".") + ".+")), "a row on Home")
        row.setGestureMargin(device.displayWidth / 10)
        repeat(times) { row.fling(Direction.RIGHT) }
        repeat(times) { row.fling(Direction.LEFT) }
    }

    /**
     * A show's page from Home's row of shows, down to its cast and a
     * person's page, then the player from the show's Play.
     */
    fun walkShow() {
        tab("Home")
        val page = await(By.res(Tags.HOME_ROWS), "Home")
        val shows = page.scrollUntil(Direction.DOWN, Until.findObject(By.res(Tags.HOME_ROWS + "." + Demo.recentlyAdded(Demo.SHOWS))))
            ?: error("Journey: no row of shows on Home")
        (shows.children.firstOrNull() ?: error("Journey: Home's row of shows is empty")).click()
        val item = await(By.res(Tags.PHONE_ITEM), "a show's page", START_MS)
        val cast = item.scroller().scrollUntil(Direction.DOWN, Until.findObject(By.res(Tags.HOME_ROWS + ".cast")))
            ?: error("Journey: no cast on the show's page")
        (cast.children.firstOrNull() ?: error("Journey: the show's cast is empty")).click()
        await(By.res(Tags.PHONE_PERSON), "a person's page", START_MS).scroller().fling(Direction.DOWN)
        device.pressBack()
        val again = await(By.res(Tags.PHONE_ITEM), "the show's page again")
        val play = again.scroller().scrollUntil(Direction.UP, Until.findObject(By.res(Tags.ITEM_PLAY)))
            ?: error("Journey: no Play on the show's page")
        play.click()
        await(By.res(Tags.PHONE_PLAYER), "the player", START_MS)
        SystemClock.sleep(PLAYING_MS)
        // A tap shows the controls; Back leaves the player, then the show's page.
        device.click(device.displayWidth / 2, device.displayHeight / 2)
        SystemClock.sleep(PAGE_MS)
        device.pressBack()
        await(By.res(Tags.PHONE_ITEM), "the show's page after the player", START_MS)
        device.pressBack()
    }

    /** The Libraries tab, then [name]'s page with its grid. */
    fun openLibrary(name: String = MOVIES) {
        tab("Libraries")
        await(By.textStartsWith(name), "the library $name").click()
        await(By.res(Tags.LIBRARY_GRID), "$name's grid", START_MS)
        SystemClock.sleep(PAGE_MS)
    }

    /** The open library's grid flung down [times] and back up. */
    fun flingGrid(times: Int = 4) {
        val grid = await(By.res(Tags.LIBRARY_GRID), "a library's grid")
        grid.setGestureMargin(device.displayWidth / 5)
        repeat(times) { grid.fling(Direction.DOWN) }
        repeat(times) { grid.fling(Direction.UP) }
    }

    /**
     * Search: [Demo.QUERY] typed, then its result rows flung down, which
     * puts the keyboard away, and back. The flings are swipes over the
     * screen's upper part: one that starts on the keyboard would type.
     */
    fun walkSearch() {
        tab("Search")
        await(By.res(Tags.PHONE_SEARCH_FIELD), "Search's field").text = Demo.QUERY
        await(By.res(Tags.PHONE_SEARCH_RESULTS), "search results for ${Demo.QUERY}", START_MS)
        val x = device.displayWidth / 2
        val (low, high) = device.displayHeight * 45 / 100 to device.displayHeight * 15 / 100
        device.swipe(x, low, x, high, FLING_STEPS)
        SystemClock.sleep(PAGE_MS)
        device.swipe(x, high, x, low, FLING_STEPS)
        SystemClock.sleep(PAGE_MS)
    }

    /** The Playlists tab, then the demo's video playlist. */
    fun walkPlaylists() {
        tab("Playlists")
        await(By.res(Tags.PHONE_PLAYLISTS_GRID), "the Playlists page", START_MS)
        await(By.textStartsWith(Demo.PLAYLIST), "the playlist ${Demo.PLAYLIST}").click()
        await(By.res(Tags.PHONE_PLAYLIST), "a playlist's page", START_MS)
        SystemClock.sleep(PAGE_MS)
        device.pressBack()
    }

    /** The Me tab, then Settings. */
    fun walkMe() {
        tab("Me")
        await(By.res(Tags.PHONE_ME), "Me")
        await(By.textStartsWith(SETTINGS), "Settings on Me").click()
        await(By.res(Tags.PHONE_SETTINGS), "Settings").scroller().fling(Direction.DOWN)
        device.pressBack()
    }

    private companion object {
        var signedIn = false
        const val PLAYING_MS = 5_000L

        /** A swipe's steps (5 ms each): quick enough to fling. */
        const val FLING_STEPS = 4

        // The demo's library and ui/phone's words (res/values/strings_*.xml).
        const val MOVIES = "Movies"
        const val SETTINGS = "Settings"
    }
}
