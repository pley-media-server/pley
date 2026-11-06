//! Builds the catalogs into the crate: every `locales/<language>/<area>.json`, listed in
//! `catalogs.rs` with `include_str!`, so a file added or changed builds again.

use std::fmt::Write as _;
use std::path::Path;
use std::{env, fs};

fn main() {
    let root = Path::new(&env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR")).join("locales");
    println!("cargo:rerun-if-changed={}", root.display());
    let mut out = String::from("/// Every catalog built in: its language, its area and its JSON.\n");
    out.push_str("pub(crate) static CATALOGS: &[(&str, &str, &str)] = &[\n");
    let mut languages: Vec<_> = fs::read_dir(&root).expect("locales/ is there").flatten().map(|e| e.path()).collect();
    languages.sort();
    for dir in languages.iter().filter(|p| p.is_dir()) {
        let language = dir.file_name().and_then(|n| n.to_str()).expect("a language folder's name");
        let mut files: Vec<_> = fs::read_dir(dir).expect("a language folder").flatten().map(|e| e.path()).collect();
        files.sort();
        for file in files.iter().filter(|p| p.extension().is_some_and(|e| e == "json")) {
            let area = file.file_stem().and_then(|n| n.to_str()).expect("an area's name");
            let path = file.to_str().expect("a catalog's path");
            writeln!(out, "    ({language:?}, {area:?}, include_str!({path:?})),").expect("writing to a string");
        }
    }
    out.push_str("];\n");
    let target = Path::new(&env::var("OUT_DIR").expect("cargo sets OUT_DIR")).join("catalogs.rs");
    fs::write(target, out).expect("catalogs.rs is written");
}
