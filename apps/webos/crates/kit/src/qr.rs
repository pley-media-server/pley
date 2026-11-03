//! QR codes (`OnboardingKit.kt`; the TV app's `onboarding/kit.tsx`):
//! the text in byte mode with error correction M, raised to Q or H when that fits in the same
//! version (qrcodegen's `boostecl`, MIT), drawn black on white as a picture at the screen's pixels:
//! each module a whole number of pixels (the tile's pixels over the modules plus the 4-module quiet
//! zone either side), the code centred in the tile, the quiet zone and what is left white.

use std::hash::{Hash, Hasher};
use std::sync::Arc;

use pley_base::{Cmd, Rect, RgbaImage};
use pley_ui::Ui;
use pley_ui::theme;
use qrcodegen::{QrCode, QrCodeEcc};

/// Why a text can't be a QR code.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum QrError {
    /// More than a version 40 code holds.
    TooLong,
}

impl std::fmt::Display for QrError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("too long for a QR code")
    }
}

impl std::error::Error for QrError {}

/// A QR code, with the last picture made of it (kept by the screen while the code shows).
#[derive(Clone, Debug)]
pub struct Qr {
    size: usize,
    modules: Vec<bool>,
    ecc: QrCodeEcc,
    key: u64,
    picture: Option<(u32, Arc<RgbaImage>, u64)>,
}

/// The quiet zone, in modules, either side.
pub const QUIET_ZONE: usize = 4;

impl Qr {
    /// The code of `text`.
    pub fn new(text: &str) -> Result<Qr, QrError> {
        let code = QrCode::encode_binary(text.as_bytes(), QrCodeEcc::Medium).map_err(|_| QrError::TooLong)?;
        let size = code.size() as usize;
        let modules = (0..size * size).map(|i| code.get_module((i % size) as i32, (i / size) as i32)).collect();
        let mut h = std::collections::hash_map::DefaultHasher::new();
        ("kit.qr", text).hash(&mut h);
        Ok(Qr { size, modules, ecc: code.error_correction_level(), key: h.finish(), picture: None })
    }

    /// Modules along a side, without the quiet zone.
    pub fn modules(&self) -> usize {
        self.size
    }

    /// Whether the module at (`x`, `y`) is dark.
    pub fn dark(&self, x: usize, y: usize) -> bool {
        x < self.size && y < self.size && self.modules[y * self.size + x]
    }

    /// The error correction the code ended with: 0 low, 1 medium, 2 quartile, 3 high.
    pub fn error_correction(&self) -> u8 {
        match self.ecc {
            QrCodeEcc::Low => 0,
            QrCodeEcc::Medium => 1,
            QrCodeEcc::Quartile => 2,
            QrCodeEcc::High => 3,
        }
    }

    /// The pixels a module takes in a tile `px` pixels square (at least one).
    pub fn module_px(&self, px: u32) -> u32 {
        (px / (self.size + 2 * QUIET_ZONE) as u32).max(1)
    }

    /// The code as a white square `px` pixels across, and the key naming its content.
    pub fn picture(&mut self, px: u32) -> (Arc<RgbaImage>, u64) {
        if let Some((p, img, key)) = &self.picture
            && *p == px
        {
            return (img.clone(), *key);
        }
        let m = self.module_px(px);
        let code = m * self.size as u32;
        let offset = px.saturating_sub(code) / 2;
        let mut img = RgbaImage::new(px, px);
        for (i, pixel) in img.pixels.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            let (x, y) = (i as u32 % px, i as u32 / px);
            let inside = |v: u32| v >= offset && v < offset + code;
            let dark = inside(x) && inside(y) && self.dark(((x - offset) / m) as usize, ((y - offset) / m) as usize);
            let c = if dark { theme::QR_DARK } else { theme::QR_LIGHT };
            *pixel = [c.r, c.g, c.b, 255];
        }
        let key = self.key ^ (px as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15);
        let img = Arc::new(img);
        self.picture = Some((px, img.clone(), key));
        (img, key)
    }

    /// Draws the code filling `rect` (at the screen's pixels), its corners rounded by `corner`, at
    /// `opacity` (40 % once the sign-in code expired).
    pub fn show(&mut self, ui: &mut Ui, rect: Rect, corner: f32, opacity: f32) {
        let px = ui.image_px(rect.w);
        let (image, key) = self.picture(px);
        ui.cmd(Cmd::Raw { rect, image, key, radius: corner, opacity });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_short_address_raises_its_error_correction() {
        // Never below M; a side is 17 + 4 × the version.
        let qr = Qr::new("http://192.168.1.20:7539/link?code=BCDFGHJK").unwrap();
        assert!(qr.error_correction() >= 1);
        assert_eq!(qr.modules() % 4, 1);
        // Two bytes fit version 1 even at H, so H it is.
        let short = Qr::new("ab").unwrap();
        assert_eq!((short.modules(), short.error_correction()), (21, 3));
        assert_eq!(Qr::new(&"x".repeat(3000)).unwrap_err(), QrError::TooLong);
    }

    #[test]
    fn the_picture_has_a_quiet_zone_and_whole_pixel_modules() {
        let mut qr = Qr::new("ab").unwrap();
        // 268 dp at 2 px a dp: 536 px over 21 + 8 modules is 18 px a module, the code 378 px.
        let (img, _) = qr.picture(536);
        assert_eq!(qr.module_px(536), 18);
        let offset = (536 - 18 * 21) / 2;
        let at = |x: u32, y: u32| img.pixels[((y * 536 + x) * 4) as usize];
        // The quiet zone is white; the finder pattern's corner module is dark.
        assert_eq!(at(offset - 1, offset), 255);
        assert_eq!(at(offset, offset), 0);
        assert_eq!(at(offset + 17, offset + 17), 0);
        // The finder's white ring (its second module).
        assert_eq!(at(offset + 18, offset + 18), 255);
        assert!(qr.dark(0, 0) && !qr.dark(1, 1) && qr.dark(2, 2));
    }
}
