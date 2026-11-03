//! Library browsing: sections, listings with Plex filters, sorting and
//! paging, metadata, hierarchy, hubs, search and artwork.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use axum::Extension;
use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::response::Response;

use super::meta::{self, Detail, Extras, Sections, kind_for_number, kinds_of, plex_type, section_type, type_number};
use super::{Node, Page, Params, PlexAdmin, PlexError, PlexInfo, PlexResult, PlexUser};
use crate::auth::AuthUser;
use crate::catalog::{self, ItemRow, ME_CTE, VISIBLE, VISIBLE_ITEMS};
use crate::db::Db;
use crate::events::ServerEvent;
use crate::library::LibraryRow;
use crate::listing::{SectionQuery, bitrates_for, parse_sort, people_role, sort_rows};
use crate::people;
use crate::state::AppState;

/// Libraries the user can see (per-user library access).
pub(super) async fn visible_libraries(state: &AppState, user: &AuthUser) -> PlexResult<Vec<LibraryRow>> {
    Ok(crate::access::visible_libraries(&state.db, user).await?)
}

pub(super) async fn library_for(
    state: &AppState,
    user: &AuthUser,
    id: &str,
) -> PlexResult<(LibraryRow, Vec<LibraryRow>)> {
    let id: i64 = id.parse().map_err(|_| PlexError::NotFound)?;
    let libraries = visible_libraries(state, user).await?;
    let lib = libraries.iter().find(|l| l.id == id).cloned().ok_or(PlexError::NotFound)?;
    Ok((lib, libraries))
}

pub(super) fn base_container() -> Node {
    Node::container()
        .attr("allowSync", false)
        .attr("identifier", "com.plexapp.plugins.library")
        .attr("mediaTagPrefix", "/system/bundle/media/flags/")
        .attr("mediaTagVersion", 1)
}

/// A paged list of items.
pub(super) fn list_container(
    lib: Option<&LibraryRow>,
    info: &PlexInfo,
    nodes: Vec<Node>,
    total: usize,
    page: &Page,
) -> Node {
    let mut mc = base_container().attr("size", nodes.len()).attr("totalSize", total).attr("offset", page.start);
    if let Some(lib) = lib {
        mc = mc
            .attr("librarySectionID", lib.id)
            .attr("librarySectionTitle", &lib.name)
            .attr("librarySectionUUID", info.section_uuid(lib.id))
            .attr("title1", &lib.name);
    }
    mc.extend(nodes);
    mc
}

/// Item elements for rows loaded through the catalog.
pub(super) async fn item_nodes(
    state: &AppState,
    rows: &[ItemRow],
    sections: &Sections,
    detail: Detail,
) -> PlexResult<Vec<Node>> {
    let ids: Vec<i64> = rows.iter().map(|r| r.id).collect();
    let mut extras = meta::load_extras(&state.db, &ids).await?;
    if detail.full {
        add_people(&state.db, &ids, &mut extras).await?;
        for (id, collections) in meta::load_collections(&state.db, &ids).await? {
            extras.entry(id).or_default().collections = collections;
        }
        for (id, locked) in meta::load_locked(&state.db, &ids).await? {
            extras.entry(id).or_default().locked = locked;
        }
    }
    let leaves: Vec<i64> = rows.iter().filter(|r| meta::is_leaf(&r.kind)).map(|r| r.id).collect();
    let files =
        if detail.media && !leaves.is_empty() { meta::load_files(&state.db, &leaves).await? } else { HashMap::new() };
    if detail.full {
        for (item, item_files) in &files {
            let with_previews: Vec<i64> =
                item_files.iter().filter(|f| state.trickplay.index(f).is_some()).map(|f| f.id).collect();
            if !with_previews.is_empty() {
                extras.entry(*item).or_default().previews = with_previews;
            }
        }
    }
    if detail.markers {
        // The first version's: the one played unless the client picks another.
        let first: Vec<(i64, i64)> = files.iter().filter_map(|(item, f)| Some((*item, f.first()?.id))).collect();
        let file_ids: Vec<i64> = first.iter().map(|(_, file)| *file).collect();
        let mut markers = meta::load_markers(&state.db, &file_ids).await?;
        for (item, file) in first {
            if let Some(found) = markers.remove(&file) {
                extras.entry(item).or_default().markers = found;
            }
        }
    }
    Ok(rows
        .iter()
        .map(|r| {
            let files = files.get(&r.id).map(Vec::as_slice).unwrap_or(&[]);
            meta::item_node(r, extras.get(&r.id), files, sections, detail)
        })
        .collect())
}

/// Cast and crew of `ids` into their extras (seasons get their show's).
async fn add_people(db: &Db, ids: &[i64], extras: &mut HashMap<i64, Extras>) -> sqlx::Result<()> {
    for (id, credits) in people::for_items(db, ids).await? {
        extras.entry(id).or_default().people = credits;
    }
    Ok(())
}

pub(super) fn listing_detail(params: &Params) -> Detail {
    Detail { guids: params.flag("includeGuids"), full: false, media: true, markers: false }
}

// ------------------------------------------------------------------ sections

pub async fn root(_user: PlexUser) -> Node {
    base_container()
        .attr("size", 3)
        .attr("title1", "Plex Library")
        .attr("title2", "")
        .child(Node::new("Directory").attr("key", "sections").attr("title", "Library Sections"))
        .child(Node::new("Directory").attr("key", "recentlyAdded").attr("title", "Recently Added Content"))
        .child(Node::new("Directory").attr("key", "onDeck").attr("title", "On Deck Content"))
}

fn section_node(state: &AppState, info: &PlexInfo, lib: &LibraryRow) -> Node {
    let (agent, scanner) = match section_type(lib) {
        "movie" => ("tv.plex.agents.movie", "Plex Movie"),
        "show" => ("tv.plex.agents.series", "Plex TV Series"),
        _ => ("tv.plex.agents.music", "Plex Music"),
    };
    let changed = lib.scanned_at.unwrap_or(lib.created_at);
    let mut node = Node::new("Directory")
        .attr("allowSync", false)
        .attr("filters", true)
        .attr("refreshing", state.scanner.is_scanning(lib.id))
        .attr("key", lib.id.to_string())
        .attr("type", section_type(lib))
        .attr("title", &lib.name)
        .attr("agent", agent)
        .attr("scanner", scanner)
        .attr("language", "en-US")
        .attr("uuid", info.section_uuid(lib.id))
        .attr("updatedAt", changed)
        .attr("createdAt", lib.created_at)
        .opt("scannedAt", lib.scanned_at)
        .attr("content", true)
        .attr("directory", true)
        .attr("contentChangedAt", changed)
        .attr("hidden", 0);
    for (i, path) in lib.paths().iter().enumerate() {
        node.push(
            Node::new("Location")
                .attr("id", lib.id * 1000 + i as i64)
                .attr("path", path.to_string_lossy().into_owned()),
        );
    }
    node
}

pub async fn sections(
    State(state): State<AppState>,
    PlexUser(user): PlexUser,
    Extension(info): Extension<Arc<PlexInfo>>,
) -> PlexResult<Node> {
    let libraries = visible_libraries(&state, &user).await?;
    let mut mc = base_container().attr("size", libraries.len()).attr("title1", "Plex Library");
    for lib in &libraries {
        mc.push(section_node(&state, &info, lib));
    }
    Ok(mc)
}

pub async fn section(
    State(state): State<AppState>,
    PlexUser(user): PlexUser,
    Extension(info): Extension<Arc<PlexInfo>>,
    Path(id): Path<String>,
) -> PlexResult<Node> {
    let (lib, _) = library_for(&state, &user, &id).await?;
    let top = lib.kind().top_level_kind();
    let plural = match top {
        "movie" => "Movies",
        "show" => "Shows",
        _ => "Artists",
    };
    let directory = |key: String, title: &str| Node::new("Directory").attr("key", key).attr("title", title);
    let mut mc = base_container()
        .attr("content", "secondary")
        .attr("librarySectionID", lib.id)
        .attr("librarySectionUUID", info.section_uuid(lib.id))
        .attr("title1", &lib.name)
        .attr("viewGroup", "secondary");
    mc.push(directory("all".into(), &format!("All {plural}")));
    mc.push(directory("recentlyAdded".into(), "Recently Added"));
    if top != "artist" {
        mc.push(directory("onDeck".into(), "On Deck"));
    }
    for (filter, title) in [("genre", "By Genre"), ("year", "By Year"), ("decade", "By Decade")] {
        mc.push(directory(filter.into(), title).attr("secondary", true));
    }
    mc.push(
        directory(format!("search?type={}", type_number(top)), &format!("Search {plural}..."))
            .attr("prompt", format!("Search {plural}"))
            .attr("search", true),
    );
    let size = mc.children().len();
    mc.set("size", size);
    Ok(mc)
}

pub async fn refresh_all(
    State(state): State<AppState>,
    PlexAdmin(_admin): PlexAdmin,
    params: Params,
) -> PlexResult<Node> {
    state.scanner.enqueue_all(params.flag("force")).await?;
    Ok(Node::container().attr("size", 0))
}

pub async fn refresh(
    State(state): State<AppState>,
    PlexAdmin(admin): PlexAdmin,
    Path(id): Path<String>,
    params: Params,
) -> PlexResult<Node> {
    let (lib, _) = library_for(&state, &admin, &id).await?;
    state.scanner.enqueue(lib.id, params.flag("force"));
    Ok(Node::container().attr("size", 0))
}

pub async fn refresh_item(
    State(state): State<AppState>,
    PlexAdmin(admin): PlexAdmin,
    Path(id): Path<i64>,
) -> PlexResult<Node> {
    let row = catalog::item(&state.db, admin.id, id).await?.ok_or(PlexError::NotFound)?;
    if state.metadata.enabled() {
        let metadata = state.metadata.clone();
        let events = state.events.clone();
        tokio::spawn(async move {
            match metadata.refresh_item(id).await {
                Ok(()) => events.send(ServerEvent::LibraryChanged { library_id: row.library_id }),
                Err(err) => tracing::warn!("metadata refresh for item {id} failed: {err:#}"),
            }
        });
    }
    Ok(Node::container().attr("size", 0))
}

/// `/library/sections/{id}/all`
pub async fn all(
    State(state): State<AppState>,
    PlexUser(user): PlexUser,
    Extension(info): Extension<Arc<PlexInfo>>,
    Path(id): Path<String>,
    params: Params,
    page: Page,
) -> PlexResult<Node> {
    if params.get("type") == Some(super::lists::COLLECTION_TYPE) {
        return super::lists::section_collections(State(state), PlexUser(user), Extension(info), Path(id), page).await;
    }
    let (lib, libraries) = library_for(&state, &user, &id).await?;
    let kind = params.get("type").and_then(kind_for_number).unwrap_or(lib.kind().top_level_kind());
    let (rows, total) = if kinds_of(&lib).contains(&kind) {
        SectionQuery::new(lib.id, kind, &params).page(&state.db, user.id, &page).await?
    } else {
        (Vec::new(), 0)
    };
    let nodes = item_nodes(&state, &rows, &Sections::new(&libraries), listing_detail(&params)).await?;
    let mut mc = list_container(Some(&lib), &info, nodes, total, &page)
        .attr("title2", format!("All {}", plural(kind)))
        .attr("viewGroup", plex_type(kind));
    if params.flag("includeMeta") {
        mc.push(filter_meta(&lib));
    }
    Ok(mc)
}

/// `/library/sections/{id}/albums` (music sections).
pub async fn section_albums(
    state: State<AppState>,
    user: PlexUser,
    info: Extension<Arc<PlexInfo>>,
    id: Path<String>,
    Params(mut pairs): Params,
    page: Page,
) -> PlexResult<Node> {
    pairs.retain(|(k, _)| k != "type");
    pairs.push(("type".into(), type_number("album").to_string()));
    all(state, user, info, id, Params(pairs), page).await
}

/// `/library/all`: a search across every library.
pub async fn library_all(
    State(state): State<AppState>,
    PlexUser(user): PlexUser,
    Extension(info): Extension<Arc<PlexInfo>>,
    params: Params,
    page: Page,
) -> PlexResult<Node> {
    let libraries = visible_libraries(&state, &user).await?;
    let wanted = params.get("type").and_then(kind_for_number);
    // Each library's share of the page is among its first `start + size` items.
    let window = Page { start: 0, size: page.size.map(|size| size.saturating_add(page.start)) };
    let mut rows = Vec::new();
    let mut total = 0;
    for lib in &libraries {
        let kinds: Vec<&str> = match wanted {
            Some(kind) if kinds_of(lib).contains(&kind) => vec![kind],
            Some(_) => continue,
            None => vec![lib.kind().top_level_kind()],
        };
        for kind in kinds {
            let (found, count) = SectionQuery::new(lib.id, kind, &params).page(&state.db, user.id, &window).await?;
            rows.extend(found);
            total += count;
        }
    }
    let sort = parse_sort(params.get("sort"));
    let bitrates = bitrates_for(&state.db, &rows, &sort).await?;
    sort_rows(&mut rows, &sort, &bitrates);
    let rows: Vec<ItemRow> = rows.into_iter().skip(page.start).take(page.size.unwrap_or(usize::MAX)).collect();
    let nodes = item_nodes(&state, &rows, &Sections::new(&libraries), listing_detail(&params)).await?;
    Ok(list_container(None, &info, nodes, total, &page))
}

fn plural(kind: &str) -> &'static str {
    match kind {
        "movie" => "Movies",
        "show" => "Shows",
        "season" => "Seasons",
        "episode" => "Episodes",
        "artist" => "Artists",
        "album" => "Albums",
        _ => "Tracks",
    }
}

/// `Meta` describing the filters, sorts and fields this API understands
/// (python-plexapi validates searches against it).
fn filter_meta(lib: &LibraryRow) -> Node {
    let mut meta = Node::new("Meta");
    for (i, kind) in kinds_of(lib).iter().enumerate() {
        let n = type_number(kind);
        let mut t = Node::new("Type")
            .attr("key", format!("/library/sections/{}/all?type={n}", lib.id))
            .attr("type", plex_type(kind))
            .attr("title", plural(kind))
            .attr("active", i == 0);
        let mut filters = vec![("genre", "Genre"), ("year", "Year"), ("decade", "Decade"), ("label", "Labels")];
        if matches!(*kind, "movie" | "show") {
            filters.push(("contentRating", "Content Rating"));
            filters.push(("studio", "Studio"));
            filters.push(("actor", "Actor"));
        }
        if *kind == "movie" {
            filters.extend([("director", "Director"), ("writer", "Writer"), ("producer", "Producer")]);
        }
        for (filter, title) in filters {
            t.push(
                Node::new("Filter")
                    .attr("filter", filter)
                    .attr("filterType", if filter == "year" || filter == "decade" { "integer" } else { "string" })
                    .attr("key", format!("/library/sections/{}/{filter}?type={n}", lib.id))
                    .attr("title", title)
                    .attr("type", "filter"),
            );
        }
        if matches!(*kind, "movie" | "episode" | "track" | "show") {
            t.push(
                Node::new("Filter")
                    .attr("filter", "unwatched")
                    .attr("filterType", "boolean")
                    .attr("key", "unwatched")
                    .attr("title", "Unplayed")
                    .attr("type", "filter"),
            );
        }
        for (key, title, dir) in [
            ("titleSort", "Title", "asc"),
            ("addedAt", "Date Added", "desc"),
            ("originallyAvailableAt", "Release Date", "desc"),
            ("year", "Year", "desc"),
            ("rating", "Critic Rating", "desc"),
            ("audienceRating", "Audience Rating", "desc"),
            ("lastViewedAt", "Last Played", "desc"),
            ("viewCount", "Plays", "desc"),
            ("duration", "Duration", "desc"),
        ] {
            t.push(
                Node::new("Sort")
                    .attr("defaultDirection", dir)
                    .attr("descKey", format!("{key}:desc"))
                    .attr("firstCharacterKey", format!("/library/sections/{}/firstCharacter", lib.id))
                    .attr("key", key)
                    .attr("title", title),
            );
        }
        let prefix = if *kind == "movie" { String::new() } else { format!("{}.", plex_type(kind)) };
        for (field, ty, title) in [
            ("title", "string", "Title"),
            ("studio", "string", "Studio"),
            ("year", "integer", "Year"),
            ("decade", "integer", "Decade"),
            ("genre", "tag", "Genre"),
            ("label", "tag", "Labels"),
            ("contentRating", "tag", "Content Rating"),
            ("unwatched", "boolean", "Unplayed"),
            ("inProgress", "boolean", "In Progress"),
            ("viewCount", "integer", "Plays"),
            ("addedAt", "date", "Date Added"),
            ("lastViewedAt", "date", "Last Played"),
            ("originallyAvailableAt", "date", "Release Date"),
        ] {
            t.push(Node::new("Field").attr("key", format!("{prefix}{field}")).attr("title", title).attr("type", ty));
        }
        meta.push(t);
    }
    let operators: &[(&str, &[(&str, &str)])] = &[
        ("tag", &[("=", "is"), ("!=", "is not")]),
        ("integer", &[("=", "is"), ("!=", "is not"), (">>=", "is greater than"), ("<<=", "is less than")]),
        (
            "string",
            &[
                ("=", "contains"),
                ("!=", "does not contain"),
                ("==", "is"),
                ("!==", "is not"),
                ("<=", "begins with"),
                (">=", "ends with"),
            ],
        ),
        ("boolean", &[("=", "is true"), ("!=", "is false")]),
        ("date", &[("<<=", "is before"), (">>=", "is after")]),
        ("resolution", &[("=", "is")]),
    ];
    for (ty, ops) in operators {
        let mut field_type = Node::new("FieldType").attr("type", *ty);
        for (key, title) in *ops {
            field_type.push(Node::new("Operator").attr("key", *key).attr("title", *title));
        }
        meta.push(field_type);
    }
    meta
}

/// `/library/sections/{id}/{genre|year|decade|contentRating|studio}`
pub async fn filter_choices(
    State(state): State<AppState>,
    PlexUser(user): PlexUser,
    Extension(info): Extension<Arc<PlexInfo>>,
    Path((id, filter)): Path<(String, String)>,
    params: Params,
) -> PlexResult<Node> {
    let (lib, _) = library_for(&state, &user, &id).await?;
    let kind = params.get("type").and_then(kind_for_number).unwrap_or(lib.kind().top_level_kind());
    let (genres, years) = catalog::library_filters(&state.db, user.id, lib.id, kind).await?;
    let values: Vec<String> = match filter.as_str() {
        "genre" => genres,
        "year" => years.iter().map(i64::to_string).collect(),
        "decade" => {
            let mut decades: Vec<i64> = years.iter().map(|y| y / 10 * 10).collect();
            decades.dedup();
            decades.iter().map(i64::to_string).collect()
        }
        field if people_role(field).is_some() => {
            // Keyed by person id, so the filter is exact. People only appear
            // through titles the user may see.
            let role = people_role(field).map(people::Role::as_str).unwrap_or_default();
            let credited: Vec<(i64, String)> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
                "{} SELECT DISTINCT pe.id, pe.name FROM item_people ip
                 JOIN people pe ON pe.id = ip.person_id
                 JOIN items i ON i.id = ip.item_id
                 LEFT JOIN items p ON p.id = i.parent_id
                 LEFT JOIN items gp ON gp.id = i.grandparent_id
                 WHERE {} AND i.library_id = ? AND i.kind = ? AND ip.role = ?
                 ORDER BY pe.name COLLATE NOCASE",
                catalog::ME_CTE,
                catalog::VISIBLE,
            )))
            .bind(user.id)
            .bind(lib.id)
            .bind(kind)
            .bind(role)
            .fetch_all(&state.db.read)
            .await?;
            let mut mc = base_container()
                .attr("size", credited.len())
                .attr("librarySectionID", lib.id)
                .attr("librarySectionUUID", info.section_uuid(lib.id))
                .attr("title1", &lib.name)
                .attr("viewGroup", "secondary");
            let n = type_number(kind);
            for (id, name) in credited {
                mc.push(
                    Node::new("Directory")
                        .attr("fastKey", format!("/library/sections/{}/all?type={n}&{filter}={id}", lib.id))
                        .attr("key", id)
                        .attr("title", name)
                        .attr("type", filter.clone()),
                );
            }
            return Ok(mc);
        }
        "label" => {
            sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
                "{ME_CTE} SELECT DISTINCT l.label FROM {VISIBLE_ITEMS} JOIN item_labels l ON l.item_id = i.id
                 WHERE {VISIBLE} AND i.library_id = ? AND i.kind = ? ORDER BY l.label"
            )))
            .bind(user.id)
            .bind(lib.id)
            .bind(kind)
            .fetch_all(&state.db.read)
            .await?
        }
        "contentRating" | "studio" => {
            let column = if filter == "studio" { "studio" } else { "content_rating" };
            sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
                "{ME_CTE} SELECT DISTINCT i.{column} FROM {VISIBLE_ITEMS}
                 WHERE {VISIBLE} AND i.library_id = ? AND i.kind = ? AND i.{column} IS NOT NULL
                 ORDER BY i.{column} COLLATE NOCASE"
            )))
            .bind(user.id)
            .bind(lib.id)
            .bind(kind)
            .fetch_all(&state.db.read)
            .await?
        }
        _ => return Err(PlexError::NotFound),
    };
    let mut mc = base_container()
        .attr("size", values.len())
        .attr("librarySectionID", lib.id)
        .attr("librarySectionUUID", info.section_uuid(lib.id))
        .attr("title1", &lib.name)
        .attr("viewGroup", "secondary");
    let n = type_number(kind);
    for value in values {
        mc.push(
            Node::new("Directory")
                .attr("fastKey", format!("/library/sections/{}/all?type={n}&{filter}={}", lib.id, encode(&value)))
                .attr("key", value.clone())
                .attr("title", value)
                .attr("type", filter.clone()),
        );
    }
    Ok(mc)
}

/// Percent-encodes a query value.
fn encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(byte as char),
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

pub async fn first_character(
    State(state): State<AppState>,
    PlexUser(user): PlexUser,
    Path(id): Path<String>,
    params: Params,
) -> PlexResult<Node> {
    let (lib, _) = library_for(&state, &user, &id).await?;
    let kind = params.get("type").and_then(kind_for_number).unwrap_or(lib.kind().top_level_kind());
    // Counted by exact first character in SQL, then merged here into what
    // Plex lists (SQLite's UPPER() only knows ASCII).
    let firsts: Vec<(String, i64)> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{ME_CTE} SELECT SUBSTR(i.sort_title, 1, 1), COUNT(*) FROM {VISIBLE_ITEMS}
         WHERE {VISIBLE} AND i.library_id = ? AND i.kind = ? GROUP BY 1"
    )))
    .bind(user.id)
    .bind(lib.id)
    .bind(kind)
    .fetch_all(&state.db.read)
    .await?;
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for (first, count) in firsts {
        let first = first.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_default();
        let key = if first.chars().all(char::is_alphabetic) && !first.is_empty() { first } else { "#".into() };
        *counts.entry(key).or_default() += usize::try_from(count).unwrap_or(0);
    }
    let mut mc = base_container().attr("size", counts.len()).attr("title1", &lib.name);
    for (letter, size) in counts {
        mc.push(Node::new("Directory").attr("size", size).attr("key", letter.clone()).attr("title", letter));
    }
    Ok(mc)
}

// ------------------------------------------------------------------ metadata

pub async fn metadata(
    State(state): State<AppState>,
    PlexUser(user): PlexUser,
    Extension(info): Extension<Arc<PlexInfo>>,
    Path(ids): Path<String>,
    params: Params,
) -> PlexResult<Node> {
    let ids: Vec<i64> = ids.split(',').filter_map(|i| i.trim().parse().ok()).take(500).collect();
    let rows = catalog::items_by_ids(&state.db, user.id, &ids).await?;
    let libraries = visible_libraries(&state, &user).await?;
    let rows: Vec<ItemRow> = rows.into_iter().filter(|r| libraries.iter().any(|l| l.id == r.library_id)).collect();
    let Some(first) = rows.first() else { return Err(PlexError::NotFound) };
    let lib = libraries.iter().find(|l| l.id == first.library_id);
    let sections = Sections::new(&libraries);
    let detail = Detail { guids: true, full: true, media: true, markers: params.flag("includeMarkers") };
    let mut nodes = item_nodes(&state, &rows, &sections, detail).await?;
    if params.flag("includeOnDeck") {
        for (node, row) in nodes.iter_mut().zip(&rows) {
            if let Some(next) = catalog::on_deck(&state.db, user.id, row).await?
                && let Some(next_row) = catalog::item(&state.db, user.id, next).await?
            {
                let detail = Detail { guids: true, full: false, media: true, markers: false };
                let next_node = item_nodes(&state, &[next_row], &sections, detail).await?;
                let mut on_deck = Node::new("OnDeck");
                on_deck.extend(next_node);
                node.push(on_deck);
            }
        }
    }
    let total = nodes.len();
    Ok(list_container(lib, &info, nodes, total, &Page::default()))
}

pub async fn children(
    State(state): State<AppState>,
    PlexUser(user): PlexUser,
    Extension(info): Extension<Arc<PlexInfo>>,
    Path(id): Path<i64>,
    params: Params,
    page: Page,
) -> PlexResult<Node> {
    let parent = catalog::item(&state.db, user.id, id).await?.ok_or(PlexError::NotFound)?;
    let libraries = visible_libraries(&state, &user).await?;
    let lib = libraries.iter().find(|l| l.id == parent.library_id).ok_or(PlexError::NotFound)?;
    let mut rows = catalog::children(&state.db, user.id, id).await?;
    if let Some(index) = params.parse::<i64>("index") {
        rows.retain(|r| r.idx == Some(index));
    }
    let (rows, total) = page.apply(rows);
    let view_group = rows.first().map(|r| plex_type(&r.kind)).unwrap_or("secondary");
    let nodes = item_nodes(&state, &rows, &Sections::new(&libraries), listing_detail(&params)).await?;
    Ok(hierarchy_container(lib, &info, &parent, nodes, total, &page).attr("viewGroup", view_group))
}

/// `/library/metadata/{id}/extras`: trailers and other bonus material.
pub async fn extras(
    State(state): State<AppState>,
    PlexUser(user): PlexUser,
    Extension(info): Extension<Arc<PlexInfo>>,
    Path(id): Path<i64>,
    params: Params,
    page: Page,
) -> PlexResult<Node> {
    let parent = catalog::item(&state.db, user.id, id).await?.ok_or(PlexError::NotFound)?;
    let libraries = visible_libraries(&state, &user).await?;
    let rows = catalog::extras(&state.db, user.id, parent.id).await?;
    let (rows, total) = page.apply(rows);
    let nodes = item_nodes(&state, &rows, &Sections::new(&libraries), listing_detail(&params)).await?;
    Ok(list_container(None, &info, nodes, total, &page))
}

fn hierarchy_container(
    lib: &LibraryRow,
    info: &PlexInfo,
    parent: &ItemRow,
    nodes: Vec<Node>,
    total: usize,
    page: &Page,
) -> Node {
    list_container(Some(lib), info, nodes, total, page)
        .attr("key", parent.id.to_string())
        .attr("parentRatingKey", parent.id.to_string())
        .attr("parentTitle", &parent.title)
        .opt("parentIndex", parent.idx)
        .opt("parentYear", parent.year)
        .opt("summary", parent.summary.as_deref())
        .attr("title2", &parent.title)
        .attr("nocache", true)
}

/// Every playable item below a show, season, artist or album.
pub async fn all_leaves(
    State(state): State<AppState>,
    PlexUser(user): PlexUser,
    Extension(info): Extension<Arc<PlexInfo>>,
    Path(id): Path<i64>,
    params: Params,
    page: Page,
) -> PlexResult<Node> {
    let parent = catalog::item(&state.db, user.id, id).await?.ok_or(PlexError::NotFound)?;
    let libraries = visible_libraries(&state, &user).await?;
    let lib = libraries.iter().find(|l| l.id == parent.library_id).ok_or(PlexError::NotFound)?;
    let mut rows = Vec::new();
    if meta::is_leaf(&parent.kind) {
        rows.push(parent.clone());
    } else {
        for child in catalog::children(&state.db, user.id, id).await? {
            if meta::is_leaf(&child.kind) {
                rows.push(child);
            } else {
                rows.extend(catalog::children(&state.db, user.id, child.id).await?);
            }
        }
    }
    let (rows, total) = page.apply(rows);
    let view_group = if matches!(parent.kind.as_str(), "artist" | "album") { "track" } else { "episode" };
    let nodes = item_nodes(&state, &rows, &Sections::new(&libraries), listing_detail(&params)).await?;
    Ok(hierarchy_container(lib, &info, &parent, nodes, total, &page).attr("viewGroup", view_group))
}

// ------------------------------------------------------------------ on deck, recently added, hubs

async fn continue_rows(
    state: &AppState,
    user: &AuthUser,
    libraries: &[LibraryRow],
    library_id: Option<i64>,
    limit: i64,
) -> PlexResult<Vec<ItemRow>> {
    let rows = catalog::continue_watching(&state.db, user.id, library_id, limit).await?;
    Ok(rows.into_iter().filter(|r| libraries.iter().any(|l| l.id == r.library_id)).collect())
}

pub async fn on_deck(
    State(state): State<AppState>,
    PlexUser(user): PlexUser,
    Extension(info): Extension<Arc<PlexInfo>>,
    params: Params,
    page: Page,
) -> PlexResult<Node> {
    let libraries = visible_libraries(&state, &user).await?;
    let rows = continue_rows(&state, &user, &libraries, None, 50).await?;
    let (rows, total) = page.apply(rows);
    let nodes = item_nodes(&state, &rows, &Sections::new(&libraries), listing_detail(&params)).await?;
    Ok(list_container(None, &info, nodes, total, &page).attr("title1", "On Deck"))
}

pub async fn section_on_deck(
    State(state): State<AppState>,
    PlexUser(user): PlexUser,
    Extension(info): Extension<Arc<PlexInfo>>,
    Path(id): Path<String>,
    params: Params,
    page: Page,
) -> PlexResult<Node> {
    let (lib, libraries) = library_for(&state, &user, &id).await?;
    let rows = continue_rows(&state, &user, &libraries, Some(lib.id), 50).await?;
    let (rows, total) = page.apply(rows);
    let nodes = item_nodes(&state, &rows, &Sections::new(&libraries), listing_detail(&params)).await?;
    Ok(list_container(Some(&lib), &info, nodes, total, &page))
}

pub async fn continue_watching(
    state: State<AppState>,
    user: PlexUser,
    info: Extension<Arc<PlexInfo>>,
    params: Params,
    page: Page,
) -> PlexResult<Node> {
    on_deck(state, user, info, params, page).await
}

pub async fn section_continue_watching(
    state: State<AppState>,
    user: PlexUser,
    info: Extension<Arc<PlexInfo>>,
    id: Path<String>,
    params: Params,
    page: Page,
) -> PlexResult<Node> {
    section_on_deck(state, user, info, id, params, page).await
}

async fn recent_rows(state: &AppState, user: &AuthUser, lib: &LibraryRow, limit: i64) -> PlexResult<Vec<ItemRow>> {
    Ok(catalog::recently_added(&state.db, user.id, lib.id, lib.kind().as_str(), limit).await?)
}

pub async fn recently_added(
    State(state): State<AppState>,
    PlexUser(user): PlexUser,
    Extension(info): Extension<Arc<PlexInfo>>,
    params: Params,
    page: Page,
) -> PlexResult<Node> {
    let libraries = visible_libraries(&state, &user).await?;
    let mut rows = Vec::new();
    for lib in &libraries {
        rows.extend(recent_rows(&state, &user, lib, 50).await?);
    }
    rows.sort_by_key(|r| std::cmp::Reverse(r.added_at));
    let (rows, total) = page.apply(rows);
    let nodes = item_nodes(&state, &rows, &Sections::new(&libraries), listing_detail(&params)).await?;
    Ok(list_container(None, &info, nodes, total, &page).attr("title1", "Recently Added"))
}

/// `/hubs/home/recentlyAdded?type=4&sectionID=2`: newest items of one type.
pub async fn home_recently_added(
    State(state): State<AppState>,
    PlexUser(user): PlexUser,
    Extension(info): Extension<Arc<PlexInfo>>,
    params: Params,
    page: Page,
) -> PlexResult<Node> {
    let libraries = visible_libraries(&state, &user).await?;
    let section = params.parse::<i64>("sectionID");
    let wanted = params.get("type").and_then(kind_for_number);
    let newest = Params(vec![("sort".into(), "addedAt:desc".into()), ("limit".into(), "50".into())]);
    let mut rows = Vec::new();
    for lib in libraries.iter().filter(|l| section.is_none_or(|s| s == l.id)) {
        match wanted {
            Some(kind) if kinds_of(lib).contains(&kind) => {
                let query = SectionQuery::new(lib.id, kind, &newest);
                rows.extend(query.page(&state.db, user.id, &Page::default()).await?.0);
            }
            Some(_) => {}
            None => rows.extend(recent_rows(&state, &user, lib, 50).await?),
        }
    }
    rows.sort_by_key(|r| std::cmp::Reverse(r.added_at));
    let (rows, total) = page.apply(rows);
    let nodes = item_nodes(&state, &rows, &Sections::new(&libraries), listing_detail(&params)).await?;
    Ok(list_container(None, &info, nodes, total, &page).attr("title1", "Recently Added"))
}

pub async fn section_recently_added(
    State(state): State<AppState>,
    PlexUser(user): PlexUser,
    Extension(info): Extension<Arc<PlexInfo>>,
    Path(id): Path<String>,
    params: Params,
    page: Page,
) -> PlexResult<Node> {
    let (lib, libraries) = library_for(&state, &user, &id).await?;
    let rows = recent_rows(&state, &user, &lib, 50).await?;
    let (rows, total) = page.apply(rows);
    let nodes = item_nodes(&state, &rows, &Sections::new(&libraries), listing_detail(&params)).await?;
    Ok(list_container(Some(&lib), &info, nodes, total, &page))
}

fn hub(identifier: String, title: String, hub_type: &str, key: Option<String>, items: Vec<Node>) -> Node {
    let ids: Vec<String> = items
        .iter()
        .filter_map(|n| match n.get("ratingKey") {
            Some(super::xml::Value::Str(id)) => Some(id.clone()),
            _ => None,
        })
        .collect();
    let mut node = Node::new("Hub")
        .attr("hubKey", format!("/library/metadata/{}", ids.join(",")))
        .opt("key", key)
        .attr("title", title)
        .attr("type", hub_type)
        .attr("hubIdentifier", identifier.clone())
        .attr("context", format!("hub.{identifier}"))
        .attr("size", items.len())
        .attr("more", false)
        .attr("style", "shelf")
        .attr("promoted", true);
    node.extend(items);
    node
}

/// The items like `id` that pley's own item page shows ("More Like This"),
/// for an item the user may see.
async fn like_this(
    state: &AppState,
    user: &AuthUser,
    id: &str,
    params: &Params,
) -> PlexResult<(Vec<ItemRow>, Vec<LibraryRow>)> {
    let id: i64 = id.parse().map_err(|_| PlexError::NotFound)?;
    let item = catalog::item(&state.db, user.id, id).await?.ok_or(PlexError::NotFound)?;
    let libraries = visible_libraries(state, user).await?;
    if !libraries.iter().any(|l| l.id == item.library_id) {
        return Err(PlexError::NotFound);
    }
    let count = params.parse::<usize>("count").unwrap_or(20).clamp(1, 50);
    Ok((crate::related::related(&state.db, user.id, id, count).await?, libraries))
}

/// `/library/metadata/{id}/related`: "More Like This" as a hub, which is
/// what Plex clients show under an item.
pub async fn related(
    State(state): State<AppState>,
    PlexUser(user): PlexUser,
    Path(id): Path<String>,
    params: Params,
) -> PlexResult<Node> {
    let (rows, libraries) = like_this(&state, &user, &id, &params).await?;
    let mut mc = base_container();
    if !rows.is_empty() {
        let nodes = item_nodes(&state, &rows, &Sections::new(&libraries), listing_detail(&params)).await?;
        let kind = hub_type(&rows);
        let identifier = format!("{}.similar", if kind == "show" { "tv" } else { kind });
        mc.push(hub(identifier, "More Like This".into(), kind, None, nodes));
    }
    let size = usize::from(!rows.is_empty());
    Ok(mc.attr("size", size))
}

/// `/library/metadata/{id}/similar`: the same items, as a plain list.
pub async fn similar(
    State(state): State<AppState>,
    PlexUser(user): PlexUser,
    Extension(info): Extension<Arc<PlexInfo>>,
    Path(id): Path<String>,
    params: Params,
) -> PlexResult<Node> {
    let (rows, libraries) = like_this(&state, &user, &id, &params).await?;
    let nodes = item_nodes(&state, &rows, &Sections::new(&libraries), listing_detail(&params)).await?;
    let total = nodes.len();
    Ok(list_container(None, &info, nodes, total, &Page::default()))
}

fn hub_type(rows: &[ItemRow]) -> &'static str {
    match rows.first() {
        Some(first) if rows.iter().all(|r| r.kind == first.kind) => plex_type(&first.kind),
        _ => "mixed",
    }
}

pub async fn hubs(State(state): State<AppState>, PlexUser(user): PlexUser, params: Params) -> PlexResult<Node> {
    let count = params.parse::<i64>("count").unwrap_or(12).clamp(1, 100);
    let mut libraries = visible_libraries(&state, &user).await?;
    if let Some(ids) = params.get("contentDirectoryID") {
        let wanted: Vec<i64> = ids.split(',').filter_map(|i| i.trim().parse().ok()).collect();
        libraries.retain(|l| wanted.contains(&l.id));
    }
    let sections = Sections::new(&libraries);
    let detail = listing_detail(&params);
    let mut hubs = Vec::new();
    let rows = continue_rows(&state, &user, &libraries, None, count).await?;
    if !rows.is_empty() {
        let t = hub_type(&rows);
        let items = item_nodes(&state, &rows, &sections, detail).await?;
        hubs.push(hub(
            "home.continue".into(),
            "Continue Watching".into(),
            t,
            Some("/hubs/continueWatching/items".into()),
            items,
        ));
    }
    for lib in &libraries {
        let rows = recent_rows(&state, &user, lib, count).await?;
        if rows.is_empty() {
            continue;
        }
        let area = match section_type(lib) {
            "movie" => "movies",
            "show" => "television",
            _ => "music",
        };
        let t = hub_type(&rows);
        let items = item_nodes(&state, &rows, &sections, detail).await?;
        hubs.push(hub(
            format!("home.{area}.recent"),
            format!("Recently Added in {}", lib.name),
            t,
            Some(format!("/library/sections/{}/recentlyAdded", lib.id)),
            items,
        ));
    }
    if let Some(identifiers) = params.get("identifier") {
        let wanted: Vec<&str> = identifiers.split(',').collect();
        hubs.retain(
            |h| matches!(h.get("hubIdentifier"), Some(super::xml::Value::Str(id)) if wanted.contains(&id.as_str())),
        );
    }
    let mut mc = Node::container()
        .attr("size", hubs.len())
        .attr("allowSync", false)
        .attr("identifier", "com.plexapp.plugins.library");
    mc.extend(hubs);
    Ok(mc)
}

pub async fn section_hubs(
    State(state): State<AppState>,
    PlexUser(user): PlexUser,
    Path(id): Path<String>,
    params: Params,
) -> PlexResult<Node> {
    let (lib, libraries) = library_for(&state, &user, &id).await?;
    let count = params.parse::<i64>("count").unwrap_or(12).clamp(1, 100);
    let sections = Sections::new(&libraries);
    let detail = listing_detail(&params);
    let prefix = match section_type(&lib) {
        "movie" => "movie",
        "show" => "tv",
        _ => "music",
    };
    let top = lib.kind().top_level_kind();
    let mut candidates: Vec<(String, String, Option<String>, Vec<ItemRow>)> = Vec::new();
    if top != "artist" {
        let rows = continue_rows(&state, &user, &libraries, Some(lib.id), count).await?;
        candidates.push((format!("{prefix}.inprogress"), "Continue Watching".into(), None, rows));
    }
    candidates.push((
        format!("{prefix}.recentlyadded"),
        "Recently Added".into(),
        Some(format!("/library/sections/{}/recentlyAdded", lib.id)),
        recent_rows(&state, &user, &lib, count).await?,
    ));
    if top != "artist" {
        let rows = catalog::recently_released(&state.db, user.id, lib.id, top, count).await?;
        candidates.push((format!("{prefix}.recentlyreleased"), "Recently Released".into(), None, rows));
        let rows = catalog::top_rated(&state.db, user.id, lib.id, top, count).await?;
        candidates.push((format!("{prefix}.toprated"), "Top Rated".into(), None, rows));
    }
    let mut mc = Node::container()
        .attr("allowSync", false)
        .attr("librarySectionID", lib.id)
        .attr("librarySectionTitle", &lib.name);
    let mut size = 0;
    for (identifier, title, key, rows) in candidates {
        if rows.is_empty() {
            continue;
        }
        let t = hub_type(&rows);
        let items = item_nodes(&state, &rows, &sections, detail).await?;
        mc.push(hub(identifier, title, t, key, items));
        size += 1;
    }
    mc.set("size", size);
    Ok(mc)
}

// ------------------------------------------------------------------ search

async fn search_rows(
    state: &AppState,
    user: &AuthUser,
    libraries: &[LibraryRow],
    query: &str,
    section: Option<i64>,
) -> PlexResult<Vec<ItemRow>> {
    let rows = catalog::search(&state.db, user.id, query, 200).await?;
    Ok(rows
        .into_iter()
        .filter(|r| libraries.iter().any(|l| l.id == r.library_id) && section.is_none_or(|s| s == r.library_id))
        .collect())
}

pub async fn hub_search(State(state): State<AppState>, PlexUser(user): PlexUser, params: Params) -> PlexResult<Node> {
    let query = params.get("query").unwrap_or_default().trim().to_owned();
    let limit = params.parse::<usize>("limit").unwrap_or(10).clamp(1, 100);
    let libraries = visible_libraries(&state, &user).await?;
    let section = params.parse::<i64>("sectionId");
    let rows = search_rows(&state, &user, &libraries, &query, section).await?;
    let sections = Sections::new(&libraries);
    let mut mc = Node::container().attr("allowSync", false).attr("identifier", "com.plexapp.plugins.library");
    let mut size = 0;
    for kind in ["movie", "show", "season", "episode", "artist", "album", "track"] {
        let group: Vec<ItemRow> = rows.iter().filter(|r| r.kind == kind).take(limit).cloned().collect();
        if group.is_empty() {
            continue;
        }
        let items =
            item_nodes(&state, &group, &sections, Detail { guids: true, full: false, media: true, markers: false })
                .await?;
        mc.push(hub(kind.into(), plural(kind).into(), kind, None, items));
        size += 1;
    }
    mc.set("size", size);
    Ok(mc)
}

pub async fn search(
    State(state): State<AppState>,
    PlexUser(user): PlexUser,
    Extension(info): Extension<Arc<PlexInfo>>,
    params: Params,
    page: Page,
) -> PlexResult<Node> {
    let libraries = visible_libraries(&state, &user).await?;
    let query = params.get("query").unwrap_or_default().trim().to_owned();
    let mut rows = search_rows(&state, &user, &libraries, &query, params.parse("sectionId")).await?;
    if let Some(kind) = params.get("type").and_then(kind_for_number) {
        rows.retain(|r| r.kind == kind);
    }
    let (rows, total) = page.apply(rows);
    let nodes = item_nodes(&state, &rows, &Sections::new(&libraries), listing_detail(&params)).await?;
    Ok(list_container(None, &info, nodes, total, &page))
}

pub async fn section_search(
    State(state): State<AppState>,
    PlexUser(user): PlexUser,
    Extension(info): Extension<Arc<PlexInfo>>,
    Path(id): Path<String>,
    params: Params,
    page: Page,
) -> PlexResult<Node> {
    let (lib, libraries) = library_for(&state, &user, &id).await?;
    let query = params.get("query").unwrap_or_default().trim().to_owned();
    let mut rows = search_rows(&state, &user, &libraries, &query, Some(lib.id)).await?;
    if let Some(kind) = params.get("type").and_then(kind_for_number) {
        rows.retain(|r| r.kind == kind);
    }
    let (rows, total) = page.apply(rows);
    let nodes = item_nodes(&state, &rows, &Sections::new(&libraries), listing_detail(&params)).await?;
    Ok(list_container(Some(&lib), &info, nodes, total, &page))
}

// ------------------------------------------------------------------ artwork

/// Serves Plex image URLs (`/library/metadata/{id}/thumb/{version}`) with
/// pley's artwork handler: posters, backgrounds and episode stills.
pub(super) async fn serve_image(
    state: AppState,
    user: AuthUser,
    id: i64,
    image: &str,
    version: Option<String>,
    width: Option<u32>,
    headers: HeaderMap,
) -> PlexResult<Response> {
    let row = catalog::item(&state.db, user.id, id).await?.ok_or(PlexError::NotFound)?;
    let kind = match image {
        "thumb" if row.kind == "episode" => "thumb",
        "thumb" => "poster",
        "art" => "backdrop",
        _ => return Err(PlexError::NotFound),
    };
    let mut query = HashMap::new();
    if let Some(version) = version {
        query.insert("v".to_owned(), version);
    }
    if let Some(width) = width {
        query.insert("w".to_owned(), width.to_string());
    }
    crate::api::items::image(State(state), user, Path((id, kind.to_owned())), Query(query), headers)
        .await
        .map_err(PlexError::from)
}

pub async fn image(
    State(state): State<AppState>,
    PlexUser(user): PlexUser,
    Path((id, image)): Path<(i64, String)>,
    params: Params,
    headers: HeaderMap,
) -> PlexResult<Response> {
    serve_image(state, user, id, &image, None, params.parse("width"), headers).await
}

pub async fn image_version(
    State(state): State<AppState>,
    PlexUser(user): PlexUser,
    Path((id, image, version)): Path<(i64, String, String)>,
    params: Params,
    headers: HeaderMap,
) -> PlexResult<Response> {
    serve_image(state, user, id, &image, Some(version), params.parse("width"), headers).await
}

/// `/photo/:/transcode?url=/library/metadata/1/thumb/123&width=300&height=450`
pub async fn photo_transcode(
    State(state): State<AppState>,
    PlexUser(user): PlexUser,
    params: Params,
    headers: HeaderMap,
) -> PlexResult<Response> {
    let url = params.get("url").ok_or(PlexError::BadRequest("url is required".into()))?;
    if let Some(response) = super::lists::photo_transcode(&state, &user, url, &params, &headers).await? {
        return Ok(response);
    }
    // Absolute URLs pointing back at this server are fine too.
    let path = match url.find("/library/metadata/") {
        Some(start) => &url[start..],
        None => return Err(PlexError::NotFound),
    };
    let path = path.split(['?', '#']).next().unwrap_or_default();
    let mut segments = path.trim_start_matches("/library/metadata/").split('/');
    let id: i64 = segments.next().and_then(|s| s.parse().ok()).ok_or(PlexError::NotFound)?;
    let image = segments.next().unwrap_or("thumb").to_owned();
    let version = segments.next().map(str::to_owned);
    let width = params.parse::<u32>("width").filter(|w| *w > 0);
    serve_image(state, user, id, &image, version, width, headers).await
}

#[cfg(test)]
mod tests {
    #[test]
    fn encodes_query_values() {
        assert_eq!(super::encode("Sci-Fi & Fantasy"), "Sci-Fi%20%26%20Fantasy");
    }
}
