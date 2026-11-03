package app.pley.android.ui.tv.player

import android.view.KeyEvent
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/** The player's key map (TV addendum §5.2, design spec §2.7). */
class TvPlayerKeysTest {
    @Test
    fun `with the controls hidden, Centre is OK, which shows them rather than pausing, unless Skip holds focus`() {
        assertEquals(PlayerKey.Ok, TvPlayerKeys.hidden(KeyEvent.KEYCODE_DPAD_CENTER, skipShowing = false))
        assertEquals(PlayerKey.Ok, TvPlayerKeys.hidden(KeyEvent.KEYCODE_ENTER, skipShowing = false))
        assertNull(TvPlayerKeys.hidden(KeyEvent.KEYCODE_DPAD_CENTER, skipShowing = true))
    }

    @Test
    fun `with the controls hidden, Left and Right seek, Up and Down show the controls`() {
        assertEquals(PlayerKey.Step(-1), TvPlayerKeys.hidden(KeyEvent.KEYCODE_DPAD_LEFT, skipShowing = false))
        assertEquals(PlayerKey.Step(1), TvPlayerKeys.hidden(KeyEvent.KEYCODE_DPAD_RIGHT, skipShowing = true))
        assertEquals(PlayerKey.Show(ControlFocus.Scrubber), TvPlayerKeys.hidden(KeyEvent.KEYCODE_DPAD_UP, skipShowing = false))
        assertEquals(PlayerKey.Show(ControlFocus.PlayPause), TvPlayerKeys.hidden(KeyEvent.KEYCODE_DPAD_DOWN, skipShowing = false))
        assertEquals(PlayerKey.Show(ControlFocus.Default), TvPlayerKeys.hidden(KeyEvent.KEYCODE_MENU, skipShowing = false))
    }

    @Test
    fun `Back and the volume are left to the system`() {
        assertNull(TvPlayerKeys.hidden(KeyEvent.KEYCODE_BACK, skipShowing = false))
        assertNull(TvPlayerKeys.hidden(KeyEvent.KEYCODE_VOLUME_UP, skipShowing = false))
        assertNull(TvPlayerKeys.media(KeyEvent.KEYCODE_BACK))
        assertNull(TvPlayerKeys.media(KeyEvent.KEYCODE_VOLUME_MUTE))
    }

    @Test
    fun `the scrubber takes Left, Right and Centre, and leaves Up and Down to focus`() {
        assertEquals(PlayerKey.Step(-1), TvPlayerKeys.scrubber(KeyEvent.KEYCODE_DPAD_LEFT))
        assertEquals(PlayerKey.PlayPause, TvPlayerKeys.scrubber(KeyEvent.KEYCODE_DPAD_CENTER))
        assertNull(TvPlayerKeys.scrubber(KeyEvent.KEYCODE_DPAD_UP))
        assertNull(TvPlayerKeys.scrubber(KeyEvent.KEYCODE_DPAD_DOWN))
    }

    @Test
    fun `while a seek waits, Up on the scrubber opens the chapters`() {
        assertEquals(PlayerKey.Chapters, TvPlayerKeys.scrubber(KeyEvent.KEYCODE_DPAD_UP, chaptersOnUp = true))
        assertNull(TvPlayerKeys.scrubber(KeyEvent.KEYCODE_DPAD_DOWN, chaptersOnUp = true))
    }

    @Test
    fun `media keys play, pause, jump 30 s, change episode or chapter, toggle captions and open the audio`() {
        assertEquals(PlayerKey.PlayPause, TvPlayerKeys.media(KeyEvent.KEYCODE_MEDIA_PLAY_PAUSE))
        assertEquals(PlayerKey.Play, TvPlayerKeys.media(KeyEvent.KEYCODE_MEDIA_PLAY))
        assertEquals(PlayerKey.Pause, TvPlayerKeys.media(KeyEvent.KEYCODE_MEDIA_PAUSE))
        assertEquals(PlayerKey.Jump(30_000), TvPlayerKeys.media(KeyEvent.KEYCODE_MEDIA_FAST_FORWARD))
        assertEquals(PlayerKey.Jump(-30_000), TvPlayerKeys.media(KeyEvent.KEYCODE_MEDIA_REWIND))
        assertEquals(PlayerKey.Next, TvPlayerKeys.media(KeyEvent.KEYCODE_MEDIA_NEXT))
        assertEquals(PlayerKey.Previous, TvPlayerKeys.media(KeyEvent.KEYCODE_MEDIA_PREVIOUS))
        assertEquals(PlayerKey.Captions, TvPlayerKeys.media(KeyEvent.KEYCODE_CAPTIONS))
        assertEquals(PlayerKey.AudioTrack, TvPlayerKeys.media(KeyEvent.KEYCODE_MEDIA_AUDIO_TRACK))
        assertEquals(PlayerKey.Stop, TvPlayerKeys.media(KeyEvent.KEYCODE_MEDIA_STOP))
    }
}
