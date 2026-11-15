//! Reading the property lists a device sends, within bounds. The `plist`
//! crate reads any shape it is given: a binary plist can point to one
//! array from many places, which it reads again each time (a few hundred
//! bytes that take minutes), and nesting deep enough overflows the stack
//! when the value is dropped. So a plist is read only when it is small, not
//! nested deep, and holds few objects counted the way the crate reads
//! them.

use plist::Value;

/// The largest plist read: a device's answers and events take a few KiB.
const MAX_BYTES: usize = 256 * 1024;
/// The deepest nesting read: Apple's events go a handful of levels down.
const MAX_DEPTH: usize = 32;
/// The most objects read, each one counted every time something points
/// to it.
const MAX_OBJECTS: usize = 20_000;

/// Reads `bytes` as a plist, when they are within bounds.
pub fn read(bytes: &[u8]) -> Option<Value> {
    if bytes.len() > MAX_BYTES {
        return None;
    }
    let fits = match bytes.strip_prefix(b"bplist00") {
        Some(_) => binary_fits(bytes),
        None => xml_fits(bytes),
    };
    if !fits {
        return None;
    }
    Value::from_reader(std::io::Cursor::new(bytes)).ok()
}

/// Whether a binary plist is nested at most [`MAX_DEPTH`] and holds at most
/// [`MAX_OBJECTS`], walked from its top object as the crate reads it.
fn binary_fits(bytes: &[u8]) -> bool {
    let Some(trailer) = bytes.len().checked_sub(32).map(|at| &bytes[at..]) else { return false };
    let offset_size = usize::from(trailer[6]);
    let ref_size = usize::from(trailer[7]);
    let count = be(&trailer[8..16]);
    let top = be(&trailer[16..24]);
    let table = be(&trailer[24..32]);
    if !(1..=8).contains(&offset_size) || !(1..=8).contains(&ref_size) || top >= count {
        return false;
    }
    let body = &bytes[..bytes.len() - 32];
    let offset_of = |object: u64| -> Option<usize> {
        let at = usize::try_from(table.checked_add(object.checked_mul(offset_size as u64)?)?).ok()?;
        let offset = usize::try_from(be(body.get(at..at.checked_add(offset_size)?)?)).ok()?;
        (offset < body.len()).then_some(offset)
    };
    let mut stack = vec![(top, 0usize)];
    let mut seen = 0usize;
    while let Some((object, depth)) = stack.pop() {
        seen += 1;
        if seen > MAX_OBJECTS || depth > MAX_DEPTH || object >= count {
            return false;
        }
        let Some(offset) = offset_of(object) else { return false };
        let marker = body[offset];
        // Arrays and sets hold `n` references, dictionaries `n` keys and `n`
        // values; nothing else points anywhere.
        let per = match marker >> 4 {
            0xA | 0xC => 1,
            0xD => 2,
            _ => continue,
        };
        let Some((n, refs_at)) = length(body, offset, marker) else { return false };
        let Some(refs) = n.checked_mul(per).and_then(|n| n.checked_mul(ref_size)) else { return false };
        let Some(refs) = refs_at.checked_add(refs).and_then(|end| body.get(refs_at..end)) else { return false };
        if stack.len() + refs.len() / ref_size > MAX_OBJECTS {
            return false;
        }
        stack.extend(refs.chunks(ref_size).map(|r| (be(r), depth + 1)));
    }
    true
}

/// A container's count and where its references start: in the marker's low
/// four bits, or in an integer after it when they are all set.
fn length(body: &[u8], offset: usize, marker: u8) -> Option<(usize, usize)> {
    let low = usize::from(marker & 0x0F);
    if low != 0x0F {
        return Some((low, offset + 1));
    }
    let int_marker = *body.get(offset + 1)?;
    if int_marker >> 4 != 0x1 {
        return None;
    }
    let size = 1usize << (int_marker & 0x0F);
    let n = body.get(offset + 2..offset + 2 + size).filter(|b| b.len() <= 8)?;
    Some((usize::try_from(be(n)).ok()?, offset + 2 + size))
}

fn be(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0u64, |n, b| (n << 8) | u64::from(*b))
}

/// Whether an XML plist is nested at most [`MAX_DEPTH`]: it has no
/// references, so its size bounds the rest.
fn xml_fits(bytes: &[u8]) -> bool {
    let mut depth = 0usize;
    let mut rest = bytes;
    while let Some(at) = rest.iter().position(|b| *b == b'<') {
        rest = &rest[at + 1..];
        let end = rest.iter().position(|b| *b == b'>').unwrap_or(rest.len());
        let tag = &rest[..end];
        let closing = tag.first() == Some(&b'/');
        let name = tag.strip_prefix(b"/").unwrap_or(tag);
        if name.starts_with(b"array") || name.starts_with(b"dict") {
            if closing {
                depth = depth.saturating_sub(1);
            } else if tag.last() != Some(&b'/') {
                depth += 1;
                if depth > MAX_DEPTH {
                    return false;
                }
            }
        }
        rest = &rest[end.min(rest.len())..];
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn binary(value: &Value) -> Vec<u8> {
        let mut out = Vec::new();
        value.to_writer_binary(&mut out).unwrap();
        out
    }

    /// A binary plist by hand: `objects` in order, each a marker and its
    /// one-byte references; the top is the first.
    fn by_hand(objects: &[Vec<u8>]) -> Vec<u8> {
        let mut out = b"bplist00".to_vec();
        let mut offsets = Vec::new();
        for object in objects {
            offsets.push(u16::try_from(out.len()).unwrap());
            out.extend(object);
        }
        let table = out.len() as u64;
        for offset in &offsets {
            out.extend(offset.to_be_bytes());
        }
        out.extend([0, 0, 0, 0, 0, 0, 2, 1]);
        out.extend((objects.len() as u64).to_be_bytes());
        out.extend(0u64.to_be_bytes());
        out.extend(table.to_be_bytes());
        out
    }

    #[test]
    fn a_device_s_usual_plists_are_read() {
        let mut info = plist::Dictionary::new();
        info.insert("eventPort".into(), Value::Integer(7000.into()));
        info.insert("streams".into(), Value::Array(vec![Value::Dictionary(info.clone())]));
        let value = Value::Dictionary(info);
        assert_eq!(read(&binary(&value)), Some(value.clone()));
        let mut xml = Vec::new();
        value.to_writer_xml(&mut xml).unwrap();
        assert_eq!(read(&xml), Some(value));
    }

    #[test]
    fn a_plist_too_deep_or_too_many_times_over_is_refused() {
        // 40 arrays each holding the next twice: 2^40 objects as read.
        let mut objects: Vec<Vec<u8>> = (0..40u8).map(|i| vec![0xA2, i + 1, i + 1]).collect();
        objects.push(vec![0x08]);
        assert_eq!(read(&by_hand(&objects)), None);
        // 100 arrays each holding the next once.
        let mut deep: Vec<Vec<u8>> = (0..100u8).map(|i| vec![0xA1, i + 1]).collect();
        deep.push(vec![0x08]);
        assert_eq!(read(&by_hand(&deep)), None);
        let shallow = by_hand(&[vec![0xA1, 1], vec![0xA1, 2], vec![0x09]]);
        assert!(read(&shallow).is_some());
        let xml = format!("<plist>{}{}</plist>", "<array>".repeat(40), "</array>".repeat(40));
        assert_eq!(read(xml.as_bytes()), None);
        // A reference past the objects, and a trailer cut short.
        assert_eq!(read(&by_hand(&[vec![0xA1, 9]])), None);
        assert_eq!(read(b"bplist00"), None);
        assert_eq!(read(&vec![0u8; MAX_BYTES + 1]), None);
    }
}
