// SPDX-License-Identifier: AGPL-3.0-only
//! Dolby Vision and HDR10+ metadata inside HEVC NAL units, for remuxes:
//!
//! - [`rpu_to_p81`] rewrites a profile 7 RPU (NAL unit type 62) as a
//!   profile 8.1 one, as `dovi_tool`'s mode 2 does: no residual, no
//!   enhancement-layer resampling, no NLQ, the P8.1 colour coefficients,
//!   and for a full enhancement layer a no-op mapping (its curves only make
//!   sense together with the layer). The enhancement layer itself (type 63)
//!   is simply left out by the caller.
//! - [`strip_hdr10plus`] takes the HDR10+ messages (ITU-T T.35, SMPTE ST
//!   2094-40) out of a prefix SEI NAL unit and keeps the others (mastering
//!   display, content light level).
//!
//! FFmpeg can't do either: its `dovi_rpu` filter rewrites RPUs but refuses
//! profile 7, `dovi_split` drops the enhancement layer but keeps profile 7
//! RPUs, and `filter_units` can only drop whole SEI NAL units. The RPU
//! syntax is the one FFmpeg (libavcodec/dovi_rpudec.c) and `dovi_tool`
//! (MIT) read; nothing of theirs is linked.

use anyhow::{Context, bail, ensure};

/// HEVC NAL unit types.
pub const NAL_PREFIX_SEI: u8 = 39;
pub const NAL_DV_RPU: u8 = 62;
pub const NAL_DV_EL: u8 = 63;

/// The type of an HEVC NAL unit (its first byte, without start code).
pub fn nal_type(nal: &[u8]) -> Option<u8> {
    nal.first().map(|b| (b >> 1) & 0x3f)
}

/// What becomes of a NAL unit.
#[derive(Debug, PartialEq)]
pub enum NalEdit {
    Keep,
    Drop,
    Replace(Vec<u8>),
}

/// RBSP bytes of a NAL payload: emulation prevention bytes (`00 00 03`)
/// taken out.
pub fn unescape(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len());
    let mut zeros = 0;
    for &b in data {
        if zeros >= 2 && b == 3 {
            zeros = 0;
            continue;
        }
        out.push(b);
        zeros = if b == 0 { zeros + 1 } else { 0 };
    }
    out
}

/// NAL payload bytes of an RBSP: `03` put in wherever two zero bytes are
/// followed by a byte up to 3 (or end the payload).
pub fn escape(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() + data.len() / 64 + 2);
    let mut zeros = 0;
    for &b in data {
        if zeros >= 2 && b <= 3 {
            out.push(3);
            zeros = 0;
        }
        out.push(b);
        zeros = if b == 0 { zeros + 1 } else { 0 };
    }
    if out.last() == Some(&0) {
        out.push(3);
    }
    out
}

// ------------------------------------------------------------------ HDR10+

/// The start of an HDR10+ message's payload: ITU-T T.35 country code (USA),
/// provider code (Samsung), provider oriented code, application identifier 4.
const HDR10PLUS_T35: [u8; 6] = [0xB5, 0x00, 0x3C, 0x00, 0x01, 0x04];
/// SEI payload type `user_data_registered_itu_t_t35`.
const SEI_T35: u64 = 4;

/// A prefix SEI NAL unit without its HDR10+ messages: unchanged when it has
/// none, dropped when they were all it had. One that can't be read is kept.
pub fn strip_hdr10plus(nal: &[u8]) -> NalEdit {
    if nal.len() < 3 || nal_type(nal) != Some(NAL_PREFIX_SEI) {
        return NalEdit::Keep;
    }
    let rbsp = unescape(&nal[2..]);
    let Some(messages) = sei_messages(&rbsp) else { return NalEdit::Keep };
    let is_hdr10plus = |m: &SeiMessage| m.kind == SEI_T35 && rbsp[m.payload.clone()].starts_with(&HDR10PLUS_T35);
    if !messages.iter().any(&is_hdr10plus) {
        return NalEdit::Keep;
    }
    let kept: Vec<&SeiMessage> = messages.iter().filter(|m| !is_hdr10plus(m)).collect();
    if kept.is_empty() {
        return NalEdit::Drop;
    }
    let mut out = Vec::with_capacity(rbsp.len());
    for m in kept {
        out.extend_from_slice(&rbsp[m.whole.clone()]);
    }
    out.push(0x80);
    let mut nal_out = nal[..2].to_vec();
    nal_out.extend(escape(&out));
    NalEdit::Replace(nal_out)
}

struct SeiMessage {
    kind: u64,
    /// The message, its type and size included.
    whole: std::ops::Range<usize>,
    payload: std::ops::Range<usize>,
}

/// The messages of an SEI RBSP, or `None` when it doesn't read right.
fn sei_messages(rbsp: &[u8]) -> Option<Vec<SeiMessage>> {
    let mut messages = Vec::new();
    let mut i = 0;
    // Until the RBSP trailing bits (0x80, then zeros at most).
    while i < rbsp.len() && !(rbsp[i] == 0x80 && rbsp[i + 1..].iter().all(|&b| b == 0)) {
        let start = i;
        let mut read = || {
            let mut value = 0u64;
            loop {
                let b = *rbsp.get(i)?;
                i += 1;
                value += u64::from(b);
                if b != 0xFF {
                    return Some(value);
                }
            }
        };
        let kind = read()?;
        let size = read()? as usize;
        let end = i.checked_add(size).filter(|&e| e <= rbsp.len())?;
        messages.push(SeiMessage { kind, whole: start..end, payload: i..end });
        i = end;
    }
    Some(messages)
}

// ------------------------------------------------------------------ bits

struct BitReader<'a> {
    data: &'a [u8],
    /// In bits.
    pos: usize,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn bit(&mut self) -> anyhow::Result<bool> {
        let byte = *self.data.get(self.pos / 8).context("the RPU ends early")?;
        let bit = (byte >> (7 - self.pos % 8)) & 1 == 1;
        self.pos += 1;
        Ok(bit)
    }

    fn bits(&mut self, n: u32) -> anyhow::Result<u64> {
        ensure!(n <= 64, "a field of {n} bits");
        let mut value = 0u64;
        for _ in 0..n {
            value = (value << 1) | u64::from(self.bit()?);
        }
        Ok(value)
    }

    fn flag(&mut self) -> anyhow::Result<bool> {
        self.bit()
    }

    /// Unsigned Exp-Golomb.
    fn ue(&mut self) -> anyhow::Result<u64> {
        let mut zeros = 0;
        while !self.bit()? {
            zeros += 1;
            ensure!(zeros <= 32, "an Exp-Golomb value too long");
        }
        Ok((1u64 << zeros) - 1 + self.bits(zeros)?)
    }

    /// Signed Exp-Golomb.
    fn se(&mut self) -> anyhow::Result<i64> {
        let k = self.ue()? as i64;
        Ok(if k % 2 == 1 { (k + 1) / 2 } else { -(k / 2) })
    }

    /// Reads up to the next byte boundary; those bits must be zero.
    fn align_zero(&mut self) -> anyhow::Result<()> {
        while !self.pos.is_multiple_of(8) {
            ensure!(!self.bit()?, "alignment bits that aren't zero");
        }
        Ok(())
    }
}

#[derive(Default)]
struct BitWriter {
    bytes: Vec<u8>,
    /// Bits written in all.
    len: usize,
}

impl BitWriter {
    fn bit(&mut self, bit: bool) {
        if self.len.is_multiple_of(8) {
            self.bytes.push(0);
        }
        if bit {
            let last = self.bytes.len() - 1;
            self.bytes[last] |= 1 << (7 - self.len % 8);
        }
        self.len += 1;
    }

    fn bits(&mut self, n: u32, value: u64) {
        for i in (0..n).rev() {
            self.bit((value >> i) & 1 == 1);
        }
    }

    fn flag(&mut self, flag: bool) {
        self.bit(flag);
    }

    fn ue(&mut self, value: u64) {
        let x = value + 1;
        let len = 64 - x.leading_zeros();
        self.bits(len - 1, 0);
        self.bits(len, x);
    }

    fn se(&mut self, value: i64) {
        let k = if value > 0 { 2 * value - 1 } else { -2 * value };
        self.ue(k as u64);
    }

    fn align_zero(&mut self) {
        while !self.len.is_multiple_of(8) {
            self.bit(false);
        }
    }
}

/// MPEG-2 CRC-32, which RPUs end with.
fn crc32_mpeg2(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        crc ^= u32::from(b) << 24;
        for _ in 0..8 {
            crc = if crc & 0x8000_0000 != 0 { (crc << 1) ^ 0x04C1_1DB7 } else { crc << 1 };
        }
    }
    crc
}

// ------------------------------------------------------------------ RPU syntax

/// The first byte of an RPU's payload.
const RPU_PREFIX: u8 = 0x19;
const RPU_FINAL_BYTE: u8 = 0x80;

#[derive(Debug, Clone, PartialEq)]
struct Header {
    rpu_type: u8,
    rpu_format: u16,
    vdr_rpu_profile: u8,
    vdr_rpu_level: u8,
    seq: Option<SeqInfo>,
    vdr_dm_metadata_present: bool,
    use_prev_vdr_rpu: bool,
    prev_vdr_rpu_id: u64,
}

#[derive(Debug, Clone, PartialEq)]
struct SeqInfo {
    chroma_resampling_explicit_filter: bool,
    coefficient_data_type: u8,
    coefficient_log2_denom: u64,
    vdr_rpu_normalized_idc: u8,
    bl_video_full_range: bool,
    /// Present when `rpu_format & 0x700 == 0` (always, in practice).
    depths: Option<Depths>,
}

#[derive(Debug, Clone, PartialEq)]
struct Depths {
    bl_bit_depth_minus8: u64,
    /// With `ext_mapping_idc` in the bits above the low 8, as it is coded.
    el_bit_depth_minus8: u64,
    vdr_bit_depth_minus8: u64,
    spatial_resampling_filter: bool,
    /// `reserved_zero_3bits` to `dovi_tool`: 1 means compressed DM data.
    dm_compression: u8,
    el_spatial_resampling_filter: bool,
    disable_residual: bool,
}

impl SeqInfo {
    /// Bits of a coefficient's fractional part (all of a float one).
    fn denom_bits(&self) -> anyhow::Result<u32> {
        match self.coefficient_data_type {
            0 => {
                ensure!(
                    self.coefficient_log2_denom <= 32,
                    "a coefficient denominator of 2^{}",
                    self.coefficient_log2_denom
                );
                Ok(self.coefficient_log2_denom as u32)
            }
            1 => Ok(32),
            other => bail!("coefficient data type {other}"),
        }
    }

    fn fixed(&self) -> bool {
        self.coefficient_data_type == 0
    }
}

impl Header {
    fn read(r: &mut BitReader) -> anyhow::Result<Header> {
        let rpu_type = r.bits(6)? as u8;
        let rpu_format = r.bits(11)? as u16;
        let vdr_rpu_profile = r.bits(4)? as u8;
        let vdr_rpu_level = r.bits(4)? as u8;
        let seq = if r.flag()? {
            let chroma_resampling_explicit_filter = r.flag()?;
            let coefficient_data_type = r.bits(2)? as u8;
            let coefficient_log2_denom = if coefficient_data_type == 0 { r.ue()? } else { 0 };
            let vdr_rpu_normalized_idc = r.bits(2)? as u8;
            let bl_video_full_range = r.flag()?;
            let depths = if rpu_format & 0x700 == 0 {
                Some(Depths {
                    bl_bit_depth_minus8: r.ue()?,
                    el_bit_depth_minus8: r.ue()?,
                    vdr_bit_depth_minus8: r.ue()?,
                    spatial_resampling_filter: r.flag()?,
                    dm_compression: r.bits(3)? as u8,
                    el_spatial_resampling_filter: r.flag()?,
                    disable_residual: r.flag()?,
                })
            } else {
                None
            };
            Some(SeqInfo {
                chroma_resampling_explicit_filter,
                coefficient_data_type,
                coefficient_log2_denom,
                vdr_rpu_normalized_idc,
                bl_video_full_range,
                depths,
            })
        } else {
            None
        };
        let vdr_dm_metadata_present = r.flag()?;
        let use_prev_vdr_rpu = r.flag()?;
        let prev_vdr_rpu_id = if use_prev_vdr_rpu { r.ue()? } else { 0 };
        Ok(Header {
            rpu_type,
            rpu_format,
            vdr_rpu_profile,
            vdr_rpu_level,
            seq,
            vdr_dm_metadata_present,
            use_prev_vdr_rpu,
            prev_vdr_rpu_id,
        })
    }

    fn write(&self, w: &mut BitWriter) {
        w.bits(6, self.rpu_type.into());
        w.bits(11, self.rpu_format.into());
        w.bits(4, self.vdr_rpu_profile.into());
        w.bits(4, self.vdr_rpu_level.into());
        w.flag(self.seq.is_some());
        if let Some(seq) = &self.seq {
            w.flag(seq.chroma_resampling_explicit_filter);
            w.bits(2, seq.coefficient_data_type.into());
            if seq.coefficient_data_type == 0 {
                w.ue(seq.coefficient_log2_denom);
            }
            w.bits(2, seq.vdr_rpu_normalized_idc.into());
            w.flag(seq.bl_video_full_range);
            if let Some(d) = &seq.depths {
                w.ue(d.bl_bit_depth_minus8);
                w.ue(d.el_bit_depth_minus8);
                w.ue(d.vdr_bit_depth_minus8);
                w.flag(d.spatial_resampling_filter);
                w.bits(3, d.dm_compression.into());
                w.flag(d.el_spatial_resampling_filter);
                w.flag(d.disable_residual);
            }
        }
        w.flag(self.vdr_dm_metadata_present);
        w.flag(self.use_prev_vdr_rpu);
        if self.use_prev_vdr_rpu {
            w.ue(self.prev_vdr_rpu_id);
        }
    }

    /// Residual data (NLQ) follows the mapping.
    fn uses_nlq(&self) -> bool {
        self.rpu_format & 0x700 == 0 && self.depths().is_some_and(|d| !d.disable_residual)
    }

    fn depths(&self) -> Option<&Depths> {
        self.seq.as_ref().and_then(|s| s.depths.as_ref())
    }
}

/// A coefficient: an integer part (fixed point only) and a fraction of
/// `denom_bits` bits (a whole IEEE float when the data type is 1).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
struct Coef {
    int: i64,
    frac: u64,
}

impl Coef {
    fn read_signed(r: &mut BitReader, seq: &SeqInfo) -> anyhow::Result<Coef> {
        let int = if seq.fixed() { r.se()? } else { 0 };
        Ok(Coef { int, frac: r.bits(seq.denom_bits()?)? })
    }

    fn read_unsigned(r: &mut BitReader, seq: &SeqInfo) -> anyhow::Result<Coef> {
        let int = if seq.fixed() { r.ue()? as i64 } else { 0 };
        Ok(Coef { int, frac: r.bits(seq.denom_bits()?)? })
    }

    fn write_signed(&self, w: &mut BitWriter, seq: &SeqInfo) -> anyhow::Result<()> {
        if seq.fixed() {
            w.se(self.int);
        }
        w.bits(seq.denom_bits()?, self.frac);
        Ok(())
    }

    /// 0 and 1 as the data type codes them.
    fn zero_and_one(seq: &SeqInfo) -> (Coef, Coef) {
        if seq.fixed() {
            (Coef { int: 0, frac: 0 }, Coef { int: 1, frac: 0 })
        } else {
            (Coef { int: 0, frac: 0 }, Coef { int: 0, frac: u64::from(1.0f32.to_bits()) })
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Piece {
    Polynomial { order_minus1: u64, coefs: Vec<Coef> },
    Mmr { order_minus1: u8, constant: Coef, coefs: Vec<Coef> },
}

#[derive(Debug, Clone, PartialEq)]
struct Curve {
    /// As coded: each one relative to the one before.
    pivots: Vec<u64>,
    pieces: Vec<Piece>,
}

#[derive(Debug, Clone, PartialEq)]
struct Nlq {
    offset: u64,
    vdr_in_max: Coef,
    /// Linear dead zone slope and threshold.
    dead_zone: (Coef, Coef),
}

#[derive(Debug, Clone, PartialEq)]
struct Mapping {
    vdr_rpu_id: u64,
    mapping_color_space: u64,
    mapping_chroma_format_idc: u64,
    curves: Vec<Curve>,
    /// The NLQ method and its two pivots, with residual data.
    nlq_header: Option<(u8, [u64; 2])>,
    num_x_partitions_minus1: u64,
    num_y_partitions_minus1: u64,
    nlq: Option<Vec<Nlq>>,
}

/// Reshaping pieces a curve may have (FFmpeg's `AV_DOVI_MAX_PIECES`).
const MAX_PIECES: u64 = 8;

impl Mapping {
    fn read(r: &mut BitReader, header: &Header) -> anyhow::Result<Mapping> {
        let seq = header.seq.as_ref().context("an RPU without sequence information")?;
        let depths = header.depths().context("an RPU without bit depths")?;
        let bl_bits = (depths.bl_bit_depth_minus8 + 8) as u32;
        let el_bits = ((depths.el_bit_depth_minus8 & 0xFF) + 8) as u32;
        ensure!(bl_bits <= 16 && el_bits <= 16, "bit depths of {bl_bits} and {el_bits}");
        let vdr_rpu_id = r.ue()?;
        let mapping_color_space = r.ue()?;
        let mapping_chroma_format_idc = r.ue()?;
        let mut pivots = Vec::with_capacity(3);
        for _ in 0..3 {
            let n = r.ue()?;
            ensure!(n < MAX_PIECES, "{} pivots", n + 2);
            pivots.push((0..n + 2).map(|_| r.bits(bl_bits)).collect::<anyhow::Result<Vec<u64>>>()?);
        }
        let nlq_header = if header.uses_nlq() {
            let method = r.bits(3)? as u8;
            ensure!(method == 0, "NLQ method {method}");
            Some((method, [r.bits(bl_bits)?, r.bits(bl_bits)?]))
        } else {
            None
        };
        let num_x_partitions_minus1 = r.ue()?;
        let num_y_partitions_minus1 = r.ue()?;
        let mut curves = Vec::with_capacity(3);
        for pivots in pivots {
            let mut pieces = Vec::with_capacity(pivots.len() - 1);
            for _ in 0..pivots.len() - 1 {
                pieces.push(match r.ue()? {
                    0 => {
                        let order_minus1 = r.ue()?;
                        ensure!(order_minus1 <= 1, "a polynomial of order {}", order_minus1 + 1);
                        if order_minus1 == 0 {
                            ensure!(!r.flag()?, "linear interpolation");
                        }
                        let coefs = (0..order_minus1 + 2)
                            .map(|_| Coef::read_signed(r, seq))
                            .collect::<anyhow::Result<Vec<Coef>>>()?;
                        Piece::Polynomial { order_minus1, coefs }
                    }
                    1 => {
                        let order_minus1 = r.bits(2)? as u8;
                        ensure!(order_minus1 <= 2, "an MMR of order {}", order_minus1 + 1);
                        let constant = Coef::read_signed(r, seq)?;
                        let coefs = (0..(usize::from(order_minus1) + 1) * 7)
                            .map(|_| Coef::read_signed(r, seq))
                            .collect::<anyhow::Result<Vec<Coef>>>()?;
                        Piece::Mmr { order_minus1, constant, coefs }
                    }
                    other => bail!("mapping method {other}"),
                });
            }
            curves.push(Curve { pivots, pieces });
        }
        let nlq = match nlq_header {
            Some(_) => Some(
                (0..3)
                    .map(|_| -> anyhow::Result<Nlq> {
                        let offset = r.bits(el_bits)?;
                        let vdr_in_max = Coef::read_unsigned(r, seq)?;
                        let slope = Coef::read_unsigned(r, seq)?;
                        let threshold = Coef::read_unsigned(r, seq)?;
                        Ok(Nlq { offset, vdr_in_max, dead_zone: (slope, threshold) })
                    })
                    .collect::<anyhow::Result<Vec<Nlq>>>()?,
            ),
            None => None,
        };
        Ok(Mapping {
            vdr_rpu_id,
            mapping_color_space,
            mapping_chroma_format_idc,
            curves,
            nlq_header,
            num_x_partitions_minus1,
            num_y_partitions_minus1,
            nlq,
        })
    }

    fn write(&self, w: &mut BitWriter, header: &Header) -> anyhow::Result<()> {
        let seq = header.seq.as_ref().context("an RPU without sequence information")?;
        let depths = header.depths().context("an RPU without bit depths")?;
        let bl_bits = (depths.bl_bit_depth_minus8 + 8) as u32;
        let el_bits = ((depths.el_bit_depth_minus8 & 0xFF) + 8) as u32;
        w.ue(self.vdr_rpu_id);
        w.ue(self.mapping_color_space);
        w.ue(self.mapping_chroma_format_idc);
        for curve in &self.curves {
            w.ue(curve.pivots.len() as u64 - 2);
            for &p in &curve.pivots {
                w.bits(bl_bits, p);
            }
        }
        if header.uses_nlq() {
            let (method, pivots) = self.nlq_header.context("residual data without its method")?;
            w.bits(3, method.into());
            w.bits(bl_bits, pivots[0]);
            w.bits(bl_bits, pivots[1]);
        }
        w.ue(self.num_x_partitions_minus1);
        w.ue(self.num_y_partitions_minus1);
        for curve in &self.curves {
            for piece in &curve.pieces {
                match piece {
                    Piece::Polynomial { order_minus1, coefs } => {
                        w.ue(0);
                        w.ue(*order_minus1);
                        if *order_minus1 == 0 {
                            w.flag(false);
                        }
                        for c in coefs {
                            c.write_signed(w, seq)?;
                        }
                    }
                    Piece::Mmr { order_minus1, constant, coefs } => {
                        w.ue(1);
                        w.bits(2, (*order_minus1).into());
                        constant.write_signed(w, seq)?;
                        for c in coefs {
                            c.write_signed(w, seq)?;
                        }
                    }
                }
            }
        }
        if header.uses_nlq() {
            for n in self.nlq.as_deref().context("residual data missing")? {
                w.bits(el_bits, n.offset);
                for c in [&n.vdr_in_max, &n.dead_zone.0, &n.dead_zone.1] {
                    if seq.fixed() {
                        w.ue(c.int as u64);
                    }
                    w.bits(seq.denom_bits()?, c.frac);
                }
            }
        }
        Ok(())
    }

    /// The enhancement layer only refines the picture (a minimal one,
    /// MEL): its residual is zero, with an input maximum of one.
    fn minimal_enhancement_layer(&self) -> bool {
        self.nlq.as_deref().is_some_and(|nlq| {
            nlq.iter().all(|n| {
                n.offset == 0
                    && n.vdr_in_max == Coef { int: 1, frac: 0 }
                    && n.dead_zone == (Coef::default(), Coef::default())
            })
        })
    }

    /// Curves that leave the base layer as it is: one linear piece, `y = x`.
    fn set_no_op(&mut self, seq: &SeqInfo, bl_bits: u32) {
        let (zero, one) = Coef::zero_and_one(seq);
        for curve in &mut self.curves {
            curve.pivots = vec![0, (1 << bl_bits) - 1];
            curve.pieces = vec![Piece::Polynomial { order_minus1: 0, coefs: vec![zero, one] }];
        }
    }
}

/// An extension block of the display management data: kept as it is.
#[derive(Debug, Clone, PartialEq)]
struct ExtBlock {
    level: u8,
    payload: Vec<u8>,
}

/// The fixed fields of uncompressed display management data.
#[derive(Debug, Clone, PartialEq)]
struct DmColour {
    ycc_to_rgb_coef: [u64; 9],
    ycc_to_rgb_offset: [u64; 3],
    rgb_to_lms_coef: [u64; 9],
    signal_eotf: u64,
    signal_eotf_param0: u64,
    signal_eotf_param1: u64,
    signal_eotf_param2: u64,
    signal_bit_depth: u64,
    signal_color_space: u64,
    signal_chroma_format: u64,
    signal_full_range_flag: u64,
    source_min_pq: u64,
    source_max_pq: u64,
    source_diagonal: u64,
}

/// Profile 8.1's colour coefficients (`dovi_tool`'s `set_p81_coeffs`):
/// BT.2020 YCbCr to RGB, and RGB to LMS.
const P81_YCC_TO_RGB: [i16; 9] = [9574, 0, 13802, 9574, -1540, -5348, 9574, 17610, 0];
const P81_YCC_TO_RGB_OFFSET: [u32; 3] = [16_777_216, 134_217_728, 134_217_728];
const P81_RGB_TO_LMS: [i16; 9] = [7222, 8771, 390, 2654, 12430, 1300, 0, 422, 15962];

#[derive(Debug, Clone, PartialEq)]
struct Dm {
    affected_dm_metadata_id: u64,
    current_dm_metadata_id: u64,
    scene_refresh_flag: u64,
    colour: Option<DmColour>,
    /// Content mapping version 2.9 blocks, then 4.0 ones (when present).
    cm_v29: Vec<ExtBlock>,
    cm_v40: Option<Vec<ExtBlock>>,
}

impl Dm {
    /// `end` is where the payload stops (the CRC starts), in bits.
    fn read(r: &mut BitReader, compressed: bool, end: usize) -> anyhow::Result<Dm> {
        let affected_dm_metadata_id = r.ue()?;
        let current_dm_metadata_id = r.ue()?;
        let scene_refresh_flag = r.ue()?;
        let colour = if compressed {
            None
        } else {
            let mut field = |n: u32| r.bits(n);
            let mut ycc_to_rgb_coef = [0; 9];
            for c in &mut ycc_to_rgb_coef {
                *c = field(16)?;
            }
            let mut ycc_to_rgb_offset = [0; 3];
            for c in &mut ycc_to_rgb_offset {
                *c = field(32)?;
            }
            let mut rgb_to_lms_coef = [0; 9];
            for c in &mut rgb_to_lms_coef {
                *c = field(16)?;
            }
            Some(DmColour {
                ycc_to_rgb_coef,
                ycc_to_rgb_offset,
                rgb_to_lms_coef,
                signal_eotf: field(16)?,
                signal_eotf_param0: field(16)?,
                signal_eotf_param1: field(16)?,
                signal_eotf_param2: field(32)?,
                signal_bit_depth: field(5)?,
                signal_color_space: field(2)?,
                signal_chroma_format: field(2)?,
                signal_full_range_flag: field(2)?,
                source_min_pq: field(12)?,
                source_max_pq: field(12)?,
                source_diagonal: field(10)?,
            })
        };
        let cm_v29 = ext_blocks(r)?;
        // More than alignment before the CRC: version 4.0 blocks.
        let cm_v40 = if end.saturating_sub(r.pos) >= 8 { Some(ext_blocks(r)?) } else { None };
        Ok(Dm { affected_dm_metadata_id, current_dm_metadata_id, scene_refresh_flag, colour, cm_v29, cm_v40 })
    }

    fn write(&self, w: &mut BitWriter) {
        w.ue(self.affected_dm_metadata_id);
        w.ue(self.current_dm_metadata_id);
        w.ue(self.scene_refresh_flag);
        if let Some(c) = &self.colour {
            for v in c.ycc_to_rgb_coef {
                w.bits(16, v);
            }
            for v in c.ycc_to_rgb_offset {
                w.bits(32, v);
            }
            for v in c.rgb_to_lms_coef {
                w.bits(16, v);
            }
            w.bits(16, c.signal_eotf);
            w.bits(16, c.signal_eotf_param0);
            w.bits(16, c.signal_eotf_param1);
            w.bits(32, c.signal_eotf_param2);
            w.bits(5, c.signal_bit_depth);
            w.bits(2, c.signal_color_space);
            w.bits(2, c.signal_chroma_format);
            w.bits(2, c.signal_full_range_flag);
            w.bits(12, c.source_min_pq);
            w.bits(12, c.source_max_pq);
            w.bits(10, c.source_diagonal);
        }
        write_ext_blocks(w, &self.cm_v29);
        if let Some(blocks) = &self.cm_v40 {
            write_ext_blocks(w, blocks);
        }
    }

    fn set_p81_colour(&mut self) {
        if let Some(c) = &mut self.colour {
            c.ycc_to_rgb_coef = P81_YCC_TO_RGB.map(|v| u64::from(v as u16));
            c.ycc_to_rgb_offset = P81_YCC_TO_RGB_OFFSET.map(u64::from);
            c.rgb_to_lms_coef = P81_RGB_TO_LMS.map(|v| u64::from(v as u16));
            c.signal_color_space = 0;
        }
    }
}

fn ext_blocks(r: &mut BitReader) -> anyhow::Result<Vec<ExtBlock>> {
    let count = r.ue()?;
    ensure!(count <= 255, "{count} extension blocks");
    if count > 0 {
        r.align_zero()?;
    }
    (0..count)
        .map(|_| {
            let length = r.ue()?;
            ensure!(length <= 4096, "an extension block of {length} bytes");
            let level = r.bits(8)? as u8;
            let payload = (0..length).map(|_| r.bits(8).map(|b| b as u8)).collect::<anyhow::Result<Vec<u8>>>()?;
            Ok(ExtBlock { level, payload })
        })
        .collect()
}

fn write_ext_blocks(w: &mut BitWriter, blocks: &[ExtBlock]) {
    w.ue(blocks.len() as u64);
    if !blocks.is_empty() {
        w.align_zero();
    }
    for b in blocks {
        w.ue(b.payload.len() as u64);
        w.bits(8, b.level.into());
        for &byte in &b.payload {
            w.bits(8, byte.into());
        }
    }
}

/// An RPU, read whole.
#[derive(Debug, Clone, PartialEq)]
struct Rpu {
    header: Header,
    mapping: Option<Mapping>,
    dm: Option<Dm>,
}

impl Rpu {
    /// Reads an RPU's RBSP (from its `0x19` prefix on). Only type 2 RPUs
    /// (all of profiles 4 to 9) are read: `None` for others.
    fn read(rbsp: &[u8]) -> anyhow::Result<Option<Rpu>> {
        let end = rbsp.iter().rposition(|&b| b != 0).map_or(0, |i| i + 1);
        let rbsp = &rbsp[..end];
        ensure!(rbsp.len() >= 6, "an RPU of {} bytes", rbsp.len());
        ensure!(rbsp[0] == RPU_PREFIX, "an RPU starting with {:#04x}", rbsp[0]);
        ensure!(rbsp[end - 1] == RPU_FINAL_BYTE, "an RPU ending with {:#04x}", rbsp[end - 1]);
        let crc_at = end - 5;
        let crc = u32::from_be_bytes(rbsp[crc_at..end - 1].try_into().expect("four bytes"));
        ensure!(crc32_mpeg2(&rbsp[1..crc_at]) == crc, "an RPU whose CRC doesn't match");
        let payload = &rbsp[..crc_at];
        let mut r = BitReader::new(payload);
        r.pos = 8;
        let header = Header::read(&mut r)?;
        if header.rpu_type != 2 {
            return Ok(None);
        }
        let mapping = if header.use_prev_vdr_rpu { None } else { Some(Mapping::read(&mut r, &header)?) };
        let dm = if header.vdr_dm_metadata_present {
            let compressed = header.depths().is_some_and(|d| d.dm_compression != 0);
            Some(Dm::read(&mut r, compressed, payload.len() * 8)?)
        } else {
            None
        };
        r.align_zero()?;
        ensure!(r.pos == payload.len() * 8, "{} bytes the RPU doesn't explain", payload.len() - r.pos / 8);
        Ok(Some(Rpu { header, mapping, dm }))
    }

    /// The RBSP: prefix, fields, alignment, CRC and final byte.
    fn write(&self) -> anyhow::Result<Vec<u8>> {
        let mut w = BitWriter::default();
        w.bits(8, RPU_PREFIX.into());
        self.header.write(&mut w);
        if let Some(mapping) = &self.mapping {
            mapping.write(&mut w, &self.header)?;
        }
        if let Some(dm) = &self.dm {
            dm.write(&mut w);
        }
        w.align_zero();
        let crc = crc32_mpeg2(&w.bytes[1..]);
        w.bits(32, crc.into());
        w.bits(8, RPU_FINAL_BYTE.into());
        Ok(w.bytes)
    }

    /// Profile 8.1 as `dovi_tool`'s mode 2 makes it. Whether anything changed.
    fn make_p81(&mut self) -> anyhow::Result<bool> {
        let seq = self.header.seq.clone().context("an RPU without sequence information")?;
        let Some(depths) = self.header.seq.as_mut().and_then(|s| s.depths.as_mut()) else {
            bail!("an RPU without bit depths");
        };
        if depths.disable_residual && !depths.el_spatial_resampling_filter {
            // Profile 8 already (or 5): nothing to do.
            return Ok(false);
        }
        depths.el_spatial_resampling_filter = false;
        depths.disable_residual = true;
        let bl_bits = (depths.bl_bit_depth_minus8 + 8) as u32;
        if let Some(mapping) = &mut self.mapping {
            let full_layer = mapping.nlq.is_some() && !mapping.minimal_enhancement_layer();
            mapping.nlq_header = None;
            mapping.nlq = None;
            mapping.num_x_partitions_minus1 = 0;
            mapping.num_y_partitions_minus1 = 0;
            if full_layer {
                mapping.set_no_op(&seq, bl_bits);
            }
        }
        if let Some(dm) = &mut self.dm {
            dm.set_p81_colour();
        }
        Ok(true)
    }
}

/// A profile 7 RPU NAL unit (type 62, with its two header bytes, escaped)
/// as profile 8.1; `Keep` when it is one already (or not an RPU this
/// reads), an error when it can't be read.
pub fn rpu_to_p81(nal: &[u8]) -> anyhow::Result<NalEdit> {
    ensure!(nal.len() > 2 && nal_type(nal) == Some(NAL_DV_RPU), "not an RPU NAL unit");
    let rbsp = unescape(&nal[2..]);
    let Some(mut rpu) = Rpu::read(&rbsp)? else { return Ok(NalEdit::Keep) };
    if !rpu.make_p81()? {
        return Ok(NalEdit::Keep);
    }
    let mut out = nal[..2].to_vec();
    out.extend(escape(&rpu.write()?));
    Ok(NalEdit::Replace(out))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Whether an RPU NAL unit reads, and writes back bit for bit.
    pub(crate) fn rewrites_as_read(nal: &[u8]) -> anyhow::Result<bool> {
        let Some(rpu) = Rpu::read(&unescape(&nal[2..]))? else { return Ok(false) };
        let mut again = nal[..2].to_vec();
        again.extend(escape(&rpu.write()?));
        // Trailing zero bytes (the stream's, not the RPU's) aside.
        let end = nal.iter().rposition(|&b| b != 0).map_or(0, |i| i + 1);
        Ok(again == nal[..end])
    }

    #[test]
    fn emulation_prevention_round_trips() {
        let raw = [0x19, 0, 0, 1, 0, 0, 0, 0, 0, 3, 7, 0, 0];
        let escaped = escape(&raw);
        assert_eq!(escaped, [0x19, 0, 0, 3, 1, 0, 0, 3, 0, 0, 3, 0, 3, 7, 0, 0, 3]);
        assert_eq!(unescape(&escaped), raw);
        assert_eq!(unescape(&[0, 0, 3, 0, 0, 3, 3]), [0, 0, 0, 0, 3]);
    }

    #[test]
    fn exp_golomb_and_bits_round_trip() {
        let mut w = BitWriter::default();
        for v in [0u64, 1, 2, 7, 8, 255, 65_288] {
            w.ue(v);
        }
        for v in [0i64, 1, -1, 5, -6, 1000] {
            w.se(v);
        }
        w.bits(11, 0x5A5);
        w.align_zero();
        let mut r = BitReader::new(&w.bytes);
        for v in [0u64, 1, 2, 7, 8, 255, 65_288] {
            assert_eq!(r.ue().unwrap(), v);
        }
        for v in [0i64, 1, -1, 5, -6, 1000] {
            assert_eq!(r.se().unwrap(), v);
        }
        assert_eq!(r.bits(11).unwrap(), 0x5A5);
        assert!(r.align_zero().is_ok());
        assert!(r.bit().is_err(), "the end");
        // The CRC every RPU ends with (MPEG-2: "123456789" → 0x0376E6E7).
        assert_eq!(crc32_mpeg2(b"123456789"), 0x0376_E6E7);
    }

    /// A profile 7 RPU as UHD Blu-rays carry it: polynomial luma, MMR
    /// chroma, residual data (`mel`: a minimal enhancement layer), display
    /// management with a level 1 and a level 6 block, and CM v4.0 blocks.
    pub(crate) fn p7_rpu(mel: bool) -> Vec<u8> {
        let seq = SeqInfo {
            chroma_resampling_explicit_filter: false,
            coefficient_data_type: 0,
            coefficient_log2_denom: 23,
            vdr_rpu_normalized_idc: 1,
            bl_video_full_range: false,
            depths: Some(Depths {
                bl_bit_depth_minus8: 2,
                el_bit_depth_minus8: 2,
                vdr_bit_depth_minus8: 4,
                spatial_resampling_filter: false,
                dm_compression: 0,
                el_spatial_resampling_filter: true,
                disable_residual: false,
            }),
        };
        let header = Header {
            rpu_type: 2,
            rpu_format: 18,
            vdr_rpu_profile: 1,
            vdr_rpu_level: 0,
            seq: Some(seq),
            vdr_dm_metadata_present: true,
            use_prev_vdr_rpu: false,
            prev_vdr_rpu_id: 0,
        };
        let c = |int: i64, frac: u64| Coef { int, frac };
        let luma = Curve {
            pivots: vec![63, 400, 451],
            pieces: vec![
                Piece::Polynomial { order_minus1: 1, coefs: vec![c(0, 1234), c(1, 77), c(-1, 8_000_000)] },
                Piece::Polynomial { order_minus1: 0, coefs: vec![c(0, 99), c(1, 3)] },
            ],
        };
        let chroma = Curve {
            pivots: vec![0, 1023],
            pieces: vec![Piece::Mmr {
                order_minus1: 2,
                constant: c(-1, 4_000_000),
                coefs: (0..21).map(|k| c(k % 3 - 1, (k as u64) * 1000)).collect(),
            }],
        };
        let nlq = |offset: u64, max: Coef, slope: Coef| Nlq {
            offset,
            vdr_in_max: max,
            dead_zone: (slope, if mel { c(0, 0) } else { c(0, 2048) }),
        };
        let residual = if mel {
            vec![nlq(0, c(1, 0), c(0, 0)); 3]
        } else {
            vec![nlq(512, c(1, 4096), c(2, 33)), nlq(511, c(1, 4095), c(1, 32)), nlq(510, c(1, 4094), c(1, 31))]
        };
        let mapping = Mapping {
            vdr_rpu_id: 0,
            mapping_color_space: 0,
            mapping_chroma_format_idc: 0,
            curves: vec![luma, chroma.clone(), chroma],
            nlq_header: Some((0, [0, 1023])),
            num_x_partitions_minus1: 0,
            num_y_partitions_minus1: 0,
            nlq: Some(residual),
        };
        let dm = Dm {
            affected_dm_metadata_id: 0,
            current_dm_metadata_id: 0,
            scene_refresh_flag: 1,
            colour: Some(DmColour {
                ycc_to_rgb_coef: [8192, 0, 12900, 8192, 64000, 61000, 8192, 15200, 0],
                ycc_to_rgb_offset: [1 << 28, 1 << 27, 1 << 27],
                rgb_to_lms_coef: [5845, 9702, 837, 2568, 12256, 1561, 0, 679, 15705],
                signal_eotf: 65535,
                signal_eotf_param0: 0,
                signal_eotf_param1: 0,
                signal_eotf_param2: 0,
                signal_bit_depth: 12,
                signal_color_space: 0,
                signal_chroma_format: 0,
                signal_full_range_flag: 1,
                source_min_pq: 62,
                source_max_pq: 3696,
                source_diagonal: 42,
            }),
            // Level 1 (min, max, average PQ) and level 6 (MaxCLL, MaxFALL…).
            cm_v29: vec![
                ExtBlock { level: 1, payload: vec![0x00, 0x00, 0x0B, 0x8E, 0x07, 0xE0] },
                ExtBlock { level: 6, payload: vec![0x03, 0xE8, 0x00, 0x32, 0x03, 0xE8, 0x01, 0x90] },
            ],
            cm_v40: Some(vec![ExtBlock { level: 254, payload: vec![0x00, 0x02] }]),
        };
        let rpu = Rpu { header, mapping: Some(mapping), dm: Some(dm) };
        let mut nal = vec![0x7C, 0x01];
        nal.extend(escape(&rpu.write().unwrap()));
        nal
    }

    fn read_nal(nal: &[u8]) -> Rpu {
        Rpu::read(&unescape(&nal[2..])).unwrap().unwrap()
    }

    #[test]
    fn rpus_read_back_as_written() {
        for mel in [true, false] {
            let nal = p7_rpu(mel);
            let rpu = read_nal(&nal);
            let mut again = vec![0x7C, 0x01];
            again.extend(escape(&rpu.write().unwrap()));
            assert_eq!(again, nal, "bit for bit");
            assert_eq!(rpu.mapping.as_ref().unwrap().minimal_enhancement_layer(), mel);
        }
    }

    #[test]
    fn profile_7_becomes_8_1() {
        // A minimal enhancement layer: its mapping stays.
        let before = read_nal(&p7_rpu(true));
        let NalEdit::Replace(nal) = rpu_to_p81(&p7_rpu(true)).unwrap() else { panic!("unchanged") };
        assert_eq!(&nal[..2], [0x7C, 0x01]);
        let after = read_nal(&nal);
        let depths = after.header.depths().unwrap();
        assert!(depths.disable_residual && !depths.el_spatial_resampling_filter);
        let (m0, m1) = (before.mapping.as_ref().unwrap(), after.mapping.as_ref().unwrap());
        assert_eq!(m1.curves, m0.curves, "the curves of a minimal layer apply to the base layer");
        assert_eq!((m1.nlq_header, &m1.nlq), (None, &None));
        let (dm0, dm1) = (before.dm.as_ref().unwrap(), after.dm.as_ref().unwrap());
        assert_eq!((&dm1.cm_v29, &dm1.cm_v40), (&dm0.cm_v29, &dm0.cm_v40), "levels 1, 6 and 254 kept");
        let colour = dm1.colour.as_ref().unwrap();
        assert_eq!(colour.ycc_to_rgb_coef[2], 13802);
        assert_eq!(colour.rgb_to_lms_coef[0], 7222);
        assert_eq!(colour.source_max_pq, 3696);
        assert!(nal.len() < p7_rpu(true).len(), "no residual data");
        // Already 8.1: left alone.
        assert_eq!(rpu_to_p81(&nal).unwrap(), NalEdit::Keep);

        // A full enhancement layer: its curves need it, they become no-ops.
        let NalEdit::Replace(nal) = rpu_to_p81(&p7_rpu(false)).unwrap() else { panic!("unchanged") };
        let after = read_nal(&nal);
        for curve in &after.mapping.as_ref().unwrap().curves {
            assert_eq!(curve.pivots, [0, 1023]);
            assert_eq!(
                curve.pieces,
                [Piece::Polynomial {
                    order_minus1: 0,
                    coefs: vec![Coef { int: 0, frac: 0 }, Coef { int: 1, frac: 0 }]
                }]
            );
        }
    }

    #[test]
    fn damaged_rpus_are_refused() {
        let mut nal = p7_rpu(true);
        let n = nal.len();
        nal[n / 2] ^= 0x10;
        assert!(rpu_to_p81(&nal).unwrap_err().to_string().contains("CRC"));
        assert!(rpu_to_p81(&[0x7C, 0x01, 0x19, 0x80]).is_err());
        assert!(rpu_to_p81(&[0x4E, 0x01, 0x05]).is_err(), "not an RPU");
    }

    /// A prefix SEI NAL unit with these messages (type, payload).
    pub(crate) fn sei(messages: &[(u8, &[u8])]) -> Vec<u8> {
        let mut rbsp = Vec::new();
        for (kind, payload) in messages {
            rbsp.push(*kind);
            rbsp.push(payload.len() as u8);
            rbsp.extend_from_slice(payload);
        }
        rbsp.push(0x80);
        let mut nal = vec![0x4E, 0x01];
        nal.extend(escape(&rbsp));
        nal
    }

    #[test]
    fn hdr10_plus_messages_leave_and_the_others_stay() {
        let hdr10plus: &[u8] = &[0xB5, 0x00, 0x3C, 0x00, 0x01, 0x04, 0x01, 0x40, 0x00, 0x00];
        let mastering: &[u8] = &[0x33, 0xC2, 0x86, 0xC4, 0x1D, 0x4C, 0x0B, 0xB8, 0x84, 0xD0, 0x3E, 0x80];
        let other_t35: &[u8] = &[0xB5, 0x00, 0x31, 0x47, 0x41, 0x39, 0x34];
        // Mastering display (137) and HDR10+ together: the first stays.
        let both = sei(&[(137, mastering), (4, hdr10plus)]);
        assert_eq!(strip_hdr10plus(&both), NalEdit::Replace(sei(&[(137, mastering)])));
        // HDR10+ alone: the NAL unit goes; other T.35 messages (captions) stay.
        assert_eq!(strip_hdr10plus(&sei(&[(4, hdr10plus)])), NalEdit::Drop);
        assert_eq!(strip_hdr10plus(&sei(&[(4, other_t35)])), NalEdit::Keep);
        assert_eq!(strip_hdr10plus(&sei(&[(144, &[0x03, 0xE8, 0x01, 0x90])])), NalEdit::Keep);
        // Not an SEI, or one that doesn't read right: as it is.
        assert_eq!(strip_hdr10plus(&[0x40, 0x01, 0x0C]), NalEdit::Keep);
        assert_eq!(strip_hdr10plus(&[0x4E, 0x01, 0x04, 0x20, 0xB5]), NalEdit::Keep);
    }
}
