//! What Settings shows, as data: each category's rows (`TvSettingsScreen.kt`'s `pane`, the TV
//! app's `settings.tsx`), the choices of each side panel, and what a row does. The screen draws
//! these; the tests read them.

use pley_base::Icon;
use pley_core::model::{QUALITY_PRESETS, SubtitleMode, TrackDefaults, quality_preset};
use pley_core::requests::tmdb_notice;
use pley_core::store::{
    PassthroughChoice, PassthroughFormat, Settings, SubtitleBackground, SubtitleFont, SubtitlePosition, SubtitleSize,
};
use pley_i18n::{Language, t};
use pley_shell::SettingsSection;

/// The languages a profile's defaults offer (ISO 639-2/B, `MeViewModel.kt:213-225`). The panel
/// names them in the language shown and lists them by those names ([`languages`]).
pub const LANGUAGES: [&str; 34] = [
    "ara", "bul", "cat", "chi", "cze", "dan", "dut", "eng", "fin", "fre", "ger", "gre", "heb", "hin", "hrv", "hun",
    "ind", "ita", "jpn", "kor", "nor", "per", "pol", "por", "rum", "rus", "slo", "spa", "srp", "swe", "tha", "tur",
    "ukr", "vie",
];

/// The subtitle modes a profile can have, in the panel's order.
pub const SUBTITLE_MODES: [SubtitleMode; 4] =
    [SubtitleMode::Off, SubtitleMode::Forced, SubtitleMode::Foreign, SubtitleMode::Always];

/// The passthrough switches LG offers (the TV app's Audio category): the TV's own pipeline always
/// passes Dolby Digital, Dolby Digital Plus and Atmos through, so only these two are choices.
pub const PASSTHROUGH_SWITCHES: [PassthroughFormat; 2] = [PassthroughFormat::TrueHd, PassthroughFormat::Dts];

/// A passthrough switch's label and the line under it.
pub fn passthrough_words(format: PassthroughFormat) -> (&'static str, &'static str) {
    match format {
        PassthroughFormat::TrueHd => (t!("settings.audio.trueHd"), t!("settings.audio.trueHdHint")),
        PassthroughFormat::Dts => (t!("settings.audio.dts"), t!("settings.audio.dtsHint")),
        // Never a switch here: the TV passes the others through itself.
        other => (other.label(), ""),
    }
}

/// A category's label and icon (`TvSettingsScreen.kt:83-92`; Lucide's `activity` is drawn as
/// `gauge`, as the TV app does).
pub fn category_look(c: SettingsSection) -> (&'static str, Icon) {
    let icon = match c {
        SettingsSection::Profile => Icon::UserRound,
        SettingsSection::Playback => Icon::Play,
        SettingsSection::Audio => Icon::AudioLines,
        SettingsSection::SubtitlesAndDisplay => Icon::Captions,
        SettingsSection::Server => Icon::Server,
        SettingsSection::Diagnostics => Icon::Activity,
        SettingsSection::About => Icon::Info,
        SettingsSection::SignOut => Icon::LogOut,
    };
    (c.label(), icon)
}

/// A language code's name in the language shown: the `languages` catalog's, else the one
/// `pley-core` gives, else the code itself.
pub fn language_label(code: &str) -> String {
    pley_i18n::lookup(&format!("languages.{code}"))
        .map(str::to_string)
        .or_else(|| pley_core::format::language_name(Some(code)))
        .unwrap_or_else(|| code.to_string())
}

/// The panel's languages, code and name, by name in the language shown (Android's
/// `TrackLanguages`).
pub fn languages() -> Vec<(&'static str, String)> {
    by_name(LANGUAGES.iter().map(|code| (*code, language_label(code))).collect())
}

/// `list` in the order of its names, an accent sorting with its letter: Spanish "Árabe" with the
/// A's, not after "Vietnamita".
fn by_name(mut list: Vec<(&'static str, String)>) -> Vec<(&'static str, String)> {
    list.sort_by_cached_key(|(_, name)| {
        let key: String = name.chars().map(base_letter).flat_map(char::to_lowercase).collect();
        (key, name.clone())
    });
    list
}

/// A letter without its accent, as names sort.
fn base_letter(c: char) -> char {
    match c {
        'À'..='Å' | 'à'..='å' => 'a',
        'Ç' | 'ç' => 'c',
        'È'..='Ë' | 'è'..='ë' => 'e',
        'Ì'..='Ï' | 'ì'..='ï' => 'i',
        'Ñ' | 'ñ' => 'n',
        'Ò'..='Ö' | 'ò'..='ö' => 'o',
        'Ù'..='Ü' | 'ù'..='ü' => 'u',
        'Ý' | 'ý' | 'ÿ' => 'y',
        _ => c,
    }
}

/// The commit the app was built from, when the build says (`PLEY_COMMIT`, as the TV app's
/// `__PLEY_COMMIT__`).
pub const COMMIT: Option<&str> = option_env!("PLEY_COMMIT");

/// "0.2.0 (1a2b3c4d5)": the app's version and the first 9 characters of its commit, when known
/// (Android's `versionOf`, the TV app's `buildLabel`).
pub fn build_label(version: &str, commit: Option<&str>) -> String {
    match commit.map(str::trim).filter(|c| !c.is_empty() && *c != "unknown") {
        Some(c) => format!("{version} ({})", c.get(..9).unwrap_or(c)),
        None => version.to_string(),
    }
}

/// A subtitle mode's label (`strings_tv_account.xml`); a mode this app doesn't know is the server's
/// default, forced.
pub fn mode_label(mode: SubtitleMode) -> &'static str {
    match mode {
        SubtitleMode::Off => t!("settings.subtitleModes.off"),
        SubtitleMode::Foreign => t!("settings.subtitleModes.foreign"),
        SubtitleMode::Always => t!("settings.subtitleModes.always"),
        _ => t!("settings.subtitleModes.forced"),
    }
}

/// What a subtitle mode does, under its label in the panel.
pub fn mode_hint(mode: SubtitleMode) -> &'static str {
    match mode {
        SubtitleMode::Off => t!("settings.subtitleModes.offHint"),
        SubtitleMode::Foreign => t!("settings.subtitleModes.foreignHint"),
        SubtitleMode::Always => t!("settings.subtitleModes.alwaysHint"),
        _ => t!("settings.subtitleModes.forcedHint"),
    }
}

/// Everything a pane needs to know, gathered from the app's state when it changes.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Facts {
    /// The profile's name, picture and colour, once known.
    pub profile: Option<ProfileFacts>,
    /// The profile's audio and subtitle defaults as they show now (a change shows at once).
    pub defaults: Option<TrackDefaults>,
    /// The sign-in may switch profiles (scope `profiles`).
    pub can_switch: bool,
    /// The sign-in may play, so may change the defaults (scope `play`).
    pub can_play: bool,
    /// The server refused to change the defaults in this visit.
    pub refused: bool,
    /// The household's profiles (id, name), when more than one.
    pub household: Vec<(i64, String)>,
    /// "Always start as" on this server.
    pub always_start_as: Option<i64>,
    /// This TV's settings.
    pub settings: Settings,
    /// The server in use, as `host:port`.
    pub server: Option<String>,
    /// The device's facts for the Diagnostics category, label and value.
    pub device: Vec<(String, String)>,
    /// The app's version as About shows it ([`build_label`]).
    pub version: String,
    /// The open-source parts the app is built from (name, licence), once read; `None` while
    /// reading, an empty list when the app carries no NOTICES.txt.
    pub licences: Option<Vec<(String, String)>>,
    /// The account's own language; none: Automatic.
    pub language: Option<Language>,
    /// What the account's Automatic is on this TV: the language picked with the globe, else the
    /// TV's, else the server's.
    pub automatic_language: Language,
}

/// Who the Profile category's header shows.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ProfileFacts {
    pub name: String,
    pub managed: bool,
    pub avatar_url: Option<String>,
    pub avatar_color: Option<String>,
}

impl Facts {
    /// Whether the defaults can be changed from here; otherwise their rows open the web dialog.
    pub fn defaults_editable(&self) -> bool {
        self.can_play && !self.refused
    }
}

/// What a setting row ends with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum End {
    Nothing,
    Chevron,
    Switch(bool),
}

/// What choosing a row does.
#[derive(Clone, Debug, PartialEq)]
pub enum RowAction {
    /// Nothing: a row that only says something (the server, the version).
    None,
    /// Opens a side panel of choices.
    Choose(Picking),
    /// Flips a switch.
    Toggle(Toggle),
    /// Opens a page over Settings.
    Open(pley_shell::Route),
    /// Starts over from a page (another profile, another server).
    Restart(pley_shell::Route),
    /// Shows "Manage your account on the web".
    Web,
    /// Asks before signing out.
    SignOut,
    /// Asks before deleting everything the app keeps.
    DeleteData,
    /// Opens the licence of the part at this place in NOTICES.txt.
    Licence(usize),
}

/// A switch of the device's settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Toggle {
    AutoplayNext,
    PlaybackDetails,
    Passthrough(PassthroughFormat),
    ReduceMotion,
}

impl Toggle {
    /// Whether it is on in `s`.
    pub fn get(self, s: &Settings) -> bool {
        match self {
            Toggle::AutoplayNext => s.autoplay_next,
            Toggle::PlaybackDetails => s.playback_details,
            Toggle::Passthrough(f) => s.passthrough.get(f) == PassthroughChoice::On,
            Toggle::ReduceMotion => s.reduce_motion,
        }
    }

    /// Flips it in `s`. A passthrough switch turned off goes back to Auto (what the TV says it
    /// plays), as the TV app's switch off did.
    pub fn flip(self, s: &mut Settings) {
        match self {
            Toggle::AutoplayNext => s.autoplay_next = !s.autoplay_next,
            Toggle::PlaybackDetails => s.playback_details = !s.playback_details,
            Toggle::Passthrough(f) => {
                let on = s.passthrough.get(f) == PassthroughChoice::On;
                s.passthrough.set(f, if on { PassthroughChoice::Auto } else { PassthroughChoice::On });
            }
            Toggle::ReduceMotion => s.reduce_motion = !s.reduce_motion,
        }
    }
}

/// A row of a pane.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    /// The 20 dp icon before the label, Muted (DangerText on a danger row).
    pub icon: Option<Icon>,
    pub label: String,
    /// The line under the label (`Tv.Caption` Muted, two lines at most); empty for none.
    pub value: String,
    pub end: End,
    /// Drawn in DangerText: Sign out, "Delete all local data".
    pub danger: bool,
    pub action: RowAction,
}

impl Row {
    fn new(icon: Option<Icon>, label: impl Into<String>, value: impl Into<String>, action: RowAction) -> Row {
        Row { icon, label: label.into(), value: value.into(), end: End::Nothing, danger: false, action }
    }

    fn chevron(self) -> Row {
        Row { end: End::Chevron, ..self }
    }
}

/// What a pane holds, top to bottom.
#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    /// The Profile category's header: the avatar, the name, the server.
    Profile,
    /// A paragraph before the rows (`Tv.Body` Muted).
    Intro(String),
    /// A heading inside the pane ("Open-source licences").
    Heading(&'static str),
    /// The device's facts, label and value, in `Tv.Caption`.
    Facts(Vec<(String, String)>),
    /// A row that does something, or only says something.
    Row(Row),
}

impl Item {
    /// Whether it takes focus.
    pub fn focusable(&self) -> bool {
        matches!(self, Item::Row(_))
    }

    /// What names a row in its pane wherever it is: its label, and its part's place for a
    /// licence. Focus stays on the row so named when rows come or go above it.
    pub fn key(&self) -> Option<(&str, usize)> {
        match self {
            Item::Row(r) => Some((r.label.as_str(), if let RowAction::Licence(i) = r.action { i } else { 0 })),
            _ => None,
        }
    }
}

/// The rows of category `c`.
pub fn items(c: SettingsSection, f: &Facts) -> Vec<Item> {
    let mut out = Vec::new();
    let mut row = |r: Row| out.push(Item::Row(r));
    match c {
        SettingsSection::Profile => return profile(f),
        SettingsSection::Playback => {
            let q = quality_preset(&f.settings.quality).unwrap_or(&QUALITY_PRESETS[1]);
            let quality = t!("settings.playback.quality");
            row(Row::new(Some(Icon::Gauge), quality, q.label(), RowAction::Choose(Picking::Quality)));
            row(switch(
                Icon::SkipForward,
                t!("settings.playback.autoplay"),
                t!("settings.playback.autoplayHint"),
                Toggle::AutoplayNext,
                f,
            ));
            row(switch(
                Icon::Info,
                t!("settings.playback.details"),
                t!("settings.playback.detailsHint"),
                Toggle::PlaybackDetails,
                f,
            ));
        }
        SettingsSection::Audio => {
            out.push(Item::Intro(t!("settings.audio.intro").into()));
            for format in PASSTHROUGH_SWITCHES {
                let (label, hint) = passthrough_words(format);
                out.push(Item::Row(switch(Icon::Volume2, label, hint, Toggle::Passthrough(format), f)));
            }
        }
        SettingsSection::SubtitlesAndDisplay => {
            let s = &f.settings;
            row(Row::new(
                Some(Icon::Captions),
                t!("settings.display.subtitleSize"),
                s.subtitle_size.label(),
                RowAction::Choose(Picking::Size),
            ));
            row(Row::new(
                Some(Icon::Square),
                t!("settings.display.subtitleBackground"),
                s.subtitle_background.label(),
                RowAction::Choose(Picking::Background),
            ));
            row(Row::new(
                Some(Icon::Pencil),
                t!("settings.display.subtitleFont"),
                s.subtitle_font.label(),
                RowAction::Choose(Picking::Font),
            ));
            row(Row::new(
                Some(Icon::ArrowUp),
                t!("settings.display.subtitlePosition"),
                s.subtitle_position.label(),
                RowAction::Choose(Picking::Position),
            ));
            row(switch(
                Icon::Sparkles,
                t!("settings.display.reduceMotion"),
                t!("settings.display.reduceMotionHint"),
                Toggle::ReduceMotion,
                f,
            ));
        }
        SettingsSection::Server => {
            let name = f.server.clone().unwrap_or_else(|| t!("settings.server.unknown").into());
            row(Row::new(Some(Icon::Server), name, t!("settings.server.value"), RowAction::None));
            let switch = t!("settings.server.switch");
            row(Row::new(Some(Icon::ArrowLeft), switch, "", RowAction::Restart(pley_shell::Route::Servers)).chevron());
        }
        SettingsSection::Diagnostics => {
            row(Row::new(
                Some(Icon::Activity),
                t!("settings.diagnostics.open"),
                t!("settings.diagnostics.openValue"),
                RowAction::Open(pley_shell::Route::Diagnostics),
            )
            .chevron());
            out.push(Item::Facts(f.device.clone()));
        }
        SettingsSection::About => {
            row(Row::new(Some(Icon::Info), t!("settings.about.version"), f.version.clone(), RowAction::None));
            out.push(Item::Heading(t!("settings.about.licences")));
            match &f.licences {
                // Read off the main thread: the rows come when it is done.
                None => {}
                // The Licences page says the same without the file.
                Some(list) if list.is_empty() => out.push(Item::Intro(crate::licences::no_notices().into())),
                Some(list) => {
                    for (i, (name, licence)) in list.iter().enumerate() {
                        out.push(Item::Row(Row::new(None, name.clone(), licence.clone(), RowAction::Licence(i))));
                    }
                }
            }
            // Discover's titles and pictures come from TMDB, whose terms ask for this line.
            out.push(Item::Heading(t!("settings.about.credits")));
            out.push(Item::Row(Row::new(None, "TMDB", tmdb_notice(), RowAction::None)));
        }
        SettingsSection::SignOut => {
            out.push(Item::Intro(match f.server.as_deref() {
                Some(server) => t!("settings.signOut.intro", server = server),
                None => t!("settings.signOut.introNoServer").into(),
            }));
            out.push(Item::Row(Row {
                danger: true,
                ..Row::new(Some(Icon::LogOut), t!("settings.signOut.button"), "", RowAction::SignOut)
            }));
            out.push(Item::Row(Row {
                danger: true,
                ..Row::new(
                    Some(Icon::Trash2),
                    t!("settings.signOut.deleteData"),
                    t!("settings.signOut.deleteDataValue"),
                    RowAction::DeleteData,
                )
            }));
        }
    }
    out
}

fn switch(icon: Icon, label: &str, hint: &str, toggle: Toggle, f: &Facts) -> Row {
    Row { end: End::Switch(toggle.get(&f.settings)), ..Row::new(Some(icon), label, hint, RowAction::Toggle(toggle)) }
}

/// The Profile category (`TvSettingsScreen.kt:331-408`).
fn profile(f: &Facts) -> Vec<Item> {
    let mut out = vec![Item::Profile];
    let mut row = |r: Row| out.push(Item::Row(r));
    if f.can_switch {
        let label = t!("settings.profile.switchProfile");
        row(Row::new(Some(Icon::Users), label, "", RowAction::Restart(pley_shell::Route::Profiles)).chevron());
    }
    let pick = |p: Picking| if f.defaults_editable() { RowAction::Choose(p) } else { RowAction::Web };
    let d = f.defaults.as_ref();
    let value = |text: String| match d {
        None => String::new(),
        Some(_) if f.defaults_editable() => text,
        // Read-only: changed on the web.
        Some(_) => t!("settings.profile.onTheWeb", value = text),
    };
    let audio = d
        .and_then(|d| d.audio_language.as_deref())
        .map_or_else(|| t!("settings.profile.fileDefault").into(), language_label);
    let label = t!("settings.profile.audioLanguage");
    row(Row::new(Some(Icon::AudioLines), label, value(audio), pick(Picking::AudioLanguage)));
    let mode = d.map_or(SubtitleMode::Forced, |d| d.subtitle_mode);
    let label = t!("settings.profile.subtitles");
    row(Row::new(Some(Icon::Captions), label, value(mode_label(mode).into()), pick(Picking::SubtitleMode)));
    if d.is_some() && matches!(mode, SubtitleMode::Foreign | SubtitleMode::Always) {
        let language = d
            .and_then(|d| d.subtitle_language.as_deref())
            .map_or_else(|| t!("settings.profile.sameAsAudio").into(), language_label);
        let label = t!("settings.profile.subtitleLanguage");
        row(Row::new(Some(Icon::Captions), label, value(language), pick(Picking::SubtitleLanguage)));
    }
    if f.profile.is_some() {
        let action = if f.can_play { RowAction::Choose(Picking::Language) } else { RowAction::Web };
        row(Row::new(Some(Icon::Languages), t!("settings.language.label"), language_value(f), action));
    }
    let history = t!("settings.profile.history");
    row(Row::new(Some(Icon::History), history, "", RowAction::Open(pley_shell::Route::History)).chevron());
    // The Libraries page sorts them; with three or fewer the rail lists them and has no way there.
    row(Row::new(
        Some(Icon::ArrowUpDown),
        t!("settings.profile.sortLibraries"),
        t!("settings.profile.sortLibrariesValue"),
        RowAction::Open(pley_shell::Route::Libraries { sort: true }),
    )
    .chevron());
    if f.household.len() > 1 {
        let name = f
            .always_start_as
            .and_then(|id| f.household.iter().find(|(p, _)| *p == id))
            .map_or_else(|| t!("settings.profile.usedLast").to_string(), |(_, name)| name.clone());
        let label = t!("settings.profile.alwaysStartAs");
        row(Row::new(Some(Icon::UserRound), label, name, RowAction::Choose(Picking::AlwaysStartAs)));
    }
    row(Row::new(
        Some(Icon::AppWindow),
        t!("settings.profile.manageWeb"),
        t!("settings.profile.manageWebValue"),
        RowAction::Web,
    ));
    out
}

/// The Language row's value: the account's language, named in itself ("Français"), else
/// Automatic with what that is on this TV.
fn language_value(f: &Facts) -> String {
    match f.language {
        Some(l) => l.name().to_string(),
        None => t!("settings.language.automatic", language = f.automatic_language.name()),
    }
}

/// The index of the first row that takes focus: where Right or OK on a category goes.
pub fn first_row(items: &[Item]) -> Option<usize> {
    items.iter().position(Item::focusable)
}

/// Which side panel is open.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Picking {
    AudioLanguage,
    SubtitleMode,
    SubtitleLanguage,
    AlwaysStartAs,
    /// The language Pley is in for the account.
    Language,
    Quality,
    Size,
    Background,
    Font,
    Position,
}

/// What a choice sets.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    /// A language of the defaults; `None` is the file's default, or the audio's language.
    Language(Option<String>),
    Mode(SubtitleMode),
    /// "Always start as"; `None` asks who's watching.
    Profile(Option<i64>),
    /// The account's language; `None` is Automatic.
    AppLanguage(Option<Language>),
    Quality(&'static str),
    Size(SubtitleSize),
    Background(SubtitleBackground),
    Font(SubtitleFont),
    Position(SubtitlePosition),
}

/// One choice of a side panel.
#[derive(Clone, Debug, PartialEq)]
pub struct Choice {
    pub label: String,
    /// What it does, under the label (three lines at most).
    pub hint: Option<&'static str>,
    /// What choosing it sets.
    pub value: Value,
}

impl Choice {
    fn new(label: impl Into<String>, value: Value) -> Choice {
        Choice { label: label.into(), hint: None, value }
    }
}

/// A side panel: its title, its choices, and the one in force.
#[derive(Clone, Debug, PartialEq)]
pub struct Panel {
    pub title: String,
    pub choices: Vec<Choice>,
    /// The index of the choice in force: checked, and focused when the panel opens.
    pub current: usize,
}

/// The panel of `p`. `fonts` are the subtitle faces this TV draws differently from the default.
pub fn panel(p: Picking, f: &Facts, fonts: &[SubtitleFont]) -> Panel {
    let d = f.defaults.clone().unwrap_or_default();
    let with_languages = |none: &str| {
        std::iter::once(Choice::new(none, Value::Language(None)))
            .chain(languages().into_iter().map(|(code, name)| Choice::new(name, Value::Language(Some(code.into())))))
            .collect::<Vec<_>>()
    };
    let s = &f.settings;
    let (title, choices, current) = match p {
        Picking::AudioLanguage => (
            t!("settings.profile.audioLanguage"),
            with_languages(t!("settings.profile.fileDefault")),
            Value::Language(d.audio_language),
        ),
        Picking::SubtitleLanguage => (
            t!("settings.profile.subtitleLanguage"),
            with_languages(t!("settings.profile.sameAsAudio")),
            Value::Language(d.subtitle_language),
        ),
        Picking::SubtitleMode => (
            t!("settings.profile.subtitles"),
            SUBTITLE_MODES
                .iter()
                .map(|m| Choice { hint: Some(mode_hint(*m)), ..Choice::new(mode_label(*m), Value::Mode(*m)) })
                .collect(),
            Value::Mode(d.subtitle_mode),
        ),
        Picking::AlwaysStartAs => (
            t!("settings.profile.alwaysStartAs"),
            std::iter::once(Choice::new(t!("settings.profile.usedLast"), Value::Profile(None)))
                .chain(f.household.iter().map(|(id, name)| Choice::new(name.clone(), Value::Profile(Some(*id)))))
                .collect(),
            Value::Profile(f.always_start_as),
        ),
        Picking::Language => (
            t!("settings.language.label"),
            std::iter::once(Choice::new(
                t!("settings.language.automatic", language = f.automatic_language.name()),
                Value::AppLanguage(None),
            ))
            .chain(Language::ALL.into_iter().map(|l| Choice::new(l.name(), Value::AppLanguage(Some(l)))))
            .collect(),
            Value::AppLanguage(f.language),
        ),
        Picking::Quality => (
            t!("settings.playback.quality"),
            QUALITY_PRESETS
                .iter()
                .map(|q| Choice { hint: q.hint(), ..Choice::new(q.label(), Value::Quality(q.id)) })
                .collect(),
            Value::Quality(quality_preset(&s.quality).map_or("auto", |q| q.id)),
        ),
        Picking::Size => (
            t!("settings.display.subtitleSize"),
            SubtitleSize::ALL.iter().map(|v| Choice::new(v.label(), Value::Size(*v))).collect(),
            Value::Size(s.subtitle_size),
        ),
        Picking::Background => (
            t!("settings.display.subtitleBackground"),
            SubtitleBackground::ALL.iter().map(|v| Choice::new(v.label(), Value::Background(*v))).collect(),
            Value::Background(s.subtitle_background),
        ),
        Picking::Font => (
            t!("settings.display.subtitleFont"),
            // The default, the faces this TV has, and the one in force even if it no longer has it.
            SubtitleFont::ALL
                .iter()
                .filter(|v| **v == SubtitleFont::Sans || **v == s.subtitle_font || fonts.contains(v))
                .map(|v| Choice::new(v.label(), Value::Font(*v)))
                .collect(),
            Value::Font(s.subtitle_font),
        ),
        Picking::Position => (
            t!("settings.display.subtitlePosition"),
            SubtitlePosition::ALL.iter().map(|v| Choice::new(v.label(), Value::Position(*v))).collect(),
            Value::Position(s.subtitle_position),
        ),
    };
    let current = choices.iter().position(|c| c.value == current).unwrap_or(0);
    Panel { title: title.into(), choices, current }
}

/// The defaults after `picking` chose `value`, or `None` when nothing changes.
pub fn defaults_for(picking: Picking, before: &TrackDefaults, value: &Value) -> Option<TrackDefaults> {
    let mut after = before.clone();
    match (picking, value) {
        (Picking::AudioLanguage, Value::Language(code)) => after.audio_language = code.clone(),
        (Picking::SubtitleLanguage, Value::Language(code)) => after.subtitle_language = code.clone(),
        (Picking::SubtitleMode, Value::Mode(m)) => after.subtitle_mode = *m,
        _ => return None,
    }
    (after != *before).then_some(after)
}

/// Applies a choice of the device's settings to `s`; `false` for a choice that isn't one.
pub fn apply_setting(s: &mut Settings, value: &Value) -> bool {
    match value {
        Value::Quality(id) => s.quality = (*id).to_string(),
        Value::Size(v) => s.subtitle_size = *v,
        Value::Background(v) => s.subtitle_background = *v,
        Value::Font(v) => s.subtitle_font = *v,
        Value::Position(v) => s.subtitle_position = *v,
        _ => return false,
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use pley_shell::Route;

    /// Alex's facts: may switch and play, a household of three, forced subtitles in English.
    fn alex() -> Facts {
        Facts {
            profile: Some(ProfileFacts { name: "Alex".into(), ..ProfileFacts::default() }),
            defaults: Some(TrackDefaults {
                audio_language: Some("eng".into()),
                subtitle_language: None,
                subtitle_mode: SubtitleMode::Forced,
            }),
            can_switch: true,
            can_play: true,
            household: vec![(1, "Alex".into()), (2, "Sam".into()), (3, "Robin".into())],
            server: Some("192.168.1.20:7539".into()),
            ..Facts::default()
        }
    }

    fn rows(items: &[Item]) -> Vec<&Row> {
        items
            .iter()
            .filter_map(|i| match i {
                Item::Row(r) => Some(r),
                _ => None,
            })
            .collect()
    }

    fn labels(items: &[Item]) -> Vec<&str> {
        rows(items).iter().map(|r| r.label.as_str()).collect()
    }

    #[test]
    fn profile_rows_follow_android() {
        let items = items(SettingsSection::Profile, &alex());
        assert_eq!(items[0], Item::Profile);
        assert_eq!(
            labels(&items),
            [
                "Switch profile",
                "Audio language",
                "Subtitles",
                "Language",
                "Watch history",
                "Sort libraries",
                "Always start as",
                "Manage your account on the web"
            ]
        );
        let r = rows(&items);
        assert_eq!(r[1].value, "English");
        assert_eq!(r[1].action, RowAction::Choose(Picking::AudioLanguage));
        assert_eq!(r[2].value, "Only for foreign dialogue");
        assert_eq!(r[3].value, "Automatic (English)");
        assert_eq!(r[3].action, RowAction::Choose(Picking::Language));
        assert_eq!(r[4].action, RowAction::Open(Route::History));
        assert_eq!(r[5].action, RowAction::Open(Route::Libraries { sort: true }));
        assert_eq!(r[6].value, "Profile used last");
        assert_eq!(r[7].value, "Name, picture, password, PIN, two-step, devices and apps");
        assert_eq!(first_row(&items), Some(1));
    }

    #[test]
    fn the_language_is_the_accounts_or_automatic_with_what_that_is_here() {
        let f = Facts { automatic_language: Language::Fr, ..alex() };
        let language = |f: &Facts| {
            let items = items(SettingsSection::Profile, f);
            rows(&items).into_iter().find(|r| r.label == "Language").map(|r| r.value.clone())
        };
        assert_eq!(language(&f).as_deref(), Some("Automatic (Français)"));
        let own = Facts { language: Some(Language::Es), ..f.clone() };
        assert_eq!(language(&own).as_deref(), Some("Español"));
        // Its panel: Automatic, then each language in itself, the account's checked.
        let p = panel(Picking::Language, &own, &[]);
        let labels: Vec<&str> = p.choices.iter().map(|c| c.label.as_str()).collect();
        assert_eq!(labels, ["Automatic (Français)", "English", "Français", "Español"]);
        assert_eq!(p.current, 3);
        assert_eq!(panel(Picking::Language, &f, &[]).current, 0);
        // In French, Automatic is too; the languages keep their own names.
        pley_i18n::with_language(Language::Fr, || {
            let p = panel(Picking::Language, &f, &[]);
            assert_eq!(p.title, "Langue");
            assert_eq!(p.choices[0].label, "Automatique (Français)");
            assert_eq!(p.choices[3].label, "Español");
        });
        // Not before who is watching is known; without play, on the web.
        assert_eq!(language(&Facts { profile: None, ..f.clone() }), None);
        let items = items(SettingsSection::Profile, &Facts { can_play: false, ..f });
        let row = rows(&items).into_iter().find(|r| r.label == "Language").unwrap();
        assert_eq!(row.action, RowAction::Web);
    }

    #[test]
    fn a_subtitle_language_only_for_modes_that_read_one() {
        let mut f = alex();
        for (mode, shown) in [
            (SubtitleMode::Off, false),
            (SubtitleMode::Forced, false),
            (SubtitleMode::Foreign, true),
            (SubtitleMode::Always, true),
        ] {
            f.defaults.as_mut().unwrap().subtitle_mode = mode;
            let items = items(SettingsSection::Profile, &f);
            assert_eq!(labels(&items).contains(&"Subtitle language"), shown, "{mode:?}");
        }
        f.defaults.as_mut().unwrap().subtitle_language = Some("fre".into());
        let items = items(SettingsSection::Profile, &f);
        assert!(rows(&items).iter().any(|r| r.label == "Subtitle language" && r.value == "French"));
    }

    #[test]
    fn without_play_or_once_refused_the_defaults_go_to_the_web() {
        for f in [Facts { can_play: false, ..alex() }, Facts { refused: true, ..alex() }] {
            let items = items(SettingsSection::Profile, &f);
            let audio = rows(&items).into_iter().find(|r| r.label == "Audio language").unwrap();
            assert_eq!(audio.value, "English · change on the web");
            assert_eq!(audio.action, RowAction::Web);
        }
    }

    #[test]
    fn a_single_profile_without_switching_starts_on_its_defaults() {
        let f = Facts { can_switch: false, household: vec![(1, "Alex".into())], defaults: None, ..alex() };
        let items = items(SettingsSection::Profile, &f);
        assert!(!labels(&items).contains(&"Switch profile"));
        assert!(!labels(&items).contains(&"Always start as"));
        // Before who is watching is known, the defaults have no value.
        assert_eq!(rows(&items)[0].label, "Audio language");
        assert_eq!(rows(&items)[0].value, "");
    }

    #[test]
    fn every_category_has_its_rows() {
        let f = alex();
        let playback = items(SettingsSection::Playback, &f);
        assert_eq!(labels(&playback), ["Quality", "Play the next episode by itself", "Playback details"]);
        let audio = items(SettingsSection::Audio, &f);
        assert!(matches!(audio[0], Item::Intro(_)));
        assert_eq!(labels(&audio), ["Pass through Dolby TrueHD", "Pass through DTS"]);
        assert_eq!(first_row(&audio), Some(1));
        let display = items(SettingsSection::SubtitlesAndDisplay, &f);
        let expected = ["Subtitle size", "Subtitle background", "Subtitle font", "Subtitle position", "Reduce motion"];
        assert_eq!(labels(&display), expected);
        assert_eq!(labels(&items(SettingsSection::Server, &f)), ["192.168.1.20:7539", "Switch server"]);
        let sign_out = items(SettingsSection::SignOut, &f);
        let intro = "Sign this TV out of 192.168.1.20:7539. You'll need a new link code to sign in again.";
        assert_eq!(sign_out[0], Item::Intro(intro.into()));
        let r = rows(&sign_out);
        assert_eq!((r[0].label.as_str(), r[0].danger, &r[0].action), ("Sign out", true, &RowAction::SignOut));
        let delete = (r[1].label.as_str(), r[1].danger, &r[1].action);
        assert_eq!(delete, ("Delete all local data", true, &RowAction::DeleteData));
    }

    #[test]
    fn rows_carry_the_android_apps_icons() {
        let f = alex();
        let icons = |c| rows(&items(c, &f)).iter().map(|r| r.icon).collect::<Vec<_>>();
        let display = [Icon::Captions, Icon::Square, Icon::Pencil, Icon::ArrowUp, Icon::Sparkles].map(Some);
        assert_eq!(icons(SettingsSection::SubtitlesAndDisplay), display);
        assert!(icons(SettingsSection::Audio).iter().all(|i| *i == Some(Icon::Volume2)));
        assert_eq!(icons(SettingsSection::Diagnostics)[0], Some(Icon::Activity));
        assert_eq!(category_look(SettingsSection::Diagnostics).1, Icon::Activity);
        assert_eq!(icons(SettingsSection::Profile).last(), Some(&Some(Icon::AppWindow)));
    }

    #[test]
    fn about_lists_the_parts_once_read() {
        let mut f = Facts { version: "0.2.0".into(), ..alex() };
        let reading = items(SettingsSection::About, &f);
        assert_eq!(labels(&reading), ["Version", "TMDB"]);
        assert_eq!(reading[1], Item::Heading("Open-source licences"));
        f.licences = Some(Vec::new());
        assert!(matches!(items(SettingsSection::About, &f)[2], Item::Intro(_)));
        let parts = vec![("Roboto (the typeface)".into(), "OFL-1.1".into()), ("serde 1.0.219".into(), "MIT".into())];
        f.licences = Some(parts);
        let about = items(SettingsSection::About, &f);
        let r = rows(&about);
        assert_eq!((r[0].label.as_str(), r[0].value.as_str()), ("Version", "0.2.0"));
        let serde = (r[2].label.as_str(), r[2].value.as_str(), &r[2].action);
        assert_eq!(serde, ("serde 1.0.219", "MIT", &RowAction::Licence(1)));
    }

    #[test]
    fn about_credits_tmdb_after_the_licences() {
        let f = Facts { licences: Some(vec![("serde 1.0.219".into(), "MIT".into())]), ..alex() };
        let about = items(SettingsSection::About, &f);
        let n = about.len();
        assert_eq!(about[n - 2], Item::Heading("Credits"));
        let Item::Row(tmdb) = &about[n - 1] else { panic!("the credit is a row") };
        assert_eq!((tmdb.label.as_str(), tmdb.value.as_str()), ("TMDB", tmdb_notice()));
        assert_eq!(tmdb.action, RowAction::None);
        // With nothing to choose it still takes focus, so it is read out like the other rows.
        assert!(about[n - 1].focusable());
    }

    #[test]
    fn switches_flip_the_settings() {
        let mut s = Settings::default();
        assert!(Toggle::AutoplayNext.get(&s));
        Toggle::AutoplayNext.flip(&mut s);
        assert!(!s.autoplay_next);
        let truehd = Toggle::Passthrough(PassthroughFormat::TrueHd);
        assert!(!truehd.get(&s));
        truehd.flip(&mut s);
        assert_eq!(s.passthrough.get(PassthroughFormat::TrueHd), PassthroughChoice::On);
        truehd.flip(&mut s);
        assert_eq!(s.passthrough.get(PassthroughFormat::TrueHd), PassthroughChoice::Auto);
        Toggle::ReduceMotion.flip(&mut s);
        assert!(s.reduce_motion);
    }

    #[test]
    fn panels_open_on_the_choice_in_force() {
        let f = alex();
        let quality = panel(Picking::Quality, &f, &[]);
        assert_eq!(quality.choices.len(), QUALITY_PRESETS.len());
        assert_eq!(quality.choices[quality.current].label, "Automatic");
        let audio = panel(Picking::AudioLanguage, &f, &[]);
        assert_eq!(audio.choices.len(), 1 + LANGUAGES.len());
        assert_eq!(audio.choices[0].label, "The file's default");
        assert_eq!(audio.choices[audio.current].label, "English");
        let subs = panel(Picking::SubtitleLanguage, &f, &[]);
        assert_eq!((subs.current, subs.choices[0].label.as_str()), (0, "Same as the audio language"));
        let modes = panel(Picking::SubtitleMode, &f, &[]);
        assert_eq!(modes.choices[modes.current].label, "Only for foreign dialogue");
        assert_eq!(modes.choices[0].hint, Some("Subtitles stay off until you turn them on."));
        let start = panel(Picking::AlwaysStartAs, &f, &[]);
        let names: Vec<&str> = start.choices.iter().map(|c| c.label.as_str()).collect();
        assert_eq!(names, ["Profile used last", "Alex", "Sam", "Robin"]);
    }

    #[test]
    fn fonts_offered_are_those_this_tv_draws() {
        let mut f = alex();
        assert_eq!(panel(Picking::Font, &f, &[]).choices.len(), 1);
        let more = panel(Picking::Font, &f, &[SubtitleFont::Serif, SubtitleFont::Monospace]);
        assert_eq!(more.choices.len(), 3);
        // The face in force stays offered even if this TV no longer has it.
        f.settings.subtitle_font = SubtitleFont::Casual;
        let kept = panel(Picking::Font, &f, &[]);
        assert_eq!(kept.choices.len(), 2);
        assert_eq!(kept.choices[kept.current].value, Value::Font(SubtitleFont::Casual));
    }

    #[test]
    fn a_choice_changes_the_defaults_or_the_settings() {
        let before = alex().defaults.unwrap();
        assert_eq!(defaults_for(Picking::AudioLanguage, &before, &Value::Language(Some("eng".into()))), None);
        let after = defaults_for(Picking::SubtitleMode, &before, &Value::Mode(SubtitleMode::Always)).unwrap();
        assert_eq!(after.subtitle_mode, SubtitleMode::Always);
        assert_eq!(after.audio_language.as_deref(), Some("eng"));
        let lang = defaults_for(Picking::SubtitleLanguage, &before, &Value::Language(Some("ger".into()))).unwrap();
        assert_eq!(lang.subtitle_language.as_deref(), Some("ger"));
        assert_eq!(defaults_for(Picking::Quality, &before, &Value::Quality("8m")), None);

        let mut s = Settings::default();
        assert!(apply_setting(&mut s, &Value::Quality("8m")));
        assert!(apply_setting(&mut s, &Value::Size(SubtitleSize::Large)));
        assert!(apply_setting(&mut s, &Value::Position(SubtitlePosition::ALL[2])));
        assert_eq!((s.quality.as_str(), s.subtitle_size), ("8m", SubtitleSize::Large));
        assert_eq!(s.subtitle_position, SubtitlePosition::ALL[2]);
        assert!(!apply_setting(&mut s, &Value::Mode(SubtitleMode::Off)));
    }

    #[test]
    fn languages_are_named() {
        assert_eq!(LANGUAGES.len(), 34);
        assert_eq!(language_label("ger"), "German");
        // Codes the panel doesn't offer are named as pley-core names them.
        assert_eq!(language_label("xx"), pley_core::format::language_name(Some("xx")).unwrap_or_else(|| "xx".into()));
        // In English the panel lists them as Android does, every one named.
        let names: Vec<String> = languages().into_iter().map(|(_, name)| name).collect();
        assert_eq!(names.first().map(String::as_str), Some("Arabic"));
        assert_eq!(names.iter().position(|n| n == "Croatian"), Some(4));
        assert_eq!(names.last().map(String::as_str), Some("Vietnamese"));
        assert!(languages().iter().all(|(code, name)| !name.eq_ignore_ascii_case(code)));
    }

    #[test]
    fn languages_sort_by_their_letters_whatever_the_accent() {
        let list = vec![("vie", "Vietnamita".to_string()), ("ara", "Árabe".to_string()), ("bul", "Búlgaro".into())];
        let codes: Vec<&str> = by_name(list).into_iter().map(|(code, _)| code).collect();
        assert_eq!(codes, ["ara", "bul", "vie"]);
    }

    #[test]
    fn the_panes_speak_french_and_spanish() {
        let f = Facts { can_play: false, defaults: Some(TrackDefaults::default()), ..alex() };
        pley_i18n::with_language(Language::Fr, || {
            assert_eq!(category_look(SettingsSection::SubtitlesAndDisplay).0, "Sous-titres et affichage");
            let profile = items(SettingsSection::Profile, &f);
            let audio = rows(&profile).into_iter().find(|r| r.label == "Langue audio").map(|r| r.value.clone());
            assert_eq!(audio.as_deref(), Some("Celle du fichier par défaut · à modifier sur le web"));
            let quality = panel(Picking::Quality, &f, &[]);
            let labels: Vec<&str> = quality.choices.iter().map(|c| c.label.as_str()).collect();
            assert_eq!(labels[..3], ["Original", "Automatique", "30 Mbit/s 4K"]);
            assert_eq!(labels.last(), Some(&"2 Mbit/s 480p"));
            assert_eq!(quality.choices[1].hint, Some("S\u{2019}adapte aux conditions."));
            assert_eq!(rows(&items(SettingsSection::Playback, &f))[0].value, "Automatique");
            let sign_out = items(SettingsSection::SignOut, &f);
            let Item::Intro(intro) = &sign_out[0] else { panic!("Sign out starts with what it does") };
            assert!(intro.starts_with("Déconnectez cette TV de 192.168.1.20:7539."), "{intro}");
        });
        pley_i18n::with_language(Language::Es, || {
            let display = items(SettingsSection::SubtitlesAndDisplay, &f);
            let values: Vec<&str> = rows(&display).iter().map(|r| r.value.as_str()).collect();
            assert_eq!(values[..4], ["Normal", "Ninguno", "Sans serif", "Abajo"]);
            let sizes = panel(Picking::Size, &f, &[]);
            assert_eq!(sizes.title, "Tamaño de los subtítulos");
            assert_eq!(sizes.choices.last().map(|c| c.label.as_str()), Some("Muy grande"));
            let modes = panel(Picking::SubtitleMode, &f, &[]);
            assert_eq!(modes.choices[2].label, "Cuando el audio está en otro idioma");
            let audio = items(SettingsSection::Audio, &f);
            assert_eq!(labels(&audio), ["Enviar Dolby TrueHD tal cual", "Enviar DTS tal cual"]);
            let server = items(SettingsSection::Server, &Facts { server: None, ..f.clone() });
            assert_eq!(labels(&server), ["Ningún servidor", "Cambiar de servidor"]);
        });
    }

    #[test]
    fn the_version_names_its_commit_when_known() {
        assert_eq!(build_label("0.2.0", Some("1a2b3c4d5e6f7a8b")), "0.2.0 (1a2b3c4d5)");
        assert_eq!(build_label("0.2.0", Some("abc")), "0.2.0 (abc)");
        assert_eq!(build_label("0.2.0", Some("unknown")), "0.2.0");
        assert_eq!(build_label("0.2.0", None), "0.2.0");
    }
}
