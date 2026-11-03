//! The catalogue as a profile sees it: its own positions, watched marks,
//! ratings and track choices in every item, playlist and collection.

use crate::catalog::{BASE_TIME, Catalog, Collection, DAY};
use crate::household::{HistoryRecord, Playlist, Profile};
use crate::model::{
    CollectionDetail, CollectionSummary, HistoryEntry, HistoryUser, ItemDetail, ItemSummary, Kind, Me, MediaFile,
    MediaStream, MediaSummary, PlaylistDetail, PlaylistEntry, PlaylistSummary, UserState,
};

/// How a list shows its items.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ListOpts {
    /// A library listing: no backdrop or still.
    pub listing: bool,
    /// Summaries for every kind (`summaries=1`); else only episodes have them,
    /// as the server's lists.
    pub summaries: bool,
    /// The best file's numbers (listings sorted by bitrate).
    pub media: bool,
}

/// An item's state for a profile; shows, seasons, artists and albums from
/// their episodes or tracks.
pub(crate) fn state(catalog: &Catalog, item: &ItemDetail, profile: &Profile) -> UserState {
    if item.kind().playable() {
        return profile.state(item.id());
    }
    let leaves: Vec<UserState> = catalog.leaves(item.id()).iter().map(|id| profile.state(*id)).collect();
    UserState {
        position_ms: 0,
        watched: !leaves.is_empty() && leaves.iter().all(|s| s.watched),
        play_count: leaves.iter().map(|s| s.play_count).min().unwrap_or(0),
        last_played_at: leaves.iter().filter_map(|s| s.last_played_at).max(),
        rating: profile.state(item.id()).rating,
    }
}

fn viewed(catalog: &Catalog, item: &ItemDetail, profile: &Profile) -> Option<i64> {
    matches!(item.kind(), Kind::Show | Kind::Season)
        .then(|| catalog.leaves(item.id()).iter().filter(|id| profile.state(**id).watched).count() as i64)
}

/// An item as lists give it.
pub(crate) fn summary(catalog: &Catalog, item: &ItemDetail, profile: &Profile, opts: ListOpts) -> ItemSummary {
    let mut s = item.base.clone();
    s.user_state = state(catalog, item, profile);
    s.viewed_leaf_count = viewed(catalog, item, profile);
    if opts.listing {
        s.backdrop_url = None;
        s.thumb_url = None;
    }
    if !opts.summaries && item.kind() != Kind::Episode {
        s.summary = None;
    }
    s.media = if opts.media { media_summary(catalog, item) } else { None };
    s
}

pub(crate) fn summary_of(catalog: &Catalog, id: i64, profile: &Profile, opts: ListOpts) -> Option<ItemSummary> {
    catalog.item(id).map(|item| summary(catalog, item, profile, opts))
}

/// GET /api/items/{id}: with the profile's state, track choices and whether
/// it is in My List. `added` gives the subtitles added to a file.
pub(crate) fn detail(
    catalog: &Catalog,
    item: &ItemDetail,
    profile: &Profile,
    listed: &[i64],
    added: &dyn Fn(i64) -> Vec<MediaStream>,
) -> ItemDetail {
    let mut d = item.clone();
    d.base.user_state = state(catalog, item, profile);
    d.base.viewed_leaf_count = viewed(catalog, item, profile);
    d.files = item
        .files
        .iter()
        .map(|file| {
            let mut file = file.clone();
            file.streams.extend(added(file.id));
            with_choices(item, &file, profile)
        })
        .collect();
    for extra in &mut d.extras {
        extra.user_state = profile.state(extra.id);
    }
    d.in_watchlist = listed.contains(&item.id());
    d
}

/// The profile's My List, newest added first. A movie leaves it once watched
/// after it was added (by playing it to the end or by hand).
pub(crate) fn listed(catalog: &Catalog, profile: &mut Profile) -> Vec<i64> {
    let mut gone = Vec::new();
    for entry in &mut profile.watchlist {
        let Some(item) = catalog.item(entry.item_id) else {
            gone.push(entry.item_id);
            continue;
        };
        if item.kind() != Kind::Movie {
            continue;
        }
        let state = profile.states.get(&item.id()).cloned().unwrap_or_default();
        if !state.watched {
            entry.watched_then = false;
        } else if !entry.watched_then || state.last_played_at.unwrap_or(0) > entry.added_at {
            gone.push(entry.item_id);
        }
    }
    if !gone.is_empty() {
        profile.watchlist.retain(|e| !gone.contains(&e.item_id));
        profile.version += 1;
    }
    let mut entries: Vec<_> = profile.watchlist.iter().map(|e| (e.added_at, e.item_id)).collect();
    entries.sort_by_key(|e| std::cmp::Reverse(e.0));
    entries.into_iter().map(|(_, id)| id).collect()
}

/// Whether an item is out of Continue Watching for the profile: taken out (an
/// episode with its show), and not played since.
pub(crate) fn hidden_from_continue(catalog: &Catalog, item: &ItemDetail, profile: &Profile) -> bool {
    let owner = if item.kind() == Kind::Episode { item.base.grandparent_id.unwrap_or(item.id()) } else { item.id() };
    let Some(&hidden_at) = profile.hidden_from_continue.get(&owner) else { return false };
    let played = if owner == item.id() {
        profile.state(item.id()).last_played_at
    } else {
        catalog.leaves(owner).iter().filter_map(|id| profile.state(*id).last_played_at).max()
    };
    played.unwrap_or(0) <= hidden_at
}

/// A movie's best file; a show's episodes (an artist's tracks) on average.
pub(crate) fn media_summary(catalog: &Catalog, item: &ItemDetail) -> Option<MediaSummary> {
    if !item.kind().playable() {
        let files: Vec<&MediaFile> =
            catalog.leaves(item.id()).iter().filter_map(|id| catalog.item(*id)?.files.first()).collect();
        if files.is_empty() {
            return None;
        }
        let rates: Vec<i64> = files.iter().filter_map(|f| f.bitrate).collect();
        return Some(MediaSummary {
            bitrate: (!rates.is_empty()).then(|| rates.iter().sum::<i64>() / rates.len() as i64),
            size: Some(files.iter().map(|f| f.size).sum()),
            width: files.iter().filter_map(|f| f.width).max(),
            height: files.iter().filter_map(|f| f.height).max(),
            video_codec: None,
            dynamic_range: None,
        });
    }
    let best = item.files.iter().max_by_key(|f| f.bitrate.unwrap_or(0))?;
    let video = best.streams.iter().find(|s| s.is("video"));
    Some(MediaSummary {
        bitrate: best.bitrate,
        size: Some(best.size),
        width: best.width,
        height: best.height,
        video_codec: best.video_codec.clone(),
        dynamic_range: video.and_then(|v| v.dynamic_range),
    })
}

/// The tracks a file starts with for the profile: its choice for the movie or
/// show, else its defaults. The audio is a pick only when the choice names
/// one; else each player gets its best track of that language.
pub(crate) fn with_choices(item: &ItemDetail, file: &MediaFile, profile: &Profile) -> MediaFile {
    let owner = if item.kind() == Kind::Episode { item.base.grandparent_id } else { Some(item.id()) };
    let choice = owner.and_then(|o| profile.stream_choices.get(&o));
    let audio: Vec<&MediaStream> = file.streams.iter().filter(|s| s.is("audio")).collect();
    let texts: Vec<&MediaStream> =
        file.streams.iter().filter(|s| s.is("subtitle") && s.text_based == Some(true)).collect();
    let defaults = &profile.track_defaults;
    // A choice without audio (subtitles picked alone) leaves it to the defaults.
    let picked = choice.and_then(|c| c.audio_index).and_then(|index| audio.iter().find(|s| s.index == index).copied());
    let chosen = picked
        .or_else(|| audio.iter().find(|s| s.language == defaults.audio_language).copied())
        .or_else(|| audio.iter().find(|s| s.default).copied());
    let mut out = file.clone();
    out.selected_audio = chosen.map(|s| s.index);
    if let Some(choice) = choice {
        out.audio_picked = picked.is_some();
        out.selected_subtitle = choice.subtitle_index.filter(|index| file.streams.iter().any(|s| s.index == *index));
        return out;
    }
    let audio_language = chosen.and_then(|s| s.language.clone());
    let wanted = defaults.subtitle_language.clone().or_else(|| audio_language.clone());
    let subtitle = match defaults.subtitle_mode.as_str() {
        "always" => texts.iter().find(|s| s.language == wanted),
        "foreign" if audio_language != wanted => texts.iter().find(|s| s.language == wanted),
        "forced" => texts.iter().find(|s| s.forced && s.language == audio_language),
        _ => None,
    };
    out.selected_subtitle = subtitle.map(|s| s.index);
    out
}

/// A collection's items, in its order.
pub(crate) fn collection_items<'a>(catalog: &'a Catalog, collection: &Collection) -> Vec<&'a ItemDetail> {
    let mut items: Vec<&ItemDetail> = match &collection.smart_filter {
        Some(filter) => filter_items(catalog, collection.library_id, filter, None),
        None => collection.item_ids.iter().filter_map(|id| catalog.item(*id)).collect(),
    };
    match collection.sort {
        "title" => items.sort_by_key(|i| i.base.sort_title.to_lowercase()),
        "release" => items.sort_by(|a, b| a.base.release_date.cmp(&b.base.release_date)),
        _ => {}
    }
    items
}

pub(crate) fn collection_summary(catalog: &Catalog, collection: &Collection) -> CollectionSummary {
    let id = collection.id;
    CollectionSummary {
        id,
        library_id: collection.library_id,
        title: collection.title.clone(),
        summary: collection.summary.clone(),
        automatic: collection.automatic,
        sort: collection.sort,
        smart_filter: collection.smart_filter.clone(),
        item_count: collection_items(catalog, collection).len() as i64,
        poster_url: format!("/api/collections/{id}/images/poster?v={}", collection.created_at),
        backdrop_url: format!("/api/collections/{id}/images/backdrop?v={}", collection.created_at),
        created_at: collection.created_at,
        updated_at: collection.created_at,
    }
}

pub(crate) fn collection_detail(
    catalog: &Catalog,
    collection: &Collection,
    profile: &Profile,
    opts: ListOpts,
) -> CollectionDetail {
    CollectionDetail {
        base: collection_summary(catalog, collection),
        items: collection_items(catalog, collection).into_iter().map(|i| summary(catalog, i, profile, opts)).collect(),
    }
}

/// What a smart filter finds among a library's top-level items, in the
/// compatibility API's syntax as far as the fixtures use it: `genre`,
/// `year`, `decade`, `unwatched=1` and `sort=<field>[:desc]` (title, year).
pub(crate) fn filter_items<'a>(
    catalog: &'a Catalog,
    library: i64,
    filter: &str,
    profile: Option<&Profile>,
) -> Vec<&'a ItemDetail> {
    let terms: Vec<(&str, &str)> = filter.split('&').filter_map(|t| t.split_once('=')).collect();
    let term = |name: &str| terms.iter().find(|(k, _)| *k == name).map(|(_, v)| *v);
    let mut items = catalog.top(library);
    if let Some(genre) = term("genre") {
        items.retain(|i| i.genres.iter().any(|g| g == genre));
    }
    if let Some(year) = term("year").and_then(|y| y.parse::<i64>().ok()) {
        items.retain(|i| i.base.year == Some(year));
    }
    if let Some(decade) = term("decade").and_then(|d| d.parse::<i64>().ok()) {
        items.retain(|i| i.base.year.unwrap_or(0) / 10 * 10 == decade);
    }
    if let (Some("1"), Some(profile)) = (term("unwatched"), profile) {
        items.retain(|i| !state(catalog, i, profile).watched);
    }
    if let Some(sort) = term("sort") {
        match sort.split(':').next() {
            Some("year") => items.sort_by_key(|i| i.base.year.unwrap_or(0)),
            _ => items.sort_by_key(|i| i.base.sort_title.to_lowercase()),
        }
        if sort.ends_with(":desc") {
            items.reverse();
        }
    }
    items.truncate(5_000);
    items
}

/// A playlist's entries: a smart one's are its filter's leaves, keyed by item.
pub(crate) fn playlist_entries(catalog: &Catalog, playlist: &Playlist, profile: &Profile) -> Vec<(i64, i64)> {
    let Some(filter) = &playlist.smart_filter else { return playlist.entries.clone() };
    let Some(library) = playlist.library_id else { return Vec::new() };
    filter_items(catalog, library, filter, Some(profile))
        .iter()
        .flat_map(|i| catalog.leaves(i.id()))
        .take(5_000)
        .map(|id| (id, id))
        .collect()
}

pub(crate) fn playlist_detail(
    catalog: &Catalog,
    playlist: &Playlist,
    profile: &Profile,
    opts: ListOpts,
) -> PlaylistDetail {
    let entries: Vec<PlaylistEntry> = playlist_entries(catalog, playlist, profile)
        .into_iter()
        .filter_map(|(entry, item)| {
            Some(PlaylistEntry { entry_id: entry, item: summary(catalog, catalog.item(item)?, profile, opts) })
        })
        .collect();
    let mut artwork: Vec<String> = Vec::new();
    for entry in &entries {
        // Episodes show their stills, the rest their posters.
        let picture = if entry.item.kind == Kind::Episode {
            catalog.item(entry.item.id).and_then(|i| i.base.thumb_url.clone())
        } else {
            entry.item.poster_url.clone()
        };
        if let Some(picture) = picture
            && !artwork.contains(&picture)
            && artwork.len() < 4
        {
            artwork.push(picture);
        }
    }
    PlaylistDetail {
        base: PlaylistSummary {
            id: playlist.id,
            title: playlist.title.clone(),
            kind: playlist.kind,
            item_count: entries.len() as i64,
            duration_ms: entries.iter().map(|e| e.item.duration_ms.unwrap_or(0)).sum(),
            artwork,
            smart_filter: playlist.smart_filter.clone(),
            library_id: playlist.library_id,
            created_at: playlist.created_at,
            updated_at: playlist.updated_at,
        },
        entries,
    }
}

/// What a playlist of `kind` may hold.
pub(crate) fn fits(kind: &str, item: &ItemDetail) -> bool {
    match kind {
        "video" => matches!(item.kind(), Kind::Movie | Kind::Episode | Kind::Extra),
        "audio" => item.kind() == Kind::Track,
        _ => false,
    }
}

/// GET /api/auth/me.
pub(crate) fn me(profile: &Profile, scope: String) -> Me {
    Me {
        id: profile.id,
        username: profile.username.to_string(),
        name: profile.name.to_string(),
        display_name: Some(profile.name.to_string()),
        is_admin: false,
        managed: profile.managed,
        home: true,
        has_pin: profile.pin.is_some(),
        avatar_url: profile.avatar_url(),
        avatar_color: Some(profile.avatar_color.to_string()),
        all_libraries: true,
        library_ids: Vec::new(),
        max_age: None,
        disabled: false,
        created_at: BASE_TIME - 500 * DAY,
        audio_language: profile.track_defaults.audio_language.clone(),
        subtitle_language: profile.track_defaults.subtitle_language.clone(),
        subtitle_mode: profile.track_defaults.subtitle_mode.clone(),
        has_password: profile.has_password,
        two_step: false,
        scope,
    }
}

pub(crate) fn history_entry(
    catalog: &Catalog,
    record: &HistoryRecord,
    profile: &Profile,
    opts: ListOpts,
) -> Option<HistoryEntry> {
    let item = catalog.item(record.item_id)?;
    Some(HistoryEntry {
        id: record.id,
        viewed_at: record.viewed_at,
        device: record.device.clone(),
        user: HistoryUser {
            id: profile.id,
            name: profile.name.to_string(),
            avatar_url: profile.avatar_url(),
            avatar_color: Some(profile.avatar_color.to_string()),
        },
        item: summary(catalog, item, profile, opts),
    })
}
