-- A media file renamed or moved keeps its row; `moved_from` is the item it
-- was a file of, until the item its new name makes has what people did
-- with that one (see the scanner's `relink`). A scan stopped in between
-- leaves it for the next.
ALTER TABLE media_files ADD COLUMN moved_from INTEGER;
CREATE INDEX media_files_moved ON media_files(library_id) WHERE moved_from IS NOT NULL;

-- What people watched of movies and episodes whose files all went: kept by
-- the title's ids for a while, and given to the movie or episode with the
-- same ids that comes (a copy replaced by one named differently, a film
-- deleted and added again). See `library::gone`. `item_id` is the id the
-- title had, which no other gets (0026).
CREATE TABLE gone_items (
    item_id      INTEGER PRIMARY KEY,
    library_id   INTEGER NOT NULL REFERENCES libraries(id) ON DELETE CASCADE,
    kind         TEXT    NOT NULL CHECK (kind IN ('movie', 'episode')),
    -- A movie's ids.
    tmdb_id      INTEGER,
    imdb_id      TEXT,
    -- An episode's: its show's, its season's number and its own.
    show_tmdb_id INTEGER,
    show_tvdb_id INTEGER,
    season       INTEGER,
    episode      INTEGER,
    gone_at      INTEGER NOT NULL
);
CREATE INDEX gone_items_library ON gone_items(library_id, kind);

-- Each person's `user_item_state` of the title, and its plays.
CREATE TABLE gone_item_state (
    item_id        INTEGER NOT NULL REFERENCES gone_items(item_id) ON DELETE CASCADE,
    user_id        INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    position_ms    INTEGER NOT NULL,
    watched        INTEGER NOT NULL,
    play_count     INTEGER NOT NULL,
    last_played_at INTEGER,
    rating         REAL,
    rated_at       INTEGER,
    updated_at     INTEGER NOT NULL,
    PRIMARY KEY (item_id, user_id)
) WITHOUT ROWID;
CREATE INDEX gone_item_state_user ON gone_item_state(user_id);

CREATE TABLE gone_play_history (
    item_id   INTEGER NOT NULL REFERENCES gone_items(item_id) ON DELETE CASCADE,
    user_id   INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    viewed_at INTEGER NOT NULL,
    device    TEXT
);
CREATE INDEX gone_play_history_item ON gone_play_history(item_id);
CREATE INDEX gone_play_history_user ON gone_play_history(user_id);

-- What people did with a title is looked up by the title when it goes or
-- another takes it: these were found only through each person's rows.
CREATE INDEX watchlist_item ON watchlist(item_id);
CREATE INDEX continue_watching_hidden_item ON continue_watching_hidden(item_id);
CREATE INDEX stream_preferences_item ON stream_preferences(item_id);
