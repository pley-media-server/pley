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
//! window isn't in front (`notices`).

mod discovery;
mod downloads;
mod notices;
mod servers;

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
    let pages = window.state::<Pages>();
    let picker = {
        let mut places = pages.0.lock().unwrap();
        places.server = None;
        places.picker.clone()
    };
    window.state::<notices::Notices>().follow(None);
    let Some(mut picker) = picker else { return };
    picker.set_query(Some("pick"));
    let _ = window.navigate(picker);
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
/// server answered).
#[tauri::command]
fn open_server(window: WebviewWindow, url: String) -> Result<(), String> {
    let origin = servers::server_origin(&url)?;
    window.state::<Pages>().0.lock().unwrap().server = Some(origin.clone());
    window.state::<notices::Notices>().follow(Some(origin.clone()));
    window.navigate(origin).map_err(|e| e.to_string())
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
        Places { picker: Some(url(picker)), server: Some(url(server)) }
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
