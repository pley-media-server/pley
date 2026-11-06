//! How subtitles look and where they go: the subtitle settings (size,
//! background, font, position) turned into the geometry and text styles the
//! player screen draws with, for the rectangle the video fills and how far
//! the player's controls have lifted the text.
//!
//! The numbers are the Android TV app's (`ScreenCaptions.kt`,
//! `TvPlayerScreen.kt`) on its 960×540 dp
//! canvas: text 3.8 to 7.6 % of the height, white and bold with a black
//! outline or on a 66 % black box, its last line 40 dp above the bottom,
//! raised by 0, 5, 10 or 15 % of the height, 48 dp from the sides and 27 dp
//! from the top, and moved up 113 dp while the controls show. Pictures keep
//! the places their frames give them over the whole video.

use pley_base::{Align, Color, Family, Rect, TextStyle, Weight};

use crate::picture::PictureCue;
use crate::vtt::{CueAlign, Span, TextCue};

/// The canvas height the dp constants below are measured on.
const CANVAS_HEIGHT: f32 = 540.0;
/// The TV's safe area at the sides (`TvSafeArea.Horizontal`).
pub const SAFE_SIDE: f32 = 48.0;
/// The TV's safe area at the top (`TvSafeArea.Vertical`): cues placed at the top stay inside it.
pub const SAFE_TOP: f32 = 27.0;
/// Text's last line stays this far above the bottom edge: the safe area's
/// 27 dp and a little more, so descenders and outlines clear an overscanned
/// edge (`SUBTITLES_BOTTOM`).
pub const SUBTITLES_BOTTOM: f32 = 40.0;
/// While the controls show, text sits above their times row, which starts
/// 399 dp down (`SUBTITLES_LIFTED` = 540 − 399 + 12).
pub const SUBTITLES_LIFTED: f32 = 153.0;
/// Line height as a share of the text size, as the TV app draws subtitles.
pub const LINE_HEIGHT: f32 = 1.25;
/// Between one cue and the next (the TV app's `.cue-line` margin).
pub const CUE_GAP: f32 = 2.0;
/// The box behind text when chosen: black at 66 % (`CaptionLook.BOX_COLOR`, `0xA8000000`).
pub const BOX_COLOR: Color = Color::rgba(0x000000A8);

/// Subtitle text height (Settings → Subtitles → Size).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum SubtitleSize {
    Small,
    #[default]
    Normal,
    Large,
    Largest,
}

impl SubtitleSize {
    /// Every size, in the order Settings offers them.
    pub const ALL: [SubtitleSize; 4] =
        [SubtitleSize::Small, SubtitleSize::Normal, SubtitleSize::Large, SubtitleSize::Largest];

    /// The text's height as a share of the screen's (Android's `SUBTITLE_SIZES`).
    pub fn fraction(self) -> f32 {
        match self {
            SubtitleSize::Small => 0.038,
            SubtitleSize::Normal => 0.048,
            SubtitleSize::Large => 0.06,
            SubtitleSize::Largest => 0.076,
        }
    }

    /// The id saved in the settings, as the TV app saves it.
    pub fn id(self) -> &'static str {
        match self {
            SubtitleSize::Small => "small",
            SubtitleSize::Normal => "normal",
            SubtitleSize::Large => "large",
            SubtitleSize::Largest => "largest",
        }
    }

    /// The size saved as `id`.
    pub fn from_id(id: &str) -> Option<SubtitleSize> {
        SubtitleSize::ALL.into_iter().find(|s| s.id() == id)
    }
}

/// Subtitle typeface (Settings → Subtitles → Font): offered only where the
/// TV draws it differently from the default ([`fonts_here`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum SubtitleFont {
    #[default]
    Default,
    Serif,
    Monospace,
    Casual,
}

impl SubtitleFont {
    /// Every font, in the order Settings offers them.
    pub const ALL: [SubtitleFont; 4] =
        [SubtitleFont::Default, SubtitleFont::Serif, SubtitleFont::Monospace, SubtitleFont::Casual];

    /// The id saved in the settings, as the TV app saves it.
    pub fn id(self) -> &'static str {
        match self {
            SubtitleFont::Default => "default",
            SubtitleFont::Serif => "serif",
            SubtitleFont::Monospace => "mono",
            SubtitleFont::Casual => "casual",
        }
    }

    /// The font saved as `id`.
    pub fn from_id(id: &str) -> Option<SubtitleFont> {
        SubtitleFont::ALL.into_iter().find(|f| f.id() == id)
    }

    /// The family text is shaped in. Serif and casual go by the generic
    /// names CSS gives them, which the shaper finds among the TV's fonts
    /// ([`SubtitleFont::faces`]) or falls back from to the default.
    pub fn family(self) -> Family {
        match self {
            SubtitleFont::Default => Family::Sans,
            SubtitleFont::Serif => Family::Named("serif".into()),
            SubtitleFont::Monospace => Family::Mono,
            SubtitleFont::Casual => Family::Named("cursive".into()),
        }
    }

    /// The faces the TV app asks for, in order, for a shaper resolving
    /// [`SubtitleFont::family`] among the TV's fonts.
    pub fn faces(self) -> &'static [&'static str] {
        match self {
            SubtitleFont::Default => &["Roboto", "LG Smart UI"],
            SubtitleFont::Serif => &["Roboto Serif", "Noto Serif", "DejaVu Serif", "Georgia"],
            SubtitleFont::Monospace => &["Roboto Mono", "Noto Sans Mono", "DejaVu Sans Mono"],
            SubtitleFont::Casual => &["Comic Sans MS", "Comic Neue", "Coming Soon"],
        }
    }
}

/// How far above the bottom text subtitles sit (Settings → Subtitles → Position).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum SubtitlePosition {
    #[default]
    Bottom,
    Raised,
    Higher,
    Highest,
}

impl SubtitlePosition {
    /// Every position, in the order Settings offers them.
    pub const ALL: [SubtitlePosition; 4] =
        [SubtitlePosition::Bottom, SubtitlePosition::Raised, SubtitlePosition::Higher, SubtitlePosition::Highest];

    /// The id saved in the settings, as the TV app saves it.
    pub fn id(self) -> &'static str {
        match self {
            SubtitlePosition::Bottom => "bottom",
            SubtitlePosition::Raised => "raised",
            SubtitlePosition::Higher => "higher",
            SubtitlePosition::Highest => "highest",
        }
    }

    /// The position saved as `id`.
    pub fn from_id(id: &str) -> Option<SubtitlePosition> {
        SubtitlePosition::ALL.into_iter().find(|p| p.id() == id)
    }

    /// The raise as a share of the height: Android's `SUBTITLE_RAISES`
    /// (0, 5, 10 and 15 %; the TV app's "Highest" was 16 %).
    pub fn raise(self) -> f32 {
        match self {
            SubtitlePosition::Bottom => 0.0,
            SubtitlePosition::Raised => 0.05,
            SubtitlePosition::Higher => 0.10,
            SubtitlePosition::Highest => 0.15,
        }
    }

    /// The raise in dp on the 540 dp canvas, whole.
    pub fn lift_dp(self) -> f32 {
        (CANVAS_HEIGHT * self.raise()).round()
    }
}

/// The subtitle settings.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct SubtitleSettings {
    pub size: SubtitleSize,
    /// A dark box behind the text rather than none ("None, with an outline").
    pub boxed: bool,
    pub font: SubtitleFont,
    pub position: SubtitlePosition,
}

/// The black edge drawn around text.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Outline {
    pub color: Color,
    /// How far it reaches outside the glyphs, in dp: Media3 strokes 2 dp
    /// centred on the glyphs' edges.
    pub width: f32,
}

/// The box drawn behind each line of text when chosen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CueBox {
    pub color: Color,
    /// Beyond the line's ends, in dp (Media3: 0.125 of the text size). The
    /// box spans the line's height.
    pub pad_x: f32,
    pub radius: f32,
}

/// Where and how text subtitles are drawn now: [`SubtitleSettings::text_layer`].
#[derive(Clone, Debug, PartialEq)]
pub struct TextLayer {
    /// Where cues are laid out, the controls' lift applied: lines wrap at its
    /// width, and cues at the bottom stack up from its bottom.
    pub area: Rect,
    /// Where cues placed at the top ([`CueGroup::Top`]) start stacking down,
    /// inside the safe area. The lift doesn't move them: Android moves its
    /// whole layer, which would take them off the screen; the TV app kept
    /// them in place. The server's WebVTT places no cue anyway.
    pub top_y: f32,
    /// How far the controls lift the text now, in dp (already in `area`).
    pub lift: f32,
    /// Plain text; [`TextLayer::span_style`] gives each span's.
    pub style: TextStyle,
    pub color: Color,
    pub outline: Option<Outline>,
    pub cue_box: Option<CueBox>,
    /// Between one cue and the next, in dp.
    pub cue_gap: f32,
    /// An underline's top below the baseline and its thickness, in dp
    /// (Skia's defaults, as Android draws `<u>`: 1/9 and 1/18 of the size).
    pub underline: (f32, f32),
}

impl TextLayer {
    /// The style of one span: italic where it says; the weight stays bold, as
    /// Android draws subtitles in a bold face whatever the markup.
    pub fn span_style(&self, span: &Span) -> TextStyle {
        TextStyle { italic: span.italic, ..self.style.clone() }
    }
}

impl SubtitleSettings {
    /// Text subtitles over `video` (the canvas rectangle the picture fills,
    /// the whole 960×540 normally), lifted by `lift` of the way above the
    /// controls (0 hidden, 1 shown; the screen animates it with the
    /// controls' 200 ms fade, as a move only: sizes and line breaks don't
    /// change). Measures scale with the video's height, so a picture moved
    /// aside keeps the look.
    pub fn text_layer(&self, video: Rect, lift: f32) -> TextLayer {
        let k = video.h / CANVAS_HEIGHT;
        let size = self.size.fraction() * video.h;
        // Android rounds the raise to whole pixels; whole dp here.
        let bottom = SUBTITLES_BOTTOM * k + (self.position.raise() * video.h).round();
        let lift = (SUBTITLES_LIFTED - SUBTITLES_BOTTOM) * k * lift.clamp(0.0, 1.0);
        let area = Rect::new(
            video.x + SAFE_SIDE * k,
            video.y + SAFE_TOP * k - lift,
            (video.w - 2.0 * SAFE_SIDE * k).max(0.0),
            (video.h - SAFE_TOP * k - bottom).max(0.0),
        );
        let style = TextStyle {
            family: self.font.family(),
            size,
            line_height: size * LINE_HEIGHT,
            weight: Weight::Bold,
            italic: false,
            tracking: 0.0,
        };
        TextLayer {
            area,
            top_y: video.y + SAFE_TOP * k,
            lift,
            style,
            color: Color::WHITE,
            outline: Some(Outline { color: Color::BLACK, width: 1.0 * k }),
            cue_box: self.boxed.then_some(CueBox { color: BOX_COLOR, pad_x: 0.125 * size, radius: 2.0 * k }),
            cue_gap: CUE_GAP * k,
            underline: (size / 9.0, size / 18.0),
        }
    }
}

/// Which end of the screen a cue stacks from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CueGroup {
    Top,
    Bottom,
}

/// Where a cue goes: near the top when it asks for one of the first four
/// lines or a line above half the height; otherwise at the bottom.
/// `position` and `size` aren't used, as on the TV app.
pub fn cue_group(cue: &TextCue) -> CueGroup {
    match cue.line {
        Some(l) if (l.snap_to_lines && (0.0..4.0).contains(&l.value)) || (!l.snap_to_lines && l.value < 50.0) => {
            CueGroup::Top
        }
        _ => CueGroup::Bottom,
    }
}

/// How a cue's lines are aligned in the layer's width.
pub fn cue_align(cue: &TextCue) -> Align {
    match cue.align {
        CueAlign::Start | CueAlign::Left => Align::Start,
        CueAlign::End | CueAlign::Right => Align::End,
        CueAlign::Center => Align::Center,
    }
}

/// Where a picture goes on the canvas: its place on the stream's plane,
/// stretched to the rectangle the video fills, as the TV app and Android
/// draw them. Pictures take no padding, raise or lift.
pub fn picture_rect(cue: &PictureCue, video: Rect) -> Rect {
    let (pw, ph) = (cue.plane.w.max(1.0), cue.plane.h.max(1.0));
    let r = cue.rect;
    Rect::new(video.x + r.x * video.w / pw, video.y + r.y * video.h / ph, r.w * video.w / pw, r.h * video.h / ph)
}

/// A sample measured in each family to tell the faces apart.
pub const FONT_SAMPLE: &str = "The quick brown fox jumps over the lazy dog 0123456789 WMwmil";

/// The fonts this TV draws differently from the default (a family the TV
/// has no face for falls back to the default's): those are offered, and the
/// default always is. `measure` gives [`FONT_SAMPLE`]'s width at 48 dp in a
/// family, `None` when it can't.
pub fn fonts_here(mut measure: impl FnMut(&Family) -> Option<f32>) -> Vec<SubtitleFont> {
    let base = measure(&SubtitleFont::Default.family());
    let mut out = vec![SubtitleFont::Default];
    for font in &SubtitleFont::ALL[1..] {
        if let (Some(base), Some(width)) = (base, measure(&font.family()))
            && (width - base).abs() > 0.5
        {
            out.push(*font);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use pley_base::{RgbaImage, Size};

    use super::*;
    use crate::vtt::{CueLine, parse_vtt};

    const SCREEN: Rect = Rect::new(0.0, 0.0, 960.0, 540.0);

    #[test]
    fn offers_the_fonts_and_positions_in_settings_order() {
        // Settings names them (pley_core's store, in the language shown); these are their ids.
        let fonts: Vec<&str> = SubtitleFont::ALL.iter().map(|f| f.id()).collect();
        assert_eq!(fonts, ["default", "serif", "mono", "casual"]);
        let positions: Vec<&str> = SubtitlePosition::ALL.iter().map(|p| p.id()).collect();
        assert_eq!(positions, ["bottom", "raised", "higher", "highest"]);
        // Android's raises: the TypeScript test's last one is 86 (16 %).
        let lifts: Vec<f32> = SubtitlePosition::ALL.iter().map(|p| p.lift_dp()).collect();
        assert_eq!(lifts, [0.0, 27.0, 54.0, 81.0]);
        assert_eq!(SubtitleSize::from_id("largest"), Some(SubtitleSize::Largest));
        assert_eq!(SubtitleFont::from_id("mono"), Some(SubtitleFont::Monospace));
        assert_eq!(SubtitlePosition::from_id("raised"), Some(SubtitlePosition::Raised));
        assert_eq!(SubtitleSize::from_id("huge"), None);
    }

    #[test]
    fn offers_a_font_only_when_the_tv_draws_it_differently_from_the_default() {
        // A TV with a monospace face, whose serif and casual families fall back to its sans.
        let widths = |f: &Family| match f {
            Family::Mono => Some(1_200.0),
            Family::Named(n) if &**n == "cursive" => Some(1_000.3),
            _ => Some(1_000.0),
        };
        assert_eq!(fonts_here(widths), [SubtitleFont::Default, SubtitleFont::Monospace]);
        // A shaper that can't measure offers the default alone.
        assert_eq!(fonts_here(|_| None), [SubtitleFont::Default]);
    }

    #[test]
    fn places_text_raised_as_chosen_and_above_the_controls_while_they_show() {
        let bottom_gap = |s: SubtitleSettings, lift: f32| 540.0 - s.text_layer(SCREEN, lift).area.bottom();
        let normal = SubtitleSettings::default();
        assert_eq!(bottom_gap(normal, 0.0), 40.0);
        let higher = SubtitleSettings { font: SubtitleFont::Serif, position: SubtitlePosition::Higher, ..normal };
        assert_eq!(bottom_gap(higher, 0.0), 94.0);
        assert_eq!(higher.text_layer(SCREEN, 0.0).style.family, Family::Named("serif".into()));
        // Android lifts the layer by 113 dp whatever its raise; the TV app kept it at max(153, 94).
        assert_eq!(bottom_gap(higher, 1.0), 207.0);
        assert_eq!(bottom_gap(normal, 1.0), 153.0);
        assert_eq!(bottom_gap(normal, 0.5), 96.5);
        let highest = SubtitleSettings { position: SubtitlePosition::Highest, ..normal };
        assert_eq!(bottom_gap(highest, 0.0), 121.0);
        let layer = normal.text_layer(SCREEN, 0.0);
        assert_eq!(layer.area, Rect::new(48.0, 27.0, 864.0, 473.0));
        // Cues placed at the top stay inside the safe area, lifted or not.
        assert_eq!((layer.top_y, normal.text_layer(SCREEN, 1.0).top_y), (27.0, 27.0));
        assert_eq!(layer.style.weight, Weight::Bold);
    }

    #[test]
    fn sizes_text_as_a_share_of_the_height_with_an_outline_or_a_box() {
        let sizes: Vec<f32> = SubtitleSize::ALL
            .iter()
            .map(|size| {
                (SubtitleSettings { size: *size, ..Default::default() }.text_layer(SCREEN, 0.0).style.size * 10.0)
                    .round()
                    / 10.0
            })
            .collect();
        assert_eq!(sizes, [20.5, 25.9, 32.4, 41.0]);
        let plain = SubtitleSettings::default().text_layer(SCREEN, 0.0);
        assert_eq!(plain.outline, Some(Outline { color: Color::BLACK, width: 1.0 }));
        assert_eq!(plain.cue_box, None);
        let boxed = SubtitleSettings { boxed: true, ..Default::default() }.text_layer(SCREEN, 0.0);
        assert_eq!(boxed.cue_box.map(|b| (b.color.a, b.radius)), Some((0xA8, 2.0)));
        let italic = Span { text: "x".into(), italic: true, ..Span::default() };
        assert!(boxed.span_style(&italic).italic);
        assert_eq!(boxed.span_style(&italic).weight, Weight::Bold);
        // Half the height: half of everything.
        let small = SubtitleSettings::default().text_layer(Rect::new(480.0, 0.0, 480.0, 270.0), 1.0);
        assert_eq!(small.area, Rect::new(504.0, 13.5 - 56.5, 432.0, 236.5));
        assert!((small.style.size - 12.96).abs() < 1e-4);
    }

    #[test]
    fn groups_and_aligns_cues_as_they_ask() {
        let cues = parse_vtt(
            "WEBVTT\n\n00:01.000 --> 00:02.000 line:0 align:start\na\n\n00:01.000 --> 00:02.000 line:20%\nb\n\n\
             00:01.000 --> 00:02.000 line:-1 align:right\nc\n\n00:01.000 --> 00:02.000 line:4\nd\n\n00:01.000 --> 00:02.000\ne\n",
        );
        let groups: Vec<CueGroup> = cues.iter().map(cue_group).collect();
        use CueGroup::{Bottom, Top};
        assert_eq!(groups, [Top, Top, Bottom, Bottom, Bottom]);
        let aligns: Vec<Align> = cues.iter().map(cue_align).collect();
        assert_eq!(aligns, [Align::Start, Align::Center, Align::End, Align::Center, Align::Center]);
        assert_eq!(cues[1].line, Some(CueLine { value: 20.0, snap_to_lines: false }));
    }

    #[test]
    fn stretches_pictures_from_their_plane_to_the_video() {
        let cue = PictureCue {
            start_ms: 0,
            end_ms: None,
            image: Arc::new(RgbaImage::new(360, 60)),
            rect: Rect::new(180.0, 480.0, 360.0, 60.0),
            plane: Size::new(720.0, 576.0),
            key: 1,
        };
        assert_eq!(picture_rect(&cue, SCREEN), Rect::new(240.0, 450.0, 480.0, 56.25));
        assert_eq!(picture_rect(&cue, Rect::new(100.0, 0.0, 480.0, 270.0)), Rect::new(220.0, 225.0, 240.0, 28.125));
    }
}
