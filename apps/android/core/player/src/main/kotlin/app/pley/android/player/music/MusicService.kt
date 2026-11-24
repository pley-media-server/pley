package app.pley.android.player.music

import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Bundle
import android.os.Process
import android.provider.MediaStore
import android.view.KeyEvent
import androidx.annotation.OptIn
import androidx.core.content.IntentCompat
import androidx.media3.common.C
import androidx.media3.common.MediaItem
import androidx.media3.common.PlaybackException
import androidx.media3.common.Player
import androidx.media3.common.util.UnstableApi
import androidx.media3.datasource.DataSourceBitmapLoader
import androidx.media3.session.CacheBitmapLoader
import androidx.media3.session.LibraryResult
import androidx.media3.session.CommandButton
import androidx.media3.session.MediaConstants
import androidx.media3.session.MediaLibraryService
import androidx.media3.session.MediaLibraryService.LibraryParams
import androidx.media3.session.MediaLibraryService.MediaLibrarySession
import androidx.media3.session.MediaNotification
import androidx.media3.session.MediaSession
import androidx.media3.session.SessionCommand
import androidx.media3.session.SessionError
import androidx.media3.session.SessionResult
import app.pley.android.common.AppLanguage
import app.pley.android.model.ItemKind
import app.pley.android.model.ItemSummary
import app.pley.android.model.LiveChannel
import app.pley.android.network.OfflineException
import app.pley.android.player.BOOK_SPEEDS
import app.pley.android.player.MusicError
import app.pley.android.player.MusicQueueState
import app.pley.android.player.MusicRepeat
import app.pley.android.player.PlayerGraphOwner
import app.pley.android.player.R
import app.pley.android.player.music.library.KeptSearches
import app.pley.android.player.music.library.MusicArt
import app.pley.android.player.music.library.MusicCallers
import app.pley.android.player.music.library.MusicId
import app.pley.android.player.music.library.MusicLibrary
import app.pley.android.player.music.library.MusicLibraryGraph
import app.pley.android.player.music.library.MusicSelection
import app.pley.android.player.music.library.SavedAnswers
import app.pley.android.player.music.library.SavedLists
import app.pley.android.player.music.library.SessionArt
import app.pley.android.player.music.library.SignedOutException
import app.pley.android.player.music.library.SpokenFocus
import app.pley.android.player.music.library.forOlderBrowser
import com.google.common.collect.ImmutableList
import com.google.common.util.concurrent.Futures
import com.google.common.util.concurrent.ListenableFuture
import com.google.common.util.concurrent.SettableFuture
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.drop
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeoutOrNull

/**
 * Music in the background (Android 17 plays background audio only from a
 * foreground service): a Media3 media library service, foreground with
 * type mediaPlayback while music plays, which shows the notification and
 * the lock screen's controls. It plays the app's one [MusicPlayback]
 * ([MusicHub]), its whole queue shown ([QueuePlayer]).
 *
 * Android Auto browses and plays Pley's music through it, as does the
 * Google app's Assistant ("play … on Pley"): the [MusicLibrary] (Recent,
 * Artists, Playlists, search), its pictures read through [MusicArt]. Now
 * Playing has Shuffle and Repeat beside Previous, Play and Next, as the
 * phone's media controls do, and the cover of what plays comes with the
 * track ([SessionArt]). Signed out, the car says so and opens Pley on the
 * phone; a track that can't play says why. A Live TV channel
 * picked in the car plays its sound as radio ([LiveRadio]), the session
 * showing it until music plays again; the last car gone, the radio stops.
 *
 * Only the app itself, the system (its media controls, the notification,
 * Bluetooth and the lock screen), Media3's notification controller and
 * those two Google apps ([MusicCallers]) may connect: no other app drives
 * or reads Pley's music. A remote's Stop key pauses the music ([StopKey]).
 */
@OptIn(markerClass = [UnstableApi::class])
class MusicService : MediaLibraryService() {
    private var session: MediaLibrarySession? = null
    private var player: QueuePlayer? = null

    /** Live TV's sound in the car, made as a channel is first picked. */
    private var radio: LiveRadio? = null
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)

    /** What follows the session made last (its queue, the sign-in): ended with it. */
    private var following: Job? = null

    /** What played last being read back ([bringBack]): the latest only. */
    private var bringingBack: Job? = null

    /** A book brought back, waiting for where the profile is in it on the server ([cueBook]). */
    private var catchingUp: Job? = null

    /** The results of a car's last searches ([KeptSearches]): the profile's, forgotten as another signs in. */
    private val searches = KeptSearches()
    private val owner: PlayerGraphOwner? get() = application as? PlayerGraphOwner

    // The notification's words, Media3's included, and the car's, in the language shown.
    override fun attachBaseContext(base: Context) {
        super.attachBaseContext(AppLanguage.wrap(base))
    }

    override fun onCreate() {
        super.onCreate()
        sessionOrMake()
    }

    // Who connects is decided in onConnect: a car's bind arrives here as Media3's stand-in for every
    // MediaBrowserCompat client, its real package checked only once it asks for the library's root.
    override fun onGetSession(controllerInfo: MediaSession.ControllerInfo): MediaLibrarySession? = sessionOrMake()

    /**
     * The session over the process's music, made again after a stop that
     * no car outlived. A car can connect with nothing playing (it browses
     * first), so the session is made whenever the app's music is: the
     * graph is made for it if Android started the service in a process
     * without one.
     */
    private fun sessionOrMake(): MediaLibrarySession? {
        session?.let { return it }
        val graph = owner?.playerGraph
        // Asking for it makes the app's music, which puts its playback in MusicHub.
        graph?.music
        val playback = MusicHub.playback ?: return null
        val queue = queuePlayer(playback, graph?.musicLibrary?.library) ?: return null
        val covers = graph?.musicLibrary?.let { library ->
            SessionArt(
                scope,
                { MusicArt.pathOf(this, it) },
                library::picture,
                CacheBitmapLoader(DataSourceBitmapLoader(this)),
                drawn = { MusicArt.hueOf(this, it) != null },
                standIn = { MusicArt.standIn(this, it) },
            )
        }
        val made = MediaLibrarySession.Builder(this, queue.forSession, Callback(playback, graph?.musicLibrary))
            .setId(SESSION_ID)
            // The session's state is sent as it changes, not also every few seconds as it plays: Android Auto's queue list
            // goes back to the track playing at each state it is sent, under a finger scrolling it. Controllers work out
            // where the track is from the state's position, time and speed.
            .setPeriodicPositionUpdateEnabled(false)
            .apply { openApp()?.let(::setSessionActivity) }
            .apply { covers?.let(::setBitmapLoader) }
            .build()
        session = made
        player = queue
        // Known to the service from now, not only once a controller binds: Android starting the service again by itself
        // (after ending the app's process) binds nothing, and the session then had no notification and no Shuffle or
        // Repeat for whoever found it.
        addSession(made)
        // The phone's own media controls show the covers the session names, read from the app as the car's are.
        if (MusicCallers.systemApp(packageManager, SYSTEM_UI)) grantArt(SYSTEM_UI)
        following = scope.launch {
            launch {
                playback.queueState.collect { queue ->
                    // The covers of the tracks either side are read before the session is told of this one: Next and
                    // Previous then show theirs at once.
                    graph?.musicLibrary?.library?.let { covers?.warm(coversAround(queue, it)) }
                    player?.queueChanged()
                }
            }
            // A track starting, failing or loaded changes what the session says plays.
            launch { playback.nowPlaying.collect { player?.queueChanged() } }
            // Shuffle's and Repeat's buttons show what they are set to, as the queue changes from anywhere; a book has its own,
            // Speed showing the speed chosen here or on the app's pages.
            launch {
                combine(playback.queueState, playback.bookSpeed) { queue, speed -> listOf(queue.shuffle, queue.repeat, queue.isBook, speed) }
                    .distinctUntilChanged().drop(1).collect {
                        // The radio has neither: the music's come back with the music (showMusic).
                        if (radio?.active != true) made.setMediaButtonPreferences(buttons(playback.queueState.value))
                    }
            }
            // Signed out or another profile: the radio (the last profile's Live TV) stops, and cars read the library again.
            graph?.musicLibrary?.let { library ->
                launch {
                    library.profileChanges.drop(1).collect { profile ->
                        covers?.clear()
                        searches.clear()
                        showMusic(made, playback, library.library)
                        tellCars(made, signedIn = profile != null)
                    }
                }
            }
            // A car showing Recent or Books reads it again a moment after what they show changes ([heardKey]), the server
            // having heard of the play, or of where the book is, meanwhile. Changes close together are told once.
            launch {
                combine(playback.queueState, playback.nowPlaying.map { it?.playing == true }.distinctUntilChanged()) { queue, playing -> heardKey(queue, playing) }
                    .filterNotNull()
                    .distinctUntilChanged()
                    .collectLatest {
                        delay(HEARD_WAIT_MS)
                        tellCarsHeard(made)
                    }
            }
            // Music playing again (from the phone, the car's own controls) takes the session back from the radio.
            launch {
                playback.nowPlaying.map { it?.playing == true }.distinctUntilChanged().collect { playing ->
                    if (playing) {
                        showMusic(made, playback, graph?.musicLibrary?.library)
                        // Music is what played last now: the radio's channel isn't what a car is shown next.
                        MusicHub.resume?.keepRadio(null)
                    }
                }
            }
        }
        playback.onStopped = { stopped(made, playback, graph?.musicLibrary?.library) }
        bringBack(made, playback, graph?.musicLibrary, car = false)
        return made
    }

    /**
     * What played last waits in the session, paused where it was, when
     * nothing plays: the music kept on the device ([MusicResume]) after
     * Android ended the app, or the radio's channel when that was what a
     * car played last. A car connecting is then shown it, with Play ready
     * (a session with nothing in it shows a car nothing to play), and the
     * car's own Play, pressed or sent as it starts, plays it. A book kept
     * waits at once as the device kept it, and moves to where the profile
     * is in it on the server once that is known ([cueBook]). With nothing
     * kept, a car is shown what the library suggests (the book or the album
     * heard last, else the album added last). Nothing is played here: only a car
     * or a person starts the sound.
     *
     * The radio is the car's: with no car connecting (the session made for
     * a headset's Play, the phone's media controls, or Android starting the
     * service by itself), a channel played last waits for the next car, and
     * Play brings back the music kept before it (onPlaybackResumption).
     */
    private fun bringBack(made: MediaLibrarySession, playback: MusicPlayback, graph: MusicLibraryGraph?, car: Boolean) {
        val resume = MusicHub.resume ?: return
        bringingBack?.cancel()
        bringingBack = scope.launch {
            // Music that plays, or will, is what plays; so is a channel already picked.
            fun busy() = session !== made || radio?.active == true || playback.nowPlaying.value?.wantsPlay == true
            if (busy()) return@launch
            val channel = resume.savedRadio()?.let { MusicId.parse(it) as? MusicId.Channel }
            // Cued now, the music kept before the channel would stay in the session once a car took it for the radio.
            if (channel != null && !car) return@launch
            // The channel's list is the server's to give: out of reach, or slow, the music kept is what waits.
            val lineup = if (channel != null && graph != null) {
                withTimeoutOrNull(BRING_BACK_WAIT_MS) { runCatching { graph.live?.lineup(channel) }.getOrNull() }
            } else {
                null
            }
            if (busy()) return@launch
            if (lineup != null && channel != null && graph != null && runCatching { playRadio(made, playback, graph, lineup, channel) }.isSuccess) return@launch
            if (playback.active) return@launch
            val saved = resume.saved()
            // Nothing kept: a car is still shown something to play, never Android Auto's "open the app" with nothing in it.
            val suggested = if (saved == null && car) suggestion(graph) else null
            if (busy() || playback.active) return@launch
            // A book kept goes on where the profile is in it now: it may have been heard elsewhere since (the web app).
            // It waits as the device kept it while the server is asked, rather than leave the car with nothing.
            val book = saved?.tracks?.firstOrNull()?.takeIf { it.kind == ItemKind.Part }?.parentId
            when {
                saved != null && book != null -> {
                    catchingUp?.cancel()
                    catchingUp = scope.launch { playback.cueBook(saved, { whereIn(graph, book) }) { !busy() } }
                }
                saved != null -> playback.cue(saved)
                suggested != null -> playback.cue(suggested.tracks, suggested.startIndex, suggested.shuffle, suggested.startMs, suggested = true)
            }
        }
    }

    /** What the library suggests ([MusicLibrary.suggestion]), within [SUGGESTION_MS]; `null` when it can't say. */
    private suspend fun suggestion(graph: MusicLibraryGraph?): MusicSelection? = asked(graph) { it.suggestion() }

    /** Book [bookId] from where the profile is in it now, within [SUGGESTION_MS]; `null` when the server can't say. */
    private suspend fun whereIn(graph: MusicLibraryGraph?, bookId: Long): MusicSelection? = asked(graph) { it.selection(MusicId.Book(bookId)) }

    private suspend fun asked(graph: MusicLibraryGraph?, ask: suspend (MusicLibrary) -> MusicSelection?): MusicSelection? {
        val library = graph?.library ?: return null
        return try {
            withTimeoutOrNull(SUGGESTION_MS) { ask(library) }
        } catch (e: CancellationException) {
            throw e
        } catch (_: Exception) {
            null
        }
    }

    /**
     * The session's player over the music's output: one per output, kept
     * across the service's runs (each listens to the output for as long as
     * it lives, and the output outlives the service).
     */
    private fun queuePlayer(playback: MusicPlayback, library: MusicLibrary?): QueuePlayer? {
        val output = playback.output() as? EngineMusicOutput ?: return null
        made?.let { (over, player) -> if (over === output) return player }
        val describe: (ItemSummary) -> MediaItem = if (library != null) { track -> library.queueItem(track) } else QueuePlayer::plainItem
        // The player outlives the service: its words come through the app, not this service.
        val app = applicationContext
        val player = QueuePlayer(output.player, playback, describe, { error, nextIn -> wordsOf(app, error, nextIn) }) { chapter, i ->
            chapter.title?.trim()?.takeIf { it.isNotEmpty() } ?: AppLanguage.resources(app).getString(R.string.music_library_chapter, i + 1)
        }
        made = output to player
        return player
    }

    /**
     * The music stopped (its output went with it). A car still connected
     * keeps the session, on a new output; otherwise the session goes and
     * the service stops, as it does on a phone.
     */
    private fun stopped(made: MediaLibrarySession, playback: MusicPlayback, library: MusicLibrary?) {
        if (session !== made) return
        // The radio plays on in the session: the music's next output is shown when music plays again.
        if (radio?.active == true) return
        val carStays = made.connectedControllers.any { it.packageName == MusicCallers.ANDROID_AUTO }
        val next = if (carStays) queuePlayer(playback, library) else null
        if (next != null) {
            made.player = next.forSession
            player = next
            made.setMediaButtonPreferences(buttons(playback.queueState.value))
            return
        }
        end(made, playback)
    }

    /**
     * The radio has [lineup]'s channel in the session, in place of the
     * music, which pauses and lets its session go as for a video. The
     * channel waits: the session's Play, which follows a request to play,
     * tunes it, and a request only to have it ready (the Assistant's, sent
     * before it speaks) leaves it waiting. [picked] is kept as what a car
     * is shown next time. The channels, as the session lists them.
     */
    private fun playRadio(
        made: MediaLibrarySession,
        playback: MusicPlayback,
        graph: MusicLibraryGraph,
        lineup: Pair<List<LiveChannel>, Int>,
        picked: MusicId.Channel,
    ): MediaSession.MediaItemsWithStartPosition {
        val live = radio ?: graph.radio()?.also {
            radio = it
            MusicHub.radio = it
        } ?: throw UnsupportedOperationException("No Live TV")
        playback.pauseForVideo()
        val (channels, at) = lineup
        live.cue(channels, at, graph.live?.listName(picked.from))
        MusicHub.resume?.keepRadio(picked.text)
        made.player = live.player
        // Shuffle and Repeat are the music's: the radio has neither.
        made.setMediaButtonPreferences(emptyList())
        val items = channels.map { graph.live?.channel(it, from = null) ?: MediaItem.Builder().setMediaId(it.id.toString()).build() }
        return MediaSession.MediaItemsWithStartPosition(items, at, C.TIME_UNSET)
    }

    /** The music in the session again, the radio stopped: music plays, or the car that played the radio left. */
    private fun showMusic(made: MediaLibrarySession, playback: MusicPlayback, library: MusicLibrary?) {
        val live = radio ?: return
        if (session !== made || (!live.active && made.player !== live.player)) return
        live.close()
        val queue = queuePlayer(playback, library) ?: return
        made.player = queue.forSession
        player = queue
        made.setMediaButtonPreferences(buttons(playback.queueState.value))
    }

    /** The session goes, and the service stops: nothing plays, and no car holds it. */
    private fun end(made: MediaLibrarySession, playback: MusicPlayback) {
        if (session !== made) return
        radio?.release()
        radio = null
        MusicHub.radio = null
        playback.onStopped = null
        following?.cancel()
        following = null
        bringingBack?.cancel()
        bringingBack = null
        catchingUp?.cancel()
        catchingUp = null
        session = null
        player = null
        made.release()
        stopSelf()
    }

    override fun onDestroy() {
        MusicHub.playback?.onStopped = null
        radio?.release()
        radio = null
        MusicHub.radio = null
        scope.cancel()
        session?.release()
        session = null
        player = null
        super.onDestroy()
    }

    /**
     * Tapping the notification opens the app as its icon does: the app's
     * task comes back as it was, with no second window over it (on a TV,
     * MainActivity hands over to the TV's window already open).
     */
    private fun openApp(): PendingIntent? = packageManager.getLaunchIntentForPackage(packageName)?.let {
        PendingIntent.getActivity(this, 0, it, PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
    }

    /**
     * Whether [controller] may connect: the app (Media3's notification among it), the system's controllers, Android
     * Auto and the Google app. Each name is held to the uid asking: Media3 knows its notification's controller by the
     * name and a hint the controller sends, which an app holding the session's token can send too.
     */
    private fun allowed(controller: MediaSession.ControllerInfo): Boolean =
        (controller.packageName == packageName && (controller.uid == Process.myUid() || MusicCallers.unknown(controller.uid))) ||
            (controller.isTrusted && MusicCallers.isSystem(packageManager, controller.packageName, controller.uid)) ||
            isCar(controller)

    /** Lets [packageName] read the library's pictures ([MusicArt]), and nothing else of the app's. */
    private fun grantArt(packageName: String) {
        runCatching {
            grantUriPermission(
                packageName,
                Uri.parse("content://${MusicArt.authority(this)}"),
                Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_PREFIX_URI_PERMISSION,
            )
        }
    }

    /** Whether [controller] is Android Auto or the Google app, as the app that asks ([MusicCallers.isCar]). */
    private fun isCar(controller: MediaSession.ControllerInfo): Boolean =
        MusicCallers.isCar(packageManager, packageName, controller.packageName, controller.uid)

    /**
     * The buttons beside Previous, Play and Next (Android Auto's Now
     * Playing, the notification, the system's media controls): Shuffle,
     * then Repeat, showing whether the queue or the track repeats. Each is
     * named for what a press does. Their pictures are Pley's: Media3's draw
     * a mode that is off at 30% opacity, which a car shows as a button that
     * can't be pressed; Pley's are as bright off as on, with a dot under one
     * that is on.
     */
    private fun buttons(queue: MusicQueueState): List<CommandButton> {
        val res = AppLanguage.resources(this)
        if (queue.isBook) return bookButtons(res)
        val shuffle = CommandButton.Builder(if (queue.shuffle) CommandButton.ICON_SHUFFLE_ON else CommandButton.ICON_SHUFFLE_OFF)
            .setCustomIconResId(if (queue.shuffle) R.drawable.music_session_shuffle_on else R.drawable.music_session_shuffle)
            .setDisplayName(res.getString(if (queue.shuffle) R.string.music_library_shuffle_off else R.string.music_library_shuffle))
            .setSessionCommand(SHUFFLE)
            .setSlots(CommandButton.SLOT_BACK_SECONDARY, CommandButton.SLOT_OVERFLOW)
            .build()
        val (icon, picture, name) = when (queue.repeat) {
            MusicRepeat.Off -> Triple(CommandButton.ICON_REPEAT_OFF, R.drawable.music_session_repeat, R.string.music_library_repeat)
            MusicRepeat.All -> Triple(CommandButton.ICON_REPEAT_ALL, R.drawable.music_session_repeat_on, R.string.music_library_repeat_one)
            MusicRepeat.One -> Triple(CommandButton.ICON_REPEAT_ONE, R.drawable.music_session_repeat_one_on, R.string.music_library_repeat_off)
        }
        val repeat = CommandButton.Builder(icon)
            .setCustomIconResId(picture)
            .setDisplayName(res.getString(name))
            .setSessionCommand(REPEAT)
            .setSlots(CommandButton.SLOT_FORWARD_SECONDARY, CommandButton.SLOT_OVERFLOW)
            .build()
        return listOf(shuffle, repeat)
    }

    /**
     * A book's buttons in place of Shuffle and Repeat, as audiobook apps
     * have them: back 15 seconds and forward 30 beside Previous and Next
     * (which go from chapter to chapter, and part to part), then Speed,
     * showing the speed it plays at, which a press takes to the next
     * ([BOOK_SPEEDS]).
     */
    private fun bookButtons(res: android.content.res.Resources): List<CommandButton> {
        val back = CommandButton.Builder(CommandButton.ICON_SKIP_BACK_15)
            .setDisplayName(res.getString(R.string.music_library_back))
            .setPlayerCommand(Player.COMMAND_SEEK_BACK)
            .setSlots(CommandButton.SLOT_BACK_SECONDARY, CommandButton.SLOT_OVERFLOW)
            .build()
        val forward = CommandButton.Builder(CommandButton.ICON_SKIP_FORWARD_30)
            .setDisplayName(res.getString(R.string.music_library_forward))
            .setPlayerCommand(Player.COMMAND_SEEK_FORWARD)
            .setSlots(CommandButton.SLOT_FORWARD_SECONDARY, CommandButton.SLOT_OVERFLOW)
            .build()
        val now = MusicHub.playback?.bookSpeed?.value ?: 1f
        val icon = SPEEDS.firstOrNull { it.first == now }?.second ?: CommandButton.ICON_PLAYBACK_SPEED
        val speed = CommandButton.Builder(icon)
            .setDisplayName(res.getString(R.string.music_library_speed, speedText(now)))
            .setSessionCommand(SPEED)
            .setSlots(CommandButton.SLOT_OVERFLOW)
            .build()
        return listOf(back, forward, speed)
    }

    /** "1.5×", "1×". */
    private fun speedText(speed: Float): String =
        java.text.NumberFormat.getNumberInstance(AppLanguage.resources(this).configuration.locales[0]).apply { maximumFractionDigits = 1 }.format(speed) + "×"

    /** Books play at the next of [BOOK_SPEEDS] (kept on the device by the app's music). */
    private fun nextBookSpeed(playback: MusicPlayback) {
        val at = BOOK_SPEEDS.indexOf(playback.bookSpeed.value)
        playback.setBookSpeed(BOOK_SPEEDS[(at + 1) % BOOK_SPEEDS.size])
    }

    /** The covers' addresses of the track playing and those Next and Previous go to. */
    private fun coversAround(queue: MusicQueueState, library: MusicLibrary): List<Uri?> {
        val tracks = queue.tracks
        if (tracks.isEmpty()) return emptyList()
        val wraps = queue.repeat == MusicRepeat.All
        return listOf(0, 1, -1).mapNotNull { step ->
            val at = queue.index + step
            tracks.getOrNull(if (wraps) Math.floorMod(at, tracks.size) else at)
        }.map { library.queueItem(it).mediaMetadata.artworkUri }.distinct()
    }

    /** Signed out, each car shows why it plays nothing, and a button opening Pley on the phone; signed in again, it clears. */
    private fun tellCars(made: MediaLibrarySession, signedIn: Boolean) {
        // Cars read the platform session's state, which Media3 makes from its notification controller's: told again
        // once that controller connects (onPostConnect), when it comes after the car.
        made.mediaNotificationControllerInfo?.let { made.setPlaybackException(it, if (signedIn) null else signedOutError()) }
        made.notifyChildrenChanged(MusicId.Root.text, 0, null)
    }

    /** Each car connected reads Recent and Books again, if it shows them; no other controller browses them. */
    private fun tellCarsHeard(made: MediaLibrarySession) {
        tellCarsChanged(made, MusicId.Recent.text)
        tellCarsChanged(made, MusicId.Books.text)
    }

    /** Each car connected reads [parentId]'s list again, if it shows it. */
    private fun tellCarsChanged(made: MediaLibrarySession, parentId: String) {
        if (session !== made) return
        for (car in made.connectedControllers) {
            if (isAndroidAuto(car)) made.notifyChildrenChanged(car, parentId, 0, null)
        }
    }

    /** Whether [controller] is Android Auto itself, a car's screen, as the app that asks. */
    private fun isAndroidAuto(controller: MediaSession.ControllerInfo): Boolean = controller.packageName == MusicCallers.ANDROID_AUTO && isCar(controller)

    private fun signedOutError(): PlaybackException {
        val res = AppLanguage.resources(this)
        val extras = Bundle().apply {
            putString(MediaConstants.EXTRAS_KEY_ERROR_RESOLUTION_ACTION_LABEL_COMPAT, res.getString(R.string.music_library_open_pley))
            openApp()?.let { putParcelable(MediaConstants.EXTRAS_KEY_ERROR_RESOLUTION_ACTION_INTENT_COMPAT, it) }
        }
        return PlaybackException(res.getString(R.string.music_library_signed_out), null, PlaybackException.ERROR_CODE_AUTHENTICATION_EXPIRED, extras)
    }

    /** The session's answers: who may connect, the library, and what a car or the Assistant asks to play. */
    private inner class Callback(
        private val playback: MusicPlayback,
        private val graph: MusicLibraryGraph?,
    ) : MediaLibrarySession.Callback {
        // What plays pauses: the radio, while the session shows it, else the music.
        private val stop = StopKey {
            val live = radio
            if (live?.active == true) live.player.pause() else playback.pause()
        }

        /** The lists a car was shown from what was saved: a car is told when the server's turn out different. */
        private val savedLists = SavedLists(
            scope,
            { parentId, page, pageSize ->
                // Off the main thread, as the list was made the first time ([LibraryAnswers]).
                withContext(Dispatchers.Default) { MusicId.parse(parentId)?.let { library().children(it, page, pageSize) }.orEmpty() }
            },
        ) { parentId -> session?.let { tellCarsChanged(it, parentId) } }

        /** What a car or the Assistant asked to play, still being looked up ([onSetMediaItems]): the latest only. */
        private var picking: Job? = null

        /**
         * Whether [controller] is answered. Once the notification is connected, Media3 lets a controller named System UI
         * in as it does the notification, without asking [onConnect]; one that only takes the name is refused here.
         */
        private fun answers(controller: MediaSession.ControllerInfo): Boolean =
            controller.packageName != SYSTEM_UI || allowed(controller)

        private fun <T : Any> refused(): ListenableFuture<LibraryResult<T>> =
            Futures.immediateFuture(LibraryResult.ofError(SessionError.ERROR_PERMISSION_DENIED))

        override fun onConnect(session: MediaSession, controller: MediaSession.ControllerInfo): MediaSession.ConnectionResult {
            if (!allowed(controller)) return MediaSession.ConnectionResult.reject()
            val car = isCar(controller)
            // The car reads the library's pictures from the app (MusicArt): it may read them, and only them.
            if (car) grantArt(controller.packageName)
            // Android Auto connecting, to a session that may have outlived the last car: what played last waits for it, the
            // radio's channel too. Told here: Android Auto browses through MediaBrowserCompat, whose connection reaches
            // onPostConnect only with the first command it sends, after the car has shown its Now Playing.
            if (car && controller.packageName == MusicCallers.ANDROID_AUTO && graph != null) {
                bringBack(session as MediaLibrarySession, playback, graph, car = true)
            }
            // Media3's defaults for this controller (super.onConnect only marks "use the defaults"), with Shuffle and Repeat.
            val accepted = MediaSession.ConnectionResult.AcceptedResultBuilder(session, controller).build()
            return MediaSession.ConnectionResult.AcceptedResultBuilder(session, controller)
                .setAvailableSessionCommands(accepted.availableSessionCommands.buildUpon().add(SHUFFLE).add(REPEAT).add(SPEED).build())
                .setAvailablePlayerCommands(accepted.availablePlayerCommands)
                .setMediaButtonPreferences(if (radio?.active == true) emptyList() else buttons(playback.queueState.value))
                .build()
        }

        override fun onCustomCommand(
            session: MediaSession,
            controller: MediaSession.ControllerInfo,
            customCommand: SessionCommand,
            args: Bundle,
        ): ListenableFuture<SessionResult> {
            if (!answers(controller)) return Futures.immediateFuture(SessionResult(SessionError.ERROR_PERMISSION_DENIED))
            val shuffle = customCommand.customAction == SHUFFLE.customAction
            val repeat = customCommand.customAction == REPEAT.customAction
            val speed = customCommand.customAction == SPEED.customAction
            if (!shuffle && !repeat && !speed) return super.onCustomCommand(session, controller, customCommand, args)
            if (radio?.active == true) return Futures.immediateFuture(SessionResult(SessionError.ERROR_NOT_SUPPORTED))
            val queue = playback.queueState.value
            if (speed) {
                if (!queue.isBook) return Futures.immediateFuture(SessionResult(SessionError.ERROR_NOT_SUPPORTED))
                nextBookSpeed(playback)
                (session as MediaLibrarySession).setMediaButtonPreferences(buttons(queue))
                return Futures.immediateFuture(SessionResult(SessionResult.RESULT_SUCCESS))
            }
            if (shuffle) playback.setShuffle(!queue.shuffle) else playback.setRepeat(queue.repeat.next)
            return Futures.immediateFuture(SessionResult(SessionResult.RESULT_SUCCESS))
        }

        override fun onPostConnect(session: MediaSession, controller: MediaSession.ControllerInfo) {
            val library = graph ?: return
            val tells = isCar(controller) || session.isMediaNotificationController(controller)
            if (!tells) return
            // Once the sign-in is read back: as the process starts, no one is signed in until it has been.
            scope.launch {
                library.signInRead()
                if (!library.signedIn()) tellCars(session as MediaLibrarySession, signedIn = false)
            }
        }

        /**
         * Stop from a car or the Assistant ("Hey Google, stop") pauses:
         * stopped, the queue is forgotten and nothing is kept for the next
         * drive, which a person who stops the music from its notification
         * means and a driver doesn't.
         */
        override fun onPlayerCommandRequest(session: MediaSession, controller: MediaSession.ControllerInfo, playerCommand: Int): Int {
            if (!answers(controller)) return SessionError.ERROR_PERMISSION_DENIED
            if (playerCommand == Player.COMMAND_STOP && isCar(controller)) {
                session.player.pause()
                return SessionResult.RESULT_INFO_SKIPPED
            }
            return super.onPlayerCommandRequest(session, controller, playerCommand)
        }

        /**
         * The last car gone: the radio stops (it is the car's), and with no
         * music either, the session and the service go, as after a stop.
         */
        override fun onDisconnected(session: MediaSession, controller: MediaSession.ControllerInfo) {
            // Android Auto is the car: the Google app staying connected (the Assistant) keeps neither the radio nor the session.
            if (controller.packageName != MusicCallers.ANDROID_AUTO || !isCar(controller)) return
            if (session.connectedControllers.any { it != controller && it.packageName == MusicCallers.ANDROID_AUTO }) return
            showMusic(session as MediaLibrarySession, playback, graph?.library)
            if (!playback.active) end(session, playback)
        }

        override fun onMediaButtonEvent(session: MediaSession, controllerInfo: MediaSession.ControllerInfo, intent: Intent): Boolean =
            stop.handle(intent) || super.onMediaButtonEvent(session, controllerInfo, intent)

        override fun onGetLibraryRoot(session: MediaLibrarySession, browser: MediaSession.ControllerInfo, params: LibraryParams?): ListenableFuture<LibraryResult<MediaItem>> {
            if (!answers(browser)) return refused()
            val library = graph?.library ?: return Futures.immediateFuture(LibraryResult.ofError(SessionError.ERROR_NOT_SUPPORTED))
            // Lists by default; the grids of albums and playlists say so themselves.
            val extras = Bundle().apply {
                putInt(MediaConstants.EXTRAS_KEY_CONTENT_STYLE_BROWSABLE, MediaConstants.EXTRAS_VALUE_CONTENT_STYLE_LIST_ITEM)
                putInt(MediaConstants.EXTRAS_KEY_CONTENT_STYLE_PLAYABLE, MediaConstants.EXTRAS_VALUE_CONTENT_STYLE_LIST_ITEM)
            }
            return Futures.immediateFuture(LibraryResult.ofItem(library.root(), LibraryParams.Builder().setExtras(extras).build()))
        }

        override fun onGetItem(session: MediaLibrarySession, browser: MediaSession.ControllerInfo, mediaId: String): ListenableFuture<LibraryResult<MediaItem>> =
            if (!answers(browser)) refused() else answer {
                val id = MusicId.parse(mediaId) ?: return@answer LibraryResult.ofError(SessionError.ERROR_BAD_VALUE)
                library().item(id)?.let { LibraryResult.ofItem(it, null) } ?: LibraryResult.ofError(SessionError.ERROR_BAD_VALUE)
            }

        override fun onGetChildren(
            session: MediaLibrarySession,
            browser: MediaSession.ControllerInfo,
            parentId: String,
            page: Int,
            pageSize: Int,
            params: LibraryParams?,
        ): ListenableFuture<LibraryResult<ImmutableList<MediaItem>>> = if (!answers(browser)) refused() else answer {
            val id = MusicId.parse(parentId) ?: return@answer LibraryResult.ofError(SessionError.ERROR_BAD_VALUE)
            val saved = SavedAnswers()
            val children = withContext(saved) { library().children(id, page, pageSize) }
            if (isAndroidAuto(browser)) onMain { savedLists.shown(parentId, page, pageSize, children, saved) }
            LibraryResult.ofItemList(rowsFor(browser, children), params)
        }

        override fun onSearch(session: MediaLibrarySession, browser: MediaSession.ControllerInfo, query: String, params: LibraryParams?): ListenableFuture<LibraryResult<Void>> =
            // Media3 answers a MediaBrowserCompat car's search only once its results are said to have changed.
            if (!answers(browser)) refused() else answer(slow = { session.notifySearchResultChanged(browser, query, 0, params) }) {
                val found = searches.get(query) ?: try {
                    library().search(query.take(MAX_QUERY))
                } catch (e: Exception) {
                    // A car waits for this, failed too: an empty result.
                    if (e !is CancellationException) onMain { session.notifySearchResultChanged(browser, query, 0, params) }
                    throw e
                }
                searches.put(query, found)
                onMain { session.notifySearchResultChanged(browser, query, found.size, params) }
                LibraryResult.ofVoid()
            }

        override fun onGetSearchResult(
            session: MediaLibrarySession,
            browser: MediaSession.ControllerInfo,
            query: String,
            page: Int,
            pageSize: Int,
            params: LibraryParams?,
        ): ListenableFuture<LibraryResult<ImmutableList<MediaItem>>> = if (!answers(browser)) refused() else answer {
            val found = searches.get(query) ?: library().search(query.take(MAX_QUERY)).also { searches.put(query, it) }
            val size = pageSize.coerceIn(1, MusicLibrary.MAX_PAGE)
            LibraryResult.ofItemList(rowsFor(browser, found.drop(page * size).take(size)), params)
        }

        /**
         * A car picked something to play, or the Assistant heard what to
         * play: it is made ready here, and the session is told its queue;
         * the session's Play, which Media3 sends after a request to play
         * and not after one to prepare, starts it. A channel, or a request
         * naming one exactly, is the radio's; a request matching no music
         * gets the closest channel, unless it asked for an artist, an album
         * or a track. A request naming nothing ("play music") gets what was
         * playing.
         */
        override fun onSetMediaItems(
            mediaSession: MediaSession,
            controller: MediaSession.ControllerInfo,
            mediaItems: MutableList<MediaItem>,
            startIndex: Int,
            startPositionMs: Long,
        ): ListenableFuture<MediaSession.MediaItemsWithStartPosition> {
            val picked = mediaItems.getOrNull(startIndex.coerceAtLeast(0)) ?: mediaItems.firstOrNull()
            val future = SettableFuture.create<MediaSession.MediaItemsWithStartPosition>()
            if (picked == null || graph == null || !answers(controller)) return Futures.immediateFailedFuture(UnsupportedOperationException())
            val made = mediaSession as MediaLibrarySession
            // The latest request is what plays. Media3 doesn't wait for one from a car or the Assistant to be answered
            // before the next (a request to have it ready, then one to play it; a second album tapped), and one looked up
            // longer would end last and replace the other, or stop it playing. What played last, still being read back
            // as a car connects, gives way too.
            picking?.cancel()
            bringingBack?.cancel()
            catchingUp?.cancel()
            picking = scope.launch {
                try {
                    val live = graph.live
                    val id = MusicId.parse(picked.mediaId)
                    val query = picked.requestMetadata.searchQuery?.take(MAX_QUERY)?.takeIf { picked.mediaId.isEmpty() }
                    val playlist = spokenPlaylist(picked)
                    // Asked for as an artist, an album or a track: music, never a channel of that name.
                    val focus = SpokenFocus.of(picked.requestMetadata.extras)
                    val music = focus.kind != null
                    val channel = when {
                        id is MusicId.Channel -> (live?.lineup(id) ?: throw UnsupportedOperationException("No such channel")) to id
                        query != null && !music -> live?.let { runCatching { it.named(query) }.getOrNull() }?.let { (listOf(it) to 0) to MusicId.Channel(it.id) }
                        else -> null
                    }
                    if (channel != null) {
                        future.set(playRadio(made, playback, graph, channel.first, channel.second))
                        return@launch
                    }
                    // "Play music" with nothing named: the queue that waits plays on, or the one kept from before the
                    // app left memory; only with neither, something new (the album added last). The session's Play
                    // that follows starts it.
                    if (query != null && query.isBlank() && playlist == null && focus.isEmpty) {
                        val saved = if (playback.active) null else MusicHub.resume?.saved()
                        if (playback.active || saved != null) {
                            showMusic(made, playback, graph.library)
                            if (saved != null) playback.cue(saved)
                            val queue = playback.queueState.value
                            future.set(MediaSession.MediaItemsWithStartPosition(queue.tracks.map { library().queueItem(it) }, queue.index, playback.position().positionMs))
                            return@launch
                        }
                    }
                    val selection = selectionFor(picked)
                    if (selection == null && query != null && !music) {
                        val closest = live?.let { runCatching { it.closest(query) }.getOrNull() }
                        if (closest != null) {
                            future.set(playRadio(made, playback, graph, listOf(closest) to 0, MusicId.Channel(closest.id)))
                            return@launch
                        }
                    }
                    if (selection == null) {
                        // Said, and found nowhere: the car and the Assistant say so a moment, and what played plays on.
                        val said = playlist ?: query?.trim()
                        if (!said.isNullOrEmpty()) {
                            val words = AppLanguage.resources(this@MusicService).getString(R.string.music_library_no_match, said)
                            made.sendError(SessionError(SessionError.ERROR_BAD_VALUE, words))
                        }
                        throw UnsupportedOperationException("Nothing to play")
                    }
                    // The session shows the music before it is told the queue: the radio, if it played, stops.
                    showMusic(made, playback, graph.library)
                    MusicHub.resume?.keepRadio(null)
                    // Ready, not playing: the session's Play, which follows a request to play, starts it once. Started
                    // here, a request only to have it ready (the Assistant's, sent before it speaks) played under its
                    // voice, and the request to play that came after started the track a second time.
                    playback.cue(selection.tracks, selection.startIndex, selection.shuffle, selection.startMs)
                    val queue = playback.queueState.value
                    player?.named(queue.tracks, selection.title)
                    val items = queue.tracks.map { library().queueItem(it) }
                    future.set(MediaSession.MediaItemsWithStartPosition(items, queue.index, C.TIME_UNSET))
                } catch (e: CancellationException) {
                    future.cancel(false)
                    throw e
                } catch (e: Exception) {
                    future.setException(e)
                }
            }
            return future
        }

        /**
         * Play from the system's media controls, a headset or a car with
         * nothing loaded, or the system asking what it could resume: what
         * waits in the session (the queue the music still holds, or what
         * played last, read back as the session was made: [bringBack]),
         * else what was kept on the device ([MusicResume]), made ready
         * where it was. Nothing is played here: asked to play, Media3 sends
         * the session's Play next, which starts it once.
         */
        override fun onPlaybackResumption(
            mediaSession: MediaSession,
            controller: MediaSession.ControllerInfo,
            isForPlayback: Boolean,
        ): ListenableFuture<MediaSession.MediaItemsWithStartPosition> {
            if (!answers(controller)) return Futures.immediateFailedFuture(UnsupportedOperationException())
            val describe = { track: ItemSummary -> graph?.library?.queueItem(track) ?: QueuePlayer.plainItem(track) }
            val future = SettableFuture.create<MediaSession.MediaItemsWithStartPosition>()
            scope.launch {
                try {
                    // What played last may still be on its way into the session: it is what resumes, not a second answer.
                    withTimeoutOrNull(BRING_BACK_WAIT_MS) { bringingBack?.join() }
                    val shown = mediaSession.player
                    if (shown.mediaItemCount > 0) {
                        val items = (0 until shown.mediaItemCount).map(shown::getMediaItemAt)
                        future.set(MediaSession.MediaItemsWithStartPosition(items, shown.currentMediaItemIndex, shown.currentPosition))
                        return@launch
                    }
                    val saved = MusicHub.resume?.saved()
                    // Played meanwhile from somewhere else: that is what plays.
                    if (saved == null || !playback.queueState.value.tracks.isEmpty()) {
                        future.setException(UnsupportedOperationException("Nothing to resume"))
                        return@launch
                    }
                    if (isForPlayback) playback.cue(saved)
                    future.set(MediaSession.MediaItemsWithStartPosition(saved.tracks.map(describe), saved.index, saved.positionMs))
                } catch (e: CancellationException) {
                    future.cancel(false)
                    throw e
                } catch (e: Exception) {
                    future.setException(e)
                }
            }
            return future
        }

        // A queue isn't added to from outside: what a car picks replaces it (onSetMediaItems).
        override fun onAddMediaItems(mediaSession: MediaSession, controller: MediaSession.ControllerInfo, mediaItems: MutableList<MediaItem>): ListenableFuture<MutableList<MediaItem>> =
            Futures.immediateFailedFuture(UnsupportedOperationException())

        private suspend fun selectionFor(item: MediaItem): MusicSelection? {
            val query = item.requestMetadata.searchQuery
            return when {
                item.mediaId.isNotEmpty() -> MusicId.parse(item.mediaId)?.let { library().selection(it) }
                query != null -> library().selectionFor(query.take(MAX_QUERY), spokenPlaylist(item), SpokenFocus.of(item.requestMetadata.extras))
                else -> null
            }
        }

        /** The playlist the Assistant heard named ("play my Night Drive playlist on Pley"), when it says. */
        private fun spokenPlaylist(item: MediaItem): String? =
            item.requestMetadata.extras?.getString(MediaStore.EXTRA_MEDIA_PLAYLIST)?.trim()?.take(MAX_QUERY)?.takeIf { it.isNotEmpty() }

        private fun library(): MusicLibrary = graph?.library ?: throw UnsupportedOperationException()

        /** [rows] as [browser] is sent them: only what each shows to a browser of the older protocol, as a car is ([forOlderBrowser]). */
        private fun rowsFor(browser: MediaSession.ControllerInfo, rows: List<MediaItem>): List<MediaItem> =
            if (browser.controllerVersion == MediaSession.ControllerInfo.LEGACY_CONTROLLER_VERSION) rows.map { it.forOlderBrowser() } else rows

        /** The library's answers, made off the main thread ([LibraryAnswers]). */
        private val libraryAnswers = LibraryAnswers(scope, ::errorOf)

        /** [block]'s result, made off the main thread, a failure as the car shows it ([LibraryAnswers.answer]). */
        private fun <T : Any> answer(slow: () -> Unit = {}, block: suspend () -> LibraryResult<T>): ListenableFuture<LibraryResult<T>> =
            libraryAnswers.answer(slow, block)

        /** [block] on the main thread, from an answer's work: what the session, and what it was told, are kept on. */
        private suspend fun <R> onMain(block: () -> R): R = libraryAnswers.onMain(block)

        private fun errorOf(e: Exception): SessionError {
            val res = AppLanguage.resources(this@MusicService)
            return when (e) {
                is SignedOutException -> SessionError(SessionError.ERROR_SESSION_AUTHENTICATION_EXPIRED, res.getString(R.string.music_library_signed_out))
                is OfflineException -> SessionError(SessionError.ERROR_IO, res.getString(R.string.music_library_offline))
                else -> SessionError(SessionError.ERROR_UNKNOWN, res.getString(R.string.music_library_failed))
            }
        }
    }

    private companion object {
        const val SESSION_ID = "pley.music"

        /** Shuffle's button: shuffles the queue, or plays it in order again. */
        val SHUFFLE = SessionCommand("app.pley.music.SHUFFLE", Bundle.EMPTY)

        /** Repeat's button: the queue repeats, then the track, then nothing again. */
        val REPEAT = SessionCommand("app.pley.music.REPEAT", Bundle.EMPTY)

        /** A book's Speed button: the next of [BOOK_SPEEDS]. */
        val SPEED = SessionCommand("app.pley.music.SPEED", Bundle.EMPTY)

        /** Each of [BOOK_SPEEDS] with Media3's icon that says it. */
        val SPEEDS = listOf(
            1.0f to CommandButton.ICON_PLAYBACK_SPEED_1_0,
            1.2f to CommandButton.ICON_PLAYBACK_SPEED_1_2,
            1.5f to CommandButton.ICON_PLAYBACK_SPEED_1_5,
            1.8f to CommandButton.ICON_PLAYBACK_SPEED_1_8,
            2.0f to CommandButton.ICON_PLAYBACK_SPEED_2_0,
            0.8f to CommandButton.ICON_PLAYBACK_SPEED_0_8,
        )

        /** The session's player and the output it shows, kept for the process's life (queuePlayer). */
        var made: Pair<EngineMusicOutput, QueuePlayer>? = null

        /** The system's UI: its media controls, on the lock screen and in the shade, show the session's covers. */
        const val SYSTEM_UI = "com.android.systemui"

        /** The longest search a car or the Assistant sends that is used. */
        const val MAX_QUERY = 200

        /** How long the radio's channel is waited for as what played last is read back ([bringBack]), and Play for that. */
        const val BRING_BACK_WAIT_MS = 3_000L

        /** How long a car waits for the library's suggestion when nothing was kept ([bringBack]): until then, it has nothing to show. */
        const val SUGGESTION_MS = 10_000L

        /** How long after what plays changes cars are told to read Recent and Books again ([heardKey]). */
        const val HEARD_WAIT_MS = 2_000L

        /** Why a track doesn't play, as a car says it, in the language shown; and when the next plays, [nextIn] s from now. */
        fun wordsOf(context: Context, error: MusicError, nextIn: Int? = null): String {
            val words = AppLanguage.resources(context)
            if (error == MusicError.Unplayable && nextIn != null) return words.getQuantityString(R.plurals.music_library_unplayable_next, nextIn, nextIn)
            return words.getString(
                when (error) {
                    MusicError.SignedOut -> R.string.music_library_signed_out
                    MusicError.Offline -> R.string.music_library_offline
                    MusicError.Unplayable -> R.string.music_library_unplayable
                    MusicError.Stopped -> R.string.music_library_stopped
                },
            )
        }
    }
}

/**
 * What a car's Recent and Books show of what plays, which changes when this
 * does: the tracks of a queue once it plays (a new album or playlist goes
 * into Recently played; skipping within it changes nothing), a book's part
 * and whether it plays (where the profile is in it is sent as it pauses,
 * and it is further on at its next part). `null` while music is paused,
 * which changes nothing, and with nothing in the queue.
 */
internal fun heardKey(queue: MusicQueueState, playing: Boolean): Any? = when {
    queue.tracks.isEmpty() -> null
    queue.isBook -> Triple(queue.current?.parentId, queue.index, playing)
    playing -> queue.tracks.mapTo(HashSet()) { it.id }
    else -> null
}

/**
 * The Stop key of a TV remote or a headset, as the Samsung app has it: the
 * music pauses and keeps its queue, where Media3 would stop it and forget
 * the queue (Now Playing would leave the rail). The notification swiped
 * away comes as Stop too, marked as its dismissal: that still ends the music.
 */
@OptIn(markerClass = [UnstableApi::class])
internal class StopKey(private val pause: () -> Unit) {
    /** Whether [intent], a media button, is the Stop key, handled here. */
    fun handle(intent: Intent): Boolean {
        val key = IntentCompat.getParcelableExtra(intent, Intent.EXTRA_KEY_EVENT, KeyEvent::class.java) ?: return false
        if (key.keyCode != KeyEvent.KEYCODE_MEDIA_STOP) return false
        if (intent.getBooleanExtra(MediaNotification.NOTIFICATION_DISMISSED_EVENT_KEY, false)) return false
        // The key's release and its repeats are handled too, and change nothing.
        if (key.action == KeyEvent.ACTION_DOWN && key.repeatCount == 0) pause()
        return true
    }
}

/**
 * The process's music, for the service: the player graph puts its
 * [MusicPlayback] here when it makes it. A service the system starts in a
 * process without one (after process death) has nothing to play.
 */
object MusicHub {
    @Volatile
    var playback: MusicPlayback? = null

    /** The car's radio while the service has one: a video opening stops it, as it pauses the music. */
    @Volatile
    var radio: LiveRadio? = null

    /** What music was playing, kept on the device: Play brings it back in a process that holds no queue. */
    @Volatile
    var resume: MusicResume? = null
}
