//! Pley for LG TV's build tasks, run from `apps/webos` as `cargo xtask <task>` (an alias in
//! `.cargo/config.toml`):
//!
//! - `build`: the TV's release binary for armv7-unknown-linux-gnueabi, keeping its symbols and
//!   with this machine's folders replaced in the paths it records;
//! - `check`: the licences of every crate the TV binary is built from, then the binary itself;
//! - `notices`: NOTICES.txt, the licences of what the binary and the package carry;
//! - `ipk`: `out/app.pley.tv_<version>_arm.ipk`, the same bytes from the same inputs;
//! - `ipk-verify`: reads an .ipk back and checks what the TV relies on.
//!
//! The TV build links with the webOS toolchain (`arm-webos-linux-gnueabi-gcc`, and its `-strip`
//! for `ipk`), which runs only on Linux: CI and `scripts/build-tv.ps1` (Docker) have it on
//! PATH. The other tasks run anywhere.

mod elf;
mod ipk;
mod licenses;
mod workspace;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use ipk::{AppInfo, File};
use workspace::{APP_BINARY, APP_PACKAGE, Graph, Metadata, TV_FEATURES, TV_TARGET, Workspace};

/// The tasks' result: any error, said in a sentence.
pub type Result<T, E = Box<dyn std::error::Error>> = std::result::Result<T, E>;

const USAGE: &str = "\
usage: cargo xtask <task>

  build [--locked]
      The TV's release binary, with its symbols, paths remapped.
  check [--binary <file>] [--target <triple>] [--licenses-only]
      The licences of the crates the binary is built from (for --target, by default the
      TV's), then the binary (by default the one `build` makes).
  notices [--target <triple>] [--out <file>]
      NOTICES.txt (by default out/NOTICES.txt).
  ipk [--no-build] [--locked] [--binary <file>] [--strip <tool> | --no-strip] [--out <folder>]
      Builds (unless --no-build) and packs out/app.pley.tv_<version>_arm.ipk, with a copy of
      the binary stripped by the toolchain's arm-webos-linux-gnueabi-strip.
  ipk-verify [<file.ipk>]
      Reads a package back (by default the one `ipk` writes) and checks it.
";

/// The toolchain's `strip`, which `ipk` runs on the copy of the binary it packs.
const STRIP: &str = "arm-webos-linux-gnueabi-strip";

fn main() -> ExitCode {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || args.iter().any(|a| a == "--help" || a == "-h" || a == "help") {
        print!("{USAGE}");
        return if args.is_empty() { ExitCode::FAILURE } else { ExitCode::SUCCESS };
    }
    let task = args.remove(0);
    match run(&task, Args(args)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(task: &str, mut args: Args) -> Result<()> {
    let ws = Workspace::locate();
    match task {
        "build" => {
            let locked = args.flag("--locked");
            args.done()?;
            build(&ws, locked)
        }
        "check" => {
            let binary = args.value("--binary")?.map(PathBuf::from);
            let target = args.value("--target")?.unwrap_or_else(|| TV_TARGET.to_owned());
            let licenses_only = args.flag("--licenses-only");
            args.done()?;
            check(&ws, binary, &target, licenses_only)
        }
        "notices" => {
            let target = args.value("--target")?.unwrap_or_else(|| TV_TARGET.to_owned());
            let out = args.value("--out")?.map_or_else(|| ws.out_dir().join("NOTICES.txt"), PathBuf::from);
            args.done()?;
            let text = notices(&ws, &ws.metadata(Some(&target))?, &target)?;
            if let Some(folder) = out.parent().filter(|p| !p.as_os_str().is_empty()) {
                fs::create_dir_all(folder)?;
            }
            fs::write(&out, text)?;
            println!("{}", out.display());
            Ok(())
        }
        "ipk" => {
            let options = IpkOptions {
                build: !args.flag("--no-build"),
                locked: args.flag("--locked"),
                binary: args.value("--binary")?.map(PathBuf::from),
                strip: match (args.value("--strip")?, args.flag("--no-strip")) {
                    (Some(_), true) => return Err("--strip and --no-strip together".into()),
                    (tool, false) => Some(tool.unwrap_or_else(|| STRIP.to_owned())),
                    (None, true) => None,
                },
                out: args.value("--out")?.map(PathBuf::from),
            };
            args.done()?;
            ipk(&ws, options).map(|_| ())
        }
        "ipk-verify" => {
            let files = args.done()?;
            if files.len() > 1 {
                return Err("ipk-verify reads one package at a time".into());
            }
            ipk_verify(&ws, files.into_iter().next().map(PathBuf::from))
        }
        _ => Err(format!("no task \"{task}\"\n\n{USAGE}").into()),
    }
}

/// The command line after the task: options are taken out as they are read.
struct Args(Vec<String>);

impl Args {
    fn flag(&mut self, name: &str) -> bool {
        let found = self.0.iter().position(|a| a == name);
        found.map(|i| self.0.remove(i)).is_some()
    }

    fn value(&mut self, name: &str) -> Result<Option<String>> {
        if let Some(i) = self.0.iter().position(|a| a == name) {
            if i + 1 >= self.0.len() {
                return Err(format!("{name} needs a value").into());
            }
            let value = self.0.remove(i + 1);
            self.0.remove(i);
            return Ok(Some(value));
        }
        let prefix = format!("{name}=");
        Ok(self.0.iter().position(|a| a.starts_with(&prefix)).map(|i| self.0.remove(i)[prefix.len()..].to_owned()))
    }

    /// What is left: the positional arguments, or an error naming an option no task reads.
    fn done(self) -> Result<Vec<String>> {
        match self.0.iter().find(|a| a.starts_with('-')) {
            Some(unknown) => Err(format!("unknown option {unknown}\n\n{USAGE}").into()),
            None => Ok(self.0),
        }
    }
}

/// Runs a command, failing if it does.
fn run_command(mut command: Command) -> Result<()> {
    eprintln!("> {command:?}");
    let status = command.status().map_err(|e| format!("{:?}: {e}", command.get_program()))?;
    if !status.success() {
        return Err(format!("{:?} failed ({status})", command.get_program()).into());
    }
    Ok(())
}

/// The release binary `build` makes.
fn default_binary(target_dir: &Path) -> PathBuf {
    target_dir.join(TV_TARGET).join("release").join(APP_BINARY)
}

/// The folders of this machine that paths compiled into the binary can start with, each with
/// the name it gets instead, broadest first (rustc applies the last prefix that matches): the
/// home folder, rustup's and cargo's (third-party sources), the repository and cargo's target
/// folder (generated sources). The binary then names no user and builds the same anywhere.
fn private_folders(ws: &Workspace, target_dir: &Path) -> Vec<(PathBuf, &'static str)> {
    let var = |name: &str| std::env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from);
    let home = var("HOME").or_else(|| var("USERPROFILE"));
    let rustup = var("RUSTUP_HOME").or_else(|| home.as_ref().map(|h| h.join(".rustup")));
    let cargo = var("CARGO_HOME").or_else(|| home.as_ref().map(|h| h.join(".cargo")));
    [
        (home, "/home"),
        (rustup, "/rustup"),
        (cargo, "/cargo"),
        (Some(ws.repo.clone()), "/pley"),
        (Some(target_dir.to_owned()), "/target"),
    ]
    .into_iter()
    .filter_map(|(from, to)| from.map(|from| (from, to)))
    .collect()
}

/// What a path that escaped the remapping looks like in the binary: each private folder with a
/// separator after it. A folder that is its own new name, or too short to tell from other text,
/// is left out.
fn leak_markers(ws: &Workspace, target_dir: &Path) -> Vec<String> {
    private_folders(ws, target_dir)
        .into_iter()
        .map(|(from, to)| (from.display().to_string(), to))
        .filter(|(from, to)| from.len() >= 5 && from != to)
        .map(|(from, _)| format!("{}{}", from.trim_end_matches(['/', '\\']), std::path::MAIN_SEPARATOR))
        .collect()
}

/// `cargo xtask build`.
fn build(ws: &Workspace, locked: bool) -> Result<()> {
    // Either variable replaces the target's flags in .cargo/config.toml instead of adding to them,
    // and the binary would quietly lose its CPU, float and link settings.
    for var in ["RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS"] {
        if std::env::var_os(var).is_some_and(|v| !v.is_empty()) {
            return Err(
                format!("{var} is set, which would replace the TV's flags in .cargo/config.toml: unset it").into()
            );
        }
    }
    let target_dir = ws.metadata(None)?.target_directory;
    let remaps: Vec<String> = private_folders(ws, &target_dir)
        .iter()
        .map(|(from, to)| format!("--remap-path-prefix={}={to}", from.display()))
        .collect();
    let mut command = ws.cargo();
    command.args(["build", "--release", "--target", TV_TARGET, "--package", APP_PACKAGE, "--bin", APP_BINARY]);
    command.args(TV_FEATURES);
    if locked {
        command.arg("--locked");
    }
    // Arrays given with --config add to the config file's: these come after its flags.
    command.arg("--config").arg(format!("target.{TV_TARGET}.rustflags={}", serde_json::to_string(&remaps)?));
    // The symbols stay for `check` and for reading crash reports; `ipk` packs a stripped copy.
    command.args(["--config", "profile.release.strip=false"]);
    run_command(command)?;
    println!("{}", default_binary(&target_dir).display());
    Ok(())
}

/// `cargo xtask check`.
fn check(ws: &Workspace, binary: Option<PathBuf>, target: &str, licenses_only: bool) -> Result<()> {
    let meta = ws.metadata(Some(target))?;
    let deny = ws.repo.join("deny.toml");
    let allowed =
        licenses::allowed_licenses(&fs::read_to_string(&deny).map_err(|e| format!("{}: {e}", deny.display()))?)?;
    // Build scripts and procedural macros run while building, but their licences count too,
    // as in scripts/check-licenses.ps1 and cargo-deny.
    let graph = meta.select(&ws.tree(target, Graph::Build)?)?;
    let mut wrong = licenses::problems(&graph, &allowed);
    println!("{APP_PACKAGE} for {target}: {} crates, licences checked against deny.toml", graph.len());

    if !licenses_only {
        let path = binary.unwrap_or_else(|| default_binary(&meta.target_directory));
        let data =
            fs::read(&path).map_err(|e| format!("{}: {e} (make it with `cargo xtask build`)", path.display()))?;
        let (facts, problems) = elf::check(&data, &leak_markers(ws, &meta.target_directory), elf::SIZE_BUDGET)?;
        println!("{}: needs {}", path.display(), facts.needed.join(" "));
        if let Some((major, minor)) = facts.newest_glibc {
            println!(
                "  newest glibc version needed: {major}.{minor} (webOS 4.0 has {}.{})",
                elf::GLIBC_FLOOR.0,
                elf::GLIBC_FLOOR.1
            );
        }
        println!("  {} bytes once stripped (budget {})", facts.stripped_size, elf::SIZE_BUDGET);
        wrong.extend(problems);
    }
    if wrong.is_empty() {
        println!("check passed");
        return Ok(());
    }
    for problem in &wrong {
        eprintln!("  {problem}");
    }
    Err(format!("check failed: {} problem{}", wrong.len(), if wrong.len() == 1 { "" } else { "s" }).into())
}

/// NOTICES.txt for the crates compiled into the app's binary for `target` (its normal
/// dependencies, without build scripts' or procedural macros', which stay on the build
/// machine) and the bundled assets.
fn notices(ws: &Workspace, meta: &Metadata, target: &str) -> Result<String> {
    let release = meta.select(&ws.tree(target, Graph::Release)?)?;
    let (text, without) = licenses::notices(&release, &ws.root)?;
    if !without.is_empty() {
        eprintln!(
            "note: these packages carry no licence text, so NOTICES.txt names their licence: {}",
            without.join(", ")
        );
    }
    Ok(text)
}

/// What `cargo xtask ipk` was asked.
struct IpkOptions {
    build: bool,
    locked: bool,
    binary: Option<PathBuf>,
    /// The `strip` to run on the packed copy, or none to pack the binary as it is.
    strip: Option<String>,
    out: Option<PathBuf>,
}

/// `cargo xtask ipk`: the package's path.
fn ipk(ws: &Workspace, options: IpkOptions) -> Result<PathBuf> {
    if options.build {
        build(ws, options.locked)?;
    }
    let appinfo = fs::read(ws.root.join("appinfo.json"))?;
    let info = AppInfo::parse(&appinfo)?;
    let meta = ws.metadata(Some(TV_TARGET))?;
    let app = meta.package(APP_PACKAGE)?;
    if app.version != info.version {
        return Err(format!(
            "appinfo.json says {} and {APP_PACKAGE} {}: set both with `bun scripts/version.mjs set lg <version>`",
            info.version, app.version
        )
        .into());
    }
    if info.main != APP_BINARY {
        return Err(format!("appinfo.json's main is {}, not {APP_BINARY}", info.main).into());
    }
    let binary = options.binary.unwrap_or_else(|| default_binary(&meta.target_directory));
    let program = match &options.strip {
        Some(tool) => strip(tool, &binary)?,
        None => fs::read(&binary).map_err(|e| format!("{}: {e}", binary.display()))?,
    };

    let dir = info.app_dir();
    let mut files = vec![
        File::new(format!("{dir}/appinfo.json"), appinfo, false),
        File::new(format!("{dir}/{}", info.main), program, true),
        File::new(format!("{dir}/LICENSE.txt"), fs::read(ws.repo.join("LICENSE"))?, false),
        File::new(format!("{dir}/LICENSING.md"), fs::read(ws.repo.join("LICENSING.md"))?, false),
        File::new(format!("{dir}/NOTICES.txt"), notices(ws, &meta, TV_TARGET)?.into_bytes(), false),
    ];
    for (picture, _) in &info.pictures {
        let path = ws.root.join("assets").join(picture);
        files.push(File::new(
            format!("{dir}/{picture}"),
            fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?,
            false,
        ));
    }
    for folder in ["assets/fonts", "assets/licenses"] {
        add_folder(&mut files, &ws.root.join(folder), &format!("{dir}/{folder}"))?;
    }
    let bytes = ipk::build(&info, files)?;
    let out = options.out.unwrap_or_else(|| ws.out_dir());
    fs::create_dir_all(&out)?;
    let path = out.join(info.file_name());
    fs::write(&path, &bytes)?;
    println!("{} ({} bytes)", path.display(), bytes.len());
    Ok(path)
}

/// Adds every file under `folder` to the package, under `to`.
fn add_folder(files: &mut Vec<File>, folder: &Path, to: &str) -> Result<()> {
    let mut entries: Vec<_> =
        fs::read_dir(folder).map_err(|e| format!("{}: {e}", folder.display()))?.collect::<Result<_, _>>()?;
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let name = entry.file_name().to_str().ok_or("a file name that isn't UTF-8")?.to_owned();
        let path = entry.path();
        if path.is_dir() {
            add_folder(files, &path, &format!("{to}/{name}"))?;
        } else {
            files.push(File::new(format!("{to}/{name}"), fs::read(&path)?, false));
        }
    }
    Ok(())
}

/// The binary without its symbol tables, as the toolchain's `strip` writes it.
fn strip(tool: &str, binary: &Path) -> Result<Vec<u8>> {
    let stripped = binary.with_extension("stripped");
    let mut command = Command::new(tool);
    command.arg("--strip-all").arg("-o").arg(&stripped).arg(binary);
    run_command(command)
        .map_err(|e| format!("{e} (the webOS toolchain's strip; --no-strip packs the binary as it is)"))?;
    Ok(fs::read(&stripped)?)
}

/// `cargo xtask ipk-verify`.
fn ipk_verify(ws: &Workspace, file: Option<PathBuf>) -> Result<()> {
    let path = match file {
        Some(file) => file,
        None => ws.out_dir().join(AppInfo::parse(&fs::read(ws.root.join("appinfo.json"))?)?.file_name()),
    };
    let bytes = fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let (summary, wrong) = ipk::verify(&bytes)?;
    println!(
        "{}: {} bytes, {} files ({} bytes) installed",
        path.display(),
        bytes.len(),
        summary.files,
        summary.payload
    );
    for (key, value) in &summary.control {
        println!("  {key}: {value}");
    }
    if wrong.is_empty() {
        println!("ipk-verify passed");
        return Ok(());
    }
    for problem in &wrong {
        eprintln!("  {problem}");
    }
    Err(format!("ipk-verify failed: {} problem{}", wrong.len(), if wrong.len() == 1 { "" } else { "s" }).into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Args {
        Args(list.iter().map(|s| s.to_string()).collect())
    }

    #[test]
    fn reads_flags_and_values() {
        let mut a = args(&["--no-build", "--binary", "x/pley", "--out=dist", "file.ipk"]);
        assert!(a.flag("--no-build"));
        assert!(!a.flag("--locked"));
        assert_eq!(a.value("--binary").unwrap().as_deref(), Some("x/pley"));
        assert_eq!(a.value("--out").unwrap().as_deref(), Some("dist"));
        assert_eq!(a.value("--strip").unwrap(), None);
        assert_eq!(a.done().unwrap(), ["file.ipk"]);
        assert!(args(&["--binary"]).value("--binary").is_err());
        assert!(args(&["--bogus"]).done().is_err());
    }

    #[test]
    fn remaps_the_broadest_folder_first() {
        let ws = Workspace::locate();
        let folders = private_folders(&ws, Path::new("/somewhere/target"));
        let names: Vec<&str> = folders.iter().map(|(_, to)| *to).collect();
        assert_eq!(&names[names.len() - 2..], ["/pley", "/target"]);
        assert!(ws.repo.join("deny.toml").is_file(), "{}", ws.repo.display());
        assert!(ws.root.join("appinfo.json").is_file());

        let markers = leak_markers(&ws, Path::new("/target"));
        let sep = std::path::MAIN_SEPARATOR;
        assert!(markers.contains(&format!("{}{sep}", ws.repo.display())), "{markers:?}");
        assert!(!markers.iter().any(|m| m.starts_with("/target")), "a folder that is its own new name: {markers:?}");
    }

    #[test]
    fn appinfo_matches_the_app_package() {
        let ws = Workspace::locate();
        let info = AppInfo::parse(&fs::read(ws.root.join("appinfo.json")).unwrap()).unwrap();
        assert_eq!(info.id, "app.pley.tv");
        assert_eq!(info.main, APP_BINARY);
        let manifest = fs::read_to_string(ws.root.join("crates/app/Cargo.toml")).unwrap();
        assert!(manifest.contains(&format!("name = \"{APP_PACKAGE}\"\nversion = \"{}\"", info.version)), "{manifest}");
        for (picture, size) in &info.pictures {
            let data = fs::read(ws.root.join("assets").join(picture)).unwrap();
            let found = (
                u32::from_be_bytes(data[16..20].try_into().unwrap()),
                u32::from_be_bytes(data[20..24].try_into().unwrap()),
            );
            assert_eq!(found, *size, "{picture}");
        }
    }
}
