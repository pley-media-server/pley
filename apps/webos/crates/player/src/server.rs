//! What playback asks of the server, as one trait: [`pley_core::Api`]
//! answers it on the TV and in the simulator; tests answer it with a
//! pretend server. Every call blocks, so it runs on a task thread (or the
//! reporter's), never on the main thread.

use std::time::Duration;

use pley_core::api::LiveBrowse;
use pley_core::http::{Method, Request};
use pley_core::model::{
    AddSubtitle, BitrateAnswer, CollectionDetail, ItemDetail, ItemSummary, LiveChannel, LiveChannelPage, LiveGroups,
    LiveGuide, LivePlan, LiveWatching, NetworkAnswer, NetworkReport, PlaybackInfo, PlaybackRequest, PlaylistDetail,
    ProgressReport, ServerInfo, SessionStatus, SpeedSample, StreamChoice, SubtitleResult, SubtitleTrack, TrickplayInfo,
    WatchRequest,
};
use pley_core::{Api, ApiError};
use pley_subs::PictureTrack;

/// A subtitle file's answer: its bytes, the status (206 for a range), and whether it is whole (`X-Pley-Complete`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SubtitleAnswer {
    pub status: u16,
    pub body: Vec<u8>,
    /// Where a 206's bytes start (its `Content-Range`).
    pub range_start: Option<u64>,
    pub complete: bool,
}

/// The server calls the player, music and Live TV make.
pub trait PlayerServer: Send + Sync {
    /// The server's address (`http://nas:7539`): keys saved offsets and memories.
    fn server(&self) -> &str;
    /// The absolute address of a path the server gave.
    fn url(&self, path: &str) -> String;

    fn item(&self, id: i64) -> Result<ItemDetail, ApiError>;
    fn next(&self, id: i64) -> Result<Option<ItemSummary>, ApiError>;
    fn playlist(&self, id: i64) -> Result<PlaylistDetail, ApiError>;
    fn collection(&self, id: i64) -> Result<CollectionDetail, ApiError>;

    fn play(&self, req: &PlaybackRequest) -> Result<PlaybackInfo, ApiError>;
    fn progress(&self, item_id: i64, report: &ProgressReport) -> Result<(), ApiError>;
    fn stop_session(&self, session: &str) -> Result<(), ApiError>;
    fn session_status(&self, session: &str, timeout: Duration) -> Result<SessionStatus, ApiError>;
    fn set_bitrate(&self, session: &str, kbps: u32) -> Result<BitrateAnswer, ApiError>;
    fn set_streams(&self, item_id: i64, choice: &StreamChoice) -> Result<(), ApiError>;
    /// `GET /api/server`: whether it answers at all.
    fn reachable(&self, timeout: Duration) -> Result<ServerInfo, ApiError>;
    /// A session's playlist (`GET`, as text) or segment (`HEAD`), as the TV's player asks for it.
    fn media(&self, method: Method, url: &str, timeout: Duration) -> Result<String, ApiError>;

    fn network(&self, client_id: &str, timeout: Duration) -> Result<NetworkAnswer, ApiError>;
    fn tell_network(
        &self,
        client_id: &str,
        report: &NetworkReport,
        timeout: Duration,
    ) -> Result<NetworkAnswer, ApiError>;
    fn speed_test(&self, bytes: u64) -> Result<SpeedSample, ApiError>;

    fn trickplay(&self, file_id: i64) -> Result<TrickplayInfo, ApiError>;
    /// A subtitle file (WebVTT or `.mks`) with `accept`, from byte `from` when given.
    fn subtitles(
        &self,
        url: &str,
        accept: &str,
        from: Option<u64>,
        timeout: Duration,
    ) -> Result<SubtitleAnswer, ApiError>;
    fn search_subtitles(
        &self,
        item_id: i64,
        file_id: Option<i64>,
        languages: &[String],
    ) -> Result<Vec<SubtitleResult>, ApiError>;
    fn add_subtitle(&self, item_id: i64, add: &AddSubtitle) -> Result<SubtitleTrack, ApiError>;

    fn watch_channel(&self, channel: i64, req: &WatchRequest) -> Result<LiveWatching, ApiError>;
    fn live_plan(&self, session: &str) -> Result<Option<LivePlan>, ApiError>;
    fn leave_channel(&self, session: &str) -> Result<(), ApiError>;
    fn live_channel(&self, id: i64) -> Result<LiveChannel, ApiError>;
    fn live_groups(&self) -> Result<LiveGroups, ApiError>;
    fn live_browse(&self, b: &LiveBrowse) -> Result<LiveChannelPage, ApiError>;
    fn live_guide(&self, from: i64, to: i64, ids: &[i64]) -> Result<LiveGuide, ApiError>;
    fn set_live_favorite(&self, channel: i64, favorite: bool) -> Result<(), ApiError>;

    /// A video on a drive plugged into the TV, with no server behind it:
    /// it plays as it is, and nothing converts it (the player's local mode).
    fn local(&self) -> bool {
        false
    }

    /// A picture subtitle track at `url` (a `.mks` address) handed over as
    /// it is, with whether it is whole; none: `subtitles` fetches its `.mks`.
    fn picture_track(&self, url: &str, timeout: Duration) -> Option<Result<(PictureTrack, bool), ApiError>> {
        let _ = (url, timeout);
        None
    }
}

/// The bytes a `Content-Range: bytes 100-199/200` starts at.
fn range_start(header: Option<&str>) -> Option<u64> {
    let rest = header?.trim().strip_prefix("bytes")?.trim_start();
    rest.split('-').next()?.trim().parse().ok()
}

impl PlayerServer for Api {
    fn server(&self) -> &str {
        Api::server(self)
    }

    fn url(&self, path: &str) -> String {
        Api::url(self, path)
    }

    fn item(&self, id: i64) -> Result<ItemDetail, ApiError> {
        Api::item(self, id)
    }

    fn next(&self, id: i64) -> Result<Option<ItemSummary>, ApiError> {
        Api::next(self, id)
    }

    fn playlist(&self, id: i64) -> Result<PlaylistDetail, ApiError> {
        Api::playlist(self, id)
    }

    fn collection(&self, id: i64) -> Result<CollectionDetail, ApiError> {
        Api::collection(self, id)
    }

    fn play(&self, req: &PlaybackRequest) -> Result<PlaybackInfo, ApiError> {
        Api::play(self, req)
    }

    fn progress(&self, item_id: i64, report: &ProgressReport) -> Result<(), ApiError> {
        Api::progress(self, item_id, report)
    }

    fn stop_session(&self, session: &str) -> Result<(), ApiError> {
        Api::stop_session(self, session)
    }

    fn session_status(&self, session: &str, timeout: Duration) -> Result<SessionStatus, ApiError> {
        Api::session_status(self, session, timeout)
    }

    fn set_bitrate(&self, session: &str, kbps: u32) -> Result<BitrateAnswer, ApiError> {
        Api::set_bitrate(self, session, kbps)
    }

    fn set_streams(&self, item_id: i64, choice: &StreamChoice) -> Result<(), ApiError> {
        Api::set_streams(self, item_id, choice)
    }

    fn reachable(&self, timeout: Duration) -> Result<ServerInfo, ApiError> {
        Api::reachable(self, timeout)
    }

    fn media(&self, method: Method, url: &str, timeout: Duration) -> Result<String, ApiError> {
        Api::media(self, method, url, timeout)
    }

    fn network(&self, client_id: &str, timeout: Duration) -> Result<NetworkAnswer, ApiError> {
        Api::network(self, client_id, timeout)
    }

    fn tell_network(
        &self,
        client_id: &str,
        report: &NetworkReport,
        timeout: Duration,
    ) -> Result<NetworkAnswer, ApiError> {
        Api::tell_network(self, client_id, report, timeout)
    }

    fn speed_test(&self, bytes: u64) -> Result<SpeedSample, ApiError> {
        Api::speed_test(self, bytes)
    }

    fn trickplay(&self, file_id: i64) -> Result<TrickplayInfo, ApiError> {
        Api::trickplay(self, file_id)
    }

    fn subtitles(
        &self,
        url: &str,
        accept: &str,
        from: Option<u64>,
        timeout: Duration,
    ) -> Result<SubtitleAnswer, ApiError> {
        let mut req = Request::new(Method::Get, Api::url(self, url)).header("Accept", accept).timeout(timeout);
        if let Some(n) = from {
            req = req.header("Range", format!("bytes={n}-"));
        }
        let r = self.send(req)?;
        let range_start = if r.status == 206 { range_start(r.header("Content-Range")) } else { None };
        Ok(SubtitleAnswer { status: r.status, complete: r.complete(), range_start, body: r.body })
    }

    fn search_subtitles(
        &self,
        item_id: i64,
        file_id: Option<i64>,
        languages: &[String],
    ) -> Result<Vec<SubtitleResult>, ApiError> {
        Api::search_subtitles(self, item_id, file_id, languages)
    }

    fn add_subtitle(&self, item_id: i64, add: &AddSubtitle) -> Result<SubtitleTrack, ApiError> {
        Api::add_subtitle(self, item_id, add)
    }

    fn watch_channel(&self, channel: i64, req: &WatchRequest) -> Result<LiveWatching, ApiError> {
        Api::watch_channel(self, channel, req)
    }

    fn live_plan(&self, session: &str) -> Result<Option<LivePlan>, ApiError> {
        Api::live_plan(self, session)
    }

    fn leave_channel(&self, session: &str) -> Result<(), ApiError> {
        Api::leave_channel(self, session)
    }

    fn live_channel(&self, id: i64) -> Result<LiveChannel, ApiError> {
        Api::live_channel(self, id)
    }

    fn live_groups(&self) -> Result<LiveGroups, ApiError> {
        Api::live_groups(self)
    }

    fn live_browse(&self, b: &LiveBrowse) -> Result<LiveChannelPage, ApiError> {
        Api::live_browse(self, b)
    }

    fn live_guide(&self, from: i64, to: i64, ids: &[i64]) -> Result<LiveGuide, ApiError> {
        Api::live_guide(self, from, to, ids)
    }

    fn set_live_favorite(&self, channel: i64, favorite: bool) -> Result<(), ApiError> {
        Api::set_live_favorite(self, channel, favorite)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_ranges() {
        assert_eq!(range_start(Some("bytes 100-199/200")), Some(100));
        assert_eq!(range_start(Some("bytes 0-0/*")), Some(0));
        assert_eq!(range_start(Some("items 1-2")), None);
        assert_eq!(range_start(None), None);
    }
}
