//! The servers the app has opened (kept in `servers.json` in its config
//! folder) and the check that a server is there.

use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};
use url::Url;

/// A server opened before.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SavedServer {
    pub url: String,
    pub name: String,
    pub id: Option<String>,
    /// When it was last opened (Unix ms), as the app noted it.
    #[serde(default)]
    pub last_used: i64,
}

/// What a server's `/api/server` says.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerInfo {
    pub name: String,
    pub version: String,
    pub setup_required: bool,
}

/// How long a server gets to answer.
const CHECK_TIMEOUT: Duration = Duration::from_secs(5);

/// The origin of a server's address, for http and https only.
pub fn server_origin(url: &str) -> Result<Url, String> {
    let parsed = Url::parse(url).map_err(|_| format!("{url} isn't an address"))?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
        return Err(format!("{url} isn't a web address"));
    }
    Url::parse(&parsed.origin().ascii_serialization()).map_err(|e| e.to_string())
}

/// `servers` with `server` in it: it replaces an entry with the same address
/// or the same id (a server found at a new address).
pub fn with_server(mut servers: Vec<SavedServer>, server: SavedServer) -> Vec<SavedServer> {
    servers.retain(|s| s.url != server.url && (server.id.is_none() || s.id != server.id));
    servers.push(server);
    servers
}

fn store(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    Ok(dir.join("servers.json"))
}

pub(crate) fn load(app: &AppHandle) -> Vec<SavedServer> {
    store(app)
        .ok()
        .and_then(|path| std::fs::read(path).ok())
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

fn save(app: &AppHandle, servers: &[SavedServer]) -> Result<(), String> {
    let path = store(app)?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let bytes = serde_json::to_vec_pretty(servers).map_err(|e| e.to_string())?;
    // Written whole, then moved over the old file.
    let partial = path.with_extension("json.part");
    std::fs::write(&partial, bytes).map_err(|e| e.to_string())?;
    std::fs::rename(&partial, &path).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn saved_servers(app: AppHandle) -> Vec<SavedServer> {
    load(&app)
}

/// Remembers `server`, opened now (the app notes when: the page can't make
/// a server open by itself at every start).
#[tauri::command]
pub fn save_server(app: AppHandle, server: SavedServer) -> Result<(), String> {
    let origin = server_origin(&server.url)?;
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64);
    let server = SavedServer { url: origin.as_str().trim_end_matches('/').to_owned(), last_used: now, ..server };
    save(&app, &with_server(load(&app), server))
}

#[tauri::command]
pub fn forget_server(app: AppHandle, url: String) -> Result<(), String> {
    let servers: Vec<SavedServer> = load(&app).into_iter().filter(|s| s.url != url).collect();
    save(&app, &servers)
}

/// Whether a pley server answers at `url`, and what it says about itself.
#[tauri::command]
pub async fn check_server(url: String) -> Result<ServerInfo, String> {
    let origin = server_origin(&url)?;
    let endpoint = origin.join("/api/server").map_err(|e| e.to_string())?;
    let client = reqwest::Client::builder().timeout(CHECK_TIMEOUT).build().map_err(|e| e.to_string())?;
    let response = client.get(endpoint).send().await.map_err(|err| {
        if err.is_timeout() {
            format!("no answer within {} seconds", CHECK_TIMEOUT.as_secs())
        } else if err.is_connect() {
            "nothing answers there".to_owned()
        } else {
            err.without_url().to_string()
        }
    })?;
    if !response.status().is_success() {
        return Err(format!("it answered HTTP {}, so it isn't a Pley server", response.status().as_u16()));
    }
    response.json::<ServerInfo>().await.map_err(|_| "that isn't a Pley server".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server(url: &str, id: Option<&str>, last_used: i64) -> SavedServer {
        SavedServer { url: url.into(), name: url.into(), id: id.map(Into::into), last_used }
    }

    #[test]
    fn addresses_are_origins_of_web_urls() {
        assert_eq!(server_origin("http://nas:7539/library/x").unwrap().as_str(), "http://nas:7539/");
        assert_eq!(server_origin("https://pley.example.com").unwrap().as_str(), "https://pley.example.com/");
        for bad in ["file:///C:/x", "ftp://nas", "nas", "javascript:alert(1)", "http://"] {
            assert!(server_origin(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn saving_replaces_the_same_address_or_id() {
        let list = vec![server("http://a:7539", Some("1"), 1), server("http://b:7539", None, 2)];
        let list = with_server(list, server("http://b:7539", None, 3));
        assert_eq!(
            list.iter().map(|s| (s.url.as_str(), s.last_used)).collect::<Vec<_>>(),
            [("http://a:7539", 1), ("http://b:7539", 3)]
        );
        // The same server at a new address.
        let list = with_server(list, server("http://192.168.1.5:7539", Some("1"), 4));
        assert_eq!(
            list.iter().map(|s| s.url.as_str()).collect::<Vec<_>>(),
            ["http://b:7539", "http://192.168.1.5:7539"]
        );
    }

    /// A one-request HTTP server that answers `body` with `status`.
    async fn answering(status: &str, body: &'static str) -> String {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let head = format!(
            "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
            body.len()
        );
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = [0u8; 1024];
            let _ = socket.read(&mut request).await;
            socket.write_all(head.as_bytes()).await.unwrap();
            socket.write_all(body.as_bytes()).await.unwrap();
        });
        format!("http://{addr}")
    }

    #[tokio::test]
    async fn checks_say_what_answered() {
        let pley = answering("200 OK", r#"{"name":"Den","version":"0.1.0","setup_required":false}"#).await;
        let info = check_server(pley).await.unwrap();
        assert_eq!((info.name.as_str(), info.version.as_str()), ("Den", "0.1.0"));

        let other = answering("200 OK", "<html>router</html>").await;
        assert_eq!(check_server(other).await.unwrap_err(), "that isn't a Pley server");
        let missing = answering("404 Not Found", "{}").await;
        assert!(check_server(missing).await.unwrap_err().contains("HTTP 404"));

        // Nothing listens on a port that was just freed.
        let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        assert_eq!(check_server(format!("http://127.0.0.1:{port}")).await.unwrap_err(), "nothing answers there");
    }
}
