//! EBML, Matroska's layout: elements of an id, a size and data, each a
//! variable-length number. Positions here are in a buffer read from the
//! file; an element whose data runs past what was read is left alone by
//! [`children`], and every value read checks its bounds.

/// An element's header, read in a buffer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct El {
    pub id: u64,
    /// Where its data starts in the buffer.
    pub data_start: usize,
    /// None: unknown (a live file's Segment or Cluster).
    pub size: Option<u64>,
}

impl El {
    /// Where its data ends in the buffer; the furthest place there is for an
    /// unknown size, or one too large to count.
    pub fn end(&self) -> usize {
        self.size
            .and_then(|s| usize::try_from(s).ok())
            .and_then(|s| self.data_start.checked_add(s))
            .unwrap_or(usize::MAX)
    }
}

/// The elements one after another from `from` to `to`, while each is whole
/// there.
pub fn children(b: &[u8], from: usize, to: usize) -> impl Iterator<Item = El> + '_ {
    let mut at = from;
    std::iter::from_fn(move || {
        if at >= to {
            return None;
        }
        let e = element(b, at, to)?;
        if e.size.is_none() || e.end() > to {
            at = to;
            return None;
        }
        at = e.end();
        Some(e)
    })
}

/// The element whose header starts at `at`, read no further than `limit`.
pub fn element(b: &[u8], at: usize, limit: usize) -> Option<El> {
    let (id, id_len, _) = vint(b, at, limit, true)?;
    let size_at = at.checked_add(id_len)?;
    let (size, size_len, unknown) = vint(b, size_at, limit, false)?;
    Some(El { id, data_start: size_at.checked_add(size_len)?, size: (!unknown).then_some(size) })
}

/// An EBML variable-length number at `at`: an element id keeps its length
/// marker (`keep_marker`), a size or track number loses it. The value, its
/// length, and whether it is a size of all ones (unknown).
pub fn vint(b: &[u8], at: usize, limit: usize, keep_marker: bool) -> Option<(u64, usize, bool)> {
    let end = limit.min(b.len());
    if at >= end {
        return None;
    }
    let first = *b.get(at)?;
    if first == 0 {
        return None;
    }
    let length = first.leading_zeros() as usize + 1;
    let bytes = b.get(at..at.checked_add(length).filter(|e| *e <= end)?)?;
    let mask = (0x80u8 >> (length - 1)).wrapping_sub(1);
    let mut value = u64::from(if keep_marker { first } else { first & mask });
    let mut all_ones = first & mask == mask;
    for x in &bytes[1..] {
        all_ones &= *x == 0xFF;
        value = (value << 8) | u64::from(*x);
    }
    Some((value, length, !keep_marker && all_ones))
}

/// The bytes of an element's data that were read.
pub fn data<'a>(b: &'a [u8], e: &El) -> &'a [u8] {
    let end = e.end().min(b.len());
    b.get(e.data_start..end).unwrap_or_default()
}

/// An unsigned integer element; its last 8 bytes, should it have more.
pub fn uint(b: &[u8], e: &El) -> u64 {
    data(b, e).iter().fold(0, |v, x| (v << 8) | u64::from(*x))
}

/// A float element (4 or 8 bytes); none otherwise.
pub fn float(b: &[u8], e: &El) -> Option<f64> {
    let d = data(b, e);
    if e.end() > b.len() {
        return None;
    }
    match d.len() {
        4 => Some(f64::from(f32::from_be_bytes(d.try_into().ok()?))),
        8 => Some(f64::from_be_bytes(d.try_into().ok()?)),
        _ => None,
    }
}

/// A string element, up to its first NUL; bytes that aren't UTF-8 are
/// replaced.
pub fn text(b: &[u8], e: &El) -> String {
    let d = data(b, e);
    let d = d.iter().position(|x| *x == 0).map_or(d, |nul| &d[..nul]);
    String::from_utf8_lossy(d).into_owned()
}
