//! Appearance against the fake server: Settings' rows open it, a theme or an accent picked shows
//! in the next frame and is kept on the TV for the profile, each profile shows its own, and the
//! player draws a light theme's dark twin.

mod common;

use common::TestApp;
use pley_base::Key;
use pley_core::store::ThemePick;
use pley_fixtures::ids;
use pley_settings::{AppearanceScreen, SettingsScreen};
use pley_shell::{Route, SettingsSection};
use pley_ui::theme::{self, Accent, Palette, Theme};

fn theme_card(app: &TestApp, t: Theme) -> pley_ui::Id {
    AppearanceScreen::theme_id(app.page(), t)
}

fn accent_chip(app: &TestApp, a: Option<Accent>) -> pley_ui::Id {
    AppearanceScreen::accent_id(app.page(), a)
}

#[test]
fn the_rows_open_the_page_on_the_themes_or_the_accents() {
    let mut app = TestApp::open_on(Route::Settings { section: Some(SettingsSection::Appearance) });
    assert!(app.shows("Theme") && app.shows("Midnight") && app.shows("Default"));
    app.press(Key::Right);
    assert_eq!(app.focused(), Some(SettingsScreen::row_id(app.page(), SettingsSection::Appearance, 1)));
    app.press(Key::Ok);
    assert!(app.until(|a| a.route() == Some(&Route::Appearance { accents: false })));
    app.settle();
    assert_eq!(app.focused(), Some(theme_card(&app, Theme::Midnight)));
    for words in ["Appearance", "Midnight", "Lights off, popcorn on", "Sunshine", "Accent", "Default", "Amber"] {
        assert!(app.shows(words), "{words}");
    }

    let app = TestApp::open_on(Route::Appearance { accents: true });
    assert_eq!(app.focused(), Some(accent_chip(&app, None)));
}

#[test]
fn a_pick_shows_in_the_next_frame_and_stays_on_the_tv() {
    let mut app = TestApp::open_on(Route::Appearance { accents: false });
    assert_eq!(app.list.clear, Palette::of(Theme::Midnight, None).bg);
    // Midday, the next card.
    app.press(Key::Right);
    assert_eq!(app.focused(), Some(theme_card(&app, Theme::Midday)));
    app.press(Key::Ok);
    assert_eq!(app.state.settings().theme, "midday");
    assert_eq!(app.state.settings().accent, None);
    assert_eq!(theme::palette(), Palette::of(Theme::Midday, None));
    assert_eq!(app.list.clear, Palette::of(Theme::Midday, None).bg);

    // Down twice reaches the accents, on the one in force; Gold is four along.
    app.press_n(Key::Down, 2);
    assert_eq!(app.focused(), Some(accent_chip(&app, None)));
    app.press_n(Key::Right, 4);
    assert_eq!(app.focused(), Some(accent_chip(&app, Some(Accent::Gold))));
    app.press(Key::Ok);
    assert_eq!(app.state.settings().accent.as_deref(), Some("gold"));
    assert_eq!(theme::accent(), Palette::of(Theme::Midday, Some(Accent::Gold)).accent);
    // Kept on the TV, for Alex and as the one shown last.
    let kept = pley_core::store::Settings::load(app.state.store());
    assert_eq!((kept.theme.as_str(), kept.accent.as_deref()), ("midday", Some("gold")));
    let server = app.state.server().unwrap().to_string();
    assert_eq!(kept.theme_of(&server, ids::ALEX), pick("midday", Some("gold")));

    // Up from any accent lands on the theme in force, Down from the cards on the accent in force.
    app.press(Key::Right);
    app.press(Key::Up);
    assert_eq!(app.focused(), Some(theme_card(&app, Theme::Midday)));
    app.press(Key::Down);
    app.press(Key::Down);
    assert_eq!(app.focused(), Some(accent_chip(&app, Some(Accent::Gold))));

    // Default goes back to the theme's own.
    app.press_n(Key::Left, 4);
    app.press(Key::Ok);
    assert_eq!(app.state.settings().accent, None);
    assert_eq!(theme::accent(), Palette::of(Theme::Midday, None).accent);
}

fn pick(theme: &str, accent: Option<&str>) -> ThemePick {
    ThemePick { theme: theme.into(), accent: accent.map(Into::into) }
}

/// Switches the TV to `profile`, as "Who's watching?" does.
fn switch_to(app: &mut TestApp, profile: i64, pin: Option<&str>) {
    let mut asked = app.state.switch_profile(profile, pin.map(Into::into));
    assert!(app.until(|_| asked.ready() || asked.failed()));
    assert!(matches!(asked.take(), Some(Ok(()))), "the switch to {profile} was refused");
    app.state.switched(profile);
    assert!(app.until(|a| a.state.me().is_some_and(|m| m.id == profile)), "{profile} never came");
    app.settle();
}

#[test]
fn each_profile_shows_its_own_theme_and_a_start_the_last_shown() {
    let mut app = TestApp::open_on(Route::Appearance { accents: false });
    app.state.pick_theme(pick("sunshine", Some("mint")));
    app.settle();
    assert_eq!(theme::palette(), Palette::of(Theme::Sunshine, Some(Accent::Mint)));

    // A profile new to the TV shows Midnight in its own accent, at once.
    switch_to(&mut app, ids::SAM, Some("1234"));
    assert_eq!(theme::palette(), Palette::of(Theme::Midnight, None));
    assert_eq!(app.state.settings().last_theme(), pick("midnight", None));
    app.state.pick_theme(pick("blacklight", Some("amber")));
    app.settle();
    assert_eq!(theme::palette(), Palette::of(Theme::Blacklight, Some(Accent::Amber)));

    // Each keeps its own.
    switch_to(&mut app, ids::ALEX, None);
    assert_eq!(theme::palette(), Palette::of(Theme::Sunshine, Some(Accent::Mint)));
    switch_to(&mut app, ids::SAM, Some("1234"));
    assert_eq!(theme::palette(), Palette::of(Theme::Blacklight, Some(Accent::Amber)));

    // Before a profile is known (signed out, on the sign-in code), the one shown last shows.
    app.state.sign_out();
    app.open(Route::Link { server: app.state.server().unwrap_or("http://127.0.0.1").to_string() });
    app.settle();
    assert_eq!(app.route().map(Route::chrome), Some(pley_shell::Chrome::Bare));
    assert_eq!(theme::palette(), Palette::of(Theme::Blacklight, Some(Accent::Amber)));
    assert_eq!(app.state.settings().last_theme(), pick("blacklight", Some("amber")));
}

#[test]
fn a_player_draws_a_light_theme_in_its_dark_twin() {
    let mut app = TestApp::open_on(Route::Appearance { accents: false });
    app.state.pick_theme(pick("starlight", Some("mint")));
    app.settle();
    assert_eq!(theme::palette(), Palette::of(Theme::Starlight, Some(Accent::Mint)));
    app.open(Route::Player(pley_shell::PlayRequest::item(12)));
    app.settle();
    assert_eq!(theme::palette(), Palette::of(Theme::Moonlight, Some(Accent::Mint)));
    app.press(Key::Back);
    app.settle();
    assert_eq!(theme::palette(), Palette::of(Theme::Starlight, Some(Accent::Mint)));
}
