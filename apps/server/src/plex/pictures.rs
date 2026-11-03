//! The pictures an item or collection has, as python-plexapi's `posters()`
//! and `arts()` list them (`/library/metadata/{key}/posters`, `/arts`): Pley
//! keeps one of each, the one shown, so that is the list. Each is named as
//! Plex names its own: `upload://…` when it was set by hand (uploaded, or
//! chosen), `metadata://…` when it is the metadata agent's. Kometa resets an
//! overlay from this list: it checks an uploaded picture for its overlay
//! before it takes it, and takes the agent's as it is.

use axum::http::HeaderMap;
use axum::response::Response;

use super::keys::Key;
use super::library::{base_container, encode, visible_libraries};
use super::{Node, PlexError, PlexResult};
use crate::auth::AuthUser;
use crate::catalog;
use crate::state::AppState;

/// A kind of picture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Poster,
    Art,
}

impl Kind {
    /// The kind a list's name (`posters`, `arts`) names.
    pub fn of_list(name: &str) -> Option<Kind> {
        match name {
            "posters" => Some(Kind::Poster),
            "arts" => Some(Kind::Art),
            _ => None,
        }
    }

    /// Plex's name of the kind in pictures' keys.
    fn plural(self) -> &'static str {
        match self {
            Kind::Poster => "posters",
            Kind::Art => "arts",
        }
    }

    /// The image this API serves the picture as.
    fn image(self) -> &'static str {
        match self {
            Kind::Poster => "thumb",
            Kind::Art => "art",
        }
    }
}

/// The picture shown of `kind`: its name and, for the agent's, the agent.
struct Current {
    rating_key: String,
    provider: Option<&'static str>,
}

/// What `key` shows of `kind`, if anything, for a user who may see it.
async fn current(state: &AppState, user: &AuthUser, key: i64, kind: Kind) -> PlexResult<Option<Current>> {
    match Key::of(key) {
        Key::Item(id) => {
            let row = catalog::item(&state.db, user.id, id).await?.ok_or(PlexError::NotFound)?;
            let libraries = visible_libraries(state, user).await?;
            let lib = libraries.iter().find(|l| l.id == row.library_id).ok_or(PlexError::NotFound)?;
            // An episode's poster is its still, as Plex shows it.
            let (field, has) = match kind {
                Kind::Poster if row.kind == "episode" => ("thumb", row.has_thumb),
                Kind::Poster => ("poster", row.has_poster || row.kind == "album"),
                Kind::Art => ("backdrop", row.has_backdrop),
            };
            if !has {
                return Ok(None);
            }
            let by_hand: bool =
                sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM item_overrides WHERE item_id = ? AND field = ?)")
                    .bind(id)
                    .bind(field)
                    .fetch_one(&state.db.read)
                    .await?;
            let agent = match super::meta::section_type(lib) {
                "movie" => "tv.plex.agents.movie",
                "show" => "tv.plex.agents.series",
                _ => "tv.plex.agents.music",
            };
            Ok(Some(named(kind, by_hand, row.updated_at, agent)))
        }
        Key::Collection(id) => {
            let (collection, ..) = super::lists::collection_for(state, user, id).await?;
            let column = if kind == Kind::Poster { "poster" } else { "backdrop" };
            // `column` is one of two of collections' columns.
            let own: bool = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
                "SELECT {column} IS NOT NULL FROM collections WHERE id = ?"
            )))
            .bind(id)
            .fetch_one(&state.db.read)
            .await?;
            // Its own, set by hand unless it came with an automatic
            // collection; else its first item's.
            let by_hand = own && !collection.automatic;
            Ok(Some(named(kind, by_hand, collection.updated_at, "tv.plex.agents.movie")))
        }
        Key::Playlist(id) => {
            crate::playlists::get_rows(&state.db, user, id).await?.ok_or(PlexError::NotFound)?;
            Ok(None)
        }
    }
}

fn named(kind: Kind, by_hand: bool, version: i64, agent: &'static str) -> Current {
    if by_hand {
        Current { rating_key: format!("upload://{}/{version}", kind.plural()), provider: None }
    } else {
        Current { rating_key: format!("metadata://{}/{version}", kind.plural()), provider: Some(agent) }
    }
}

/// `/library/metadata/{key}/posters` (or `/arts`).
pub async fn list(state: &AppState, user: &AuthUser, key: i64, kind: Kind) -> PlexResult<Node> {
    let mut mc = base_container();
    if let Some(picture) = current(state, user, key, kind).await? {
        let url = format!("/library/metadata/{key}/file?url={}", encode(&picture.rating_key));
        mc.push(
            Node::new("Photo")
                .attr("key", url.clone())
                .attr("ratingKey", picture.rating_key)
                .attr("selected", true)
                .attr("thumb", url)
                .opt("provider", picture.provider),
        );
    }
    let size = mc.children().len();
    Ok(mc.attr("size", size))
}

/// The kind of picture a picture's name (`upload://posters/…`) is of.
fn kind_named(url: &str) -> Option<Kind> {
    let rest = url.strip_prefix("upload://").or_else(|| url.strip_prefix("metadata://"))?;
    Kind::of_list(rest.split('/').next()?)
}

/// `/library/metadata/{key}/file?url=…`: the picture a list named, which is
/// the one shown.
pub async fn file(
    state: AppState,
    user: AuthUser,
    key: i64,
    url: &str,
    width: Option<u32>,
    headers: HeaderMap,
) -> PlexResult<Response> {
    let kind = kind_named(url).ok_or(PlexError::NotFound)?;
    super::library::serve_key_image(state, user, key, kind.image(), None, width, headers).await
}

/// Whether `url` (as `select()` sends it back) names the picture of `kind`
/// that `key` shows: choosing it again changes nothing. `None`: it doesn't
/// name one of Pley's pictures.
pub async fn is_current(
    state: &AppState,
    user: &AuthUser,
    key: i64,
    kind: Kind,
    url: &str,
) -> PlexResult<Option<bool>> {
    if kind_named(url).is_none() {
        return Ok(None);
    }
    let shown = current(state, user, key, kind).await?;
    Ok(Some(shown.is_some_and(|c| c.rating_key == url)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pictures_are_named_as_plex_names_them() {
        let picture = named(Kind::Poster, true, 17, "tv.plex.agents.movie");
        assert_eq!((picture.rating_key.as_str(), picture.provider), ("upload://posters/17", None));
        assert_eq!(named(Kind::Art, false, 3, "a").rating_key, "metadata://arts/3");
        assert_eq!(kind_named("upload://posters/17"), Some(Kind::Poster));
        assert_eq!(kind_named("metadata://arts/3"), Some(Kind::Art));
        assert_eq!(kind_named("https://image.tmdb.org/t/p/original/x.jpg"), None);
    }
}
