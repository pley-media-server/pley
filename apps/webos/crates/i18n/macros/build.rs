//! Builds the English catalogs into the macro, which checks keys against them: every
//! `../locales/en/<area>.json`, listed in `english.rs` with `include_str!`, so a word added or
//! changed builds the macro, and the code that uses it, again.

use std::fmt::Write as _;
use std::path::Path;
use std::{env, fs};

fn main() {
    let manifest = Path::new(&env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR")).to_path_buf();
    let root = manifest.join("..").join("locales").join("en");
    println!("cargo:rerun-if-changed={}", root.display());
    let mut out = String::from("/// The English catalogs: each area's name and its JSON.\n");
    out.push_str("static ENGLISH: &[(&str, &str)] = &[\n");
    let mut files: Vec<_> = fs::read_dir(&root).expect("locales/en is there").flatten().map(|e| e.path()).collect();
    files.sort();
    for file in files.iter().filter(|p| p.extension().is_some_and(|e| e == "json")) {
        let area = file.file_stem().and_then(|n| n.to_str()).expect("an area's name");
        let path = file.to_str().expect("a catalog's path");
        writeln!(out, "    ({area:?}, include_str!({path:?})),").expect("writing to a string");
    }
    out.push_str("];\n");
    let target = Path::new(&env::var("OUT_DIR").expect("cargo sets OUT_DIR")).join("english.rs");
    fs::write(target, out).expect("english.rs is written");
}
