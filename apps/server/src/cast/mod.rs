//! Casting to Google Cast devices (Chromecasts, Google TVs, TVs and
//! speakers with Cast built in): the protocol spoken to them on the local
//! network, and Google's own player started on them to play a session's
//! stream. The casts themselves, whatever the protocol, are
//! [`crate::nearby`]'s.

pub mod connection;
pub mod device;
pub mod discovery;
pub mod frame;
pub mod player;
pub mod profile;
