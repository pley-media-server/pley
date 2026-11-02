package app.pley.android.ui.tv.player

import android.view.KeyEvent

/** What a remote's key asks of the player. */
internal sealed interface PlayerKey {
    data object PlayPause : PlayerKey

    data object Play : PlayerKey

    data object Pause : PlayerKey

    /** Seek by [deltaMs] without waiting for OK (fast forward and rewind: ±30 s). */
    data class Jump(val deltaMs: Long) : PlayerKey

    /** Left or Right with the controls hidden or the scrubber focused: [direction] −1 or +1, held or not. */
    data class Step(val direction: Int) : PlayerKey

    /** Show the controls, focus on [focus]. */
    data class Show(val focus: ControlFocus) : PlayerKey

    data object Next : PlayerKey

    data object Previous : PlayerKey

    /** Subtitles on (the last ones, or the first) or off. */
    data object Captions : PlayerKey

    /** The Audio panel, to choose a track there. */
    data object AudioTrack : PlayerKey

    /** Stop and go back to where playing started. */
    data object Stop : PlayerKey
}

/** Where focus goes when the controls show. */
internal enum class ControlFocus {
    /** Skip while a marker runs, else the scrubber. */
    Default,
    Scrubber,

    /** The track buttons (Subtitles). */
    Tracks,
}

/**
 * The player's keys (TV addendum §5.2, design spec §2.7): which key does
 * what, apart from what focus does with it. Media keys work whatever
 * shows; the rest depends on whether the controls are hidden.
 */
internal object TvPlayerKeys {
    /** Fast forward and rewind. */
    const val JUMP_MS = 30_000L

    /** A media key's meaning; `null` when [keyCode] isn't one. Media keys are always the player's (they return true). */
    fun media(keyCode: Int): PlayerKey? = when (keyCode) {
        KeyEvent.KEYCODE_MEDIA_PLAY_PAUSE, KeyEvent.KEYCODE_HEADSETHOOK -> PlayerKey.PlayPause
        KeyEvent.KEYCODE_MEDIA_PLAY -> PlayerKey.Play
        KeyEvent.KEYCODE_MEDIA_PAUSE -> PlayerKey.Pause
        KeyEvent.KEYCODE_MEDIA_FAST_FORWARD, KeyEvent.KEYCODE_MEDIA_SKIP_FORWARD -> PlayerKey.Jump(JUMP_MS)
        KeyEvent.KEYCODE_MEDIA_REWIND, KeyEvent.KEYCODE_MEDIA_SKIP_BACKWARD -> PlayerKey.Jump(-JUMP_MS)
        KeyEvent.KEYCODE_MEDIA_NEXT -> PlayerKey.Next
        KeyEvent.KEYCODE_MEDIA_PREVIOUS -> PlayerKey.Previous
        KeyEvent.KEYCODE_CAPTIONS -> PlayerKey.Captions
        KeyEvent.KEYCODE_MEDIA_AUDIO_TRACK -> PlayerKey.AudioTrack
        KeyEvent.KEYCODE_MEDIA_STOP -> PlayerKey.Stop
        else -> null
    }

    /**
     * A key with the controls hidden; `null` leaves it to focus (a showing
     * Skip button takes Centre) or to the system (Back, volume).
     */
    fun hidden(keyCode: Int, skipShowing: Boolean): PlayerKey? = when (keyCode) {
        in SELECT -> if (skipShowing) null else PlayerKey.PlayPause
        KeyEvent.KEYCODE_DPAD_LEFT -> PlayerKey.Step(-1)
        KeyEvent.KEYCODE_DPAD_RIGHT -> PlayerKey.Step(1)
        KeyEvent.KEYCODE_DPAD_UP -> PlayerKey.Show(ControlFocus.Scrubber)
        KeyEvent.KEYCODE_DPAD_DOWN -> PlayerKey.Show(ControlFocus.Tracks)
        in OTHER_SHOW -> PlayerKey.Show(ControlFocus.Default)
        else -> null
    }

    /** A key while the scrubber has focus: Left and Right seek, Centre plays or pauses. */
    fun scrubber(keyCode: Int): PlayerKey? = when (keyCode) {
        in SELECT -> PlayerKey.PlayPause
        KeyEvent.KEYCODE_DPAD_LEFT -> PlayerKey.Step(-1)
        KeyEvent.KEYCODE_DPAD_RIGHT -> PlayerKey.Step(1)
        else -> null
    }

    fun isSelect(keyCode: Int): Boolean = keyCode in SELECT

    private val SELECT = setOf(KeyEvent.KEYCODE_DPAD_CENTER, KeyEvent.KEYCODE_ENTER, KeyEvent.KEYCODE_NUMPAD_ENTER)

    /** Keys that show the controls without saying where focus goes. */
    private val OTHER_SHOW = setOf(
        KeyEvent.KEYCODE_MENU,
        KeyEvent.KEYCODE_INFO,
        KeyEvent.KEYCODE_GUIDE,
        KeyEvent.KEYCODE_DPAD_UP_LEFT,
        KeyEvent.KEYCODE_DPAD_UP_RIGHT,
        KeyEvent.KEYCODE_DPAD_DOWN_LEFT,
        KeyEvent.KEYCODE_DPAD_DOWN_RIGHT,
    )
}
