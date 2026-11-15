//! Google Cast's messages as they go over the wire: each a protocol buffer
//! (`CastMessage`, of which Pley uses the string kind alone) after its
//! length, four bytes big-endian. What is read comes from any device on the
//! network: lengths are bounded and a message that doesn't parse is
//! refused.

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// The longest message read: a receiver's status is a few kilobytes.
pub const MAX_MESSAGE: usize = 64 * 1024;

/// One message: from `source` to `destination` on `namespace`, its payload
/// JSON.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub source: String,
    pub destination: String,
    pub namespace: String,
    pub payload: String,
}

// CastMessage's fields: protocol_version (enum, 0 = CASTV2_1_0),
// source_id, destination_id, namespace, payload_type (enum, 0 = STRING),
// payload_utf8. payload_binary (7) is never sent, and skipped when read.
const VERSION: u32 = 1;
const SOURCE: u32 = 2;
const DESTINATION: u32 = 3;
const NAMESPACE: u32 = 4;
const PAYLOAD_TYPE: u32 = 5;
const PAYLOAD_UTF8: u32 = 6;

const VARINT: u32 = 0;
const BYTES: u32 = 2;

fn put_varint(out: &mut Vec<u8>, mut n: u64) {
    loop {
        let byte = (n & 0x7F) as u8;
        n >>= 7;
        if n == 0 {
            out.push(byte);
            return;
        }
        out.push(byte | 0x80);
    }
}

fn put_string(out: &mut Vec<u8>, field: u32, value: &str) {
    put_varint(out, u64::from(field << 3 | BYTES));
    put_varint(out, value.len() as u64);
    out.extend_from_slice(value.as_bytes());
}

impl Message {
    /// The message as it is sent: its length, then itself.
    pub fn encode(&self) -> Vec<u8> {
        let mut body = Vec::with_capacity(64 + self.payload.len());
        put_varint(&mut body, u64::from(VERSION << 3 | VARINT));
        put_varint(&mut body, 0);
        put_string(&mut body, SOURCE, &self.source);
        put_string(&mut body, DESTINATION, &self.destination);
        put_string(&mut body, NAMESPACE, &self.namespace);
        put_varint(&mut body, u64::from(PAYLOAD_TYPE << 3 | VARINT));
        put_varint(&mut body, 0);
        put_string(&mut body, PAYLOAD_UTF8, &self.payload);
        let mut out = Vec::with_capacity(4 + body.len());
        out.extend_from_slice(&(body.len() as u32).to_be_bytes());
        out.extend_from_slice(&body);
        out
    }

    /// A message's body (without its length), or `None` for one that
    /// isn't a well-formed string message.
    pub fn decode(body: &[u8]) -> Option<Message> {
        let mut at = 0;
        let mut message = Message {
            source: String::new(),
            destination: String::new(),
            namespace: String::new(),
            payload: String::new(),
        };
        let mut has_payload = false;
        while at < body.len() {
            let key = varint(body, &mut at)?;
            let (field, kind) = ((key >> 3) as u32, (key & 7) as u32);
            match kind {
                VARINT => {
                    let value = varint(body, &mut at)?;
                    // A binary payload is none Pley reads.
                    if field == PAYLOAD_TYPE && value != 0 {
                        return None;
                    }
                }
                BYTES => {
                    let len = usize::try_from(varint(body, &mut at)?).ok()?;
                    let bytes = body.get(at..at.checked_add(len)?)?;
                    at += len;
                    let text = || std::str::from_utf8(bytes).ok().map(str::to_owned);
                    match field {
                        SOURCE => message.source = text()?,
                        DESTINATION => message.destination = text()?,
                        NAMESPACE => message.namespace = text()?,
                        PAYLOAD_UTF8 => {
                            message.payload = text()?;
                            has_payload = true;
                        }
                        _ => {}
                    }
                }
                // 64- and 32-bit fixed fields: none in CastMessage.
                1 => at = at.checked_add(8).filter(|end| *end <= body.len())?,
                5 => at = at.checked_add(4).filter(|end| *end <= body.len())?,
                _ => return None,
            }
        }
        has_payload.then_some(message)
    }
}

fn varint(body: &[u8], at: &mut usize) -> Option<u64> {
    let mut value = 0u64;
    for shift in (0..64).step_by(7) {
        let byte = *body.get(*at)?;
        *at += 1;
        value |= u64::from(byte & 0x7F) << shift;
        if byte & 0x80 == 0 {
            return Some(value);
        }
    }
    None
}

/// Reads the next message: `Ok(None)` for one too long or not well formed
/// (the connection is then not to be trusted), an error when it ends.
pub async fn read(reader: &mut (impl AsyncRead + Unpin)) -> std::io::Result<Option<Message>> {
    let len = reader.read_u32().await? as usize;
    if len > MAX_MESSAGE {
        return Ok(None);
    }
    let mut body = vec![0; len];
    reader.read_exact(&mut body).await?;
    Ok(Message::decode(&body))
}

/// Writes `message`.
pub async fn write(writer: &mut (impl AsyncWrite + Unpin), message: &Message) -> std::io::Result<()> {
    writer.write_all(&message.encode()).await?;
    writer.flush().await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message() -> Message {
        Message {
            source: "sender-pley".into(),
            destination: "receiver-0".into(),
            namespace: "urn:x-cast:com.google.cast.tp.connection".into(),
            payload: r#"{"type":"CONNECT"}"#.into(),
        }
    }

    #[test]
    fn a_message_reads_back_as_it_was_written() {
        let bytes = message().encode();
        assert_eq!(u32::from_be_bytes(bytes[..4].try_into().unwrap()) as usize, bytes.len() - 4);
        assert_eq!(Message::decode(&bytes[4..]), Some(message()));
        // Long fields take several bytes of length.
        let long = Message { payload: "x".repeat(300), ..message() };
        assert_eq!(Message::decode(&long.encode()[4..]), Some(long));
    }

    #[test]
    fn what_isnt_a_string_message_is_refused() {
        let bytes = message().encode();
        // Cut short anywhere.
        for end in 4..bytes.len() - 1 {
            assert_eq!(Message::decode(&bytes[4..end]), None, "{end}");
        }
        // A binary payload.
        let mut binary = bytes[4..].to_vec();
        let at = binary.windows(2).position(|w| w == [5 << 3, 0]).unwrap();
        binary[at + 1] = 1;
        assert_eq!(Message::decode(&binary), None);
        // Not UTF-8, and an endless varint.
        let mut wrong = bytes[4..].to_vec();
        let last = wrong.len() - 1;
        wrong[last] = 0xFF;
        assert_eq!(Message::decode(&wrong), None);
        assert_eq!(Message::decode(&[0x80; 12]), None);
    }

    #[tokio::test]
    async fn messages_are_read_one_by_one_and_too_long_ones_refused() {
        let mut bytes = message().encode();
        bytes.extend(Message { payload: "{}".into(), ..message() }.encode());
        let mut reader = &bytes[..];
        assert_eq!(read(&mut reader).await.unwrap(), Some(message()));
        assert_eq!(read(&mut reader).await.unwrap().unwrap().payload, "{}");
        assert!(read(&mut reader).await.is_err(), "the end");
        let huge = ((MAX_MESSAGE + 1) as u32).to_be_bytes();
        assert_eq!(read(&mut &huge[..]).await.unwrap(), None);
    }
}
