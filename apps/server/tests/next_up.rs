//! Which episode comes next: Continue Watching's next episodes and a show's
//! Play (docs/API.md, "Continue Watching").

mod common;

use axum::http::StatusCode;
use serde_json::{Value, json};

use common::{TestApp, titles};

const EPISODES: [&str; 4] = ["Emissary", "Past Prologue", "A Man Alone", "Babel"];

struct Show {
    app: TestApp,
    token: String,
    show: i64,
    episodes: std::collections::HashMap<String, i64>,
}

impl Show {
    /// A season of four episodes, nothing watched.
    async fn new() -> Self {
        let app = TestApp::new().await;
        let token = app.setup_admin().await;
        for (e, title) in EPISODES.iter().enumerate() {
            let e = e + 1;
            app.media_file(
                &format!("tv/Deep Space Nine (1993)/Season 01/Deep Space Nine - S01E{e:02} - {title}.mkv"),
                1024,
            );
        }
        let lib = app.library(&token, "TV", "shows", "tv").await;
        let show = app.ids(lib, &token).await["Deep Space Nine"];
        let season = app.children(show, &token).await["Season 1"];
        let episodes = app.children(season, &token).await;
        Show { app, token, show, episodes }
    }

    async fn progress(&self, title: &str, position_ms: i64) {
        let r = self
            .app
            .post(
                &format!("/api/items/{}/progress", self.episodes[title]),
                &self.token,
                json!({"position_ms": position_ms, "duration_ms": 3_000_000, "state": "stopped"}),
            )
            .await;
        assert_eq!(r.status, StatusCode::NO_CONTENT);
    }

    /// Watches `title` to the end, at `at` (viewings are seconds apart, the
    /// server's clock counts seconds).
    async fn watch(&self, title: &str, at: i64) {
        self.progress(title, 2_900_000).await;
        sqlx::query("UPDATE user_item_state SET last_played_at = ? WHERE item_id = ?")
            .bind(at)
            .bind(self.episodes[title])
            .execute(&self.app.state.db.write)
            .await
            .unwrap();
    }

    /// Watches `title` again from the start, at `at`.
    async fn rewatch(&self, title: &str, at: i64) {
        self.progress(title, 600_000).await;
        self.watch(title, at).await;
    }

    /// Continue Watching's titles, and what the show's Play starts.
    async fn next_up(&self) -> (Vec<String>, Value) {
        let home = self.app.get("/api/hubs/home", &self.token).await;
        let cw = home.body["hubs"].as_array().unwrap().iter().find(|h| h["key"] == "continue_watching").cloned();
        let deck = self.app.get(&format!("/api/items/{}/on-deck", self.show), &self.token).await;
        (cw.map(|h| titles(&h["items"])).unwrap_or_default(), deck.body["item"]["title"].clone())
    }

    async fn play_count(&self, title: &str) -> Value {
        let detail = self.app.get(&format!("/api/items/{}", self.episodes[title]), &self.token).await;
        detail.body["user_state"]["play_count"].clone()
    }
}

/// Watching a show through again: Continue Watching and the show's Play go
/// on in order through the episodes seen the first time, not past them.
#[tokio::test]
async fn a_rewatch_goes_on_in_order() {
    let s = Show::new().await;
    for (i, title) in EPISODES.into_iter().enumerate() {
        s.watch(title, 1_000 + i as i64 * 3_000).await;
    }
    // Seen through: nothing comes next.
    assert_eq!(s.next_up().await.0, Vec::<String>::new());

    // Started again: after the first episode, the second, seen or not.
    s.rewatch("Emissary", 100_000).await;
    assert_eq!(s.play_count("Emissary").await, json!(2));
    assert_eq!(s.next_up().await, (vec!["Past Prologue".to_owned()], json!("Past Prologue")));

    // And on, in order.
    s.rewatch("Past Prologue", 103_000).await;
    assert_eq!(s.next_up().await, (vec!["A Man Alone".to_owned()], json!("A Man Alone")));
}

/// One favorite seen again while part way through: back to where the user
/// had got to, not to the episode after the favorite.
#[tokio::test]
async fn a_favourite_seen_again_keeps_the_place() {
    let s = Show::new().await;
    s.watch("Emissary", 1_000).await;
    s.watch("Past Prologue", 4_000).await;
    s.watch("A Man Alone", 7_000).await;
    s.rewatch("Emissary", 100_000).await;
    assert_eq!(s.next_up().await, (vec!["Babel".to_owned()], json!("Babel")));
}
