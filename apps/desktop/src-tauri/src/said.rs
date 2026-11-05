//! The words the app writes for people (why a server didn't answer, why a
//! download failed), which the picker says in the computer's language.
//! Each goes to it as a key of `said` in `src/locales/<language>.json`, with
//! what goes in its `{{values}}`: `{"key": "answeredHttp", "status": 502}`.
//! As text (`to_string`) it is the English, which the server's pages are
//! told of downloads in. A test checks the English here against the
//! picker's.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "key", rename_all = "camelCase")]
pub enum Said {
    // Why nothing answered at an address as a Pley server does, or the
    // window didn't open it: after "Couldn't reach nas:".
    NoAnswerWithin {
        seconds: u64,
    },
    NothingAnswers,
    AnsweredHttp {
        status: u16,
    },
    NotPleyHttp {
        status: u16,
    },
    NotPley,
    /// Something answered without the proof (`proofs`).
    NotYours,
    /// The server proved itself for another address (`proofs`).
    ProvesItselfAs {
        address: String,
    },
    NetworkChanged,
    NotAnAddress {
        address: String,
    },
    NotAWebAddress {
        address: String,
    },

    // Why a download failed, or waits for its server.
    NotMedia,
    NotSaved,
    InvalidAddress,
    NoAnswer,
    NamedOtherwise,
    AnotherDevice,
    NotOnServer,
    SignedOut,
    DiskFull,
    NotWritten {
        detail: String,
    },
    CantMake,
    NotMade,
    /// The server failed to make the file, for the reason it gives (in English).
    NotMadeBecause {
        detail: String,
    },
    Unreadable,
    Busy,
    NoReply,
    Unreachable,
    Dropped,
    NotSent,
    StoppedSending,
    DownloadGone,
    NotWhole,

    /// The system's words, or a library's, as they are.
    Other {
        detail: String,
    },
}

impl From<String> for Said {
    fn from(detail: String) -> Self {
        Self::Other { detail }
    }
}

impl std::fmt::Display for Said {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoAnswerWithin { seconds } => write!(f, "no answer within {seconds} seconds"),
            Self::NothingAnswers => f.write_str("nothing answers there"),
            Self::AnsweredHttp { status } => write!(f, "it answered HTTP {status}"),
            Self::NotPleyHttp { status } => write!(f, "it answered HTTP {status}, so it isn't a Pley server"),
            Self::NotPley => f.write_str("that isn't a Pley server"),
            Self::NotYours => f.write_str(
                "another device answers there (if your server was set up anew, forget it and connect again)",
            ),
            Self::ProvesItselfAs { address } => write!(
                f,
                "it proves itself as {address} here: the proxy in front of it must pass on the Host and X-Forwarded-Proto it was asked with"
            ),
            Self::NetworkChanged => f.write_str("the network changed: open it again"),
            Self::NotAnAddress { address } => write!(f, "{address} isn't an address"),
            Self::NotAWebAddress { address } => write!(f, "{address} isn't a web address"),
            Self::NotMedia => f.write_str("Pley keeps audio and video files only."),
            Self::NotSaved => f.write_str("The file couldn't be saved."),
            Self::InvalidAddress => f.write_str("This server's address isn't valid."),
            Self::NoAnswer => f.write_str("The server didn't answer."),
            Self::NamedOtherwise => {
                f.write_str("The server names itself otherwise here: check the proxy in front of it.")
            }
            Self::AnotherDevice => f.write_str("Another device answers at this server's address."),
            Self::NotOnServer => f.write_str("This file is no longer on the server."),
            Self::SignedOut => f.write_str("Sign in to the server again, then download it again."),
            Self::DiskFull => f.write_str("The disk is full."),
            Self::NotWritten { detail } => write!(f, "The file couldn't be written: {detail}"),
            Self::CantMake => f.write_str("The server can't make files for this computer's player: update it."),
            Self::NotMade => f.write_str("The server couldn't make it for this computer."),
            Self::NotMadeBecause { detail } => write!(f, "The server couldn't make it for this computer: {detail}"),
            Self::Unreadable => f.write_str("The server's answer can't be read."),
            Self::Busy => f.write_str("The server can't make it right now."),
            Self::NoReply => f.write_str("The server couldn't answer."),
            Self::Unreachable => f.write_str("The server can't be reached."),
            Self::Dropped => f.write_str("The connection to the server dropped."),
            Self::NotSent => f.write_str("The server couldn't send the file."),
            Self::StoppedSending => f.write_str("The server stopped sending."),
            Self::DownloadGone => f.write_str("This download is gone."),
            Self::NotWhole => f.write_str("This download isn't whole."),
            Self::Other { detail } => f.write_str(detail),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One of each, with values.
    fn every() -> Vec<Said> {
        let detail = || "Access is denied. (os error 5)".to_owned();
        let address = || "https://pley.example.com".to_owned();
        vec![
            Said::NoAnswerWithin { seconds: 5 },
            Said::NothingAnswers,
            Said::AnsweredHttp { status: 502 },
            Said::NotPleyHttp { status: 404 },
            Said::NotPley,
            Said::NotYours,
            Said::ProvesItselfAs { address: address() },
            Said::NetworkChanged,
            Said::NotAnAddress { address: address() },
            Said::NotAWebAddress { address: address() },
            Said::NotMedia,
            Said::NotSaved,
            Said::InvalidAddress,
            Said::NoAnswer,
            Said::NamedOtherwise,
            Said::AnotherDevice,
            Said::NotOnServer,
            Said::SignedOut,
            Said::DiskFull,
            Said::NotWritten { detail: detail() },
            Said::CantMake,
            Said::NotMade,
            Said::NotMadeBecause { detail: "ffmpeg exited with 1".to_owned() },
            Said::Unreadable,
            Said::Busy,
            Said::NoReply,
            Said::Unreachable,
            Said::Dropped,
            Said::NotSent,
            Said::StoppedSending,
            Said::DownloadGone,
            Said::NotWhole,
            Said::Other { detail: detail() },
        ]
    }

    #[test]
    fn the_picker_says_in_english_what_the_app_says() {
        let picker: serde_json::Value = serde_json::from_str(include_str!("../../src/locales/en.json")).unwrap();
        let words = picker["said"].as_object().unwrap();
        let every = every();
        for said in &every {
            let serde_json::Value::Object(mut values) = serde_json::to_value(said).unwrap() else { panic!("{said:?}") };
            let key = values.remove("key").unwrap();
            let key = key.as_str().unwrap();
            let mut english =
                words.get(key).and_then(|w| w.as_str()).unwrap_or_else(|| panic!("no said.{key}")).to_owned();
            for (name, value) in values {
                let value = value.as_str().map_or_else(|| value.to_string(), str::to_owned);
                english = english.replace(&format!("{{{{{name}}}}}"), &value);
            }
            assert_eq!(english, said.to_string(), "said.{key}");
        }
        assert_eq!(words.len(), every.len(), "every key of said is something the app says");
    }

    #[test]
    fn what_the_list_kept_comes_back() {
        for said in every() {
            let json = serde_json::to_string(&said).unwrap();
            assert_eq!(serde_json::from_str::<Said>(&json).unwrap(), said, "{json}");
        }
        assert_eq!(serde_json::to_string(&Said::DiskFull).unwrap(), r#"{"key":"diskFull"}"#);
    }
}
