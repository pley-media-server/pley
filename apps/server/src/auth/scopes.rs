//! What an app may do with its token: the scopes a person allows on pley's
//! own page, and the routes each one opens. Everything else stays closed to
//! apps (one's own sign-in settings and devices, the Jellyfin-compatible API):
//! there is no scope for it. Administration opens only to pley's own apps,
//! with `server`, on an administrator's account.

use std::fmt;

use axum::http::Method;

/// One thing an app may be allowed to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Scope {
    /// See the profile's libraries, playlists and watch history.
    Library,
    /// Play, and keep the place, watched marks, ratings, track choices and
    /// the order of the libraries.
    Play,
    /// Make, change and delete playlists.
    Playlists,
    /// Switch to the household's other profiles (pley's own apps only: a
    /// profile's PIN is typed into the app).
    Profiles,
    /// Ask for movies and shows the libraries don't have (Discover), and
    /// follow the profile's requests.
    Requests,
    /// Manage everyone's requests where the account is an administrator
    /// (pley's own apps only, like `profiles`; it does nothing for other
    /// accounts).
    Approvals,
    /// See what's playing and stop it: the profile's own streams, and
    /// everyone's where the account is an administrator (pley's own apps
    /// only, see `AuthUser::oversees_playback`; other apps see and stop
    /// only the profile's own).
    Activity,
    /// Manage the server where the account is an administrator: its
    /// libraries, accounts, invitations and the dashboard (pley's own apps
    /// only, never with a TV's code; it does nothing for other accounts, see
    /// `AuthUser::administers`). What holds keys and passwords stays in the
    /// web app: the server's settings, its network, the transcoder's
    /// settings, Radarr and Sonarr, backups and Live TV's sources; and Live
    /// TV's lineup and tuners are managed there with its sources.
    Server,
}

impl Scope {
    /// Every scope, in the order they are written and shown.
    pub const ALL: [Scope; 8] = [
        Scope::Library,
        Scope::Play,
        Scope::Playlists,
        Scope::Profiles,
        Scope::Requests,
        Scope::Approvals,
        Scope::Activity,
        Scope::Server,
    ];

    /// The name in `scope` parameters and answers.
    pub fn name(self) -> &'static str {
        match self {
            Scope::Library => "library",
            Scope::Play => "play",
            Scope::Playlists => "playlists",
            Scope::Profiles => "profiles",
            Scope::Requests => "requests",
            Scope::Approvals => "approvals",
            Scope::Activity => "activity",
            Scope::Server => "server",
        }
    }

    /// What it allows, in the words of the consent page, the profile being
    /// the one the app acts as: "this app wasn't allowed to <words>".
    pub fn words(self) -> &'static str {
        match self {
            Scope::Library => "see the profile's libraries, playlists and watch history",
            Scope::Play => {
                "play titles, and keep the profile's place, watched marks, ratings, track choices and library order"
            }
            Scope::Playlists => "make, change and delete the profile's playlists",
            Scope::Profiles => "switch to the household's other profiles",
            Scope::Requests => "ask for movies and shows the libraries don't have, and follow the profile's requests",
            Scope::Approvals => "approve and decline what people ask for, where the account is an administrator",
            Scope::Activity => {
                "see what's playing and stop it: the profile's streams, and everyone's where the account is an administrator"
            }
            Scope::Server => {
                "manage the server's libraries, accounts and invitations, and see its dashboard, where the account is an administrator"
            }
        }
    }

    pub fn from_name(name: &str) -> Option<Scope> {
        Scope::ALL.into_iter().find(|scope| scope.name() == name)
    }

    fn bit(self) -> u8 {
        1 << (self as u8)
    }
}

/// A set of scopes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Scopes(u8);

impl Scopes {
    /// Every scope there is.
    pub fn all() -> Scopes {
        Scope::ALL.into_iter().fold(Scopes::default(), Scopes::with)
    }

    /// A `scope` parameter: names separated by spaces, each known. The name
    /// that isn't is the error ("openid" among them: pley signs nobody in to
    /// other sites).
    pub fn parse(text: &str) -> Result<Scopes, String> {
        let mut scopes = Scopes::default();
        for name in text.split(' ').filter(|name| !name.is_empty()) {
            scopes = scopes.with(Scope::from_name(name).ok_or_else(|| name.to_owned())?);
        }
        Ok(scopes)
    }

    /// A scope stored with a grant. Names this version doesn't know (from a
    /// newer one) are left out rather than refused.
    pub fn from_stored(text: &str) -> Scopes {
        text.split(' ').filter_map(Scope::from_name).fold(Scopes::default(), Scopes::with)
    }

    pub fn with(self, scope: Scope) -> Scopes {
        Scopes(self.0 | scope.bit())
    }

    pub fn contains(self, scope: Scope) -> bool {
        self.0 & scope.bit() != 0
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub fn is_subset_of(self, other: Scopes) -> bool {
        self.0 & !other.0 == 0
    }

    /// The names, in the fixed order of [`Scope::ALL`].
    pub fn names(self) -> Vec<&'static str> {
        Scope::ALL.into_iter().filter(|scope| self.contains(*scope)).map(Scope::name).collect()
    }
}

impl fmt::Display for Scopes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.names().join(" "))
    }
}

/// What an app's token needs for a route.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Need {
    /// Any app token (who it acts as, and pictures of its household).
    AnyApp,
    Scope(Scope),
    /// Either scope opens it; a token with neither is told of the first.
    Either(Scope, Scope),
}

use Need::AnyApp;
use Scope::{Activity, Approvals, Library, Play, Playlists, Profiles, Requests, Server};

/// The routes apps may use: (method, path as routed, with `/api`, what the
/// token needs). HEAD is looked up as GET.
///
/// A route missing from this table is closed to apps: a route added to
/// `api/mod.rs` stays first-party until it is added here on purpose
/// (tests/app_tokens.rs checks that every route is classified, and that
/// every entry here names a real route).
pub const APP_ROUTES: &[(&str, &str, Need)] = &[
    // Any app.
    ("GET", "/api/auth/me", AnyApp),
    ("GET", "/api/users/{id}/avatar", AnyApp),
    // whether requests are on, and whether the app should sign in again to
    // ask for movies and shows
    ("GET", "/api/requests/summary", AnyApp),
    // library
    ("GET", "/api/libraries", Need::Scope(Library)),
    ("GET", "/api/libraries/{id}", Need::Scope(Library)),
    ("GET", "/api/libraries/{id}/items", Need::Scope(Library)),
    ("GET", "/api/libraries/{id}/filters", Need::Scope(Library)),
    ("GET", "/api/libraries/{id}/hubs", Need::Scope(Library)),
    ("GET", "/api/libraries/{id}/collections", Need::Scope(Library)),
    ("GET", "/api/collections/{id}", Need::Scope(Library)),
    ("GET", "/api/collections/{id}/images/{kind}", Need::Scope(Library)),
    ("GET", "/api/playlists", Need::Scope(Library)),
    ("GET", "/api/playlists/{id}", Need::Scope(Library)),
    ("GET", "/api/items/{id}", Need::Scope(Library)),
    ("GET", "/api/items/{id}/children", Need::Scope(Library)),
    ("GET", "/api/items/{id}/on-deck", Need::Scope(Library)),
    ("GET", "/api/items/{id}/next", Need::Scope(Library)),
    ("GET", "/api/items/{id}/related", Need::Scope(Library)),
    ("GET", "/api/items/{id}/images/{kind}", Need::Scope(Library)),
    ("GET", "/api/items/{id}/collections", Need::Scope(Library)),
    ("GET", "/api/people/{id}", Need::Scope(Library)),
    ("GET", "/api/people/{id}/photo", Need::Scope(Library)),
    ("GET", "/api/history", Need::Scope(Library)),
    ("GET", "/api/hubs/home", Need::Scope(Library)),
    ("GET", "/api/watchlist", Need::Scope(Library)),
    ("GET", "/api/search", Need::Scope(Library)),
    ("GET", "/api/events", Need::Scope(Library)),
    // Live TV's channels and guide
    ("GET", "/api/live", Need::Scope(Library)),
    ("GET", "/api/live/channels", Need::Scope(Library)),
    ("GET", "/api/live/groups", Need::Scope(Library)),
    ("GET", "/api/live/browse", Need::Scope(Library)),
    ("GET", "/api/live/channels/{id}", Need::Scope(Library)),
    ("GET", "/api/live/channels/{id}/logo", Need::Scope(Library)),
    ("GET", "/api/live/channels/{id}/programs/{start}/image", Need::Scope(Library)),
    ("GET", "/api/live/guide", Need::Scope(Library)),
    // play
    ("POST", "/api/playback", Need::Scope(Play)),
    // watching Live TV, and the profile's favourite channels
    ("POST", "/api/live/channels/{id}/watch", Need::Scope(Play)),
    ("GET", "/api/live/channels/{id}/stream.ts", Need::Scope(Play)),
    ("GET", "/api/live/sessions/{session}/plan", Need::Scope(Play)),
    ("GET", "/api/live/sessions/{session}/{file}", Need::Scope(Play)),
    ("DELETE", "/api/live/sessions/{session}", Need::Scope(Play)),
    ("PUT", "/api/live/channels/{id}/favorite", Need::Scope(Play)),
    ("DELETE", "/api/live/channels/{id}/favorite", Need::Scope(Play)),
    // what Automatic knows of the network, and a speed test of it
    ("GET", "/api/playback/network", Need::Scope(Play)),
    ("PUT", "/api/playback/network", Need::Scope(Play)),
    ("GET", "/api/playback/speedtest", Need::Scope(Play)),
    ("GET", "/api/playback/{session}", Need::Scope(Play)),
    ("DELETE", "/api/playback/{session}", Need::Scope(Play)),
    ("PUT", "/api/playback/{session}/bitrate", Need::Scope(Play)),
    ("GET", "/api/playback/{session}/{file}", Need::Scope(Play)),
    ("GET", "/api/files/{id}/stream", Need::Scope(Play)),
    ("POST", "/api/conversions", Need::Scope(Play)),
    ("GET", "/api/conversions/{id}", Need::Scope(Play)),
    ("DELETE", "/api/conversions/{id}", Need::Scope(Play)),
    ("GET", "/api/conversions/{id}/file", Need::Scope(Play)),
    ("GET", "/api/files/{id}/subtitles/{file}", Need::Scope(Play)),
    ("GET", "/api/files/{id}/trickplay", Need::Scope(Play)),
    ("GET", "/api/files/{id}/trickplay/{sheet}", Need::Scope(Play)),
    ("GET", "/api/files/{id}/chapters/{image}", Need::Scope(Play)),
    ("POST", "/api/items/{id}/prefetch", Need::Scope(Play)),
    ("POST", "/api/items/{id}/progress", Need::Scope(Play)),
    ("POST", "/api/items/{id}/watched", Need::Scope(Play)),
    ("POST", "/api/items/{id}/unwatched", Need::Scope(Play)),
    ("PUT", "/api/items/{id}/streams", Need::Scope(Play)),
    ("PUT", "/api/items/{id}/rating", Need::Scope(Play)),
    // subtitles found or added from the player
    ("GET", "/api/items/{id}/subtitles/search", Need::Scope(Play)),
    ("POST", "/api/items/{id}/subtitles", Need::Scope(Play)),
    ("DELETE", "/api/items/{id}/subtitles/{index}", Need::Scope(Play)),
    ("DELETE", "/api/history/{id}", Need::Scope(Play)),
    ("DELETE", "/api/continue-watching/{id}", Need::Scope(Play)),
    ("PUT", "/api/watchlist/{id}", Need::Scope(Play)),
    ("DELETE", "/api/watchlist/{id}", Need::Scope(Play)),
    // the profile's own sessions: an app acts as a viewer, so never another's
    // (stopping one excepted: see `activity` below, which stops them too)
    ("GET", "/api/sessions", Need::Scope(Play)),
    ("DELETE", "/api/sessions/{id}", Need::Either(Play, Activity)),
    // remote control of the profile's own players: never another's, an
    // administrator's profile included
    ("POST", "/api/sessions/{id}/command", Need::Scope(Play)),
    // casting to the profile's own devices, and for one of Pley's own apps
    // allowed `profiles`, to the household's other profiles' devices that
    // switch to it (checked in the handlers); never one outside the
    // household, an administrator's profile included
    ("GET", "/api/remote/devices", Need::Scope(Play)),
    ("POST", "/api/remote/devices/{id}/cast", Need::Scope(Play)),
    // the profile's track choices for every title (its name, picture and
    // sign-in stay in the web app)
    ("PUT", "/api/me/track-defaults", Need::Scope(Play)),
    // the language the profile sees Pley in, as its track choices
    ("PUT", "/api/me/language", Need::Scope(Play)),
    // the order the profile sees its libraries in
    ("PUT", "/api/me/libraries/order", Need::Scope(Play)),
    ("DELETE", "/api/me/libraries/order", Need::Scope(Play)),
    // playlists
    ("POST", "/api/playlists", Need::Scope(Playlists)),
    ("PATCH", "/api/playlists/{id}", Need::Scope(Playlists)),
    ("DELETE", "/api/playlists/{id}", Need::Scope(Playlists)),
    ("POST", "/api/playlists/{id}/items", Need::Scope(Playlists)),
    ("DELETE", "/api/playlists/{id}/items/{entry_id}", Need::Scope(Playlists)),
    ("PUT", "/api/playlists/{id}/order", Need::Scope(Playlists)),
    // profiles (the switch itself is a grant of the token endpoint, which
    // checks the grant's scope)
    ("GET", "/api/oauth/profiles", Need::Scope(Profiles)),
    // requests: Discover, asking, and the profile's own requests
    ("GET", "/api/discover/{list}", Need::Scope(Requests)),
    ("GET", "/api/discover/search", Need::Scope(Requests)),
    ("GET", "/api/discover/movie/{tmdb_id}", Need::Scope(Requests)),
    ("GET", "/api/discover/show/{tmdb_id}", Need::Scope(Requests)),
    ("GET", "/api/discover/images/{kind}/{file}", Need::Scope(Requests)),
    ("GET", "/api/items/{id}/request", Need::Scope(Requests)),
    ("GET", "/api/requests", Need::Scope(Requests)),
    ("POST", "/api/requests", Need::Scope(Requests)),
    ("POST", "/api/requests/seen", Need::Scope(Requests)),
    ("GET", "/api/requests/{id}", Need::Scope(Requests)),
    ("DELETE", "/api/requests/{id}", Need::Scope(Requests)),
    ("DELETE", "/api/requests/{id}/watch", Need::Scope(Requests)),
    // approvals: managing everyone's (an administrator's account only, see
    // AuthUser::manages_requests)
    ("POST", "/api/requests/{id}/approve", Need::Scope(Approvals)),
    ("POST", "/api/requests/{id}/decline", Need::Scope(Approvals)),
    ("POST", "/api/requests/{id}/handled", Need::Scope(Approvals)),
    ("GET", "/api/requests/options", Need::Scope(Approvals)),
    // activity: what plays, the profile's own, and on an administrator's
    // account everyone's, as the dashboard shows it (see
    // AuthUser::oversees_playback). Of others' sessions it reaches only
    // stopping them (`DELETE /api/sessions/{id}`), never remote control or
    // casting. With `server`, the dashboard's too: everyone's, with the
    // administrators' counters.
    ("GET", "/api/activity", Need::Either(Activity, Server)),
    // server: the parts of the web app's Settings a phone has, for pley's
    // own apps on an administrator's account (see AuthUser::administers);
    // the routes are administrators' (`AdminUser`), which answer such an
    // app as they answer an administrator's own sign-in. What holds keys,
    // passwords or providers' addresses stays with the web app: the
    // server's settings and their tests, the network, the transcode folder,
    // backups, the library work's schedule, Live TV's settings and sources,
    // and with them its lineup and tuners.
    // One's own name, password, PIN, two-step sign-in, devices and apps
    // stay in Account, as for every app.
    // libraries
    ("POST", "/api/libraries", Need::Scope(Server)),
    ("POST", "/api/libraries/scan", Need::Scope(Server)),
    ("PUT", "/api/libraries/order", Need::Scope(Server)),
    ("PATCH", "/api/libraries/{id}", Need::Scope(Server)),
    ("DELETE", "/api/libraries/{id}", Need::Scope(Server)),
    ("POST", "/api/libraries/{id}/scan", Need::Scope(Server)),
    ("POST", "/api/fs", Need::Scope(Server)),
    // accounts and invitations
    ("GET", "/api/users", Need::Scope(Server)),
    ("POST", "/api/users", Need::Scope(Server)),
    ("PATCH", "/api/users/{id}", Need::Scope(Server)),
    ("DELETE", "/api/users/{id}", Need::Scope(Server)),
    ("GET", "/api/users/{id}/devices", Need::Scope(Server)),
    ("DELETE", "/api/users/{id}/devices", Need::Scope(Server)),
    ("GET", "/api/users/{id}/apps", Need::Scope(Server)),
    ("DELETE", "/api/users/{id}/apps/{grant}", Need::Scope(Server)),
    ("DELETE", "/api/users/{id}/two-step", Need::Scope(Server)),
    ("GET", "/api/invites", Need::Scope(Server)),
    ("POST", "/api/invites", Need::Scope(Server)),
    ("DELETE", "/api/invites/{code}", Need::Scope(Server)),
    // what an age limit and its exceptions are picked from
    ("GET", "/api/ratings/systems", Need::Scope(Server)),
    ("GET", "/api/labels", Need::Scope(Server)),
    // the dashboard and storage
    ("GET", "/api/system", Need::Scope(Server)),
    ("GET", "/api/system/resources", Need::Scope(Server)),
    ("GET", "/api/system/storage", Need::Scope(Server)),
    ("POST", "/api/system/storage/clean", Need::Scope(Server)),
    ("GET", "/api/transcoder", Need::Scope(Server)),
];

/// What an app's token needs for `method` on the routed `path`; `None`:
/// apps may not use it at all.
pub fn required(method: &Method, path: &str) -> Option<Need> {
    let method = if method == Method::HEAD { "GET" } else { method.as_str() };
    APP_ROUTES.iter().find(|(m, p, _)| *m == method && *p == path).map(|(_, _, need)| *need)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scopes_parse_and_print() {
        let scopes = Scopes::parse("playlists library").unwrap();
        assert_eq!(scopes.to_string(), "library playlists", "always in the same order");
        assert_eq!(Scopes::parse("play  play library").unwrap().to_string(), "library play", "duplicates once");
        assert_eq!(Scopes::parse("").unwrap(), Scopes::default());
        assert_eq!(Scopes::parse("openid library"), Err("openid".to_owned()));
        assert_eq!(Scopes::parse("library admin"), Err("admin".to_owned()));
        assert_eq!(Scopes::parse("Library"), Err("Library".to_owned()), "names are lower case");
        assert_eq!(Scopes::parse("library\tplay"), Err("library\tplay".to_owned()), "spaces only");
        assert_eq!(Scopes::from_stored("library future play"), Scopes::parse("library play").unwrap());
        assert_eq!(Scopes::all().to_string(), "library play playlists profiles requests approvals activity server");
        assert_eq!(
            Scopes::all().names(),
            ["library", "play", "playlists", "profiles", "requests", "approvals", "activity", "server"]
        );
        assert_eq!(Scopes::parse("server library").unwrap().to_string(), "library server");
        assert_eq!(Scopes::parse("activity library").unwrap().to_string(), "library activity");
        assert_eq!(Scopes::parse("profiles library").unwrap().to_string(), "library profiles");
        assert_eq!(Scopes::parse("approvals requests").unwrap().to_string(), "requests approvals");
        assert_eq!(
            Scopes::from_stored("library play playlists profiles"),
            Scopes::parse("library play playlists profiles").unwrap(),
            "grants from before keep what they had"
        );

        let library = Scopes::default().with(Scope::Library);
        assert!(library.contains(Scope::Library) && !library.contains(Scope::Play));
        assert!(library.is_subset_of(Scopes::all()) && library.is_subset_of(library));
        assert!(!Scopes::all().is_subset_of(library));
        assert!(Scopes::default().is_subset_of(library) && Scopes::default().is_empty());
        for scope in Scope::ALL {
            assert_eq!(Scope::from_name(scope.name()), Some(scope));
            assert!(!scope.words().is_empty());
        }
    }

    #[test]
    fn every_app_route_is_on_a_real_path() {
        // Whether each names a route of api/mod.rs is checked by
        // tests/app_tokens.rs, which reads it.
        assert_eq!(APP_ROUTES.len(), 131);
        for (i, (method, path, _)) in APP_ROUTES.iter().enumerate() {
            assert!(["GET", "POST", "PUT", "PATCH", "DELETE"].contains(method), "{method} {path}");
            assert!(path.starts_with("/api/") && !path.ends_with('/'), "{path}");
            assert!(
                !APP_ROUTES[..i].iter().any(|(m, p, _)| m == method && p == path),
                "{method} {path} is listed twice"
            );
        }
        assert_eq!(required(&Method::GET, "/api/libraries"), Some(Need::Scope(Scope::Library)));
        assert_eq!(required(&Method::HEAD, "/api/files/{id}/stream"), Some(Need::Scope(Scope::Play)));
        assert_eq!(required(&Method::POST, "/api/playlists"), Some(Need::Scope(Scope::Playlists)));
        assert_eq!(required(&Method::GET, "/api/auth/me"), Some(Need::AnyApp));
        assert_eq!(required(&Method::GET, "/api/oauth/profiles"), Some(Need::Scope(Scope::Profiles)));
        assert_eq!(required(&Method::GET, "/api/requests/summary"), Some(Need::AnyApp), "even apps that can't ask");
        assert_eq!(required(&Method::POST, "/api/requests"), Some(Need::Scope(Scope::Requests)));
        assert_eq!(required(&Method::POST, "/api/requests/{id}/approve"), Some(Need::Scope(Scope::Approvals)));
        assert_eq!(required(&Method::DELETE, "/api/requests/{id}/watch"), Some(Need::Scope(Scope::Requests)));
        assert_eq!(required(&Method::GET, "/api/requests/options"), Some(Need::Scope(Scope::Approvals)));
        assert_eq!(required(&Method::GET, "/api/activity"), Some(Need::Either(Scope::Activity, Scope::Server)));
        assert_eq!(required(&Method::DELETE, "/api/sessions/{id}"), Some(Need::Either(Scope::Play, Scope::Activity)));
        assert_eq!(required(&Method::PUT, "/api/me/track-defaults"), Some(Need::Scope(Scope::Play)));
        assert_eq!(required(&Method::POST, "/api/remote/devices/{id}/cast"), Some(Need::Scope(Scope::Play)));
        assert_eq!(required(&Method::PATCH, "/api/me"), None, "name and picture stay in the web app");
        assert_eq!(required(&Method::POST, "/api/auth/switch"), None, "the web's switch makes sessions");
        assert_eq!(required(&Method::POST, "/api/libraries"), Some(Need::Scope(Scope::Server)), "administration");
        assert_eq!(required(&Method::GET, "/api/settings"), None, "the server's settings hold its keys");
        assert_eq!(required(&Method::GET, "/api/live/sources"), None, "sources hold providers' passwords");
        assert_eq!(required(&Method::PATCH, "/api/live/lineup"), None, "the lineup stays in the web app");
        assert_eq!(required(&Method::DELETE, "/api/live/tuners/{channel}"), None, "and the tuners");
        assert_eq!(required(&Method::DELETE, "/api/users/{id}/two-step"), Some(Need::Scope(Scope::Server)));
        assert_eq!(required(&Method::PUT, "/api/auth/password"), None, "one's own password stays in Account");
        assert_eq!(required(&Method::PUT, "/api/me/pin"), None, "one's own PIN too");
        assert_eq!(required(&Method::GET, "/api/me/devices"), None, "and one's own devices");
        for (method, path, need) in APP_ROUTES {
            // Every administrator's route an app may use goes by `server`
            // alone, so no other scope reaches administration.
            if path.starts_with("/api/users") && *path != "/api/users/{id}/avatar" || path.starts_with("/api/system") {
                assert_eq!(*need, Need::Scope(Scope::Server), "{method} {path}");
            }
        }
        assert_eq!(required(&Method::GET, "/api/libraries/1"), None, "routed paths, not request paths");
        assert_eq!(required(&Method::OPTIONS, "/api/libraries"), None);
    }
}
