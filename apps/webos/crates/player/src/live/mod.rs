//! Live TV: watching a channel ([`session::LiveSession`]), and what the Live
//! TV screens need beside it: what's on and the categories ([`data`]), a
//! category's channels by pages ([`pages::ChannelPages`]) and the guide's
//! programmes by chunks ([`guide::GuideStore`]).

pub mod data;
pub mod guide;
pub mod pages;
pub mod session;

pub use data::Category;
pub use guide::GuideStore;
pub use pages::ChannelPages;
pub use session::{LivePhase, LiveSession, LiveState, Trouble, TroubleKind, holders, trouble_of};
