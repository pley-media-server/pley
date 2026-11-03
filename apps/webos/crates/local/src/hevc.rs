//! What a film's first HEVC frames say of its picture, whatever its
//! container left out: a muxer that writes no colour, an older one that
//! writes no Dolby Vision configuration. The NAL units are length-prefixed,
//! as Matroska and MP4 hold them; SEI messages are walked as the standard
//! lays them out (types and sizes in 255 steps, emulation bytes taken out),
//! not matched as bytes.

use pley_core::model::{DynamicRange, MediaStream};

const PREFIX_SEI: u8 = 39;
const SUFFIX_SEI: u8 = 40;
const DOLBY_RPU: u8 = 62;
const DOLBY_EL: u8 = 63;
const MASTERING_DISPLAY: usize = 137;
const LIGHT_LEVEL: usize = 144;
const ALTERNATIVE_TRANSFER: usize = 147;
const REGISTERED_T35: usize = 4;

/// The most of a unit read for its SEI messages: HDR10+'s, the largest
/// looked for, is a few hundred bytes.
const SEI_MOST: usize = 64 * 1024;
/// The most bytes of SEI walked in one scan, so bytes that only look like
/// units can't make it slow.
const SEI_BUDGET: usize = 16 * 1024 * 1024;

/// What the first frames say of the picture.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HevcMarks {
    /// HDR10's static metadata (mastering display, light levels): a PQ picture.
    pub mastering: bool,
    /// The alternative transfer message names HLG.
    pub hlg: bool,
    /// HDR10+'s dynamic metadata.
    pub hdr10plus: bool,
    /// Dolby Vision's RPU, in the frames.
    pub dolby_vision: bool,
    /// Dolby Vision's enhancement layer in the same track: profile 7.
    pub enhancement_layer: bool,
}

impl HevcMarks {
    /// Whether the frames say anything of the picture.
    pub fn any(&self) -> bool {
        self.mastering || self.hlg || self.hdr10plus || self.dolby_vision
    }
}

/// The marks in `bytes`, a stretch of the file from its first frames.
pub fn scan(bytes: &[u8]) -> HevcMarks {
    let mut m = HevcMarks::default();
    let mut el = false;
    let mut budget = SEI_BUDGET;
    let mut payload = Vec::new();
    // A unit is its 4-byte length, then its 2-byte header: type in bits 1-6,
    // layer and temporal id after. The scan starts in the container's own
    // bytes, so every offset is tried.
    for i in 0..bytes.len().saturating_sub(5) {
        let Some(&[l0, l1, l2, l3, header, temporal]) = bytes.get(i..i + 6) else { break };
        if header & 0x80 != 0 || temporal & 7 == 0 {
            continue;
        }
        let kind = (header >> 1) & 0x3F;
        if !matches!(kind, PREFIX_SEI | SUFFIX_SEI | DOLBY_RPU | DOLBY_EL) {
            continue;
        }
        let length = u32::from_be_bytes([l0, l1, l2, l3]) as usize;
        if !(3..=1 << 20).contains(&length) {
            continue;
        }
        let end = bytes.len().min(i + 4 + length);
        match kind {
            // Its payload opens with the RPU's prefix, 25.
            DOLBY_RPU => m.dolby_vision |= bytes.get(i + 6) == Some(&0x19),
            DOLBY_EL => el = true,
            _ if budget > 0 => {
                let to = end.min(i + 6 + SEI_MOST);
                budget = budget.saturating_sub(to.saturating_sub(i + 6));
                unescape(bytes.get(i + 6..to).unwrap_or_default(), &mut payload);
                sei(&payload, &mut m);
            }
            _ => {}
        }
    }
    m.enhancement_layer = el && m.dolby_vision;
    m
}

fn sei(p: &[u8], m: &mut HevcMarks) {
    let mut at = 0;
    // Messages until the trailing bits (0x80) or the end.
    while let Some(&first) = p.get(at)
        && first != 0x80
    {
        let Some(kind) = sei_number(p, &mut at) else { return };
        let Some(size) = sei_number(p, &mut at) else { return };
        let Some(body) = at.checked_add(size).and_then(|end| p.get(at..end)) else { return };
        match kind {
            MASTERING_DISPLAY if size == 24 => m.mastering = true,
            LIGHT_LEVEL if size == 4 => m.mastering = true,
            ALTERNATIVE_TRANSFER if body.first() == Some(&18) => m.hlg = true,
            // From the USA (0xB5), Samsung's (0x003C), HDR10+'s own (0x0001, application 4).
            REGISTERED_T35 if body.starts_with(&[0xB5, 0x00, 0x3C, 0x00, 0x01, 0x04]) => m.hdr10plus = true,
            _ => {}
        }
        at += size;
    }
}

/// An SEI message's type or size: 255 for each 0xFF, then the last byte.
fn sei_number(p: &[u8], at: &mut usize) -> Option<usize> {
    let mut n = 0usize;
    loop {
        let b = *p.get(*at)?;
        *at += 1;
        n = n.saturating_add(usize::from(b));
        if b != 0xFF {
            return Some(n);
        }
    }
}

/// A unit's payload without the emulation bytes (00 00 03 becomes 00 00).
fn unescape(b: &[u8], out: &mut Vec<u8>) {
    out.clear();
    let mut zeros = 0;
    for &x in b {
        if zeros >= 2 && x == 3 {
            zeros = 0;
            continue;
        }
        zeros = if x == 0 { zeros + 1 } else { 0 };
        out.push(x);
    }
}

/// `video` with what its frames say added to what its container said: a
/// container's word stands where it gave one; the frames fill in the rest.
pub fn apply(video: &MediaStream, m: &HevcMarks) -> MediaStream {
    let transfer = video.color_transfer.clone().or_else(|| {
        if m.mastering || m.hdr10plus || (m.dolby_vision && !m.hlg) {
            Some("smpte2084".into())
        } else if m.hlg {
            Some("arib-std-b67".into())
        } else {
            None
        }
    });
    let dv = video.dv_profile.is_some() || m.dolby_vision;
    // Without a configuration record, the profile follows from the layers: an
    // enhancement layer is 7, a PQ base 8.1, an HLG base 8.4, no base metadata 5.
    let mut profile = video.dv_profile.or(m.dolby_vision.then_some(if m.enhancement_layer { 7 } else { 8 }));
    let compat = video.dv_bl_compat.or(m.dolby_vision.then_some(if m.enhancement_layer {
        6
    } else if m.hlg {
        4
    } else if m.mastering || m.hdr10plus {
        1
    } else {
        0
    }));
    if profile == Some(8) && compat == Some(0) {
        profile = Some(5);
    }
    let plus = video.hdr10plus == Some(true) || m.hdr10plus;
    let range = match transfer.as_deref() {
        _ if dv => DynamicRange::DolbyVision,
        Some("smpte2084") if plus => DynamicRange::Hdr10Plus,
        Some("smpte2084") => DynamicRange::Hdr10,
        Some("arib-std-b67") => DynamicRange::Hlg,
        _ => video.dynamic_range.unwrap_or(DynamicRange::Sdr),
    };
    let wide = matches!(transfer.as_deref(), Some("smpte2084" | "arib-std-b67"));
    MediaStream {
        color_primaries: video.color_primaries.clone().or(wide.then(|| "bt2020".into())),
        color_transfer: transfer,
        dynamic_range: Some(range),
        hdr: Some(range != DynamicRange::Sdr),
        hdr10plus: if plus { Some(true) } else { video.hdr10plus },
        dv_profile: if dv { profile } else { None },
        dv_bl_compat: if dv { compat } else { None },
        ..video.clone()
    }
}
