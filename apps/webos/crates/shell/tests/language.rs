//! What the app words away from the interface's thread follows the language shown. This test
//! makes its thread the interface's, as the app's start does, so it has a binary of its own.

mod common;

use std::cell::RefCell;
use std::rc::Rc;

use common::{Heard, TestApp, device, http, registry, store};
use pley_i18n::Language;

#[test]
fn diagnostics_lines_are_worded_again_in_the_language_picked() {
    pley_i18n::share_from_this_thread();
    let heard: Heard = Rc::new(RefCell::new(Vec::new()));
    let device = device();
    let (store, http) = (store(&device), http());
    let mut app = TestApp::new(device, store, http, registry(&heard, &[]));
    app.state.probe_caps(false);
    app.settle();
    let panel = |app: &TestApp| app.state.caps().and_then(|r| r.lines.get(2).cloned());
    assert_eq!(panel(&app).as_deref(), Some("Panel: 4K OLED"));
    // The TV probed in English, then French picked: the lines are worded again, in French.
    app.state.choose_language_here(Some(Language::Fr));
    app.settle();
    assert_eq!(app.state.language(), Language::Fr);
    assert_eq!(panel(&app).as_deref(), Some("Dalle\u{a0}: 4K OLED"));
}
