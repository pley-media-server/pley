//! Motion: the durations and easings of the Android TV app (`designsystem/PleyMotion.kt`) and the
//! tweens that carry values to their targets. A tween ends at a known time, so the frame loop
//! knows exactly when nothing moves any more and stops drawing.

use crate::id::{Id, IdMap};
use crate::input::KeyGate;

/// How a tween's progress maps to its value: straight, or a cubic Bézier from (0, 0) to (1, 1)
/// through two control points, as CSS's `cubic-bezier()` and Compose's `CubicBezierEasing`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Easing {
    Linear,
    Cubic { x1: f32, y1: f32, x2: f32, y2: f32 },
}

impl Easing {
    /// Progress as it is.
    pub const LINEAR: Easing = Easing::Linear;
    /// Compose's `FastOutSlowInEasing`, cubic-bezier(0.4, 0, 0.2, 1): things arriving.
    pub const FAST_OUT_SLOW_IN: Easing = Easing::cubic(0.4, 0.0, 0.2, 1.0);
    /// Compose's `FastOutLinearInEasing`, cubic-bezier(0.4, 0, 1, 1): things leaving.
    pub const FAST_OUT_LINEAR_IN: Easing = Easing::cubic(0.4, 0.0, 1.0, 1.0);
    /// Compose's `LinearOutSlowInEasing`, cubic-bezier(0, 0, 0.2, 1).
    pub const LINEAR_OUT_SLOW_IN: Easing = Easing::cubic(0.0, 0.0, 0.2, 1.0);

    /// A cubic Bézier through (x1, y1) and (x2, y2); x1 and x2 lie between 0 and 1.
    pub const fn cubic(x1: f32, y1: f32, x2: f32, y2: f32) -> Easing {
        Easing::Cubic { x1, y1, x2, y2 }
    }

    /// The eased value of `fraction` (0 to 1). As Compose does, fractions at or outside the ends
    /// pass through unchanged; inside, the curve's x is solved for `fraction` to float precision
    /// and its y returned.
    pub fn ease(self, fraction: f32) -> f32 {
        match self {
            Easing::Linear => fraction,
            Easing::Cubic { x1, y1, x2, y2 } => {
                if fraction <= 0.0 || fraction >= 1.0 {
                    return fraction;
                }
                let t = solve_t(x1 as f64, x2 as f64, fraction as f64);
                bezier(y1 as f64, y2 as f64, t) as f32
            }
        }
    }
}

/// One coordinate of the Bézier at `t`, with its ends at 0 and 1.
fn bezier(a: f64, b: f64, t: f64) -> f64 {
    let u = 1.0 - t;
    3.0 * a * u * u * t + 3.0 * b * u * t * t + t * t * t
}

/// The `t` at which the curve's x is `x`: Newton's method from `t = x`, falling back to bisection
/// where the slope flattens (x increases with t for control points between 0 and 1).
fn solve_t(x1: f64, x2: f64, x: f64) -> f64 {
    let mut t = x;
    for _ in 0..8 {
        let err = bezier(x1, x2, t) - x;
        if err.abs() < 1e-9 {
            return t;
        }
        let u = 1.0 - t;
        let slope = 3.0 * x1 * u * u + 6.0 * (x2 - x1) * u * t + 3.0 * (1.0 - x2) * t * t;
        if slope.abs() < 1e-6 {
            break;
        }
        t -= err / slope;
        if !(0.0..=1.0).contains(&t) {
            break;
        }
    }
    let (mut lo, mut hi) = (0.0f64, 1.0f64);
    t = 0.5;
    for _ in 0..64 {
        let err = bezier(x1, x2, t) - x;
        if err.abs() < 1e-10 {
            break;
        }
        if err < 0.0 {
            lo = t;
        } else {
            hi = t;
        }
        t = 0.5 * (lo + hi);
    }
    t
}

/// A duration and easing from the design, and when it gives way to a snap.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Motion {
    /// In seconds.
    pub duration: f32,
    pub easing: Easing,
    /// Still animates with Reduce motion on; everything else snaps then.
    pub reduced: bool,
    /// Snaps on Low-tier TVs (Android's T0).
    pub low_snaps: bool,
    /// Still animates while a direction key is held ([`Motion::GLIDE`]); everything else snaps
    /// then.
    pub held: bool,
}

impl Motion {
    /// A tile or control taking focus: scale up, ring in (`FOCUS_MS`).
    pub const FOCUS: Motion = Motion::new(0.150, Easing::FAST_OUT_SLOW_IN);
    /// Losing focus (`UNFOCUS_MS`); on Low-tier TVs only the focused tile animates.
    pub const UNFOCUS: Motion = Motion { low_snaps: true, ..Motion::new(0.120, Easing::FAST_OUT_LINEAR_IN) };
    /// The pressed scale while OK is held (`PRESS_MS`).
    pub const PRESS: Motion = Motion::new(0.100, Easing::LINEAR);
    /// Round buttons' labels, info blocks, the hero's fade, toasts, the offline notice
    /// (`REVEAL_MS`); kept with Reduce motion.
    pub const REVEAL: Motion = Motion { reduced: true, ..Motion::new(0.150, Easing::LINEAR) };
    /// A backdrop's cross-fade (`BACKDROP_MS`); Low-tier TVs swap at once.
    pub const BACKDROP: Motion = Motion { low_snaps: true, ..Motion::new(0.300, Easing::LINEAR) };
    /// Side panels sliding and fading, dialogs fading (`OVERLAY_MS`).
    pub const OVERLAY: Motion = Motion::new(0.200, Easing::FAST_OUT_SLOW_IN);
    /// A screen change (`SCREEN_MS`): the curtain, a Bg rectangle over the page that snaps to
    /// opaque and fades out over half of it ([`Motion::CURTAIN`]); none for the first screen.
    pub const SCREEN: Motion = Motion::new(0.220, Easing::LINEAR);
    /// The curtain's fade-out: half of [`Motion::SCREEN`].
    pub const CURTAIN: Motion = Motion::new(0.110, Easing::LINEAR);
    /// A picture that wasn't in memory fading in (`IMAGE_FADE_MS`); none on Low-tier TVs.
    pub const IMAGE_FADE: Motion = Motion { low_snaps: true, ..Motion::new(0.150, Easing::LINEAR) };
    /// A row, page or grid following focus under a held key: evenly over a little more than the
    /// time between held moves ([`KeyGate::GLIDE`]), so each glide runs into the next and the
    /// content slides on without a pause (the Samsung app's `Scrolling.Glide`). Snaps with Reduce
    /// motion.
    pub const GLIDE: Motion = Motion { held: true, ..Motion::new(KeyGate::GLIDE as f32, Easing::LINEAR) };

    /// A motion that snaps with Reduce motion on and animates on every tier.
    pub const fn new(duration: f32, easing: Easing) -> Motion {
        Motion { duration, easing, reduced: false, low_snaps: false, held: false }
    }

    /// The eased progress `elapsed` seconds in, 0 to 1.
    pub fn progress(&self, elapsed: f64) -> f32 {
        if self.duration <= 0.0 {
            return 1.0;
        }
        self.easing.ease((elapsed / self.duration as f64).clamp(0.0, 1.0) as f32)
    }
}

/// Up to four lanes animated together: a number, a colour's channels or a rectangle.
pub(crate) type Lanes = [f32; 4];

/// A value on its way from one target to the next.
#[derive(Clone, Copy, Debug)]
struct Tween {
    from: Lanes,
    to: Lanes,
    start: f64,
    motion: Motion,
    /// The last frame that asked for it; values nobody asked for are dropped.
    touched: u64,
}

impl Tween {
    fn value(&self, now: f64) -> Lanes {
        if self.from == self.to {
            return self.to;
        }
        let p = self.motion.progress(now - self.start);
        if p >= 1.0 {
            return self.to;
        }
        std::array::from_fn(|i| self.from[i] + (self.to[i] - self.from[i]) * p)
    }

    fn running(&self, now: f64) -> bool {
        self.from != self.to && now < self.start + self.motion.duration as f64
    }
}

/// Every animated value, by id.
#[derive(Default)]
pub(crate) struct Tweens {
    map: IdMap<Tween>,
}

impl Tweens {
    /// The value of `id` at `now`, heading for `to`: a new value starts at `from` (or at `to`),
    /// and a changed target starts a tween from where the value is. Also says whether it still
    /// moves.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn animate(
        &mut self,
        id: Id,
        from: Option<Lanes>,
        to: Lanes,
        motion: Motion,
        now: f64,
        frame: u64,
        snap: bool,
    ) -> (Lanes, bool) {
        let t = self.map.entry(id).or_insert_with(|| {
            let start = from.unwrap_or(to);
            Tween { from: start, to: start, start: now, motion, touched: frame }
        });
        t.touched = frame;
        if !same(t.to, to) {
            t.from = t.value(now);
            t.to = to;
            t.start = now;
            t.motion = motion;
        }
        if snap {
            t.from = t.to;
        }
        (t.value(now), t.running(now))
    }

    /// Drops the values no frame asked for since `frame`.
    pub(crate) fn sweep(&mut self, frame: u64) {
        self.map.retain(|_, t| t.touched >= frame);
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.map.len()
    }
}

/// Targets this close are the same target, so one recomputed with rounding differences doesn't
/// restart its tween.
fn same(a: Lanes, b: Lanes) -> bool {
    a.iter().zip(b.iter()).all(|(x, y)| (x - y).abs() <= 1e-4)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The curve's y at x by plain bisection in f64: an independent reference.
    fn reference(x1: f64, y1: f64, x2: f64, y2: f64, x: f64) -> f64 {
        let (mut lo, mut hi) = (0.0, 1.0);
        for _ in 0..200 {
            let mid = (lo + hi) / 2.0;
            if bezier(x1, x2, mid) < x { lo = mid } else { hi = mid }
        }
        bezier(y1, y2, (lo + hi) / 2.0)
    }

    #[test]
    fn cubic_matches_the_curve() {
        for e in [Easing::FAST_OUT_SLOW_IN, Easing::FAST_OUT_LINEAR_IN, Easing::LINEAR_OUT_SLOW_IN] {
            let Easing::Cubic { x1, y1, x2, y2 } = e else { unreachable!() };
            for i in 1..100 {
                let x = i as f64 / 100.0;
                let want = reference(x1 as f64, y1 as f64, x2 as f64, y2 as f64, x);
                assert!((e.ease(x as f32) as f64 - want).abs() < 1e-5, "{e:?} at {x}");
            }
        }
    }

    #[test]
    fn ends_pass_through_and_curves_are_monotonic() {
        let e = Easing::FAST_OUT_SLOW_IN;
        assert_eq!((e.ease(0.0), e.ease(1.0), e.ease(-0.5), e.ease(1.5)), (0.0, 1.0, -0.5, 1.5));
        let mut last = 0.0;
        for i in 1..=1000 {
            let v = e.ease(i as f32 / 1000.0);
            assert!(v >= last);
            last = v;
        }
        // FastOutSlowIn is past three quarters at half time; FastOutLinearIn well short of half.
        assert!(e.ease(0.5) > 0.75 && e.ease(0.5) < 0.8);
        assert!(Easing::FAST_OUT_LINEAR_IN.ease(0.5) < 0.35);
        assert_eq!(Easing::LINEAR.ease(0.3), 0.3);
    }

    #[test]
    fn motions_match_the_design() {
        assert_eq!((Motion::FOCUS.duration, Motion::FOCUS.easing), (0.150, Easing::FAST_OUT_SLOW_IN));
        assert_eq!((Motion::UNFOCUS.duration, Motion::UNFOCUS.easing), (0.120, Easing::FAST_OUT_LINEAR_IN));
        assert_eq!((Motion::PRESS.duration, Motion::PRESS.easing), (0.100, Easing::LINEAR));
        assert_eq!((Motion::REVEAL.duration, Motion::REVEAL.reduced), (0.150, true));
        assert_eq!(Motion::BACKDROP.duration, 0.300);
        assert_eq!((Motion::OVERLAY.duration, Motion::OVERLAY.easing), (0.200, Easing::FAST_OUT_SLOW_IN));
        assert_eq!(Motion::SCREEN.duration, 0.220);
        assert_eq!(Motion::CURTAIN.duration * 2.0, Motion::SCREEN.duration);
        assert_eq!(Motion::IMAGE_FADE.duration, 0.150);
        const { assert!(Motion::UNFOCUS.low_snaps && Motion::BACKDROP.low_snaps && Motion::IMAGE_FADE.low_snaps) };
        const { assert!(!Motion::FOCUS.low_snaps && !Motion::FOCUS.reduced && !Motion::FOCUS.held) };
        assert!((Motion::GLIDE.duration - 0.05625).abs() < 1e-6 && Motion::GLIDE.easing == Easing::LINEAR);
        const { assert!(Motion::GLIDE.held && !Motion::GLIDE.reduced) };
    }

    #[test]
    fn tweens_run_to_their_target_and_settle() {
        let id = Id::new("t");
        let mut tw = Tweens::default();
        // A new value starts at its target.
        assert_eq!(tw.animate(id, None, [0.0; 4], Motion::FOCUS, 0.0, 1, false), ([0.0; 4], false));
        let (v, running) = tw.animate(id, None, [1.0; 4], Motion::FOCUS, 1.0, 2, false);
        assert_eq!((v[0], running), (0.0, true));
        let (v, running) = tw.animate(id, None, [1.0; 4], Motion::FOCUS, 1.075, 3, false);
        assert!(running && (v[0] - Easing::FAST_OUT_SLOW_IN.ease(0.5)).abs() < 1e-6);
        assert_eq!(tw.animate(id, None, [1.0; 4], Motion::FOCUS, 1.151, 4, false), ([1.0; 4], false));
        // Turning back starts from where it is, with the new motion.
        tw.animate(id, None, [0.0; 4], Motion::UNFOCUS, 2.0, 5, false);
        let (v, _) = tw.animate(id, None, [0.0; 4], Motion::FOCUS, 2.06, 6, false);
        assert!((v[0] - (1.0 - Easing::FAST_OUT_LINEAR_IN.ease(0.5))).abs() < 1e-6);
        // A snap lands at once.
        assert_eq!(tw.animate(id, None, [1.0; 4], Motion::FOCUS, 2.07, 7, true), ([1.0; 4], false));
    }

    #[test]
    fn an_explicit_start_animates_a_new_value_and_untouched_values_go() {
        let mut tw = Tweens::default();
        let (v, running) = tw.animate(Id::new("panel"), Some([0.0; 4]), [1.0; 4], Motion::OVERLAY, 5.0, 1, false);
        assert_eq!((v[0], running), (0.0, true));
        tw.animate(Id::new("other"), None, [1.0; 4], Motion::OVERLAY, 5.0, 2, false);
        tw.sweep(2);
        assert_eq!(tw.len(), 1);
    }
}
