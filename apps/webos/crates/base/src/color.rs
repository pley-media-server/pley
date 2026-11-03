//! Colours as straight (not premultiplied) RGBA, 8 bits a channel.

/// A colour. The renderer premultiplies when it draws.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const TRANSPARENT: Color = Color { r: 0, g: 0, b: 0, a: 0 };
    pub const BLACK: Color = Color::rgb(0x000000);
    pub const WHITE: Color = Color::rgb(0xffffff);

    /// An opaque colour from `0xRRGGBB`, as the design system writes them.
    pub const fn rgb(hex: u32) -> Color {
        Color { r: (hex >> 16) as u8, g: (hex >> 8) as u8, b: hex as u8, a: 0xff }
    }

    /// A colour from `0xRRGGBBAA`.
    pub const fn rgba(hex: u32) -> Color {
        Color { r: (hex >> 24) as u8, g: (hex >> 16) as u8, b: (hex >> 8) as u8, a: hex as u8 }
    }

    /// This colour with its alpha multiplied by `f` (0 to 1): "Bg @ 60 %" is
    /// `BG.alpha(0.6)`.
    pub fn alpha(self, f: f32) -> Color {
        Color { a: (self.a as f32 * f.clamp(0.0, 1.0)).round() as u8, ..self }
    }

    /// The colour `t` of the way from `self` to `to`, channel by channel.
    pub fn lerp(self, to: Color, t: f32) -> Color {
        let t = t.clamp(0.0, 1.0);
        let mix = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t).round() as u8;
        Color { r: mix(self.r, to.r), g: mix(self.g, to.g), b: mix(self.b, to.b), a: mix(self.a, to.a) }
    }

    pub fn is_transparent(&self) -> bool {
        self.a == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_forms() {
        assert_eq!(Color::rgb(0x38bdf8), Color { r: 0x38, g: 0xbd, b: 0xf8, a: 0xff });
        assert_eq!(Color::rgba(0x0b0d10cc), Color { r: 0x0b, g: 0x0d, b: 0x10, a: 0xcc });
    }

    #[test]
    fn alpha_scales() {
        assert_eq!(Color::rgb(0x0b0d10).alpha(0.6).a, 153);
        assert_eq!(Color::rgb(0x0b0d10).alpha(0.8), Color::rgba(0x0b0d10cc));
    }

    #[test]
    fn lerp_ends() {
        let a = Color::BLACK;
        let b = Color::WHITE;
        assert_eq!(a.lerp(b, 0.0), a);
        assert_eq!(a.lerp(b, 1.0), b);
    }
}
