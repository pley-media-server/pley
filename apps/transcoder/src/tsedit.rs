// SPDX-License-Identifier: AGPL-3.0-only
//! Edits of the HEVC stream inside finished MPEG-TS segments, for what
//! FFmpeg's stream copy can't do (see [`crate::dovi`]): Dolby Vision
//! profile 7 made 8.1 (the enhancement layer left out, the RPUs rewritten)
//! and HDR10+ metadata taken out.
//!
//! A segment is read whole, its video PES packets are taken apart into NAL
//! units and edited, then packetized again: the first TS packet of each PES
//! keeps its adaptation field (PCR, random access), continuity counters run
//! on, and other PIDs' packets stay as they were (those met in the middle
//! of a PES come right after it). Audio is never touched.

use std::path::Path;

use crate::dovi::{self, NAL_DV_EL, NAL_DV_RPU, NAL_PREFIX_SEI, NalEdit};

const PACKET: usize = 188;
/// `stream_type` of HEVC video in a PMT.
const STREAM_TYPE_HEVC: u8 = 0x24;

/// What to do to a segment's HEVC stream.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Edits {
    /// Dolby Vision profile 7 → 8.1.
    pub to_81: bool,
    pub strip_hdr10plus: bool,
}

impl Edits {
    pub fn any(&self) -> bool {
        self.to_81 || self.strip_hdr10plus
    }
}

/// What the edits did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Edited {
    /// RPUs rewritten as profile 8.1.
    pub rpus_converted: u64,
    /// RPUs that couldn't be read, left out.
    pub rpus_dropped: u64,
    /// Enhancement-layer NAL units left out.
    pub el_dropped: u64,
    /// SEI NAL units that lost HDR10+ messages.
    pub hdr10plus_removed: u64,
    /// Why the first RPU left out couldn't be read.
    pub first_error: Option<String>,
}

/// Edits the segment file at `path` in place (it isn't served yet).
pub fn edit_segment(path: &Path, edits: Edits) -> std::io::Result<Edited> {
    let data = std::fs::read(path)?;
    let (out, edited) = edit_ts(&data, edits).map_err(std::io::Error::other)?;
    if let Some(out) = out {
        std::fs::write(path, out)?;
    }
    Ok(edited)
}

/// The edited transport stream, or `None` when nothing changed.
pub fn edit_ts(data: &[u8], edits: Edits) -> Result<(Option<Vec<u8>>, Edited), String> {
    let mut edited = Edited::default();
    if !edits.any() || data.is_empty() {
        return Ok((None, edited));
    }
    let out = rewrite_video(data, |es| edit_access_unit(es, edits, &mut edited))?;
    Ok((out, edited))
}

/// The transport stream with each HEVC access unit passed through `edit`
/// (`None`: unchanged), or `None` when nothing changed.
pub fn rewrite_video(data: &[u8], mut edit: impl FnMut(&[u8]) -> Option<Vec<u8>>) -> Result<Option<Vec<u8>>, String> {
    if !data.len().is_multiple_of(PACKET) || data.chunks(PACKET).any(|p| p[0] != 0x47) {
        return Err("not an MPEG-TS file".into());
    }
    let Some(video) = hevc_pid(data) else { return Ok(None) };
    let mut out = Vec::with_capacity(data.len());
    // Other packets met while a PES is gathered: written after it.
    let mut held: Vec<&[u8]> = Vec::new();
    let mut pes: Option<Pes> = None;
    let mut cc: Option<u8> = None;
    let mut changed = false;
    for packet in data.chunks(PACKET) {
        let p = Packet::parse(packet);
        if p.pid != video || p.payload.is_empty() {
            // (Adaptation-only packets of the video PID carry a PCR.)
            if pes.is_some() {
                held.push(packet);
            } else {
                out.extend_from_slice(packet);
            }
            continue;
        }
        if p.start {
            if let Some(done) = pes.take() {
                changed |= done.finish(video, &mut cc, &mut edit, &mut out);
                held.drain(..).for_each(|h| out.extend_from_slice(h));
            }
            pes = Some(Pes { adaptation: p.meaningful_adaptation(), data: p.payload.to_vec(), cc: p.cc });
        } else if let Some(open) = pes.as_mut() {
            open.data.extend_from_slice(p.payload);
        } else {
            // The end of a PES that began before the file: as it is.
            out.extend_from_slice(packet);
        }
    }
    if let Some(done) = pes.take() {
        changed |= done.finish(video, &mut cc, &mut edit, &mut out);
        held.drain(..).for_each(|h| out.extend_from_slice(h));
    }
    Ok(changed.then_some(out))
}

struct Packet<'a> {
    pid: u16,
    start: bool,
    cc: u8,
    adaptation: &'a [u8],
    payload: &'a [u8],
}

impl<'a> Packet<'a> {
    fn parse(p: &'a [u8]) -> Packet<'a> {
        let pid = (u16::from(p[1] & 0x1F) << 8) | u16::from(p[2]);
        let start = p[1] & 0x40 != 0;
        let control = (p[3] >> 4) & 3;
        let cc = p[3] & 0x0F;
        let (adaptation, payload_at) = if control & 2 != 0 {
            let len = usize::from(p[4]).min(PACKET - 5);
            (&p[5..5 + len], 5 + len)
        } else {
            (&p[0..0], 4)
        };
        let payload = if control & 1 != 0 { &p[payload_at..] } else { &p[0..0] };
        Packet { pid, start, cc, adaptation, payload }
    }

    /// The adaptation field's flags and fields, stuffing left out; `None`
    /// when it is only stuffing.
    fn meaningful_adaptation(&self) -> Option<Vec<u8>> {
        let a = self.adaptation;
        let flags = *a.first()?;
        if flags == 0 {
            return None;
        }
        let mut n = 1;
        if flags & 0x10 != 0 {
            n += 6; // PCR
        }
        if flags & 0x08 != 0 {
            n += 6; // OPCR
        }
        if flags & 0x04 != 0 {
            n += 1; // splice countdown
        }
        if flags & 0x02 != 0 {
            n += 1 + usize::from(*a.get(n).unwrap_or(&0)); // private data
        }
        if flags & 0x01 != 0 {
            n += 1 + usize::from(*a.get(n).unwrap_or(&0)); // extension
        }
        Some(a[..n.min(a.len())].to_vec())
    }
}

/// A video PES being gathered: its first packet's adaptation field and
/// continuity counter, and its bytes.
struct Pes {
    adaptation: Option<Vec<u8>>,
    data: Vec<u8>,
    cc: u8,
}

impl Pes {
    /// Edits and packetizes the PES into `out`; whether it changed.
    fn finish(
        self,
        pid: u16,
        cc: &mut Option<u8>,
        edit: &mut impl FnMut(&[u8]) -> Option<Vec<u8>>,
        out: &mut Vec<u8>,
    ) -> bool {
        let next = cc.unwrap_or(self.cc);
        let new = edit_pes(&self.data, edit);
        let changed = new.is_some();
        let bytes = new.as_deref().unwrap_or(&self.data);
        *cc = Some(packetize(bytes, pid, self.adaptation.as_deref(), next, out));
        changed || next != self.cc
    }
}

/// Writes `pes` as TS packets of `pid` from continuity counter `cc`;
/// answers the counter after them.
fn packetize(pes: &[u8], pid: u16, adaptation: Option<&[u8]>, mut cc: u8, out: &mut Vec<u8>) -> u8 {
    let mut rest = pes;
    let mut first = true;
    while first || !rest.is_empty() {
        let field = if first { adaptation } else { None };
        let room = 184 - field.map_or(0, |f| 1 + f.len());
        let take = rest.len().min(room);
        let stuffing = room - take;
        out.push(0x47);
        out.push(if first { 0x40 } else { 0 } | ((pid >> 8) as u8 & 0x1F));
        out.push(pid as u8);
        let has_field = field.is_some() || stuffing > 0;
        out.push(if has_field { 0x30 } else { 0x10 } | cc);
        match field {
            Some(f) => {
                out.push((f.len() + stuffing) as u8);
                out.extend_from_slice(f);
                out.extend(std::iter::repeat_n(0xFF, stuffing));
            }
            None if stuffing == 1 => out.push(0),
            None if stuffing > 1 => {
                out.push((stuffing - 1) as u8);
                out.push(0);
                out.extend(std::iter::repeat_n(0xFF, stuffing - 2));
            }
            None => {}
        }
        out.extend_from_slice(&rest[..take]);
        rest = &rest[take..];
        cc = (cc + 1) & 0x0F;
        first = false;
    }
    cc
}

/// The PES with its access unit edited, or `None` when nothing changed.
fn edit_pes(pes: &[u8], edit: &mut impl FnMut(&[u8]) -> Option<Vec<u8>>) -> Option<Vec<u8>> {
    if pes.len() < 9 || pes[..3] != [0, 0, 1] || pes[3] & 0xF0 != 0xE0 {
        return None;
    }
    let es_at = 9 + usize::from(pes[8]);
    let es = pes.get(es_at..)?;
    let new_es = edit(es)?;
    let mut out = Vec::with_capacity(es_at + new_es.len());
    out.extend_from_slice(&pes[..es_at]);
    out.extend_from_slice(&new_es);
    // A stated length (FFmpeg leaves video's unstated) follows the change.
    if u16::from_be_bytes([pes[4], pes[5]]) != 0 {
        let len = u16::try_from(out.len() - 6).unwrap_or(0);
        out[4..6].copy_from_slice(&len.to_be_bytes());
    }
    Some(out)
}

/// An Annex B access unit with its NAL units edited, or `None` when
/// nothing changed.
pub fn edit_access_unit(es: &[u8], edits: Edits, edited: &mut Edited) -> Option<Vec<u8>> {
    let units = nal_units(es);
    let mut decisions = Vec::with_capacity(units.len());
    let mut any = false;
    for unit in &units {
        let nal = &es[unit.nal.clone()];
        let decision = match dovi::nal_type(nal) {
            Some(NAL_DV_EL) if edits.to_81 => {
                edited.el_dropped += 1;
                NalEdit::Drop
            }
            Some(NAL_DV_RPU) if edits.to_81 => match dovi::rpu_to_p81(nal) {
                Ok(NalEdit::Replace(new)) => {
                    edited.rpus_converted += 1;
                    NalEdit::Replace(new)
                }
                Ok(other) => other,
                Err(err) => {
                    edited.rpus_dropped += 1;
                    if edited.first_error.is_none() {
                        edited.first_error = Some(format!("{err:#}"));
                    }
                    NalEdit::Drop
                }
            },
            Some(NAL_PREFIX_SEI) if edits.strip_hdr10plus => {
                let decision = dovi::strip_hdr10plus(nal);
                if decision != NalEdit::Keep {
                    edited.hdr10plus_removed += 1;
                }
                decision
            }
            _ => NalEdit::Keep,
        };
        any |= decision != NalEdit::Keep;
        decisions.push(decision);
    }
    if !any {
        return None;
    }
    let mut out = Vec::with_capacity(es.len());
    out.extend_from_slice(&es[..units.first().map_or(es.len(), |u| u.start_code.start)]);
    for (unit, decision) in units.iter().zip(decisions) {
        match decision {
            NalEdit::Keep => out.extend_from_slice(&es[unit.start_code.start..unit.nal.end]),
            NalEdit::Drop => {}
            NalEdit::Replace(new) => {
                out.extend_from_slice(&es[unit.start_code.clone()]);
                out.extend_from_slice(&new);
            }
        }
    }
    Some(out)
}

pub(crate) struct NalUnit {
    /// `00 00 01`, or `00 00 00 01`.
    pub(crate) start_code: std::ops::Range<usize>,
    pub(crate) nal: std::ops::Range<usize>,
}

/// The NAL units of an Annex B byte stream.
pub(crate) fn nal_units(es: &[u8]) -> Vec<NalUnit> {
    let mut starts = Vec::new();
    let mut i = 0;
    while i + 3 <= es.len() {
        if es[i] == 0 && es[i + 1] == 0 && es[i + 2] == 1 {
            let from = if i > 0 && es[i - 1] == 0 { i - 1 } else { i };
            starts.push((from, i + 3));
            i += 3;
        } else {
            i += 1;
        }
    }
    starts
        .iter()
        .enumerate()
        .map(|(k, &(from, at))| NalUnit {
            start_code: from..at,
            nal: at..starts.get(k + 1).map_or(es.len(), |next| next.0),
        })
        .collect()
}

/// The PID of the first HEVC stream, from the PAT and PMT.
fn hevc_pid(data: &[u8]) -> Option<u16> {
    let section = |pid: u16| {
        data.chunks(PACKET).map(Packet::parse).find(|p| p.pid == pid && p.start).and_then(|p| {
            let pointer = usize::from(*p.payload.first()?);
            let table = p.payload.get(1 + pointer..)?;
            let len = (usize::from(*table.get(1)? & 0x0F) << 8) | usize::from(*table.get(2)?);
            table.get(..3 + len).map(<[u8]>::to_vec)
        })
    };
    let pat = section(0)?;
    // Programs from byte 8 to the CRC; program 0 is the network PID.
    let pmt = pat.get(8..pat.len().checked_sub(4)?)?.as_chunks::<4>().0.iter().find_map(|entry| {
        let program = u16::from_be_bytes([entry[0], entry[1]]);
        (program != 0).then(|| (u16::from(entry[2] & 0x1F) << 8) | u16::from(entry[3]))
    })?;
    let pmt = section(pmt)?;
    let info_len = (usize::from(*pmt.get(10)? & 0x0F) << 8) | usize::from(*pmt.get(11)?);
    let mut i = 12 + info_len;
    let end = pmt.len().checked_sub(4)?;
    while i + 5 <= end {
        let kind = pmt[i];
        let pid = (u16::from(pmt[i + 1] & 0x1F) << 8) | u16::from(pmt[i + 2]);
        let es_info = (usize::from(pmt[i + 3] & 0x0F) << 8) | usize::from(pmt[i + 4]);
        if kind == STREAM_TYPE_HEVC {
            return Some(pid);
        }
        i += 5 + es_info;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dovi::tests::{p7_rpu, sei};

    const AUD: [u8; 3] = [0x46, 0x01, 0x50];
    const SLICE: [u8; 6] = [0x26, 0x01, 0xAF, 0x09, 0x40, 0xF3];

    /// An access unit: AUD, an SEI with HDR10+ and a mastering display, a
    /// slice, the RPU and an enhancement-layer unit.
    fn access_unit(slice_bytes: usize) -> Vec<u8> {
        let hdr10plus: &[u8] = &[0xB5, 0x00, 0x3C, 0x00, 0x01, 0x04, 0x01, 0x40];
        let mut au = vec![0, 0, 0, 1];
        au.extend(AUD);
        au.extend([0, 0, 1]);
        au.extend(sei(&[(137, &[1; 24]), (4, hdr10plus)]));
        au.extend([0, 0, 1]);
        au.extend(SLICE);
        au.extend((0..slice_bytes).map(|i| (i % 251) as u8 | 0x10));
        au.extend([0, 0, 1]);
        au.extend([0x7E, 0x01, 0x22, 0x33]); // type 63: enhancement layer
        au.extend([0, 0, 1]);
        au.extend(p7_rpu(true));
        au
    }

    fn psi(pid: u16, table: &[u8]) -> Vec<u8> {
        let mut p = vec![0x47, 0x40 | (pid >> 8) as u8, pid as u8, 0x10, 0];
        p.extend_from_slice(table);
        p.resize(PACKET, 0xFF);
        p
    }

    /// PAT, PMT (HEVC on 0x100, AC-3 on 0x101), then two video PES with an
    /// audio packet between their packets.
    fn segment(slice_bytes: usize) -> Vec<u8> {
        let mut ts = psi(0, &[0x00, 0xB0, 0x0D, 0, 1, 0xC1, 0, 0, 0, 1, 0xF0, 0x00, 0, 0, 0, 0]);
        ts.extend(psi(
            0x1000,
            &[
                0x02, 0xB0, 0x17, 0, 1, 0xC1, 0, 0, 0xE1, 0x00, 0xF0, 0, 0x24, 0xE1, 0x00, 0xF0, 0, 0x81, 0xE1, 0x01,
                0xF0, 0, 0, 0, 0, 0,
            ],
        ));
        let mut cc = 3;
        for k in 0..2u8 {
            let mut pes = vec![0, 0, 1, 0xE0, 0, 0, 0x80, 0x80, 5, 0x21, 0, 1 + k * 2, 0, 1];
            pes.extend(access_unit(slice_bytes));
            // PCR and random access on the first packet.
            let pcr = [0x50, 0, 0, 0, 0x10, 0x7E, 0];
            cc = packetize(&pes, 0x100, Some(&pcr), cc, &mut ts);
            let mut audio = vec![0x47, 0x41, 0x01, 0x10 | k];
            audio.resize(PACKET, 0xAA);
            // In the middle of the PES's packets.
            let at = ts.len() - PACKET;
            ts.splice(at..at, audio);
        }
        ts
    }

    /// The video PES payloads of a transport stream, and its video
    /// continuity counters.
    fn video_of(ts: &[u8]) -> (Vec<Vec<u8>>, Vec<u8>) {
        let mut pes: Vec<Vec<u8>> = Vec::new();
        let mut ccs = Vec::new();
        for p in ts.chunks(PACKET).map(Packet::parse).filter(|p| p.pid == 0x100) {
            if p.start {
                pes.push(Vec::new());
            }
            pes.last_mut().unwrap().extend_from_slice(p.payload);
            ccs.push(p.cc);
        }
        (pes, ccs)
    }

    #[test]
    fn segments_get_8_1_rpus_and_lose_hdr10_plus() {
        for slice_bytes in [10, 170, 4000] {
            let ts = segment(slice_bytes);
            let edits = Edits { to_81: true, strip_hdr10plus: true };
            let (out, edited) = edit_ts(&ts, edits).unwrap();
            let out = out.expect("changed");
            assert_eq!(out.len() % PACKET, 0);
            assert_eq!(
                (edited.rpus_converted, edited.el_dropped, edited.hdr10plus_removed, edited.rpus_dropped),
                (2, 2, 2, 0)
            );
            let (pes, ccs) = video_of(&out);
            assert_eq!(pes.len(), 2);
            let expected_ccs: Vec<u8> = (0..ccs.len()).map(|i| ((3 + i) % 16) as u8).collect();
            assert_eq!(ccs, expected_ccs, "continuity counters run on");
            for p in &pes {
                let es = &p[14..];
                let kinds: Vec<u8> =
                    nal_units(es).iter().map(|u| dovi::nal_type(&es[u.nal.clone()]).unwrap()).collect();
                assert_eq!(kinds, [35, 39, 19, 62], "AUD, SEI, slice, RPU: no enhancement layer");
                let units = nal_units(es);
                let sei_nal = &es[units[1].nal.clone()];
                assert_eq!(sei_nal, sei(&[(137, &[1; 24])]), "the mastering display stays");
                assert!(es.windows(SLICE.len()).any(|w| w == SLICE), "the picture is untouched");
            }
            // The first packet of each PES keeps its PCR; audio is all there.
            let packets: Vec<Packet> = out.chunks(PACKET).map(Packet::parse).collect();
            let starts: Vec<&Packet> = packets.iter().filter(|p| p.pid == 0x100 && p.start).collect();
            assert!(starts.iter().all(|p| p.adaptation[..7] == [0x50, 0, 0, 0, 0x10, 0x7E, 0]));
            assert_eq!(packets.iter().filter(|p| p.pid == 0x101).count(), 2);
            assert_eq!((packets[0].pid, packets[1].pid), (0, 0x1000));
            // Done once, nothing more to do.
            let (again, _) = edit_ts(&out, edits).unwrap();
            assert_eq!(again, None);
        }
    }

    #[test]
    fn only_what_is_asked_changes() {
        let ts = segment(500);
        let (out, edited) = edit_ts(&ts, Edits { to_81: false, strip_hdr10plus: true }).unwrap();
        assert_eq!((edited.rpus_converted, edited.hdr10plus_removed), (0, 2));
        let (pes, _) = video_of(&out.unwrap());
        let es = &pes[0][14..];
        let kinds: Vec<u8> = nal_units(es).iter().map(|u| dovi::nal_type(&es[u.nal.clone()]).unwrap()).collect();
        assert_eq!(kinds, [35, 39, 19, 63, 62], "Dolby Vision as it was");
        assert_eq!(edit_ts(&ts, Edits::default()).unwrap(), (None, Edited::default()));
        assert!(edit_ts(&ts[..100], Edits { to_81: true, strip_hdr10plus: false }).is_err());
    }

    #[test]
    fn start_codes_of_three_and_four_bytes() {
        let es = [0, 0, 0, 1, 0x46, 0x01, 0x50, 0, 0, 1, 0x7E, 0x01, 0, 0, 0, 1, 0x26, 0x01, 0xAA];
        let units = nal_units(&es);
        assert_eq!(
            units.iter().map(|u| (u.start_code.clone(), u.nal.clone())).collect::<Vec<_>>(),
            [(0..4, 4..7), (7..10, 10..12), (12..16, 16..19)]
        );
        let mut edited = Edited::default();
        let out = edit_access_unit(&es, Edits { to_81: true, strip_hdr10plus: false }, &mut edited).unwrap();
        assert_eq!(out, [0, 0, 0, 1, 0x46, 0x01, 0x50, 0, 0, 0, 1, 0x26, 0x01, 0xAA]);
        assert_eq!(edited.el_dropped, 1);
    }
}
