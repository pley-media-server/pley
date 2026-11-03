//! Fonts: the faces shipped with the app, the system's faces in the folders it
//! is given, and which face draws each character.
//!
//! Font files are memory-mapped once and stay mapped for the life of the
//! process, so parsed faces borrow their bytes without copies. The system's
//! folders are walked only the first time something needs them (a family
//! asked for by name, the monospace face, or a character the app's faces
//! lack), and then only each face's name, OS/2 and post tables are read; a
//! face is parsed further only when it draws something.

use std::collections::HashMap;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};

use memmap2::Mmap;
use pley_base::{Family, Weight};
use rustybuzz::ttf_parser::{self, PlatformId, Tag, name_id};
use rustybuzz::{Direction, Feature, Script, ShapePlan};

use crate::families::{self, Generic};

/// Set in [`pley_base::Glyph::font`] when a face without an italic draws an
/// italic style: the rasteriser slants the glyph.
pub const SYNTHETIC_ITALIC: u16 = 0x8000;

/// The most faces a set holds: indices leave [`SYNTHETIC_ITALIC`]'s bit free.
const MAX_FACES: usize = SYNTHETIC_ITALIC as usize;
/// How deep below a font folder faces are looked for (symbolic links can loop).
const MAX_DEPTH: usize = 6;
/// The most faces read from one collection file.
const MAX_COLLECTION: u32 = 64;
/// Characters remembered with the face fallback chose before the memo starts
/// over.
const MAX_FALLBACK_MEMO: usize = 8192;

/// A run's shaping features: optional ligatures off (letter-spaced text, as
/// Android does), tabular digits on (Roboto standing in for a monospace face).
pub(crate) const NO_LIGATURES: u8 = 1;
pub(crate) const TABULAR: u8 = 2;

/// What a face is, as its name, OS/2 and post tables describe it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FaceInfo {
    /// The family name people know it by: the typographic family when the
    /// font names one, in English when it has English names.
    pub family: String,
    /// Its weight, 1 to 1000 (400 regular, 700 bold).
    pub weight: u16,
    pub italic: bool,
    pub monospace: bool,
    /// The file it is in.
    pub path: PathBuf,
    /// Its index in that file (non-zero only in collections).
    pub index: u32,
}

/// A face known by its file, parsed when first used.
struct Slot {
    info: FaceInfo,
    /// Every family name the font gives, in lower case, in every language.
    names: Vec<String>,
    /// Whether it has outlines to draw (bitmap-only emoji faces have none).
    outlines: bool,
    face: OnceLock<Option<Arc<Face>>>,
}

impl Slot {
    fn face(&self) -> Option<Arc<Face>> {
        self.face.get_or_init(|| Face::load(&self.info).map(Arc::new)).clone()
    }
}

/// A face in use: parsed, with the metrics layout needs. Its shaping tables
/// are read the first time it shapes, and its shaping plans are kept.
pub(crate) struct Face {
    pub(crate) ttf: ttf_parser::Face<'static>,
    shaper: OnceLock<rustybuzz::Face<'static>>,
    plans: Mutex<Vec<(PlanKey, Arc<ShapePlan>)>>,
    /// Font units per em; the metrics below are in font units.
    pub(crate) upem: f32,
    /// Above the baseline and below it (positive), as Android's text engine
    /// measures a line: `hhea`, or OS/2's typographic values when the font
    /// asks for them.
    pub(crate) ascent: f32,
    pub(crate) descent: f32,
    pub(crate) cap_height: f32,
    pub(crate) x_height: f32,
}

type PlanKey = (Direction, Script, u8);

impl Face {
    fn load(info: &FaceInfo) -> Option<Face> {
        let data = map_file(&info.path)?;
        let ttf = ttf_parser::Face::parse(data, info.index).ok()?;
        Some(Face::new(ttf))
    }

    fn new(ttf: ttf_parser::Face<'static>) -> Face {
        let upem = f32::from(ttf.units_per_em().max(1));
        let (mut ascent, mut descent) = (f32::from(ttf.ascender()), -f32::from(ttf.descender()));
        if ascent + descent <= 0.0 {
            // A face without line metrics gets Roboto's proportions.
            ascent = upem * 0.928;
            descent = upem * 0.244;
        }
        let ink_top = |c| ttf.glyph_index(c).and_then(|g| ttf.glyph_bounding_box(g)).map(|b| f32::from(b.y_max));
        let cap_height =
            ttf.capital_height().filter(|&h| h > 0).map(f32::from).or_else(|| ink_top('H')).unwrap_or(ascent * 0.75);
        let x_height =
            ttf.x_height().filter(|&h| h > 0).map(f32::from).or_else(|| ink_top('x')).unwrap_or(cap_height * 0.7);
        Face {
            ttf,
            shaper: OnceLock::new(),
            plans: Mutex::new(Vec::new()),
            upem,
            ascent,
            descent,
            cap_height,
            x_height,
        }
    }

    /// Whether the face has a glyph for `c`.
    pub(crate) fn covers(&self, c: char) -> bool {
        self.ttf.glyph_index(c).is_some()
    }

    pub(crate) fn shaper(&self) -> &rustybuzz::Face<'static> {
        self.shaper.get_or_init(|| rustybuzz::Face::from_face(self.ttf.clone()))
    }

    /// The shaping plan for a direction, script and set of features; plans
    /// are made once (making one costs more than shaping a title).
    pub(crate) fn plan(&self, direction: Direction, script: Script, features: u8) -> Arc<ShapePlan> {
        let key = (direction, script, features);
        let mut plans = lock(&self.plans);
        if let Some((_, plan)) = plans.iter().find(|(k, _)| *k == key) {
            return plan.clone();
        }
        let plan = Arc::new(ShapePlan::new(self.shaper(), direction, Some(script), None, &feature_list(features)));
        plans.push((key, plan.clone()));
        plan
    }
}

fn feature_list(features: u8) -> Vec<Feature> {
    let mut list = Vec::new();
    if features & NO_LIGATURES != 0 {
        list.push(Feature::new(Tag::from_bytes(b"liga"), 0, ..));
        list.push(Feature::new(Tag::from_bytes(b"clig"), 0, ..));
    }
    if features & TABULAR != 0 {
        list.push(Feature::new(Tag::from_bytes(b"tnum"), 1, ..));
    }
    list
}

/// The faces a style's text is drawn with, tried in order for each character;
/// fallback by coverage comes after them.
pub(crate) struct Chain {
    pub(crate) faces: Vec<ChainFace>,
    pub(crate) weight: u16,
    pub(crate) italic: bool,
}

/// A face of a chain, as runs use it.
#[derive(Clone)]
pub(crate) struct ChainFace {
    /// The face's index, with [`SYNTHETIC_ITALIC`] when it is drawn slanted.
    pub(crate) id: u16,
    pub(crate) face: Arc<Face>,
    /// Shaping features this face always uses ([`TABULAR`]).
    pub(crate) features: u8,
}

/// Faces by every family name they give, in lower case.
type Families = HashMap<String, Vec<u16>>;

/// A family as fallback tries it: its lower-case name and its faces.
type Group = (String, Vec<u16>);

/// The system's faces, indexed the first time they are needed.
struct SystemIndex {
    slots: Vec<Slot>,
    families: Families,
    /// Families in the order fallback tries them, each its faces with outlines,
    /// the regular upright one first.
    groups: Vec<Vec<u16>>,
}

/// The faces text is drawn with: the app's own (from the folder it ships in)
/// and the system's (from the folders given), loaded as they are needed.
pub struct FontSet {
    bundled: Vec<Slot>,
    bundled_families: Families,
    /// The app's families in the order fallback tries them (their files' order).
    bundled_groups: Vec<Vec<u16>>,
    dirs: Vec<PathBuf>,
    system: OnceLock<SystemIndex>,
    /// Which face fallback chose for a character, weight and slant.
    memo: Mutex<HashMap<(char, u16, bool), Option<u16>>>,
}

impl FontSet {
    /// The app's faces from `bundled_dir` (its `assets/fonts`: Roboto 400, 500
    /// and 700 in Latin and Latin Extended files) and the system's from
    /// `fallback_dirs` and their subfolders (the TV's `/usr/share/fonts`; see
    /// [`system_font_dirs`]). Only the app's own files are opened here.
    pub fn load(bundled_dir: &Path, fallback_dirs: &[PathBuf]) -> FontSet {
        let mut files: Vec<PathBuf> = fs::read_dir(bundled_dir)
            .into_iter()
            .flatten()
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.is_file() && is_font_file(path))
            .collect();
        // File names order a family's subsets: "roboto-latin-400" before "roboto-latin-ext-400".
        files.sort();
        let mut bundled = Vec::new();
        for path in &files {
            if let Some(data) = map_file(path) {
                read_slots(path, data, &mut bundled);
            }
        }
        bundled.truncate(MAX_FACES);
        let (bundled_families, bundled_groups) = index_families(&bundled, 0);
        FontSet {
            bundled,
            bundled_families,
            bundled_groups: bundled_groups.into_iter().map(|(_, ids)| ids).collect(),
            dirs: fallback_dirs.to_vec(),
            system: OnceLock::new(),
            memo: Mutex::new(HashMap::new()),
        }
    }

    /// What the face behind a glyph's `font` is (the synthetic-italic flag is
    /// ignored); `None` for an index this set never gave out.
    pub fn info(&self, font: u16) -> Option<&FaceInfo> {
        self.slot(font).map(|slot| &slot.info)
    }

    /// Whether a family asked for by name is here: the app's or the system's,
    /// or, for a generic name ("serif", "monospace", "casual"), one of the
    /// families that stand for it. Settings offers "Casual" only when it is.
    pub fn has_family(&self, name: &str) -> bool {
        let key = name.trim().to_lowercase();
        match families::generic(&key) {
            Some(Generic::Sans) => self.sans().is_some(),
            Some(Generic::Mono) => self.first_family(families::MONO).is_some(),
            Some(Generic::List(list)) => self.first_family(list).is_some(),
            None => self.family(&key).is_some(),
        }
    }

    /// Reads the system folders' names now rather than when a string first
    /// needs them; for a worker thread at start-up, so the first title in
    /// another script doesn't hold up a frame.
    pub fn index_system_fonts(&self) {
        self.system();
    }

    fn system(&self) -> &SystemIndex {
        self.system.get_or_init(|| SystemIndex::build(&self.dirs, self.bundled.len()))
    }

    fn slot(&self, font: u16) -> Option<&Slot> {
        let index = usize::from(font & !SYNTHETIC_ITALIC);
        match index.checked_sub(self.bundled.len()) {
            None => self.bundled.get(index),
            Some(i) => self.system.get()?.slots.get(i),
        }
    }

    /// The face behind a glyph's `font`, parsed on first use.
    pub(crate) fn face(&self, font: u16) -> Option<Arc<Face>> {
        self.slot(font)?.face()
    }

    /// A family's faces by lower-case name: the app's, else the system's.
    fn family(&self, key: &str) -> Option<&[u16]> {
        if let Some(ids) = self.bundled_families.get(key) {
            return Some(ids);
        }
        self.system().families.get(key).map(Vec::as_slice)
    }

    fn first_family(&self, names: &[&str]) -> Option<&[u16]> {
        names.iter().find_map(|name| self.family(name))
    }

    /// The app's own face: the bundled Roboto, else whatever the app's folder
    /// holds, else the system's nearest equivalent.
    fn sans(&self) -> Option<&[u16]> {
        self.bundled_families
            .get("roboto")
            .map(Vec::as_slice)
            .or_else(|| self.bundled_groups.first().map(Vec::as_slice))
            .or_else(|| self.first_family(families::SANS))
    }

    /// The chain for a family, weight and slant: the family's best faces, then
    /// Roboto's (which a family asked for by name falls back to, as
    /// `pley_base::Family::Named` says).
    pub(crate) fn chain(&self, family: &Family, weight: Weight, italic: bool) -> Chain {
        let mut chain = Chain { faces: Vec::new(), weight: weight.value(), italic };
        let generic = match family {
            Family::Sans => Some(Generic::Sans),
            Family::Mono => Some(Generic::Mono),
            Family::Named(name) => {
                let key = name.trim().to_lowercase();
                match families::generic(&key) {
                    Some(generic) => Some(generic),
                    None => {
                        if let Some(ids) = self.family(&key) {
                            self.push_best(&mut chain, ids, 0);
                        }
                        None
                    }
                }
            }
        };
        let mut sans_features = 0;
        match generic {
            Some(Generic::Mono) => match self.first_family(families::MONO) {
                Some(ids) => self.push_best(&mut chain, ids, 0),
                None => sans_features = TABULAR,
            },
            Some(Generic::List(list)) => {
                if let Some(ids) = self.first_family(list) {
                    self.push_best(&mut chain, ids, 0);
                }
            }
            Some(Generic::Sans) | None => {}
        }
        if let Some(ids) = self.sans() {
            self.push_best(&mut chain, ids, sans_features);
        }
        chain
    }

    /// Adds the faces of a family that best suit the chain's weight and slant:
    /// every one of them when the family is split into files by script.
    fn push_best(&self, chain: &mut Chain, ids: &[u16], features: u8) {
        let rank = |id: u16| {
            self.slot(id).map(|s| families::match_rank(chain.weight, chain.italic, s.info.weight, s.info.italic))
        };
        let Some(best) = ids.iter().filter_map(|&id| rank(id)).min() else { return };
        for &id in ids {
            if rank(id) != Some(best) || chain.faces.iter().any(|f| f.id & !SYNTHETIC_ITALIC == id) {
                continue;
            }
            let Some(slot) = self.slot(id) else { continue };
            let Some(face) = slot.face() else { continue };
            let id = if chain.italic && !slot.info.italic { id | SYNTHETIC_ITALIC } else { id };
            chain.faces.push(ChainFace { id, face, features });
        }
    }

    /// The face that draws `c` when no face of a style has it: the first family
    /// that covers it (the app's own, then the system's in fallback order), in
    /// the weight and slant nearest the style's.
    pub(crate) fn fallback(&self, c: char, weight: u16, italic: bool) -> Option<ChainFace> {
        let key = (c, weight, italic);
        let known = lock(&self.memo).get(&key).copied();
        let id = match known {
            Some(id) => id,
            None => {
                // Searched outside the lock: the first search may index the system's folders.
                let found = self.find_fallback(c, weight, italic);
                let mut memo = lock(&self.memo);
                if memo.len() >= MAX_FALLBACK_MEMO {
                    memo.clear();
                }
                memo.insert(key, found);
                found
            }
        }?;
        Some(ChainFace { id, face: self.face(id)?, features: 0 })
    }

    fn find_fallback(&self, c: char, weight: u16, italic: bool) -> Option<u16> {
        if let Some(id) = self.bundled_groups.iter().find_map(|group| self.member_for(group, c, weight, italic)) {
            return Some(id);
        }
        self.system().groups.iter().find_map(|group| self.member_for(group, c, weight, italic))
    }

    /// The face of a family for `c`: none when the family's regular faces lack
    /// it (usually one parse rules a family out; a family split into files by
    /// script has a regular face per file), else its best face that has it.
    fn member_for(&self, group: &[u16], c: char, weight: u16, italic: bool) -> Option<u16> {
        let covers = |id: u16| self.face(id).is_some_and(|face| face.covers(c));
        let rank = |id: u16, weight: u16, italic: bool| {
            let info = &self.slot(id)?.info;
            Some(families::match_rank(weight, italic, info.weight, info.italic))
        };
        // Groups list their regular upright faces first.
        let regular = rank(*group.first()?, 400, false);
        if !group.iter().take_while(|&&id| rank(id, 400, false) == regular).any(|&id| covers(id)) {
            return None;
        }
        let mut members: Vec<(u32, u16)> =
            group.iter().filter_map(|&id| Some((rank(id, weight, italic)?, id))).collect();
        members.sort_unstable();
        let id = members.into_iter().map(|(_, id)| id).find(|&id| covers(id))?;
        let slanted = italic && !self.slot(id)?.info.italic;
        Some(if slanted { id | SYNTHETIC_ITALIC } else { id })
    }
}

impl SystemIndex {
    fn build(dirs: &[PathBuf], first: usize) -> SystemIndex {
        let mut slots = Vec::new();
        for path in font_files(dirs) {
            if first + slots.len() >= MAX_FACES {
                break;
            }
            // Mapped only to read the names; a face that draws maps its file again, for good.
            if let Some(map) = map(&path) {
                read_slots(&path, &map, &mut slots);
            }
        }
        slots.truncate(MAX_FACES.saturating_sub(first));
        let (families, mut groups) = index_families(&slots, first);
        groups.sort_by_cached_key(|(family, ids)| {
            let lead = &slots[usize::from(ids[0]) - first].info;
            (families::fallback_rank(family), lead.path.clone(), lead.index)
        });
        SystemIndex { slots, families, groups: groups.into_iter().map(|(_, ids)| ids).collect() }
    }
}

/// Faces by every family name they give, and the families with outlines as
/// fallback groups (their faces with the regular upright one first), in the
/// order they first appear.
fn index_families(slots: &[Slot], first: usize) -> (Families, Vec<Group>) {
    let mut names = Families::new();
    let mut groups: Vec<Group> = Vec::new();
    let mut group_of: HashMap<String, usize> = HashMap::new();
    for (i, slot) in slots.iter().enumerate() {
        let id = (first + i) as u16;
        for name in &slot.names {
            names.entry(name.clone()).or_default().push(id);
        }
        if !slot.outlines {
            continue;
        }
        let family = slot.info.family.to_lowercase();
        let group = *group_of.entry(family.clone()).or_insert_with(|| {
            groups.push((family, Vec::new()));
            groups.len() - 1
        });
        groups[group].1.push(id);
    }
    for (_, ids) in &mut groups {
        ids.sort_by_key(|&id| {
            let info = &slots[usize::from(id) - first].info;
            families::match_rank(400, false, info.weight, info.italic)
        });
    }
    (names, groups)
}

/// Adds the faces of a font file (every face of a collection) to `out`.
fn read_slots(path: &Path, data: &[u8], out: &mut Vec<Slot>) {
    let count = ttf_parser::fonts_in_collection(data).unwrap_or(1).min(MAX_COLLECTION);
    out.extend((0..count).filter_map(|index| read_slot(data, index, path)));
}

/// A face's names, weight, slant and kind, from its name, OS/2 and post tables
/// alone.
fn read_slot(data: &[u8], index: u32, path: &Path) -> Option<Slot> {
    let raw = ttf_parser::RawFace::parse(data, index).ok()?;
    let table = |tag: &[u8; 4]| raw.table(Tag::from_bytes(tag));
    let os2 = table(b"OS/2").and_then(ttf_parser::os2::Table::parse);
    let weight = os2.map(|t| t.weight().to_number()).filter(|w| (1..=1000).contains(w)).unwrap_or(400);
    let italic = os2.is_some_and(|t| t.style() != ttf_parser::Style::Normal);
    let monospace = table(b"post").and_then(ttf_parser::post::Table::parse).is_some_and(|post| post.is_monospaced);
    let outlines = [b"glyf", b"CFF ", b"CFF2"].iter().any(|tag| table(tag).is_some());

    let name = ttf_parser::name::Table::parse(table(b"name")?)?;
    let mut names: Vec<String> = Vec::new();
    let mut family: Option<(u8, String)> = None;
    let mut typographic = false;
    for entry in name.names {
        if entry.name_id != name_id::TYPOGRAPHIC_FAMILY && entry.name_id != name_id::FAMILY {
            continue;
        }
        let Some(text) = entry.to_string() else { continue };
        let text = text.trim();
        if text.is_empty() {
            continue;
        }
        let lower = text.to_lowercase();
        if !names.contains(&lower) {
            names.push(lower);
        }
        // The typographic family groups weights the legacy one splits ("Segoe UI
        // Semibold" is "Segoe UI" at 600); English names are the ones people know.
        typographic |= entry.name_id == name_id::TYPOGRAPHIC_FAMILY;
        let english =
            entry.platform_id == PlatformId::Unicode || entry.language() == ttf_parser::Language::English_UnitedStates;
        let score = u8::from(entry.name_id != name_id::TYPOGRAPHIC_FAMILY) * 2 + u8::from(!english);
        if family.as_ref().is_none_or(|(best, _)| score < *best) {
            family = Some((score, text.to_string()));
        }
    }
    let (_, mut family) = family?;
    if !typographic && let Some(base) = families::base_family(&family, weight) {
        // Without a typographic family, a weight named as a family of its own
        // ("Roboto Medium") still belongs to its family.
        let base = base.to_string();
        let lower = base.to_lowercase();
        if !names.contains(&lower) {
            names.push(lower);
        }
        family = base;
    }
    Some(Slot {
        info: FaceInfo { family, weight, italic, monospace, path: path.to_path_buf(), index },
        names,
        outlines,
        face: OnceLock::new(),
    })
}

/// The system's font folders on the machine this runs on: the TV's
/// `/usr/share/fonts` (with Linux's other folders), Windows' and macOS's for
/// the simulator. Folders that don't exist are left out.
pub fn system_font_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    let home = std::env::var_os("HOME").map(PathBuf::from);
    if cfg!(windows) {
        let windows = std::env::var_os("WINDIR").map_or_else(|| PathBuf::from(r"C:\Windows"), PathBuf::from);
        dirs.push(windows.join("Fonts"));
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            dirs.push(PathBuf::from(local).join(r"Microsoft\Windows\Fonts"));
        }
    } else if cfg!(target_os = "macos") {
        dirs.extend(["/System/Library/Fonts", "/Library/Fonts"].map(PathBuf::from));
        dirs.extend(home.map(|home| home.join("Library/Fonts")));
    } else {
        dirs.extend(["/usr/share/fonts", "/usr/local/share/fonts"].map(PathBuf::from));
        if let Some(home) = home {
            dirs.push(home.join(".local/share/fonts"));
            dirs.push(home.join(".fonts"));
        }
    }
    dirs.retain(|dir| dir.is_dir());
    dirs
}

fn is_font_file(path: &Path) -> bool {
    let extension = path.extension().and_then(|e| e.to_str()).unwrap_or_default();
    ["ttf", "otf", "ttc", "otc"].iter().any(|known| extension.eq_ignore_ascii_case(known))
}

/// Every font file in the folders and their subfolders, in path order.
fn font_files(dirs: &[PathBuf]) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for dir in dirs {
        walk(dir, 0, &mut files);
    }
    files.sort();
    files.dedup();
    files
}

fn walk(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(mut kind) = entry.file_type() else { continue };
        if kind.is_symlink() {
            // Font folders link faces between them; follow the link.
            let Ok(target) = fs::metadata(&path) else { continue };
            kind = target.file_type();
        }
        if kind.is_dir() {
            if depth < MAX_DEPTH {
                walk(&path, depth + 1, out);
            }
        } else if kind.is_file() && is_font_file(&path) {
            out.push(path);
        }
    }
}

/// A font file's bytes, mapped once and kept for the life of the process:
/// faces borrow them, and the files a run of the app draws with are few.
fn map_file(path: &Path) -> Option<&'static [u8]> {
    type Mapped = HashMap<PathBuf, Option<&'static [u8]>>;
    static FILES: OnceLock<Mutex<Mapped>> = OnceLock::new();
    let mut files = lock(FILES.get_or_init(Mutex::default));
    if let Some(&bytes) = files.get(path) {
        return bytes;
    }
    let bytes = map(path).map(|map| -> &'static [u8] { Box::leak(Box::new(map)) });
    files.insert(path.to_path_buf(), bytes);
    bytes
}

fn map(path: &Path) -> Option<Mmap> {
    let file = File::open(path).ok()?;
    // SAFETY: the mapping is only read, and font files are the app's own or
    // the system's, which nothing rewrites while the app runs.
    let map = unsafe { Mmap::map(&file) }.ok()?;
    (map.len() >= 12).then_some(map)
}

pub(crate) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}
