//! Diagnostics and the licences: the TV's facts, what it plays, the network and the log, each line
//! taking focus; About's parts and their texts from a NOTICES.txt; the page without one.

mod common;

use std::path::PathBuf;

use common::TestApp;
use pley_base::Key;
use pley_settings::{LicencesScreen, notices};
use pley_shell::{Route, SettingsSection};

#[test]
fn diagnostics_lists_the_tv_what_it_plays_the_network_and_the_log() {
    let mut app = TestApp::open_on(Route::Diagnostics);
    assert!(app.until(|a| a.shows("webOS 22") && !a.shows("Checking what this TV plays…")));
    assert!(app.shows("Diagnostics"));
    assert!(app.shows("This TV"));
    assert!(app.shows("App"));
    assert!(app.shows_start("Pley "));
    assert!(app.shows("OLED65C2PUA"));
    assert!(app.shows("What it plays"));
    assert!(app.shows_start("TV: OLED65C2PUA"));
    // The first fact has focus; Down goes line by line.
    let lines = app.page().with("lines");
    assert_eq!(app.focused(), Some(lines.with(2u64)));
    app.press(Key::Down);
    assert_eq!(app.focused(), Some(lines.with(3u64)));
    // Down to the end: past what it plays, the network and the log.
    app.press_n(Key::Down, 40);
    assert!(app.shows("Network"));
    assert!(app.shows("Connected"));
    assert!(app.shows("Log"));
}

/// A NOTICES.txt as `cargo xtask notices` writes it, in a folder of the test's own.
fn notices_file(name: &str) -> PathBuf {
    let rule = "=".repeat(78);
    let mut text = String::from("Pley for LG TV includes the following third-party software.\n");
    text +=
        &format!("\n{rule}\nRoboto (the typeface), OFL-1.1\n{rule}\n\nCopyright 2011 The Roboto Project Authors.\n");
    for line in 0..80 {
        text += &format!("Line {line} of the Open Font License.\n");
    }
    text += &format!("\n{rule}\nserde 1.0.219, MIT OR Apache-2.0\nhttps://github.com/serde-rs/serde\n{rule}\n\n");
    text += "--- LICENSE-MIT ---\n\nPermission is hereby granted, free of charge.\n";
    let dir = std::env::temp_dir().join(format!("pley-settings-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("NOTICES.txt");
    std::fs::write(&path, text).unwrap();
    path
}

#[test]
fn about_lists_the_parts_and_opens_their_texts() {
    notices::use_paths(vec![notices_file("about")]);
    let mut app = TestApp::open_on(Route::Settings { section: Some(SettingsSection::About) });
    assert!(app.until(|a| a.shows("Roboto (the typeface)")));
    assert!(app.shows("serde 1.0.219"));
    assert!(app.shows("MIT OR Apache-2.0"));
    // Version, then the parts under their heading: the second part.
    app.press(Key::Right);
    app.press_n(Key::Down, 2);
    app.press(Key::Ok);
    app.settle();
    assert_eq!(app.route(), Some(&Route::Licences));
    assert!(app.until(|a| a.shows("Permission is hereby granted, free of charge.")));
    assert_eq!(app.focused(), Some(LicencesScreen::row_id(app.page(), 1)));
    assert!(app.shows("https://github.com/serde-rs/serde"));
}

#[test]
fn the_licence_text_takes_focus_and_scrolls() {
    notices::use_paths(vec![notices_file("scroll")]);
    let mut app = TestApp::open_on(Route::Licences);
    assert!(app.until(|a| a.shows("Line 0 of the Open Font License.")));
    assert_eq!(app.focused(), Some(LicencesScreen::row_id(app.page(), 0)));
    assert!(!app.shows("Line 79 of the Open Font License."));
    app.press(Key::Right);
    let text = LicencesScreen::text_id(app.page());
    assert_eq!(app.focused(), Some(text));
    app.press_n(Key::Down, 60);
    app.settle();
    assert_eq!(app.focused(), Some(text));
    assert!(app.shows("Line 79 of the Open Font License."));
    assert!(!app.shows("Line 0 of the Open Font License."));
    // Left goes back to the parts; Down there shows the next part's text.
    app.press(Key::Left);
    assert_eq!(app.focused(), Some(LicencesScreen::row_id(app.page(), 0)));
    app.press(Key::Down);
    assert!(app.shows("Permission is hereby granted, free of charge."));
}

#[test]
fn without_notices_the_page_says_so_with_back() {
    notices::use_paths(Vec::new());
    let mut app = TestApp::open_on(Route::Settings { section: Some(SettingsSection::About) });
    // About says the file is missing, as the Licences page does.
    assert!(app.until(|a| a.shows(pley_settings::licences::no_notices())));
    // TMDB's credit comes after the licences, whatever the file holds.
    assert!(app.shows("Credits") && app.shows("TMDB"));
    assert!(app.shows(pley_core::requests::tmdb_notice()));
    app.open(Route::Licences);
    assert!(app.until(|a| a.shows("No licence texts")));
    app.settle();
    app.press(Key::Ok);
    app.settle();
    assert_eq!(app.route(), Some(&Route::Settings { section: Some(SettingsSection::About) }));
}
