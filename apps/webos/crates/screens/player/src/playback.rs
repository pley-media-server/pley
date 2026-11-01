//! What the player screen asks of a playback: pley-player's [`PlaybackController`], seen through
//! a trait so the screen's keys, focus and overlays can be tested against a stand-in.

use pley_core::media::EngineStats;
use pley_core::model::{ServerEvent, SubtitleResult};
use pley_player::controller::SeekPreview;
use pley_player::sleep::SleepChoice;
use pley_player::{PlaybackController, PlayerClock, PlayerState, SavedPlayback};
use pley_subs::Shown as Subtitles;

/// One video's playback, as the screen drives it (each method is [`PlaybackController`]'s own).
pub trait Playback {
    /// Collects answers and engine news at `now_ms`; true when what shows changed.
    fn tick(&mut self, now_ms: i64) -> bool;
    /// When [`Playback::tick`] next has something to do.
    fn next_wake(&self) -> Option<i64>;
    /// What the screen shows, but the clock and the subtitles.
    fn state(&self) -> &PlayerState;
    /// Where the video is and how long it is.
    fn clock(&self) -> PlayerClock;
    /// The subtitles to draw now.
    fn subtitles(&self) -> &Subtitles;
    /// Whether it means to play (Play/Pause shows the other).
    fn wants_to_play(&self) -> bool;
    /// The seek preview for a target.
    fn seek_preview(&self, target_ms: i64) -> Option<SeekPreview>;
    /// What the TV's player says of itself, for Playback details.
    fn engine_stats(&self) -> EngineStats;
    /// A server event (a session stopped elsewhere).
    fn server_event(&mut self, event: &ServerEvent);
    /// Plays (after the end: again from the start).
    fn play(&mut self);
    /// Pauses.
    fn pause(&mut self);
    /// Seeks, a quarter second short of the end at most.
    fn seek_to(&mut self, position_ms: i64);
    /// Plays the audio stream of the server's `index`, and keeps the choice.
    fn select_audio(&mut self, index: i64);
    /// Shows the subtitle stream of `index`, or none, and keeps the choice.
    fn select_subtitle(&mut self, index: Option<i64>);
    /// Searches subtitles for the file in the profile's language.
    fn find_subtitles(&mut self);
    /// Adds a subtitle the search found to the file, and shows it.
    fn add_found_subtitle(&mut self, result: &SubtitleResult);
    /// Moves the subtitles by `ms` (Sync), kept for the file and profile.
    fn set_subtitle_offset(&mut self, ms: i64);
    /// Plays at `rate` (1: normal).
    fn set_speed(&mut self, rate: f32);
    /// Sets the sleep timer.
    fn set_sleep(&mut self, choice: SleepChoice);
    /// The sleep notice's Keep watching: the timer runs again.
    fn keep_watching(&mut self);
    /// Plays at the quality of the Quality menu's `id`.
    fn select_quality(&mut self, id: &str);
    /// Skips the intro or the credits playing.
    fn skip(&mut self);
    /// Plays what comes next now (Play now, the Next key).
    fn play_next(&mut self);
    /// Hides the Up Next card (at the end: the player leaves).
    fn cancel_up_next(&mut self);
    /// Tries the original again after the player chose a conversion by itself.
    fn play_original(&mut self);
    /// Playback details show (Info or the readout): their answers are asked for.
    fn set_details_open(&mut self, open: bool);
    /// The notice was shown as a toast.
    fn dismiss_notice(&mut self);
    /// Try again, after an error.
    fn retry(&mut self);
    /// Where it is, to start again there when the app comes back.
    fn save_state(&self) -> SavedPlayback;
    /// Ends the session and lets the TV's player go.
    fn close(&mut self);
}

impl Playback for PlaybackController {
    fn tick(&mut self, now_ms: i64) -> bool {
        PlaybackController::tick(self, now_ms)
    }

    fn next_wake(&self) -> Option<i64> {
        PlaybackController::next_wake(self)
    }

    fn state(&self) -> &PlayerState {
        PlaybackController::state(self)
    }

    fn clock(&self) -> PlayerClock {
        PlaybackController::clock(self)
    }

    fn subtitles(&self) -> &Subtitles {
        PlaybackController::subtitles(self)
    }

    fn wants_to_play(&self) -> bool {
        PlaybackController::wants_to_play(self)
    }

    fn seek_preview(&self, target_ms: i64) -> Option<SeekPreview> {
        PlaybackController::seek_preview(self, target_ms)
    }

    fn engine_stats(&self) -> EngineStats {
        PlaybackController::engine_stats(self)
    }

    fn server_event(&mut self, event: &ServerEvent) {
        PlaybackController::server_event(self, event)
    }

    fn play(&mut self) {
        PlaybackController::play(self)
    }

    fn pause(&mut self) {
        PlaybackController::pause(self)
    }

    fn seek_to(&mut self, position_ms: i64) {
        PlaybackController::seek_to(self, position_ms)
    }

    fn select_audio(&mut self, index: i64) {
        PlaybackController::select_audio(self, index)
    }

    fn select_subtitle(&mut self, index: Option<i64>) {
        PlaybackController::select_subtitle(self, index)
    }

    fn find_subtitles(&mut self) {
        PlaybackController::find_subtitles(self, None)
    }

    fn add_found_subtitle(&mut self, result: &SubtitleResult) {
        PlaybackController::add_found_subtitle(self, result)
    }

    fn set_subtitle_offset(&mut self, ms: i64) {
        PlaybackController::set_subtitle_offset(self, ms)
    }

    fn set_speed(&mut self, rate: f32) {
        PlaybackController::set_speed(self, rate)
    }

    fn set_sleep(&mut self, choice: SleepChoice) {
        PlaybackController::set_sleep(self, choice)
    }

    fn keep_watching(&mut self) {
        PlaybackController::keep_watching(self)
    }

    fn select_quality(&mut self, id: &str) {
        PlaybackController::select_quality(self, id)
    }

    fn skip(&mut self) {
        PlaybackController::skip(self)
    }

    fn play_next(&mut self) {
        PlaybackController::play_next(self)
    }

    fn cancel_up_next(&mut self) {
        PlaybackController::cancel_up_next(self)
    }

    fn play_original(&mut self) {
        PlaybackController::play_original(self)
    }

    fn set_details_open(&mut self, open: bool) {
        PlaybackController::set_details_open(self, open)
    }

    fn dismiss_notice(&mut self) {
        PlaybackController::dismiss_notice(self)
    }

    fn retry(&mut self) {
        PlaybackController::retry(self)
    }

    fn save_state(&self) -> SavedPlayback {
        PlaybackController::save_state(self)
    }

    fn close(&mut self) {
        PlaybackController::close(self)
    }
}
