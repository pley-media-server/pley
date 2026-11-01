//! The server's JSON as the app reads and writes it (docs/API.md): every
//! answer and request body the TV uses, and the events of its stream.
//!
//! Tolerant, as the Android app's model is (`PleyJson`): keys the app
//! doesn't know are skipped, so a newer server can add fields; a field left
//! out, or sent as `null`, reads as its default; and every enum the server
//! may grow reads a name it doesn't know as its fallback (`Unknown`), never
//! as an error. Request bodies leave out what they don't set, since the
//! server reads a missing field as `null` (one exception:
//! [`StreamChoice`]'s audio).
//!
//! Units are the server's: ids are `i64`, times unix seconds, durations and
//! positions milliseconds, bitrates bits per second in answers and kbps in
//! requests.

mod account;
mod caps;
mod error;
mod events;
mod item;
mod library;
mod live;
mod oauth;
mod playback;

pub use account::*;
pub use caps::*;
pub use error::*;
pub use events::*;
pub use item::*;
pub use library::*;
pub use live::*;
pub use oauth::*;
pub use playback::*;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer};

/// Reads a field the server may send as `null` as its type's default, as
/// Android's `coerceInputValues` does. Used with `#[serde(default)]`, so a
/// field left out reads the same way.
pub(crate) fn nd<'de, D, T>(d: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de> + Default,
{
    Ok(Option::<T>::deserialize(d)?.unwrap_or_default())
}

/// Whether a flag is off: request bodies leave such flags out.
pub(crate) fn is_false(b: &bool) -> bool {
    !*b
}

/// One answer read from its JSON bytes.
pub fn decode<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, serde_json::Error> {
    serde_json::from_slice(bytes)
}

/// An enum the server names with strings, read leniently: a name this app
/// doesn't know reads as the fallback variant named after `or`, so a newer
/// server can add a kind, a state or a phase without breaking an answer.
/// It writes its wire name back. One variant carries `#[default]`.
macro_rules! wire_enum {
    (
        $(#[$meta:meta])*
        pub enum $name:ident or $fallback:ident {
            $( $(#[$vmeta:meta])* $variant:ident = $wire:literal ),+ $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum $name {
            $( $(#[$vmeta])* $variant ),+
        }

        impl $name {
            /// Every value, in the order the server documents them.
            pub const ALL: &'static [$name] = &[$($name::$variant),+];

            /// The name on the wire.
            pub fn as_str(self) -> &'static str {
                match self {
                    $($name::$variant => $wire),+
                }
            }

            /// The value named `wire`; a name this app doesn't know gives the fallback.
            pub fn from_wire(wire: &str) -> $name {
                match wire {
                    $($wire => $name::$variant,)+
                    _ => $name::$fallback,
                }
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl serde::Serialize for $name {
            fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                s.serialize_str(self.as_str())
            }
        }

        impl<'de> serde::Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<$name, D::Error> {
                struct Wire;
                impl serde::de::Visitor<'_> for Wire {
                    type Value = $name;

                    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        f.write_str("a string")
                    }

                    fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<$name, E> {
                        Ok($name::from_wire(v))
                    }
                }
                d.deserialize_str(Wire)
            }
        }
    };
}
pub(crate) use wire_enum;

/// Numbers written as JavaScript writes them: a whole number without a
/// decimal point (`60`, not `60.0`). The capability report keeps the bytes
/// the web app's report had, which the LG port's snapshots compare.
pub(crate) mod js_number {
    use serde::Serializer;
    use serde::ser::SerializeSeq;

    fn whole(v: f64) -> Option<i64> {
        (v.fract() == 0.0 && v.abs() < 9_007_199_254_740_992.0).then_some(v as i64)
    }

    pub fn serialize<S: Serializer>(v: &f64, s: S) -> Result<S::Ok, S::Error> {
        match whole(*v) {
            Some(n) => s.serialize_i64(n),
            None => s.serialize_f64(*v),
        }
    }

    pub mod list {
        use super::*;

        pub fn serialize<S: Serializer>(v: &[f64], s: S) -> Result<S::Ok, S::Error> {
            let mut seq = s.serialize_seq(Some(v.len()))?;
            for x in v {
                match whole(*x) {
                    Some(n) => seq.serialize_element(&n)?,
                    None => seq.serialize_element(x)?,
                }
            }
            seq.end()
        }
    }
}
