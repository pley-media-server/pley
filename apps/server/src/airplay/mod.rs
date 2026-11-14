//! AirPlay: pairing with an AirPlay device (Apple TV, and TVs that speak
//! AirPlay 2), the encrypted connection pairing gives, and playing a cast
//! through the device's play queue.

pub mod connection;
pub mod crypto;
pub mod device;
pub mod discovery;
pub mod pairing;
pub mod pairings;
pub mod player;
pub mod tlv;
