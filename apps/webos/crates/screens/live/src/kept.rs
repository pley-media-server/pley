//! What Live TV remembers (`LiveTvViews.kt`): the view (the guide or the cards), the category and the
//! channel watched last with the category it was played in, each under keys of their own per server
//! and profile, so the page opens back as this profile left it on this server, after a restart too.
//! Without a server and profile nothing is read or kept. Reads and writes go through the task pool:
//! the main thread never waits on the disk.

use pley_core::Store;
use pley_core::model::LiveCategory;
use pley_player::live::Category;
use pley_shell::AppCx;

/// The view kept for `server` and `profile`: `guide` or `channels`.
pub fn view_key(server: &str, profile: i64) -> String {
    format!("pley.live.view.{server}.{profile}")
}

/// The category kept for `server` and `profile` (`all`, `favorites`, `recent`, `group:<name>`).
pub fn category_key(server: &str, profile: i64) -> String {
    format!("pley.live.category.{server}.{profile}")
}

/// The channel watched last on `server` by `profile`, with the category it was played in: `[id, key]`.
pub fn last_watched_key(server: &str, profile: i64) -> String {
    format!("pley.live.lastWatched.{server}.{profile}")
}

/// How the page shows a category's channels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum View {
    /// The programme guide.
    #[default]
    Guide,
    /// The channels as cards.
    Channels,
}

impl View {
    /// `guide` or `channels`, as kept.
    pub fn key(self) -> &'static str {
        match self {
            View::Guide => "guide",
            View::Channels => "channels",
        }
    }

    /// The view kept as `key`; the guide for anything else.
    pub fn parse(key: Option<&str>) -> View {
        if key == Some("channels") { View::Channels } else { View::Guide }
    }
}

/// What the page opens with.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Kept {
    pub view: View,
    /// The category; never a search.
    pub category: Category,
    /// The channel watched last and its category.
    pub last: Option<(i64, LiveCategory)>,
}

/// Reads what `profile` kept on `server` (on a task thread); the guide of every channel without them.
pub fn read(store: &Store, server: Option<&str>, profile: Option<i64>) -> Kept {
    let (Some(server), Some(profile)) = (server, profile) else { return Kept::default() };
    let view = View::parse(store.get::<String>(&view_key(server, profile)).as_deref());
    let category = match Category::parse(store.get::<String>(&category_key(server, profile)).as_deref()) {
        Category::Search(_) => Category::All,
        c => c,
    };
    let last =
        store.get::<(i64, String)>(&last_watched_key(server, profile)).map(|(id, c)| (id, LiveCategory::parse(&c)));
    Kept { view, category, last }
}

/// Writes `what` with `write` under the key `key_of` makes for the server and profile in use, off the
/// main thread.
fn keep(
    cx: &AppCx,
    key_of: fn(&str, i64) -> String,
    what: &'static str,
    write: impl FnOnce(&Store, &str) -> std::io::Result<()> + Send + 'static,
) {
    let (Some(server), Some(profile)) = (cx.state.server(), cx.state.profile()) else { return };
    let (store, key) = (cx.store().clone(), key_of(server, profile));
    cx.tasks().detach(move || {
        if let Err(e) = write(&store, &key) {
            log::warn!("{what} not kept: {e}");
        }
    });
}

/// Keeps the view.
pub fn keep_view(cx: &AppCx, view: View) {
    keep(cx, view_key, "Live TV's view", move |store, key| store.set(key, view.key()));
}

/// Keeps the category (a search is never kept).
pub fn keep_category(cx: &AppCx, category: &Category) {
    if !matches!(category, Category::Search(_)) {
        let value = category.key();
        keep(cx, category_key, "Live TV's category", move |store, key| store.set(key, &value));
    }
}

/// The channel just tuned and the category it was played in: the page opens back at it.
pub fn keep_last_watched(cx: &mut AppCx, channel: i64, category: &LiveCategory) {
    cx.state.live_last_watched = Some((channel, category.clone()));
    let value = (channel, category.key());
    keep(cx, last_watched_key, "the channel watched last", move |store, key| store.set(key, &value));
}

/// A page's category as the player and the route name it: a search plays among every channel.
pub fn live_category(c: &Category) -> LiveCategory {
    match c {
        Category::All | Category::Search(_) => LiveCategory::All,
        Category::Favorites => LiveCategory::Favorites,
        Category::Recent => LiveCategory::Recent,
        Category::Group(g) => LiveCategory::Group(g.clone()),
    }
}

/// The player's category as the pages list it.
pub fn page_category(c: &LiveCategory) -> Category {
    match c {
        LiveCategory::All => Category::All,
        LiveCategory::Favorites => Category::Favorites,
        LiveCategory::Recent => Category::Recent,
        LiveCategory::Group(g) => Category::Group(g.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn views_and_categories_round_trip() {
        assert_eq!(View::parse(Some(View::Channels.key())), View::Channels);
        assert_eq!(View::parse(Some("guide")), View::Guide);
        assert_eq!(View::parse(None), View::Guide);
        for c in [Category::All, Category::Favorites, Category::Recent, Category::Group("UK| News".into())] {
            assert_eq!(page_category(&live_category(&c)), c);
        }
        assert_eq!(live_category(&Category::Search("bbc".into())), LiveCategory::All);
    }

    #[test]
    fn each_profile_on_each_server_keeps_its_own() {
        let dir = pley_core::device::scratch_dir("live-kept");
        let store = Store::open(&dir).unwrap();
        let a = "http://a";
        store.set(&view_key(a, 2), "channels").unwrap();
        store.set(&category_key(a, 2), "search:bbc").unwrap();
        store.set(&last_watched_key(a, 2), &(7_i64, "group:Kids")).unwrap();
        store.set(&category_key(a, 3), "favorites").unwrap();
        let k = read(&store, Some(a), Some(2));
        assert_eq!(k.view, View::Channels);
        assert_eq!(k.category, Category::All, "a search is never opened on");
        assert_eq!(k.last, Some((7, LiveCategory::Group("Kids".into()))));
        let other = read(&store, Some(a), Some(3));
        assert_eq!((other.view, other.category, other.last), (View::Guide, Category::Favorites, None));
        assert_eq!(read(&store, Some("http://b"), Some(2)), Kept::default());
        assert_eq!(read(&store, None, Some(2)), Kept::default());
    }
}
