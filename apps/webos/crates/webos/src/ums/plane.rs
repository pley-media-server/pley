//! How the picture of a uMediaServer playback reaches the screen, per webOS
//! version (RES/plx-media.md §18.2).
//!
//! On webOS 5 and later the exported window's id in the load payload is the
//! whole binding. On 4.x nothing is established: the candidates are tried in
//! order, judged at run time, and the one that worked is remembered per
//! firmware build in the data folder (an update can change the pipeline):
//!
//! 1. the load's `option.appId` alone, with the transparent interface;
//! 2. LG's ACB (`libAcbAPI.so.1`) given the pipeline's `mediaId`: sink, media
//!    id, `LOADED`, display window, `PLAYING`, without `setMediaVideoData`
//!    (which failed with `ERROR_06` for an address playback);
//! 3. uMediaServer's own `setDisplayWindow`.
//!
//! A candidate is judged once the pipeline plays: uMediaServer's
//! `getForegroundAppInfo` lists the foreground app's pipelines with the
//! rectangles the TV gives them; a pipeline with a rectangle is on screen.

use serde_json::{Value, json};

/// How the picture is bound.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Plane {
    /// webOS 5 and later: an exported window.
    Exported,
    /// webOS 4.x: one of the candidates.
    Legacy(Candidate),
}

/// webOS 4.x's ways to bind the picture, in the order they are tried.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Candidate {
    AppIdOnly,
    Acb,
    DisplayWindow,
}

impl Candidate {
    /// Every candidate, in order.
    pub const ALL: [Candidate; 3] = [Candidate::AppIdOnly, Candidate::Acb, Candidate::DisplayWindow];

    /// Its name in the data folder.
    pub fn name(self) -> &'static str {
        match self {
            Candidate::AppIdOnly => "app_id",
            Candidate::Acb => "acb",
            Candidate::DisplayWindow => "display_window",
        }
    }

    fn from_name(name: &str) -> Option<Candidate> {
        Candidate::ALL.into_iter().find(|c| c.name() == name)
    }

    /// Whether it can bind a pipeline already loaded (all but the first,
    /// which is decided by the load itself).
    pub fn binds_after_load(self) -> bool {
        self != Candidate::AppIdOnly
    }
}

/// What was learned about the candidates on one firmware build.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PlaneMemory {
    /// The build it was learned on.
    pub firmware: String,
    pub winner: Option<Candidate>,
    pub failed: Vec<Candidate>,
}

/// The file the memory is kept in, in the data folder.
pub const FILE: &str = "video-plane.json";

impl PlaneMemory {
    /// The memory read from its file, when it is for `firmware`; a fresh one
    /// otherwise.
    pub fn for_firmware(text: Option<&str>, firmware: &str) -> PlaneMemory {
        let parsed = text.and_then(|t| serde_json::from_str::<Value>(t).ok());
        let names = |v: Option<&Value>| -> Vec<Candidate> {
            v.and_then(Value::as_array)
                .map(|a| a.iter().filter_map(|n| n.as_str().and_then(Candidate::from_name)).collect())
                .unwrap_or_default()
        };
        match parsed {
            Some(v) if v.get("firmware").and_then(Value::as_str) == Some(firmware) => PlaneMemory {
                firmware: firmware.into(),
                winner: v.get("winner").and_then(Value::as_str).and_then(Candidate::from_name),
                failed: names(v.get("failed")),
            },
            _ => PlaneMemory { firmware: firmware.into(), ..PlaneMemory::default() },
        }
    }

    /// The memory as its file holds it.
    pub fn to_json(&self) -> String {
        json!({
            "firmware": self.firmware,
            "winner": self.winner.map(Candidate::name),
            "failed": self.failed.iter().map(|c| c.name()).collect::<Vec<_>>(),
        })
        .to_string()
    }

    /// The candidate to play with: the one that worked, else the first not yet
    /// failed (ACB only where its library is), else the first.
    pub fn choose(&self, acb_available: bool) -> Candidate {
        self.winner.unwrap_or_else(|| {
            Candidate::ALL
                .into_iter()
                .find(|c| !self.failed.contains(c) && (acb_available || *c != Candidate::Acb))
                .unwrap_or(Candidate::AppIdOnly)
        })
    }

    /// Whether a verdict is still wanted.
    pub fn undecided(&self, acb_available: bool) -> bool {
        self.winner.is_none() && self.next_after(None, acb_available).is_some()
    }

    /// The next candidate to try after `current` failed, if any.
    pub fn next_after(&self, current: Option<Candidate>, acb_available: bool) -> Option<Candidate> {
        Candidate::ALL
            .into_iter()
            .find(|c| Some(*c) != current && !self.failed.contains(c) && (acb_available || *c != Candidate::Acb))
    }
}

/// Whether `getForegroundAppInfo`'s answer shows `media_id` on screen: one of
/// its `acbs` names the pipeline (or none names any) and has a rectangle or is
/// full screen. `None` when the answer says nothing either way.
pub fn shown_in(answer: &Value, media_id: &str) -> Option<bool> {
    let acbs = answer.get("acbs")?.as_array()?;
    let named = acbs.iter().any(|a| a.get("pipelineId").is_some());
    let on_screen = |a: &Value| {
        let size = |k: &str| a.get(k).and_then(Value::as_i64).unwrap_or(0);
        a.get("isFullScreen") == Some(&Value::Bool(true)) || (size("width") > 0 && size("height") > 0)
    };
    Some(acbs.iter().any(|a| (!named || a.get("pipelineId").and_then(Value::as_str) == Some(media_id)) && on_screen(a)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidates_go_in_order_and_the_winner_stays() {
        let mut m = PlaneMemory::for_firmware(None, "4.10.2-31");
        assert_eq!(m.choose(true), Candidate::AppIdOnly);
        m.failed.push(Candidate::AppIdOnly);
        assert_eq!(m.choose(true), Candidate::Acb);
        assert_eq!(m.choose(false), Candidate::DisplayWindow, "no ACB library: skip it");
        assert_eq!(m.next_after(Some(Candidate::Acb), true), Some(Candidate::DisplayWindow));
        m.winner = Some(Candidate::Acb);
        assert_eq!(m.choose(true), Candidate::Acb);
        assert!(!m.undecided(true));
        m.winner = None;
        m.failed = Candidate::ALL.to_vec();
        assert_eq!(m.choose(true), Candidate::AppIdOnly);
        assert!(!m.undecided(true));
    }

    #[test]
    fn the_memory_is_kept_per_firmware_build() {
        let m = PlaneMemory {
            firmware: "4.10.2-31".into(),
            winner: Some(Candidate::DisplayWindow),
            failed: vec![Candidate::AppIdOnly, Candidate::Acb],
        };
        let text = m.to_json();
        assert_eq!(PlaneMemory::for_firmware(Some(&text), "4.10.2-31"), m);
        let updated = PlaneMemory::for_firmware(Some(&text), "4.10.3-12");
        assert_eq!(updated, PlaneMemory { firmware: "4.10.3-12".into(), ..PlaneMemory::default() });
        assert_eq!(PlaneMemory::for_firmware(Some("garbage"), "x").winner, None);
    }

    #[test]
    fn the_foreground_info_says_whether_the_picture_is_bound() {
        let info = json!({ "appId": "app.pley.tv", "acbs": [
            { "pipelineId": "_other", "width": 1920, "height": 1080 },
            { "pipelineId": "_m1", "isFullScreen": false, "width": 0, "height": 0 }
        ] });
        assert_eq!(shown_in(&info, "_m1"), Some(false));
        assert_eq!(shown_in(&info, "_other"), Some(true));
        let unnamed = json!({ "acbs": [{ "isFullScreen": true }] });
        assert_eq!(shown_in(&unnamed, "_m1"), Some(true));
        assert_eq!(shown_in(&json!({ "acbs": [] }), "_m1"), Some(false));
        assert_eq!(shown_in(&json!({ "returnValue": true }), "_m1"), None);
    }
}
