//! HTTP API (`/api/*`, see docs/API.md) plus the web UI fallback.

mod accounts;
mod activity;
mod admin;
pub(crate) mod auth;
mod browse;
pub(crate) mod collections;
mod compatibility_tokens;
pub(crate) mod editing;
mod history;
mod import;
pub(crate) mod items;
mod libraries;
mod livetv;
mod oauth;
pub(crate) mod people;
pub(crate) mod playback;
mod playlists;
pub(crate) mod remote;
mod requests;
mod subtitles;
mod two_step;
mod watchlist;

use std::collections::HashMap;
use std::time::Duration;

use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::http::{HeaderName, HeaderValue, header};
use axum::routing::{any, delete, get, patch, post, put};
use tower_http::compression::predicate::{NotForContentType, Predicate, SizeAbove};
use tower_http::compression::{CompressionLayer, CompressionLevel};
use tower_http::set_header::SetResponseHeaderLayer;
use tower_http::trace::TraceLayer;

use crate::error::AppError;
use crate::state::AppState;

pub(crate) use libraries::{add as add_library, validate_paths as validate_library_paths};

pub fn router(state: AppState) -> Router {
    let api = Router::new()
        .route("/health", get(|| async { axum::Json(serde_json::json!({ "status": "ok" })) }))
        .route("/server", get(auth::server_status))
        .route("/server/proof-key", get(auth::server_proof_key))
        .route("/setup", post(auth::setup))
        .route("/auth/login", post(auth::login))
        .route("/auth/logout", post(auth::logout))
        .route("/auth/me", get(auth::me))
        .route("/auth/password", put(auth::change_password))
        .route("/auth/profiles", get(auth::profiles))
        .route("/auth/switch", post(auth::switch))
        .route("/me/two-step", get(two_step::status).post(two_step::enable).delete(two_step::disable))
        .route("/me/two-step/setup", post(two_step::setup))
        .route("/me/two-step/recovery-codes", post(two_step::recovery_codes))
        .route("/me", patch(accounts::update_me))
        .route("/me/pin", put(accounts::set_my_pin))
        .route("/me/track-defaults", put(accounts::set_track_defaults))
        .route("/me/language", put(accounts::set_language))
        .route("/me/libraries/order", put(libraries::set_my_order).delete(libraries::forget_my_order))
        .route(
            "/me/avatar",
            put(accounts::upload_avatar).delete(accounts::delete_avatar).layer(DefaultBodyLimit::max(10 * 1024 * 1024)),
        )
        .route("/me/devices", get(accounts::my_devices))
        .route("/me/devices/{id}", delete(accounts::sign_out_device))
        .route("/me/apps", get(oauth::my_apps))
        .route("/me/apps/{id}", delete(oauth::sign_out_my_app))
        .route("/me/compatibility-tokens", get(compatibility_tokens::list).post(compatibility_tokens::create))
        .route("/me/compatibility-tokens/{id}", delete(compatibility_tokens::revoke))
        .route("/oauth/requests", post(oauth::create_request).layer(DefaultBodyLimit::max(FORM_MAX)))
        .route("/oauth/requests/{id}", post(oauth::decide))
        .route("/oauth/requests/{id}/continue", get(oauth::continue_to_app))
        .route("/oauth/token", post(oauth::token).layer(DefaultBodyLimit::max(FORM_MAX)))
        .route("/oauth/revoke", post(oauth::revoke).layer(DefaultBodyLimit::max(FORM_MAX)))
        .route("/oauth/device", post(oauth::device_authorization).layer(DefaultBodyLimit::max(FORM_MAX)))
        .route("/oauth/device/verify", post(oauth::verify_user_code).layer(DefaultBodyLimit::max(FORM_MAX)))
        .route("/oauth/device/qr", get(oauth::device_qr))
        .route("/oauth/profiles", get(oauth::profiles))
        .route("/import/plex", get(import::progress).delete(import::forget))
        .route("/import/plex/pin", get(import::sign_in_status).post(import::start_sign_in))
        .route("/import/plex/token", post(import::use_token))
        .route("/import/plex/servers", get(import::servers))
        .route("/import/plex/server", post(import::choose_server))
        .route("/import/plex/plan", get(import::plan))
        .route("/import/plex/start", post(import::start))
        .route("/libraries", get(libraries::list).post(libraries::create))
        .route("/libraries/scan", post(libraries::scan_all))
        .route("/libraries/order", put(libraries::reorder))
        .route("/libraries/{id}", get(libraries::get).patch(libraries::update).delete(libraries::remove))
        .route("/libraries/{id}/scan", post(libraries::scan))
        .route("/libraries/{id}/items", get(libraries::items))
        .route("/libraries/{id}/filters", get(libraries::filters))
        .route("/libraries/{id}/hubs", get(libraries::hubs))
        .route("/libraries/{id}/collections", get(collections::list).post(collections::create))
        .route("/collections/{id}", get(collections::get).patch(collections::update).delete(collections::delete))
        .route("/collections/{id}/items", post(collections::add_items))
        .route("/collections/{id}/items/{item_id}", delete(collections::remove_item))
        .route("/collections/{id}/order", put(collections::reorder))
        .route("/collections/{id}/images/{kind}", get(collections::image))
        .route("/playlists", get(playlists::list).post(playlists::create))
        .route("/playlists/{id}", get(playlists::get).patch(playlists::rename).delete(playlists::delete))
        .route("/playlists/{id}/items", post(playlists::add))
        .route("/playlists/{id}/items/{entry_id}", delete(playlists::remove))
        .route("/playlists/{id}/order", put(playlists::reorder))
        .route("/items/{id}", get(items::get).patch(editing::edit))
        .route("/items/{id}/matches", get(editing::matches))
        .route("/items/{id}/match", post(editing::rematch))
        .route(
            "/items/{id}/artwork/{kind}",
            get(editing::artwork_options).put(editing::choose_artwork).delete(editing::reset_artwork),
        )
        .route(
            "/items/{id}/artwork/{kind}/upload",
            post(editing::upload_artwork).layer(DefaultBodyLimit::max(25 * 1024 * 1024)),
        )
        .route("/items/{id}/children", get(items::children))
        .route("/items/{id}/on-deck", get(items::on_deck))
        .route("/items/{id}/next", get(items::next))
        .route("/items/{id}/related", get(items::related))
        .route("/items/{id}/prefetch", post(items::prefetch))
        .route("/items/{id}/progress", post(items::progress))
        .route("/items/{id}/watched", post(items::watched))
        .route("/items/{id}/streams", put(items::set_streams))
        .route("/items/{id}/rating", put(items::set_rating))
        .route("/items/{id}/unwatched", post(items::unwatched))
        .route("/items/{id}/refresh", post(items::refresh))
        .route("/items/{id}/analyze", post(items::analyze))
        .route("/items/{id}/images/{kind}", get(items::image))
        .route("/items/{id}/collections", get(collections::for_item))
        .route("/items/{id}/request", get(requests::item_request))
        .route(
            "/items/{id}/subtitles",
            post(subtitles::add).layer(DefaultBodyLimit::max(crate::subtitles::MAX_BYTES + 64 * 1024)),
        )
        .route("/items/{id}/subtitles/search", get(subtitles::search))
        .route("/items/{id}/subtitles/{index}", delete(subtitles::delete))
        .route("/people/{id}", get(people::get))
        .route("/people/{id}/photo", get(people::photo))
        .route("/history", get(history::list))
        .route("/history/{id}", delete(history::delete))
        .route("/hubs/home", get(browse::home))
        .route("/continue-watching/{id}", delete(browse::hide_continue_watching))
        .route("/watchlist", get(watchlist::list))
        .route("/watchlist/{id}", put(watchlist::add).delete(watchlist::remove))
        .route("/search", get(browse::search))
        .route("/discover/search", get(requests::search))
        .route("/discover/movie/{tmdb_id}", get(requests::movie))
        .route("/discover/show/{tmdb_id}", get(requests::show))
        .route("/discover/images/{kind}/{file}", get(requests::image))
        .route("/discover/{list}", get(requests::discover))
        .route("/requests", get(requests::list).post(requests::create))
        .route("/requests/summary", get(requests::summary))
        .route("/requests/seen", post(requests::seen))
        .route("/requests/options", get(requests::options))
        .route("/requests/{id}", get(requests::get).delete(requests::delete))
        .route("/requests/{id}/watch", delete(requests::unwatch))
        .route("/requests/{id}/approve", post(requests::approve))
        .route("/requests/{id}/decline", post(requests::decline))
        .route("/requests/{id}/handled", post(requests::handled))
        .route("/playback", post(playback::start))
        .route("/playback/network", get(playback::network).put(playback::report_network))
        .route("/playback/speedtest", get(playback::speed_test))
        .route("/playback/{session}", get(playback::status).delete(playback::stop))
        .route("/playback/{session}/bitrate", put(playback::set_bitrate))
        .route("/playback/{session}/{file}", get(playback::hls_file))
        .route("/media/{key}/{file}", get(playback::keyed_media))
        .route("/files/{id}/stream", get(playback::stream_file))
        .route("/conversions", post(playback::convert))
        .route("/live", get(livetv::summary))
        .route("/live/channels", get(livetv::channels))
        .route("/live/groups", get(livetv::groups))
        .route("/live/browse", get(livetv::browse))
        .route("/live/channels/{id}", get(livetv::channel))
        .route("/live/channels/{id}/favorite", put(livetv::favorite).delete(livetv::unfavorite))
        .route("/live/channels/{id}/logo", get(livetv::logo))
        .route("/live/channels/{id}/programs/{start}/image", get(livetv::program_image))
        .route("/live/channels/{id}/watch", post(livetv::watch))
        .route("/live/channels/{id}/stream.ts", get(livetv::stream))
        .route("/live/guide", get(livetv::guide))
        .route("/live/sessions/{session}", delete(livetv::leave))
        .route("/live/sessions/{session}/plan", get(livetv::session_plan))
        .route("/live/sessions/{session}/{file}", get(livetv::session_file))
        .route("/live/media/{key}/{file}", get(livetv::keyed_file))
        .route("/live/settings", get(livetv::get_settings).put(livetv::put_settings))
        .route("/live/sources", get(livetv::sources).post(livetv::add_source))
        .route("/live/sources/test", post(livetv::test_source))
        .route("/live/sources/{id}", put(livetv::update_source).delete(livetv::delete_source))
        .route("/live/sources/{id}/refresh", post(livetv::refresh_source))
        .route("/live/lineup", get(livetv::admin_channels).patch(livetv::patch_channels))
        .route("/live/lineup/groups", get(livetv::admin_groups))
        .route("/live/lineup/page", get(livetv::admin_page))
        .route("/live/tuners", get(livetv::tuners))
        .route("/live/tuners/{channel}", delete(livetv::stop_tuner))
        .route("/conversions/{id}", get(playback::conversion).delete(playback::cancel_conversion))
        .route("/conversions/{id}/file", get(playback::conversion_file))
        .route("/files/{id}/subtitles/{file}", get(playback::subtitles))
        .route("/files/{id}/trickplay", get(playback::trickplay))
        .route("/files/{id}/trickplay/{sheet}", get(playback::trickplay_sheet))
        .route("/files/{id}/chapters/{image}", get(playback::chapter_image))
        .route("/sessions", get(remote::list))
        .route("/sessions/{id}", delete(playback::stop_session))
        .route("/sessions/{id}/command", post(remote::command))
        .route("/remote/devices", get(remote::devices))
        .route("/remote/devices/{id}/cast", post(remote::cast))
        .route("/activity", get(activity::activity))
        .route("/events", get(activity::events))
        .route("/users", get(accounts::list_users).post(accounts::create_user))
        .route("/users/{id}", patch(accounts::update_user).delete(accounts::delete_user))
        .route("/users/{id}/avatar", get(accounts::avatar))
        .route("/users/{id}/devices", get(accounts::user_devices).delete(accounts::sign_out_everywhere))
        .route("/users/{id}/apps", get(oauth::user_apps))
        .route("/users/{id}/apps/{grant}", delete(oauth::sign_out_user_app))
        .route("/users/{id}/two-step", delete(two_step::reset_user))
        .route("/invites", get(accounts::list_invites).post(accounts::create_invite))
        .route("/invites/{code}", get(accounts::check_invite).delete(accounts::delete_invite))
        .route("/invites/{code}/accept", post(accounts::accept_invite))
        .route("/ratings/systems", get(accounts::rating_systems))
        .route("/labels", get(editing::labels))
        .route("/system", get(admin::system))
        .route("/system/resources", get(admin::resources))
        .route("/system/library-work", get(admin::library_work))
        .route("/system/storage", get(admin::storage))
        .route("/system/backups", get(admin::backups).post(admin::create_backup))
        .route("/system/storage/clean", post(admin::clean_storage))
        .route("/system/transcode-dir", put(admin::put_transcode_dir))
        .route("/transcoder", get(admin::transcoder_status))
        .route("/settings", get(admin::get_settings).put(admin::put_settings))
        .route("/settings/test", post(admin::test_service))
        .route("/settings/arr", post(admin::arr_options))
        .route("/network", get(admin::get_network).put(admin::put_network))
        .route("/fs", post(admin::browse_fs))
        .fallback(|| async { AppError::not_found("no such endpoint") })
        .layer(SetResponseHeaderLayer::if_not_present(header::CACHE_CONTROL, HeaderValue::from_static("no-store")));

    // Compress text responses only (never media, never the SSE stream).
    let compressible = SizeAbove::new(1024)
        .and(NotForContentType::new("video/"))
        .and(NotForContentType::new("audio/"))
        .and(NotForContentType::new("image/"))
        .and(NotForContentType::new("text/event-stream"))
        .and(NotForContentType::new("application/octet-stream"));

    Router::new()
        .nest("/api", api)
        // Before the web app's pages, which would answer any path.
        .route("/.well-known/oauth-authorization-server", get(oauth::metadata))
        .route("/.well-known/{*rest}", any(oauth::no_document))
        .fallback(crate::web::static_handler)
        .layer(axum::middleware::from_fn(tv_app_cors))
        .layer(axum::middleware::from_fn_with_state(state.clone(), proxy_port))
        .layer(SetResponseHeaderLayer::if_not_present(
            header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_static(CONTENT_SECURITY_POLICY),
        ))
        .layer(SetResponseHeaderLayer::overriding(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff")))
        .layer(SetResponseHeaderLayer::overriding(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY")))
        .layer(SetResponseHeaderLayer::overriding(header::REFERRER_POLICY, HeaderValue::from_static("same-origin")))
        .layer(SetResponseHeaderLayer::overriding(
            HeaderName::from_static("cross-origin-opener-policy"),
            HeaderValue::from_static("same-origin"),
        ))
        .layer(CompressionLayer::new().quality(CompressionLevel::Precise(4)).compress_when(compressible))
        .layer(TraceLayer::new_for_http().make_span_with(|req: &axum::http::Request<_>| {
            tracing::debug_span!("http", method = %req.method(), uri = %loggable_uri(req.uri()))
        }))
        .layer(tower_http::timeout::TimeoutLayer::with_status_code(
            axum::http::StatusCode::REQUEST_TIMEOUT,
            Duration::from_secs(60 * 30),
        ))
        // The compatibility APIs' clients, before anything of pley's: the
        // Jellyfin-compatible API's first, which leaves the requests that
        // name themselves the Plex-compatible API's to it.
        .layer(axum::middleware::from_fn_with_state(crate::plex::OnWebPort::new(&state), crate::plex::on_web_port))
        .layer(axum::middleware::from_fn_with_state(
            crate::jellyfin::OnWebPort::new(&state),
            crate::jellyfin::on_web_port,
        ))
        .with_state(state)
}

/// The largest body of the authorization server's endpoints (forms, and
/// the requests of the consent and `/link` pages).
const FORM_MAX: usize = 16 * 1024;

/// Puts back the port a reverse proxy left out of `Host` (nginx's `$host`,
/// Nginx Proxy Manager's default) for the addresses set in Settings →
/// Network (`proxy_addresses`): pley names itself by `Host`, and apps
/// reaching `https://media.example.com:9999` refuse a server calling itself
/// `https://media.example.com`.
async fn proxy_port(
    axum::extract::State(state): axum::extract::State<AppState>,
    mut request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let settings = state.settings.get();
    if let Some(host) = with_proxy_port(request.headers(), &settings.proxy_addresses) {
        request.headers_mut().insert(header::HOST, host);
    }
    next.run(request).await
}

/// `Host` with the port of the proxy address of its scheme and host name,
/// when it has no port of its own.
fn with_proxy_port(headers: &axum::http::HeaderMap, addresses: &[String]) -> Option<HeaderValue> {
    if addresses.is_empty() {
        return None;
    }
    let host = headers.get(header::HOST)?.to_str().ok()?.trim();
    // A port after the name, or after an IPv6 address's brackets.
    let named = host.rsplit_once(']').map_or(host, |(_, after)| after);
    if named.contains(':') {
        return None;
    }
    let scheme = if crate::auth::forwarded_https(headers) { "https" } else { "http" };
    let asked = url::Url::parse(&format!("{scheme}://{host}")).ok()?;
    addresses.iter().find_map(|address| {
        let address = url::Url::parse(address).ok()?;
        let port = address.port()?;
        (address.scheme() == scheme && address.host() == asked.host())
            .then(|| HeaderValue::from_str(&format!("{host}:{port}")).ok())
            .flatten()
    })
}

/// Whether a request comes from one of pley's packaged TV apps: Samsung's
/// and LG's web runtimes load them from the TV (`file://`), so their
/// requests come with `Origin: file://`, `null`, or an `app://` address.
/// A website's page in a sandboxed frame sends `null` too: what it gets is
/// only what a token it holds opens (no cookie is let through, see
/// [`tv_app_cors`]), and starting a TV sign-in still takes someone signed
/// in to type its code and allow it. Nor does it get a server's proof
/// (`auth::server_status`).
pub(crate) fn tv_app_origin(origin: &[u8]) -> bool {
    matches!(origin, b"null" | b"file://") || origin.starts_with(b"app://")
}

/// What CORS opens to the TV apps: the API and the server's metadata, but
/// never the consent page's own endpoints (they take pley's page only).
fn cors_path(path: &str) -> bool {
    if path == "/.well-known/oauth-authorization-server" {
        return true;
    }
    path.starts_with("/api/") && !path.starts_with("/api/oauth/requests") && path != "/api/oauth/device/verify"
}

/// CORS for pley's packaged TV apps (docs/API.md, "CORS for TV apps"). Their web
/// runtimes enforce it, so their API calls, which carry the app's token in
/// `Authorization` and never a cookie, are answered with
/// `Access-Control-Allow-Origin: *` and their preflights let that header
/// through. A wildcard never lets a page read an answer made with cookies,
/// and only the TV apps' origins get it at all.
async fn tv_app_cors(request: axum::extract::Request, next: axum::middleware::Next) -> axum::response::Response {
    use axum::response::IntoResponse;
    let allowed = request.headers().get(header::ORIGIN).is_some_and(|o| tv_app_origin(o.as_bytes()))
        && cors_path(request.uri().path());
    if !allowed {
        return next.run(request).await;
    }
    if request.method() == axum::http::Method::OPTIONS
        && request.headers().contains_key(header::ACCESS_CONTROL_REQUEST_METHOD)
    {
        return (
            axum::http::StatusCode::NO_CONTENT,
            [
                (header::ACCESS_CONTROL_ALLOW_ORIGIN, "*"),
                (header::ACCESS_CONTROL_ALLOW_METHODS, "GET, HEAD, POST, PUT, PATCH, DELETE"),
                (header::ACCESS_CONTROL_ALLOW_HEADERS, "Authorization, Content-Type, Accept, Range"),
                (header::ACCESS_CONTROL_MAX_AGE, "600"),
            ],
        )
            .into_response();
    }
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, HeaderValue::from_static("*"));
    headers.insert(
        header::ACCESS_CONTROL_EXPOSE_HEADERS,
        HeaderValue::from_static("WWW-Authenticate, X-Pley-Complete, ETag, Content-Range, Retry-After"),
    );
    response
}

/// What pages of the web app may load and do: only pley's own scripts and
/// styles (an injected script can't run, nor send what it finds elsewhere),
/// pictures from pley and TMDB (Fix Match, artwork), media and hls.js's
/// worker from pley or `blob:`, and no other site may frame them.
const CONTENT_SECURITY_POLICY: &str = "default-src 'self'; script-src 'self'; style-src 'self'; \
    img-src 'self' data: blob: https://image.tmdb.org; media-src 'self' blob:; worker-src 'self' blob:; \
    connect-src 'self'; font-src 'self'; object-src 'none'; base-uri 'none'; form-action 'self'; \
    frame-ancestors 'none'";

/// `Some(None)` for an explicit `null`, `None` when the field is absent.
pub(crate) fn nullable<'de, D: serde::Deserializer<'de>, T: serde::Deserialize<'de>>(
    d: D,
) -> Result<Option<Option<T>>, D::Error> {
    <Option<T> as serde::Deserialize>::deserialize(d).map(Some)
}

/// Query string helper: `"true"`/`"1"` → true.
pub(crate) fn truthy(value: &str) -> bool {
    matches!(value, "1" | "true" | "yes")
}

/// `summaries=1`: a list's movies, shows and seasons with their synopses
/// (see [`ItemRow::to_summary_with`](crate::catalog::ItemRow::to_summary_with)).
pub(crate) fn wants_summaries(query: &HashMap<String, String>) -> bool {
    query.get("summaries").is_some_and(|v| truthy(v))
}

/// The request URI for logs, with credentials in the query left out:
/// `token`, `X-Plex-Token` and `api_key`, and what apps and TVs send while
/// signing in (`code`, `state`, `code_verifier`, `access_token`,
/// `refresh_token`, `device_code`, `user_code`). Bodies and the
/// Authorization header are never logged.
pub(crate) fn loggable_uri(uri: &axum::http::Uri) -> String {
    // A keyed media address carries its session's key in the path.
    let keyed = ["/api/media/", "/api/live/media/"]
        .into_iter()
        .find_map(|prefix| uri.path().strip_prefix(prefix).map(|rest| (prefix, rest)));
    let path = match keyed {
        Some((prefix, rest)) => format!("{prefix}…{}", rest.find('/').map_or("", |at| &rest[at..])),
        // A Jellyfin media source's id stands for its token.
        None => {
            uri.path()
                .split('/')
                .map(|segment| {
                    if segment.len() == 64 && segment.bytes().all(|b| b.is_ascii_hexdigit()) { "…" } else { segment }
                })
                .collect::<Vec<_>>()
                .join("/")
        }
    };
    let Some(query) = uri.query() else { return path };
    let query: Vec<String> = query
        .split('&')
        .map(|pair| match pair.split_once('=') {
            Some((key, _))
                if matches!(
                    key.to_ascii_lowercase().as_str(),
                    "token"
                        | "x-plex-token"
                        | "api_key"
                        | "apikey"
                        | "mediasourceid"
                        | "code"
                        | "state"
                        | "code_verifier"
                        | "access_token"
                        | "refresh_token"
                        | "device_code"
                        | "user_code"
                ) =>
            {
                format!("{key}=…")
            }
            _ => pair.to_owned(),
        })
        .collect();
    format!("{}?{}", path, query.join("&"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_proxy_that_left_the_port_out_gets_it_back() {
        let addresses = ["https://media.example.com:9999".to_owned(), "http://[fd00::5]:8096".to_owned()];
        let host = |pairs: &[(&'static str, &str)]| {
            let mut headers = axum::http::HeaderMap::new();
            for (name, value) in pairs {
                headers.insert(*name, value.parse().unwrap());
            }
            with_proxy_port(&headers, &addresses).map(|h| h.to_str().unwrap().to_owned())
        };
        let https = ("x-forwarded-proto", "https");
        assert_eq!(host(&[("host", "media.example.com"), https]).as_deref(), Some("media.example.com:9999"));
        assert_eq!(host(&[("host", "Media.Example.com"), https]).as_deref(), Some("Media.Example.com:9999"));
        assert_eq!(host(&[("host", "[fd00::5]")]).as_deref(), Some("[fd00::5]:8096"));
        // A port of its own, another scheme or another host stays as it came.
        assert_eq!(host(&[("host", "media.example.com:443"), https]), None);
        assert_eq!(host(&[("host", "media.example.com")]), None, "came over http");
        assert_eq!(host(&[("host", "other.example.com"), https]), None);
        assert_eq!(host(&[("host", "[fd00::5]:7539")]), None);
        assert_eq!(host(&[https]), None, "no Host");
    }

    #[test]
    fn the_policy_is_one_header_line() {
        assert!(HeaderValue::from_static(CONTENT_SECURITY_POLICY).to_str().is_ok());
        assert!(!CONTENT_SECURITY_POLICY.contains("unsafe") && !CONTENT_SECURITY_POLICY.contains("  "));
    }

    #[test]
    fn logs_leave_codes_and_tokens_out() {
        let uri: axum::http::Uri =
            "/api/files/3/stream?token=secret&t=10&ApiKey=abc&api_key=k&X-Plex-Token=p".parse().unwrap();
        assert_eq!(loggable_uri(&uri), "/api/files/3/stream?token=…&t=10&ApiKey=…&api_key=…&X-Plex-Token=…");
        let uri: axum::http::Uri = "/api/x?code=c0de&state=st4te".parse().unwrap();
        assert_eq!(loggable_uri(&uri), "/api/x?code=…&state=…");
        let uri =
            "/oauth/authorize?client_id=c&code_challenge=x&State=s&code_verifier=v&access_token=a&refresh_token=r"
                .parse()
                .unwrap();
        assert_eq!(
            loggable_uri(&uri),
            "/oauth/authorize?client_id=c&code_challenge=x&State=…&code_verifier=…&access_token=…&refresh_token=…"
        );
        // A TV's codes: the address the person opens, and what an app may
        // put in one by mistake.
        let uri: axum::http::Uri = "/link?code=BCDFGHJK".parse().unwrap();
        assert_eq!(loggable_uri(&uri), "/link?code=…");
        let uri: axum::http::Uri = "/link?user_code=BCDF-GHJK&device_code=d&client_id=c".parse().unwrap();
        assert_eq!(loggable_uri(&uri), "/link?user_code=…&device_code=…&client_id=c");
        assert_eq!(loggable_uri(&"/api/items/1".parse().unwrap()), "/api/items/1");
        // A keyed media address's key is in its path.
        assert_eq!(loggable_uri(&"/api/media/pley_mk_abc/12.ts".parse().unwrap()), "/api/media/…/12.ts");
        assert_eq!(loggable_uri(&"/api/media/pley_mk_abc".parse().unwrap()), "/api/media/…");
        assert_eq!(loggable_uri(&"/api/live/media/pley_mk_abc/s12.ts".parse().unwrap()), "/api/live/media/…/s12.ts");
    }

    #[test]
    fn cors_is_for_the_tv_apps_and_never_the_consent_page() {
        assert!(tv_app_origin(b"file://") && tv_app_origin(b"null") && tv_app_origin(b"app://pley.tizen"));
        assert!(!tv_app_origin(b"https://evil.example") && !tv_app_origin(b"http://192.168.1.20:7539"));
        assert!(cors_path("/api/hubs/home") && cors_path("/api/oauth/token") && cors_path("/api/oauth/device"));
        assert!(cors_path("/.well-known/oauth-authorization-server"));
        assert!(!cors_path("/api/oauth/requests") && !cors_path("/api/oauth/requests/7/continue"));
        assert!(!cors_path("/api/oauth/device/verify") && !cors_path("/link") && !cors_path("/"));
    }
}
