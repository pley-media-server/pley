//! Settings against the fake server: focus between the categories and their rows, switches,
//! side panels, the profile's defaults on the server, the questions before signing out or deleting,
//! and the web dialog.

mod common;

use common::TestApp;
use pley_base::Key;
use pley_core::model::SubtitleMode;
use pley_fixtures::ids;
use pley_settings::SettingsScreen;
use pley_shell::{Route, SettingsSection};

fn settings(section: SettingsSection) -> Route {
    Route::Settings { section: Some(section) }
}

/// The page's id of category `c`'s row.
fn category(app: &TestApp, c: SettingsSection) -> pley_ui::Id {
    SettingsScreen::category_id(app.page(), c)
}

/// The page's id of row `i` of category `c`'s pane.
fn row(app: &TestApp, c: SettingsSection, i: usize) -> pley_ui::Id {
    SettingsScreen::row_id(app.page(), c, i)
}

#[test]
fn focus_on_a_category_shows_its_rows() {
    let mut app = TestApp::open_on(Route::Settings { section: None });
    assert_eq!(app.focused(), Some(category(&app, SettingsSection::Profile)));
    assert!(app.shows("Settings"));
    assert!(app.shows("Alex"));
    assert!(app.shows("Switch profile"));
    assert!(app.shows("Audio language"));
    assert!(app.shows("English"));
    assert!(app.shows("Only for foreign dialogue"));
    // Alex's defaults are "forced": no subtitle language.
    assert!(!app.shows("Subtitle language"));

    app.press(Key::Down);
    assert_eq!(app.focused(), Some(category(&app, SettingsSection::Playback)));
    assert!(app.shows("Quality"));
    assert!(app.shows("Automatic"));
    assert!(app.shows("Play the next episode by itself"));
    assert!(!app.shows("Switch profile"));

    app.press(Key::Down);
    assert!(app.shows("Pass through Dolby TrueHD"));
    app.press(Key::Down);
    assert!(app.shows("Subtitle size"));
    assert!(app.shows("Reduce motion"));
    app.press_n(Key::Down, 2);
    assert!(app.shows("Open diagnostics"));
    assert!(app.shows("OLED65C2PUA"));
    app.press(Key::Down);
    assert!(app.shows("Version"));
    assert!(app.shows("Open-source licences"));
    app.press(Key::Down);
    assert_eq!(app.focused(), Some(category(&app, SettingsSection::SignOut)));
    assert!(app.shows("Delete all local data"));
    // The last category stops Down.
    app.press(Key::Down);
    assert_eq!(app.focused(), Some(category(&app, SettingsSection::SignOut)));
}

#[test]
fn right_goes_into_the_first_row_and_left_comes_back() {
    let mut app = TestApp::open_on(settings(SettingsSection::Playback));
    assert_eq!(app.focused(), Some(category(&app, SettingsSection::Playback)));
    app.press(Key::Right);
    assert_eq!(app.focused(), Some(row(&app, SettingsSection::Playback, 0)));
    app.press(Key::Down);
    assert_eq!(app.focused(), Some(row(&app, SettingsSection::Playback, 1)));
    app.press(Key::Left);
    assert_eq!(app.focused(), Some(category(&app, SettingsSection::Playback)));
    assert!(!app.shell.rail_open());

    // Audio's pane starts with a paragraph: its first row is the first switch.
    app.press(Key::Down);
    app.press(Key::Right);
    assert_eq!(app.focused(), Some(row(&app, SettingsSection::Audio, 1)));
    // Left from the categories opens the rail.
    app.press(Key::Left);
    app.press(Key::Left);
    assert!(app.shell.rail_open());
}

#[test]
fn ok_on_a_category_goes_into_its_rows() {
    let mut app = TestApp::open_on(settings(SettingsSection::Server));
    app.press(Key::Ok);
    assert_eq!(app.focused(), Some(row(&app, SettingsSection::Server, 0)));
    app.press(Key::Down);
    app.press(Key::Ok);
    app.settle();
    assert_eq!(app.route(), Some(&Route::Servers));
}

#[test]
fn ok_on_a_switch_flips_it() {
    let mut app = TestApp::open_on(settings(SettingsSection::Playback));
    assert!(app.state.settings().autoplay_next);
    app.press(Key::Right);
    app.press(Key::Down);
    app.press(Key::Ok);
    assert!(!app.state.settings().autoplay_next);
    app.press(Key::Ok);
    assert!(app.state.settings().autoplay_next);

    app.press(Key::Down);
    app.press(Key::Ok);
    assert!(app.state.settings().playback_details);
}

#[test]
fn a_choice_opens_a_panel_on_the_one_in_force_and_saves() {
    let mut app = TestApp::open_on(settings(SettingsSection::Playback));
    app.press(Key::Right);
    app.press(Key::Ok);
    app.settle();
    assert!(app.modal());
    assert!(app.shows("Original"));
    assert!(app.shows("2 Mbps 480p"));
    // Automatic is in force.
    assert_eq!(app.focused(), Some(SettingsScreen::choice_id(app.page(), 1)));
    app.press(Key::Up);
    app.press(Key::Ok);
    app.settle();
    assert!(!app.modal());
    assert_eq!(app.state.settings().quality, "original");
    assert_eq!(app.focused(), Some(row(&app, SettingsSection::Playback, 0)));
    assert!(app.shows("Original"));
}

#[test]
fn back_closes_a_panel_and_changes_nothing() {
    let mut app = TestApp::open_on(settings(SettingsSection::SubtitlesAndDisplay));
    app.press(Key::Right);
    app.press(Key::Ok);
    app.settle();
    assert!(app.modal());
    app.press(Key::Down);
    app.press(Key::Back);
    app.settle();
    assert!(!app.modal());
    assert_eq!(app.route(), Some(&settings(SettingsSection::SubtitlesAndDisplay)));
    assert_eq!(app.state.settings().subtitle_size, pley_core::store::SubtitleSize::default());
    assert_eq!(app.focused(), Some(row(&app, SettingsSection::SubtitlesAndDisplay, 0)));
}

#[test]
fn the_subtitle_mode_is_kept_on_the_server() {
    let mut app = TestApp::open_on(Route::Settings { section: None });
    app.press(Key::Right);
    assert_eq!(app.focused(), Some(row(&app, SettingsSection::Profile, 1)));
    app.press_n(Key::Down, 2);
    assert_eq!(app.focused(), Some(row(&app, SettingsSection::Profile, 3)));
    app.press(Key::Ok);
    app.settle();
    assert!(app.shows("When the audio is in another language"));
    // Forced is in force.
    assert_eq!(app.focused(), Some(SettingsScreen::choice_id(app.page(), 1)));
    app.press_n(Key::Down, 2);
    app.press(Key::Ok);
    // Shown at once, then kept by the server.
    assert!(app.shows("Always"));
    assert!(app.until(|a| a.shell.toast() == Some("Saved")));
    let api = app.state.api().unwrap().clone();
    assert_eq!(api.me().unwrap().track_defaults().subtitle_mode, SubtitleMode::Always);
    app.settle();
    // "Always" reads a subtitle language.
    assert!(app.shows("Subtitle language"));
    assert!(app.shows("Same as the audio language"));
}

#[test]
fn defaults_without_the_play_scope_are_changed_on_the_web() {
    let mut app = TestApp::start(ids::ALEX, Some("library profiles"), Route::Settings { section: None });
    assert!(app.shows("English · change on the web"));
    app.press(Key::Right);
    app.press(Key::Down);
    app.press(Key::Ok);
    app.settle();
    assert!(app.modal());
    assert!(app.shows("Manage your account on the web"));
    let address = format!("{}/account", app.server.url().split_once("://").unwrap().1);
    assert!(app.shows(&address));
    app.press(Key::Back);
    app.settle();
    assert!(!app.modal());
    assert_eq!(app.route(), Some(&Route::Settings { section: None }));
}

#[test]
fn manage_on_the_web_shows_the_address_and_closes() {
    let mut app = TestApp::open_on(Route::Settings { section: None });
    app.press(Key::Right);
    // Down stops at the last row: Manage your account on the web.
    app.press_n(Key::Down, 12);
    app.press(Key::Ok);
    app.settle();
    assert!(app.modal());
    assert!(app.shows("Close"));
    let address = format!("{}/account", app.server.url().split_once("://").unwrap().1);
    assert!(app.shows(&address));
    app.press(Key::Ok);
    app.settle();
    assert!(!app.modal());
}

#[test]
fn always_start_as_is_kept_for_this_server() {
    let mut app = TestApp::open_on(Route::Settings { section: None });
    assert!(app.until(|a| a.shows("Always start as")));
    app.press(Key::Right);
    // Switch profile, Audio language, Subtitles, Language, Watch history, Sort libraries, Always
    // start as.
    app.press_n(Key::Down, 6);
    app.press(Key::Ok);
    app.settle();
    assert!(app.shows("Profile used last"));
    assert_eq!(app.focused(), Some(SettingsScreen::choice_id(app.page(), 0)));
    // Ask, Alex, Sam: Sam.
    app.press_n(Key::Down, 2);
    app.press(Key::Ok);
    app.settle();
    assert_eq!(app.state.always_start_as(), Some(ids::SAM));
    assert!(app.shows("Sam"));
}

#[test]
fn a_managed_profile_says_so() {
    let app = TestApp::start(ids::SAM, None, Route::Settings { section: None });
    let host = app.server.url().split_once("://").unwrap().1.to_string();
    assert!(app.shows("Sam"));
    assert!(app.shows(&format!("Managed profile · {host}")));
}

#[test]
fn watch_history_opens_from_profile() {
    let mut app = TestApp::open_on(Route::Settings { section: None });
    app.press(Key::Right);
    // Switch profile, Audio language, Subtitles, Language, Watch history.
    app.press_n(Key::Down, 4);
    app.press(Key::Ok);
    app.settle();
    assert_eq!(app.route(), Some(&Route::History));
}

#[test]
fn a_language_chosen_is_the_accounts_and_shows_at_once() {
    use pley_i18n::Language;
    let mut app = TestApp::open_on(Route::Settings { section: None });
    assert!(app.until(|a| a.shows("Automatic (English)")));
    app.press(Key::Right);
    // Switch profile, Audio language, Subtitles, Language.
    app.press_n(Key::Down, 3);
    app.press(Key::Ok);
    app.settle();
    for words in ["Automatic (English)", "English", "Français", "Español"] {
        assert!(app.shows(words), "{words}");
    }
    assert_eq!(app.focused(), Some(SettingsScreen::choice_id(app.page(), 0)));
    // Automatic, English, Français.
    app.press_n(Key::Down, 2);
    app.press(Key::Ok);
    assert!(app.until(|a| a.state.language() == Language::Fr), "the account's language never showed");
    assert_eq!(app.state.me().and_then(|m| m.language.as_deref()), Some("fr"));
    app.settle();
    // Settings is drawn again in French: the row is the account's language now, and keeps focus.
    assert!(app.shows("Langue") && app.shows("Français"));
    assert_eq!(app.focused(), Some(row(&app, SettingsSection::Profile, 4)));
}

#[test]
fn sign_out_asks_first_and_not_signing_out_comes_back_to_the_category() {
    let mut app = TestApp::open_on(settings(SettingsSection::SignOut));
    assert!(app.shows_start("Sign this TV out of 127.0.0.1:"));
    // OK on the category asks at once.
    app.press(Key::Ok);
    app.settle();
    assert!(app.modal());
    assert!(app.shows("Sign out?"));
    app.press(Key::Ok);
    app.settle();
    assert!(!app.modal());
    assert_eq!(app.focused(), Some(category(&app, SettingsSection::SignOut)));

    // From the row, Back on the question: focus goes to the category too.
    app.press(Key::Right);
    assert_eq!(app.focused(), Some(row(&app, SettingsSection::SignOut, 1)));
    app.press(Key::Ok);
    app.settle();
    assert!(app.modal());
    app.press(Key::Back);
    app.settle();
    assert!(!app.modal());
    assert_eq!(app.focused(), Some(category(&app, SettingsSection::SignOut)));
    assert_eq!(app.route(), Some(&settings(SettingsSection::SignOut)));
}

#[test]
fn signing_out_goes_to_the_servers_code_screen() {
    let mut app = TestApp::open_on(settings(SettingsSection::SignOut));
    let server = app.state.server().unwrap().to_string();
    app.press(Key::Ok);
    app.settle();
    app.press(Key::Right);
    app.press(Key::Ok);
    app.settle();
    assert_eq!(app.route(), Some(&Route::Link { server }));
}

#[test]
fn delete_all_local_data_asks_then_starts_over() {
    let mut app = TestApp::open_on(settings(SettingsSection::SignOut));
    app.press(Key::Right);
    app.press(Key::Down);
    assert_eq!(app.focused(), Some(row(&app, SettingsSection::SignOut, 2)));
    app.press(Key::Ok);
    app.settle();
    assert!(app.shows("Delete all local data?"));
    // Cancel has focus: Right then OK deletes.
    app.press(Key::Right);
    app.press(Key::Ok);
    app.settle();
    assert_eq!(app.route(), Some(&Route::Servers));
    assert_eq!(app.state.server(), None);
    assert!(app.state.saved_servers().is_empty());
}
