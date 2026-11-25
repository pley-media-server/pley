package app.pley.android.player.music

import android.app.Application
import android.content.Context
import android.content.ContextWrapper
import android.content.pm.ApplicationInfo
import android.content.pm.PackageInfo
import android.content.pm.SigningInfo
import android.os.Bundle
import android.os.Looper
import android.os.Process
import android.provider.Settings
import androidx.media3.common.MediaItem
import androidx.media3.common.Player
import androidx.media3.common.util.UnstableApi
import androidx.media3.session.CustomCommandPendingIntentBuilder
import androidx.media3.session.LibraryResult
import androidx.media3.session.MediaBrowser
import androidx.media3.session.MediaController
import androidx.media3.session.MediaLibraryService.MediaLibrarySession
import androidx.media3.session.SessionCommand
import androidx.media3.session.SessionError
import androidx.test.core.app.ApplicationProvider
import app.pley.android.model.ItemKind
import app.pley.android.model.ItemList
import app.pley.android.model.ItemSummary
import app.pley.android.model.SearchResults
import app.pley.android.model.caps.ClientCaps
import app.pley.android.player.PlayerGraph
import app.pley.android.player.PlayerGraphOwner
import app.pley.android.player.music.library.MusicCallers
import app.pley.android.player.music.library.MusicId
import app.pley.android.player.session.SessionReporter
import app.pley.android.testing.network.FakePleyApi
import app.pley.android.testing.player.engine.FakePlayerEngine
import com.google.common.util.concurrent.ListenableFuture
import java.util.concurrent.ExecutionException
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.android.controller.ServiceController
import org.robolectric.annotation.Config
import org.robolectric.annotation.Implementation
import org.robolectric.annotation.Implements
import org.robolectric.shadows.ShadowApplicationPackageManager
import org.robolectric.shadows.ShadowBinder

/**
 * Who the music service answers, through the service itself and Media3's
 * own session: each caller is a Media3 browser holding the session's
 * token, under the name it gives and from the uid it asks from, the
 * notification's controller connected as the service connects it.
 *
 * Media3 takes a name it can't check (an app this one can't see), and
 * hides a controller named System UI behind the notification's: it lets it
 * in without asking the service, and hands it to the service's answers as
 * the notification's own. An app holding the token, as one with access to
 * notifications does, can take that name: it is refused the library and
 * picks nothing to play, and still plays, pauses and skips. So is any app
 * that starts the service, which is exported, with a request made as the
 * notification's buttons make theirs: Media3 has its notification's
 * controller send it. The app, the system's own callers and Android Auto
 * are answered as before.
 *
 * The car and Bluetooth browse through the older protocol on a device; a
 * Media3 browser stands for each here, checked by the same name and uid.
 */
@RunWith(RobolectricTestRunner::class)
@Config(application = MusicServiceCallersTest.App::class, shadows = [MusicServiceCallersTest.SignedPackages::class])
@androidx.annotation.OptIn(UnstableApi::class)
class MusicServiceCallersTest {
    /** The app as the service finds it: the owner of the player graph. */
    class App : Application(), PlayerGraphOwner {
        override lateinit var playerGraph: PlayerGraph
    }

    /** The package manager, which also says an app in [signedByGoogle] has the certificate asked about: Robolectric's answers for none. */
    @Implements(className = "android.app.ApplicationPackageManager", isInAndroidSdk = false)
    open class SignedPackages : ShadowApplicationPackageManager() {
        @Implementation
        protected fun hasSigningCertificate(packageName: String, certificate: ByteArray, type: Int): Boolean = packageName in signedByGoogle
    }

    private val app: App = ApplicationProvider.getApplicationContext()
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    private val api = FakePleyApi()
    private val album = (1..3L).map { track(it) }
    private val playback = MusicPlayback(
        MusicEnvironment(
            api = { api },
            caps = { ClientCaps() },
            clientId = { "android-test-music" },
            reporter = SessionReporter(scope),
            output = { EngineMusicOutput(FakePlayerEngine(), scope) },
        ),
        scope,
    )
    private lateinit var service: ServiceController<MusicService>
    private lateinit var session: MediaLibrarySession
    private val browsers = mutableListOf<MediaBrowser>()

    // Uids no app of the test has, so none collides with the test's own.
    // The stranger's is no app's the package manager knows: an app this one can't see, as most are.
    private val strangerUid = Process.myUid() + 1
    private val systemUiUid = Process.myUid() + 2
    private val carUid = Process.myUid() + 3
    private val watchUid = Process.myUid() + 4

    @Before
    fun setUp() {
        // The server's answers: every album has the three tracks, and every search finds them.
        api.answer("children", ItemList(album))
        api.answer("search", SearchResults(items = album))
        install(SYSTEM_UI, systemUiUid, system = true)
        app.playerGraph = MusicServiceGraph(app, playback, api)
        MusicHub.playback = playback
        service = Robolectric.buildService(MusicService::class.java).create()
        session = service.get().sessions.single() as MediaLibrarySession
        // What Media3 hides a controller named System UI behind.
        assertNotNull(session.mediaNotificationControllerInfo)
    }

    @After
    fun tearDown() {
        browsers.forEach { it.release() }
        service.destroy()
        scope.cancel()
        MusicHub.playback = null
        signedByGoogle.clear()
        ShadowBinder.reset()
    }

    @Test
    fun anAppThatTakesSystemUisNameIsLetInAsTheNotificationsAndReadsNothingOfTheLibrary() {
        val fake = browser(SYSTEM_UI, strangerUid)

        // Media3 let it in without asking the service who it is, with all the notification's controller may do: what
        // refuses it below is the service's answer, not a command it wasn't given.
        assertTrue(fake.isConnected)
        for (command in LIBRARY_COMMANDS) assertTrue("command $command", fake.isSessionCommandAvailable(command))
        assertTrue(session.connectedControllers.none { it.packageName == SYSTEM_UI })

        assertEquals(DENIED, done(fake.getLibraryRoot(null)).resultCode)
        assertEquals(DENIED, done(fake.getChildren(MusicId.Root.text, 0, PAGE, null)).resultCode)
        assertEquals(DENIED, done(fake.getItem(MusicId.Artists.text)).resultCode)
        assertEquals(DENIED, done(fake.search("deep", null)).resultCode)
        assertEquals(DENIED, done(fake.getSearchResult("deep", 0, PAGE, null)).resultCode)
        // Subscribing reads the node subscribed to, by Media3's default.
        assertEquals(DENIED, done(fake.subscribe(MusicId.Root.text, null)).resultCode)
        // Nothing was asked of the server for it.
        assertEquals(emptyList<FakePleyApi.Call>(), api.calls)
    }

    @Test
    fun anAppThatTakesSystemUisNamePicksNothingToPlay() {
        val fake = browser(SYSTEM_UI, strangerUid)
        assertTrue(fake.availableCommands.containsAny(Player.COMMAND_SET_MEDIA_ITEM, Player.COMMAND_CHANGE_MEDIA_ITEMS))

        // An album by its id, alone and as a list, then what the Assistant would have heard.
        fake.setMediaItem(MediaItem.Builder().setMediaId(MusicId.Album(ALBUM).text).build())
        settle()
        fake.setMediaItems(listOf(MediaItem.Builder().setMediaId(MusicId.Album(ALBUM).text).build()), 0, 0L)
        settle()
        fake.setMediaItem(MediaItem.Builder().setRequestMetadata(MediaItem.RequestMetadata.Builder().setSearchQuery("deep water").build()).build())
        settle()

        assertEquals(emptyList<Long>(), queued())
        assertEquals(emptyList<FakePleyApi.Call>(), api.calls)
    }

    @Test
    fun anAppThatTakesSystemUisNameStillPlaysPausesAndSkips() {
        // What the phone plays: an album, waiting for Play.
        playback.cue(album)
        val fake = browser(SYSTEM_UI, strangerUid)

        fake.play()
        await { playback.nowPlaying.value?.wantsPlay == true }
        assertEquals(true, playback.nowPlaying.value?.wantsPlay)

        fake.seekToNextMediaItem()
        await { playback.queueState.value.index == 1 }
        assertEquals(1, playback.queueState.value.index)

        fake.pause()
        await { playback.nowPlaying.value?.wantsPlay == false }
        assertEquals(false, playback.nowPlaying.value?.wantsPlay)
    }

    @Test
    fun anAppThatStartsTheServiceAsTheNotificationsButtonsDoPicksNothingToPlay() {
        // A button of the notification is a player's command in the service's own intent, an item to play among them.
        val picked = MediaItem.Builder().setMediaId(MusicId.Album(ALBUM).text).build()
        start(SessionCommand(PLAYER_COMMAND + Player.COMMAND_SET_MEDIA_ITEM, Bundle().apply { putBundle(COMMAND_PARAMETER, picked.toBundle()) }))
        settle()
        assertEquals(emptyList<Long>(), queued())
        assertEquals(emptyList<FakePleyApi.Call>(), api.calls)

        // Its Next, as the notification's own, still skips.
        playback.cue(album)
        start(SessionCommand(PLAYER_COMMAND + Player.COMMAND_SEEK_TO_NEXT_MEDIA_ITEM, Bundle.EMPTY))
        await { playback.queueState.value.index == 1 }
        assertEquals(1, playback.queueState.value.index)
    }

    @Test
    fun theAppsOwnBrowserReadsTheLibraryAndPicksWhatPlays() {
        val own = browser()

        assertEquals(MusicId.Root.text, done(own.getLibraryRoot(null)).value?.mediaId)
        assertTrue(MusicId.Artists.text in tabsOf(own))
        assertEquals(MusicId.Artists.text, done(own.getItem(MusicId.Artists.text)).value?.mediaId)
        assertEquals(LibraryResult.RESULT_SUCCESS, done(own.search("deep", null)).resultCode)
        assertEquals(album.map { it.title }, done(own.getSearchResult("deep", 0, PAGE, null)).value.orEmpty().map { it.mediaMetadata.title.toString() })

        own.setMediaItem(MediaItem.Builder().setMediaId(MusicId.Album(ALBUM).text).build())
        await { queued().isNotEmpty() }
        assertEquals(album.map { it.id }, queued())
    }

    @Test
    fun theSystemsOwnCallersStillReadTheLibrary() {
        // Bluetooth lists the library on a car's or a speaker's own screen, from its system uid; Media3 trusts it for
        // its permission to control media.
        install(BLUETOOTH, Process.BLUETOOTH_UID, system = true)
        shadowOf(app).grantPermissions(Process.myPid(), Process.BLUETOOTH_UID, MEDIA_CONTENT_CONTROL)
        val bluetooth = browser(BLUETOOTH, Process.BLUETOOTH_UID)

        assertEquals(MusicId.Root.text, done(bluetooth.getLibraryRoot(null)).value?.mediaId)
        assertTrue(MusicId.Artists.text in tabsOf(bluetooth))
    }

    @Test
    fun androidAutoStillReadsTheLibraryAndPicksWhatPlays() {
        install(MusicCallers.ANDROID_AUTO, carUid)
        signedByGoogle += MusicCallers.ANDROID_AUTO
        // Android Auto has access to notifications, which Media3 trusts it for.
        listensToNotifications(MusicCallers.ANDROID_AUTO)
        val car = browser(MusicCallers.ANDROID_AUTO, carUid)

        assertEquals(MusicId.Root.text, done(car.getLibraryRoot(null)).value?.mediaId)
        assertTrue(MusicId.Artists.text in tabsOf(car))

        car.setMediaItem(MediaItem.Builder().setMediaId(MusicId.Album(ALBUM).text).build())
        await { queued().isNotEmpty() }
        assertEquals(album.map { it.id }, queued())
    }

    @Test
    fun anAppWithAccessToNotificationsUnderItsOwnNameOnlyPlaysPausesAndSkips() {
        install(WATCH, watchUid)
        listensToNotifications(WATCH)
        val watch = browser(WATCH, watchUid)

        assertTrue(watch.availableCommands.containsAny(Player.COMMAND_PLAY_PAUSE))
        assertFalse(watch.availableCommands.containsAny(Player.COMMAND_SET_MEDIA_ITEM, Player.COMMAND_CHANGE_MEDIA_ITEMS, Player.COMMAND_SEEK_TO_MEDIA_ITEM))
        for (command in LIBRARY_COMMANDS) assertFalse("command $command", watch.isSessionCommandAvailable(command))
        assertEquals(DENIED, done(watch.getLibraryRoot(null)).resultCode)
        assertEquals(emptyList<FakePleyApi.Call>(), api.calls)
    }

    @Test
    fun anAppThatTakesAndroidAutosNameIsNoCar() {
        install(MusicCallers.ANDROID_AUTO, carUid)
        signedByGoogle += MusicCallers.ANDROID_AUTO
        listensToNotifications(MusicCallers.ANDROID_AUTO)
        // Media3 trusts the name, for the access to notifications the real one has; the name isn't the asking uid's.
        val impostor = browser(MusicCallers.ANDROID_AUTO, strangerUid)

        assertFalse(impostor.availableCommands.containsAny(Player.COMMAND_SET_MEDIA_ITEM, Player.COMMAND_CHANGE_MEDIA_ITEMS))
        for (command in LIBRARY_COMMANDS) assertFalse("command $command", impostor.isSessionCommandAvailable(command))
    }

    @Test
    fun anAppWithoutAccessToNotificationsIsRefused() {
        // Under its own name, and under the app's with the hint Media3 knows its notification's controller by.
        val asNotification = Bundle().apply { putBoolean(MediaController.KEY_MEDIA_NOTIFICATION_CONTROLLER_FLAG, true) }

        assertRefused(connecting(STRANGER, strangerUid))
        assertRefused(connecting(app.packageName, strangerUid, asNotification))
    }

    /** A browser holding the session's token, as an app that asked the platform session for it does: under [name], asking from [uid]. */
    private fun connecting(name: String, uid: Int, hints: Bundle = Bundle.EMPTY): ListenableFuture<MediaBrowser> {
        // The uid Media3 reads as the connection is asked for, in the call itself.
        ShadowBinder.setCallingUid(uid)
        try {
            return MediaBrowser.Builder(Named(app, name), session.token).setConnectionHints(hints).buildAsync()
        } finally {
            ShadowBinder.reset()
        }
    }

    private fun browser(name: String = app.packageName, uid: Int = Process.myUid()): MediaBrowser = done(connecting(name, uid)).also { browsers += it }

    /** Starts the service with [command], as the intent of a button of the notification does, and as any app can. */
    private fun start(command: SessionCommand) {
        service.get().onStartCommand(CustomCommandPendingIntentBuilder.createCustomCommandIntent(app, command, session.id, MusicService::class.java), 0, 1)
    }

    private fun assertRefused(connecting: ListenableFuture<MediaBrowser>) {
        await { connecting.isDone }
        val refused = assertThrows(ExecutionException::class.java) { connecting.get(1, TimeUnit.SECONDS) }
        assertTrue(refused.cause.toString(), refused.cause is SecurityException)
    }

    /** The ids of the library's tabs, as [browser] is given them. */
    private fun tabsOf(browser: MediaBrowser): List<String> = done(browser.getChildren(MusicId.Root.text, 0, PAGE, null)).value.orEmpty().map { it.mediaId }

    /** The ids of the tracks the music holds. */
    private fun queued(): List<Long> = playback.queueState.value.tracks.map { it.id }

    /** [future]'s result, the main thread run meanwhile: Media3's and the service's work is on it. */
    private fun <T> done(future: ListenableFuture<T>): T {
        await { future.isDone }
        return future.get(1, TimeUnit.SECONDS)
    }

    /** Runs the main thread until [done], the library's answers made off it given time to come back: 5 s at most, a slow machine's. */
    private fun await(done: () -> Boolean) {
        val until = System.currentTimeMillis() + 5_000
        while (!done() && System.currentTimeMillis() < until) {
            shadowOf(Looper.getMainLooper()).idle()
            if (!done()) Thread.sleep(2)
        }
    }

    /** Lets what was asked be done, or not: an album picked is in the queue well within this. */
    private fun settle() = repeat(20) {
        shadowOf(Looper.getMainLooper()).idle()
        Thread.sleep(5)
    }

    private fun install(name: String, uid: Int, system: Boolean = false) {
        val info = PackageInfo().apply {
            packageName = name
            applicationInfo = ApplicationInfo().apply {
                packageName = name
                this.uid = uid
                if (system) flags = ApplicationInfo.FLAG_SYSTEM
            }
            signingInfo = SigningInfo()
        }
        shadowOf(app.packageManager).installPackage(info)
    }

    /** [name] has access to notifications, as the system's settings list it. */
    private fun listensToNotifications(name: String) {
        Settings.Secure.putString(app.contentResolver, "enabled_notification_listeners", "$name/.Listener")
    }

    private fun track(id: Long) = ItemSummary(
        id = id,
        libraryId = 3,
        kind = ItemKind.Track,
        title = "Track $id",
        parentId = ALBUM,
        parentTitle = "Deep Water",
        grandparentTitle = "Harbour Lights",
        durationMs = 180_000,
    )

    /** The app's context as another app's would be: a browser made with it gives the session [name] as its own. */
    private class Named(base: Context, private val name: String) : ContextWrapper(base) {
        override fun getPackageName(): String = name
    }

    private companion object {
        const val SYSTEM_UI = "com.android.systemui"
        const val BLUETOOTH = "com.android.bluetooth"
        const val WATCH = "com.example.watch"
        const val STRANGER = "com.example.stranger"
        const val MEDIA_CONTENT_CONTROL = "android.permission.MEDIA_CONTENT_CONTROL"
        const val DENIED = SessionError.ERROR_PERMISSION_DENIED
        const val ALBUM = 50L
        const val PAGE = 20

        /** How Media3 names a player's command sent as a custom one, and where its parameter goes (CommandButton). */
        const val PLAYER_COMMAND = "androidx.media3.session.PLAYER_COMMAND_"
        const val COMMAND_PARAMETER = "androidx.media3.session.CUSTOM_COMMAND_PARAMETER"

        val LIBRARY_COMMANDS = listOf(
            SessionCommand.COMMAND_CODE_LIBRARY_GET_LIBRARY_ROOT,
            SessionCommand.COMMAND_CODE_LIBRARY_GET_CHILDREN,
            SessionCommand.COMMAND_CODE_LIBRARY_GET_ITEM,
            SessionCommand.COMMAND_CODE_LIBRARY_SEARCH,
            SessionCommand.COMMAND_CODE_LIBRARY_GET_SEARCH_RESULT,
            SessionCommand.COMMAND_CODE_LIBRARY_SUBSCRIBE,
        )
    }
}

/** The apps the package manager of [MusicServiceCallersTest.SignedPackages] says are signed with the certificate asked about. */
private val signedByGoogle = mutableSetOf<String>()
