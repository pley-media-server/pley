//! Everything of Pley for LG TV that is not drawn: the server's API and event
//! stream, sign-in, finding servers, what the TV keeps, background tasks,
//! the words for media facts, and the traits the TV implements (the device
//! and the player engine).
//!
//! Nothing here blocks the main thread: every network call is a blocking
//! function meant to run on a [`task::Tasks`] worker (or the event stream's
//! own thread), and answers come back through a [`task::Promise`] and the
//! main loop's [`pley_base::Waker`].

pub mod addresses;
pub mod api;
pub mod auth;
pub mod caps;
pub mod device;
pub mod discovery;
pub mod events;
pub mod format;
pub mod http;
pub mod media;
pub mod model;
pub mod requests;
pub mod store;
pub mod task;

pub use api::{Api, image_bucket};
pub use auth::{AuthError, DeviceSignIn, Tokens};
pub use device::{CapsOverrides, CapsReport, Device, DeviceEvent, DeviceInfo, Drive, DriveSpace, FakeDevice, Tier};
pub use events::{CastReceiver, EventStream, StreamNews};
pub use http::{ApiError, Http, NetError, TokenSource};
pub use media::{AudioTrack, EngineEvent, EngineStats, LoadRequest, MediaKind, PlayerEngine};
pub use store::{Settings, Snapshots, Store};
pub use task::{Cancel, Promise, Tasks};
