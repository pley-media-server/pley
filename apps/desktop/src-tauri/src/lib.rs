//! pley for Windows, macOS and Linux: a window on a pley server's web app.
//!
//! It opens on a server picker (the page in `../src`), which finds servers on
//! the local network and remembers the ones opened, then shows the chosen
//! server's web app in the same window. That web app leads back to the picker
//! through `/__app/servers`, which the window catches; links to other sites
//! open in the browser (web links only: no other kind of address is handed
//! to the system). Pages from servers get no access to the app's commands:
//! only the picker, bundled with the app, has it. The files a server's page
//! downloads are fetched by the app, to play while the server is away
//! (`downloads`), and the system's notifications tell of requests while the
//! window isn't in front (`notices`), in the language of the server's pages
//! (`words`). Before the app opens a server, and
//! before its own requests with the window's session, the server proves it
//! is the one the window signed in to (`proofs`); the server's pages in the
//! window send the session themselves, so on another network the window
//! leaves a plain-http server at once and opens it again through a proof.
//! What the app writes for the picker (why a server didn't answer, why a
//! download failed) goes by key, which the picker says in the computer's
//! language (`said`).

mod discovery;
mod downloads;
mod notices;
mod proofs;
mod said;
mod servers;
mod words;

use std::sync::Mutex;

use tauri::webview::{DownloadEvent, NewWindowResponse};
use tauri::{AppHandle, Manager, Runtime, WebviewUrl, WebviewWindow, WebviewWindowBuilder};
use tauri_plugin_opener::OpenerExt;
use url::Url;

/// Where the web app sends people who want another server.
const SWITCH_SERVER: &str = "/__app/servers";
/// Tells the web app it runs in this app (src/lib/shell.ts over there).
const INIT_SCRIPT: &str = "window.__PLEY_APP__ = 'desktop';";

/// Where the window is.
#[derive(Debug, Default)]
struct Places {
    /// The picker's address, once it has loaded (the dev server's, while
    /// developing).
    picker: Option<Url>,
    /// The server shown now.
    server: Option<Url>,
    /// The address of this computer the server was reached from when it
    /// opened (see [`network_changed`]).
    route: Option<std::net::IpAddr>,
}

#[derive(Default)]
struct Pages(Mutex<Places>);

fn same_origin(a: &Url, b: &Url) -> bool {
    a.origin() == b.origin()
}

/// What the window does with a navigation to `url`.
#[derive(Debug, PartialEq)]
enum Navigation {
    Allow,
    /// Back to the picker.
    Picker,
    /// Another site: the browser opens it.
    Browser,
    /// Nothing to do with it (another kind of address).
    Deny,
}

/// Whether `url` is one of the app's own pages: Tauri serves them from
/// `tauri://localhost` (`http://tauri.localhost` on Windows), or from the
/// dev server while developing (the picker's address then).
fn is_app_page(url: &Url, picker: Option<&Url>) -> bool {
    let bundled = matches!(
        (url.scheme(), url.host_str(), url.port()),
        ("tauri", Some("localhost"), None) | ("http" | "https", Some("tauri.localhost"), None)
    );
    bundled || picker.is_some_and(|p| same_origin(p, url))
}

/// What to do with a navigation to `url`, following the window from the
/// picker to a server (see [`Places`]).
fn navigation(url: &Url, places: &mut Places) -> Navigation {
    if is_app_page(url, places.picker.as_ref()) {
        // The first of the app's pages is the picker: where "Switch server" leads.
        if places.picker.is_none() {
            let mut picker = url.clone();
            picker.set_query(None);
            places.picker = Some(picker);
        }
        return Navigation::Allow;
    }
    if places.server.as_ref().is_some_and(|server| same_origin(server, url)) {
        if url.path().starts_with(SWITCH_SERVER) {
            return Navigation::Picker;
        }
        return Navigation::Allow;
    }
    match url.scheme() {
        "http" | "https" | "mailto" => Navigation::Browser,
        _ if url.as_str() == "about:blank" => Navigation::Allow,
        _ => Navigation::Deny,
    }
}

fn show_picker<R: Runtime>(window: &WebviewWindow<R>) {
    let picker = window.state::<Pages>().0.lock().unwrap().picker.clone();
    leave_for(
        window,
        picker.map(|mut picker| {
            picker.set_query(Some("pick"));
            picker
        }),
    );
}

/// Takes the window from `server` back to the picker, which says that
/// another device answers there now (see [`proofs`]).
pub(crate) fn not_yours<R: Runtime>(app: &AppHandle<R>, server: &Url) {
    let Some(window) = app.get_webview_window("main") else { return };
    let pages = window.state::<Pages>();
    let picker = {
        let places = pages.0.lock().unwrap();
        if !places.server.as_ref().is_some_and(|shown| same_origin(shown, server)) {
            return;
        }
        places.picker.clone()
    };
    leave_for(
        &window,
        picker.map(|mut picker| {
            picker.query_pairs_mut().append_key_only("pick").append_pair("not-yours", server.as_str());
            picker
        }),
    );
}

/// The computer's addresses changed, or it slept: every server proves
/// itself again. A plain-http server shown leaves the window for the
/// picker, which opens it again once it proves itself there, when the way
/// to it changed (another network) or the computer slept (it may have woken
/// on another). An https server is safe where it is, its address checked by
/// its certificate, and so is one on this computer.
fn network_changed<R: Runtime>(app: &AppHandle<R>, slept: bool) {
    app.state::<proofs::Proofs>().doubt_all();
    let Some(window) = app.get_webview_window("main") else { return };
    let (shown, route, picker) = {
        let pages = window.state::<Pages>();
        let places = pages.0.lock().unwrap();
        let Some(shown) = places.server.clone() else { return };
        (shown, places.route, places.picker.clone())
    };
    let on_this_computer = match shown.host() {
        Some(url::Host::Domain(name)) => name.eq_ignore_ascii_case("localhost"),
        Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
        Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
        None => false,
    };
    if shown.scheme() != "http" || on_this_computer {
        return;
    }
    if slept {
        leave_for(&window, picker);
        return;
    }
    tauri::async_runtime::spawn(async move {
        let changed = route_to(shown.clone()).await != route;
        // Unless the window went elsewhere while the name resolved.
        let still = window.state::<Pages>().0.lock().unwrap().server.as_ref() == Some(&shown);
        if changed && still {
            leave_for(&window, picker);
        }
    });
}

/// [`proofs::route_to`], off the window's thread and the async runtime's:
/// resolving a name can take seconds, the more so on another network.
async fn route_to(server: Url) -> Option<std::net::IpAddr> {
    tauri::async_runtime::spawn_blocking(move || proofs::route_to(&server)).await.ok().flatten()
}

fn leave_for<R: Runtime>(window: &WebviewWindow<R>, picker: Option<Url>) {
    window.state::<Pages>().0.lock().unwrap().server = None;
    window.state::<notices::Notices>().follow(None);
    if let Some(picker) = picker {
        let _ = window.navigate(picker);
    }
}

/// Opens a web or mail link in the browser; any other kind of address (a
/// file, a program's protocol) isn't handed to the system.
fn open_in_browser<R: Runtime>(app: &AppHandle<R>, url: &Url) {
    if !matches!(url.scheme(), "http" | "https" | "mailto") {
        return;
    }
    if let Err(err) = app.opener().open_url(url.as_str(), None::<&str>) {
        eprintln!("cannot open {url}: {err}");
    }
}

/// Shows the server at `url` in the window (the picker calls this once the
/// server answered), with the computer's `languages` as its web view has
/// them, for the notices.
#[tauri::command]
async fn open_server(window: WebviewWindow, url: String, languages: Vec<String>) -> Result<(), said::Said> {
    let origin = servers::server_origin(&url)?;
    window.state::<notices::Notices>().languages_are(languages);
    let route = route_to(origin.clone()).await;
    // The network may have changed since the picker checked it.
    let proofs = window.state::<proofs::Proofs>();
    if !proofs.may_open(&origin) {
        return Err(said::Said::NetworkChanged);
    }
    // The window's cookies go to every port of the host: a server proven at
    // another port keeps its session, which one without a key doesn't get.
    if proofs.host_keyed_elsewhere(&origin) {
        servers::sign_out(&window, &origin);
    }
    {
        let pages = window.state::<Pages>();
        let mut places = pages.0.lock().unwrap();
        places.server = Some(origin.clone());
        places.route = route;
    }
    window.state::<notices::Notices>().follow(Some(origin.clone()));
    window.navigate(origin).map_err(|e| e.to_string().into())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .manage(Pages::default())
        .manage(notices::Notices::default())
        .invoke_handler(tauri::generate_handler![
            servers::saved_servers,
            servers::save_server,
            servers::forget_server,
            servers::check_server,
            discovery::discover,
            open_server,
            downloads::downloads,
            downloads::play_download,
            downloads::show_download,
            downloads::remove_download,
            downloads::retry_download
        ])
        .setup(|app| {
            // While developing, the picker comes from Vite's dev server.
            if cfg!(dev) {
                app.state::<Pages>().0.lock().unwrap().picker = app.config().build.dev_url.clone();
            }
            // On this computer only (not a roaming profile's folder).
            app.manage(proofs::Proofs::load(app.path().app_local_data_dir()?.join("proofs.json")));
            let list = app.path().app_data_dir()?.join("downloads.json");
            app.manage(downloads::Downloads::load(list, downloads::folder(app.handle())));
            let handle = app.handle().clone();
            let opener = app.handle().clone();
            // The picker's address is known once it loads (see `navigation`):
            // until then, the window's address is about:blank.
            WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
                .title("Pley")
                .inner_size(1280.0, 800.0)
                .min_inner_size(720.0, 480.0)
                .initialization_script(INIT_SCRIPT)
                .on_navigation(move |url| {
                    let decision = navigation(url, &mut handle.state::<Pages>().0.lock().unwrap());
                    match decision {
                        Navigation::Allow => true,
                        Navigation::Picker => {
                            if let Some(window) = handle.get_webview_window("main") {
                                show_picker(&window);
                            }
                            false
                        }
                        Navigation::Browser => {
                            open_in_browser(&handle, url);
                            false
                        }
                        Navigation::Deny => false,
                    }
                })
                // Links that open a new window (`target="_blank"`) go to the browser.
                .on_new_window(move |url, _features| {
                    open_in_browser(&opener, &url);
                    NewWindowResponse::Deny
                })
                .on_download(|webview, event| {
                    let app = webview.app_handle();
                    let server = app.state::<Pages>().0.lock().unwrap().server.clone();
                    match event {
                        DownloadEvent::Requested { url, destination } => {
                            downloads::requested(app, server.as_ref(), &url, destination)
                        }
                        DownloadEvent::Finished { url, path, success } => {
                            downloads::finished(app, server.as_ref(), &url, path.as_deref(), success);
                            true
                        }
                        _ => true,
                    }
                })
                .build()?;
            // After the window: downloads and notices use its cookies (the
            // servers' sessions).
            tauri::async_runtime::spawn(downloads::run(app.handle().clone()));
            tauri::async_runtime::spawn(notices::run(app.handle().clone()));
            let watching = app.handle().clone();
            tauri::async_runtime::spawn(proofs::watch_network(move |slept| network_changed(&watching, slept)));
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("Pley could not start");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(s: &str) -> Url {
        Url::parse(s).unwrap()
    }

    fn at_server(picker: &str, server: &str) -> Places {
        Places { picker: Some(url(picker)), server: Some(url(server)), route: None }
    }

    #[test]
    fn the_window_keeps_to_the_server_and_its_picker() {
        let mut places = at_server("http://tauri.localhost/index.html", "http://192.168.1.20:7539");
        let mut at = |u: &str| navigation(&url(u), &mut places);
        assert_eq!(at("http://tauri.localhost/index.html?pick"), Navigation::Allow);
        assert_eq!(at("http://192.168.1.20:7539/item/kx7bq2m"), Navigation::Allow);
        assert_eq!(at("http://192.168.1.20:7539/__app/servers"), Navigation::Picker);
        assert_eq!(at("https://www.themoviedb.org/movie/603"), Navigation::Browser);
        assert_eq!(at("https://www.themoviedb.org/tv/1399"), Navigation::Browser);
        assert_eq!(at("https://www.imdb.com/title/tt0113277/"), Navigation::Browser);
        // Discover and requests are the server's pages.
        for page in ["/discover?q=heat", "/discover/movie/603", "/discover/show/1399", "/requests?status=pending"] {
            assert_eq!(at(&format!("http://192.168.1.20:7539{page}")), Navigation::Allow, "{page}");
        }
        assert_eq!(at("http://192.168.1.20:8080/"), Navigation::Browser, "another port is another site");
        assert_eq!(at("about:blank"), Navigation::Allow);
        // Other kinds of addresses go nowhere: not to the system either.
        for other in
            ["file:///C:/Windows/System32/calc.exe", "search-ms:query=x", "ms-word:ofe|u|x", "javascript:alert(1)"]
        {
            assert_eq!(at(other), Navigation::Deny, "{other}");
        }
        // Only the app's own origins are its pages.
        assert_eq!(at("http://tauri.localhost:8080/"), Navigation::Browser);
        // Before a server is open, only the picker is at home, even before
        // the app knows its address (the first page is loading).
        let mut picking = Places { picker: Some(url("http://tauri.localhost/")), ..Places::default() };
        assert_eq!(navigation(&url("http://192.168.1.20:7539/"), &mut picking), Navigation::Browser);
        let mut loading = Places::default();
        assert_eq!(navigation(&url("http://tauri.localhost/index.html"), &mut loading), Navigation::Allow);
        assert_eq!(navigation(&url("tauri://localhost/index.html"), &mut loading), Navigation::Allow);
        assert_eq!(
            navigation(&url("http://192.168.1.20:7539/__app/servers"), &mut picking),
            Navigation::Browser,
            "only the open server can lead back"
        );
    }

    #[test]
    fn the_first_app_page_is_the_picker_switch_server_leads_to() {
        let mut places = Places::default();
        assert_eq!(navigation(&url("http://tauri.localhost/?pick"), &mut places), Navigation::Allow);
        assert_eq!(places.picker.as_ref().map(Url::as_str), Some("http://tauri.localhost/"));
        places.server = Some(url("http://192.168.1.20:7539"));
        assert_eq!(navigation(&url("http://192.168.1.20:7539/__app/servers"), &mut places), Navigation::Picker);
        // A page of the app elsewhere doesn't move it.
        assert_eq!(navigation(&url("tauri://localhost/index.html"), &mut places), Navigation::Allow);
        assert_eq!(places.picker.as_ref().map(Url::as_str), Some("http://tauri.localhost/"));
    }

    #[test]
    fn another_site_after_a_sign_in_page_opens_in_the_browser() {
        let mut places = at_server("http://tauri.localhost/index.html", "http://192.168.1.20:7539");
        for page in ["http://192.168.1.20:7539/login?next=%2F", "http://192.168.1.20:7539/api/auth/login"] {
            assert_eq!(navigation(&url(page), &mut places), Navigation::Allow, "{page}");
            // Signing in happens on the server's own pages: other sites, and
            // other addresses of the server, open in the browser.
            for other in [
                "https://auth.example.com/authorize?client_id=pley",
                "https://pley.example.com/api/auth/login",
                "https://pley.example.com/",
            ] {
                assert_eq!(navigation(&url(other), &mut places), Navigation::Browser, "{other} after {page}");
            }
        }
        assert_eq!(places.server.as_ref().map(Url::as_str), Some("http://192.168.1.20:7539/"));
    }
}
