//! Third-party licences: the allow list in `deny.toml`, SPDX expressions, and NOTICES.txt.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use crate::Result;
use crate::workspace::Package;

/// The licences `deny.toml`'s `[licenses]` table allows (its `allow` array).
pub fn allowed_licenses(deny_toml: &str) -> Result<Vec<String>> {
    let mut table = String::new();
    let mut collecting = false;
    let mut allowed = Vec::new();
    for raw in deny_toml.lines() {
        let line = raw.split('#').next().unwrap_or_default().trim();
        if !collecting {
            if line.starts_with('[') {
                table = line.trim_matches(['[', ']']).trim().to_owned();
                continue;
            }
            let Some(value) = line.strip_prefix("allow").map(str::trim_start) else { continue };
            let Some(value) = value.strip_prefix('=') else { continue };
            if table != "licenses" {
                continue;
            }
            collecting = true;
            collect_strings(value, &mut allowed);
            if value.contains(']') {
                break;
            }
        } else {
            collect_strings(line, &mut allowed);
            if line.contains(']') {
                break;
            }
        }
    }
    if allowed.is_empty() {
        return Err("deny.toml: no [licenses] allow list".into());
    }
    Ok(allowed)
}

/// Appends every double-quoted string on the line.
fn collect_strings(line: &str, into: &mut Vec<String>) {
    let mut parts = line.split('"');
    parts.next();
    while let (Some(text), Some(_)) = (parts.next(), parts.next()) {
        into.push(text.to_owned());
    }
}

/// Whether the SPDX licence expression is satisfied by the allowed licences: an `OR` needs one
/// side allowed, an `AND` both. `A WITH exception` counts as `A` (an exception only adds
/// permissions) unless the list names the pair. The legacy `/` separator reads as `OR`.
pub fn is_allowed(expression: &str, allowed: &[String]) -> Result<bool> {
    let spaced = expression.replace('(', " ( ").replace(')', " ) ").replace('/', " OR ");
    let tokens: Vec<&str> = spaced.split_whitespace().collect();
    let mut parser = Parser { tokens: &tokens, at: 0, allowed };
    let value = parser.or()?;
    if parser.at != tokens.len() {
        return Err(format!("can't read the licence expression \"{expression}\"").into());
    }
    Ok(value)
}

struct Parser<'a> {
    tokens: &'a [&'a str],
    at: usize,
    allowed: &'a [String],
}

impl Parser<'_> {
    fn peek_is(&self, word: &str) -> bool {
        self.tokens.get(self.at).is_some_and(|t| t.eq_ignore_ascii_case(word))
    }

    fn or(&mut self) -> Result<bool> {
        let mut value = self.and()?;
        while self.peek_is("OR") {
            self.at += 1;
            value |= self.and()?;
        }
        Ok(value)
    }

    fn and(&mut self) -> Result<bool> {
        let mut value = self.with()?;
        while self.peek_is("AND") {
            self.at += 1;
            value &= self.with()?;
        }
        Ok(value)
    }

    fn with(&mut self) -> Result<bool> {
        if self.peek_is("(") {
            self.at += 1;
            let value = self.or()?;
            if !self.peek_is(")") {
                return Err("a licence expression with an unclosed parenthesis".into());
            }
            self.at += 1;
            return Ok(value);
        }
        let id = *self.tokens.get(self.at).ok_or("a licence expression that ends too soon")?;
        if ["AND", "OR", "WITH", ")"].iter().any(|op| id.eq_ignore_ascii_case(op)) {
            return Err(format!("a licence expression with \"{id}\" where a licence belongs").into());
        }
        self.at += 1;
        let id = id.trim_end_matches('+');
        let mut value = self.names(id);
        if self.peek_is("WITH") {
            let exception = *self.tokens.get(self.at + 1).ok_or("a licence expression that ends after WITH")?;
            self.at += 2;
            value |= self.names(&format!("{id} WITH {exception}"));
        }
        Ok(value)
    }

    fn names(&self, id: &str) -> bool {
        self.allowed.iter().any(|a| a.split_whitespace().collect::<Vec<_>>().join(" ").eq_ignore_ascii_case(id))
    }
}

/// What is wrong with the packages' licences: third-party crates (the workspace's own are
/// Pley's, unpublished, like `[licenses.private]` in deny.toml) whose expression the allow
/// list doesn't satisfy, or that declare none.
pub fn problems(packages: &[&Package], allowed: &[String]) -> Vec<String> {
    let mut wrong = Vec::new();
    for p in packages.iter().filter(|p| p.source.is_some()) {
        match &p.license {
            None => wrong.push(format!(
                "{} {} declares no licence expression{}",
                p.name,
                p.version,
                p.license_file.as_deref().map(|f| format!(" (only a file, {f})")).unwrap_or_default()
            )),
            Some(expression) => match is_allowed(expression, allowed) {
                Ok(true) => {}
                Ok(false) => {
                    wrong.push(format!("{} {} is {expression}, which deny.toml doesn't allow", p.name, p.version))
                }
                Err(e) => wrong.push(format!("{} {}: {e}", p.name, p.version)),
            },
        }
    }
    wrong
}

/// What the app ships besides crates: (name, what it is, licence, the text's file under `apps/webos`).
pub const ASSETS: &[(&str, &str, &str, &str)] = &[
    ("Roboto", "the typeface", "OFL-1.1", "assets/fonts/OFL.txt"),
    ("Lucide", "the icons", "ISC", "assets/licenses/lucide.txt"),
];

/// NOTICES.txt: the bundled assets' licences, then each third-party crate compiled into the
/// binary with its licence expression and the licence texts its package carries. A text that
/// another crate already gave word for word is named, not repeated: most of them are the same
/// Apache-2.0 text. Returns the text and the crates that carry no licence text.
pub fn notices(packages: &[&Package], workspace_root: &Path) -> Result<(String, Vec<String>)> {
    let rule = "=".repeat(78);
    let mut out = String::from(
        "Pley for LG TV includes the following third-party software, each under its own licence.\n\
         Pley itself is under the licence in LICENSE.txt (see LICENSING.md).\n",
    );
    for (name, what, license, file) in ASSETS {
        let text = fs::read_to_string(workspace_root.join(file)).map_err(|e| format!("{file}: {e}"))?;
        write!(out, "\n{rule}\n{name} ({what}), {license}\n{rule}\n\n{}\n", normalise(&text))?;
    }
    // Compiled in like a crate, though cargo doesn't list it.
    write!(
        out,
        "\n{rule}\nThe Rust standard library, MIT OR Apache-2.0\nhttps://github.com/rust-lang/rust\n{rule}\n\n\
         Copyright The Rust Project Developers. The texts of both licences are among the crates' below; the \
         notices of\nthe code it is built from are at https://github.com/rust-lang/rust/blob/master/COPYRIGHT.\n"
    )?;
    // Each text given so far, and where.
    let mut given: HashMap<String, String> = HashMap::new();
    let mut without_text = Vec::new();
    for p in packages.iter().filter(|p| p.source.is_some()) {
        let license = p.license.as_deref().unwrap_or("see its licence file");
        write!(out, "\n{rule}\n{} {}, {license}\n", p.name, p.version)?;
        if let Some(repository) = &p.repository {
            writeln!(out, "{repository}")?;
        }
        writeln!(out, "{rule}")?;
        let dir = p.manifest_path.parent().ok_or("a manifest without a folder")?;
        let texts = license_texts(dir)?;
        if texts.is_empty() {
            without_text.push(format!("{} {}", p.name, p.version));
            let authors = if p.authors.is_empty() { "its authors".to_owned() } else { p.authors.join(", ") };
            write!(out, "\nCopyright {authors}. Licensed under {license}; the package carries no licence text.\n")?;
        }
        for (file, text) in texts {
            let text = normalise(&text);
            match given.get(&text) {
                Some(first) => writeln!(out, "\n--- {file}: the same text as {first} ---")?,
                None => {
                    write!(out, "\n--- {file} ---\n\n{text}\n")?;
                    given.insert(text, format!("{} {}'s {file}", p.name, p.version));
                }
            }
        }
    }
    Ok((out, without_text))
}

/// The licence texts in a crate's folder (LICENSE*, LICENCE*, COPYING*, NOTICE*, UNLICENSE*,
/// COPYRIGHT*, and the files of a LICENSES folder), sorted by name.
fn license_texts(dir: &Path) -> Result<Vec<(String, String)>> {
    let is_license = |name: &str| {
        let upper = name.to_ascii_uppercase();
        ["LICENSE", "LICENCE", "COPYING", "NOTICE", "UNLICENSE", "COPYRIGHT"].iter().any(|p| upper.starts_with(p))
    };
    let mut texts = Vec::new();
    let mut entries: Vec<_> = fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !is_license(&name) {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            let mut inner: Vec<_> = fs::read_dir(&path)?.flatten().filter(|e| e.path().is_file()).collect();
            inner.sort_by_key(|e| e.file_name());
            for file in inner {
                if let Ok(text) = fs::read_to_string(file.path()) {
                    texts.push((format!("{name}/{}", file.file_name().to_string_lossy()), text));
                }
            }
        } else if let Ok(text) = fs::read_to_string(&path) {
            texts.push((name, text));
        }
    }
    Ok(texts)
}

/// LF line endings, no trailing spaces, no blank lines at either end.
fn normalise(text: &str) -> String {
    let lines: Vec<&str> = text.lines().map(str::trim_end).collect();
    lines.join("\n").trim_matches('\n').to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::Workspace;
    use std::path::PathBuf;

    fn allow(ids: &[&str]) -> Vec<String> {
        ids.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn reads_the_repository_allow_list() {
        let ws = Workspace::locate();
        let text = fs::read_to_string(ws.repo.join("deny.toml")).unwrap();
        let allowed = allowed_licenses(&text).unwrap();
        for id in ["MIT", "Apache-2.0", "Apache-2.0 WITH LLVM-exception", "ISC", "Zlib", "Unicode-3.0"] {
            assert!(allowed.iter().any(|a| a == id), "{id} in {allowed:?}");
        }
        assert!(!allowed.iter().any(|a| a.contains("GPL")));
    }

    #[test]
    fn reads_only_the_licenses_table() {
        let text = "[bans]\nallow = [\"x\"]\n\n[licenses]\n# allow = [\"commented\"]\nallow = [\n    \"MIT\", # why\n    \"ISC\",\n]\n[other]\nallow = [\"y\"]\n";
        assert_eq!(allowed_licenses(text).unwrap(), ["MIT", "ISC"]);
        assert_eq!(allowed_licenses("[licenses]\nallow = [\"MIT\", \"Zlib\"]\n").unwrap(), ["MIT", "Zlib"]);
        assert!(allowed_licenses("[licenses]\nconfidence-threshold = 0.9\n").is_err());
    }

    #[test]
    fn evaluates_spdx_expressions() {
        let allowed = allow(&["MIT", "Apache-2.0", "Unicode-3.0", "BSD-3-Clause"]);
        let ok = |e: &str| is_allowed(e, &allowed).unwrap();
        assert!(ok("MIT"));
        assert!(ok("MIT OR Apache-2.0"));
        assert!(ok("GPL-3.0-only OR MIT"));
        assert!(!ok("GPL-3.0-only"));
        assert!(!ok("LGPL-2.1-or-later AND MIT"));
        assert!(ok("(MIT OR Apache-2.0) AND Unicode-3.0"));
        assert!(!ok("(MIT OR Apache-2.0) AND MPL-2.0"));
        assert!(ok("MIT/Apache-2.0"), "the legacy slash");
        assert!(ok("Apache-2.0 WITH LLVM-exception"));
        assert!(ok("mit or apache-2.0"), "case doesn't matter");
        assert!(ok("Apache-2.0+"));
        assert!(!ok("GPL-2.0 WITH Classpath-exception-2.0"));
        assert!(ok("Zlib OR (Apache-2.0 AND (MIT OR BSD-3-Clause))"));
        assert!(is_allowed("MIT OR", &allowed).is_err());
        assert!(is_allowed("(MIT", &allowed).is_err());
        assert!(is_allowed("MIT Apache-2.0", &allowed).is_err());
    }

    #[test]
    fn a_listed_exception_pair_is_allowed() {
        let allowed = allow(&["GPL-2.0  WITH  Classpath-exception-2.0"]);
        assert!(is_allowed("GPL-2.0 WITH Classpath-exception-2.0", &allowed).unwrap());
        assert!(!is_allowed("GPL-2.0", &allowed).unwrap());
    }

    fn package(name: &str, license: Option<&str>, dir: PathBuf, source: Option<&str>) -> Package {
        Package {
            name: name.into(),
            version: "1.0.0".into(),
            license: license.map(Into::into),
            license_file: None,
            manifest_path: dir.join("Cargo.toml"),
            source: source.map(Into::into),
            authors: vec!["Someone".into()],
            repository: Some(format!("https://example.com/{name}")),
        }
    }

    #[test]
    fn flags_disallowed_and_undeclared_licences_but_not_the_workspace() {
        let allowed = allow(&["MIT"]);
        let own = package("pley-lg", Some("LicenseRef-Restricted"), PathBuf::from("."), None);
        let good = package("good", Some("MIT"), PathBuf::from("."), Some("registry"));
        let bad = package("bad", Some("GPL-3.0-only"), PathBuf::from("."), Some("registry"));
        let none = package("none", None, PathBuf::from("."), Some("registry"));
        let wrong = problems(&[&own, &good, &bad, &none], &allowed);
        assert_eq!(wrong.len(), 2, "{wrong:?}");
        assert!(wrong[0].starts_with("bad 1.0.0 is GPL-3.0-only"));
        assert!(wrong[1].starts_with("none 1.0.0 declares no licence"));
    }

    #[test]
    fn notices_carry_the_assets_and_each_crates_texts() {
        let ws = Workspace::locate();
        let dir = std::env::temp_dir().join(format!("pley-xtask-notices-{}", std::process::id()));
        let with = dir.join("with");
        let without = dir.join("without");
        let same = dir.join("same");
        fs::create_dir_all(with.join("LICENSES")).unwrap();
        fs::create_dir_all(&without).unwrap();
        fs::create_dir_all(&same).unwrap();
        fs::write(with.join("LICENSE-MIT"), "MIT text  \r\nsecond line\r\n\r\n").unwrap();
        fs::write(with.join("LICENSES").join("Apache-2.0.txt"), "Apache text\n").unwrap();
        fs::write(with.join("README.md"), "not a licence").unwrap();
        fs::write(same.join("LICENSE-APACHE"), "Apache text").unwrap();
        let a = package("alpha", Some("MIT OR Apache-2.0"), with, Some("registry"));
        let b = package("beta", Some("MIT"), without, Some("registry"));
        let c = package("gamma", Some("Apache-2.0"), same, Some("registry"));
        let own = package("pley-lg", Some("LicenseRef-Restricted"), dir.clone(), None);
        let (text, missing) = notices(&[&a, &b, &c, &own], &ws.root).unwrap();
        fs::remove_dir_all(&dir).unwrap();
        assert!(text.contains("--- LICENSE-APACHE: the same text as alpha 1.0.0's LICENSES/Apache-2.0.txt ---\n"));
        assert_eq!(text.matches("Apache text").count(), 1);
        assert!(text.contains("Roboto (the typeface), OFL-1.1"));
        assert!(text.contains("SIL Open Font License"));
        assert!(text.contains("Lucide (the icons), ISC"));
        assert!(text.contains("The Rust standard library, MIT OR Apache-2.0"));
        assert!(text.contains("alpha 1.0.0, MIT OR Apache-2.0\nhttps://example.com/alpha\n"));
        assert!(text.contains("--- LICENSE-MIT ---\n\nMIT text\nsecond line\n"));
        assert!(text.contains("--- LICENSES/Apache-2.0.txt ---\n\nApache text\n"));
        assert!(!text.contains("not a licence"));
        assert!(text.contains("Copyright Someone. Licensed under MIT; the package carries no licence text."));
        assert!(!text.contains("pley-lg"), "Pley's own crates aren't third-party");
        assert!(!text.contains('\r'));
        assert_eq!(missing, ["beta 1.0.0"]);
    }
}
