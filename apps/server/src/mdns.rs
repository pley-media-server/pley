//! The little of DNS's wire format (RFC 1035, RFC 6762) that Pley's Bonjour
//! responder needs ([`crate::bonjour`]): reading the questions of a query and
//! the names a response answers for, and writing answers. What is read comes
//! from any device on the network: names, counts and pointers are bounded,
//! and nothing read is kept.

use std::net::Ipv4Addr;

pub const TYPE_A: u16 = 1;
pub const TYPE_PTR: u16 = 12;
pub const TYPE_TXT: u16 = 16;
pub const TYPE_SRV: u16 = 33;
pub const TYPE_ANY: u16 = 255;
const CLASS_IN: u16 = 1;
/// In a question: answer by unicast (QU). In a record: replaces what caches
/// hold for the name (cache flush).
const TOP_BIT: u16 = 0x8000;
/// A response from an authoritative answer.
const FLAGS_RESPONSE: u16 = 0x8400;

/// A name's length at most, as RFC 1035 has it.
const MAX_NAME: usize = 255;
/// A label's length at most.
pub const MAX_LABEL: usize = 63;
/// Compression pointers followed in one name, at most.
const MAX_POINTERS: usize = 16;
/// Questions read from one query, and names from one response, at most: a
/// Bonjour browser asks a handful.
const MAX_ENTRIES: usize = 32;

/// A name, label by label. An instance's label may hold dots and any UTF-8
/// (DNS-SD, RFC 6763 §4.3): labels are never split on dots.
pub type Name = Vec<String>;

/// Whether two names are the same: labels compare without ASCII case, as
/// DNS's do.
pub fn same(a: &[String], b: &[String]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(a, b)| a.eq_ignore_ascii_case(b))
}

/// A question of a query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Question {
    pub name: Name,
    pub kind: u16,
    /// The querier asks for a unicast answer (QU).
    pub unicast: bool,
}

/// What a query asks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Query {
    pub id: u16,
    pub questions: Vec<Question>,
}

/// What a packet is: a query, or a response with the names it answers for
/// (its answer and authority records': a probe puts the records it would
/// own there).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Packet {
    Query(Query),
    Response(Vec<Name>),
}

/// Reads `packet`, or `None` for anything that isn't a well-formed standard
/// query or response within the bounds.
pub fn read(packet: &[u8]) -> Option<Packet> {
    let header = packet.get(..12)?;
    let word = |at: usize| u16::from_be_bytes([header[at], header[at + 1]]);
    let (id, flags) = (word(0), word(2));
    // Opcode 0 (a standard query) only, and responses with a response code
    // of 0 (RFC 6762 §18.11).
    if flags & 0x7800 != 0 || (flags & 0x8000 != 0 && flags & 0x000F != 0) {
        return None;
    }
    let questions = usize::from(word(4));
    let records = usize::from(word(6)) + usize::from(word(8));
    let mut at = 12;
    if flags & 0x8000 == 0 {
        let mut asked = Vec::new();
        for _ in 0..questions.min(MAX_ENTRIES) {
            let (name, next) = read_name(packet, at)?;
            let kind = u16::from_be_bytes([*packet.get(next)?, *packet.get(next + 1)?]);
            let class = u16::from_be_bytes([*packet.get(next + 2)?, *packet.get(next + 3)?]);
            at = next + 4;
            if class & !TOP_BIT == CLASS_IN || class & !TOP_BIT == TYPE_ANY {
                asked.push(Question { name, kind, unicast: class & TOP_BIT != 0 });
            }
        }
        return Some(Packet::Query(Query { id, questions: asked }));
    }
    // A response: skip its questions, read the names its records answer
    // for. A goodbye (a TTL of 0) answers for none: it gives a name up.
    for _ in 0..questions.min(MAX_ENTRIES) {
        at = skip_name(packet, at)? + 4;
    }
    let mut names = Vec::new();
    for _ in 0..records.min(MAX_ENTRIES) {
        let (name, next) = read_name(packet, at)?;
        let ttl = u32::from_be_bytes(packet.get(next + 4..next + 8)?.try_into().ok()?);
        let length = usize::from(u16::from_be_bytes([*packet.get(next + 8)?, *packet.get(next + 9)?]));
        at = next + 10 + length;
        if at > packet.len() {
            return None;
        }
        if ttl > 0 {
            names.push(name);
        }
    }
    Some(Packet::Response(names))
}

/// The name at `at` in `packet`, and where what follows it begins.
/// Compression pointers must point before the label that points, and are
/// followed [`MAX_POINTERS`] times at most, which ends a loop between
/// them.
fn read_name(packet: &[u8], mut at: usize) -> Option<(Name, usize)> {
    let mut labels = Vec::new();
    let mut length = 0;
    let mut after = None;
    let mut jumps = 0;
    loop {
        let len = usize::from(*packet.get(at)?);
        match len {
            0 => {
                return Some((labels, after.unwrap_or(at + 1)));
            }
            1..=63 => {
                let label = packet.get(at + 1..at + 1 + len)?;
                length += len + 1;
                if length > MAX_NAME {
                    return None;
                }
                labels.push(String::from_utf8_lossy(label).into_owned());
                at += 1 + len;
            }
            0xC0..=0xFF => {
                let target = ((len & 0x3F) << 8) | usize::from(*packet.get(at + 1)?);
                jumps += 1;
                if target >= at || jumps > MAX_POINTERS {
                    return None;
                }
                after.get_or_insert(at + 2);
                at = target;
            }
            // 0x40 and 0x80, extended label types, aren't used.
            _ => return None,
        }
    }
}

/// Where the name at `at` ends, without reading it.
fn skip_name(packet: &[u8], mut at: usize) -> Option<usize> {
    loop {
        let len = usize::from(*packet.get(at)?);
        match len {
            0 => return Some(at + 1),
            1..=63 => at += 1 + len,
            0xC0..=0xFF => return Some(at + 2),
            _ => return None,
        }
    }
}

/// What a record says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Data {
    A(Ipv4Addr),
    Ptr(Name),
    Srv { port: u16, target: Name },
    Txt(Vec<String>),
}

impl Data {
    fn kind(&self) -> u16 {
        match self {
            Data::A(_) => TYPE_A,
            Data::Ptr(_) => TYPE_PTR,
            Data::Srv { .. } => TYPE_SRV,
            Data::Txt(_) => TYPE_TXT,
        }
    }
}

/// A record to send.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    pub name: Name,
    pub data: Data,
    /// Seconds; 0 says goodbye.
    pub ttl: u32,
    /// Pley alone owns it (SRV, TXT, its host's address), not a shared PTR.
    pub unique: bool,
}

/// A response with `answers` and `additionals`. `echo` is a legacy unicast
/// querier's id and question (RFC 6762 §6.7), which the answer repeats.
pub fn write(answers: &[Record], additionals: &[Record], echo: Option<(u16, &Question)>) -> Vec<u8> {
    let mut out = Vec::with_capacity(512);
    let id = echo.map_or(0, |(id, _)| id);
    let questions = u16::from(echo.is_some());
    for word in [id, FLAGS_RESPONSE, questions, answers.len() as u16, 0, additionals.len() as u16] {
        out.extend_from_slice(&word.to_be_bytes());
    }
    if let Some((_, question)) = echo {
        write_name(&mut out, &question.name);
        out.extend_from_slice(&question.kind.to_be_bytes());
        out.extend_from_slice(&CLASS_IN.to_be_bytes());
    }
    for record in answers.iter().chain(additionals) {
        write_name(&mut out, &record.name);
        out.extend_from_slice(&record.data.kind().to_be_bytes());
        // A legacy unicast answer goes to a plain DNS resolver: no cache flush.
        let class = if record.unique && echo.is_none() { CLASS_IN | TOP_BIT } else { CLASS_IN };
        out.extend_from_slice(&class.to_be_bytes());
        out.extend_from_slice(&record.ttl.to_be_bytes());
        let at = out.len();
        out.extend_from_slice(&[0, 0]);
        match &record.data {
            Data::A(ip) => out.extend_from_slice(&ip.octets()),
            Data::Ptr(name) => write_name(&mut out, name),
            Data::Srv { port, target } => {
                out.extend_from_slice(&[0, 0, 0, 0]);
                out.extend_from_slice(&port.to_be_bytes());
                write_name(&mut out, target);
            }
            Data::Txt(strings) => {
                for s in strings {
                    let s = &s.as_bytes()[..s.len().min(255)];
                    out.push(s.len() as u8);
                    out.extend_from_slice(s);
                }
            }
        }
        let length = (out.len() - at - 2) as u16;
        out[at..at + 2].copy_from_slice(&length.to_be_bytes());
    }
    out
}

/// A query for `name`'s records of `kind`, as a probe or a browser sends it.
pub fn write_query(name: &[String], kind: u16) -> Vec<u8> {
    let mut out = vec![0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0];
    write_name(&mut out, name);
    out.extend_from_slice(&kind.to_be_bytes());
    out.extend_from_slice(&CLASS_IN.to_be_bytes());
    out
}

/// Writes `name` uncompressed; each label is bounded to [`MAX_LABEL`] bytes
/// (Pley's own names, made to fit).
fn write_name(out: &mut Vec<u8>, name: &[String]) {
    for label in name {
        let label = &label.as_bytes()[..label.len().min(MAX_LABEL)];
        out.push(label.len() as u8);
        out.extend_from_slice(label);
    }
    out.push(0);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name(text: &str) -> Name {
        text.split('.').map(str::to_owned).collect()
    }

    #[test]
    fn queries_are_read_with_their_compressed_names() {
        // _pley._tcp.local PTR, then <pointer to _tcp.local> as a second
        // name with a label of its own, QU.
        let mut p = vec![0x12, 0x34, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0];
        p.extend_from_slice(b"\x05_pley\x04_tcp\x05local\x00\x00\x0c\x00\x01");
        p.extend_from_slice(b"\x03abc\xc0\x12\x00\x21\x80\x01");
        let Some(Packet::Query(q)) = read(&p) else { panic!("{:?}", read(&p)) };
        assert_eq!(q.id, 0x1234);
        assert_eq!(
            q.questions,
            [
                Question { name: name("_pley._tcp.local"), kind: TYPE_PTR, unicast: false },
                Question { name: name("abc._tcp.local"), kind: TYPE_SRV, unicast: true },
            ]
        );
    }

    #[test]
    fn crafted_names_are_refused() {
        let header = |n: u8| vec![0, 0, 0, 0, 0, n, 0, 0, 0, 0, 0, 0];
        // A pointer to itself, and one forward.
        let mut p = header(1);
        p.extend_from_slice(b"\xc0\x0c\x00\x01\x00\x01");
        assert_eq!(read(&p), None);
        let mut p = header(1);
        p.extend_from_slice(b"\xc0\x20\x00\x01\x00\x01");
        assert_eq!(read(&p), None);
        // Longer than 255 bytes.
        let mut p = header(1);
        for _ in 0..5 {
            p.push(63);
            p.extend_from_slice(&[b'a'; 63]);
        }
        p.extend_from_slice(b"\x00\x00\x01\x00\x01");
        assert_eq!(read(&p), None);
        // Cut short, and an extended label type.
        assert_eq!(read(&[0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 5, b'a']), None);
        let mut p = header(1);
        p.extend_from_slice(b"\x41\x00\x00\x01\x00\x01");
        assert_eq!(read(&p), None);
        // Another opcode.
        assert_eq!(read(&[0, 0, 0x28, 0, 0, 0, 0, 0, 0, 0, 0, 0]), None);
        // A count far above what follows reads what is there and no more.
        let mut p = header(255);
        p.extend_from_slice(b"\x01a\x00\x00\x01\x00\x01");
        assert_eq!(read(&p), None, "the second question isn't there");
        assert_eq!(read(&[0; 5]), None);
    }

    #[test]
    fn responses_give_the_names_they_answer_for() {
        let record = Record {
            name: name("Den._pley._tcp.local"),
            data: Data::Txt(vec!["pley=1".into()]),
            ttl: 120,
            unique: true,
        };
        let p = write(std::slice::from_ref(&record), &[], None);
        assert_eq!(read(&p), Some(Packet::Response(vec![name("Den._pley._tcp.local")])));
        // A goodbye gives the name up: it answers for nothing.
        let goodbye = write(&[Record { ttl: 0, ..record.clone() }], &[], None);
        assert_eq!(read(&goodbye), Some(Packet::Response(Vec::new())));
        // A response with an error code is no answer.
        let mut failed = p.clone();
        failed[3] |= 3;
        assert_eq!(read(&failed), None);
    }

    #[test]
    fn answers_are_written_as_dns_reads_them() {
        let instance: Name = vec!["media.home".into(), "_pley".into(), "_tcp".into(), "local".into()];
        let host = name("pley-0123.local");
        let records = [
            Record { name: name("_pley._tcp.local"), data: Data::Ptr(instance.clone()), ttl: 4500, unique: false },
            Record {
                name: instance.clone(),
                data: Data::Srv { port: 7539, target: host.clone() },
                ttl: 120,
                unique: true,
            },
            Record { name: host.clone(), data: Data::A(Ipv4Addr::new(192, 168, 1, 10)), ttl: 120, unique: true },
        ];
        let p = write(&records[..1], &records[1..], None);
        // Header: a response, 1 answer, 2 additionals.
        assert_eq!(&p[..12], &[0, 0, 0x84, 0, 0, 0, 0, 1, 0, 0, 0, 2]);
        // The instance's label keeps its dot.
        assert!(p.windows(11).any(|w| w == b"\x0amedia.home"));
        // The PTR is shared (no cache flush), the SRV and A are Pley's own.
        let ptr_class = 12 + 18 + 2;
        assert_eq!(&p[ptr_class..ptr_class + 2], &[0, 1]);
        assert!(p.windows(8).any(|w| w == [0, 33, 0x80, 1, 0, 0, 0, 120]), "SRV, flush, TTL 120");
        assert!(p.ends_with(&[0, 1, 0x80, 1, 0, 0, 0, 120, 0, 4, 192, 168, 1, 10]));
        // Legacy unicast: the id and the question repeated, no flush.
        let question = Question { name: host.clone(), kind: TYPE_A, unicast: false };
        let p = write(&records[2..], &[], Some((0x4242, &question)));
        assert_eq!(&p[..12], &[0x42, 0x42, 0x84, 0, 0, 1, 0, 1, 0, 0, 0, 0]);
        assert!(p.ends_with(&[0, 1, 0, 1, 0, 0, 0, 120, 0, 4, 192, 168, 1, 10]));
        let Some(Packet::Query(q)) = read(&write_query(&host, TYPE_ANY)) else { panic!() };
        assert_eq!(q.questions, [Question { name: host, kind: TYPE_ANY, unicast: false }]);
    }
}
