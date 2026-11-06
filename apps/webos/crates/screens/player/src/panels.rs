//! The player's side panels (`TvPlayerPanels.kt`, `TvPlayerExtras.kt`; the TV app's
//! `ui/player/panels.tsx`): Subtitles (with Sync, Find subtitles… and the style choices), Find
//! subtitles, Audio, Quality, Sleep timer, Speed, and Info (Playback details). Each is
//! a list of rows built from the player's state, drawn in a kit `SidePanel` and scrolled by a kit
//! `List`.

use pley_base::{Icon, Key, Point, Rect, TextOpts};
use pley_core::Settings;
use pley_core::format::{Fact, FactBlock, media_facts, stream_block};
use pley_core::format::{channels_label, codec_label, format_bitrate};
use pley_core::media::EngineStats;
use pley_core::model::SubtitleResult;
use pley_core::store::{SubtitleBackground, SubtitleFont, SubtitlePosition, SubtitleSize};
use pley_i18n::t;
use pley_kit::{Heading, List, ListRow, ListState, Stepper, Trailing};
use pley_player::PlayerState;
use pley_player::controller::SubtitleSearch;
use pley_player::details::{SubtitleDelivery, burned_in};
use pley_player::sleep::{SLEEP_CHOICES, SleepChoice, sleep_label};
use pley_subs::{SYNC_MAX_MS, format_offset};
use pley_ui::theme;
use pley_ui::{Dirs, FocusOpts, Id, Ui};

use crate::controls::{Btn, register};
use crate::text;

/// A panel of the player.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Panel {
    Subtitles,
    Find,
    Audio,
    Quality,
    Sleep,
    Speed,
    Info,
}

impl Panel {
    /// The control row's button it opens from, where focus goes back when it closes.
    pub fn button(self) -> Btn {
        match self {
            Panel::Subtitles | Panel::Find => Btn::Subtitles,
            Panel::Audio => Btn::Audio,
            Panel::Quality => Btn::Quality,
            Panel::Sleep => Btn::Sleep,
            Panel::Speed => Btn::Speed,
            Panel::Info => Btn::Info,
        }
    }

    /// The panel a button opens.
    pub fn of(b: Btn) -> Option<Panel> {
        Some(match b {
            Btn::Subtitles => Panel::Subtitles,
            Btn::Audio => Panel::Audio,
            Btn::Quality => Panel::Quality,
            Btn::Sleep => Panel::Sleep,
            Btn::Speed => Panel::Speed,
            Btn::Info => Panel::Info,
            _ => return None,
        })
    }
}

/// What a row does when chosen.
#[derive(Clone, Debug, PartialEq)]
pub enum Pick {
    Subtitle(Option<i64>),
    Find,
    Size(SubtitleSize),
    Background(SubtitleBackground),
    Font(SubtitleFont),
    Position(SubtitlePosition),
    Result(usize),
    Audio(i64),
    PlayOriginal,
    Quality(&'static str),
    Sleep(SleepChoice),
    Speed(f32),
}

impl Pick {
    /// Choosing it leaves the panel open: the subtitle look's choices, and the search's rows
    /// (the panel closes once the subtitles are added).
    pub fn keeps_open(&self) -> bool {
        matches!(
            self,
            Pick::Size(_) | Pick::Background(_) | Pick::Font(_) | Pick::Position(_) | Pick::Find | Pick::Result(_)
        )
    }
}

/// A row of a panel.
#[derive(Clone, Debug, PartialEq)]
pub enum Row {
    Title(String),
    Heading(String),
    Note(String),
    /// The subtitle Sync row, with its value.
    Sync(i32),
    Choice {
        pick: Pick,
        label: String,
        hint: String,
        checked: bool,
        icon: Option<Icon>,
        enabled: bool,
    },
}

impl Row {
    fn choice(pick: Pick, label: impl Into<String>, checked: bool) -> Row {
        Row::Choice { pick, label: label.into(), hint: String::new(), checked, icon: None, enabled: true }
    }

    fn hint(self, text: impl Into<String>) -> Row {
        match self {
            Row::Choice { pick, label, checked, icon, enabled, .. } => {
                Row::Choice { pick, label, hint: text.into(), checked, icon, enabled }
            }
            other => other,
        }
    }

    fn icon(self, i: Icon) -> Row {
        match self {
            Row::Choice { pick, label, hint, checked, enabled, .. } => {
                Row::Choice { pick, label, hint, checked, icon: Some(i), enabled }
            }
            other => other,
        }
    }

    /// Whether focus can land on it.
    fn focusable(&self) -> bool {
        matches!(self, Row::Sync(_) | Row::Choice { enabled: true, .. })
    }

    fn checked(&self) -> bool {
        matches!(self, Row::Choice { checked: true, .. })
    }
}

/// What the panels need besides the player's state.
pub struct PanelInput<'a> {
    pub state: &'a PlayerState,
    pub settings: &'a Settings,
    /// The fonts the TV can draw subtitles in.
    pub fonts: &'a [SubtitleFont],
    /// Find subtitles… is offered (the server can search).
    pub can_find: bool,
    /// The last results found, kept while one is being added.
    pub found: &'a [SubtitleResult],
    /// Why adding the last one failed.
    pub add_failed: Option<&'a str>,
}

/// The rows of `panel`.
pub fn rows(panel: Panel, i: &PanelInput) -> Vec<Row> {
    let s = i.state;
    let mut out = Vec::new();
    match panel {
        Panel::Subtitles => {
            out.push(Row::Title(t!("player.controls.subtitles").into()));
            if s.subtitle_sync {
                out.push(Row::Sync(s.subtitle_offset_ms));
            }
            for t in &s.subtitles {
                let mut r = Row::choice(Pick::Subtitle(t.index), &t.label, t.selected);
                if t.burn_in {
                    r = r.hint(burned_in());
                } else if t.added {
                    r = r.hint(t!("player.subtitles.added"));
                }
                out.push(r);
            }
            if i.can_find {
                out.push(Row::choice(Pick::Find, t!("player.subtitles.find"), false).icon(Icon::Search));
            }
            let set = i.settings;
            out.push(Row::Heading(t!("player.subtitles.size").into()));
            out.extend(
                SubtitleSize::ALL.iter().map(|v| Row::choice(Pick::Size(*v), v.label(), *v == set.subtitle_size)),
            );
            out.push(Row::Heading(t!("player.subtitles.background").into()));
            out.extend(
                SubtitleBackground::ALL
                    .iter()
                    .map(|v| Row::choice(Pick::Background(*v), v.label(), *v == set.subtitle_background)),
            );
            out.push(Row::Heading(t!("player.subtitles.font").into()));
            out.extend(i.fonts.iter().map(|v| Row::choice(Pick::Font(*v), v.label(), *v == set.subtitle_font)));
            out.push(Row::Heading(t!("player.subtitles.position").into()));
            out.extend(
                SubtitlePosition::ALL
                    .iter()
                    .map(|v| Row::choice(Pick::Position(*v), v.label(), *v == set.subtitle_position)),
            );
        }
        Panel::Find => {
            out.push(Row::Title(t!("player.subtitles.find").into()));
            if let Some(why) = i.add_failed {
                out.push(Row::Note(why.into()));
            }
            match &s.search {
                SubtitleSearch::Idle | SubtitleSearch::Searching => {
                    out.push(Row::Note(t!("player.find.searching").into()));
                }
                SubtitleSearch::Failed(e) if i.add_failed.is_none() => out.push(Row::Note(text::search_problem(e))),
                SubtitleSearch::Found(r) if r.is_empty() => {
                    out.push(Row::Note(t!("player.find.nothing").into()));
                }
                _ => {
                    let adding = match &s.search {
                        SubtitleSearch::Adding(id) => Some(id.as_str()),
                        _ => None,
                    };
                    for (n, r) in i.found.iter().enumerate() {
                        let mine = adding == Some(r.id.as_str());
                        let hint = if mine { t!("player.find.adding").to_string() } else { text::result_hint(r) };
                        let row = Row::Choice {
                            pick: Pick::Result(n),
                            label: text::result_label(r),
                            hint,
                            checked: false,
                            icon: None,
                            enabled: adding.is_none() || mine,
                        };
                        out.push(row);
                    }
                }
            }
        }
        Panel::Audio => {
            out.push(Row::Title(t!("player.controls.audio").into()));
            for t in &s.audio {
                if let Some(index) = t.index {
                    out.push(Row::choice(Pick::Audio(index), &t.label, t.selected));
                }
            }
        }
        Panel::Quality => {
            out.push(Row::Title(t!("player.controls.quality").into()));
            if let Some(note) = &s.note {
                out.push(Row::Note(note.clone()));
            }
            if s.can_play_original {
                out.push(
                    Row::choice(Pick::PlayOriginal, t!("player.quality.playOriginal"), false)
                        .hint(t!("player.quality.tryOriginal"))
                        .icon(Icon::RotateCcw),
                );
            }
            for q in &s.qualities {
                out.push(
                    Row::choice(Pick::Quality(q.id), &q.label, q.selected).hint(q.hint.clone().unwrap_or_default()),
                );
            }
        }
        Panel::Sleep => {
            out.push(Row::Title(t!("player.sleep.title").into()));
            for c in SLEEP_CHOICES {
                let on = c == s.sleep.choice;
                let mut r = Row::choice(Pick::Sleep(c), sleep_label(c, s.kind), on);
                if let (true, SleepChoice::Minutes(_), Some(m)) = (on, c, s.sleep.minutes_left) {
                    r = r.hint(t!("player.sleep.minutesLeft", count = m));
                }
                out.push(r);
            }
        }
        Panel::Speed => {
            out.push(Row::Title(t!("player.controls.speed").into()));
            if s.speeds.is_empty() {
                out.push(Row::Note(t!("player.speed.notOffered").into()));
            }
            for rate in &s.speeds {
                out.push(Row::choice(Pick::Speed(*rate), text::speed(*rate), *rate == s.speed));
            }
        }
        Panel::Info => {}
    }
    out
}

/// The row focus starts on: the choice in force, else Find subtitles… when there are no tracks,
/// else the first that takes focus.
pub fn initial(rows: &[Row]) -> usize {
    let first_track = rows.iter().position(|r| matches!(r, Row::Choice { pick: Pick::Subtitle(_), .. }));
    let find = rows.iter().position(|r| matches!(r, Row::Choice { pick: Pick::Find, .. }));
    if let (None, Some(f)) = (first_track, find) {
        return f;
    }
    // The track or the choice in force, before the subtitle look's choices further down.
    let checked = rows.iter().position(|r| {
        r.checked()
            && !matches!(
                r,
                Row::Choice { pick: Pick::Size(_) | Pick::Background(_) | Pick::Font(_) | Pick::Position(_), .. }
            )
    });
    checked.or_else(|| rows.iter().position(Row::focusable)).unwrap_or(0)
}

/// What the rows answered this frame.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PanelAnswer {
    pub pick: Option<Pick>,
    /// Sync's steps, or `Some(0)` to put it back.
    pub sync: Option<i32>,
    pub sync_reset: bool,
}

fn row_height(ui: &Ui, row: &Row, width: f32) -> f32 {
    match row {
        Row::Title(t) => Heading::PanelTitle.height(ui, t, width),
        Row::Heading(t) => Heading::Section.height(ui, t, width),
        Row::Note(t) => Heading::Note.height(ui, t, width),
        Row::Sync(_) => (theme::LABEL.line_height + theme::CAPTION.line_height + 16.0).max(theme::LIST_ROW_HEIGHT),
        Row::Choice { label, hint, icon, checked, .. } => list_row(label, hint, *icon, *checked).height(ui, width),
    }
}

fn list_row<'a>(label: &'a str, hint: &'a str, icon: Option<Icon>, checked: bool) -> ListRow<'a> {
    let mut r = ListRow::new(label).supporting(hint).trailing(Trailing::Check(checked));
    if let Some(i) = icon {
        r = r.icon(i);
    }
    r
}

/// Draws `rows` in the panel's content `area`, scrolled by `state`; `list` names the list.
pub fn draw(ui: &mut Ui, list: Id, area: Rect, rows: &[Row], state: &mut ListState) -> PanelAnswer {
    let mut answer = PanelAnswer::default();
    if let Some(i) = rows.iter().position(Row::focusable).map(|_| state.index.min(rows.len() - 1)) {
        ui.initial_focus(list.with(i as u64));
    }
    List::new(area).show(
        ui,
        list,
        state,
        rows.len(),
        |ui, i| row_height(ui, &rows[i], area.w),
        |ui, slot| {
            let at = Point::new(slot.rect.x, slot.rect.y);
            let w = slot.rect.w;
            match &rows[slot.index] {
                Row::Title(t) => {
                    Heading::PanelTitle.show(ui, t, at, w);
                }
                Row::Heading(t) => {
                    Heading::Section.show(ui, t, at, w);
                }
                Row::Note(t) => {
                    Heading::Note.show(ui, t, at, w);
                }
                Row::Sync(ms) => {
                    let value = format_offset(i64::from(*ms));
                    let r = Stepper::new(t!("player.subtitles.sync"), &value)
                        .hint(t!("player.subtitles.syncHint"))
                        .limits(*ms > -SYNC_MAX_MS, *ms < SYNC_MAX_MS)
                        .show(ui, slot.id, at, w);
                    if r.step != 0 {
                        answer.sync = Some(r.step);
                    }
                    answer.sync_reset |= r.reset;
                }
                Row::Choice { pick, label, hint, checked, icon, enabled } => {
                    let r = list_row(label, hint, *icon, *checked).enabled(*enabled).show(ui, slot.id, at, w);
                    if r.activated {
                        answer.pick = Some(pick.clone());
                    }
                }
            }
        },
    );
    answer
}

/// Info's sections (`readoutSections`): how it plays, the file, its video and audio, the
/// subtitles, and what this TV says.
pub fn info_sections(s: &PlayerState, stats: &EngineStats) -> Vec<FactBlock> {
    let Some(d) = &s.details else { return Vec::new() };
    let fact = |label: &str, value: String| Fact { label: label.into(), value };
    let mut out = Vec::new();
    let playback = match &d.pipeline {
        Some(p) => format!("{} · {p}", d.method),
        None => d.method.clone(),
    };
    let mut how = vec![fact(t!("player.info.playback"), playback)];
    if let Some(why) = &d.why {
        how.push(fact(t!("player.info.why"), why.clone()));
    }
    how.extend(d.rows.iter().map(|r| fact(&r.label, r.value.clone())));
    // How fast the TV's network receives, beside the server's Speed: a server keeping up whose
    // video stalls is a network that can't carry it (the Samsung app's row; Android's Network is
    // ExoPlayer's own estimate).
    if let Some(bps) = stats.network_bps.filter(|b| *b > 0) {
        how.push(fact(t!("player.info.network"), format_bitrate(Some(i64::try_from(bps).unwrap_or(i64::MAX)))));
    }
    how.push(fact(t!("player.info.quality"), s.quality.clone()));
    out.push(FactBlock { title: t!("player.info.howItPlays").into(), facts: how });
    if let Some(file) = &d.file
        && let Some(block) = media_facts(file).into_iter().next()
    {
        out.push(block);
    }
    if let Some(v) = &d.video {
        out.push(stream_block(v, t!("player.details.video")));
    }
    if let Some(a) = &d.audio {
        out.push(stream_block(a, t!("player.details.audio")));
    }
    // The stream's facts as Video and Audio give theirs (Format, Language by name, Title,
    // Flags), then how it is shown (`TvReadout.subtitles`).
    match &d.subtitle {
        None => out.push(FactBlock {
            title: t!("player.details.subtitles").into(),
            facts: vec![fact(t!("player.info.track"), t!("player.info.off").into())],
        }),
        Some(track) => {
            let mut block = stream_block(track, t!("player.details.subtitles"));
            let shown = d.subtitle_delivery.unwrap_or(SubtitleDelivery::ServerText).label();
            block.facts.push(fact(t!("player.info.shown"), shown.into()));
            out.push(block);
        }
    }
    let mut tv = vec![fact(t!("player.info.player"), stats.player.clone())];
    let mut decoded = Vec::new();
    if let (Some(w), Some(h)) = (stats.width, stats.height) {
        decoded.push(format!("{w}×{h}"));
    }
    if let Some(fps) = stats.fps.filter(|f| *f > 0.0) {
        decoded.push(t!("player.info.fps", fps = text::short_decimal(f64::from(fps), 3)));
    }
    if let Some(r) = &stats.dynamic_range {
        decoded.push(r.clone());
    }
    if let Some(c) = &stats.video_codec {
        decoded.push(codec_label(c));
    }
    if !decoded.is_empty() {
        tv.push(fact(t!("player.info.decoded"), decoded.join(" · ")));
    }
    if let Some(c) = &stats.audio_codec {
        let ch = channels_label(stats.audio_channels.map(i64::from), None);
        tv.push(fact(
            t!("player.details.audio"),
            [codec_label(c), ch].iter().filter(|s| !s.is_empty()).cloned().collect::<Vec<_>>().join(" "),
        ));
    }
    if let Some(kbps) = stats.bitrate_kbps.filter(|k| *k > 0) {
        tv.push(fact(t!("player.info.currentBitrate"), format_bitrate(Some(i64::from(kbps) * 1000))));
    }
    if let (Some(dropped), Some(total)) = (stats.dropped_frames, stats.total_frames.filter(|t| *t > 0)) {
        tv.push(fact(t!("player.info.droppedFrames"), t!("player.info.framesOf", dropped = dropped, frames = total)));
    }
    if let Some(ms) = stats.buffered_ahead_ms {
        tv.push(fact(t!("player.info.bufferedAhead"), t!("player.info.seconds", seconds = (ms + 500) / 1000)));
    }
    tv.push(fact(t!("player.info.stalls"), s.stalls.to_string()));
    tv.extend(stats.extra.iter().map(|(l, v)| fact(l, v.clone())));
    if let Some(ms) = s.first_frame_ms {
        tv.push(fact(t!("player.info.firstFrame"), t!("player.info.milliseconds", ms = ms)));
    }
    out.push(FactBlock { title: t!("player.info.thisTv").into(), facts: tv });
    out
}

/// The compact readout's lines (`CompactReadout`): how it plays, the details' rows, the
/// subtitles, the bitrate the file has and what arrives now, the decode, dropped frames and
/// the buffer.
pub fn readout_lines(s: &PlayerState, stats: &EngineStats) -> Vec<(String, String)> {
    let Some(d) = &s.details else { return Vec::new() };
    let mut out = vec![(
        t!("player.info.playback").to_string(),
        match &d.pipeline {
            Some(p) => format!("{} · {p}", d.method),
            None => d.method.clone(),
        },
    )];
    // The rows' own Bitrate gives way to the one line below, the file's and what arrives now
    // (`TvReadout.compact` has a single Bitrate line).
    let bitrate = t!("player.details.bitrate");
    out.extend(d.rows.iter().filter(|r| r.label != bitrate).map(|r| (r.label.clone(), r.value.clone())));
    let sub = match &d.subtitle {
        Some(t) => {
            let lang = t.language.clone().unwrap_or_default();
            [lang, codec_label(&t.codec)].iter().filter(|x| !x.is_empty()).cloned().collect::<Vec<_>>().join(" · ")
        }
        None => t!("player.info.off").into(),
    };
    out.push((t!("player.details.subtitles").into(), sub));
    let file = d.file.as_ref().and_then(|f| f.bitrate).filter(|b| *b > 0).map(|b| format_bitrate(Some(b)));
    let now = stats.bitrate_kbps.filter(|k| *k > 0).map(|k| format_bitrate(Some(i64::from(k) * 1000)));
    let mut rates: Vec<String> = file.into_iter().chain(now).collect();
    rates.dedup();
    if !rates.is_empty() {
        out.push((bitrate.into(), rates.join(" → ")));
    }
    if let (Some(w), Some(h)) = (stats.width, stats.height) {
        let range = stats.dynamic_range.as_ref().map(|r| format!(" · {r}")).unwrap_or_default();
        out.push((t!("player.info.decoded").into(), format!("{w}×{h}{range}")));
    }
    if let (Some(dropped), Some(total)) = (stats.dropped_frames, stats.total_frames.filter(|t| *t > 0)) {
        out.push((t!("player.info.dropped").into(), t!("player.info.framesOf", dropped = dropped, frames = total)));
    }
    if let Some(ms) = stats.buffered_ahead_ms {
        out.push((t!("player.info.buffer").into(), t!("player.info.secondsAhead", seconds = (ms + 500) / 1000)));
    }
    out
}

/// How far a press of Up or Down moves Info (`TvReadout.kt`).
pub const INFO_STEP: f32 = 96.0;
const FACT_LABEL_W: f32 = 112.0;

/// Info: "Playback details" and the sections, in one element that takes the keys, Up and Down
/// moving it 96 dp a press; "Asking the server…" until the details come.
pub fn info(ui: &mut Ui, id: Id, area: Rect, sections: &[FactBlock], scroll: &mut f32) {
    let r = register(ui, id, area, FocusOpts::default().takes(Dirs::ALL));
    ui.initial_focus(id);
    let content = info_height(ui, sections, area.w);
    let max = (content - area.h).max(0.0);
    if r.focused {
        for k in ui.keys() {
            if !k.pressed {
                continue;
            }
            match k.key {
                Key::Down => *scroll = (*scroll + INFO_STEP).min(max),
                Key::Up => *scroll = (*scroll - INFO_STEP).max(0.0),
                _ => {}
            }
        }
    }
    *scroll = scroll.clamp(0.0, max);
    let shown = ui.animate(id.with("scroll"), *scroll, pley_kit::SCROLL);
    ui.clip(area, |ui| {
        let mut y = area.y - shown;
        y += Heading::PanelTitle.show(ui, t!("player.details.title"), Point::new(area.x, y), area.w);
        if sections.is_empty() {
            Heading::Note.show(ui, t!("player.details.asking"), Point::new(area.x, y), area.w);
            return;
        }
        for s in sections {
            y += Heading::Section.show(ui, &s.title, Point::new(area.x, y), area.w);
            for f in &s.facts {
                y += fact_line(ui, f, Point::new(area.x + 16.0, y), area.w - 32.0) + 4.0;
            }
        }
    });
}

fn info_height(ui: &Ui, sections: &[FactBlock], width: f32) -> f32 {
    let mut h = Heading::PanelTitle.height(ui, t!("player.details.title"), width);
    for s in sections {
        h += Heading::Section.height(ui, &s.title, width);
        for f in &s.facts {
            h += fact_height(ui, f, width - 32.0) + 4.0;
        }
    }
    h + 24.0
}

/// A fact's label, up to two lines: French and Spanish labels can outgrow the column.
fn label_opts() -> TextOpts {
    TextOpts::wrap(FACT_LABEL_W - 8.0, 2)
}

fn value_opts(width: f32) -> TextOpts {
    TextOpts::wrap((width - FACT_LABEL_W).max(1.0), 3)
}

fn fact_height(ui: &Ui, f: &Fact, width: f32) -> f32 {
    let label = ui.measure(&f.label, &theme::CAPTION, &label_opts()).h;
    let value = ui.measure(&f.value, &theme::CAPTION, &value_opts(width)).h;
    label.max(value).max(theme::CAPTION.line_height)
}

/// A fact: its label in Muted in a 112 dp column, up to two lines, its value in Fg, up to three.
fn fact_line(ui: &mut Ui, f: &Fact, at: Point, width: f32) -> f32 {
    let label = ui.text(at, &f.label, &theme::CAPTION, &label_opts(), theme::MUTED);
    let value =
        ui.text(Point::new(at.x + FACT_LABEL_W, at.y), &f.value, &theme::CAPTION, &value_opts(width), theme::FG);
    label.h.max(value.h).max(theme::CAPTION.line_height)
}

#[cfg(test)]
mod tests {
    use pley_player::controller::{QualityChoice, TrackChoice};

    use super::*;

    fn input<'a>(state: &'a PlayerState, settings: &'a Settings, fonts: &'a [SubtitleFont]) -> PanelInput<'a> {
        PanelInput { state, settings, fonts, can_find: true, found: &[], add_failed: None }
    }

    fn track(index: Option<i64>, label: &str, selected: bool) -> TrackChoice {
        TrackChoice { index, label: label.into(), selected, burn_in: false, added: false }
    }

    #[test]
    fn subtitles_offer_sync_tracks_search_and_the_look() {
        let mut s = PlayerState {
            subtitle_sync: true,
            subtitle_offset_ms: 500,
            subtitles: vec![track(None, "Off", false), track(Some(2), "English (SRT)", true)],
            ..PlayerState::default()
        };
        let settings = Settings::default();
        let fonts = [SubtitleFont::Sans, SubtitleFont::Serif, SubtitleFont::Monospace];
        let r = rows(Panel::Subtitles, &input(&s, &settings, &fonts));
        assert_eq!(r[0], Row::Title("Subtitles".into()));
        assert_eq!(r[1], Row::Sync(500));
        assert!(matches!(&r[3], Row::Choice { pick: Pick::Subtitle(Some(2)), checked: true, .. }));
        assert!(matches!(&r[4], Row::Choice { pick: Pick::Find, icon: Some(Icon::Search), .. }));
        assert_eq!(r[5], Row::Heading("Subtitle size".into()));
        let labels: Vec<String> = r
            .iter()
            .filter_map(|r| match r {
                Row::Choice { pick: Pick::Font(_), label, .. } => Some(label.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(labels, ["Sans serif", "Serif", "Monospace"]);
        // Focus starts on the track playing, not on the look's choices.
        assert_eq!(initial(&r), 3);
        // Without tracks, on Find subtitles….
        s.subtitles.clear();
        s.subtitle_sync = false;
        let r = rows(Panel::Subtitles, &input(&s, &settings, &fonts));
        assert!(matches!(&r[initial(&r)], Row::Choice { pick: Pick::Find, .. }));
    }

    #[test]
    fn quality_says_its_note_and_offers_the_original_again() {
        let mut s = PlayerState {
            note: Some("Lowered for your connection".into()),
            can_play_original: true,
            ..PlayerState::default()
        };
        s.qualities = vec![
            QualityChoice {
                id: "auto",
                label: "Automatic".into(),
                hint: Some("Dynamically adjust".into()),
                selected: true,
            },
            QualityChoice { id: "original", label: "Original".into(), hint: None, selected: false },
        ];
        let settings = Settings::default();
        let r = rows(Panel::Quality, &input(&s, &settings, &[]));
        assert_eq!(r[1], Row::Note("Lowered for your connection".into()));
        assert!(
            matches!(&r[2], Row::Choice { pick: Pick::PlayOriginal, hint, .. } if hint == "Try the original again")
        );
        assert_eq!(initial(&r), 3);
    }

    #[test]
    fn sleep_and_speed_rows() {
        let mut s = PlayerState { kind: Some(pley_core::model::ItemKind::Episode), ..PlayerState::default() };
        s.sleep.choice = SleepChoice::Minutes(45);
        s.sleep.minutes_left = Some(42);
        s.speeds = vec![0.5, 0.75, 1.0, 1.25, 1.5, 2.0];
        let settings = Settings::default();
        let r = rows(Panel::Sleep, &input(&s, &settings, &[]));
        let names: Vec<String> = r
            .iter()
            .filter_map(|r| match r {
                Row::Choice { label, .. } => Some(label.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(names, ["Off", "15 min", "30 min", "45 min", "60 min", "90 min", "End of this episode"]);
        assert!(matches!(&r[4], Row::Choice { hint, checked: true, .. } if hint == "42 min left"));
        let r = rows(Panel::Speed, &input(&s, &settings, &[]));
        assert!(matches!(&r[3], Row::Choice { label, checked: true, .. } if label == "1×"));

        // In Spanish: the minutes left counted, the end named for an episode, speeds with a comma.
        pley_i18n::with_language(pley_i18n::Language::Es, || {
            let r = rows(Panel::Sleep, &input(&s, &settings, &[]));
            assert_eq!(r[0], Row::Title("Temporizador de suspensión".into()));
            assert!(matches!(&r[4], Row::Choice { hint, checked: true, .. } if hint == "Quedan 42 min"));
            assert!(matches!(&r[7], Row::Choice { label, .. } if label == "Final de este episodio"));
            s.sleep.minutes_left = Some(1);
            let r = rows(Panel::Sleep, &input(&s, &settings, &[]));
            assert!(matches!(&r[4], Row::Choice { hint, .. } if hint == "Queda 1 min"));
            let r = rows(Panel::Speed, &input(&s, &settings, &[]));
            assert!(matches!(&r[5], Row::Choice { label, .. } if label == "1,5×"));
        });
    }

    #[test]
    fn find_says_where_the_search_stands() {
        let mut s = PlayerState::default();
        let settings = Settings::default();
        s.search = SubtitleSearch::Searching;
        assert_eq!(rows(Panel::Find, &input(&s, &settings, &[]))[1], Row::Note("Searching…".into()));
        s.search = SubtitleSearch::Found(Vec::new());
        assert_eq!(
            rows(Panel::Find, &input(&s, &settings, &[]))[1],
            Row::Note("No subtitles found for this file".into())
        );
        let found = vec![
            SubtitleResult { id: "a".into(), name: "One".into(), downloads: 3, ..SubtitleResult::default() },
            SubtitleResult { id: "b".into(), name: "Two".into(), ..SubtitleResult::default() },
        ];
        s.search = SubtitleSearch::Adding("a".into());
        let i = PanelInput { found: &found, ..input(&s, &settings, &[]) };
        let r = rows(Panel::Find, &i);
        assert!(matches!(&r[1], Row::Choice { hint, enabled: true, .. } if hint == "Adding…"));
        assert!(matches!(&r[2], Row::Choice { enabled: false, .. }));
    }
}
