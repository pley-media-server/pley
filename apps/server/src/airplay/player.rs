//! Playing on an AirPlay device, as tvOS 26 and later take it: a session
//! set up over the paired connection (RTSP `SETUP` with a timing port, the
//! device's event connection, `RECORD`), a remote-control stream in it,
//! and commands posted to that stream: the stream's address put in the
//! device's play queue, the rate set, the place asked. The device answers
//! on its event connection. Seeking puts the address in the queue again,
//! at the new place, and takes the last one out.
//!
//! The device fetches the stream itself, from the session's keyed address.
//! It shows no subtitles from beside the stream, and keeps its volume to
//! itself.

use std::io;
use std::net::{IpAddr, SocketAddr};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use plist::{Dictionary, Value};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::{TcpStream, UdpSocket};
use tokio::sync::{mpsc, oneshot};

use super::connection::{Channel, Keyed};
use crate::nearby::player::{Ended, Event, Failure, Media, State};

/// How long the device may take to start the stream (a transcode's first
/// segment comes first), how often it is asked where it is while it
/// starts, and how often it is told the sender is there.
const LOAD_WITHIN: Duration = Duration::from_secs(60);
const ASK_WHILE_STARTING: Duration = Duration::from_secs(2);
const FEEDBACK_EVERY: Duration = Duration::from_secs(2);
/// A stream paused this close to its end has played to it.
const AT_THE_END_MS: i64 = 2_000;

/// What Apple's senders call a remote-control stream's client type.
const CLIENT_TYPE: &str = "A6B27562-B43A-4F2D-B75F-82391E250194";

fn bplist(value: Value) -> Vec<u8> {
    let mut out = Vec::new();
    value.to_writer_binary(&mut out).expect("a plist in memory");
    out
}

fn read_plist(bytes: &[u8]) -> Option<Value> {
    super::plists::read(bytes)
}

fn dict<const N: usize>(pairs: [(&str, Value); N]) -> Value {
    Value::Dictionary(pairs.into_iter().map(|(k, v)| (k.to_owned(), v)).collect())
}

fn uuid() -> String {
    uuid::Uuid::new_v4().to_string().to_uppercase()
}

/// A place as AirPlay's commands take it: a CMTime of milliseconds.
fn cm_time(ms: i64) -> Value {
    dict([
        ("value", Value::Integer(ms.max(0).into())),
        ("timescale", Value::Integer(1000.into())),
        ("flags", Value::Integer(1.into())),
        ("epoch", Value::Integer(0.into())),
    ])
}

/// A place the device says, in milliseconds: a CMTime, or seconds.
fn read_time(value: &Value) -> Option<i64> {
    match value {
        Value::Dictionary(d) => {
            let number = |v: &Value| v.as_signed_integer().map(|n| n as f64).or_else(|| v.as_real());
            let v = number(d.get("value")?)?;
            let scale = number(d.get("timescale")?)?;
            // Flags without "valid" (1) are no time: an unknown length.
            let valid = d.get("flags").and_then(Value::as_signed_integer).is_none_or(|f| f & 1 != 0);
            (valid && scale > 0.0).then(|| (v * 1000.0 / scale) as i64)
        }
        Value::Real(seconds) if seconds.is_finite() => Some((seconds * 1000.0) as i64),
        Value::Integer(seconds) => seconds.as_signed().map(|s| s.saturating_mul(1000)),
        _ => None,
    }
}

/// The session set up with a paired device, ready to play.
pub struct Session<S> {
    control: Channel<S>,
    /// What came on the device's event connection, read by a task of its
    /// own; `None` when it ended.
    notices: mpsc::Receiver<Option<Dictionary>>,
    session_id: String,
    stream_id: String,
    rtsp_url: String,
    /// Answers the device's clock: ends with the session.
    _timing: Option<Timing>,
}

/// The task answering a session's clock, ended when it is dropped.
struct Timing(tokio::task::JoinHandle<()>);

impl Drop for Timing {
    fn drop(&mut self) {
        self.0.abort();
    }
}

impl Session<TcpStream> {
    /// Sets up a session over `control`, paired and keyed with `secret`.
    /// `sender` is the address the server sends from; `name` and `id`,
    /// how the device knows it.
    pub async fn open(mut control: Channel<TcpStream>, secret: &[u8], device: IpAddr, id: &str) -> io::Result<Self> {
        let local = control.local_address()?.ip();
        let dacp = format!("{:016X}", rand_u64());
        control.rtsp_headers(vec![
            ("DACP-ID", dacp.clone()),
            ("Active-Remote", format!("{}", rand_u64() as u32)),
            ("Client-Instance", dacp),
        ]);
        let timing = UdpSocket::bind(SocketAddr::new(local, 0)).await?;
        let timing_port = timing.local_addr()?.port();
        let timing = Timing(tokio::spawn(answer_clock(timing)));
        let session_id = uuid();
        let mac = sender_mac(id);
        let rtsp_url = format!("rtsp://{local}/{}", rand_u64() as u32);
        let setup = dict([
            ("timingProtocol", "NTP".into()),
            ("timingPort", Value::Integer(u64::from(timing_port).into())),
            ("isMultiSelectAirPlay", true.into()),
            ("groupContainsGroupLeader", false.into()),
            ("senderSupportsRelay", false.into()),
            ("statsCollectionEnabled", false.into()),
            ("sessionUUID", session_id.clone().into()),
            ("sessionCorrelationUUID", uuid().into()),
            ("deviceID", mac.clone().into()),
            ("macAddress", mac.clone().into()),
            ("name", "Pley".into()),
            ("model", "Pley".into()),
            ("osName", "Pley".into()),
            ("sourceVersion", "550.10".into()),
        ]);
        let plist_type = [("Content-Type", "application/x-apple-binary-plist")];
        let answer = control.request("SETUP", &rtsp_url, true, &plist_type, &bplist(setup)).await?;
        let event_port = read_plist(&answer.body)
            .and_then(|v| v.as_dictionary()?.get("eventPort")?.as_unsigned_integer())
            .and_then(|p| u16::try_from(p).ok())
            .filter(|_| answer.status() == 200)
            .ok_or_else(|| invalid("the device set up no event connection"))?;
        let events = tokio::time::timeout(
            super::connection::CONNECT_WITHIN,
            TcpStream::connect(SocketAddr::new(device, event_port)),
        )
        .await
        .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "the device's events didn't answer"))??;
        let mut events = Channel::over(events);
        events.encrypt(secret, Keyed::Events);
        let (notices, received) = mpsc::channel(32);
        tokio::spawn(read_events(events, notices));
        expect_ok(control.request("RECORD", &rtsp_url, true, &[], &[]).await?)?;
        // Apple's senders ask its info and record again before the
        // remote-control stream. An Apple TV answers the second RECORD with
        // 500 and plays all the same, so its answer is only logged.
        let _ = control.request("GET", "/info", true, &[], &[]).await?;
        let again = control.request("RECORD", &rtsp_url, true, &[], &[]).await?;
        tracing::debug!("an AirPlay device answered RECORD again with {}", again.status());
        let stream = dict([
            ("clientUUID", uuid().into()),
            ("clientTypeUUID", CLIENT_TYPE.into()),
            ("channelID", format!("{mac}-RCS-1").into()),
            ("controlType", Value::Integer(1.into())),
            ("type", Value::Integer(130.into())),
        ]);
        let body = bplist(dict([("streams", Value::Array(vec![stream]))]));
        let answer = control.request("SETUP", &rtsp_url, true, &plist_type, &body).await?;
        let stream_id = read_plist(&answer.body)
            .and_then(|v| {
                let id =
                    v.as_dictionary()?.get("streams")?.as_array()?.first()?.as_dictionary()?.get("streamID")?.clone();
                match id {
                    Value::Integer(n) => Some(n.to_string()),
                    Value::String(s) => Some(s),
                    _ => None,
                }
            })
            .filter(|_| answer.status() == 200)
            .ok_or_else(|| invalid("the device set up no remote-control stream"))?;
        Ok(Session { control, notices: received, session_id, stream_id, rtsp_url, _timing: Some(timing) })
    }
}

fn invalid(what: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, what.to_owned())
}

fn expect_ok(answer: super::connection::Message) -> io::Result<()> {
    match answer.status() {
        200 => Ok(()),
        status => Err(invalid(&format!("the device answered {status}"))),
    }
}

fn rand_u64() -> u64 {
    let bytes = uuid::Uuid::new_v4().into_bytes();
    u64::from_le_bytes(bytes[..8].try_into().expect("8 bytes"))
}

/// A MAC address the device knows the server by, the same for one
/// pairing id: locally administered, so no real device's.
fn sender_mac(id: &str) -> String {
    let hash = super::crypto::sha512(&[id.as_bytes()]);
    let mut bytes = [hash[0], hash[1], hash[2], hash[3], hash[4], hash[5]];
    bytes[0] = (bytes[0] | 0x02) & 0xFE;
    bytes.iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(":")
}

/// Answers the device's NTP requests on `socket` with the server's clock,
/// as the session's timing port: the device waits for its clock before it
/// plays.
async fn answer_clock(socket: UdpSocket) {
    let mut buf = [0u8; 128];
    loop {
        let Ok((len, from)) = socket.recv_from(&mut buf).await else { return };
        if len < 32 {
            continue;
        }
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
        // NTP counts from 1900.
        let seconds = (now.as_secs() + 2_208_988_800) as u32;
        let fraction = ((u64::from(now.subsec_nanos()) << 32) / 1_000_000_000) as u32;
        let mut out = [0u8; 32];
        out[0] = buf[0];
        out[1] = 0xD3;
        out[2..4].copy_from_slice(&7u16.to_be_bytes());
        // Its send time, as the reference it measures by.
        out[8..16].copy_from_slice(&buf[24..32]);
        for at in [16, 24] {
            out[at..at + 4].copy_from_slice(&seconds.to_be_bytes());
            out[at + 4..at + 8].copy_from_slice(&fraction.to_be_bytes());
        }
        let _ = socket.send_to(&out, from).await;
    }
}

/// Reads what the device says on its event connection, answers each, and
/// passes on what it says; `None` when the connection ends.
async fn read_events<S: AsyncRead + AsyncWrite + Unpin>(
    mut events: Channel<S>,
    notices: mpsc::Sender<Option<Dictionary>>,
) {
    loop {
        let Ok(request) = events.read().await else { break };
        if events.answer(&request).await.is_err() {
            break;
        }
        if let Some(notice) = notice(&request.body)
            && notices.send(Some(notice)).await.is_err()
        {
            return;
        }
    }
    let _ = notices.send(None).await;
}

/// What an event says, its fields in one dictionary: a command's are in
/// `params.data`, a binary plist of its own; others' in `params`, or the
/// event itself.
fn notice(body: &[u8]) -> Option<Dictionary> {
    let outer = read_plist(body)?.into_dictionary()?;
    let params = outer.get("params").and_then(Value::as_dictionary);
    if let Some(data) = params.and_then(|p| p.get("data")).and_then(Value::as_data) {
        return read_plist(data)?.into_dictionary();
    }
    Some(params.cloned().unwrap_or(outer))
}

/// What a notice tells of the stream.
#[derive(Debug, Default, Clone, PartialEq)]
struct Told {
    state: Option<State>,
    position_ms: Option<i64>,
    duration_ms: Option<i64>,
    rate: Option<f64>,
    /// The item now playing, when it changed: none when nothing plays.
    item_changed: Option<Option<String>>,
    failed: bool,
}

fn read_notice(notice: &Dictionary) -> Told {
    // A playbackInfo answer's fields are in `info`.
    let info = notice.get("info").and_then(Value::as_dictionary).unwrap_or(notice);
    let mut told = Told {
        position_ms: info.get("position").and_then(read_time),
        duration_ms: info.get("duration").and_then(read_time).filter(|d| *d > 0),
        rate: info.get("rate").and_then(|r| r.as_real().or_else(|| r.as_signed_integer().map(|n| n as f64))),
        ..Told::default()
    };
    let flag = |key: &str| info.get(key).and_then(Value::as_boolean);
    let name = notice.get("name").and_then(Value::as_string).unwrap_or_default();
    told.state = match (notice.get("type").and_then(Value::as_string), name) {
        (Some("playbackState"), "playing") => Some(State::Playing),
        (Some("playbackState"), "paused") => Some(State::Paused),
        (Some("playbackState"), "loading" | "buffering" | "waiting") => Some(State::Buffering),
        _ if flag("playbackBufferEmpty") == Some(true) || flag("readyToPlay") == Some(false) => Some(State::Buffering),
        _ => match told.rate {
            Some(rate) if rate > 0.0 => Some(State::Playing),
            Some(_) => Some(State::Paused),
            None => None,
        },
    };
    if name == "currentItemChanged" {
        let current = notice.get("itemCurrent").and_then(Value::as_dictionary).and_then(|i| i.get("uuid"));
        told.item_changed = Some(current.and_then(Value::as_string).map(str::to_owned));
    }
    told.failed = info.contains_key("error") || name.to_ascii_lowercase().contains("fail");
    told
}

/// What the remotes ask of the player.
enum Command {
    Status,
    Rate(f64),
    Seek(i64),
    Stop(oneshot::Sender<()>),
}

/// The device's player, playing a cast's stream.
pub struct Player {
    commands: mpsc::Sender<Command>,
}

impl<S: AsyncRead + AsyncWrite + Unpin + Send + 'static> Session<S> {
    /// Puts `media` in the device's queue and plays it. Its events come on
    /// the receiver returned, until it ends ([`Event::Ended`], then nothing).
    pub async fn start(mut self, media: &Media) -> Result<(Player, mpsc::Receiver<Event>), Failure> {
        let item = uuid();
        let insert = insert(&item, &media.url, media.start_ms);
        let lost = |_| Failure::Lost;
        self.command(insert).await.map_err(lost)?;
        let date_range = dict([
            ("type", "setProperty".into()),
            ("property", "isInterestedInDateRange".into()),
            ("value", true.into()),
            ("item", dict([("uuid", item.clone().into())])),
        ]);
        self.command(date_range).await.map_err(lost)?;
        // Paused at its end, the stream stays the device's last item, and
        // its place says it played to the end.
        let at_end = dict([
            ("type", "setProperty".into()),
            ("property", "actionAtItemEnd".into()),
            ("value", Value::Integer(1.into())),
        ]);
        self.command(at_end).await.map_err(lost)?;
        self.command(rate(1.0)).await.map_err(lost)?;

        let (commands, received) = mpsc::channel(8);
        let (events, ours) = mpsc::channel(32);
        let (started, starting) = oneshot::channel();
        let playing = Playing { item, position_ms: media.start_ms, ..Playing::default() };
        tokio::spawn(self.drive(playing, media.url.clone(), received, events, started));
        match tokio::time::timeout(LOAD_WITHIN, starting).await {
            Ok(Ok(Ok(()))) => Ok((Player { commands }, ours)),
            Ok(Ok(Err(failure))) => Err(failure),
            Ok(Err(_)) => Err(Failure::Lost),
            Err(_) => Err(Failure::NotStarted("the device didn't start the stream".into())),
        }
    }

    /// Posts `command` to the remote-control stream.
    async fn command(&mut self, command: Value) -> io::Result<()> {
        let body = bplist(dict([("params", dict([("data", Value::Data(bplist(command)))]))]));
        let headers = [
            ("Content-Type", "application/x-apple-binary-plist"),
            ("X-Apple-ProtocolVersion", "1"),
            ("X-Apple-Session-ID", self.session_id.as_str()),
            ("X-Apple-StreamID", self.stream_id.as_str()),
            ("CSeq", "1"),
        ];
        expect_ok(self.control.request("POST", "/command", false, &headers, &body).await?)
    }

    /// Follows the stream until it ends: the device's notices read, the
    /// remotes' commands sent, the device told the sender is there.
    async fn drive(
        mut self,
        mut playing: Playing,
        url: String,
        mut commands: mpsc::Receiver<Command>,
        events: mpsc::Sender<Event>,
        started: oneshot::Sender<Result<(), Failure>>,
    ) {
        let mut started = Some(started);
        let mut feedback = tokio::time::interval(FEEDBACK_EVERY);
        let mut asking = tokio::time::interval(ASK_WHILE_STARTING);
        let mut asked = 0u64;
        let ended = loop {
            tokio::select! {
                // What the remotes asked before what the device said since.
                biased;
                command = commands.recv() => {
                    let done = match command {
                        // The cast left the device to another.
                        None => return,
                        Some(Command::Status) => {
                            asked += 1;
                            self.command(info_request(asked)).await
                        }
                        Some(Command::Rate(r)) => self.command(rate(r)).await,
                        Some(Command::Seek(ms)) => {
                            let next = uuid();
                            let moved = async {
                                self.command(insert(&next, &url, ms)).await?;
                                let remove = dict([
                                    ("type", "removePlayQueueItem".into()),
                                    ("item", dict([("uuid", playing.item.clone().into())])),
                                ]);
                                self.command(remove).await
                            };
                            let moved = moved.await;
                            if moved.is_ok() {
                                playing.item = next;
                                playing.position_ms = ms;
                            }
                            moved
                        }
                        Some(Command::Stop(done)) => {
                            let _ = self.control.request("TEARDOWN", &self.rtsp_url.clone(), true, &[], &[]).await;
                            let _ = done.send(());
                            return;
                        }
                    };
                    if done.is_err() {
                        break Ended::Lost;
                    }
                }
                notice = self.notices.recv() => {
                    // The device closed the session: its remote, another
                    // sender, or gone.
                    let Some(Some(notice)) = notice else { break playing.closed() };
                    let told = read_notice(&notice);
                    tracing::debug!("an AirPlay device said {told:?}");
                    match playing.update(told) {
                        Some(Update::Status(state)) => {
                            if let Some(started) = started.take() {
                                let _ = started.send(Ok(()));
                            }
                            let status = Event::Status { state, position_ms: playing.position_ms };
                            if events.send(status).await.is_err() {
                                return;
                            }
                        }
                        Some(Update::Ended(ended)) => break ended,
                        None => {}
                    }
                }
                _ = feedback.tick() => {
                    if self.control.request("POST", "/feedback", true, &[], &[]).await.is_err() {
                        break Ended::Lost;
                    }
                }
                _ = asking.tick(), if started.is_some() => {
                    asked += 1;
                    if self.command(info_request(asked)).await.is_err() {
                        break Ended::Lost;
                    }
                }
            }
        };
        match started.take() {
            Some(started) => {
                let failure = match ended {
                    Ended::Failed => Failure::Refused("the device couldn't play it".into()),
                    Ended::Lost => Failure::Lost,
                    _ => Failure::NotStarted("the device stopped before it played".into()),
                };
                let _ = started.send(Err(failure));
            }
            None => {
                let _ = events.send(Event::Ended(ended)).await;
            }
        }
    }
}

fn insert(item: &str, url: &str, start_ms: i64) -> Value {
    dict([
        ("type", "insertPlayQueueItem".into()),
        (
            "item",
            dict([
                ("uuid", item.into()),
                ("mediaType", "file".into()),
                ("Content-Location", url.into()),
                ("Start-Position", cm_time(start_ms)),
            ]),
        ),
    ])
}

fn rate(rate: f64) -> Value {
    dict([("type", "setRate".into()), ("rate", Value::Real(rate))])
}

fn info_request(id: u64) -> Value {
    dict([("type", "playbackInfo".into()), ("kind", "request".into()), ("messageID", Value::Integer(id.into()))])
}

/// What Pley knows of the stream on the device.
#[derive(Debug, Default)]
struct Playing {
    item: String,
    position_ms: i64,
    duration_ms: i64,
    state: Option<State>,
}

enum Update {
    Status(State),
    Ended(Ended),
}

impl Playing {
    fn update(&mut self, told: Told) -> Option<Update> {
        if let Some(duration) = told.duration_ms {
            self.duration_ms = duration;
        }
        if let Some(position) = told.position_ms {
            self.position_ms = position;
        }
        if told.failed {
            return Some(Update::Ended(Ended::Failed));
        }
        match told.item_changed {
            Some(Some(item)) if item == self.item => {}
            // Something else in its place: another sender's.
            Some(Some(_)) => return Some(Update::Ended(Ended::TakenOver)),
            Some(None) => return Some(Update::Ended(self.closed())),
            None => {}
        }
        let state = told.state?;
        if state == State::Paused && self.at_the_end() {
            return Some(Update::Ended(Ended::Finished));
        }
        self.state = Some(state);
        Some(Update::Status(state))
    }

    fn at_the_end(&self) -> bool {
        self.duration_ms > 0 && self.position_ms >= self.duration_ms - AT_THE_END_MS
    }

    /// Why it ended when the device let it go: at the end, it played to it.
    fn closed(&self) -> Ended {
        if self.at_the_end() { Ended::Finished } else { Ended::Stopped }
    }
}

impl Player {
    /// Asks where it is: the answer comes as an [`Event::Status`].
    pub async fn status(&self) -> bool {
        self.commands.send(Command::Status).await.is_ok()
    }

    pub async fn play(&self) -> bool {
        self.commands.send(Command::Rate(1.0)).await.is_ok()
    }

    pub async fn pause(&self) -> bool {
        self.commands.send(Command::Rate(0.0)).await.is_ok()
    }

    pub async fn seek(&self, position_ms: i64) -> bool {
        self.commands.send(Command::Seek(position_ms)).await.is_ok()
    }

    /// Ends the session, which leaves the device as it was before the
    /// cast.
    pub async fn stop(&self) {
        let (done, stopped) = oneshot::channel();
        if self.commands.send(Command::Stop(done)).await.is_ok() {
            let _ = stopped.await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(position: f64, duration: f64, rate: f64) -> Dictionary {
        let info = dict([
            ("position", cm_time((position * 1000.0) as i64)),
            ("duration", Value::Real(duration)),
            ("rate", Value::Real(rate)),
            ("readyToPlay", true.into()),
            ("playbackBufferEmpty", false.into()),
        ]);
        dict([("kind", "response".into()), ("info", info)]).into_dictionary().unwrap()
    }

    #[test]
    fn what_the_device_says_reads_as_a_state_a_place_and_an_end() {
        let told = read_notice(&info(61.5, 5400.0, 1.0));
        assert_eq!(
            (told.state, told.position_ms, told.duration_ms),
            (Some(State::Playing), Some(61_500), Some(5_400_000))
        );
        assert_eq!(read_notice(&info(61.5, 5400.0, 0.0)).state, Some(State::Paused));
        let loading = dict([("type", "playbackState".into()), ("name", "loading".into())]).into_dictionary().unwrap();
        assert_eq!(read_notice(&loading).state, Some(State::Buffering));
        // An unknown length is no length.
        let unknown = dict([("duration", dict([("value", 0.into()), ("timescale", 0.into()), ("flags", 0.into())]))]);
        assert_eq!(read_notice(&unknown.into_dictionary().unwrap()).duration_ms, None);

        let mut playing = Playing { item: "A".into(), ..Playing::default() };
        assert!(matches!(playing.update(read_notice(&info(10.0, 100.0, 1.0))), Some(Update::Status(State::Playing))));
        assert!(matches!(playing.update(read_notice(&info(20.0, 100.0, 0.0))), Some(Update::Status(State::Paused))));
        assert!(matches!(playing.update(read_notice(&info(99.0, 100.0, 0.0))), Some(Update::Ended(Ended::Finished))));
        let other = dict([("name", "currentItemChanged".into()), ("itemCurrent", dict([("uuid", "B".into())]))]);
        let mut playing = Playing { item: "A".into(), ..Playing::default() };
        assert!(matches!(
            playing.update(read_notice(&other.into_dictionary().unwrap())),
            Some(Update::Ended(Ended::TakenOver))
        ));
        assert_eq!(playing.closed(), Ended::Stopped);
    }

    #[test]
    fn an_event_reads_from_its_command_data_or_its_params() {
        let inner = bplist(dict([("type", "playbackState".into()), ("name", "paused".into())]));
        let wrapped = bplist(dict([("params", dict([("data", Value::Data(inner))]))]));
        assert_eq!(notice(&wrapped).unwrap().get("name").and_then(Value::as_string), Some("paused"));
        let plain = bplist(dict([("params", dict([("name", "timeJumped".into())]))]));
        assert_eq!(notice(&plain).unwrap().get("name").and_then(Value::as_string), Some("timeJumped"));
        assert_eq!(notice(b"not a plist"), None);
        assert_eq!(read_time(&cm_time(90_500)), Some(90_500));
        assert!(sender_mac("x").starts_with(|c: char| c.is_ascii_hexdigit()));
        assert_eq!(sender_mac("x"), sender_mac("x"));
    }

    /// A device's end of the control connection: the commands Pley posts
    /// come out of `next`, each answered 200.
    async fn next_command(device: &mut Channel<tokio::io::DuplexStream>) -> (String, Option<Dictionary>) {
        let request = device.read().await.unwrap();
        device.reply("HTTP/1.1 200 OK", &[], &[]).await.unwrap();
        (request.first.clone(), notice(&request.body))
    }

    #[tokio::test]
    async fn a_stream_starts_seeks_and_ends_where_the_device_says() {
        let (ours, theirs) = tokio::io::duplex(64 * 1024);
        let (tell, notices) = mpsc::channel(8);
        let session = Session {
            control: Channel::over(ours),
            notices,
            session_id: "S".into(),
            stream_id: "7".into(),
            rtsp_url: "rtsp://pley/1".into(),
            _timing: None,
        };
        let mut device = Channel::over(theirs);
        let media = Media {
            url: "http://192.168.1.2:7539/api/media/k/index.m3u8".into(),
            content_type: "application/x-mpegurl".into(),
            start_ms: 30_000,
            title: "1917".into(),
            subtitle: None,
            fmp4: true,
        };
        let starting = tokio::spawn(async move { session.start(&media).await });
        let (_, insert) = next_command(&mut device).await;
        let item = insert.as_ref().unwrap()["item"].as_dictionary().unwrap();
        assert_eq!(read_time(&item["Start-Position"]), Some(30_000));
        let first = item["uuid"].as_string().unwrap().to_owned();
        for _ in 0..3 {
            next_command(&mut device).await;
        }
        tell.send(Some(info(30.0, 100.0, 1.0))).await.unwrap();
        // Asked where it is while it starts, and told the sender is there.
        let answering = tokio::spawn(async move {
            let mut seen = Vec::new();
            while let Ok(request) = device.read().await {
                device.reply("HTTP/1.1 200 OK", &[], &[]).await.unwrap();
                seen.push((request.first.clone(), notice(&request.body)));
            }
            seen
        });
        let (player, mut events) = starting.await.unwrap().unwrap();
        assert_eq!(events.recv().await, Some(Event::Status { state: State::Playing, position_ms: 30_000 }));
        assert!(player.seek(60_000).await);
        tell.send(Some(info(99.0, 100.0, 0.0))).await.unwrap();
        assert_eq!(events.recv().await, Some(Event::Ended(Ended::Finished)));
        drop((player, tell));
        let seen = answering.await.unwrap();
        let types: Vec<&str> = seen.iter().filter_map(|(_, n)| n.as_ref()?.get("type")?.as_string()).collect();
        let insert = types.iter().position(|t| *t == "insertPlayQueueItem").unwrap();
        assert_eq!(types[insert + 1], "removePlayQueueItem");
        let removed = seen
            .iter()
            .filter_map(|(_, n)| n.as_ref())
            .find(|n| n.get("type").and_then(Value::as_string) == Some("removePlayQueueItem"));
        assert_eq!(removed.unwrap()["item"].as_dictionary().unwrap()["uuid"].as_string(), Some(first.as_str()));
    }
}
