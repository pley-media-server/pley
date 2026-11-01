//! The .ipk a TV installs: an `ar` archive of `debian-binary`, `control.tar.gz` and
//! `data.tar.gz` (DESIGN.md §5.6; RES/plx-build.md §6), the same bytes from the same inputs.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Write};

use flate2::read::GzDecoder;
use flate2::{Compression, GzBuilder};
use serde_json::Value;

use crate::Result;

/// `Maintainer` in the control file. LG's tools want the form `Name <a@b.tld>`; the project's
/// GitHub address receives no mail.
pub const MAINTAINER: &str = "Pley <pley-media-server@users.noreply.github.com>";
/// `Homepage` in the control file.
pub const HOMEPAGE: &str = "https://github.com/pley-media-server/pley";
/// The date of every entry: 2020-01-01T00:00:00Z. A fixed date keeps the package reproducible;
/// not 1970, which TVs refuse (RES/webos-ecosystem.md §0).
pub const MTIME: u64 = 1_577_836_800;
/// The `ar` members, in the order and with the bare names the TV's installer looks for: GNU
/// `ar`'s `name/` makes it fail with "Failed to extract package" (RES/plx-build.md §6.1).
pub const MEMBERS: [&str; 3] = ["debian-binary", "control.tar.gz", "data.tar.gz"];
/// Id prefixes reserved for the system's own apps.
const RESERVED_IDS: [&str; 4] = ["com.palm", "com.webos", "com.lge", "com.palmdts"];

/// One file of the package.
#[derive(Clone, Debug)]
pub struct File {
    /// Its path from the install prefix, `/`-separated (`usr/palm/applications/…`).
    pub path: String,
    /// Its bytes.
    pub data: Vec<u8>,
    /// Whether it is a program (mode 0755; 0644 otherwise).
    pub executable: bool,
}

impl File {
    /// A file of the package.
    pub fn new(path: impl Into<String>, data: Vec<u8>, executable: bool) -> File {
        File { path: path.into(), data, executable }
    }
}

/// What the package says about the app, from its `appinfo.json`.
#[derive(Clone, Debug)]
pub struct AppInfo {
    /// The app's id, `app.pley.tv`.
    pub id: String,
    /// Its version: three integers.
    pub version: String,
    /// Its title in the TV's launcher.
    pub title: String,
    /// Its vendor.
    pub vendor: String,
    /// The program SAM starts, relative to the app's folder.
    pub main: String,
    /// The pictures it names (`icon`, `largeIcon`, `splashBackground`), with their required sizes.
    pub pictures: Vec<(String, (u32, u32))>,
}

/// The pictures `appinfo.json` may name and the sizes LG's launcher wants (RES/plx-build.md §6.2).
const PICTURES: [(&str, (u32, u32)); 3] =
    [("icon", (80, 80)), ("largeIcon", (130, 130)), ("splashBackground", (1920, 1080))];

impl AppInfo {
    /// Reads and checks an `appinfo.json`: a native app with an allowed id and a version of three integers.
    pub fn parse(bytes: &[u8]) -> Result<AppInfo> {
        let json: Value = serde_json::from_slice(bytes).map_err(|e| format!("appinfo.json: {e}"))?;
        let text = |key: &str| -> Result<String> {
            json.get(key)
                .and_then(Value::as_str)
                .map(str::to_owned)
                .ok_or_else(|| format!("appinfo.json: no \"{key}\"").into())
        };
        let info = AppInfo {
            id: text("id")?,
            version: text("version")?,
            title: text("title")?,
            vendor: text("vendor")?,
            main: text("main")?,
            pictures: PICTURES
                .iter()
                .filter_map(|(key, size)| json.get(*key).and_then(Value::as_str).map(|v| (v.to_owned(), *size)))
                .collect(),
        };
        if text("type")? != "native" {
            return Err("appinfo.json: \"type\" isn't \"native\"".into());
        }
        if let Some(problem) = id_problem(&info.id) {
            return Err(format!("appinfo.json: {problem}").into());
        }
        if !is_version(&info.version) {
            return Err(format!("appinfo.json: version \"{}\" isn't three integers", info.version).into());
        }
        if info.main.contains(['/', '\\']) {
            return Err("appinfo.json: \"main\" must be a file in the app's folder".into());
        }
        Ok(info)
    }

    /// The app's folder in the package.
    pub fn app_dir(&self) -> String {
        format!("usr/palm/applications/{}", self.id)
    }

    /// The package descriptor's path, which the TV's installer reads first.
    pub fn packageinfo_path(&self) -> String {
        format!("usr/palm/packages/{}/packageinfo.json", self.id)
    }

    /// The package's file name: `<id>_<version>_arm.ipk`.
    pub fn file_name(&self) -> String {
        format!("{}_{}_arm.ipk", self.id, self.version)
    }

    /// `packageinfo.json`, keyed and indented as LG's packager writes it.
    pub fn packageinfo(&self) -> String {
        let s = |v: &str| Value::from(v).to_string();
        format!(
            "{{\n    \"app\": {},\n    \"id\": {},\n    \"loc_name\": {},\n    \"package_format_version\": 2,\n    \"vendor\": {},\n    \"version\": {}\n}}\n",
            s(&self.id),
            s(&self.id),
            s(&self.title),
            s(&self.vendor),
            s(&self.version)
        )
    }

    /// The control file for a payload of `installed_kib`.
    pub fn control(&self, installed_kib: u64) -> String {
        format!(
            "Package: {}\nVersion: {}\nSection: misc\nPriority: optional\nArchitecture: arm\nInstalled-Size: {installed_kib}\n\
             Maintainer: {MAINTAINER}\nHomepage: {HOMEPAGE}\nLicense: LicenseRef-Restricted\nDescription: Pley for LG TV\n\
             webOS-Package-Format-Version: 2\nwebOS-Packager-Version: pley-xtask/1\n",
            self.id, self.version
        )
    }
}

/// Why `id` can't be an app's id, if it can't.
fn id_problem(id: &str) -> Option<String> {
    let valid = !id.is_empty()
        && id.split('.').all(|part| !part.is_empty())
        && id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.' || c == '-');
    if !valid {
        return Some(format!("id \"{id}\" isn't lowercase reverse-DNS"));
    }
    RESERVED_IDS
        .iter()
        .find(|p| id == **p || id.starts_with(&format!("{p}.")))
        .map(|p| format!("id \"{id}\" starts with {p}, reserved for the system's apps"))
}

/// Three integers without leading zeros, at most nine digits each, as webOS takes them.
pub fn is_version(text: &str) -> bool {
    let parts: Vec<&str> = text.split('.').collect();
    parts.len() == 3
        && parts.iter().all(|p| {
            !p.is_empty()
                && p.len() <= 9
                && p.bytes().all(|b| b.is_ascii_digit())
                && (p.len() == 1 || !p.starts_with('0'))
        })
}

/// What opkg checks free space against: the files' sizes, each rounded up to a KiB.
pub fn installed_kib<'a>(sizes: impl IntoIterator<Item = &'a u64>) -> u64 {
    sizes.into_iter().map(|s| s.div_ceil(1024)).sum()
}

/// The .ipk holding `files` (the app's folder and anything else under the install prefix) and
/// the package descriptor made from `info`.
pub fn build(info: &AppInfo, mut files: Vec<File>) -> Result<Vec<u8>> {
    files.push(File::new(info.packageinfo_path(), info.packageinfo().into_bytes(), false));
    let mut seen = BTreeSet::new();
    for file in &files {
        if !seen.insert(file.path.as_str()) {
            return Err(format!("{} is in the package twice", file.path).into());
        }
    }
    let sizes: Vec<u64> = files.iter().map(|f| f.data.len() as u64).collect();
    let data = tar_gz(&files, true)?;
    let control = tar_gz(&[File::new("control", info.control(installed_kib(&sizes)).into_bytes(), false)], false)?;
    Ok(ar(&[(MEMBERS[0], b"2.0\n"), (MEMBERS[1], &control), (MEMBERS[2], &data)]))
}

/// A gzip-compressed tar of the files (and, with `folders`, every folder above them): entries
/// sorted by path, owned by root (0:0), dated [`MTIME`], 0755 for folders and programs and 0644
/// for the rest; gzip at its best level with no name and a zero date.
fn tar_gz(files: &[File], folders: bool) -> Result<Vec<u8>> {
    let mut entries: BTreeMap<String, Option<&File>> = BTreeMap::new();
    for file in files {
        if file.path.starts_with('/') || file.path.split('/').any(|p| p.is_empty() || p == "." || p == "..") {
            return Err(format!("\"{}\" isn't a plain relative path", file.path).into());
        }
        if folders {
            let mut at = 0;
            while let Some(slash) = file.path[at..].find('/') {
                at += slash + 1;
                entries.insert(file.path[..at].to_owned(), None);
            }
        }
        entries.insert(file.path.clone(), Some(file));
    }
    let mut tar = tar::Builder::new(Vec::new());
    for (path, file) in &entries {
        let mut header = tar::Header::new_gnu();
        header.set_uid(0);
        header.set_gid(0);
        header.set_username("root")?;
        header.set_groupname("root")?;
        header.set_mtime(MTIME);
        match file {
            None => {
                header.set_entry_type(tar::EntryType::Directory);
                header.set_mode(0o755);
                header.set_size(0);
                tar.append_data(&mut header, path, std::io::empty())?;
            }
            Some(file) => {
                header.set_entry_type(tar::EntryType::Regular);
                header.set_mode(if file.executable { 0o755 } else { 0o644 });
                header.set_size(file.data.len() as u64);
                tar.append_data(&mut header, path, file.data.as_slice())?;
            }
        }
    }
    let tar = tar.into_inner()?;
    // 3 is Unix in gzip's OS byte: set, so the host that packs doesn't show in the bytes.
    let mut gz = GzBuilder::new().mtime(0).operating_system(3).write(Vec::new(), Compression::best());
    gz.write_all(&tar)?;
    Ok(gz.finish()?)
}

/// A System V `ar` archive of the members, with bare names, dated [`MTIME`], 0:0, mode 0644.
fn ar(members: &[(&str, &[u8])]) -> Vec<u8> {
    let mut out = b"!<arch>\n".to_vec();
    for (name, data) in members {
        let header = format!("{name:<16}{MTIME:<12}{:<6}{:<6}{:<8}{:<10}`\n", 0, 0, "100644", data.len());
        debug_assert_eq!(header.len(), 60);
        out.extend_from_slice(header.as_bytes());
        out.extend_from_slice(data);
        if data.len() % 2 == 1 {
            out.push(b'\n');
        }
    }
    out
}

/// An `ar` archive's members: (name as written, data).
fn ar_members(bytes: &[u8]) -> Result<Vec<(String, &[u8])>> {
    let mut rest = bytes.strip_prefix(b"!<arch>\n").ok_or("not an ar archive")?;
    let mut members = Vec::new();
    while !rest.is_empty() {
        if rest.len() < 60 || &rest[58..60] != b"`\n" {
            return Err("an ar member header is cut short or damaged".into());
        }
        let name = String::from_utf8_lossy(&rest[..16]).trim_end().to_owned();
        let size: usize =
            String::from_utf8_lossy(&rest[48..58]).trim().parse().map_err(|_| "an ar member without a size")?;
        let data = rest.get(60..60 + size).ok_or("an ar member runs past the end")?;
        members.push((name, data));
        rest = &rest[(60 + size + size % 2).min(rest.len())..];
    }
    Ok(members)
}

/// One entry of a tarball read back.
struct Entry {
    path: String,
    folder: bool,
    mode: u32,
    owner: (u64, u64),
    mtime: u64,
    data: Vec<u8>,
}

fn untar_gz(bytes: &[u8]) -> Result<Vec<Entry>> {
    let mut archive = tar::Archive::new(GzDecoder::new(bytes));
    let mut entries = Vec::new();
    for entry in archive.entries()? {
        let mut entry = entry?;
        let header = entry.header();
        let path = entry.path()?.to_string_lossy().replace('\\', "/");
        let path = path.strip_prefix("./").unwrap_or(&path).to_owned();
        let folder = header.entry_type().is_dir();
        let (mode, owner, mtime) = (header.mode()?, (header.uid()?, header.gid()?), header.mtime()?);
        let mut data = Vec::new();
        entry.read_to_end(&mut data)?;
        entries.push(Entry { path, folder, mode, owner, mtime, data });
    }
    Ok(entries)
}

/// What [`verify`] read from the package.
#[derive(Debug)]
pub struct Summary {
    /// The control file's fields, in order.
    pub control: Vec<(String, String)>,
    /// How many files the payload holds.
    pub files: usize,
    /// Their total size in bytes.
    pub payload: u64,
}

/// Reads an .ipk back and checks what the TV's installer and launcher rely on: the `ar`
/// members, the control fields, `packageinfo.json` and `appinfo.json` agreeing on the id and
/// version, the program and the pictures `appinfo.json` names, the licence files, and
/// reproducible entries. Returns what it read and what is wrong.
pub fn verify(bytes: &[u8]) -> Result<(Summary, Vec<String>)> {
    let mut wrong = Vec::new();
    let members = ar_members(bytes)?;
    let names: Vec<&str> = members.iter().map(|(n, _)| n.as_str()).collect();
    if names != MEMBERS {
        wrong.push(format!("ar members {names:?}, not {MEMBERS:?} (bare names, in that order)"));
    }
    let member = |name: &str| members.iter().find(|(n, _)| n.trim_end_matches('/') == name).map(|(_, d)| *d);
    if member("debian-binary") != Some(b"2.0\n") {
        wrong.push("debian-binary isn't \"2.0\\n\"".to_owned());
    }

    let control_entries = untar_gz(member("control.tar.gz").ok_or("no control.tar.gz")?)?;
    let control_file =
        control_entries.iter().find(|e| e.path == "control").ok_or("control.tar.gz has no control file")?;
    let control_text = String::from_utf8_lossy(&control_file.data).into_owned();
    let control: Vec<(String, String)> = control_text
        .lines()
        .filter_map(|l| l.split_once(':').map(|(k, v)| (k.trim().to_owned(), v.trim().to_owned())))
        .collect();
    let field = |key: &str| control.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str());
    for key in ["Package", "Version", "Architecture", "Installed-Size", "Maintainer", "Description"] {
        if field(key).is_none_or(str::is_empty) {
            wrong.push(format!("the control file has no {key}"));
        }
    }
    let id = field("Package").unwrap_or_default().to_owned();
    let version = field("Version").unwrap_or_default().to_owned();
    if field("Architecture").is_some_and(|a| a != "arm") {
        wrong.push("Architecture isn't arm".to_owned());
    }
    if field("webOS-Package-Format-Version") != Some("2") {
        wrong.push("webOS-Package-Format-Version isn't 2".to_owned());
    }
    if let Some(maintainer) = field("Maintainer").filter(|m| !is_maintainer(m)) {
        wrong.push(format!("Maintainer \"{maintainer}\" isn't of the form Name <a@b.tld>"));
    }
    if !is_version(&version) {
        wrong.push(format!("Version \"{version}\" isn't three integers"));
    }
    if let Some(problem) = id_problem(&id) {
        wrong.push(problem);
    }

    let data = untar_gz(member("data.tar.gz").ok_or("no data.tar.gz")?)?;
    let app_dir = format!("usr/palm/applications/{id}/");
    let package_dir = format!("usr/palm/packages/{id}/");
    let mut files = BTreeMap::new();
    for entry in &data {
        if entry.owner != (0, 0) {
            wrong.push(format!("{} isn't owned by 0:0", entry.path));
        }
        if entry.mtime == 0 {
            wrong.push(format!("{} is dated 1970", entry.path));
        }
        let modes: &[u32] = if entry.folder { &[0o755] } else { &[0o755, 0o644] };
        if !modes.contains(&(entry.mode & 0o7777)) {
            wrong.push(format!("{} has mode {:o}", entry.path, entry.mode));
        }
        if !entry.folder {
            let path = entry.path.as_str();
            if !path.starts_with(&app_dir) && !path.starts_with(&package_dir) {
                wrong.push(format!("{path} is outside the app's and the package's folders"));
            }
            if path.ends_with(".so") || path.contains(".so.") {
                wrong.push(format!("{path}: the package ships no libraries"));
            }
            files.insert(path.to_owned(), entry);
        }
    }
    let payload: u64 = files.values().map(|e| e.data.len() as u64).sum();
    let sizes: Vec<u64> = files.values().map(|e| e.data.len() as u64).collect();
    let expected_kib = installed_kib(&sizes);
    if field("Installed-Size").and_then(|s| s.parse::<u64>().ok()) != Some(expected_kib) {
        wrong.push(format!("Installed-Size isn't {expected_kib} (KiB of the payload)"));
    }

    match files.get(&format!("{package_dir}packageinfo.json")) {
        None => wrong.push(format!("no {package_dir}packageinfo.json (the installer needs it)")),
        Some(entry) => match serde_json::from_slice::<Value>(&entry.data) {
            Err(e) => wrong.push(format!("packageinfo.json: {e}")),
            Ok(json) => {
                for key in ["id", "app"] {
                    if json[key] != id.as_str() {
                        wrong.push(format!("packageinfo.json's {key} isn't {id}"));
                    }
                }
                if json["version"] != version.as_str() {
                    wrong.push(format!("packageinfo.json's version isn't {version}"));
                }
                if json["package_format_version"] != 2 {
                    wrong.push("packageinfo.json's package_format_version isn't 2".to_owned());
                }
            }
        },
    }

    match files.get(&format!("{app_dir}appinfo.json")) {
        None => wrong.push(format!("no {app_dir}appinfo.json")),
        Some(entry) => match AppInfo::parse(&entry.data) {
            Err(e) => wrong.push(e.to_string()),
            Ok(info) => {
                if info.id != id {
                    wrong.push(format!("appinfo.json's id {} isn't the package's {id}", info.id));
                }
                if info.version != version {
                    wrong.push(format!("appinfo.json's version {} isn't the package's {version}", info.version));
                }
                match files.get(&format!("{app_dir}{}", info.main)) {
                    None => wrong.push(format!("no {} (appinfo.json's main)", info.main)),
                    Some(main) if main.mode & 0o111 == 0 => wrong.push(format!("{} isn't executable", info.main)),
                    Some(_) => {}
                }
                for (picture, size) in &info.pictures {
                    match files.get(&format!("{app_dir}{picture}")) {
                        None => wrong.push(format!("no {picture}, which appinfo.json names")),
                        Some(entry) => match png_size(&entry.data) {
                            Some(found) if found == *size => {}
                            found => wrong.push(format!("{picture} is {found:?}, not {size:?} pixels")),
                        },
                    }
                }
                for (path, entry) in &files {
                    let main = format!("{app_dir}{}", info.main);
                    if entry.mode & 0o111 != 0 && *path != main {
                        wrong.push(format!("{path} is executable; only {} should be", info.main));
                    }
                }
            }
        },
    }
    for name in ["LICENSE.txt", "LICENSING.md", "NOTICES.txt"] {
        if !files.contains_key(&format!("{app_dir}{name}")) {
            wrong.push(format!("no {name} in the app's folder"));
        }
    }
    Ok((Summary { control, files: files.len(), payload }, wrong))
}

/// `Name <a@b.tld>`.
fn is_maintainer(text: &str) -> bool {
    let Some((name, rest)) = text.split_once(" <") else { return false };
    let Some(address) = rest.strip_suffix('>') else { return false };
    let Some((local, domain)) = address.split_once('@') else { return false };
    !name.trim().is_empty()
        && !local.is_empty()
        && domain.contains('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
        && !address.contains(char::is_whitespace)
}

/// A PNG's width and height, from its header.
fn png_size(data: &[u8]) -> Option<(u32, u32)> {
    if data.len() < 24 || &data[..8] != b"\x89PNG\r\n\x1a\n" || &data[12..16] != b"IHDR" {
        return None;
    }
    Some((u32::from_be_bytes(data[16..20].try_into().ok()?), u32::from_be_bytes(data[20..24].try_into().ok()?)))
}

#[cfg(test)]
mod tests {
    use super::*;

    const APPINFO: &str = r##"{
  "id": "app.pley.tv",
  "version": "1.2.3",
  "vendor": "Pley",
  "type": "native",
  "main": "pley",
  "title": "Pley",
  "icon": "icon.png",
  "largeIcon": "largeIcon.png",
  "splashBackground": "splash.png"
}
"##;

    /// A PNG's signature and header, enough for its size to be read.
    fn png(width: u32, height: u32) -> Vec<u8> {
        let mut data = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
        data.extend_from_slice(&width.to_be_bytes());
        data.extend_from_slice(&height.to_be_bytes());
        data.extend_from_slice(&[8, 2, 0, 0, 0, 0, 0, 0, 0]);
        data
    }

    fn files(info: &AppInfo) -> Vec<File> {
        let dir = info.app_dir();
        vec![
            File::new(format!("{dir}/appinfo.json"), APPINFO.as_bytes().to_vec(), false),
            File::new(format!("{dir}/pley"), b"\x7fELF stand-in".to_vec(), true),
            File::new(format!("{dir}/icon.png"), png(80, 80), false),
            File::new(format!("{dir}/largeIcon.png"), png(130, 130), false),
            File::new(format!("{dir}/splash.png"), png(1920, 1080), false),
            File::new(format!("{dir}/assets/fonts/roboto-latin-400.ttf"), vec![7; 3000], false),
            File::new(
                format!("{dir}/assets/licenses/a-licence-file-with-a-name-long-enough-for-gnu-long-names.txt"),
                b"ISC".to_vec(),
                false,
            ),
            File::new(format!("{dir}/LICENSE.txt"), b"Restricted".to_vec(), false),
            File::new(format!("{dir}/LICENSING.md"), b"# Licensing".to_vec(), false),
            File::new(format!("{dir}/NOTICES.txt"), b"Notices".to_vec(), false),
        ]
    }

    fn package() -> (AppInfo, Vec<u8>) {
        let info = AppInfo::parse(APPINFO.as_bytes()).unwrap();
        let ipk = build(&info, files(&info)).unwrap();
        (info, ipk)
    }

    #[test]
    fn reads_and_checks_appinfo() {
        let info = AppInfo::parse(APPINFO.as_bytes()).unwrap();
        assert_eq!((info.id.as_str(), info.version.as_str(), info.main.as_str()), ("app.pley.tv", "1.2.3", "pley"));
        assert_eq!(info.pictures.len(), 3);
        assert_eq!(info.file_name(), "app.pley.tv_1.2.3_arm.ipk");
        let web = APPINFO.replace("\"native\"", "\"web\"");
        assert!(AppInfo::parse(web.as_bytes()).is_err());
        let reserved = APPINFO.replace("app.pley.tv", "com.webos.pley");
        assert!(AppInfo::parse(reserved.as_bytes()).is_err());
        let suffix = APPINFO.replace("1.2.3", "1.2.3-rc.1");
        assert!(AppInfo::parse(suffix.as_bytes()).is_err());
    }

    #[test]
    fn versions_are_three_plain_integers() {
        assert!(is_version("0.1.0"));
        assert!(is_version("10.20.30"));
        assert!(!is_version("1.2"));
        assert!(!is_version("1.02.3"));
        assert!(!is_version("1.2.3.4"));
        assert!(!is_version("1.2.1234567890"));
        assert!(!is_version("1.2.x"));
    }

    #[test]
    fn the_package_is_reproducible() {
        let (_, first) = package();
        let (_, second) = package();
        assert_eq!(first, second);
    }

    #[test]
    fn ar_members_are_bare_and_in_order() {
        let (_, ipk) = package();
        assert!(ipk.starts_with(b"!<arch>\ndebian-binary   1577836800  0     0     100644  4         `\n2.0\n"));
        let members = ar_members(&ipk).unwrap();
        let names: Vec<&str> = members.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, MEMBERS);
        assert!(ipk.windows(16).any(|w| w == b"control.tar.gz  "));
        assert!(!ipk.windows(15).any(|w| w == b"control.tar.gz/"));
    }

    #[test]
    fn gzip_headers_carry_no_date_or_name() {
        let (_, ipk) = package();
        for (name, data) in ar_members(&ipk).unwrap().into_iter().skip(1) {
            // Magic, deflate, no flags (so no file name), a zero date, best compression, Unix.
            assert_eq!(&data[..10], b"\x1f\x8b\x08\x00\x00\x00\x00\x00\x02\x03", "{name}");
        }
    }

    #[test]
    fn tar_entries_are_sorted_owned_by_root_and_dated() {
        let (info, ipk) = package();
        let members = ar_members(&ipk).unwrap();
        let entries = untar_gz(members[2].1).unwrap();
        let paths: Vec<&str> = entries.iter().map(|e| e.path.as_str()).collect();
        let mut sorted = paths.clone();
        sorted.sort();
        assert_eq!(paths, sorted);
        assert_eq!(paths[0], "usr/");
        assert!(paths.contains(&"usr/palm/packages/app.pley.tv/"));
        assert!(paths.contains(&"usr/palm/packages/app.pley.tv/packageinfo.json"));
        assert!(paths.iter().any(|p| p.ends_with("a-licence-file-with-a-name-long-enough-for-gnu-long-names.txt")));
        for entry in &entries {
            assert_eq!(entry.owner, (0, 0));
            assert_eq!(entry.mtime, MTIME);
            let program = entry.path == format!("{}/pley", info.app_dir());
            let mode = if entry.folder || program { 0o755 } else { 0o644 };
            assert_eq!(entry.mode, mode, "{}", entry.path);
        }
        let control = untar_gz(members[1].1).unwrap();
        assert_eq!(control.len(), 1);
        assert_eq!(control[0].path, "control");
    }

    #[test]
    fn control_and_packageinfo_say_what_the_tv_reads() {
        let info = AppInfo::parse(APPINFO.as_bytes()).unwrap();
        let control = info.control(42);
        assert!(control.starts_with("Package: app.pley.tv\nVersion: 1.2.3\n"));
        assert!(control.contains("\nArchitecture: arm\nInstalled-Size: 42\n"));
        assert!(control.contains("\nwebOS-Package-Format-Version: 2\n"));
        assert!(is_maintainer(MAINTAINER));
        assert_eq!(
            info.packageinfo(),
            "{\n    \"app\": \"app.pley.tv\",\n    \"id\": \"app.pley.tv\",\n    \"loc_name\": \"Pley\",\n    \
             \"package_format_version\": 2,\n    \"vendor\": \"Pley\",\n    \"version\": \"1.2.3\"\n}\n"
        );
        // Each file rounds up on its own: 0, 1, 1 and 2 KiB.
        assert_eq!(installed_kib(&[0, 1, 1024, 1025]), 4);
    }

    #[test]
    fn verify_accepts_what_build_makes() {
        let (_, ipk) = package();
        let (summary, wrong) = verify(&ipk).unwrap();
        assert_eq!(wrong, Vec::<String>::new());
        assert_eq!(summary.files, 11);
        assert!(summary.control.contains(&("Package".to_owned(), "app.pley.tv".to_owned())));
    }

    #[test]
    fn verify_finds_gnu_ar_names_and_disagreements() {
        let (info, ipk) = package();
        let mut gnu = ipk.clone();
        let at = gnu.windows(16).position(|w| w == b"data.tar.gz     ").unwrap();
        gnu[at + 11] = b'/';
        let (_, wrong) = verify(&gnu).unwrap();
        assert!(wrong[0].starts_with("ar members"), "{wrong:?}");

        let mut other = files(&info);
        other[0].data = APPINFO.replace("1.2.3", "1.2.4").into_bytes();
        other[2].data = png(64, 64);
        other[5].executable = true;
        other.retain(|f| !f.path.ends_with("NOTICES.txt"));
        let (_, wrong) = verify(&build(&info, other).unwrap()).unwrap();
        assert!(wrong.contains(&"appinfo.json's version 1.2.4 isn't the package's 1.2.3".to_owned()), "{wrong:?}");
        assert!(wrong.contains(&"icon.png is Some((64, 64)), not (80, 80) pixels".to_owned()), "{wrong:?}");
        assert!(wrong.iter().any(|w| w.contains("roboto-latin-400.ttf is executable")), "{wrong:?}");
        assert!(wrong.contains(&"no NOTICES.txt in the app's folder".to_owned()), "{wrong:?}");
    }

    #[test]
    fn verify_wants_packageinfo_and_the_program() {
        let info = AppInfo::parse(APPINFO.as_bytes()).unwrap();
        let mut without_main = files(&info);
        without_main.retain(|f| !f.path.ends_with("/pley"));
        let ipk = build(&info, without_main).unwrap();
        let (_, wrong) = verify(&ipk).unwrap();
        assert!(wrong.contains(&"no pley (appinfo.json's main)".to_owned()), "{wrong:?}");
        assert!(verify(b"not an archive").is_err());
        assert!(is_maintainer("Name <a@b.tld>"));
        assert!(!is_maintainer("Name a@b.tld"));
        assert!(!is_maintainer("<a@b.tld>"));
        assert!(!is_maintainer("Name <a@localhost>"));
    }

    #[test]
    fn rejects_duplicate_and_escaping_paths() {
        let info = AppInfo::parse(APPINFO.as_bytes()).unwrap();
        let mut twice = files(&info);
        twice.push(twice[0].clone());
        assert!(build(&info, twice).is_err());
        let escaping = vec![File::new("usr/../etc/passwd", Vec::new(), false)];
        assert!(build(&info, escaping).is_err());
    }
}
