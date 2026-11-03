//! Points, sizes and rectangles in dp on the 960×540 canvas.

use std::ops::{Add, Sub};

/// A position in dp.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub const ZERO: Point = Point { x: 0.0, y: 0.0 };

    pub const fn new(x: f32, y: f32) -> Point {
        Point { x, y }
    }
}

impl Add for Point {
    type Output = Point;

    fn add(self, o: Point) -> Point {
        Point::new(self.x + o.x, self.y + o.y)
    }
}

impl Sub for Point {
    type Output = Point;

    fn sub(self, o: Point) -> Point {
        Point::new(self.x - o.x, self.y - o.y)
    }
}

/// A width and height in dp.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Size {
    pub w: f32,
    pub h: f32,
}

impl Size {
    pub const ZERO: Size = Size { w: 0.0, h: 0.0 };

    pub const fn new(w: f32, h: f32) -> Size {
        Size { w, h }
    }
}

/// Space kept inside a rectangle's edges.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Insets {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl Insets {
    pub const fn all(v: f32) -> Insets {
        Insets { left: v, top: v, right: v, bottom: v }
    }

    pub const fn xy(x: f32, y: f32) -> Insets {
        Insets { left: x, top: y, right: x, bottom: y }
    }
}

/// An axis-aligned rectangle in dp: its top-left corner and its size.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub const ZERO: Rect = Rect { x: 0.0, y: 0.0, w: 0.0, h: 0.0 };

    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect { x, y, w, h }
    }

    pub fn from_origin_size(origin: Point, size: Size) -> Rect {
        Rect::new(origin.x, origin.y, size.w, size.h)
    }

    /// The rectangle between two corners, whichever way round they are given.
    pub fn from_corners(a: Point, b: Point) -> Rect {
        let (x0, x1) = if a.x <= b.x { (a.x, b.x) } else { (b.x, a.x) };
        let (y0, y1) = if a.y <= b.y { (a.y, b.y) } else { (b.y, a.y) };
        Rect::new(x0, y0, x1 - x0, y1 - y0)
    }

    pub fn origin(&self) -> Point {
        Point::new(self.x, self.y)
    }

    pub fn size(&self) -> Size {
        Size::new(self.w, self.h)
    }

    pub fn right(&self) -> f32 {
        self.x + self.w
    }

    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }

    pub fn center(&self) -> Point {
        Point::new(self.x + self.w / 2.0, self.y + self.h / 2.0)
    }

    pub fn is_empty(&self) -> bool {
        self.w <= 0.0 || self.h <= 0.0
    }

    pub fn contains(&self, p: Point) -> bool {
        p.x >= self.x && p.x < self.right() && p.y >= self.y && p.y < self.bottom()
    }

    pub fn intersects(&self, o: &Rect) -> bool {
        self.x < o.right() && o.x < self.right() && self.y < o.bottom() && o.y < self.bottom()
    }

    /// The overlap of two rectangles, if they overlap at all.
    pub fn intersect(&self, o: &Rect) -> Option<Rect> {
        let x0 = self.x.max(o.x);
        let y0 = self.y.max(o.y);
        let x1 = self.right().min(o.right());
        let y1 = self.bottom().min(o.bottom());
        (x1 > x0 && y1 > y0).then(|| Rect::new(x0, y0, x1 - x0, y1 - y0))
    }

    /// The smallest rectangle holding both.
    pub fn union(&self, o: &Rect) -> Rect {
        if self.is_empty() {
            return *o;
        }
        if o.is_empty() {
            return *self;
        }
        let x0 = self.x.min(o.x);
        let y0 = self.y.min(o.y);
        Rect::new(x0, y0, self.right().max(o.right()) - x0, self.bottom().max(o.bottom()) - y0)
    }

    pub fn translate(&self, dx: f32, dy: f32) -> Rect {
        Rect::new(self.x + dx, self.y + dy, self.w, self.h)
    }

    /// Grown by `d` on every side (shrunk when `d` is negative).
    pub fn expand(&self, d: f32) -> Rect {
        Rect::new(self.x - d, self.y - d, self.w + 2.0 * d, self.h + 2.0 * d)
    }

    pub fn inset(&self, i: Insets) -> Rect {
        Rect::new(self.x + i.left, self.y + i.top, self.w - i.left - i.right, self.h - i.top - i.bottom)
    }

    /// Scaled by `s` about `pivot`, as a focused tile grows about its centre.
    pub fn scale_about(&self, pivot: Point, s: f32) -> Rect {
        Rect::new(pivot.x + (self.x - pivot.x) * s, pivot.y + (self.y - pivot.y) * s, self.w * s, self.h * s)
    }

    /// A rectangle of `size` placed inside this one: `fx` and `fy` from 0 (start)
    /// to 1 (end), 0.5 centring it.
    pub fn place(&self, size: Size, fx: f32, fy: f32) -> Rect {
        Rect::new(self.x + (self.w - size.w) * fx, self.y + (self.h - size.h) * fy, size.w, size.h)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intersect_and_union() {
        let a = Rect::new(0.0, 0.0, 10.0, 10.0);
        let b = Rect::new(5.0, 5.0, 10.0, 10.0);
        assert_eq!(a.intersect(&b), Some(Rect::new(5.0, 5.0, 5.0, 5.0)));
        assert_eq!(a.union(&b), Rect::new(0.0, 0.0, 15.0, 15.0));
        assert_eq!(a.intersect(&Rect::new(20.0, 0.0, 1.0, 1.0)), None);
    }

    #[test]
    fn scale_about_centre_keeps_centre() {
        let r = Rect::new(96.0, 210.0, 108.0, 162.0);
        let s = r.scale_about(r.center(), 1.08);
        assert!((s.center().x - r.center().x).abs() < 1e-4);
        assert!((s.w - 116.64).abs() < 1e-3);
    }

    #[test]
    fn place_centres() {
        let r = Rect::new(0.0, 0.0, 100.0, 50.0).place(Size::new(20.0, 10.0), 0.5, 0.5);
        assert_eq!(r, Rect::new(40.0, 20.0, 20.0, 10.0));
    }
}
