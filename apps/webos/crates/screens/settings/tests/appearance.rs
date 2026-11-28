//! Appearance against the fake server: Settings' rows open it, a theme or an accent picked shows
//! in the next frame and is kept on the TV for the profile, each profile shows its own and the
//! pages outside a profile Midnight, the player draws a light theme's dark twin, what has focus
//! has one outline, and the picked theme's check sits at its preview's bottom right.

mod common;

use common::TestApp;
use pley_base::{Cmd, Color, Key, Rect};
use pley_core::store::ThemePick;
use pley_fixtures::ids;
use pley_settings::{AppearanceScreen, SettingsScreen};
use pley_shell::{Route, SettingsSection};
use pley_ui::theme::{self, Accent, Palette, Theme, focus, radius};

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
    assert_eq!(app.state.theme_pick(), pick("midday", None));
    assert_eq!(theme::palette(), Palette::of(Theme::Midday, None));
    assert_eq!(app.list.clear, Palette::of(Theme::Midday, None).bg);

    // Down twice reaches the accents, on the one in force; Gold is four along.
    app.press_n(Key::Down, 2);
    assert_eq!(app.focused(), Some(accent_chip(&app, None)));
    app.press_n(Key::Right, 4);
    assert_eq!(app.focused(), Some(accent_chip(&app, Some(Accent::Gold))));
    app.press(Key::Ok);
    assert_eq!(app.state.theme_pick(), pick("midday", Some("gold")));
    assert_eq!(theme::accent(), Palette::of(Theme::Midday, Some(Accent::Gold)).accent);
    // Kept on the TV, for Alex.
    let kept = pley_core::store::Settings::load(app.state.store());
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
    assert_eq!(app.state.theme_pick(), pick("midday", None));
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
fn each_profile_shows_its_own_theme_and_outside_a_profile_midnight() {
    let mut app = TestApp::open_on(Route::Appearance { accents: false });
    app.state.pick_theme(pick("sunshine", Some("mint")));
    app.settle();
    assert_eq!(theme::palette(), Palette::of(Theme::Sunshine, Some(Accent::Mint)));

    // A profile new to the TV shows Midnight in its own accent, at once.
    switch_to(&mut app, ids::SAM, Some("1234"));
    assert_eq!(theme::palette(), Palette::of(Theme::Midnight, None));
    app.state.pick_theme(pick("blacklight", Some("amber")));
    app.settle();
    assert_eq!(theme::palette(), Palette::of(Theme::Blacklight, Some(Accent::Amber)));

    // Each keeps its own.
    switch_to(&mut app, ids::ALEX, None);
    assert_eq!(theme::palette(), Palette::of(Theme::Sunshine, Some(Accent::Mint)));
    switch_to(&mut app, ids::SAM, Some("1234"));
    assert_eq!(theme::palette(), Palette::of(Theme::Blacklight, Some(Accent::Amber)));

    // "Who's watching?" is no profile's: Midnight in its own accent, then the profile's again
    // once Back leaves it.
    app.open(Route::Profiles);
    app.settle();
    assert_eq!(app.route(), Some(&Route::Profiles));
    assert_eq!(theme::palette(), Palette::of(Theme::Midnight, None));
    app.press(Key::Back);
    app.settle();
    assert_eq!(app.route(), Some(&Route::Appearance { accents: false }));
    assert_eq!(theme::palette(), Palette::of(Theme::Blacklight, Some(Accent::Amber)));

    // Signed out, on the sign-in code: Midnight, and the picks stay with their profiles.
    app.state.sign_out();
    app.open(Route::Link { server: app.state.server().unwrap_or("http://127.0.0.1").to_string() });
    app.settle();
    assert_eq!(app.route().map(Route::chrome), Some(pley_shell::Chrome::Bare));
    assert_eq!(theme::palette(), Palette::of(Theme::Midnight, None));
    let server = app.server.url().to_string();
    assert_eq!(app.state.settings().theme_of(&server, ids::SAM), pick("blacklight", Some("amber")));
}

/// A start with a saved sign-in draws its profile's pick from its first frame, before the server
/// has said a word.
#[test]
fn a_start_draws_the_saved_profile_in_its_pick_from_the_first_frame() {
    let route = Route::Settings { section: None };
    let mut app = TestApp::launch(
        ids::ALEX,
        None,
        route,
        |_| {},
        |store, server| {
            store.set_last_profile(server, ids::ALEX).unwrap();
            let mut settings = pley_core::store::Settings::default();
            settings.set_theme(server, ids::ALEX, pick("twilight", Some("gold")));
            settings.save(store).unwrap();
        },
    );
    assert!(app.state.me().is_none());
    assert_eq!(theme::palette(), Palette::of(Theme::Twilight, Some(Accent::Gold)));
    assert_eq!(app.list.clear, Palette::of(Theme::Twilight, Some(Accent::Gold)).bg);
    assert!(app.until(|a| a.state.me().is_some()));
    app.settle();
    assert_eq!(theme::palette(), Palette::of(Theme::Twilight, Some(Accent::Gold)));
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

/// How many outlines of `color` and `width` the last frame drew.
fn strokes(app: &TestApp, color: Color, width: f32) -> usize {
    let drawn = |c: &&Cmd| matches!(c, Cmd::Stroke { color: c, width: w, .. } if *c == color && *w == width);
    app.list.cmds.iter().filter(drawn).count()
}

#[test]
fn what_has_focus_has_one_outline_the_ring_in_place_of_its_edge() {
    let mut app = TestApp::open_on(Route::Appearance { accents: false });
    app.settle();
    let ring = |app: &TestApp| strokes(app, focus::ring_color(), focus::RING_STROKE);
    let picked = |app: &TestApp| strokes(app, theme::accent(), 2.0);
    let edges = |app: &TestApp| strokes(app, theme::surface4(), 1.0);
    // On Midnight, the theme in force: the ring, without the accent edge beside it.
    assert_eq!(app.focused(), Some(theme_card(&app, Theme::Midnight)));
    assert_eq!((ring(&app), picked(&app)), (1, 0));
    let all = edges(&app);
    // On Midday: Midnight's edge is back, and Midday's gives way to the ring.
    app.press(Key::Right);
    app.settle();
    assert_eq!((ring(&app), picked(&app), edges(&app)), (1, 1, all - 1));
    // On Sky's chip, the same.
    app.press_n(Key::Down, 2);
    app.press(Key::Right);
    app.settle();
    assert_eq!(app.focused(), Some(accent_chip(&app, Some(Accent::Sky))));
    assert_eq!((ring(&app), picked(&app), edges(&app)), (1, 1, all - 1));
}

/// The first theme's preview (Midnight's, by its 1 dp edge) and the check's disk on it.
fn preview_and_check(app: &TestApp) -> (Rect, Rect) {
    let edge = theme::fg().alpha(0.1);
    let preview = app.list.cmds.iter().find_map(|c| match c {
        Cmd::Stroke { rect, color, radius: r, width } if *color == edge && *r == radius::CARD && *width == 1.0 => {
            Some(*rect)
        }
        _ => None,
    });
    let disk = app.list.cmds.iter().find_map(|c| match c {
        Cmd::Fill { rect, color, .. } if *color == theme::accent() && rect.w == 22.0 && rect.h == 22.0 => Some(*rect),
        _ => None,
    });
    (preview.expect("Midnight's preview"), disk.expect("the check"))
}

#[test]
fn the_picked_themes_check_sits_at_its_previews_bottom_right() {
    let mut app = TestApp::open_on(Route::Appearance { accents: false });
    app.settle();
    for focused in [true, false] {
        assert_eq!(app.focused() == Some(theme_card(&app, Theme::Midnight)), focused);
        let (preview, disk) = preview_and_check(&app);
        // 5 in from the right and bottom edges (10 px at 1080p, as on every TV app), below the accent
        // pill at the preview's top right.
        assert_eq!((disk.right(), disk.bottom()), (preview.right() - 5.0, preview.bottom() - 5.0), "{focused}");
        assert!(disk.y > preview.center().y, "{focused}");
        app.press(Key::Right);
        app.settle();
    }
}
