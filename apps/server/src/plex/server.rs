//! Server-level endpoints: `/`, `/identity`, accounts, preferences and the
//! empty lists Plex clients probe for.

use std::sync::Arc;

use axum::Extension;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode, header};

use super::library::{item_nodes, list_container, visible_libraries};
use super::meta::{Detail, Sections, section_type};
use super::{Node, PLEX_VERSION, Page, Params, PlexError, PlexInfo, PlexResult, PlexUser};
use crate::history;
use crate::state::AppState;

fn platform() -> &'static str {
    match std::env::consts::OS {
        "windows" => "Windows",
        "macos" => "MacOSX",
        "linux" => "Linux",
        "freebsd" => "FreeBSD",
        other => other,
    }
}

/// `/`: server capabilities.
pub async fn root(
    State(state): State<AppState>,
    PlexUser(_user): PlexUser,
    Extension(info): Extension<Arc<PlexInfo>>,
) -> PlexResult<Node> {
    let settings = state.settings.get();
    let transcodes = state.sessions.active_transcodes();
    let directories = [
        "activities",
        "butler",
        "clients",
        "hubs",
        "library",
        "playlists",
        "search",
        "servers",
        "status",
        "system",
        "transcode",
    ];
    let mut mc = Node::container()
        .attr("size", directories.len())
        .attr("allowCameraUpload", false)
        .attr("allowChannelAccess", false)
        .attr("allowMediaDeletion", false)
        .attr("allowSharing", false)
        .attr("allowSync", false)
        .attr("allowTuners", false)
        .attr("backgroundProcessing", false)
        .attr("certificate", false)
        .attr("companionProxy", false)
        .attr("eventStream", false)
        .attr("friendlyName", &settings.server_name)
        .attr("hubSearch", true)
        .attr("itemClusters", false)
        .attr("livetv", 0)
        .attr("machineIdentifier", &info.machine_id)
        .attr("mediaProviders", true)
        .attr("multiuser", true)
        .attr("myPlex", false)
        .attr("myPlexMappingState", "unknown")
        .attr("myPlexSigninState", "none")
        .attr("myPlexSubscription", false)
        .attr("myPlexUsername", "")
        .attr("offlineTranscode", 1)
        .attr("ownerFeatures", "")
        .attr("photoAutoTag", false)
        .attr("platform", platform())
        .attr("platformVersion", "")
        .attr("pluginHost", false)
        .attr("pushNotifications", false)
        .attr("readOnlyLibraries", false)
        .attr("streamingBrainABRVersion", 3)
        .attr("streamingBrainVersion", 2)
        .attr("sync", false)
        .attr("transcoderActiveVideoSessions", transcodes)
        .attr("transcoderAudio", true)
        .attr("transcoderLyrics", false)
        .attr("transcoderPhoto", true)
        .attr("transcoderSubtitles", true)
        .attr("transcoderVideo", true)
        .attr("transcoderVideoBitrates", "64,96,208,320,720,1500,2000,3000,4000,8000,10000,12000,20000")
        .attr("transcoderVideoQualities", "0,1,2,3,4,5,6,7,8,9,10,11,12")
        .attr("transcoderVideoResolutions", "128,128,160,240,320,480,768,720,720,1080,1080,1080,1080")
        .attr("updatedAt", crate::util::now())
        .attr("updater", false)
        .attr("version", PLEX_VERSION)
        .attr("voiceSearch", false);
    for key in directories {
        mc.push(Node::new("Directory").attr("count", 1).attr("key", key).attr("title", key));
    }
    Ok(mc)
}

/// `/identity`: no authentication, like Plex.
pub async fn identity(Extension(info): Extension<Arc<PlexInfo>>) -> Node {
    Node::container()
        .attr("size", 0)
        .attr("claimed", false)
        .attr("machineIdentifier", &info.machine_id)
        .attr("version", PLEX_VERSION)
}

#[derive(sqlx::FromRow)]
struct UserRow {
    id: i64,
    username: String,
}

/// `/accounts`: pley users (an administrator's token with `activity` sees
/// everyone).
pub async fn accounts(State(state): State<AppState>, PlexUser(user): PlexUser) -> PlexResult<Node> {
    let users: Vec<UserRow> = if super::sees_everyone(&user) {
        sqlx::query_as("SELECT id, username FROM users ORDER BY id").fetch_all(&state.db.read).await?
    } else {
        vec![UserRow { id: user.id, username: user.username.clone() }]
    };
    let mut mc = Node::container().attr("size", users.len()).attr("identifier", "com.plexapp.system.accounts");
    for u in users {
        mc.push(
            Node::new("Account")
                .attr("id", u.id)
                .attr("key", format!("/accounts/{}", u.id))
                .attr("name", u.username)
                .attr("defaultAudioLanguage", "en")
                .attr("autoSelectAudio", true)
                .attr("defaultSubtitleLanguage", "")
                .attr("subtitleMode", 0)
                .attr("thumb", ""),
        );
    }
    Ok(mc)
}

/// `/myplex/account`: pley is never linked to plex.tv.
pub async fn myplex_account(PlexUser(user): PlexUser) -> Node {
    Node::new("MyPlex")
        .attr("authToken", "")
        .attr("username", &user.username)
        .attr("mappingState", "unknown")
        .attr("mappingError", "")
        .attr("signInState", "none")
        .attr("publicAddress", "")
        .attr("publicPort", 0)
        .attr("privateAddress", "")
        .attr("privatePort", 0)
        .attr("subscriptionFeatures", "")
        .attr("subscriptionActive", false)
        .attr("subscriptionState", "Inactive")
}

pub async fn system(PlexUser(_user): PlexUser) -> Node {
    Node::container().attr("size", 0).attr("identifier", "com.plexapp.system")
}

/// `/servers`: this server, at the address the client used (with its
/// scheme's port when it named none: through a reverse proxy on pley's
/// port, say).
pub async fn servers(
    State(state): State<AppState>,
    PlexUser(_user): PlexUser,
    Extension(info): Extension<Arc<PlexInfo>>,
    headers: HeaderMap,
) -> Node {
    let host_header = headers.get(header::HOST).and_then(|v| v.to_str().ok()).unwrap_or("127.0.0.1");
    let scheme_port = if crate::auth::forwarded_https(&headers) { 443 } else { 80 };
    let (host, port) = match host_header.rsplit_once(':') {
        Some((h, p)) if !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()) => {
            (h.to_owned(), p.parse().unwrap_or(scheme_port))
        }
        _ => (host_header.to_owned(), scheme_port),
    };
    Node::container().attr("size", 1).child(
        Node::new("Server")
            .attr("name", &state.settings.get().server_name)
            .attr("host", host.clone())
            .attr("address", host)
            .attr("port", port)
            .attr("machineIdentifier", &info.machine_id)
            .attr("version", PLEX_VERSION),
    )
}

/// Lists pley has nothing in (clients, devices, playlists, …).
pub async fn empty(PlexUser(_user): PlexUser) -> Node {
    Node::container().attr("size", 0).attr("totalSize", 0).attr("offset", 0)
}

/// Accepted requests for features pley doesn't have (ratings, trash, …).
pub async fn accepted(PlexUser(_user): PlexUser) -> StatusCode {
    StatusCode::OK
}

/// `PUT /:/prefs`: the server's settings, which no token changes, whatever
/// its scopes: they are an administrator's, in Pley.
pub async fn settings_refused(PlexUser(_user): PlexUser) -> PlexError {
    PlexError::Forbidden
}

/// `/prefs`: the few server preferences that map onto pley settings.
pub async fn prefs(
    State(state): State<AppState>,
    PlexUser(_user): PlexUser,
    Extension(info): Extension<Arc<PlexInfo>>,
) -> Node {
    let settings = state.settings.get();
    let setting = |id: &'static str, label: &'static str, value: String| {
        Node::new("Setting")
            .attr("id", id)
            .attr("label", label)
            .attr("summary", "")
            .attr("type", "text")
            .attr("default", "")
            .attr("value", value)
            .attr("hidden", false)
            .attr("advanced", false)
            .attr("group", "general")
    };
    Node::container()
        .attr("size", 2)
        .child(setting("FriendlyName", "Friendly name", settings.server_name.clone()))
        .child(setting("MachineIdentifier", "Machine identifier", info.machine_id.clone()))
}

/// `/media/providers`: the library provider with per-section totals (the
/// sections this user may open).
pub async fn media_providers(State(state): State<AppState>, PlexUser(user): PlexUser) -> PlexResult<Node> {
    let totals: Vec<(i64, i64, i64)> = sqlx::query_as(
        "SELECT library_id, COALESCE(SUM(duration_ms), 0), COALESCE(SUM(size), 0) FROM media_files GROUP BY library_id",
    )
    .fetch_all(&state.db.read)
    .await?;
    let mut content = Node::new("Feature").attr("key", "/library/sections").attr("type", "content");
    for lib in visible_libraries(&state, &user).await? {
        let (duration, storage) =
            totals.iter().find(|(id, ..)| *id == lib.id).map(|(_, d, s)| (*d, *s)).unwrap_or_default();
        // The pivots matter beyond looks: python-plexapi treats elements
        // without children as missing.
        content.push(
            Node::new("Directory")
                .attr("id", lib.id.to_string())
                .attr("key", format!("/library/sections/{}", lib.id))
                .attr("title", &lib.name)
                .attr("type", section_type(&lib))
                .attr("durationTotal", duration)
                .attr("storageTotal", storage)
                .child(
                    Node::new("Pivot")
                        .attr("id", "recommended")
                        .attr("key", format!("/hubs/sections/{}", lib.id))
                        .attr("type", "hub")
                        .attr("title", "Recommended")
                        .attr("context", "content.discover"),
                )
                .child(
                    Node::new("Pivot")
                        .attr("id", "library")
                        .attr("key", format!("/library/sections/{}/all", lib.id))
                        .attr("type", "list")
                        .attr("title", "Library")
                        .attr("context", "content.library"),
                ),
        );
    }
    let provider = Node::new("MediaProvider")
        .attr("identifier", "com.plexapp.plugins.library")
        .attr("title", "Library")
        .attr("types", "video,audio")
        .attr("protocols", "stream,download")
        .child(content)
        .child(Node::new("Feature").attr("key", "/hubs/search").attr("type", "search"))
        .child(Node::new("Feature").attr("key", "/library/onDeck").attr("type", "onDeck"))
        .child(Node::new("Feature").attr("type", "timeline").attr("key", "/:/timeline"))
        .child(Node::new("Feature").attr("type", "universal").attr("key", "/video/:/transcode/universal"));
    Ok(Node::container().attr("size", 1).child(provider))
}

/// `/status/sessions/history/all`: plays, newest first. An administrator's
/// token with `activity` sees everyone's (or one account's with
/// `accountID`), others their own.
pub async fn history(
    State(state): State<AppState>,
    PlexUser(user): PlexUser,
    Extension(info): Extension<Arc<PlexInfo>>,
    params: Params,
    page: Page,
) -> PlexResult<Node> {
    let account = params.parse::<i64>("accountID");
    let filter = history::Filter {
        user_id: if super::sees_everyone(&user) { account } else { Some(user.id) },
        item_id: params.parse::<i64>("metadataItemID"),
        library_id: params.parse::<i64>("librarySectionID"),
        // `viewedAt>=…` arrives as the key `viewedAt>`.
        since: params.parse::<i64>("viewedAt>").or_else(|| params.parse::<i64>("viewedAt")),
        offset: page.start as i64,
        limit: page.size.map(|s| s as i64).unwrap_or(1_000).clamp(0, 10_000),
    };
    let (records, total) = history::list(&state.db, user.id, &filter).await?;
    let libraries = visible_libraries(&state, &user).await?;
    let rows: Vec<_> = records.iter().map(|r| r.item.clone()).collect();
    let detail = Detail { guids: false, full: false, media: false, markers: false };
    let nodes = item_nodes(&state, &rows, &Sections::new(&libraries), detail).await?;
    let nodes = nodes
        .into_iter()
        .zip(&records)
        .map(|(node, record)| {
            node.attr("historyKey", format!("/status/sessions/history/{}", record.id))
                .attr("viewedAt", record.viewed_at)
                .attr("accountID", record.user.id)
                .attr("deviceID", 0)
        })
        .collect();
    Ok(list_container(None, &info, nodes, total as usize, &page))
}

/// `DELETE /status/sessions/history/{id}`: one's own plays, and with
/// `manage` anyone's (deleting changes their history, which seeing it
/// with `activity` doesn't).
pub async fn delete_history(
    State(state): State<AppState>,
    PlexUser(user): PlexUser,
    Path(id): Path<i64>,
) -> PlexResult<StatusCode> {
    let anyones = user.compatibility_allows(crate::auth::compatibility::Scope::Manage);
    if history::delete(&state.db, user.id, anyones, id).await? { Ok(StatusCode::OK) } else { Err(PlexError::NotFound) }
}
