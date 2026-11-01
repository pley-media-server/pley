//! The shell: Back's four levels, the start gate, the rail's focus rules, the curtain, toasts and
//! the registry's placeholders.

mod common;

use std::cell::RefCell;
use std::rc::Rc;

use common::{TestApp, device, fixtures, http, registry, sign_in, store};
use pley_base::Key;
use pley_shell::{NavCommand, PLACEHOLDER_TEXT, Rail, Registry, Route};

fn heard() -> common::Heard {
    Rc::new(RefCell::new(Vec::new()))
}

#[test]
fn back_closes_the_page_s_own_then_pops_then_opens_the_rail_then_leaves() {
    let server = fixtures();
    let device = device();
    let (store, http) = (store(&device), http());
    sign_in(&server, pley_fixtures::ids::ALEX, &store, &http);
    let mut app = TestApp::new(device.clone(), store, http, registry(&heard(), &[]));
    assert_eq!(app.shell.top_route(), Some(&Route::Home));
    let home_button = app.button();
    assert_eq!(app.focused(), Some(home_button));

    app.press(Key::Ok);
    assert_eq!(app.shell.routes(), [&Route::Home, &Route::Item { id: 1 }]);
    assert_eq!(app.focused(), Some(app.button()));

    // 1. What the page has open closes first.
    app.press(Key::Menu);
    let panel_row = app.shell.top().unwrap().group_id().with("panel").with("row");
    assert_eq!(app.focused(), Some(panel_row));
    app.press(Key::Back);
    assert_eq!(app.shell.depth(), 2);
    assert_eq!(app.focused(), Some(app.button()));

    // 2. A page opened from another goes back to it, focus on what opened it.
    app.press(Key::Back);
    assert_eq!(app.shell.routes(), [&Route::Home]);
    assert_eq!(app.focused(), Some(home_button));

    // 3. A first page sends focus to the rail's current entry.
    app.press(Key::Back);
    assert!(app.shell.rail_open());
    assert_eq!(app.focused(), Some(Rail::entry_focus_id("home")));
    assert!(!device.exited());

    // 4. Back in the rail leaves the app.
    app.press(Key::Back);
    assert!(device.exited());
    assert!(app.shell.exited());
}

#[test]
fn back_without_the_rail_leaves_the_app() {
    let device = device();
    let mut app = TestApp::new(device.clone(), store(&device), http(), registry(&heard(), &[]));
    assert_eq!(app.shell.top_route(), Some(&Route::Servers));
    app.press(Key::Back);
    assert!(device.exited());
}

#[test]
fn the_start_gate_picks_the_first_page_from_what_the_tv_kept() {
    let server = fixtures();
    let url = server.url().to_string();

    // No server: Choose your server.
    let device = device();
    let app = TestApp::new(device.clone(), store(&device), http(), Registry::new());
    assert_eq!(app.shell.top_route(), Some(&Route::Servers));

    // A server without a sign-in: its code screen.
    let device = common::device();
    let s = store(&device);
    s.set_active_server(Some(&url)).unwrap();
    let app = TestApp::new(device.clone(), s, http(), Registry::new());
    assert_eq!(app.shell.top_route(), Some(&Route::Link { server: url.clone() }));

    // Signed in, a household of one (or unknown): Home.
    let device = common::device();
    let (s, h) = (store(&device), http());
    sign_in(&server, pley_fixtures::ids::ALEX, &s, &h);
    let app = TestApp::new(device.clone(), s.clone(), h.clone(), Registry::new());
    assert_eq!(app.shell.top_route(), Some(&Route::Home));
    drop(app);

    // A household of three: "Who's watching?"…
    let three = vec![
        pley_core::model::AppProfile { id: 1, name: "Alex".into(), ..Default::default() },
        pley_core::model::AppProfile { id: 2, name: "Sam".into(), ..Default::default() },
        pley_core::model::AppProfile { id: 3, name: "Robin".into(), ..Default::default() },
    ];
    s.set_household(&url, &three).unwrap();
    let mut app = TestApp::new(device.clone(), s.clone(), h.clone(), Registry::new());
    assert_eq!(app.shell.top_route(), Some(&Route::Profiles));
    assert_eq!(app.state.household_size(), 3);
    // …unless Settings' "Always start as" names one, or it was answered in this launch.
    app.state.set_always_start_as(Some(2));
    assert_eq!(app.state.start_route(), Route::Home);
    app.state.set_always_start_as(None);
    assert_eq!(app.state.start_route(), Route::Profiles);
    app.state.pass_profile_gate();
    assert_eq!(app.state.start_route(), Route::Home);
}

#[test]
fn left_at_the_page_s_edge_opens_the_rail_and_right_comes_back() {
    let server = fixtures();
    let device = device();
    let (store, http) = (store(&device), http());
    sign_in(&server, pley_fixtures::ids::ALEX, &store, &http);
    let mut app = TestApp::new(device, store, http, registry(&heard(), &[]));
    let button = app.button();
    app.press(Key::Left);
    assert!(app.shell.rail_open());
    assert_eq!(app.focused(), Some(Rail::entry_focus_id("home")));
    // Up and Down move along it; Left does nothing.
    app.press(Key::Up);
    assert_eq!(app.focused(), Some(Rail::entry_focus_id("search")));
    app.press(Key::Left);
    assert_eq!(app.focused(), Some(Rail::entry_focus_id("search")));
    app.press(Key::Right);
    assert!(!app.shell.rail_open());
    assert_eq!(app.focused(), Some(button));
    // A panel keeps Left to itself.
    app.press(Key::Menu);
    app.press(Key::Left);
    assert!(!app.shell.rail_open());
}

#[test]
fn a_page_chosen_in_the_rail_starts_the_stack_and_takes_focus_once_it_can() {
    let server = fixtures();
    let device = device();
    let (store, http) = (store(&device), http());
    sign_in(&server, pley_fixtures::ids::ALEX, &store, &http);
    let mut app = TestApp::new(device, store, http, registry(&heard(), &["search"]));
    app.press(Key::Ok);
    assert_eq!(app.shell.depth(), 2);
    app.press(Key::Back);
    app.press(Key::Back);
    assert!(app.shell.rail_open());

    // The page shown already: back to it, as it was.
    app.press(Key::Ok);
    assert!(!app.shell.rail_open());
    assert_eq!(app.shell.routes(), [&Route::Home]);

    // Another page: the stack starts over with it; it has nothing to focus yet, so the rail keeps
    // focus, open.
    app.press(Key::Left);
    app.press(Key::Up);
    app.press(Key::Ok);
    assert_eq!(app.shell.routes(), [&Route::Search { query: String::new() }]);
    assert!(app.shell.rail_open());
    assert_eq!(app.focused(), Some(Rail::entry_focus_id("search")));
}

#[test]
fn a_page_that_can_take_focus_at_once_takes_it_from_the_rail() {
    let server = fixtures();
    let device = device();
    let (store, http) = (store(&device), http());
    sign_in(&server, pley_fixtures::ids::ALEX, &store, &http);
    let mut app = TestApp::new(device, store, http, registry(&heard(), &[]));
    app.press(Key::Left);
    app.press(Key::Up);
    app.press(Key::Ok);
    assert_eq!(app.shell.routes(), [&Route::Search { query: String::new() }]);
    assert!(!app.shell.rail_open());
    assert_eq!(app.focused(), Some(app.button()));
    // The rail's current entry now is Search's.
    app.press(Key::Left);
    assert_eq!(app.focused(), Some(Rail::entry_focus_id("search")));
}

#[test]
fn pages_under_the_top_hear_what_changed_when_they_show_again() {
    let server = fixtures();
    let device = device();
    let (store, http) = (store(&device), http());
    sign_in(&server, pley_fixtures::ids::ALEX, &store, &http);
    let heard = heard();
    let mut app = TestApp::new(device, store, http, registry(&heard, &[]));
    app.press(Key::Ok);
    heard.borrow_mut().clear();
    app.state.refresh();
    app.frame(Vec::new());
    assert_eq!(heard.borrow().as_slice(), [(Route::Item { id: 1 }, pley_shell::AppEvent::Changed)]);
    app.press(Key::Back);
    let home: Vec<_> = heard.borrow().iter().filter(|(r, _)| *r == Route::Home).map(|(_, e)| e.clone()).collect();
    assert_eq!(home, [pley_shell::AppEvent::Shown, pley_shell::AppEvent::Changed]);
}

#[test]
fn every_route_has_a_screen_and_placeholders_name_theirs() {
    let device = device();
    let mut app = TestApp::new(device.clone(), store(&device), http(), Registry::new());
    for route in Route::samples() {
        app.navigate(NavCommand::Reset(route.clone()));
        assert_eq!(app.shell.top_route(), Some(&route));
        assert_eq!(app.shell.top().unwrap().screen().chrome(), route.chrome(), "{}", route.name());
        assert!(app.list.says(PLACEHOLDER_TEXT), "{}", route.name());
        assert!(app.list.says(&route.title()), "{}", route.name());
        // Its one button does what Back does.
        assert!(app.focused().is_some(), "{}", route.name());
    }
}

#[test]
fn a_new_page_comes_under_a_curtain() {
    let server = fixtures();
    let device = device();
    let (store, http) = (store(&device), http());
    sign_in(&server, pley_fixtures::ids::ALEX, &store, &http);
    let mut app = TestApp::new(device, store, http, Registry::new());
    let curtain = |app: &TestApp| {
        app.list.cmds.iter().any(|c| {
            matches!(c, pley_base::Cmd::Fill { rect, color, .. }
                if rect.w == 960.0 && rect.h == 540.0 && color.r == 0x0b && color.a > 0)
        })
    };
    // The first page comes without one.
    assert!(!curtain(&app));
    app.navigate(NavCommand::Push(Route::Item { id: 3 }));
    assert!(curtain(&app));
    app.advance(0.2);
    assert!(!curtain(&app));
    // None with Reduce motion.
    app.ui.set_reduced_motion(true);
    app.navigate(NavCommand::Push(Route::Person { id: 1 }));
    assert!(!curtain(&app));
}

#[test]
fn toasts_show_for_their_time() {
    let server = fixtures();
    let device = device();
    let (store, http) = (store(&device), http());
    sign_in(&server, pley_fixtures::ids::ALEX, &store, &http);
    let mut app = TestApp::new(device, store, http, registry(&heard(), &[]));
    app.navigate(NavCommand::Reset(Route::Settings { section: None }));
    app.press(Key::Ok);
    assert_eq!(app.shell.toast(), Some("Saved"));
    assert!(app.list.says("Saved"));
    app.advance(3.0);
    assert!(app.list.says("Saved"));
    app.advance(1.0);
    assert!(!app.list.says("Saved"));
    assert_eq!(app.shell.toast(), None);
}

#[test]
fn an_idle_screen_dims_and_the_key_that_wakes_it_does_nothing_else() {
    let server = fixtures();
    let device = device();
    let (store, http) = (store(&device), http());
    sign_in(&server, pley_fixtures::ids::ALEX, &store, &http);
    let mut app = TestApp::new(device, store, http, registry(&heard(), &[]));
    app.advance(pley_shell::DIM_AFTER + 1.0);
    assert!(app.shell.dimmed());
    app.press(Key::Ok);
    assert!(!app.shell.dimmed());
    assert_eq!(app.shell.routes(), [&Route::Home]);
    app.press(Key::Ok);
    assert_eq!(app.shell.depth(), 2);
}
