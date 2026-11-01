//! The workspace as cargo describes it (`cargo metadata`, `cargo tree`), and running cargo in it.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

use crate::Result;

/// The TV's target: the ARMv7 soft-float ABI of webOS userland (DESIGN.md §7).
pub const TV_TARGET: &str = "armv7-unknown-linux-gnueabi";
/// The package that builds the app's binary.
pub const APP_PACKAGE: &str = "pley-lg";
/// The app's binary, `main` in `appinfo.json`.
pub const APP_BINARY: &str = "pley";
/// How the TV's binary is built: the firmware's SDL2, luna-service2 and media pipeline instead
/// of the simulator's desktop window and fake server (`crates/app/Cargo.toml`).
pub const TV_FEATURES: [&str; 3] = ["--no-default-features", "--features", "tv"];

/// Which of the binary's dependencies `cargo tree` follows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Graph {
    /// What is compiled into the binary: normal dependencies, without procedural macros.
    Release,
    /// Everything it is built from: normal and build dependencies and procedural macros, not
    /// dev-dependencies. The licence rules hold for all of it, as in cargo-deny.
    Build,
}

/// Where things are: `apps/webos` (the workspace) and the repository.
pub struct Workspace {
    /// `apps/webos`, the workspace's root, where cargo must run for `.cargo/config.toml` to apply.
    pub root: PathBuf,
    /// The repository's root (`LICENSE`, `LICENSING.md`, `deny.toml`).
    pub repo: PathBuf,
}

impl Workspace {
    /// The workspace this xtask was built in.
    pub fn locate() -> Workspace {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().expect("xtask sits in the workspace").to_path_buf();
        // apps/webos → apps → the repository. Not canonicalised: on Windows that adds `\\?\`,
        // which matches none of the paths the compiler records.
        let repo = root.parent().and_then(Path::parent).expect("apps/webos sits two folders down").to_path_buf();
        Workspace { root, repo }
    }

    /// Where the packages go: `apps/webos/out`.
    pub fn out_dir(&self) -> PathBuf {
        self.root.join("out")
    }

    /// A cargo command run in the workspace's root, with the cargo that runs this xtask.
    pub fn cargo(&self) -> Command {
        let mut command = Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()));
        command.current_dir(&self.root);
        command
    }

    /// Every package of the workspace's resolved graph, with the TV's features as well as the
    /// defaults, limited to what `platform` builds when one is given.
    pub fn metadata(&self, platform: Option<&str>) -> Result<Metadata> {
        let mut command = self.cargo();
        command.args(["metadata", "--format-version", "1", "--features", &format!("{APP_PACKAGE}/tv")]);
        if let Some(platform) = platform {
            command.args(["--filter-platform", platform]);
        }
        Metadata::parse(&serde_json::from_slice(&output(command)?)?)
    }

    /// The (name, version) of every package the TV's binary reaches for `target`, as `cargo
    /// build` resolves it with the TV's features. `cargo metadata` can't say this: it resolves
    /// the whole workspace's features at once.
    pub fn tree(&self, target: &str, graph: Graph) -> Result<BTreeSet<(String, String)>> {
        let mut command = self.cargo();
        let edges = match graph {
            Graph::Release => "normal,no-proc-macro",
            Graph::Build => "normal,build",
        };
        command.args(["tree", "--package", APP_PACKAGE, "--target", target, "--edges", edges]);
        command.args(["--prefix", "none", "--format", "{p}"]).args(TV_FEATURES);
        Ok(parse_tree(&String::from_utf8_lossy(&output(command)?)))
    }
}

/// A command's output, or an error with what it said.
fn output(mut command: Command) -> Result<Vec<u8>> {
    let output = command.output().map_err(|e| format!("{command:?}: {e}"))?;
    if !output.status.success() {
        return Err(format!("{command:?} failed:\n{}", String::from_utf8_lossy(&output.stderr)).into());
    }
    Ok(output.stdout)
}

/// `cargo tree --prefix none --format {p}`'s lines (`name v1.2.3`, then a source or `(*)`) as
/// (name, version).
fn parse_tree(text: &str) -> BTreeSet<(String, String)> {
    text.lines()
        .filter_map(|line| {
            let mut words = line.split_whitespace();
            let name = words.next()?;
            let version = words.next()?.strip_prefix('v')?;
            Some((name.to_owned(), version.to_owned()))
        })
        .collect()
}

/// One package of the graph.
#[derive(Clone, Debug)]
pub struct Package {
    /// Its name.
    pub name: String,
    /// Its version.
    pub version: String,
    /// Its SPDX licence expression, if it declares one.
    pub license: Option<String>,
    /// The licence file it names instead, if any.
    pub license_file: Option<String>,
    /// Its `Cargo.toml`, in the cargo registry for a third-party crate.
    pub manifest_path: PathBuf,
    /// Where it comes from: `None` for the workspace's own packages.
    pub source: Option<String>,
    /// Its authors, as its manifest lists them.
    pub authors: Vec<String>,
    /// Its repository's address, if it gives one.
    pub repository: Option<String>,
}

/// The parts of `cargo metadata` the tasks use.
pub struct Metadata {
    /// Every package in the graph.
    pub packages: Vec<Package>,
    /// Cargo's target folder.
    pub target_directory: PathBuf,
}

impl Metadata {
    /// Reads `cargo metadata --format-version 1`'s output.
    pub fn parse(json: &Value) -> Result<Metadata> {
        let str_of = |v: &Value, key: &str| v.get(key).and_then(Value::as_str).map(str::to_owned);
        let mut packages = Vec::new();
        for p in json["packages"].as_array().ok_or("cargo metadata: no packages")? {
            packages.push(Package {
                name: str_of(p, "name").ok_or("cargo metadata: a package without a name")?,
                version: str_of(p, "version").unwrap_or_default(),
                license: str_of(p, "license"),
                license_file: str_of(p, "license_file"),
                manifest_path: str_of(p, "manifest_path").unwrap_or_default().into(),
                source: str_of(p, "source"),
                authors: p["authors"]
                    .as_array()
                    .map(|a| a.iter().filter_map(|a| a.as_str().map(str::to_owned)).collect())
                    .unwrap_or_default(),
                repository: str_of(p, "repository"),
            });
        }
        let target_directory = str_of(json, "target_directory").ok_or("cargo metadata: no target_directory")?.into();
        Ok(Metadata { packages, target_directory })
    }

    /// The workspace's package called `name`.
    pub fn package(&self, name: &str) -> Result<&Package> {
        self.packages
            .iter()
            .find(|p| p.name == name && p.source.is_none())
            .ok_or_else(|| format!("no package {name} in the workspace").into())
    }

    /// The packages `tree` names, sorted by name and version; an error for one it doesn't know.
    pub fn select(&self, tree: &BTreeSet<(String, String)>) -> Result<Vec<&Package>> {
        tree.iter()
            .map(|(name, version)| {
                self.packages.iter().find(|p| &p.name == name && &p.version == version).ok_or_else(|| {
                    format!("cargo tree names {name} {version}, which cargo metadata doesn't list").into()
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_cargo_tree_lines() {
        let text = "pley-lg v0.1.0 (C:\\pley\\apps\\webos\\crates\\app)\nlog v0.4.29\npley-base v0.1.0 (/w/crates/base)\n\
                    log v0.4.29 (*)\nserde v1.0.229\n\n";
        let tree = parse_tree(text);
        let names: Vec<String> = tree.iter().map(|(n, v)| format!("{n}@{v}")).collect();
        assert_eq!(names, ["log@0.4.29", "pley-base@0.1.0", "pley-lg@0.1.0", "serde@1.0.229"]);
    }

    #[test]
    fn reads_metadata_and_selects_packages() {
        let json = serde_json::json!({
            "packages": [
                {"name": "pley-lg", "version": "0.1.0", "license": "LicenseRef-Restricted",
                 "manifest_path": "/w/crates/app/Cargo.toml", "source": null, "authors": []},
                {"name": "alpha", "version": "1.0.0", "license": "MIT", "manifest_path": "/r/alpha/Cargo.toml",
                 "source": "registry+https://github.com/rust-lang/crates.io-index", "authors": ["A"],
                 "repository": "https://example.com/alpha"},
                {"name": "alpha", "version": "2.0.0", "license": null, "license_file": "LICENSE",
                 "manifest_path": "/r/alpha2/Cargo.toml", "source": "registry+x", "authors": []}
            ],
            "target_directory": "/w/target"
        });
        let meta = Metadata::parse(&json).unwrap();
        assert_eq!(meta.target_directory, PathBuf::from("/w/target"));
        assert!(meta.package("pley-lg").unwrap().source.is_none());
        assert!(meta.package("alpha").is_err(), "a registry crate isn't a workspace package");
        let tree =
            BTreeSet::from([("alpha".to_owned(), "2.0.0".to_owned()), ("pley-lg".to_owned(), "0.1.0".to_owned())]);
        let selected = meta.select(&tree).unwrap();
        assert_eq!(selected.len(), 2);
        assert_eq!((selected[0].name.as_str(), selected[0].version.as_str()), ("alpha", "2.0.0"));
        assert_eq!(selected[0].license_file.as_deref(), Some("LICENSE"));
        let unknown = BTreeSet::from([("beta".to_owned(), "1.0.0".to_owned())]);
        assert!(meta.select(&unknown).is_err());
    }
}
