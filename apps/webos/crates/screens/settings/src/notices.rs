//! The open-source parts Pley for LG TV is built from, read from the NOTICES.txt its package
//! carries beside the binary (`cargo xtask notices` writes it: the bundled assets, the Rust
//! standard library, then each crate with its licence and texts).
//!
//! The file is read once per launch, off the main thread, and kept: About lists the parts and the
//! Licences page shows their texts.

use std::cell::RefCell;
use std::io::Read;
use std::path::PathBuf;
use std::sync::Arc;

use pley_core::{Promise, Tasks};

/// The line around each part's heading in NOTICES.txt.
const RULE: &str = "==============================================================================";

/// The most of NOTICES.txt read: the file is the app's own, but a file is never read unbounded.
pub const MAX_BYTES: u64 = 4 * 1024 * 1024;

/// One part and its licence.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Notice {
    /// `Roboto (the typeface)`, `serde 1.0.219`.
    pub name: String,
    /// `OFL-1.1`, `MIT OR Apache-2.0`.
    pub licence: String,
    /// Where it comes from, when the file says.
    pub link: Option<String>,
    /// Its licence texts and notices.
    pub text: String,
}

/// What NOTICES.txt says.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Notices {
    /// The lines before the first part.
    pub intro: String,
    pub parts: Vec<Notice>,
}

impl Notices {
    /// Each part's name and licence, as About lists them.
    pub fn list(&self) -> Vec<(String, String)> {
        self.parts.iter().map(|n| (n.name.clone(), n.licence.clone())).collect()
    }
}

/// Reads NOTICES.txt's text into its parts. A part starts with a rule, a heading line
/// (`name, licence`), perhaps an address, and a rule; its text runs to the next part.
pub fn parse(text: &str) -> Notices {
    let lines: Vec<&str> = text.lines().collect();
    // Where each heading's opening rule is, with the heading's lines and the line after the
    // closing rule.
    let mut heads: Vec<(usize, &[&str], usize)> = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        if lines[i] == RULE
            && let Some(close) = (i + 2..(i + 4).min(lines.len())).find(|&j| lines[j] == RULE)
        {
            heads.push((i, &lines[i + 1..close], close + 1));
            i = close + 1;
            continue;
        }
        i += 1;
    }
    let block = |from: usize, to: usize| lines[from..to].join("\n").trim_matches('\n').trim_end().to_string();
    let intro = block(0, heads.first().map_or(lines.len(), |h| h.0));
    let mut parts = Vec::with_capacity(heads.len());
    for (n, (_, head, body)) in heads.iter().enumerate() {
        let end = heads.get(n + 1).map_or(lines.len(), |h| h.0);
        let first = head.first().copied().unwrap_or("");
        // An SPDX expression has no comma, so the last one ends the name.
        let (name, licence) = first.rsplit_once(", ").unwrap_or((first, ""));
        parts.push(Notice {
            name: name.trim().to_string(),
            licence: licence.trim().to_string(),
            link: head.get(1).map(|l| l.trim().to_string()).filter(|l| !l.is_empty()),
            text: block(*body, end),
        });
    }
    Notices { intro, parts }
}

/// Where NOTICES.txt may be: beside the binary in the app's folder on the TV (found through the
/// running program, never `HOME`), else in the working folder or its `out` (where `cargo xtask
/// notices` writes it) while developing.
pub fn candidates() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(dir) = std::env::current_exe().ok().and_then(|exe| exe.parent().map(PathBuf::from)) {
        out.push(dir.join("NOTICES.txt"));
    }
    if let Ok(dir) = std::env::current_dir() {
        out.push(dir.join("NOTICES.txt"));
        out.push(dir.join("out").join("NOTICES.txt"));
    }
    out
}

/// NOTICES.txt from the first of `paths` that reads, or `None` when none does.
pub fn read(paths: &[PathBuf]) -> Option<Notices> {
    paths.iter().find_map(|path| {
        let file = std::fs::File::open(path).ok()?;
        let mut text = String::new();
        file.take(MAX_BYTES).read_to_string(&mut text).ok()?;
        Some(parse(&text))
    })
}

/// The file as this launch has it: being read, or read (`None` when the app carries none).
enum Shared {
    Idle,
    Reading(Promise<Option<Notices>>),
    Done(Option<Arc<Notices>>),
}

thread_local! {
    // Screens live on the main thread; the file is read once for all of them.
    static SHARED: RefCell<Shared> = const { RefCell::new(Shared::Idle) };
    static PATHS: RefCell<Option<Vec<PathBuf>>> = const { RefCell::new(None) };
}

/// Reads NOTICES.txt from `paths` instead of [`candidates`] from now on, on this thread (the
/// tests' own file, or none).
pub fn use_paths(paths: Vec<PathBuf>) {
    PATHS.with(|p| *p.borrow_mut() = Some(paths));
    SHARED.with(|cell| *cell.borrow_mut() = Shared::Idle);
}

/// NOTICES.txt once read: `None` while it is being read (the first call starts reading it on the
/// task pool, which wakes the app when done), then `Some(None)` when the app carries none.
pub fn shared(tasks: &Tasks) -> Option<Option<Arc<Notices>>> {
    SHARED.with(|cell| {
        let mut shared = cell.borrow_mut();
        if matches!(*shared, Shared::Idle) {
            let paths = PATHS.with(|p| p.borrow().clone()).unwrap_or_else(candidates);
            *shared = Shared::Reading(tasks.spawn(move || read(&paths)));
        }
        if let Shared::Reading(p) = &mut *shared {
            if let Some(read) = p.take() {
                *shared = Shared::Done(read.map(Arc::new));
            } else if p.failed() {
                *shared = Shared::Done(None);
            }
        }
        match &*shared {
            Shared::Done(n) => Some(n.clone()),
            _ => None,
        }
    })
}

thread_local! {
    static OPEN_AT: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Says which part the Licences page opens on: About's row chosen just before it opens.
pub fn open_at(index: usize) {
    OPEN_AT.with(|c| c.set(index));
}

/// The part the Licences page opens on, and forgets it (the next page opens on the first).
pub fn take_open_at() -> usize {
    OPEN_AT.with(|c| c.replace(0))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A NOTICES.txt as `cargo xtask notices` writes it (`xtask/src/licenses.rs`).
    fn sample() -> String {
        [
            "Pley for LG TV includes the following third-party software, each under its own licence.",
            "Pley itself is under the licence in LICENSE.txt (see LICENSING.md).",
            "",
            RULE,
            "Roboto (the typeface), OFL-1.1",
            RULE,
            "",
            "Copyright 2011 The Roboto Project Authors.",
            "",
            "SIL OPEN FONT LICENSE Version 1.1",
            "",
            RULE,
            "The Rust standard library, MIT OR Apache-2.0",
            "https://github.com/rust-lang/rust",
            RULE,
            "",
            "Copyright The Rust Project Developers.",
            "",
            RULE,
            "serde 1.0.219, MIT OR Apache-2.0",
            "https://github.com/serde-rs/serde",
            RULE,
            "",
            "--- LICENSE-MIT ---",
            "",
            "Permission is hereby granted, free of charge.",
            "",
            "--- LICENSE-APACHE: the same text as itoa 1.0.15's LICENSE-APACHE ---",
            "",
        ]
        .join("\n")
    }

    #[test]
    fn parts_are_read_with_their_licence_link_and_text() {
        let n = parse(&sample());
        assert!(n.intro.starts_with("Pley for LG TV includes"));
        assert_eq!(n.parts.len(), 3);
        let roboto = &n.parts[0];
        assert_eq!(
            (roboto.name.as_str(), roboto.licence.as_str(), roboto.link.as_deref()),
            ("Roboto (the typeface)", "OFL-1.1", None)
        );
        assert_eq!(roboto.text, "Copyright 2011 The Roboto Project Authors.\n\nSIL OPEN FONT LICENSE Version 1.1");
        let rust = &n.parts[1];
        assert_eq!(rust.name, "The Rust standard library");
        assert_eq!(rust.link.as_deref(), Some("https://github.com/rust-lang/rust"));
        let serde = &n.parts[2];
        assert_eq!((serde.name.as_str(), serde.licence.as_str()), ("serde 1.0.219", "MIT OR Apache-2.0"));
        assert!(serde.text.starts_with("--- LICENSE-MIT ---\n\nPermission"));
        assert!(serde.text.ends_with("LICENSE-APACHE ---"));
        assert_eq!(n.list()[2], ("serde 1.0.219".to_string(), "MIT OR Apache-2.0".to_string()));
    }

    #[test]
    fn a_file_without_parts_has_none() {
        assert_eq!(parse("Nothing here.\n").parts.len(), 0);
        assert_eq!(parse("").parts.len(), 0);
        // A rule with no closing rule is text, not a heading.
        assert_eq!(parse(&format!("{RULE}\nname, MIT\nno rule\nmore\n")).parts.len(), 0);
    }

    #[test]
    fn the_first_file_that_reads_is_used() {
        let dir = std::env::temp_dir().join(format!("pley-notices-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("NOTICES.txt");
        std::fs::write(&path, sample()).unwrap();
        let missing = dir.join("missing.txt");
        assert_eq!(read(&[missing.clone(), path.clone()]).map(|n| n.parts.len()), Some(3));
        assert_eq!(read(&[missing]), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_licences_page_opens_on_the_part_chosen_once() {
        open_at(2);
        assert_eq!(take_open_at(), 2);
        assert_eq!(take_open_at(), 0);
    }
}
