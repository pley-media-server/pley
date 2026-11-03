//! Headless testing for the toolkit, the kit and the screens: a text shaper with fixed advances,
//! a picture cache that answers as a test says, and a harness that runs frames with scripted
//! input and returns each frame's display list and focused element.

use std::collections::HashMap;
use std::sync::Arc;

use pley_base::{
    Align, DisplayList, Glyph, ImageId, ImageProvider, ImageRequest, ImageState, Key, KeyEvent, Point, PointerEvent,
    Priority, Size, TextLayout, TextLine, TextOpts, TextShaper, TextStyle,
};

use crate::id::Id;
use crate::input::{FrameInput, InputEvent};
use crate::ui::{FrameOutput, Ui, UiState};

/// A shaper with predictable results: every character advances half the font size plus the
/// style's tracking; lines are `line_height` apart; wrapping breaks at spaces (a word longer than
/// the line at characters); a cut line ends with "…" (one character).
#[derive(Clone, Copy, Debug, Default)]
pub struct StubShaper;

impl StubShaper {
    /// How far each character advances in `style`.
    pub fn advance(style: &TextStyle) -> f32 {
        style.size * 0.5 + style.tracking
    }
}

impl TextShaper for StubShaper {
    fn layout(&self, text: &str, style: &TextStyle, opts: &TextOpts) -> Arc<TextLayout> {
        let adv = StubShaper::advance(style);
        let max_w = opts.max_width.unwrap_or(f32::INFINITY);
        let fits = |n: usize| n as f32 * adv <= max_w + 1e-3;
        let per_line = if adv > 0.0 { ((max_w + 1e-3) / adv).floor().max(1.0) as usize } else { usize::MAX };

        let mut lines: Vec<Vec<char>> = Vec::new();
        for para in text.split('\n') {
            if opts.max_lines == 1 {
                lines.push(para.chars().collect());
                continue;
            }
            let mut line: Vec<char> = Vec::new();
            for word in para.split(' ') {
                let w: Vec<char> = word.chars().collect();
                let joined = if line.is_empty() { w.len() } else { line.len() + 1 + w.len() };
                if fits(joined) {
                    if !line.is_empty() {
                        line.push(' ');
                    }
                    line.extend(w);
                    continue;
                }
                if !line.is_empty() {
                    lines.push(std::mem::take(&mut line));
                }
                let mut rest = &w[..];
                while !fits(rest.len()) {
                    let n = per_line.min(rest.len());
                    lines.push(rest[..n].to_vec());
                    rest = &rest[n..];
                }
                line.extend_from_slice(rest);
            }
            lines.push(line);
        }

        let max_lines = if opts.max_lines == 0 { usize::MAX } else { opts.max_lines as usize };
        let dropped = lines.len() > max_lines;
        lines.truncate(max_lines);
        let mut truncated = dropped;
        let last = lines.len().saturating_sub(1);
        for (i, line) in lines.iter_mut().enumerate() {
            if fits(line.len()) && !(dropped && i == last) {
                continue;
            }
            truncated = true;
            if opts.ellipsis {
                while !line.is_empty() && !fits(line.len() + 1) {
                    line.pop();
                }
                while line.last() == Some(&' ') {
                    line.pop();
                }
                line.push('…');
            } else {
                while !line.is_empty() && !fits(line.len()) {
                    line.pop();
                }
            }
        }

        let widths: Vec<f32> = lines.iter().map(|l| l.len() as f32 * adv).collect();
        let widest = widths.iter().fold(0.0f32, |a, &b| a.max(b));
        let box_w = match (opts.align, opts.max_width) {
            (Align::Start, _) | (_, None) => widest,
            (_, Some(w)) => w,
        };
        let mut glyphs = Vec::new();
        let mut out = Vec::with_capacity(lines.len());
        for (i, (line, w)) in lines.iter().zip(widths).enumerate() {
            let x = match opts.align {
                Align::Start => 0.0,
                Align::Center => (box_w - w) / 2.0,
                Align::End => box_w - w,
            };
            let top = i as f32 * style.line_height;
            let baseline = top + (style.line_height - style.size) / 2.0 + style.size * 0.8;
            let start = glyphs.len() as u32;
            for (k, &ch) in line.iter().enumerate() {
                glyphs.push(Glyph {
                    font: 0,
                    id: ch as u32 as u16,
                    x: x + k as f32 * adv,
                    y: baseline,
                    size: style.size,
                });
            }
            out.push(TextLine { glyphs: start..glyphs.len() as u32, x, width: w, top, baseline });
        }
        Arc::new(TextLayout {
            text: text.into(),
            size: Size::new(box_w, out.len() as f32 * style.line_height),
            lines: out,
            glyphs,
            truncated,
        })
    }
}

/// A picture cache that answers as the test says (every picture loading until told otherwise),
/// and keeps every request for the test to check.
#[derive(Debug, Default)]
pub struct StubImages {
    states: HashMap<String, ImageState>,
    next: u32,
    requests: Vec<(ImageRequest, Priority)>,
}

impl StubImages {
    /// A cache in which every picture is loading.
    pub fn new() -> StubImages {
        StubImages::default()
    }

    /// Makes `url` ready as a `width`×`height` picture since `since` (a time on the harness's
    /// clock; long ago for one already in memory).
    pub fn ready(&mut self, url: &str, width: u32, height: u32, since: f64) -> ImageId {
        self.next += 1;
        let id = ImageId(self.next);
        self.states.insert(url.to_owned(), ImageState::Ready { id, width, height, since });
        id
    }

    /// Makes `url` fail for good.
    pub fn fail(&mut self, url: &str) {
        self.states.insert(url.to_owned(), ImageState::Failed);
    }

    /// Every request so far, in order.
    pub fn requests(&self) -> &[(ImageRequest, Priority)] {
        &self.requests
    }

    /// Forgets the requests so far.
    pub fn clear_requests(&mut self) {
        self.requests.clear();
    }
}

impl ImageProvider for StubImages {
    fn request(&mut self, req: &ImageRequest, priority: Priority) -> ImageState {
        self.requests.push((req.clone(), priority));
        self.states.get(&*req.url).copied().unwrap_or(ImageState::Loading)
    }
}

/// One frame as a test sees it.
#[derive(Clone, Debug)]
pub struct Frame {
    pub time: f64,
    pub list: DisplayList,
    pub focused: Option<Id>,
    pub output: FrameOutput,
}

/// One scripted step of input.
#[derive(Clone, Debug, PartialEq)]
pub enum Step {
    /// A key pressed and released within one frame.
    Press(Key),
    /// A key going down (a first press).
    Down(Key),
    /// The remote repeating a held key.
    Repeat(Key),
    Up(Key),
    /// Time passing: frames run whenever the last one asked for another (at most 60 a second),
    /// as the main loop runs them.
    Wait(f64),
    /// The pointer moving to a point on screen (dp).
    Point(Point),
    /// The pointer's button pressed and released where the pointer is.
    Click,
    /// A wheel notch: positive away from the viewer.
    Wheel(f32),
    /// Text typed.
    Text(String),
    /// The pointer hiding (the remote put it to sleep).
    HidePointer,
}

/// Runs frames headless, with a [`StubShaper`], [`StubImages`] and a clock the test moves.
pub struct Harness {
    pub state: UiState,
    pub shaper: StubShaper,
    pub images: StubImages,
    /// Device pixels per dp (2, as on the TV).
    pub scale: f32,
    time: f64,
    last_frame: f64,
    wake: Option<f64>,
}

impl Default for Harness {
    fn default() -> Harness {
        Harness::new()
    }
}

/// The shortest time between two frames the harness runs on its own: a 60 Hz display.
const FRAME: f64 = 1.0 / 60.0;

impl Harness {
    /// A harness at time 0.
    pub fn new() -> Harness {
        Harness {
            state: UiState::new(),
            shaper: StubShaper,
            images: StubImages::new(),
            scale: 2.0,
            time: 0.0,
            last_frame: f64::NEG_INFINITY,
            wake: None,
        }
    }

    /// The harness's clock, in seconds.
    pub fn time(&self) -> f64 {
        self.time
    }

    /// Runs one frame now with `events`, then the extra frame a focus change after drawing asks
    /// for ([`FrameOutput::redraw`]); returns the last.
    pub fn frame(&mut self, events: Vec<InputEvent>, draw: &mut dyn FnMut(&mut Ui)) -> Frame {
        let f = self.one(events, draw);
        if f.output.redraw { self.one(Vec::new(), draw) } else { f }
    }

    /// A frame with no input.
    pub fn idle(&mut self, draw: &mut dyn FnMut(&mut Ui)) -> Frame {
        self.frame(Vec::new(), draw)
    }

    /// Runs `step`; returns the last frame drawn (an idle one if waiting drew none).
    pub fn step(&mut self, step: Step, draw: &mut dyn FnMut(&mut Ui)) -> Frame {
        let frames = self.run(&[step], draw);
        match frames.into_iter().last() {
            Some(f) => f,
            None => self.idle(draw),
        }
    }

    /// Runs the steps in order; returns every frame drawn.
    pub fn run(&mut self, steps: &[Step], draw: &mut dyn FnMut(&mut Ui)) -> Vec<Frame> {
        let mut frames = Vec::new();
        for step in steps {
            let key =
                |key: Key, pressed: bool, repeat: bool| InputEvent::Key(KeyEvent { key, pressed, repeat, code: 0 });
            let events = match step {
                Step::Press(k) => vec![key(*k, true, false), key(*k, false, false)],
                Step::Down(k) => vec![key(*k, true, false)],
                Step::Repeat(k) => vec![key(*k, true, true)],
                Step::Up(k) => vec![key(*k, false, false)],
                Step::Point(p) => vec![InputEvent::Pointer(PointerEvent::Moved(*p))],
                Step::Click => vec![
                    InputEvent::Pointer(PointerEvent::Button { pressed: true }),
                    InputEvent::Pointer(PointerEvent::Button { pressed: false }),
                ],
                Step::Wheel(d) => vec![InputEvent::Pointer(PointerEvent::Wheel(*d))],
                Step::Text(s) => vec![InputEvent::Text(s.clone())],
                Step::HidePointer => vec![InputEvent::Pointer(PointerEvent::Hidden)],
                Step::Wait(secs) => {
                    frames.extend(self.wait(*secs, draw));
                    continue;
                }
            };
            frames.push(self.frame(events, draw));
        }
        frames
    }

    /// Lets `secs` pass, running the frames that come due meanwhile.
    pub fn wait(&mut self, secs: f64, draw: &mut dyn FnMut(&mut Ui)) -> Vec<Frame> {
        let end = self.time + secs;
        let mut frames = Vec::new();
        // A bound, so a frame that always asks for another can't hang a test.
        for _ in 0..100_000 {
            let Some(due) = self.wake else { break };
            let at = due.max(self.last_frame + FRAME);
            if at > end {
                break;
            }
            self.time = at;
            frames.push(self.frame(Vec::new(), draw));
        }
        self.time = end;
        frames
    }

    fn one(&mut self, events: Vec<InputEvent>, draw: &mut dyn FnMut(&mut Ui)) -> Frame {
        let input = FrameInput { time: self.time, events };
        let mut list = DisplayList::default();
        let mut ui = Ui::new(&mut self.state, &mut list, &self.shaper, &mut self.images, self.scale, &input);
        draw(&mut ui);
        let output = ui.finish();
        self.wake = output.wake_at;
        self.last_frame = self.time;
        Frame { time: self.time, list, focused: output.focused, output }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pley_base::Weight;

    fn style() -> TextStyle {
        TextStyle::new(10.0, 14.0, Weight::Regular)
    }

    #[test]
    fn stub_shaper_measures_by_characters() {
        let l = StubShaper.layout("Hello", &style(), &TextOpts { max_width: None, ..TextOpts::default() });
        assert_eq!(l.size, Size::new(25.0, 14.0));
        assert_eq!(l.glyphs.len(), 5);
        assert!(!l.truncated);
    }

    #[test]
    fn stub_shaper_wraps_cuts_and_aligns() {
        // 6 characters a line at 5 dp each.
        let l = StubShaper.layout("aaa bbb ccc ddd", &style(), &TextOpts::wrap(30.0, 2));
        assert_eq!(l.lines.len(), 2);
        assert!(l.truncated);
        assert_eq!(l.lines[1].glyphs.len(), 4);
        assert_eq!(l.glyphs.last().unwrap().id, '…' as u32 as u16);
        let l = StubShaper.layout("abcdefghij", &style(), &TextOpts::line(30.0));
        assert_eq!((l.lines.len(), l.lines[0].width, l.truncated), (1, 30.0, true));
        let l = StubShaper.layout("abcdefghij", &style(), &TextOpts::wrap(30.0, 0));
        assert_eq!(l.lines.len(), 2);
        let l = StubShaper.layout("ab", &style(), &TextOpts::line(30.0).aligned(Align::Center));
        assert_eq!((l.size.w, l.lines[0].x), (30.0, 10.0));
    }
}
