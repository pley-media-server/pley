//! Casting to Google Cast devices (Chromecasts, Google TVs, TVs and
//! speakers with Cast built in): the protocol spoken to them on the local
//! network, and Google's own player started on them to play a session's
//! stream.

pub mod connection;
pub mod devices;
pub mod discovery;
pub mod frame;
pub mod player;
pub mod profile;
