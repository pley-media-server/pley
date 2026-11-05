-- The NFO file of a movie, show, season or episode (as Kodi, Jellyfin,
-- Radarr, Sonarr and tinyMediaManager write them), as the last scan read
-- it. Its size and modification time tell a changed file apart without
-- reading it again; its ids decide the item's match, and its fields are
-- written onto the item after every refresh, below hand edits (see
-- overrides.rs). `ids` and `fields` are JSON; `locked` is Jellyfin's
-- lockdata, which keeps the item from being refreshed online.
CREATE TABLE item_nfo (
    item_id INTEGER PRIMARY KEY REFERENCES items(id) ON DELETE CASCADE,
    path    TEXT    NOT NULL,
    size    INTEGER NOT NULL,
    mtime   INTEGER NOT NULL,
    ids     TEXT    NOT NULL,
    fields  TEXT    NOT NULL,
    locked  INTEGER NOT NULL DEFAULT 0
);
