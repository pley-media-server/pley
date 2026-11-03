//! Diagnostics (`TvDiagnosticsScreen.kt`; the TV app's `DiagnosticsScreen`): one column from x 96
//! that scrolls with focus. "This TV" (the app and the TV's facts), "What it plays" (the
//! capability report's lines, one each), then, for LG, "Network" (the TV's address, the server,
//! whether it answers) and "Log" (the log's last lines). Every line takes focus, so the remote
//! scrolls through them; nothing acts but "Check again" after a check that failed.

use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use pley_base::{Icon, Point, Rect, TextOpts};
use pley_core::Promise;
use pley_kit::look::{focus_progress, mix};
use pley_kit::{Button, List, ListSlot, ListState};
use pley_shell::{AppCx, AppEvent, Chrome, Screen};
use pley_ui::theme::{self, radius};
use pley_ui::{Edge, Id, Ui};

use crate::pane;
use crate::settings::device_facts;

/// The TV's log file (pley-webos's `log::LOG_DIR`).
pub const TV_LOG: &str = "/tmp/app.pley.tv/pley.log";
/// The log's lines shown, newest last.
pub const LOG_LINES: usize = 40;
/// How much of the log's end is read for them.
const LOG_TAIL_BYTES: u64 = 16 * 1024;
/// A check of the TV that gives no answer in this time has failed.
const CHECK_TIMEOUT: f64 = 20.0;
/// A line's label column (`Line` in `TvDiagnosticsScreen.kt`).
const LABEL_W: f32 = 180.0;
/// The column: x 96 to 912.
const WIDTH: f32 = theme::CANVAS_W - theme::GUTTER - theme::SAFE_X;

/// The last `max` lines of the first log of `paths` that reads, without the first line cut in
/// half; `None` when there is none.
pub fn log_tail(paths: &[PathBuf], max: usize) -> Option<Vec<String>> {
    paths.iter().find_map(|path| tail(path, max))
}

fn tail(path: &Path, max: usize) -> Option<Vec<String>> {
    let mut file = std::fs::File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    let from = len.saturating_sub(LOG_TAIL_BYTES);
    file.seek(SeekFrom::Start(from)).ok()?;
    let mut bytes = Vec::new();
    file.take(LOG_TAIL_BYTES).read_to_end(&mut bytes).ok()?;
    let text = String::from_utf8_lossy(&bytes);
    let mut lines: Vec<&str> = text.lines().collect();
    if from > 0 && !lines.is_empty() {
        lines.remove(0);
    }
    let lines: Vec<String> = lines.iter().map(|l| l.trim_end()).filter(|l| !l.is_empty()).map(str::to_string).collect();
    let skip = lines.len().saturating_sub(max);
    Some(lines.into_iter().skip(skip).collect())
}

/// Where the log may be: the TV's, else the simulator's (`logs/pley.log` beside the data folder).
pub fn log_paths(data_dir: &Path) -> Vec<PathBuf> {
    let mut out = vec![PathBuf::from(TV_LOG)];
    if let Some(parent) = data_dir.parent() {
        out.push(parent.join("logs").join("pley.log"));
    }
    out.push(data_dir.join("logs").join("pley.log"));
    out
}

/// One line of the page.
#[derive(Clone, Debug, PartialEq)]
pub enum Line {
    /// "Diagnostics", under the safe area.
    Title,
    /// A section's heading; the first has no space above.
    Heading(&'static str, bool),
    /// A fact: its label and value.
    Fact(String, String),
    /// A line of what the TV plays.
    Plays(String),
    /// A line of the log.
    Log(String),
    /// A note in Muted, or DangerText.
    Note(&'static str, bool),
    /// "Check again".
    Again,
    /// Room under the last line.
    End,
}

impl Line {
    /// Whether it takes focus.
    pub fn focusable(&self) -> bool {
        matches!(self, Line::Fact(..) | Line::Plays(_) | Line::Log(_) | Line::Again)
    }

    fn height(&self, ui: &Ui) -> f32 {
        match self {
            Line::Title => theme::SAFE_Y + theme::PAGE_TITLE.line_height + 20.0,
            Line::Heading(_, first) => (if *first { 0.0 } else { 20.0 }) + theme::SECTION.line_height + 8.0,
            Line::Fact(_, value) => {
                let h = ui.measure(value, &theme::BODY, &TextOpts::wrap(WIDTH - 32.0 - LABEL_W, 0)).h;
                (h + 16.0).max(36.0)
            }
            Line::Plays(value) => {
                (ui.measure(value, &theme::BODY, &TextOpts::wrap(WIDTH - 32.0, 0)).h + 16.0).max(36.0)
            }
            Line::Log(_) => theme::CAPTION.line_height + 10.0,
            Line::Note(..) => theme::BODY.line_height,
            Line::Again => 12.0 + theme::BUTTON_HEIGHT,
            Line::End => 160.0,
        }
    }
}

/// How the check of what the TV plays stands.
#[derive(Clone, Debug, PartialEq)]
pub enum Check {
    /// Asked, no answer yet.
    Checking,
    /// Asked again after a failure: "Check again" keeps focus meanwhile.
    Rechecking,
    Failed,
    Lines(Vec<String>),
}

/// The page's lines.
pub fn lines(
    facts: &[(String, String)],
    check: &Check,
    network: &[(String, String)],
    log: Option<&[String]>,
) -> Vec<Line> {
    let mut out = vec![Line::Title, Line::Heading("This TV", true)];
    out.extend(facts.iter().map(|(l, v)| Line::Fact(l.clone(), v.clone())));
    out.push(Line::Heading("What it plays", false));
    match check {
        Check::Checking => out.push(Line::Note("Checking what this TV plays…", false)),
        Check::Rechecking => out.extend([Line::Note("Checking what this TV plays…", false), Line::Again]),
        Check::Failed => out.extend([Line::Note("The check of this TV failed.", true), Line::Again]),
        Check::Lines(lines) => out.extend(lines.iter().cloned().map(Line::Plays)),
    }
    out.push(Line::Heading("Network", false));
    out.extend(network.iter().map(|(l, v)| Line::Fact(l.clone(), v.clone())));
    out.push(Line::Heading("Log", false));
    match log {
        None => out.push(Line::Note("Reading the log…", false)),
        Some([]) => out.push(Line::Note("This TV keeps no log.", false)),
        Some(lines) => out.extend(lines.iter().cloned().map(Line::Log)),
    }
    out.push(Line::End);
    out
}

/// What names line `i` wherever it is: its section (the headings before it) and its place in it.
/// Focus stays on the line so named when lines come or go above it.
pub fn line_key(lines: &[Line], i: usize) -> (usize, usize) {
    let heading = lines[..=i.min(lines.len().saturating_sub(1))]
        .iter()
        .rposition(|l| matches!(l, Line::Heading(..)))
        .unwrap_or(0);
    let section = lines[..heading].iter().filter(|l| matches!(l, Line::Heading(..))).count();
    (section, i - heading)
}

/// How the check stands, told apart without copying its lines.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stand {
    Checking,
    Rechecking,
    Failed,
    /// The report's lines, by count.
    Done(usize),
}

/// The Diagnostics page.
#[derive(Default)]
pub struct DiagnosticsScreen {
    list: ListState,
    /// When the check was last asked for.
    asked: Option<f64>,
    failed_once: bool,
    /// The check was asked again: the first line it finds takes focus.
    again: bool,
    reading: Option<Promise<Option<Vec<String>>>>,
    log: Option<Vec<String>>,
    /// The app's and the TV's facts, read when the page first shows and when it shows again.
    facts: Option<Vec<(String, String)>>,
    /// The network's facts, and whether the server was out of reach when they were made.
    network: Option<(Vec<(String, String)>, bool)>,
    /// The page's lines and their heights, made again when what they show changes.
    lines: Vec<Line>,
    heights: Vec<f32>,
    /// How the check stood when `lines` were made; `None` when they are to be made again.
    made_for: Option<Stand>,
}

impl DiagnosticsScreen {
    /// The page, checking the TV if no check has run yet.
    pub fn new() -> DiagnosticsScreen {
        DiagnosticsScreen::default()
    }

    fn read_log(&mut self, cx: &AppCx) {
        let paths = log_paths(&cx.device().data_dir());
        self.reading = Some(cx.tasks().spawn(move || log_tail(&paths, LOG_LINES)));
    }

    fn stand(&mut self, cx: &AppCx, now: f64) -> Stand {
        let report = cx.state.caps();
        let late = report.is_none() && self.asked.is_some_and(|t| now - t > CHECK_TIMEOUT);
        match report {
            Some(r) if !r.lines.is_empty() => Stand::Done(r.lines.len()),
            Some(_) => {
                self.failed_once = true;
                Stand::Failed
            }
            None if late => {
                self.failed_once = true;
                Stand::Failed
            }
            None if self.failed_once => Stand::Rechecking,
            None => Stand::Checking,
        }
    }

    /// The app's and the TV's facts: "Pley 0.2.0 (1a2b3c4d5)", the model, the system.
    fn read_facts(cx: &AppCx) -> Vec<(String, String)> {
        let info = cx.device().info();
        let mut facts =
            vec![("App".to_string(), format!("Pley {}", pane::build_label(&info.app_version, pane::COMMIT)))];
        facts.extend(device_facts(&info, cx.device().tier()));
        facts
    }

    /// The TV's address, the server, and whether it answers.
    fn read_network(cx: &AppCx) -> Vec<(String, String)> {
        let server = cx.state.server().map(pley_core::addresses::host_port);
        let connection = match (&server, cx.state.offline()) {
            (None, _) => "No server",
            (Some(_), true) => "Can't reach the server",
            (Some(_), false) => "Connected",
        };
        vec![
            (
                "Address".to_string(),
                cx.device().local_ipv4().map_or_else(|| "No network".to_string(), |ip| ip.to_string()),
            ),
            ("Server".to_string(), server.unwrap_or_else(|| "None".into())),
            ("Connection".to_string(), connection.to_string()),
        ]
    }

    /// Makes the lines again from what they show, keeping focus on the line it was on.
    fn make_lines(&mut self, ui: &mut Ui, cx: &AppCx, stand: Stand, id: Id) {
        let check = match stand {
            Stand::Checking => Check::Checking,
            Stand::Rechecking => Check::Rechecking,
            Stand::Failed => Check::Failed,
            Stand::Done(_) => Check::Lines(cx.state.caps().map(|r| r.lines.clone()).unwrap_or_default()),
        };
        let facts = self.facts.as_deref().unwrap_or_default();
        let network = self.network.as_ref().map_or(&[][..], |(n, _)| n.as_slice());
        let lines = lines(facts, &check, network, self.log.as_deref());
        let at = self.list.index;
        let key = (ui.focused_in(id).is_some() && at < self.lines.len()).then(|| line_key(&self.lines, at));
        if let Some(key) = key
            && let Some(now) = (0..lines.len()).find(|&j| lines[j].focusable() && line_key(&lines, j) == key)
            && now != at
        {
            self.list.index = now;
            ui.request_focus(id.with(now as u64));
        }
        self.heights = lines.iter().map(|l| l.height(ui)).collect();
        self.lines = lines;
        self.made_for = Some(stand);
    }
}

impl Screen for DiagnosticsScreen {
    fn ui(&mut self, ui: &mut Ui, cx: &mut AppCx) {
        let now = ui.time();
        if self.asked.is_none() {
            self.asked = Some(now);
            cx.state.probe_caps(false);
            self.read_log(cx);
        }
        if let Some(p) = self.reading.as_mut()
            && (p.ready() || p.failed())
        {
            self.log = Some(p.take().flatten().unwrap_or_default());
            self.reading = None;
            self.made_for = None;
        }
        let stand = self.stand(cx, now);
        if matches!(stand, Stand::Checking | Stand::Rechecking) {
            // A check that never answers fails after a while.
            ui.repaint_after(CHECK_TIMEOUT + 0.1 - self.asked.map_or(0.0, |t| now - t));
        }
        // What doesn't change while the page shows is read once, not each frame.
        if self.facts.is_none() {
            self.facts = Some(DiagnosticsScreen::read_facts(cx));
            self.made_for = None;
        }
        let offline = cx.state.offline();
        if self.network.as_ref().is_none_or(|(_, was)| *was != offline) {
            self.network = Some((DiagnosticsScreen::read_network(cx), offline));
            self.made_for = None;
        }
        let root = ui.scope_id();
        let id = root.with("lines");
        if self.made_for != Some(stand) {
            self.make_lines(ui, cx, stand, id);
        }
        let (lines, heights) = (&self.lines, &self.heights);
        let first_fact = lines.iter().position(Line::focusable).unwrap_or(0);
        ui.initial_focus(id.with(first_fact as u64));
        let mut again = false;
        let view = Rect::new(theme::GUTTER, 0.0, WIDTH, theme::CANVAS_H);
        List::new(view).gap(2.0).edges(Edge::Stop, Edge::Stop).show(
            ui,
            id,
            &mut self.list,
            lines.len(),
            |_, i| heights[i],
            |ui, s| again |= draw_line(ui, s, &lines[s.index]),
        );
        // While checking, "Check again" waits for the check under way (the TV app's `!checking`).
        if again && !matches!(stand, Stand::Checking | Stand::Rechecking) {
            cx.state.probe_caps(true);
            self.asked = Some(now);
            self.again = true;
        }
        if self.again && matches!(stand, Stand::Done(_)) {
            self.again = false;
            if let Some(first) = self.lines.iter().position(|l| matches!(l, Line::Plays(_))) {
                ui.request_focus(id.with(first as u64));
            }
        }
    }

    fn event(&mut self, event: &AppEvent, cx: &mut AppCx) {
        match event {
            AppEvent::Shown => {
                // On top again: the facts, the network and the log are read anew.
                self.facts = None;
                self.network = None;
                if self.reading.is_none() {
                    self.read_log(cx);
                }
            }
            // The TV's network changed, perhaps: its address is read again.
            AppEvent::Device(_) => self.network = None,
            _ => {}
        }
    }

    fn chrome(&self) -> Chrome {
        Chrome::Rail
    }
}

/// Draws a line in its slot; whether "Check again" was chosen.
fn draw_line(ui: &mut Ui, s: &ListSlot, line: &Line) -> bool {
    let r = s.rect;
    match line {
        Line::Title => {
            ui.text(
                Point::new(r.x, r.y + theme::SAFE_Y),
                "Diagnostics",
                &theme::PAGE_TITLE,
                &TextOpts::line(r.w),
                theme::FG,
            );
        }
        Line::Heading(text, first) => {
            let y = r.y + if *first { 0.0 } else { 20.0 };
            ui.text(Point::new(r.x, y), text, &theme::SECTION, &TextOpts::line(r.w), theme::FG);
        }
        Line::Note(text, danger) => {
            let color = if *danger { theme::DANGER_TEXT } else { theme::MUTED };
            ui.text(Point::new(r.x + 16.0, r.y), text, &theme::BODY, &TextOpts::line(r.w - 16.0), color);
        }
        Line::Again => {
            let at = Point::new(r.x, r.y + 12.0);
            return Button::pill("Check again").icon(Icon::RefreshCw).show(ui, s.id, at).activated;
        }
        Line::End => {}
        Line::Fact(label, value) => {
            let p = focused_fill(ui, s);
            let y = r.y + 8.0;
            let label_color = mix(theme::MUTED, theme::ON_ACCENT, p);
            ui.text(Point::new(r.x + 16.0, y), label, &theme::BODY, &TextOpts::line(LABEL_W - 8.0), label_color);
            let value_color = mix(theme::FG, theme::ON_ACCENT, p);
            let opts = TextOpts::wrap(r.w - 32.0 - LABEL_W, 0);
            ui.text(Point::new(r.x + 16.0 + LABEL_W, y), value, &theme::BODY, &opts, value_color);
        }
        Line::Plays(value) => {
            let p = focused_fill(ui, s);
            let color = mix(theme::FG, theme::ON_ACCENT, p);
            ui.text(Point::new(r.x + 16.0, r.y + 8.0), value, &theme::BODY, &TextOpts::wrap(r.w - 32.0, 0), color);
        }
        Line::Log(text) => {
            let p = focused_fill(ui, s);
            let color = mix(theme::MUTED, theme::ON_ACCENT, p);
            ui.text(Point::new(r.x + 16.0, r.y + 5.0), text, &theme::CAPTION, &TextOpts::line(r.w - 32.0), color);
        }
    }
    false
}

/// Registers a line and fills it with Accent while it has focus (the rows' 4 dp corners); how far
/// focus has come.
fn focused_fill(ui: &mut Ui, s: &ListSlot) -> f32 {
    let r = ui.focusable(s.id, s.rect);
    let p = focus_progress(ui, s.id, r.focused);
    if p > 0.0 {
        ui.fill(s.rect, mix(pley_base::Color::TRANSPARENT, theme::ACCENT, p), radius::FIELD);
    }
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("pley-diagnostics-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn the_log_gives_its_last_lines() {
        let dir = temp("short");
        let log = dir.join("pley.log");
        let text: String = (0..100).map(|i| format!("line {i}\n\n")).collect();
        std::fs::write(&log, text).unwrap();
        let lines = log_tail(&[dir.join("missing.log"), log], LOG_LINES).unwrap();
        assert_eq!(lines.len(), LOG_LINES);
        assert_eq!(lines.first().map(String::as_str), Some("line 60"));
        assert_eq!(lines.last().map(String::as_str), Some("line 99"));
        assert_eq!(log_tail(&[dir.join("missing.log")], LOG_LINES), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_long_log_is_read_from_its_end_without_a_cut_line() {
        let dir = temp("long");
        let log = dir.join("pley.log");
        // 100 KiB of 100-byte lines: the read starts inside one, which is left out.
        let text: String = (0..1024).map(|i| format!("{:<99}\n", format!("entry {i}"))).collect();
        std::fs::write(&log, text).unwrap();
        let lines = log_tail(std::slice::from_ref(&log), 1000).unwrap();
        assert!(lines.iter().all(|l| l.starts_with("entry ")), "{:?}", lines.first());
        assert_eq!(lines.last().map(|l| l.trim_end()), Some("entry 1023"));
        assert!(lines.len() < 1024 && lines.len() >= (LOG_TAIL_BYTES / 100) as usize - 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_log_is_looked_for_on_the_tv_then_beside_the_data() {
        let paths = log_paths(Path::new("/sim/data"));
        assert_eq!(paths[0], PathBuf::from(TV_LOG));
        assert_eq!(paths[1], Path::new("/sim").join("logs").join("pley.log"));
    }

    #[test]
    fn the_page_says_how_the_check_stands() {
        let facts = vec![("App".to_string(), "Pley 0.2.0".to_string())];
        let network = vec![("Server".to_string(), "192.168.1.20:7539".to_string())];
        let checking = lines(&facts, &Check::Checking, &network, None);
        assert_eq!(checking[0], Line::Title);
        assert_eq!(checking[1], Line::Heading("This TV", true));
        assert_eq!(checking[2], Line::Fact("App".into(), "Pley 0.2.0".into()));
        assert!(checking.contains(&Line::Note("Checking what this TV plays…", false)));
        assert!(checking.contains(&Line::Note("Reading the log…", false)));
        assert!(!checking.contains(&Line::Again));
        let failed = lines(&facts, &Check::Failed, &network, Some(&[]));
        assert!(failed.contains(&Line::Note("The check of this TV failed.", true)));
        assert!(failed.contains(&Line::Again));
        assert!(failed.contains(&Line::Note("This TV keeps no log.", false)));
        let done = lines(&facts, &Check::Lines(vec!["webOS 22".into()]), &network, Some(&["0.1 INFO".into()]));
        assert!(done.contains(&Line::Plays("webOS 22".into())));
        assert!(done.contains(&Line::Log("0.1 INFO".into())));
        assert_eq!(done.last(), Some(&Line::End));
        let focusable: Vec<bool> = done.iter().map(Line::focusable).collect();
        assert_eq!(focusable.iter().filter(|f| **f).count(), 4);
    }

    #[test]
    fn a_line_keeps_its_name_when_lines_come_above_it() {
        let facts = vec![("App".to_string(), "Pley 0.2.0".to_string())];
        let network = vec![("Address".to_string(), "192.168.1.30".to_string())];
        let checking = lines(&facts, &Check::Checking, &network, None);
        let report = Check::Lines(vec!["TV: OLED65C2PUA".into(), "HEVC Main 10".into(), "AV1".into()]);
        let done = lines(&facts, &report, &network, None);
        let address = |l: &[Line]| l.iter().position(|x| matches!(x, Line::Fact(label, _) if label == "Address"));
        let (before, after) = (address(&checking).unwrap(), address(&done).unwrap());
        assert_eq!(after, before + 2);
        assert_eq!(line_key(&checking, before), line_key(&done, after));
        assert_ne!(line_key(&done, before), line_key(&done, after));
    }
}
