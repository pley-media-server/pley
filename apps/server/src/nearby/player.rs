//! A cast's player on a device, whatever the protocol: what it is given to
//! play, what it says as it plays, and the commands it takes.

use tokio::sync::mpsc;

/// What to play.
#[derive(Debug, Clone, PartialEq)]
pub struct Media {
    /// Where the device fetches it: the session's keyed address, whole.
    pub url: String,
    /// `application/x-mpegurl` for HLS, the file's type for a direct play.
    pub content_type: String,
    pub start_ms: i64,
    /// Shown by the player and by the device's own remotes.
    pub title: String,
    pub subtitle: Option<Subtitle>,
    /// HLS of fMP4 segments, which some players are told of (Google's
    /// takes MPEG-TS otherwise).
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
    /// Stopped: from Pley, or on the device (its remote, its own apps).
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
    Status {
        state: State,
        position_ms: i64,
    },
    /// The device's volume, 0 to 100 (0 while it is muted).
    Volume(u8),
    Ended(Ended),
}

/// Why a cast didn't start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    /// The player refused the stream: another may do.
    Refused(String),
    /// The device didn't start its player, or the player the stream, in
    /// time; or the player was closed.
    NotStarted(String),
    /// The connection ended.
    Lost,
}

/// The player of a cast, in its protocol.
pub enum Player {
    Cast(crate::cast::player::Player),
    AirPlay(crate::airplay::player::Player),
}

impl Player {
    /// Asks where it is: the answer comes as an [`Event::Status`].
    pub async fn status(&self) -> bool {
        match self {
            Player::Cast(p) => p.status().await,
            Player::AirPlay(p) => p.status().await,
        }
    }

    pub async fn play(&self) -> bool {
        match self {
            Player::Cast(p) => p.play().await,
            Player::AirPlay(p) => p.play().await,
        }
    }

    pub async fn pause(&self) -> bool {
        match self {
            Player::Cast(p) => p.pause().await,
            Player::AirPlay(p) => p.pause().await,
        }
    }

    pub async fn seek(&self, position_ms: i64) -> bool {
        match self {
            Player::Cast(p) => p.seek(position_ms).await,
            Player::AirPlay(p) => p.seek(position_ms).await,
        }
    }

    /// Asks the device its volume: the answer comes as an [`Event::Volume`].
    pub async fn ask_volume(&self) -> bool {
        match self {
            Player::Cast(p) => p.ask_volume().await,
            Player::AirPlay(_) => false,
        }
    }

    /// The device's own volume, 0 to 100.
    pub async fn set_volume(&self, percent: u8) -> bool {
        match self {
            Player::Cast(p) => p.set_volume(percent).await,
            Player::AirPlay(_) => false,
        }
    }

    /// Stops the stream and leaves the device as it was before the cast.
    pub async fn stop(&self) {
        match self {
            Player::Cast(p) => p.stop().await,
            Player::AirPlay(p) => p.stop().await,
        }
    }

    /// Lets the device be, playing or not, to another cast.
    pub fn leave(&self) {
        match self {
            Player::Cast(p) => p.leave(),
            // The session ends as the player is dropped.
            Player::AirPlay(_) => {}
        }
    }
}

/// A connection to a device, its player not started yet.
pub enum Connected {
    Cast(crate::cast::connection::Connection, mpsc::Receiver<crate::cast::connection::Incoming>),
    AirPlay(Box<crate::airplay::player::Session<tokio::net::TcpStream>>),
}

impl Connected {
    /// Starts the device's player and loads `media` into it. Its events
    /// come on the receiver returned, until it ends ([`Event::Ended`], then
    /// nothing).
    pub async fn start(self, media: &Media) -> Result<(Player, mpsc::Receiver<Event>), Failure> {
        match self {
            Connected::Cast(connection, incoming) => {
                let (player, events) = crate::cast::player::Player::start(connection, incoming, media).await?;
                Ok((Player::Cast(player), events))
            }
            Connected::AirPlay(session) => {
                let (player, events) = session.start(media).await?;
                Ok((Player::AirPlay(player), events))
            }
        }
    }
}
