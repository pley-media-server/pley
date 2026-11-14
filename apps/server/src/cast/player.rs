//! Playing on a Google Cast device: Google's Default Media Receiver started
//! on it, a stream loaded into it, what it plays followed, and the remote's
//! commands passed on. The device fetches the stream itself, from the
//! session's keyed address.

use std::time::Duration;

use serde_json::{Value, json};
use tokio::sync::mpsc;

use super::connection::{Connection, Incoming, NS_CONNECTION, NS_MEDIA, NS_RECEIVER, PLATFORM};

/// Google's own player, which every Cast device has: it plays HLS and MP4
/// from an address, with WebVTT subtitles beside.
pub const DEFAULT_MEDIA_RECEIVER: &str = "CC1AD845";

/// How long the device may take to start the player, and the player to
/// start the stream (a transcode's first segment comes first).
const LAUNCH_WITHIN: Duration = Duration::from_secs(20);
const LOAD_WITHIN: Duration = Duration::from_secs(60);

/// What to play.
#[derive(Debug, Clone, PartialEq)]
pub struct Media {
    /// Where the device fetches it: the session's keyed address, whole.
    pub url: String,
    /// `application/x-mpegurl` for HLS, the file's type for a direct play.
    pub content_type: String,
    pub start_ms: i64,
    /// Shown by the player and by Google's own remotes.
    pub title: String,
    pub subtitle: Option<Subtitle>,
    /// HLS of fMP4 segments, which Google's player is told of (it takes
    /// MPEG-TS otherwise).
    pub fmp4: bool,
}

/// A text track the player shows, as WebVTT.
#[derive(Debug, Clone, PartialEq)]
pub struct Subtitle {
    pub url: String,
    pub language: Option<String>,
    pub name: String,
}

/// What the player is doing, at `position_ms`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Buffering,
    Playing,
    Paused,
}

/// Why playing on the device ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ended {
    /// It played to the end.
    Finished,
    /// Stopped: from Pley, or on the device (its remote, Google's apps).
    Stopped,
    /// Another app took the device over (someone cast something else).
    TakenOver,
    /// The player couldn't play it.
    Failed,
    /// The device went away, or stopped answering.
    Lost,
}

/// What comes from the device as it plays.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    Status { state: State, position_ms: i64 },
    Ended(Ended),
}

/// Why a cast didn't start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    /// The device didn't start Google's player.
    Launch(String),
    /// The player refused the stream, or didn't start it in time.
    Load(String),
    /// The connection ended.
    Lost,
}

/// Google's player playing a stream on a device.
pub struct Player {
    connection: Connection,
    transport: String,
    /// The player app's session, which a device's own list of apps names.
    app_session: String,
    media_session: i64,
}

impl Player {
    /// Starts Google's player on the device at the other end of
    /// `connection` and loads `media` into it. Its events come on the
    /// receiver returned, until it ends ([`Event::Ended`], then nothing).
    pub async fn start(
        connection: Connection,
        mut incoming: mpsc::Receiver<Incoming>,
        media: &Media,
    ) -> Result<(Player, mpsc::Receiver<Event>), Failure> {
        if !connection.open(PLATFORM).await {
            return Err(Failure::Lost);
        }
        let launch = connection
            .request(PLATFORM, NS_RECEIVER, json!({"type": "LAUNCH", "appId": DEFAULT_MEDIA_RECEIVER}))
            .await
            .ok_or(Failure::Lost)?;
        let (transport, app_session) = tokio::time::timeout(LAUNCH_WITHIN, async {
            loop {
                let message = incoming.recv().await.ok_or(Failure::Lost)?;
                if message.namespace != NS_RECEIVER {
                    continue;
                }
                let p = &message.payload;
                match p["type"].as_str() {
                    Some("RECEIVER_STATUS") => {
                        if let Some(app) = running(p) {
                            return Ok(app);
                        }
                    }
                    Some("LAUNCH_ERROR" | "INVALID_REQUEST") if p["requestId"] == launch => {
                        return Err(Failure::Launch(reason(p)));
                    }
                    _ => {}
                }
            }
        })
        .await
        .map_err(|_| Failure::Launch("the device didn't start the player".into()))??;

        if !connection.open(&transport).await {
            return Err(Failure::Lost);
        }
        let load = connection.request(&transport, NS_MEDIA, load_request(media)).await.ok_or(Failure::Lost)?;
        let media_session = tokio::time::timeout(LOAD_WITHIN, async {
            loop {
                let message = incoming.recv().await.ok_or(Failure::Lost)?;
                let p = &message.payload;
                if message.namespace == NS_RECEIVER && p["type"] == "RECEIVER_STATUS" && running(p).is_none() {
                    return Err(Failure::Load("the player was closed".into()));
                }
                if message.namespace != NS_MEDIA {
                    continue;
                }
                match p["type"].as_str() {
                    Some("MEDIA_STATUS") => {
                        let status = statuses(p).find_map(|s| s["mediaSessionId"].as_i64().map(|id| (id, s)));
                        if let Some((id, s)) = status
                            && s["playerState"] != "IDLE"
                        {
                            return Ok(id);
                        }
                        if let Some((_, s)) = status
                            && s["idleReason"] == "ERROR"
                        {
                            return Err(Failure::Load("the player couldn't play it".into()));
                        }
                    }
                    Some("LOAD_FAILED" | "LOAD_CANCELLED" | "INVALID_REQUEST" | "ERROR") if p["requestId"] == load => {
                        return Err(Failure::Load(reason(p)));
                    }
                    _ => {}
                }
            }
        })
        .await
        .map_err(|_| Failure::Load("the player didn't start the stream".into()))??;

        let (events, received) = mpsc::channel(32);
        tokio::spawn(follow(incoming, events, app_session.clone(), media_session));
        let player = Player { connection, transport, app_session, media_session };
        Ok((player, received))
    }

    async fn media(&self, payload: Value) -> bool {
        let mut payload = payload;
        payload["mediaSessionId"] = json!(self.media_session);
        self.connection.request(&self.transport, NS_MEDIA, payload).await.is_some()
    }

    /// Asks where it is: the answer comes as an [`Event::Status`]. The
    /// player says so itself only when its state changes.
    pub async fn status(&self) -> bool {
        self.media(json!({"type": "GET_STATUS"})).await
    }

    pub async fn play(&self) -> bool {
        self.media(json!({"type": "PLAY"})).await
    }

    pub async fn pause(&self) -> bool {
        self.media(json!({"type": "PAUSE"})).await
    }

    pub async fn seek(&self, position_ms: i64) -> bool {
        self.media(json!({"type": "SEEK", "currentTime": position_ms as f64 / 1000.0})).await
    }

    /// The device's own volume, 0 to 100.
    pub async fn set_volume(&self, percent: u8) -> bool {
        let level = f64::from(percent.min(100)) / 100.0;
        self.connection
            .request(PLATFORM, NS_RECEIVER, json!({"type": "SET_VOLUME", "volume": {"level": level}}))
            .await
            .is_some()
    }

    /// Stops the stream and closes Google's player, which leaves the
    /// device as it was before the cast; then the connection ends.
    pub async fn stop(&self) {
        self.media(json!({"type": "STOP"})).await;
        self.connection.request(PLATFORM, NS_RECEIVER, json!({"type": "STOP", "sessionId": self.app_session})).await;
        self.connection.close();
    }

    /// Lets the device be, playing or not: the connection ends.
    pub fn leave(&self) {
        self.connection.close();
    }
}

/// The LOAD request for `media`.
fn load_request(media: &Media) -> Value {
    let mut info = json!({
        "contentId": media.url,
        "contentUrl": media.url,
        "contentType": media.content_type,
        "streamType": "BUFFERED",
        "metadata": {"metadataType": 0, "title": media.title},
    });
    if media.fmp4 {
        info["hlsSegmentFormat"] = json!("fmp4");
        info["hlsVideoSegmentFormat"] = json!("fmp4");
    }
    let mut active = Vec::new();
    if let Some(subtitle) = &media.subtitle {
        info["tracks"] = json!([{
            "trackId": 1,
            "type": "TEXT",
            "subtype": "SUBTITLES",
            "trackContentId": subtitle.url,
            "trackContentType": "text/vtt",
            "name": subtitle.name,
            "language": subtitle.language,
        }]);
        active.push(1);
    }
    json!({
        "type": "LOAD",
        "media": info,
        "autoplay": true,
        "currentTime": media.start_ms.max(0) as f64 / 1000.0,
        "activeTrackIds": active,
    })
}

/// Google's player, running on the device of a receiver status: its
/// transport (the address its messages go to) and app session.
fn running(status: &Value) -> Option<(String, String)> {
    let apps = status["status"]["applications"].as_array()?;
    let app = apps.iter().find(|a| a["appId"] == DEFAULT_MEDIA_RECEIVER)?;
    Some((app["transportId"].as_str()?.to_owned(), app["sessionId"].as_str()?.to_owned()))
}

/// The media statuses of a MEDIA_STATUS message.
fn statuses(payload: &Value) -> impl Iterator<Item = &Value> {
    payload["status"].as_array().into_iter().flatten()
}

/// What an error message says went wrong, for logs.
fn reason(payload: &Value) -> String {
    let words = payload["reason"].as_str().or(payload["detailedErrorCode"].as_str()).or(payload["type"].as_str());
    words.unwrap_or("unknown").chars().take(80).collect()
}

/// Turns what the device says into [`Event`]s until playing ends.
async fn follow(mut incoming: mpsc::Receiver<Incoming>, events: mpsc::Sender<Event>, app: String, media: i64) {
    let ended = loop {
        let Some(message) = incoming.recv().await else { break Ended::Lost };
        let p = &message.payload;
        match (message.namespace.as_str(), p["type"].as_str()) {
            (NS_RECEIVER, Some("RECEIVER_STATUS")) => {
                let apps = p["status"]["applications"].as_array().into_iter().flatten();
                let ours = apps.clone().any(|a| a["sessionId"] == app.as_str());
                if !ours {
                    // Another app in its place, or none: closed on the device.
                    let other = apps.into_iter().any(|a| a["isIdleScreen"] != true);
                    break if other { Ended::TakenOver } else { Ended::Stopped };
                }
            }
            (NS_CONNECTION, Some("CLOSE")) => break Ended::Stopped,
            (NS_MEDIA, Some("MEDIA_STATUS")) => {
                for s in statuses(p).filter(|s| s["mediaSessionId"] == media) {
                    let position_ms = (s["currentTime"].as_f64().unwrap_or(0.0) * 1000.0) as i64;
                    let state = match s["playerState"].as_str() {
                        Some("PLAYING") => State::Playing,
                        Some("PAUSED") => State::Paused,
                        Some("BUFFERING" | "LOADING") => State::Buffering,
                        Some("IDLE") => {
                            let ended = match s["idleReason"].as_str() {
                                Some("FINISHED") => Ended::Finished,
                                Some("ERROR") => Ended::Failed,
                                // Another stream loaded in its place.
                                Some("INTERRUPTED") => Ended::TakenOver,
                                _ => Ended::Stopped,
                            };
                            let _ = events.send(Event::Ended(ended)).await;
                            return;
                        }
                        _ => continue,
                    };
                    if events.send(Event::Status { state, position_ms }).await.is_err() {
                        return;
                    }
                }
            }
            _ => {}
        }
    };
    let _ = events.send(Event::Ended(ended)).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_load_names_the_stream_where_it_starts_and_its_subtitles() {
        let media = Media {
            url: "http://192.168.1.2:7539/api/media/pley_mk_a/index.m3u8".into(),
            content_type: "application/x-mpegurl".into(),
            start_ms: 600_500,
            title: "1917".into(),
            subtitle: Some(Subtitle {
                url: "http://192.168.1.2:7539/api/media/pley_mk_a/subtitles-2.vtt".into(),
                language: Some("eng".into()),
                name: "English".into(),
            }),
            fmp4: false,
        };
        let load = load_request(&media);
        assert_eq!(load["media"]["contentId"], media.url);
        assert_eq!(load["media"]["streamType"], "BUFFERED");
        assert_eq!(load["currentTime"], 600.5);
        assert_eq!(load["media"]["tracks"][0]["trackContentId"], media.subtitle.as_ref().unwrap().url);
        assert_eq!(load["activeTrackIds"], json!([1]));
        assert!(load["media"].get("hlsSegmentFormat").is_none());
        let plain = load_request(&Media { subtitle: None, fmp4: true, ..media });
        assert!(plain["media"].get("tracks").is_none());
        assert_eq!(plain["activeTrackIds"], json!([]));
        assert_eq!(
            (plain["media"]["hlsSegmentFormat"].clone(), plain["media"]["hlsVideoSegmentFormat"].clone()),
            (json!("fmp4"), json!("fmp4"))
        );
    }

    use super::super::frame::{self, Message};
    use tokio::io::DuplexStream;

    /// A Cast device at the other end of an in-memory stream: what Pley
    /// sends comes out of `next`, and `say` answers.
    struct Device {
        stream: DuplexStream,
    }

    impl Device {
        /// The next message Pley sent, heartbeats aside.
        async fn next(&mut self) -> Message {
            loop {
                let message = frame::read(&mut self.stream).await.unwrap().unwrap();
                if !message.namespace.ends_with("heartbeat") {
                    return message;
                }
            }
        }

        async fn say(&mut self, source: &str, namespace: &str, payload: Value) {
            let message = Message {
                source: source.into(),
                destination: "sender-pley".into(),
                namespace: namespace.into(),
                payload: payload.to_string(),
            };
            frame::write(&mut self.stream, &message).await.unwrap();
        }

        fn status(apps: Value) -> Value {
            json!({"type": "RECEIVER_STATUS", "requestId": 0, "status": {"applications": apps}})
        }

        /// Answers a launch and a load as a Chromecast does, then is
        /// playing at 10 s.
        async fn starts_playing(&mut self) -> Value {
            assert_eq!(self.next().await.payload, r#"{"type":"CONNECT"}"#);
            let launch: Value = serde_json::from_str(&self.next().await.payload).unwrap();
            assert_eq!(launch["appId"], DEFAULT_MEDIA_RECEIVER);
            let app = json!([{"appId": DEFAULT_MEDIA_RECEIVER, "transportId": "web-5", "sessionId": "s-1"}]);
            self.say(PLATFORM, NS_RECEIVER, Device::status(app)).await;
            let connect = self.next().await;
            assert_eq!((connect.destination.as_str(), connect.payload.as_str()), ("web-5", r#"{"type":"CONNECT"}"#));
            let load = self.next().await;
            assert_eq!(load.destination, "web-5");
            let load: Value = serde_json::from_str(&load.payload).unwrap();
            self.say(
                "web-5",
                NS_MEDIA,
                json!({"type": "MEDIA_STATUS", "requestId": load["requestId"], "status": [
                    {"mediaSessionId": 7, "playerState": "BUFFERING", "currentTime": 10.0}
                ]}),
            )
            .await;
            load
        }
    }

    fn media() -> Media {
        Media {
            url: "http://pley/api/media/pley_mk_a/index.m3u8".into(),
            content_type: "application/x-mpegurl".into(),
            start_ms: 10_000,
            title: "Heat".into(),
            subtitle: None,
            fmp4: false,
        }
    }

    fn pair() -> (Connection, mpsc::Receiver<Incoming>, Device) {
        let (ours, theirs) = tokio::io::duplex(64 * 1024);
        let (connection, incoming) = Connection::over(ours);
        (connection, incoming, Device { stream: theirs })
    }

    #[tokio::test]
    async fn a_cast_starts_plays_takes_commands_and_finishes() {
        let m = media();
        let (connection, incoming, mut device) = pair();
        let (started, load) = tokio::join!(Player::start(connection, incoming, &m), device.starts_playing());
        assert_eq!(load["media"]["contentId"], media().url);
        let (player, mut events) = started.unwrap();

        device
            .say(
                "web-5",
                NS_MEDIA,
                json!({"type": "MEDIA_STATUS", "requestId": 0, "status": [
                    {"mediaSessionId": 7, "playerState": "PLAYING", "currentTime": 12.5}
                ]}),
            )
            .await;
        assert_eq!(events.recv().await, Some(Event::Status { state: State::Playing, position_ms: 12_500 }));

        assert!(player.seek(60_000).await);
        let seek: Value = serde_json::from_str(&device.next().await.payload).unwrap();
        assert_eq!(
            (seek["type"].as_str(), seek["currentTime"].as_f64(), seek["mediaSessionId"].as_i64()),
            (Some("SEEK"), Some(60.0), Some(7))
        );
        assert!(player.pause().await);
        assert!(device.next().await.payload.contains(r#""type":"PAUSE""#));
        assert!(player.set_volume(40).await);
        let volume = device.next().await;
        assert_eq!(volume.destination, PLATFORM);
        assert!(volume.payload.contains(r#""level":0.4"#));

        device
            .say(
                "web-5",
                NS_MEDIA,
                json!({"type": "MEDIA_STATUS", "requestId": 0, "status": [
                    {"mediaSessionId": 7, "playerState": "IDLE", "idleReason": "FINISHED", "currentTime": 0.0}
                ]}),
            )
            .await;
        assert_eq!(events.recv().await, Some(Event::Ended(Ended::Finished)));
        assert_eq!(events.recv().await, None);
    }

    #[tokio::test]
    async fn stopping_closes_the_player_on_the_device() {
        let m = media();
        let (connection, incoming, mut device) = pair();
        let (started, _) = tokio::join!(Player::start(connection, incoming, &m), device.starts_playing());
        let (player, _events) = started.unwrap();
        player.stop().await;
        assert!(device.next().await.payload.contains(r#""type":"STOP""#));
        let close = device.next().await;
        assert_eq!(close.destination, PLATFORM);
        assert!(close.payload.contains(r#""sessionId":"s-1""#));
    }

    #[tokio::test]
    async fn another_app_taking_the_device_ends_the_cast() {
        let m = media();
        let (connection, incoming, mut device) = pair();
        let (started, _) = tokio::join!(Player::start(connection, incoming, &m), device.starts_playing());
        let (_player, mut events) = started.unwrap();
        let youtube = json!([{"appId": "233637DE", "transportId": "web-9", "sessionId": "s-2"}]);
        device.say(PLATFORM, NS_RECEIVER, Device::status(youtube)).await;
        assert_eq!(events.recv().await, Some(Event::Ended(Ended::TakenOver)));
        // Closed on the device, back to its home screen.
        let (connection, incoming, mut device) = pair();
        let (started, _) = tokio::join!(Player::start(connection, incoming, &m), device.starts_playing());
        let (_player, mut events) = started.unwrap();
        let home = json!([{"appId": "E8C28D3C", "transportId": "web-1", "sessionId": "b", "isIdleScreen": true}]);
        device.say(PLATFORM, NS_RECEIVER, Device::status(home)).await;
        assert_eq!(events.recv().await, Some(Event::Ended(Ended::Stopped)));
    }

    #[tokio::test]
    async fn a_refused_load_or_a_vanished_device_fails_the_start() {
        let m = media();
        let (connection, incoming, mut device) = pair();
        let refuse = async {
            device.next().await;
            device.next().await;
            let app = json!([{"appId": DEFAULT_MEDIA_RECEIVER, "transportId": "web-5", "sessionId": "s-1"}]);
            device.say(PLATFORM, NS_RECEIVER, Device::status(app)).await;
            device.next().await;
            let load: Value = serde_json::from_str(&device.next().await.payload).unwrap();
            device.say("web-5", NS_MEDIA, json!({"type": "LOAD_FAILED", "requestId": load["requestId"]})).await;
        };
        let (started, ()) = tokio::join!(Player::start(connection, incoming, &m), refuse);
        assert!(matches!(started, Err(Failure::Load(_))));

        let (connection, incoming, device) = pair();
        drop(device);
        assert!(matches!(Player::start(connection, incoming, &m).await, Err(Failure::Lost)));
    }

    #[test]
    fn the_player_is_found_among_the_devices_apps() {
        let status = json!({"type": "RECEIVER_STATUS", "status": {"applications": [
            {"appId": "E8C28D3C", "transportId": "web-1", "sessionId": "b", "isIdleScreen": true},
            {"appId": DEFAULT_MEDIA_RECEIVER, "transportId": "web-2", "sessionId": "a"},
        ]}});
        assert_eq!(running(&status), Some(("web-2".into(), "a".into())));
        assert_eq!(running(&json!({"status": {}})), None);
    }
}
